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

    pub fn get_spread(&self) -> (Option<u64>, Option<u64>) {
        let best_bid_price = self
            .bids
            .find_max()
            .map(|h| self.bids.leaf_nodes[h as usize].price());
        let best_ask_price = self
            .asks
            .find_min()
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

            let opposite_slab = self.get_tree(side.opposite());

            let mut best_bo_ref = &mut opposite_slab.leaf_nodes[best_bo_h as usize];

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
                    == opposite_slab.callback_infos[best_bo_h as usize].as_callback_id();
                if order_would_self_trade {
                    let best_offer_id = best_bo_ref.order_id();

                    if self_trade_behavior == SelfTradeBehavior::AbortTransaction {
                        return Err(AoError::WouldSelfTrade);
                    }
                    let provide_out_callback_info =
                        &opposite_slab.callback_infos[best_bo_h as usize];
                    let provide_out = OutEvent {
                        side: side.opposite() as u8,
                        delete: true as u8,
                        order_id: best_offer_id,
                        base_size: best_bo_ref.base_quantity,
                        tag: EventTag::Out as u8,
                        _padding: [0; 13],
                    };
                    println!("Hi");
                    event_queue
                        .push_back(provide_out, Some(provide_out_callback_info), None)
                        .map_err(|_| AoError::EventQueueFull)?;

                    self.get_tree(side.opposite())
                        .remove_by_key(best_offer_id)
                        .unwrap();

                    match_limit -= 1;

                    continue;
                }
            }

            let maker_callback_info = &opposite_slab.callback_infos[best_bo_h as usize];

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
                    side: cur_side as u8,
                    order_id: best_offer_id,
                    base_size: best_bo_ref.base_quantity,
                    delete: true as u8,
                    tag: EventTag::Out as u8,
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
        let k = if let Err(AoError::SlabOutOfSpace) = insert_result {
            // Boot out the least aggressive orders
            msg!("Orderbook is full! booting least aggressive orders...");
            let slab = self.get_tree(side);
            let boot_candidate = match side {
                Side::Bid => slab.find_min().unwrap(),
                Side::Ask => slab.find_max().unwrap(),
            };
            let boot_candidate_key = slab.leaf_nodes[boot_candidate as usize].key;
            let boot_candidate_price = LeafNode::price_from_key(boot_candidate_key);
            let should_boot = match side {
                Side::Bid => boot_candidate_price < limit_price,
                Side::Ask => boot_candidate_price > limit_price,
            };
            if should_boot {
                let (order, callback_info_booted) = slab.remove_by_key(boot_candidate_key).unwrap();
                let out = OutEvent {
                    side: side as u8,
                    delete: true as u8,
                    order_id: order.order_id(),
                    base_size: order.base_quantity,
                    tag: EventTag::Out as u8,
                    _padding: [0; 13],
                };
                event_queue
                    .push_back(out, Some(callback_info_booted), None)
                    .map_err(|_| AoError::EventQueueFull)?;
                slab.insert_leaf(&new_leaf).unwrap().0
            } else {
                return Ok(OrderSummary {
                    posted_order_id: None,
                    total_base_qty: max_base_qty - base_qty_remaining,
                    total_quote_qty: max_quote_qty - quote_qty_remaining,
                    total_base_qty_posted: 0,
                });
            }
        } else {
            insert_result.unwrap().0
        };
        *self.get_tree(side).get_callback_info_mut(k) = callback_info;
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

#[cfg(test)]
mod tests {
    use solana_program::pubkey::Pubkey;

    use crate::state::event_queue::{EventRef, FillEventRef, OutEventRef};

    use super::*;

    type SlabTest<'a> = Slab<'a, [u8; 32]>;
    type OrderBookStateTest<'a> = OrderBookState<'a, [u8; 32]>;
    type EventQueueTest<'a> = EventQueue<'a, [u8; 32]>;

    pub struct TestContext {
        asks_buffer: Vec<u8>,
        bids_buffer: Vec<u8>,
        event_queue_buffer: Vec<u8>,
    }

