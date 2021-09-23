use crate::{
    error::AoError,
    orderbook::OrderBookState,
    state::{AccountTag, EventQueue, EventQueueHeader, MarketState, EVENT_QUEUE_HEADER_LEN},
    utils::{check_account_key, check_account_owner, check_signer},
};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};

#[derive(BorshDeserialize, BorshSerialize)]
/**
The required arguments for a close_market instruction.
*/
pub struct Params {}

struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    event_queue: &'a AccountInfo<'b>,
    bids: &'a AccountInfo<'b>,
    asks: &'a AccountInfo<'b>,
    authority: &'a AccountInfo<'b>,
    lamports_target_account: &'a AccountInfo<'b>,
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
            authority: next_account_info(accounts_iter)?,
            lamports_target_account: next_account_info(accounts_iter)?,
        };
        check_account_owner(a.market, program_id)?;
        check_signer(a.authority)?;
        Ok(a)
    }
}

pub(crate) fn process(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    let mut market_state = {
        let mut market_data: &[u8] = &accounts.market.data.borrow();
        MarketState::deserialize(&mut market_data)
            .unwrap()
            .check()?
    };

    check_account_key(accounts.event_queue, &market_state.event_queue)
        .map_err(|_| AoError::WrongEventQueueAccount)?;
    check_account_key(accounts.bids, &market_state.bids).map_err(|_| AoError::WrongBidsAccount)?;
    check_account_key(accounts.asks, &market_state.asks).map_err(|_| AoError::WrongAsksAccount)?;
    check_account_key(accounts.authority, &market_state.caller_authority)
        .map_err(|_| AoError::WrongCallerAuthority)?;

    // Check if there are still orders in the book
    let orderbook_state = OrderBookState::new_safe(
        accounts.bids,
        accounts.asks,
        market_state.callback_info_len as usize,
        market_state.callback_id_len as usize,
    )
    .unwrap();
    if !orderbook_state.is_empty() {
        msg!("The orderbook must be empty");
        return Err(ProgramError::from(AoError::MarketStillActive));
    }

    // Check if all events have been processed
    let header = {
        let mut event_queue_data: &[u8] =
            &accounts.event_queue.data.borrow()[0..EVENT_QUEUE_HEADER_LEN];
        EventQueueHeader::deserialize(&mut event_queue_data).unwrap()
    };
    let event_queue = EventQueue::new_safe(
        header,
        accounts.event_queue,
        market_state.callback_info_len as usize,
    )?;
    if event_queue.header.count != 0 {
        msg!("The event queue needs to be empty");
        return Err(ProgramError::from(AoError::MarketStillActive));
    }

    market_state.tag = AccountTag::Uninitialized;
    let mut market_state_data: &mut [u8] = &mut accounts.market.data.borrow_mut();
    market_state.serialize(&mut market_state_data).unwrap();

    let mut market_lamports = accounts.market.lamports.borrow_mut();
    let mut bids_lamports = accounts.bids.lamports.borrow_mut();
    let mut asks_lamports = accounts.asks.lamports.borrow_mut();
    let mut event_queue_lamports = accounts.event_queue.lamports.borrow_mut();

    let mut target_lamports = accounts.lamports_target_account.lamports.borrow_mut();

    **target_lamports +=
        **market_lamports + **bids_lamports + **asks_lamports + **event_queue_lamports;

    **market_lamports = 0;
    **bids_lamports = 0;
    **asks_lamports = 0;
    **event_queue_lamports = 0;

    Ok(())
}
