use std::rc::Rc;

use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::{
    critbit::{LeafNode, Slab},
    error::AOError,
    orderbook::OrderBookState,
    state::{Event, EventQueue, EventQueueHeader, EventView, MarketState, SelfTradeBehavior, Side},
};

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct NewOrderParams {
    pub max_base_qty: u64,
    pub max_quote_qty: u64,
    pub order_id: u128,
    pub limit_price: u64,
    pub owner: Pubkey,
    pub post_only: bool,
    pub post_allowed: bool,
    pub self_trade_behavior: SelfTradeBehavior,
}

//TODO make price FP32

struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    admin: &'a AccountInfo<'b>,
    asks: &'a AccountInfo<'b>,
    bids: &'a AccountInfo<'b>,
    event_queue: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();
        let market = next_account_info(accounts_iter)?;
        let admin = next_account_info(accounts_iter)?;
        let asks = next_account_info(accounts_iter)?;
        let bids = next_account_info(accounts_iter)?;
        let event_queue = next_account_info(accounts_iter)?;
        //TODO
        // check_account_owner(market, program_id)?;
        // check_signer(admin)?;
        Ok(Self {
            market,
            admin,
            asks,
            bids,
            event_queue,
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
        max_quote_qty,
        order_id,
        limit_price,
        owner,
        post_only,
        post_allowed,
        self_trade_behavior,
    } = params;

    let mut market_data: &[u8] = &accounts.market.data.borrow();
    let market_state = MarketState::deserialize(&mut market_data).unwrap();
    let mut order_book = OrderBookState {
        bids: Slab(Rc::clone(&accounts.bids.data)),
        asks: Slab(Rc::clone(&accounts.asks.data)),
        market_state,
    };

    let mut event_queue_data: &[u8] = &accounts.event_queue.data.borrow();
    let header = EventQueueHeader::deserialize(&mut event_queue_data).unwrap();
    let mut event_queue = EventQueue {
        header,
        buffer: Rc::clone(&accounts.event_queue.data),
    };

    let mut base_qty_remaining = max_base_qty;
    let mut quote_qty_remaining = max_quote_qty;

    // New bid
    let crossed;
    let done = loop {
        let best_offer_h = match order_book.find_bbo(Side::Ask) {
            None => {
                crossed = false;
                break true;
            }
            Some(h) => h,
        };

        let mut best_offer_ref = order_book
            .asks
            .get_node(best_offer_h)
            .unwrap()
            .as_leaf()
            .unwrap();

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
                SelfTradeBehavior::AbortTransaction => return Err(AOError::WouldSelfTrade.into()),
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
                order_book.asks.remove_by_key(best_offer_id).unwrap();
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
            order_book.asks.remove_by_key(best_offer_id).unwrap();
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

    if !done {
        if base_qty_remaining > 0 && base_qty_remaining > 0 {
            msg!("Unmatched remains from the order: base_qty_remaining {:?}, quote_qty_remaining {:?}", base_qty_remaining, quote_qty_remaining);
            return Ok(());
        }
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
        let mut bids = order_book.bids;
        let new_leaf = LeafNode::new(order_id, owner, coin_qty_to_post);
        let insert_result = bids.insert_leaf(&new_leaf);
        if let Err(AOError::SlabOutOfSpace) = insert_result {
            // boot out the least aggressive bid
            msg!("bids full! booting...");
            let order = bids.remove_min().unwrap();
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
            bids.insert_leaf(&new_leaf).unwrap();
        } else {
            insert_result.unwrap();
        }
    }

    Ok(())
}
