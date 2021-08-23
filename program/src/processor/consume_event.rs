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
    pub number_of_entries_to_consume: u64,
}

struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    event_queue: &'a AccountInfo<'b>,
    authority: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let mut accounts_iter = accounts.iter();
        let a = Self {
            market: next_account_info(&mut accounts_iter)?,
            event_queue: next_account_info(&mut accounts_iter)?,
            authority: next_account_info(&mut accounts_iter)?,
        };
        check_account_owner(a.market, program_id).unwrap();
        check_account_owner(a.event_queue, program_id).unwrap();
        check_signer(a.authority).unwrap();
        Ok(a)
    }
}

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], params: Params) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    let mut market_data: &[u8] = &accounts.market.data.borrow();
    let market_state = MarketState::deserialize(&mut market_data).unwrap();

    check_account_key(accounts.event_queue, &market_state.event_queue).unwrap();
    check_account_key(accounts.authority, &market_state.caller_authority).unwrap();

    if &market_state.event_queue != accounts.event_queue.key {
        msg!("Invalid event queue for current market");
        return Err(ProgramError::InvalidArgument);
    }

    let header = {
        let mut event_queue_data: &[u8] =
            &accounts.event_queue.data.borrow()[0..EVENT_QUEUE_HEADER_LEN];
        EventQueueHeader::deserialize(&mut event_queue_data).unwrap()
    };
    let mut event_queue = EventQueue {
        header,
        buffer: Rc::clone(&accounts.event_queue.data),
        callback_info_len: market_state.callback_info_len as usize,
    };

    event_queue.pop_n(params.number_of_entries_to_consume);

    let mut event_queue_data: &mut [u8] = &mut accounts.event_queue.data.borrow_mut();
    event_queue.header.serialize(&mut event_queue_data).unwrap();

    Ok(())
}
