use std::rc::Rc;

use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::{
    critbit::Slab,
    orderbook::OrderBookState,
    state::{EventQueue, EventQueueHeader, MarketState, SelfTradeBehavior, Side},
};

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct NewOrderParams {
    pub max_base_qty: u64,
    pub max_quote_qty: u64,
    pub order_id: u128,
    pub limit_price: u64,
    pub side: Side,
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

    match params.side {
        Side::Bid => order_book.new_bid(params, &mut event_queue)?,
        Side::Ask => order_book.new_ask(params, &mut event_queue)?,
    }

    let mut event_queue_data: &mut [u8] = &mut accounts.event_queue.data.borrow_mut();
    event_queue.header.serialize(&mut event_queue_data).unwrap();

    //TODO rewrite OB

    Ok(())
}
