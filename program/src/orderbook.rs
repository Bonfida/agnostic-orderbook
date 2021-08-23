use crate::{
    critbit::{LeafNode, NodeHandle, Slab},
    error::AOError,
    processor::new_order,
    state::{Event, EventQueue, MarketState, SelfTradeBehavior, Side},
    utils::{fp32_div, fp32_mul},
};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::msg;

#[derive(BorshSerialize, BorshDeserialize)]
pub struct OrderSummary {
    total_asset_qty: u64,
    total_quote_qty: u64,
}

pub struct OrderBookState<'a> {
    // first byte of a key is 0xaa or 0xbb, disambiguating bids and asks
    pub bids: Slab<'a>,
    pub asks: Slab<'a>,
    pub market_state: MarketState,
}

impl<'ob> OrderBookState<'ob> {
    fn find_bbo(&self, side: Side) -> Option<NodeHandle> {
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

    pub(crate) fn new_order(
        &mut self,
        params: new_order::Params,
        event_queue: &mut EventQueue,
    ) -> Result<OrderSummary, AOError> {
        let new_order::Params {
            max_asset_qty,
            max_quote_qty,
            order_id,
            side,
            limit_price,
            callback_info,
            post_only,
            post_allowed,
            self_trade_behavior,
            mut match_limit,
        } = params;

        let mut asset_qty_remaining = max_asset_qty;
        let mut quote_qty_remaining = max_quote_qty;

        // New bid
        let mut crossed = true;
        #[allow(clippy::never_loop)]
        loop {
            if match_limit == 0 {
                break;
            }
            let best_bo_h = match self.find_bbo(side) {
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
                .as_leaf(self.market_state.callback_info_len as usize)
                .unwrap();

            let trade_price = best_bo_ref.price();
            crossed = match side {
                Side::Bid => limit_price >= trade_price,
                Side::Ask => limit_price <= trade_price,
            };

            if post_only {
                break;
            }

            let offer_size = best_bo_ref.asset_quantity;
            let asset_trade_qty = offer_size
                .min(asset_qty_remaining)
                .min(fp32_div(quote_qty_remaining, best_bo_ref.price()));

            if asset_trade_qty == 0 {
                break;
            }

            if self_trade_behavior != SelfTradeBehavior::DecrementTake {
                let order_would_self_trade = callback_info == best_bo_ref.callback_info;
                if order_would_self_trade {
                    let best_offer_id = best_bo_ref.order_id();
                    let cancelled_provide_asset_qty;

                    match self_trade_behavior {
                        SelfTradeBehavior::CancelProvide => {
                            cancelled_provide_asset_qty = best_bo_ref.asset_quantity;
                        }
                        SelfTradeBehavior::AbortTransaction => return Err(AOError::WouldSelfTrade),
                        SelfTradeBehavior::DecrementTake => unreachable!(),
                    };

                    let remaining_provide_asset_qty =
                        best_bo_ref.asset_quantity - cancelled_provide_asset_qty;
                    let provide_out = Event::Out {
                        side: side.opposite(),
                        order_id: best_offer_id,
                        asset_size: cancelled_provide_asset_qty,
                        callback_info: best_bo_ref.callback_info.clone(),
                    };
                    event_queue
                        .push_back(provide_out)
                        .map_err(|_| AOError::EventQueueFull)?;
                    if remaining_provide_asset_qty == 0 {
                        self.get_tree(side.opposite())
                            .remove_by_key(best_offer_id)
                            .unwrap();
                    } else {
                        best_bo_ref.set_asset_quantity(remaining_provide_asset_qty);
                    }

                    return Ok(OrderSummary {
                        total_asset_qty: max_asset_qty - asset_qty_remaining,
                        total_quote_qty: max_quote_qty - quote_qty_remaining,
                    });
                }
            }

            let quote_maker_qty = fp32_mul(asset_trade_qty, trade_price);

            let maker_fill = Event::Fill {
                taker_side: side,
                maker_callback_info: best_bo_ref.callback_info.clone(),
                taker_callback_info: callback_info.clone(),
                maker_order_id: best_bo_ref.order_id(),
                quote_size: quote_maker_qty,
                asset_size: asset_trade_qty,
            };
            event_queue
                .push_back(maker_fill)
                .map_err(|_| AOError::EventQueueFull)?;

            best_bo_ref.set_asset_quantity(best_bo_ref.asset_quantity - asset_trade_qty);
            asset_qty_remaining -= asset_trade_qty;
            quote_qty_remaining -= quote_maker_qty;

            if best_bo_ref.asset_quantity == 0 {
                let best_offer_id = best_bo_ref.order_id();
                self.get_tree(side.opposite())
                    .remove_by_key(best_offer_id)
                    .unwrap();
            }

            match_limit -= 1;
        }

        if crossed || !post_allowed {
            return Ok(OrderSummary {
                total_asset_qty: max_asset_qty - asset_qty_remaining,
                total_quote_qty: max_quote_qty - quote_qty_remaining,
            });
        }
        let asset_qty_to_post = match side {
            Side::Bid => std::cmp::min(
                fp32_div(quote_qty_remaining, limit_price),
                asset_qty_remaining,
            ),
            Side::Ask => asset_qty_remaining, // TODO: check accuracy
        };
        let new_leaf = LeafNode::new(order_id, callback_info.clone(), asset_qty_to_post);
        let insert_result = self.get_tree(side).insert_leaf(&new_leaf);
        if let Err(AOError::SlabOutOfSpace) = insert_result {
            // boot out the least aggressive bid
            msg!("bids full! booting...");
            let order = self.get_tree(side).remove_min().unwrap();
            let out = Event::Out {
                side: Side::Bid,
                order_id: order.order_id(),
                asset_size: order.asset_quantity,
                callback_info: order.callback_info.clone(),
            };
            event_queue
                .push_back(out)
                .map_err(|_| AOError::EventQueueFull)?;
            self.get_tree(side).insert_leaf(&new_leaf).unwrap();
        } else {
            insert_result.unwrap();
        }
        Ok(OrderSummary {
            total_asset_qty: max_asset_qty - asset_qty_remaining,
            total_quote_qty: max_quote_qty - quote_qty_remaining,
        })
    }
}
