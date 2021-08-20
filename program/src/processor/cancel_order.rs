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
    critbit::Slab,
    error::AOError,
    orderbook::OrderBookState,
    state::{
        Event, EventQueue, EventQueueHeader, EventView, MarketState, SelfTradeBehavior, Side,
        EVENT_QUEUE_HEADER_LEN,
    },
    utils::{check_account_key, check_account_owner, check_signer},
};

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct Params {
    pub order_id: u128,
    pub side: Side,
}

struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    admin: &'a AccountInfo<'b>,
    asks: &'a AccountInfo<'b>,
    bids: &'a AccountInfo<'b>,
    order_owner: &'a AccountInfo<'b>,
    event_queue: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();
        let market = next_account_info(accounts_iter)?;
        let event_queue = next_account_info(accounts_iter)?;
        let bids = next_account_info(accounts_iter)?;
        let asks = next_account_info(accounts_iter)?;
        let order_owner = next_account_info(accounts_iter)?;
        let admin = next_account_info(accounts_iter)?;
        check_account_owner(market, program_id)?;
        check_account_owner(event_queue, program_id)?;
        check_account_owner(bids, program_id)?;
        check_account_owner(asks, program_id)?;
        check_signer(order_owner)?;
        //TODO check if caller auth signs?
        Ok(Self {
            market,
            admin,
            asks,
            bids,
            order_owner,
            event_queue,
        })
    }
}

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], params: Params) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    let mut market_data: &[u8] = &accounts.market.data.borrow();
    let market_state = MarketState::deserialize(&mut market_data).unwrap();

    check_account_key(accounts.event_queue, &market_state.event_queue).unwrap();
    check_account_key(accounts.bids, &market_state.bids).unwrap();
    check_account_key(accounts.asks, &market_state.asks).unwrap();

    let order_book = OrderBookState {
        bids: Slab(Rc::clone(&accounts.bids.data)),
        asks: Slab(Rc::clone(&accounts.asks.data)),
        market_state,
    };

    let header = {
        let mut event_queue_data: &[u8] =
            &accounts.event_queue.data.borrow()[0..EVENT_QUEUE_HEADER_LEN];
        EventQueueHeader::deserialize(&mut event_queue_data).unwrap()
    };
    let mut event_queue = EventQueue {
        header,
        buffer: Rc::clone(&accounts.event_queue.data),
    };

    let mut slab = match params.side {
        Side::Bid => order_book.bids,
        Side::Ask => order_book.asks,
    };
    let leaf_node = slab
        .remove_by_key(params.order_id)
        .ok_or(AOError::OrderNotFound)?;

    if leaf_node.owner() != *accounts.order_owner.key {
        msg!("Order owner mismatch.");
        return Err(AOError::OrderNotYours.into());
    }

    let native_qty_unlocked = match params.side {
        Side::Bid => leaf_node.quantity() * leaf_node.price(),
        Side::Ask => leaf_node.quantity(),
    };

    event_queue
        .push_back(Event::new(EventView::Out {
            side: params.side,
            release_funds: false,
            native_qty_unlocked,
            native_qty_still_locked: 0,
            owner: *accounts.order_owner.key,
            order_id: params.order_id,
        }))
        .map_err(|_| AOError::EventQueueFull)?;

    let mut event_queue_header_data: &mut [u8] = &mut accounts.event_queue.data.borrow_mut();
    event_queue
        .header
        .serialize(&mut event_queue_header_data)
        .unwrap();

    Ok(())
}
