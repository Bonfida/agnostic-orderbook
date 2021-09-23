use crate::{
    critbit::{LeafNode, Node, NodeHandle, Slab},
    error::AoError,
    processor::new_order,
    state::{Event, EventQueue, SelfTradeBehavior, Side},
    utils::{fp32_div, fp32_mul},
};
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

pub(crate) struct OrderBookState<'a> {
    bids: Slab<'a>,
    asks: Slab<'a>,
    callback_id_len: usize,
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

    pub fn get_tree(&mut self, side: Side) -> &mut Slab<'ob> {
        match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        }
    }

    pub(crate) fn commit_changes(&self) {
        self.bids.write_header();
        self.asks.write_header();
    }

    pub(crate) fn new_order(
        &mut self,
        params: new_order::Params,
        event_queue: &mut EventQueue,
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

            let mut best_bo_ref = self
                .get_tree(side.opposite())
                .get_node(best_bo_h)
                .and_then(|a| match a {
                    Node::Leaf(l) => Some(l),
                    _ => None,
                })
                .unwrap();

            let trade_price = best_bo_ref.price();
            crossed = match side {
                Side::Bid => limit_price >= trade_price,
                Side::Ask => limit_price <= trade_price,
            };

            if post_only || !crossed {
                break;
            }

            let offer_size = best_bo_ref.base_quantity;
            let base_trade_qty = offer_size
                .min(base_qty_remaining)
                .min(fp32_div(quote_qty_remaining, best_bo_ref.price()));

            if base_trade_qty == 0 {
                break;
            }

            // The decrement take case can be handled by the caller program on event consumption, so no special logic
            // is needed for it.
            if self_trade_behavior != SelfTradeBehavior::DecrementTake {
                let order_would_self_trade = callback_info[..self.callback_id_len]
                    == best_bo_ref.callback_info[..self.callback_id_len];
                if order_would_self_trade {
                    let best_offer_id = best_bo_ref.order_id();
                    let cancelled_provide_base_qty;

                    match self_trade_behavior {
                        SelfTradeBehavior::CancelProvide => {
                            cancelled_provide_base_qty = best_bo_ref.base_quantity;
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
                        callback_info: best_bo_ref.callback_info.clone(),
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
                            .write_node(&Node::Leaf(best_bo_ref), best_bo_h)
                            .unwrap();
                    }

                    continue;
                }
            }

            let quote_maker_qty = fp32_mul(base_trade_qty, trade_price);

            let maker_fill = Event::Fill {
                taker_side: side,
                maker_callback_info: best_bo_ref.callback_info.clone(),
                taker_callback_info: callback_info.clone(),
                maker_order_id: best_bo_ref.order_id(),
                quote_size: quote_maker_qty,
                base_size: base_trade_qty,
            };
            event_queue
                .push_back(maker_fill)
                .map_err(|_| AoError::EventQueueFull)?;

            let initial_best_bo_base_quantity = best_bo_ref.base_quantity;

            best_bo_ref.set_base_quantity(best_bo_ref.base_quantity - base_trade_qty);
            base_qty_remaining -= base_trade_qty;
            quote_qty_remaining -= quote_maker_qty;

            if best_bo_ref.base_quantity == 0 {
                let best_offer_id = best_bo_ref.order_id();
                let cur_side = side.opposite();
                self.get_tree(cur_side)
                    .remove_by_key(best_offer_id)
                    .unwrap();
                let out_event = Event::Out {
                    side: cur_side,
                    order_id: best_offer_id,
                    base_size: initial_best_bo_base_quantity,
                    callback_info: best_bo_ref.callback_info,
                    delete: true,
                };
                event_queue
                    .push_back(out_event)
                    .map_err(|_| AoError::EventQueueFull)?;
            } else {
                self.get_tree(side.opposite())
                    .write_node(&Node::Leaf(best_bo_ref), best_bo_h)
                    .unwrap();
            }

            match_limit -= 1;
        }

        if crossed || !post_allowed {
            return Ok(OrderSummary {
                posted_order_id: None,
                total_base_qty: max_base_qty - base_qty_remaining,
                total_quote_qty: max_quote_qty - quote_qty_remaining,
                total_base_qty_posted: 0,
            });
        }
        let base_qty_to_post = std::cmp::min(
            fp32_div(quote_qty_remaining, limit_price),
            base_qty_remaining,
        );
        let new_leaf_order_id = event_queue.gen_order_id(limit_price, side);
        let new_leaf = Node::Leaf(LeafNode::new(
            new_leaf_order_id,
            callback_info,
            base_qty_to_post,
        ));
        let insert_result = self.get_tree(side).insert_leaf(&new_leaf);
        if let Err(AoError::SlabOutOfSpace) = insert_result {
            // Boot out the least aggressive orders
            msg!("Orderbook is full! booting lest aggressive orders...");
            let order = match side {
                Side::Bid => self.get_tree(Side::Bid).remove_min().unwrap(),
                Side::Ask => self.get_tree(Side::Ask).remove_max().unwrap(),
            };
            let l = order.as_leaf().unwrap();
            let out = Event::Out {
                side: Side::Bid,
                delete: true,
                order_id: l.order_id(),
                base_size: l.base_quantity,
                callback_info: l.callback_info.clone(),
            };
            event_queue
                .push_back(out)
                .map_err(|_| AoError::EventQueueFull)?;
            self.get_tree(side).insert_leaf(&new_leaf).unwrap();
        } else {
            insert_result.unwrap();
        }
        base_qty_remaining -= base_qty_to_post;
        quote_qty_remaining -= fp32_mul(base_qty_to_post, limit_price);
        Ok(OrderSummary {
            posted_order_id: Some(new_leaf_order_id),
            total_base_qty: max_base_qty - base_qty_remaining,
            total_quote_qty: max_quote_qty - quote_qty_remaining,
            total_base_qty_posted: base_qty_to_post,
        })
    }
}
