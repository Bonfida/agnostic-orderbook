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
    orderbook::OrderBookState,
    state::{
        EventQueue, EventQueueHeader, MarketState, SelfTradeBehavior, Side, EVENT_QUEUE_HEADER_LEN,
    },
    utils::{check_account_key, check_account_owner, check_signer},
};

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct Params {
    pub max_asset_qty: u64,
    pub max_quote_qty: u64,
    pub order_id: u128,
    pub limit_price: u64,
    pub side: Side,
    pub match_limit: u64,
    pub callback_info: Vec<u8>,
    pub post_only: bool,
    pub post_allowed: bool,
    pub self_trade_behavior: SelfTradeBehavior,
}

//TODO make price FP32
//TODO add missing order types
//TODO cranking reward

struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    event_queue: &'a AccountInfo<'b>,
    bids: &'a AccountInfo<'b>,
    asks: &'a AccountInfo<'b>,
    admin: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();
        let a = Self {
            market: next_account_info(accounts_iter)?,
            event_queue: next_account_info(accounts_iter)?,
            bids: next_account_info(accounts_iter)?,
            asks: next_account_info(accounts_iter)?,
            admin: next_account_info(accounts_iter)?,
        };
        check_account_owner(a.market, program_id).unwrap();
        check_account_owner(a.event_queue, program_id).unwrap();
        check_account_owner(a.bids, program_id).unwrap();
        check_account_owner(a.asks, program_id).unwrap();
        check_signer(a.admin).unwrap();
        Ok(a)
    }
}

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], params: Params) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    let mut market_data: &[u8] = &accounts.market.data.borrow();
    let market_state = MarketState::deserialize(&mut market_data).unwrap();

    check_account_key(accounts.event_queue, &market_state.event_queue).unwrap();
    check_account_key(accounts.bids, &market_state.bids).unwrap();
    check_account_key(accounts.asks, &market_state.asks).unwrap();
    // check_account_key(accounts.authority, &market_state.caller_authority).unwrap();

    let callback_info_len = market_state.callback_info_len as usize;

    let mut order_book = OrderBookState {
        bids: Slab::new_from_acc_info(accounts.bids, callback_info_len),
        asks: Slab::new_from_acc_info(accounts.asks, callback_info_len),
        market_state,
    };

    if params.callback_info.len() != callback_info_len {
        msg!("Invalid callback information");
        return Err(ProgramError::InvalidArgument);
    }

    let header = {
        let mut event_queue_data: &[u8] =
            &accounts.event_queue.data.borrow()[0..EVENT_QUEUE_HEADER_LEN];
        EventQueueHeader::deserialize(&mut event_queue_data).unwrap()
    };
    let mut event_queue = EventQueue::new_safe(header, &accounts.event_queue, callback_info_len);

    //TODO loop
    let order_summary = order_book.new_order(params, &mut &mut event_queue)?;
    event_queue.write_to_register(order_summary);

    let mut event_queue_header_data: &mut [u8] = &mut accounts.event_queue.data.borrow_mut();
    event_queue
        .header
        .serialize(&mut event_queue_header_data)
        .unwrap();

    Ok(())
}
