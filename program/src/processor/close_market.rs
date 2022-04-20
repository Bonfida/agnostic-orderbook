//! Close an existing market.
use crate::{
    error::AoError,
    state::{
        event_queue::EventQueue,
        market_state::MarketState,
        orderbook::{CallbackInfo, OrderBookState},
        AccountTag,
    },
    utils::{check_account_key, check_account_owner},
};
use bonfida_utils::{BorshSize, InstructionsAccount};
use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::Pod;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};

#[derive(BorshDeserialize, BorshSerialize, BorshSize)]
/**
The required arguments for a close_market instruction.
*/
pub struct Params {}

/// The required accounts for a close_market instruction.
#[derive(InstructionsAccount)]
pub struct Accounts<'a, T> {
    #[allow(missing_docs)]
    #[cons(writable)]
    pub market: &'a T,
    #[allow(missing_docs)]
    #[cons(writable)]
    pub event_queue: &'a T,
    #[allow(missing_docs)]
    #[cons(writable)]
    pub bids: &'a T,
    #[allow(missing_docs)]
    #[cons(writable)]
    pub asks: &'a T,
    #[allow(missing_docs)]
    #[cons(writable)]
    pub lamports_target_account: &'a T,
}

impl<'a, 'b: 'a> Accounts<'a, AccountInfo<'b>> {
    pub(crate) fn parse(accounts: &'a [AccountInfo<'b>]) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();

        let a = Self {
            market: next_account_info(accounts_iter)?,
            event_queue: next_account_info(accounts_iter)?,
            bids: next_account_info(accounts_iter)?,
            asks: next_account_info(accounts_iter)?,
            lamports_target_account: next_account_info(accounts_iter)?,
        };
        Ok(a)
    }

    pub(crate) fn perform_checks(&self, program_id: &Pubkey) -> Result<(), ProgramError> {
        check_account_owner(
            self.market,
            &program_id.to_bytes(),
            AoError::WrongMarketOwner,
        )?;
        Ok(())
    }
}
/// Apply the close_market instruction to the provided accounts
pub fn process<'a, 'b: 'a, C: CallbackInfo + PartialEq + Pod>(
    program_id: &Pubkey,
    accounts: Accounts<'a, AccountInfo<'b>>,
    _params: Params,
) -> ProgramResult
where
    <C as CallbackInfo>::CallbackId: PartialEq,
{
    accounts.perform_checks(program_id)?;
    let mut market_data = accounts.market.data.borrow_mut();
    let market_state = MarketState::from_buffer(&mut market_data, AccountTag::Market)?;

    check_accounts(&accounts, market_state)?;

    let mut bids_guard = accounts.bids.data.borrow_mut();
    let mut asks_guard = accounts.asks.data.borrow_mut();

    // Check if there are still orders in the book
    let orderbook_state = OrderBookState::<C>::new_safe(&mut bids_guard, &mut asks_guard).unwrap();
    if !orderbook_state.is_empty() {
        msg!("The orderbook must be empty");
        return Err(ProgramError::from(AoError::MarketStillActive));
    }

    let mut event_queue_data = accounts.event_queue.data.borrow_mut();

    // Check if all events have been processed
    let event_queue = EventQueue::<C>::from_buffer(&mut event_queue_data, AccountTag::EventQueue)?;
    if event_queue.header.count != 0 {
        msg!("The event queue needs to be empty");
        return Err(ProgramError::from(AoError::MarketStillActive));
    }

    *bytemuck::from_bytes_mut(&mut market_data[0..8]) = AccountTag::Uninitialized as u64;

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

fn check_accounts<'a, 'b: 'a>(
    accounts: &Accounts<'a, AccountInfo<'b>>,
    market_state: &MarketState,
) -> ProgramResult {
    check_account_key(
        accounts.event_queue,
        &market_state.event_queue,
        AoError::WrongEventQueueAccount,
    )?;
    check_account_key(accounts.bids, &market_state.bids, AoError::WrongBidsAccount)?;
    check_account_key(accounts.asks, &market_state.asks, AoError::WrongAsksAccount)?;

    Ok(())
}
