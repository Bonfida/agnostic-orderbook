use std::{cell::RefMut, rc::Rc};

use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::{
    critbit::Slab,
    error::{AOError, AOResult},
    orderbook::OrderBookState,
    state::{Event, EventView, MarketState, SelfTradeBehavior, Side},
};

#[derive(BorshDeserialize, BorshSerialize)]
pub struct NewOrderParams {
    pub max_base_qty: u64,
    pub max_quote_qty_locked: u64,
    pub limit_price: u64,
    pub owner: Pubkey,
    pub post_only: bool,
    pub post_allowed: u64,
    pub self_trade_behavior: SelfTradeBehavior,
}

//TODO reintroduce lot sizes?

struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    // admin: &'a AccountInfo<'b>,
    asks: &'a AccountInfo<'b>,
    bids: &'a AccountInfo<'b>,
    // event_queue: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();
        let market = next_account_info(accounts_iter)?;
        // let admin = next_account_info(accounts_iter)?;
        let asks = next_account_info(accounts_iter)?;
        let bids = next_account_info(accounts_iter)?;
        // let event_queue = next_account_info(accounts_iter)?;
        // check_account_owner(market, program_id)?;
        // check_signer(admin)?;
        Ok(Self {
            market,
            // admin,
            asks,
            bids,
            // event_queue,
        })
    }
}

