use crate::{
    error::AoError,
    processor::new_order,
    state::{
        critbit::{LeafNode, NodeHandle, Slab},
        event_queue::{EventQueue, EventTag, FillEvent, OutEvent},
        AccountTag, SelfTradeBehavior, Side,
    },
};
use bonfida_utils::fp_math::{fp32_div, fp32_mul};
use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::Pod;
use solana_program::{msg, program_error::ProgramError};

/// This struct is written back into the event queue's register after new_order or cancel_order.
///
/// In the case of a new order, the quantities describe the total order amounts which
/// were either matched against other orders or written into the orderbook.
///
/// In the case of an order cancellation, the quantities describe what was left of the order in the orderbook.
#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub struct OrderSummary {
    /// When applicable, the order id of the newly created order.
    pub posted_order_id: Option<u128>,
    #[allow(missing_docs)]
    pub total_base_qty: u64,
    #[allow(missing_docs)]
    pub total_quote_qty: u64,
    #[allow(missing_docs)]
    pub total_base_qty_posted: u64,
}

pub trait CallbackInfo: Pod + Copy {
    type CallbackId;
    fn as_callback_id(&self) -> &Self::CallbackId;
}

impl CallbackInfo for [u8; 32] {
    type CallbackId = Self;

    fn as_callback_id(&self) -> &Self::CallbackId {
        self
    }
}

/// The serialized size of an OrderSummary object.
pub const ORDER_SUMMARY_SIZE: u32 = 41;

#[doc(hidden)]
pub struct OrderBookState<'a, C> {
    pub bids: Slab<'a, C>,
    pub asks: Slab<'a, C>,
}

// pub type OrderBookStateRef<'slab, C> = OrderBookState<Slab<'slab, C>>;

impl<'slab, C: Pod + Copy> OrderBookState<'slab, C> {
    pub(crate) fn new_safe(
        bids_account: &'slab mut [u8],
        asks_account: &'slab mut [u8],
    ) -> Result<Self, ProgramError> {
        let bids = Slab::from_buffer(bids_account, AccountTag::Bids)?;
        let asks = Slab::from_buffer(asks_account, AccountTag::Asks)?;
        Ok(Self { bids, asks })
    }
}

impl<'a, C> OrderBookState<'a, C> {
    pub fn find_bbo(&self, side: Side) -> Option<NodeHandle> {
        match side {
            Side::Bid => self.bids.find_max(),
            Side::Ask => self.asks.find_min(),
        }
    }

    #[cfg(feature = "no-entrypoint")]
    pub fn get_spread(&self) -> (Option<u64>, Option<u64>) {
        let best_bid_price = self
            .bids
            .find_max()
            .map(|h| self.bids.leaf_nodes[h as usize].price());
        let best_ask_price = self
            .asks
            .find_max()
            .map(|h| self.asks.leaf_nodes[h as usize].price());
        (best_bid_price, best_ask_price)
    }

    pub fn get_tree(&mut self, side: Side) -> &mut Slab<'a, C> {
        match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.asks.header.leaf_count == 0 && self.bids.header.leaf_count == 0
    }
}

