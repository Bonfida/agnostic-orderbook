use solana_program::msg;

use crate::{
    critbit::{LeafNode, NodeHandle, Slab},
    error::{AOError, AOResult},
    processor::new_order,
    state::{Event, EventQueue, EventView, MarketState, SelfTradeBehavior, Side},
};

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

    pub(crate) fn new_bid(
        &mut self,
        params: new_order::Params,
        event_queue: &mut EventQueue,
    ) -> AOResult {
        let new_order::Params {
            max_base_qty,
            max_quote_qty,
            order_id,
            side: _,
            limit_price,
            owner,
            post_only,
            post_allowed,
            self_trade_behavior,
        } = params;

        let mut base_qty_remaining = max_base_qty;
        let mut quote_qty_remaining = max_quote_qty;

        // New bid
        let crossed;
        #[allow(clippy::never_loop)]
        let done = loop {
            let best_offer_h = match self.find_bbo(Side::Ask) {
                None => {
                    crossed = false;
                    break true;
                }
                Some(h) => h,
            };

            let mut best_offer_ref = self.asks.get_node(best_offer_h).unwrap().as_leaf().unwrap();

            let trade_price = best_offer_ref.price();
            crossed = limit_price >= trade_price;
            if !crossed || post_only {
                break true;
            }

            let offer_size = best_offer_ref.quantity();
            let trade_qty = offer_size
                .min(base_qty_remaining)
                .min(quote_qty_remaining / best_offer_ref.price());

            if trade_qty == 0 {
                break true;
            }

            let order_would_self_trade = owner == best_offer_ref.owner();
            if order_would_self_trade {
                let best_offer_id = best_offer_ref.order_id();

                let cancelled_take_qty;
                let cancelled_provide_qty;

                match self_trade_behavior {
                    SelfTradeBehavior::CancelProvide => {
                        cancelled_take_qty = 0;
                        cancelled_provide_qty = best_offer_ref.quantity();
                    }
                    SelfTradeBehavior::DecrementTake => {
                        cancelled_take_qty = trade_qty;
                        cancelled_provide_qty = trade_qty;
                    }
                    SelfTradeBehavior::AbortTransaction => return Err(AOError::WouldSelfTrade),
                };

                let remaining_provide_qty = best_offer_ref.quantity() - cancelled_provide_qty;
                let provide_out = Event::new(EventView::Out {
                    side: Side::Ask,
                    order_id: best_offer_id,
                    release_funds: true,
                    native_qty_unlocked: cancelled_provide_qty,
                    native_qty_still_locked: remaining_provide_qty,
                    owner: best_offer_ref.owner(),
                });
                event_queue
                    .push_back(provide_out)
                    .map_err(|_| AOError::EventQueueFull)?;
                if remaining_provide_qty == 0 {
                    self.asks.remove_by_key(best_offer_id).unwrap();
                } else {
                    best_offer_ref.set_quantity(remaining_provide_qty);
                }

                let quote_taker_unlocked = cancelled_take_qty * trade_price;
                let quote_taker_still_locked = max_quote_qty - quote_taker_unlocked;

                msg!("Unmatched remains from the order: base_qty_remaining {:?}, quote_qty_remaining {:?}", base_qty_remaining - cancelled_take_qty, quote_taker_still_locked);

                let native_quote_qty_unlocked;
                let native_qty_still_locked;
                native_quote_qty_unlocked = quote_taker_unlocked;
                native_qty_still_locked = quote_taker_still_locked;

                // to_release.unlock_native_pc(native_qty_unlocked);
                let take_out = Event::new(EventView::Out {
                    side: Side::Bid,
                    order_id,
                    release_funds: false,
                    native_qty_unlocked: native_quote_qty_unlocked,
                    native_qty_still_locked,
                    owner,
                });
                event_queue
                    .push_back(take_out)
                    .map_err(|_| AOError::EventQueueFull)?;

                return Ok(());
            }
            let quote_maker_qty = trade_qty * trade_price;

            let maker_fill = Event::new(EventView::Fill {
                side: Side::Ask,
                maker: true,
                order_id: best_offer_ref.order_id(),
                native_qty_paid: trade_qty,
                native_qty_received: quote_maker_qty,
                owner: best_offer_ref.owner(),
            });
            event_queue
                .push_back(maker_fill)
                .map_err(|_| AOError::EventQueueFull)?;

            best_offer_ref.set_quantity(best_offer_ref.quantity() - trade_qty);
            base_qty_remaining -= trade_qty;
            quote_qty_remaining -= trade_qty * trade_price;

            if best_offer_ref.quantity() == 0 {
                let best_offer_id = best_offer_ref.order_id();
                event_queue
                    .push_back(Event::new(EventView::Out {
                        side: Side::Ask,
                        release_funds: true,
                        order_id: best_offer_id,
                        native_qty_unlocked: 0,
                        native_qty_still_locked: 0,
                        owner: best_offer_ref.owner(),
                    }))
                    .map_err(|_| AOError::EventQueueFull)?;
                self.asks.remove_by_key(best_offer_id).unwrap();
            }

            break false;
        };

        let native_accum_fill_price = max_quote_qty - base_qty_remaining;

        let base_received = max_base_qty - base_qty_remaining;
        let native_quote_paid = native_accum_fill_price;

        // to_release.credit_coin(coin_lots_received);
        // to_release.debit_native_pc(native_pc_paid);
        if native_accum_fill_price > 0 {
            let taker_fill = Event::new(EventView::Fill {
                side: Side::Bid,
                maker: false,
                native_qty_paid: native_quote_paid,
                native_qty_received: base_received,
                order_id,
                owner,
            });
            event_queue
                .push_back(taker_fill)
                .map_err(|_| AOError::EventQueueFull)?;
        }

        if !done && base_qty_remaining > 0 && quote_qty_remaining > 0 {
            msg!("Unmatched remains from the order: base_qty_remaining {:?}, quote_qty_remaining {:?}", base_qty_remaining, quote_qty_remaining);
            return Ok(());
        }

        let (coin_qty_to_post, pc_qty_to_keep_locked) = if post_allowed && !crossed {
            let coin_qty_to_post = base_qty_remaining.min(base_qty_remaining / limit_price);
            (coin_qty_to_post, coin_qty_to_post * limit_price)
        } else {
            (0, 0)
        };

        let out = {
            let native_qty_still_locked = pc_qty_to_keep_locked;
            let native_qty_unlocked = base_qty_remaining - native_qty_still_locked;
            // to_release.unlock_native_pc(native_qty_unlocked);

            Event::new(EventView::Out {
                side: Side::Bid,
                release_funds: false,
                order_id,
                native_qty_unlocked,
                native_qty_still_locked,
                owner,
            })
        };
        event_queue
            .push_back(out)
            .map_err(|_| AOError::EventQueueFull)?;

        if pc_qty_to_keep_locked > 0 {
            let new_leaf = LeafNode::new(order_id, owner, coin_qty_to_post);
            let insert_result = self.bids.insert_leaf(&new_leaf);
            if let Err(AOError::SlabOutOfSpace) = insert_result {
                // boot out the least aggressive bid
                msg!("bids full! booting...");
                let order = self.bids.remove_min().unwrap();
                let out = Event::new(EventView::Out {
                    side: Side::Bid,
                    release_funds: true,
                    native_qty_unlocked: order.quantity() * order.price(),
                    native_qty_still_locked: 0,
                    order_id: order.order_id(),
                    owner: order.owner(),
                });
                event_queue
                    .push_back(out)
                    .map_err(|_| AOError::EventQueueFull)?;
                self.bids.insert_leaf(&new_leaf).unwrap();
            } else {
                insert_result.unwrap();
            }
        }
        Ok(())
    }

    pub(crate) fn new_ask(
        &mut self,
        params: new_order::Params,
        event_queue: &mut EventQueue,
    ) -> AOResult {
        let new_order::Params {
            max_base_qty,
            max_quote_qty: _,
            order_id,
            side: _,
            limit_price,
            owner,
            post_only,
            post_allowed,
            self_trade_behavior,
        } = params;

        let mut unfilled_base_qty = max_base_qty;
        let mut accum_fill_price = 0;

        let crossed;
        #[allow(clippy::never_loop)]
        let done = loop {
            let best_bid_h = match self.find_bbo(Side::Bid) {
                None => {
                    crossed = false;
                    break true;
                }
                Some(h) => h,
            };

            let mut best_bid_ref = self.bids.get_node(best_bid_h).unwrap().as_leaf().unwrap();

            let trade_price = best_bid_ref.price();
            crossed = limit_price <= trade_price;

            if !crossed || post_only {
                break true;
            }

            let bid_size = best_bid_ref.quantity();
            let trade_qty = bid_size.min(unfilled_base_qty);

            if trade_qty == 0 {
                break true;
            }

            let order_would_self_trade = owner == best_bid_ref.owner();
            if order_would_self_trade {
                let best_bid_id = best_bid_ref.order_id();
                let cancelled_provide_qty;
                let cancelled_take_qty;

                match self_trade_behavior {
                    SelfTradeBehavior::DecrementTake => {
                        cancelled_provide_qty = trade_qty;
                        cancelled_take_qty = trade_qty;
                    }
                    SelfTradeBehavior::CancelProvide => {
                        cancelled_provide_qty = best_bid_ref.quantity();
                        cancelled_take_qty = 0;
                    }
                    SelfTradeBehavior::AbortTransaction => return Err(AOError::WouldSelfTrade),
                };

                let remaining_provide_size = bid_size - cancelled_provide_qty;
                let provide_out = Event::new(EventView::Out {
                    side: Side::Bid,
                    release_funds: true,
                    native_qty_unlocked: cancelled_provide_qty * trade_price,
                    native_qty_still_locked: remaining_provide_size * trade_price,
                    order_id: best_bid_id,
                    owner: best_bid_ref.owner(),
                });
                event_queue
                    .push_back(provide_out)
                    .map_err(|_| AOError::EventQueueFull)?;
                if remaining_provide_size == 0 {
                    self.bids.remove_by_key(best_bid_id).unwrap();
                } else {
                    best_bid_ref.set_quantity(remaining_provide_size);
                }

                unfilled_base_qty -= cancelled_take_qty;
                let take_out = Event::new(EventView::Out {
                    side: Side::Ask,
                    release_funds: false,
                    native_qty_unlocked: cancelled_take_qty,
                    native_qty_still_locked: unfilled_base_qty,
                    order_id,
                    owner,
                });
                event_queue
                    .push_back(take_out)
                    .map_err(|_| AOError::EventQueueFull)?;
                // to_release.unlock_coin(cancelled_take_qty);

                // let order_remaining = OrderRemaining {
                //     coin_qty_remaining: unfilled_base_qty,
                //     native_pc_qty_remaining: None,
                // };
                return Ok(());
            }

            let native_maker_pc_qty = trade_qty * trade_price;

            let maker_fill = Event::new(EventView::Fill {
                side: Side::Bid,
                maker: true,
                native_qty_paid: native_maker_pc_qty,
                native_qty_received: trade_qty,
                order_id: best_bid_ref.order_id(),
                owner: best_bid_ref.owner(),
            });
            event_queue
                .push_back(maker_fill)
                .map_err(|_| AOError::EventQueueFull)?;

            best_bid_ref.set_quantity(best_bid_ref.quantity() - trade_qty);
            unfilled_base_qty -= trade_qty;
            accum_fill_price += trade_qty * trade_price;

            if best_bid_ref.quantity() == 0 {
                let best_bid_id = best_bid_ref.order_id();
                event_queue
                    .push_back(Event::new(EventView::Out {
                        side: Side::Bid,
                        release_funds: true,
                        native_qty_unlocked: 0,
                        native_qty_still_locked: 0,
                        order_id: best_bid_id,
                        owner: best_bid_ref.owner(),
                    }))
                    .map_err(|_| AOError::EventQueueFull)?;
                self.bids.remove_by_key(best_bid_id).unwrap();
            }

            break false;
        };

        let native_taker_pc_qty = accum_fill_price;

        {
            let net_taker_pc_qty = native_taker_pc_qty;
            let coin_lots_traded = max_base_qty - unfilled_base_qty;

            // to_release.credit_native_pc(net_taker_pc_qty);
            // to_release.debit_coin(coin_lots_traded);

            if native_taker_pc_qty > 0 {
                let taker_fill = Event::new(EventView::Fill {
                    side: Side::Ask,
                    maker: false,
                    native_qty_paid: coin_lots_traded,
                    native_qty_received: net_taker_pc_qty,
                    order_id,
                    owner,
                });
                event_queue
                    .push_back(taker_fill)
                    .map_err(|_| AOError::EventQueueFull)?;
            }
        }

        // self.market_state.pc_deposits_total -= net_fees_before_referrer_rebate;

        if !done {
            // if let Some(coin_qty_remaining) = NonZeroU64::new(unfilled_base_qty) {
            //     return Ok(Some(OrderRemaining {
            //         coin_qty_remaining,
            //         native_pc_qty_remaining: None,
            //     }));
            return Ok(());
        }

        if post_allowed && !crossed && unfilled_base_qty > 0 {
            let new_order = LeafNode::new(order_id, owner, unfilled_base_qty);
            let insert_result = self.asks.insert_leaf(&new_order);
            if let Err(AOError::SlabOutOfSpace) = insert_result {
                // boot out the least aggressive offer
                msg!("offers full! booting...");
                let order = self.asks.remove_max().unwrap();
                let out = Event::new(EventView::Out {
                    side: Side::Ask,
                    release_funds: true,
                    native_qty_unlocked: order.quantity(),
                    native_qty_still_locked: 0,
                    order_id: order.order_id(),
                    owner: order.owner(),
                });
                event_queue
                    .push_back(out)
                    .map_err(|_| AOError::EventQueueFull)?;
                self.asks.insert_leaf(&new_order).unwrap();
            } else {
                insert_result.unwrap();
            }
        } else {
            // to_release.unlock_coin(unfilled_base_qty);
            let out = Event::new(EventView::Out {
                side: Side::Ask,
                release_funds: false,
                native_qty_unlocked: unfilled_base_qty,
                native_qty_still_locked: 0,
                order_id,
                owner,
            });
            event_queue
                .push_back(out)
                .map_err(|_| AOError::EventQueueFull)?;
        }

        Ok(())
    }
}
