//! Close an existing market.
use crate::{
    error::AoError,
    orderbook::OrderBookState,
    state::{AccountTag, EventQueueHeader, MarketState, EVENT_QUEUE_HEADER_LEN},
    utils::{check_account_key, check_account_owner, check_signer},
};
use bonfida_utils::{BorshSize, InstructionsAccount};
use borsh::{BorshDeserialize, BorshSerialize};
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
    #[cons(signer)]
    #[cfg(not(feature = "lib"))]
    pub authority: &'a T,
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
            #[cfg(not(feature = "lib"))]
            authority: next_account_info(accounts_iter)?,
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
        #[cfg(not(feature = "lib"))]
        check_signer(self.authority)?;
        Ok(())
    }
}
/// Apply the close_market instruction to the provided accounts
pub fn process<'a, 'b: 'a>(
    program_id: &Pubkey,
    accounts: Accounts<'a, AccountInfo<'b>>,
    _params: Params,
) -> ProgramResult {
    accounts.perform_checks(program_id)?;
    let mut market_state = MarketState::get(accounts.market)?;

    check_accounts(&accounts, &market_state)?;

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
    if header.count != 0 {
        msg!("The event queue needs to be empty");
        return Err(ProgramError::from(AoError::MarketStillActive));
    }

    market_state.tag = AccountTag::Uninitialized as u64;

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
    #[cfg(not(feature = "lib"))]
    check_account_key(
        accounts.authority,
        &market_state.caller_authority,
        AoError::WrongCallerAuthority,
    )?;

    Ok(())
}