pub fn process_new_order(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    params: NewOrderParams,
) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    let NewOrderParams {
        max_base_qty,
        max_quote_qty_locked,
        limit_price,
        owner,
        post_only,
        post_allowed,
        self_trade_behavior,
    } = params;

    let mut market_data: &[u8] = &accounts.market.data.borrow();
    let mut market_state = MarketState::deserialize(&mut market_data).unwrap();
    let order_book = OrderBookState {
        bids: Slab(Rc::clone(&accounts.bids.data)),
        asks: Slab(Rc::clone(&accounts.asks.data)),
        market_state,
    };

    // let event_queue = accounts.event_queue.data.borrow();

    // New bid
    // let crossed;
    // let done = loop {
    let best_offer_h = order_book.find_bbo(Side::Ask); //{
                                                       //     let best_offer_h = match order_book.find_bbo(Side::Ask) {
                                                       //     None => {
                                                       //         crossed = false;
                                                       //         break true;
                                                       //     }
                                                       //     Some(h) => h,
                                                       // };

    // let best_offer_ref = order_book
    //     .orders_mut(Side::Ask)
    //     .get_mut(best_offer_h)
    //     .unwrap()
    //     .as_leaf_mut()
    //     .unwrap();

    // let trade_price = best_offer_ref.price();
    // crossed = limit_price >= trade_price;
    // if !crossed || post_only {
    //     break true;
    // }

    // let offer_size = best_offer_ref.quantity();
    // let trade_qty = offer_size
    //     .min(max_base_qty)
    //     .min(max_quote_qty_locked / best_offer_ref.price().get());

    // if trade_qty == 0 {
    //     break true;
    // }

    // let order_would_self_trade = owner == best_offer_ref.owner();
    // if order_would_self_trade {
    //     let best_offer_id = best_offer_ref.order_id();

    //     let cancelled_take_qty;
    //     let cancelled_provide_qty;

    //     match self_trade_behavior {
    //         SelfTradeBehavior::CancelProvide => {
    //             cancelled_take_qty = 0;
    //             cancelled_provide_qty = best_offer_ref.quantity();
    //         }
    //         SelfTradeBehavior::DecrementTake => {
    //             cancelled_take_qty = trade_qty;
    //             cancelled_provide_qty = trade_qty;
    //         }
    //         SelfTradeBehavior::AbortTransaction => return Err(AOError::WouldSelfTrade.into()),
    //     };

    //     let remaining_provide_qty = best_offer_ref.quantity() - cancelled_provide_qty;
    //     let provide_out = Event::new(EventView::Out {
    //         side: Side::Ask,
    //         release_funds: true,
    //         native_qty_unlocked: cancelled_provide_qty,
    //         native_qty_still_locked: remaining_provide_qty,
    //         owner: best_offer_ref.owner(),
    //     });
    //         event_q
    //             .push_back(provide_out)
    //             .map_err(|_| AOError::EventQueueFull)?;
    //         if remaining_provide_qty == 0 {
    //             order_book
    //                 .orders_mut(Side::Ask)
    //                 .remove_by_key(best_offer_id)
    //                 .unwrap();
    //         } else {
    //             best_offer_ref.set_quantity(remaining_provide_qty);
    //         }

    //         let native_taker_pc_unlocked = cancelled_take_qty * trade_price.get() * pc_lot_size;
    //         let native_taker_pc_still_locked =
    //             native_pc_qty_locked.get() - native_taker_pc_unlocked;

    //         let order_remaining = (|| {
    //             Some(OrderRemaining {
    //                 coin_qty_remaining: NonZeroU64::new(coin_qty_remaining - cancelled_take_qty)?,
    //                 native_pc_qty_remaining: Some(NonZeroU64::new(native_taker_pc_still_locked)?),
    //             })
    //         })();

    //         {
    //             let native_qty_unlocked;
    //             let native_qty_still_locked;
    //             match order_remaining {
    //                 Some(_) => {
    //                     native_qty_unlocked = native_taker_pc_unlocked;
    //                     native_qty_still_locked = native_taker_pc_still_locked;
    //                 }
    //                 None => {
    //                     native_qty_unlocked = native_pc_qty_locked.get();
    //                     native_qty_still_locked = 0;
    //                 }
    //             };
    //             to_release.unlock_native_pc(native_qty_unlocked);
    //             let take_out = Event::new(EventView::Out {
    //                 side: Side::Bid,
    //                 release_funds: false,
    //                 native_qty_unlocked,
    //                 native_qty_still_locked,
    //                 order_id,
    //                 owner,
    //                 owner_slot,
    //                 client_order_id: NonZeroU64::new(client_order_id),
    //             });
    //             event_q
    //                 .push_back(take_out)
    //                 .map_err(|_| DexErrorCode::EventQueueFull)?;
    //         };

    //         return Ok(order_remaining);
    //     }
    //     let maker_fee_tier = best_offer_ref.fee_tier();
    //     let native_maker_pc_qty = trade_qty * trade_price.get() * pc_lot_size;
    //     let native_maker_rebate = maker_fee_tier.maker_rebate(native_maker_pc_qty);
    //     accum_maker_rebates += native_maker_rebate;

    //     let maker_fill = Event::new(EventView::Fill {
    //         side: Side::Ask,
    //         maker: true,
    //         native_qty_paid: trade_qty * coin_lot_size,
    //         native_qty_received: native_maker_pc_qty + native_maker_rebate,
    //         native_fee_or_rebate: native_maker_rebate,
    //         order_id: best_offer_ref.order_id(),
    //         owner: best_offer_ref.owner(),
    //         owner_slot: best_offer_ref.owner_slot(),
    //         fee_tier: maker_fee_tier,
    //         client_order_id: NonZeroU64::new(best_offer_ref.client_order_id()),
    //     });
    //     event_q
    //         .push_back(maker_fill)
    //         .map_err(|_| DexErrorCode::EventQueueFull)?;

    //     best_offer_ref.set_quantity(best_offer_ref.quantity() - trade_qty);
    //     coin_qty_remaining -= trade_qty;
    //     pc_qty_remaining -= trade_qty * trade_price.get();

    //     if best_offer_ref.quantity() == 0 {
    //         let best_offer_id = best_offer_ref.order_id();
    //         event_q
    //             .push_back(Event::new(EventView::Out {
    //                 side: Side::Ask,
    //                 release_funds: true,
    //                 native_qty_unlocked: 0,
    //                 native_qty_still_locked: 0,
    //                 order_id: best_offer_id,
    //                 owner: best_offer_ref.owner(),
    //                 owner_slot: best_offer_ref.owner_slot(),
    //                 client_order_id: NonZeroU64::new(best_offer_ref.client_order_id()),
    //             }))
    //             .map_err(|_| DexErrorCode::EventQueueFull)?;
    //         order_book
    //             .orders_mut(Side::Ask)
    //             .remove_by_key(best_offer_id)
    //             .unwrap();
    //     }

    //     break false;
    // };

    // let native_accum_fill_price = (max_pc_qty - pc_qty_remaining) * pc_lot_size;
    // let native_taker_fee = fee_tier.taker_fee(native_accum_fill_price);
    // let native_pc_qty_remaining =
    //     native_pc_qty_locked.get() - native_accum_fill_price - native_taker_fee;

    // {
    //     let coin_lots_received = max_coin_qty.get() - coin_qty_remaining;
    //     let native_pc_paid = native_accum_fill_price + native_taker_fee;

    //     to_release.credit_coin(coin_lots_received);
    //     to_release.debit_native_pc(native_pc_paid);

    //     if native_accum_fill_price > 0 {
    //         let taker_fill = Event::new(EventView::Fill {
    //             side: Side::Bid,
    //             maker: false,
    //             native_qty_paid: native_pc_paid,
    //             native_qty_received: coin_lots_received * coin_lot_size,
    //             native_fee_or_rebate: native_taker_fee,
    //             order_id,
    //             owner,
    //             owner_slot,
    //             fee_tier,
    //             client_order_id: NonZeroU64::new(client_order_id),
    //         });
    //         event_q
    //             .push_back(taker_fill)
    //             .map_err(|_| DexErrorCode::EventQueueFull)?;
    //     }
    // }

    // let net_fees_before_referrer_rebate = native_taker_fee - accum_maker_rebates;
    // let referrer_rebate = fees::referrer_rebate(native_taker_fee);
    // let net_fees = net_fees_before_referrer_rebate - referrer_rebate;

    // order_book.market_state.referrer_rebates_accrued += referrer_rebate;
    // order_book.market_state.pc_fees_accrued += net_fees;
    // order_book.market_state.pc_deposits_total -= net_fees_before_referrer_rebate;

    // if !done {
    //     if let Some(coin_qty_remaining) = NonZeroU64::new(coin_qty_remaining) {
    //         if let Some(native_pc_qty_remaining) = NonZeroU64::new(native_pc_qty_remaining) {
    //             return Ok(Some(OrderRemaining {
    //                 coin_qty_remaining,
    //                 native_pc_qty_remaining: Some(native_pc_qty_remaining),
    //             }));
    //         }
    //     }
    // }

    // let (coin_qty_to_post, pc_qty_to_keep_locked) = match limit_price {
    //     Some(price) if post_allowed && !crossed => {
    //         let coin_qty_to_post =
    //             coin_qty_remaining.min(native_pc_qty_remaining / pc_lot_size / price.get());
    //         (coin_qty_to_post, coin_qty_to_post * price.get())
    //     }
    //     _ => (0, 0),
    // };

    // let out = {
    //     let native_qty_still_locked = pc_qty_to_keep_locked * pc_lot_size;
    //     let native_qty_unlocked = native_pc_qty_remaining - native_qty_still_locked;

    //     to_release.unlock_native_pc(native_qty_unlocked);

    //     Event::new(EventView::Out {
    //         side: Side::Bid,
    //         release_funds: false,
    //         native_qty_unlocked,
    //         native_qty_still_locked,
    //         order_id,
    //         owner,
    //         owner_slot,
    //         client_order_id: NonZeroU64::new(client_order_id),
    //     })
    // };
    // event_q
    //     .push_back(out)
    //     .map_err(|_| DexErrorCode::EventQueueFull)?;

    // if pc_qty_to_keep_locked > 0 {
    //     let bids = order_book.orders_mut(Side::Bid);
    //     let new_leaf = LeafNode::new(
    //         owner_slot,
    //         order_id,
    //         owner,
    //         coin_qty_to_post,
    //         fee_tier,
    //         client_order_id,
    //     );
    //     let insert_result = bids.insert_leaf(&new_leaf);
    //     if let Err(SlabTreeError::OutOfSpace) = insert_result {
    //         // boot out the least aggressive bid
    //         msg!("bids full! booting...");
    //         let order = bids.remove_min().unwrap();
    //         let out = Event::new(EventView::Out {
    //             side: Side::Bid,
    //             release_funds: true,
    //             native_qty_unlocked: order.quantity() * order.price().get() * pc_lot_size,
    //             native_qty_still_locked: 0,
    //             order_id: order.order_id(),
    //             owner: order.owner(),
    //             owner_slot: order.owner_slot(),
    //             client_order_id: NonZeroU64::new(order.client_order_id()),
    //         });
    //         event_q
    //             .push_back(out)
    //             .map_err(|_| DexErrorCode::EventQueueFull)?;
    //         bids.insert_leaf(&new_leaf).unwrap();
    //     } else {
    //         insert_result.unwrap();
    //     }
    // }

    Ok(())
}