    impl TestContext {
        pub fn new(order_capacity: usize, event_capacity: usize) -> Self {
            let allocation_size = SlabTest::compute_allocation_size(order_capacity);
            let (mut asks_buffer, mut bids_buffer) =
                (vec![0; allocation_size], vec![0; allocation_size]);
            SlabTest::initialize(&mut asks_buffer, &mut bids_buffer, Pubkey::new_unique()).unwrap();
            Self {
                asks_buffer,
                bids_buffer,
                event_queue_buffer: vec![
                    0;
                    EventQueueTest::compute_allocation_size(event_capacity)
                ],
            }
        }
        pub fn get(&mut self) -> (OrderBookStateTest, EventQueueTest) {
            (
                OrderBookStateTest::new_safe(&mut self.bids_buffer, &mut self.asks_buffer).unwrap(),
                EventQueueTest::from_buffer(
                    &mut self.event_queue_buffer,
                    AccountTag::Uninitialized,
                )
                .unwrap(),
            )
        }
    }

    #[test]
    fn test_ob_0() {
        let mut test_context = TestContext::new(1000, 1000);
        let (mut orderbook, mut event_queue) = test_context.get();
        let alice = [1; 32];
        let bob = [2; 32];
        orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 10,
                    max_quote_qty: 10,
                    limit_price: 10 << 32,
                    side: Side::Ask,
                    match_limit: 0,
                    callback_info: [0; 32],
                    post_only: false,
                    post_allowed: false,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                10,
            )
            .unwrap();
        assert!(event_queue.header.count == 0);

        // Alice posts a bid order for 1 BTC at 10 USD/BTC

