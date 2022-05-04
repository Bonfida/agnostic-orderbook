use crate::{
    critbit::{LeafNode, Node, NodeHandle, Slab},
    error::AoError,
    processor::new_order,
    state::{Event, EventQueue, SelfTradeBehavior, Side},
};
use bonfida_utils::fp_math::{fp32_div, fp32_mul};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{account_info::AccountInfo, msg, program_error::ProgramError};

#[derive(BorshSerialize, BorshDeserialize, Debug)]
/// This struct is written back into the event queue's register after new_order or cancel_order.
///
/// In the case of a new order, the quantities describe the total order amounts which
/// were either matched against other orders or written into the orderbook.
///
/// In the case of an order cancellation, the quantities describe what was left of the order in the orderbook.
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

/// The serialized size of an OrderSummary object.
pub const ORDER_SUMMARY_SIZE: u32 = 41;

#[doc(hidden)]
pub struct OrderBookState<'a> {
    pub bids: Slab<'a>,
    pub asks: Slab<'a>,
    pub callback_id_len: usize,
}

impl<'ob> OrderBookState<'ob> {
    pub(crate) fn new_safe(
        bids_account: &AccountInfo<'ob>,
        asks_account: &AccountInfo<'ob>,
        callback_info_len: usize,
        callback_id_len: usize,
    ) -> Result<Self, ProgramError> {
        let bids = Slab::new_from_acc_info(bids_account, callback_info_len);
        let asks = Slab::new_from_acc_info(asks_account, callback_info_len);
        if !(bids.check(Side::Bid) && asks.check(Side::Ask)) {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(Self {
            bids,
            asks,
            callback_id_len,
        })
    }
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
            .map(|h| self.bids.get_node(h).unwrap().as_leaf().unwrap().price());
        let best_ask_price = self
            .asks
            .find_min()
            .map(|h| self.asks.get_node(h).unwrap().as_leaf().unwrap().price());
        (best_bid_price, best_ask_price)
    }

    pub fn get_tree(&mut self, side: Side) -> &mut Slab<'ob> {
        match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        }
    }

    #[doc(hidden)]
    pub fn commit_changes(&self) {
        self.bids.write_header();
        self.asks.write_header();
    }

    pub fn new_order(
        &mut self,
        params: new_order::Params,
        event_queue: &mut EventQueue,
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
        let callback_id_len = self.callback_id_len;
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

            let mut best_bo_ref = self
                .get_tree(side.opposite())
                .get_node(best_bo_h)
                .unwrap()
                .as_leaf()
                .unwrap()
                .to_owned();

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
                let order_would_self_trade = &callback_info[..callback_id_len]
                    == (&self
                        .get_tree(side.opposite())
                        .get_callback_info(best_bo_ref.callback_info_pt as usize)[..callback_id_len]
                        as &[u8]);
                if order_would_self_trade {
                    let best_offer_id = best_bo_ref.order_id();
                    let cancelled_provide_base_qty;

                    match self_trade_behavior {
                        SelfTradeBehavior::CancelProvide => {
                            cancelled_provide_base_qty =
                                std::cmp::min(base_qty_remaining, best_bo_ref.base_quantity);
                        }
                        SelfTradeBehavior::AbortTransaction => return Err(AoError::WouldSelfTrade),
                        SelfTradeBehavior::DecrementTake => unreachable!(),
                    };

                    let remaining_provide_base_qty =
                        best_bo_ref.base_quantity - cancelled_provide_base_qty;
                    let delete = remaining_provide_base_qty == 0;
                    let provide_out = Event::Out {
                        side: side.opposite(),
                        delete,
                        order_id: best_offer_id,
                        base_size: cancelled_provide_base_qty,
                        callback_info: self
                            .get_tree(side.opposite())
                            .get_callback_info(best_bo_ref.callback_info_pt as usize)
                            .to_owned(),
                    };
                    event_queue
                        .push_back(provide_out)
                        .map_err(|_| AoError::EventQueueFull)?;
                    if delete {
                        self.get_tree(side.opposite())
                            .remove_by_key(best_offer_id)
                            .unwrap();
                    } else {
                        best_bo_ref.set_base_quantity(remaining_provide_base_qty);
                        self.get_tree(side.opposite())
                            .write_node(&Node::Leaf(best_bo_ref), best_bo_h);
                    }

                    continue;
                }
            }

            let maker_fill = Event::Fill {
                taker_side: side,
                maker_callback_info: self
                    .get_tree(side.opposite())
                    .get_callback_info(best_bo_ref.callback_info_pt as usize)
                    .to_owned(),
                taker_callback_info: callback_info.clone(),
                maker_order_id: best_bo_ref.order_id(),
                quote_size: quote_maker_qty,
                base_size: base_trade_qty,
            };
            event_queue
                .push_back(maker_fill)
                .map_err(|_| AoError::EventQueueFull)?;

            best_bo_ref.set_base_quantity(best_bo_ref.base_quantity - base_trade_qty);
            base_qty_remaining -= base_trade_qty;
            quote_qty_remaining -= quote_maker_qty;

            if best_bo_ref.base_quantity < min_base_order_size {
                let best_offer_id = best_bo_ref.order_id();
                let cur_side = side.opposite();
                let out_event = Event::Out {
                    side: cur_side,
                    order_id: best_offer_id,
                    base_size: best_bo_ref.base_quantity,
                    callback_info: self
                        .get_tree(side.opposite())
                        .get_callback_info(best_bo_ref.callback_info_pt as usize)
                        .to_owned(),
                    delete: true,
                };

                self.get_tree(cur_side)
                    .remove_by_key(best_offer_id)
                    .unwrap();
                event_queue
                    .push_back(out_event)
                    .map_err(|_| AoError::EventQueueFull)?;
            } else {
                self.get_tree(side.opposite())
                    .write_node(&Node::Leaf(best_bo_ref), best_bo_h);
            }

            match_limit -= 1;
        }

        let base_qty_to_post = std::cmp::min(
            fp32_div(quote_qty_remaining, limit_price).ok_or(AoError::NumericalOverflow)?,
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
        let callback_info_offset = self
            .get_tree(side)
            .write_callback_info(&callback_info)
            .unwrap();
        let new_leaf = Node::Leaf(LeafNode {
            key: new_leaf_order_id,
            callback_info_pt: callback_info_offset,
            base_quantity: base_qty_to_post,
        });
        let insert_result = self.get_tree(side).insert_leaf(&new_leaf);
        if let Err(AoError::SlabOutOfSpace) = insert_result {
            // Boot out the least aggressive orders
            msg!("Orderbook is full! booting lest aggressive orders...");
            let (order, callback_info) = match side {
                Side::Bid => self.get_tree(Side::Bid).remove_min().unwrap(),
                Side::Ask => self.get_tree(Side::Ask).remove_max().unwrap(),
            };
            let l = order.as_leaf().unwrap();
            let out = Event::Out {
                side,
                delete: true,
                order_id: l.order_id(),
                base_size: l.base_quantity,
                callback_info,
            };
            event_queue
                .push_back(out)
                .map_err(|_| AoError::EventQueueFull)?;
            self.get_tree(side).insert_leaf(&new_leaf).unwrap();
        } else {
            insert_result.unwrap();
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

    pub fn is_empty(&self) -> bool {
        self.asks.root().is_none() && self.bids.root().is_none()
    }
}