impl<'a, C: CallbackInfo> OrderBookState<'a, C>
where
    <C as CallbackInfo>::CallbackId: PartialEq,
{
    pub fn new_order(
        &mut self,
        params: new_order::Params<C>,
        event_queue: &mut EventQueue<'a, C>,
        min_base_order_size: u64,
    ) -> Result<OrderSummary, AoError> {
        let new_order::Params {
            max_base_qty,
            max_quote_qty,
            side,
            limit_price,
            callback_info,
            post_only,
            post_allowed,
            self_trade_behavior,
            mut match_limit,
        } = params;

        let mut base_qty_remaining = max_base_qty;
        let mut quote_qty_remaining = max_quote_qty;

        // New bid
        let mut crossed = true;
        loop {
            if match_limit == 0 {
                break;
            }
            let best_bo_h = match self.find_bbo(side.opposite()) {
                None => {
                    crossed = false;
                    break;
                }
                Some(h) => h,
            };

            let mut best_bo_ref = self.get_tree(side.opposite()).leaf_nodes[best_bo_h as usize];

            let trade_price = best_bo_ref.price();
            crossed = match side {
                Side::Bid => limit_price >= trade_price,
                Side::Ask => limit_price <= trade_price,
            };

            if post_only || !crossed {
                break;
            }

            let offer_size = best_bo_ref.base_quantity;
            let base_trade_qty = offer_size.min(base_qty_remaining).min(
                fp32_div(quote_qty_remaining, best_bo_ref.price())
                    .ok_or(AoError::NumericalOverflow)?,
            );

            if base_trade_qty == 0 {
                break;
            }

            let quote_maker_qty =
                fp32_mul(base_trade_qty, trade_price).ok_or(AoError::NumericalOverflow)?;

            if quote_maker_qty == 0 {
                break;
            }

            // The decrement take case can be handled by the caller program on event consumption, so no special logic
            // is needed for it.
            if self_trade_behavior != SelfTradeBehavior::DecrementTake {
                let order_would_self_trade = callback_info.as_callback_id()
                    == self
                        .get_tree(side.opposite())
                        .get_callback_info(best_bo_h)
                        .as_callback_id();
                if order_would_self_trade {
                    let best_offer_id = best_bo_ref.order_id();

                    let cancelled_provide_base_qty = match self_trade_behavior {
                        SelfTradeBehavior::CancelProvide => {
                            std::cmp::min(base_qty_remaining, best_bo_ref.base_quantity)
                        }
                        SelfTradeBehavior::AbortTransaction => return Err(AoError::WouldSelfTrade),
                        SelfTradeBehavior::DecrementTake => unreachable!(),
                    };

                    let remaining_provide_base_qty =
                        best_bo_ref.base_quantity - cancelled_provide_base_qty;
                    let delete = remaining_provide_base_qty == 0;
                    let provide_out_callback_info =
                        self.get_tree(side.opposite()).get_callback_info(best_bo_h);
                    let provide_out = OutEvent {
                        side: side.opposite(),
                        delete,
                        order_id: best_offer_id,
                        base_size: cancelled_provide_base_qty,
                        tag: EventTag::Out,
                        _padding: [0; 13],
                    };
                    event_queue
                        .push_back(provide_out, Some(provide_out_callback_info), None)
                        .map_err(|_| AoError::EventQueueFull)?;
                    if delete {
                        self.get_tree(side.opposite())
                            .remove_by_key(best_offer_id)
                            .unwrap();
                    } else {
                        best_bo_ref.base_quantity = remaining_provide_base_qty;
                    }

                    continue;
                }
            }

            let maker_callback_info = self.get_tree(side.opposite()).get_callback_info(best_bo_h);

            let maker_fill = FillEvent {
                taker_side: side as u8,
                maker_order_id: best_bo_ref.order_id(),
                quote_size: quote_maker_qty,
                base_size: base_trade_qty,
                tag: EventTag::Fill as u8,
                _padding: [0; 6],
            };
            event_queue
                .push_back(maker_fill, Some(maker_callback_info), Some(&callback_info))
                .map_err(|_| AoError::EventQueueFull)?;

            best_bo_ref.base_quantity -= base_trade_qty;
            base_qty_remaining -= base_trade_qty;
            quote_qty_remaining -= quote_maker_qty;

            if best_bo_ref.base_quantity < min_base_order_size {
                let best_offer_id = best_bo_ref.order_id();
                let cur_side = side.opposite();
                let out_event = OutEvent {
                    side: cur_side,
                    order_id: best_offer_id,
                    base_size: best_bo_ref.base_quantity,
                    delete: true,
                    tag: EventTag::Out,
                    _padding: [0; 13],
                };

                let (_, out_event_callback_info) = self
                    .get_tree(cur_side)
                    .remove_by_key(best_offer_id)
                    .unwrap();
                event_queue
                    .push_back(out_event, Some(out_event_callback_info), None)
                    .map_err(|_| AoError::EventQueueFull)?;
            }

            match_limit -= 1;
        }

        let base_qty_to_post = std::cmp::min(
            fp32_div(quote_qty_remaining, limit_price).unwrap_or(u64::MAX),
            base_qty_remaining,
        );

        if crossed || !post_allowed || base_qty_to_post < min_base_order_size {
            return Ok(OrderSummary {
                posted_order_id: None,
                total_base_qty: max_base_qty - base_qty_remaining,
                total_quote_qty: max_quote_qty - quote_qty_remaining,
                total_base_qty_posted: 0,
            });
        }

        let new_leaf_order_id = event_queue.gen_order_id(limit_price, side);
        let new_leaf = LeafNode {
            key: new_leaf_order_id,
            base_quantity: base_qty_to_post,
        };
        let insert_result = self.get_tree(side).insert_leaf(&new_leaf);
        if let Err(AoError::SlabOutOfSpace) = insert_result {
            // Boot out the least aggressive orders
            msg!("Orderbook is full! booting lest aggressive orders...");
            let (order, callback_info) = match side {
                Side::Bid => self.get_tree(Side::Bid).remove_min().unwrap(),
                Side::Ask => self.get_tree(Side::Ask).remove_max().unwrap(),
            };
            let out = OutEvent {
                side,
                delete: true,
                order_id: order.order_id(),
                base_size: order.base_quantity,
                tag: EventTag::Out,
                _padding: [0; 13],
            };
            event_queue
                .push_back(out, Some(callback_info), None)
                .map_err(|_| AoError::EventQueueFull)?;
            self.get_tree(side).insert_leaf(&new_leaf).unwrap();
        } else {
            let k = insert_result.unwrap().0;
            *self.get_tree(side).get_callback_info_mut(k) = callback_info;
        }
        base_qty_remaining -= base_qty_to_post;
        quote_qty_remaining -=
            fp32_mul(base_qty_to_post, limit_price).ok_or(AoError::NumericalOverflow)?;
        Ok(OrderSummary {
            posted_order_id: Some(new_leaf_order_id),
            total_base_qty: max_base_qty - base_qty_remaining,
            total_quote_qty: max_quote_qty - quote_qty_remaining,
            total_base_qty_posted: base_qty_to_post,
        })
    }
}