        let OrderSummary {
            posted_order_id,
            total_base_qty,
            total_quote_qty,
            total_base_qty_posted,
        } = orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 2_000_000,
                    max_quote_qty: 10_000_000,
                    limit_price: 10 << 32,
                    side: Side::Bid,
                    match_limit: 10,
                    callback_info: alice,
                    post_only: false,
                    post_allowed: true,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                10,
            )
            .unwrap();
        assert!(posted_order_id.is_some());
        assert_eq!(total_base_qty, 1_000_000);
        assert_eq!(total_quote_qty, 10_000_000);
        assert_eq!(total_base_qty_posted, 1_000_000);

        // Bob posts an ask order for 3 BTC at 20 USD/BTC
        let OrderSummary {
            posted_order_id,
            total_base_qty,
            total_quote_qty,
            total_base_qty_posted,
        } = orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 3_000_000,
                    max_quote_qty: 1_000_000_000,
                    limit_price: 20 << 32,
                    side: Side::Ask,
                    match_limit: 10,
                    callback_info: bob,
                    post_only: false,
                    post_allowed: true,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                10,
            )
            .unwrap();
        assert!(posted_order_id.is_some());
        assert_eq!(total_base_qty, 3_000_000);
        assert_eq!(total_quote_qty, 60_000_000);
        assert_eq!(total_base_qty_posted, 3_000_000);

        // Bob posts a bid order for 0.5 BTC at 15 USD/BTC
        let OrderSummary {
            posted_order_id: bob_order_id_0,
            total_base_qty,
            total_quote_qty,
            total_base_qty_posted,
        } = orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 500_000,
                    max_quote_qty: 1_000_000_000,
                    limit_price: 15 << 32,
                    side: Side::Bid,
                    match_limit: 10,
                    callback_info: bob,
                    post_only: false,
                    post_allowed: true,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                10,
            )
            .unwrap();
        assert!(posted_order_id.is_some());
        assert_eq!(total_base_qty, 500_000);
        assert_eq!(total_quote_qty, 7_500_000);
        assert_eq!(total_base_qty_posted, 500_000);
        assert_eq!(orderbook.get_spread(), (Some(15 << 32), Some(20 << 32)));

        // Alice posts an ask order for 0.75 BTC at 14 USD/BTC, and is partially matched
        let OrderSummary {
            posted_order_id: alice_order_id_0,
            total_base_qty,
            total_quote_qty,
            total_base_qty_posted,
        } = orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 750_000,
                    max_quote_qty: 1_000_000_000,
                    limit_price: 14 << 32,
                    side: Side::Ask,
                    match_limit: 10,
                    callback_info: alice,
                    post_only: false,
                    post_allowed: true,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                10,
            )
            .unwrap();
        assert!(posted_order_id.is_some());
        assert_eq!(total_base_qty, 750_000);
        assert_eq!(total_quote_qty, 500_000 * 15 + 250_000 * 14);
        assert_eq!(total_base_qty_posted, 250_000);
        assert_eq!(orderbook.get_spread(), (Some(10 << 32), Some(14 << 32)));

        assert_eq!(event_queue.header.count, 2);
        let mut event_queue_iter = event_queue.iter();
        assert_eq!(
            event_queue_iter.next().unwrap(),
            EventRef::Fill(FillEventRef {
                event: &FillEvent {
                    tag: EventTag::Fill as u8,
                    taker_side: Side::Ask as u8,
                    _padding: [0; 6],
                    quote_size: 500_000 * 15,
                    maker_order_id: bob_order_id_0.unwrap(),
                    base_size: 500_000
                },
                maker_callback_info: &bob,
                taker_callback_info: &alice
            })
        );

        assert_eq!(
            event_queue_iter.next().unwrap(),
            EventRef::Out(OutEventRef {
                event: &OutEvent {
                    tag: EventTag::Out as u8,
                    side: Side::Bid as u8,
                    _padding: [0; 13],
                    base_size: 0,
                    delete: true as u8,
                    order_id: bob_order_id_0.unwrap()
                },
                callback_info: &bob
            })
        );
        println!("Event queue head: {}", event_queue.header.head);
        event_queue.pop_n(2);
        println!("Event queue head: {}", event_queue.header.head);

        assert_eq!(event_queue.header.count, 0);

        // Alice makes a bid for 0.05 BTC at 15 USD/BTC, and attempts to self-trade
        let r = orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 50_000,
                    max_quote_qty: 1_000_000_000,
                    limit_price: 15 << 32,
                    side: Side::Bid,
                    match_limit: 10,
                    callback_info: alice,
                    post_only: false,
                    post_allowed: true,
                    self_trade_behavior: SelfTradeBehavior::AbortTransaction,
                },
                &mut event_queue,
                10,
            )
            .unwrap_err();
        assert!(matches!(r, AoError::WouldSelfTrade));
        println!("Event queue head: {}", event_queue.header.head);

        assert_eq!(event_queue.header.count, 0);

        // Alice changes her self trading behavior

        let OrderSummary {
            posted_order_id,
            total_base_qty,
            total_quote_qty,
            total_base_qty_posted,
        } = orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 50_000,
                    max_quote_qty: 1_000_000_000,
                    limit_price: 15 << 32,
                    side: Side::Bid,
                    match_limit: 10,
                    callback_info: alice,
                    post_only: false,
                    post_allowed: true,
                    self_trade_behavior: SelfTradeBehavior::CancelProvide,
                },
                &mut event_queue,
                10,
            )
            .unwrap();
        assert!(posted_order_id.is_some());
        assert_eq!(total_base_qty, 50_000);
        assert_eq!(total_quote_qty, 50_000 * 15);
        assert_eq!(total_base_qty_posted, 50_000);
        assert_eq!(event_queue.header.count, 1);
        println!("Event queue head: {}", event_queue.header.head);

        assert_eq!(
            event_queue.iter().next().unwrap(),
            EventRef::Out(OutEventRef {
                event: &OutEvent {
                    tag: EventTag::Out as u8,
                    side: Side::Ask as u8,
                    _padding: [0; 13],
                    base_size: 250_000,
                    delete: true as u8,
                    order_id: alice_order_id_0.unwrap()
                },
                callback_info: &alice
            })
        );

        event_queue.pop_n(1);

        orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 1,
                    max_quote_qty: 10,
                    limit_price: 1,
                    side: Side::Ask,
                    match_limit: 10,
                    callback_info: [0; 32],
                    post_only: false,
                    post_allowed: false,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                10,
            )
            .unwrap();
        assert!(event_queue.header.count == 0);

        println!("Spread : {:?}", orderbook.get_spread());
    }

    #[test]
    fn test_ob_booting_ask() {
        let mut test_context = TestContext::new(2, 1000);
        let (mut orderbook, mut event_queue) = test_context.get();
        let alice = [1; 32];

        // Alice posts an ask order for 3 BTC at 20 USD/BTC
        let OrderSummary {
            posted_order_id,
            total_base_qty,
            total_quote_qty,
            total_base_qty_posted,
        } = orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 3_000_000,
                    max_quote_qty: 1_000_000_000,
                    limit_price: 20 << 32,
                    side: Side::Ask,
                    match_limit: 10,
                    callback_info: alice,
                    post_only: false,
                    post_allowed: true,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                10,
            )
            .unwrap();
        assert!(posted_order_id.is_some());
        assert_eq!(total_base_qty, 3_000_000);
        assert_eq!(total_quote_qty, 60_000_000);
        assert_eq!(total_base_qty_posted, 3_000_000);
        assert_eq!(event_queue.header.count, 0);

        // Alice posts an ask order for 6 BTC at 40 USD/BTC
        let OrderSummary {
            posted_order_id: order_id_to_be_booted,
            total_base_qty,
            total_quote_qty,
            total_base_qty_posted,
        } = orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 6_000_000,
                    max_quote_qty: 1_000_000_000,
                    limit_price: 40 << 32,
                    side: Side::Ask,
                    match_limit: 10,
                    callback_info: alice,
                    post_only: false,
                    post_allowed: true,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                10,
            )
            .unwrap();
        assert!(posted_order_id.is_some());
        assert_eq!(total_base_qty, 6_000_000);
        assert_eq!(total_quote_qty, 240_000_000);
        assert_eq!(total_base_qty_posted, 6_000_000);
        assert_eq!(event_queue.header.count, 0);

        // Alice posts an ask order for 1 BTC at 10 USD/BTC
        let OrderSummary {
            posted_order_id,
            total_base_qty,
            total_quote_qty,
            total_base_qty_posted,
        } = orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 1_000_000,
                    max_quote_qty: 1_000_000_000,
                    limit_price: 10 << 32,
                    side: Side::Ask,
                    match_limit: 10,
                    callback_info: alice,
                    post_only: false,
                    post_allowed: true,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                10,
            )
            .unwrap();
        assert!(posted_order_id.is_some());
        assert_eq!(total_base_qty, 1_000_000);
        assert_eq!(total_quote_qty, 10_000_000);
        assert_eq!(total_base_qty_posted, 1_000_000);
        assert_eq!(event_queue.header.count, 1);

        assert_eq!(
            event_queue.iter().next().unwrap(),
            EventRef::Out(OutEventRef {
                event: &OutEvent {
                    tag: EventTag::Out as u8,
                    side: Side::Ask as u8,
                    _padding: [0; 13],
                    base_size: 6_000_000,
                    delete: true as u8,
                    order_id: order_id_to_be_booted.unwrap()
                },
                callback_info: &alice
            })
        );

        event_queue.pop_n(1);

        // Alice posts an ask order for 1 BTC at 50 USD/BTC, but is not added to the ob
        let OrderSummary {
            posted_order_id,
            total_base_qty,
            total_quote_qty,
            total_base_qty_posted,
        } = orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 1_000_000,
                    max_quote_qty: 1_000_000_000,
                    limit_price: 50 << 32,
                    side: Side::Ask,
                    match_limit: 10,
                    callback_info: alice,
                    post_only: false,
                    post_allowed: true,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                10,
            )
            .unwrap();
        assert!(posted_order_id.is_none());
        assert_eq!(total_base_qty, 0);
        assert_eq!(total_quote_qty, 0);
        assert_eq!(total_base_qty_posted, 0);
        assert_eq!(event_queue.header.count, 0);
    }

    #[test]
    fn test_ob_small_prices() {
        let mut test_context = TestContext::new(2, 1000);
        let (mut orderbook, mut event_queue) = test_context.get();
        let alice = [1; 32];

        // Alice posts a bid order for 1 BTC at 0.25 USD/BTC

        let OrderSummary {
            posted_order_id,
            total_base_qty,
            total_quote_qty,
            total_base_qty_posted,
        } = orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 2_000_000,
                    max_quote_qty: 10_000_000,
                    limit_price: 1 << 30,
                    side: Side::Bid,
                    match_limit: 10,
                    callback_info: alice,
                    post_only: false,
                    post_allowed: true,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                10,
            )
            .unwrap();
        assert!(posted_order_id.is_some());
        assert_eq!(total_base_qty, 2_000_000);
        assert_eq!(total_quote_qty, 500_000);
        assert_eq!(total_base_qty_posted, 2_000_000);
        assert!(event_queue.header.count == 0);

        // Best bid is 0.25 USD / BTC

        orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 3,
                    max_quote_qty: 1_000_000,
                    limit_price: 1 << 28,
                    side: Side::Ask,
                    match_limit: 10,
                    callback_info: [0; 32],
                    post_only: false,
                    post_allowed: false,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                10,
            )
            .unwrap();
        assert!(event_queue.header.count == 0);
    }

    #[test]
    fn test_ob_booting_bid() {
        let mut test_context = TestContext::new(2, 1000);
        let (mut orderbook, mut event_queue) = test_context.get();
        let alice = [1; 32];

        // Alice posts an ask order for 3 BTC at 20 USD/BTC
        let OrderSummary {
            posted_order_id,
            total_base_qty,
            total_quote_qty,
            total_base_qty_posted,
        } = orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 3_000_000,
                    max_quote_qty: 1_000_000_000,
                    limit_price: 20 << 32,
                    side: Side::Bid,
                    match_limit: 10,
                    callback_info: alice,
                    post_only: false,
                    post_allowed: true,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                10,
            )
            .unwrap();
        assert!(posted_order_id.is_some());
        assert_eq!(total_base_qty, 3_000_000);
        assert_eq!(total_quote_qty, 60_000_000);
        assert_eq!(total_base_qty_posted, 3_000_000);
        assert_eq!(event_queue.header.count, 0);

        // Alice posts a bid order for 6 BTC at 10 USD/BTC
        let OrderSummary {
            posted_order_id: order_id_to_be_booted,
            total_base_qty,
            total_quote_qty,
            total_base_qty_posted,
        } = orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 6_000_000,
                    max_quote_qty: 1_000_000_000,
                    limit_price: 10 << 32,
                    side: Side::Bid,
                    match_limit: 10,
                    callback_info: alice,
                    post_only: false,
                    post_allowed: true,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                10,
            )
            .unwrap();
        assert!(posted_order_id.is_some());
        assert_eq!(total_base_qty, 6_000_000);
        assert_eq!(total_quote_qty, 60_000_000);
        assert_eq!(total_base_qty_posted, 6_000_000);
        assert_eq!(event_queue.header.count, 0);

        // Alice posts a bid order for 1 BTC at 40 USD/BTC
        let OrderSummary {
            posted_order_id,
            total_base_qty,
            total_quote_qty,
            total_base_qty_posted,
        } = orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 1_000_000,
                    max_quote_qty: 1_000_000_000,
                    limit_price: 40 << 32,
                    side: Side::Bid,
                    match_limit: 10,
                    callback_info: alice,
                    post_only: false,
                    post_allowed: true,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                10,
            )
            .unwrap();
        assert!(posted_order_id.is_some());
        assert_eq!(total_base_qty, 1_000_000);
        assert_eq!(total_quote_qty, 40_000_000);
        assert_eq!(total_base_qty_posted, 1_000_000);
        assert_eq!(event_queue.header.count, 1);

        assert_eq!(
            event_queue.iter().next().unwrap(),
            EventRef::Out(OutEventRef {
                event: &OutEvent {
                    tag: EventTag::Out as u8,
                    side: Side::Bid as u8,
                    _padding: [0; 13],
                    base_size: 6_000_000,
                    delete: true as u8,
                    order_id: order_id_to_be_booted.unwrap()
                },
                callback_info: &alice
            })
        );

        event_queue.pop_n(1);

        // Alice posts an ask order for 1 BTC at 50 USD/BTC, but is not added to the ob
        let OrderSummary {
            posted_order_id,
            total_base_qty,
            total_quote_qty,
            total_base_qty_posted,
        } = orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 1_000_000,
                    max_quote_qty: 1_000_000_000,
                    limit_price: 5 << 32,
                    side: Side::Bid,
                    match_limit: 10,
                    callback_info: alice,
                    post_only: false,
                    post_allowed: true,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                10,
            )
            .unwrap();
        assert!(posted_order_id.is_none());
        assert_eq!(total_base_qty, 0);
        assert_eq!(total_quote_qty, 0);
        assert_eq!(total_base_qty_posted, 0);
        assert_eq!(event_queue.header.count, 0);
    }
}
