//! Cancels the most aggressive quotes in the book based on a callback identifier

use bonfida_utils::{BorshSize, InstructionsAccount};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::{
    error::AoError,
    orderbook::OrderBookState,
    state::{EventQueue, EventQueueHeader, MarketState, Side, EVENT_QUEUE_HEADER_LEN},
    utils::{assert, check_account_key, check_account_owner, check_signer},
};
#[derive(BorshDeserialize, BorshSerialize, Clone, BorshSize)]
/**
The required arguments for a mass_cancel_quotes instruction.
*/
pub struct Params {
    pub num_orders: u64,
    pub prioritized_side: Side,
    pub callback_id: Vec<u8>,
}

/// The required accounts for a cancel_order instruction.
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
    pub authority: &'a T,
}

impl<'a, 'b: 'a> Accounts<'a, AccountInfo<'b>> {
    pub(crate) fn parse(accounts: &'a [AccountInfo<'b>]) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();

        let a = Self {
            market: next_account_info(accounts_iter)?,
            event_queue: next_account_info(accounts_iter)?,
            bids: next_account_info(accounts_iter)?,
            asks: next_account_info(accounts_iter)?,
            authority: next_account_info(accounts_iter)?,
        };
        Ok(a)
    }

    /// Perform basic security checks on the accounts
    pub(crate) fn perform_checks(&self, program_id: &Pubkey) -> Result<(), ProgramError> {
        check_account_owner(
            self.market,
            &program_id.to_bytes(),
            AoError::WrongMarketOwner,
        )?;
        check_account_owner(
            self.event_queue,
            &program_id.to_bytes(),
            AoError::WrongEventQueueOwner,
        )?;
        check_account_owner(self.bids, &program_id.to_bytes(), AoError::WrongBidsOwner)?;
        check_account_owner(self.asks, &program_id.to_bytes(), AoError::WrongAsksOwner)?;
        #[cfg(not(feature = "lib"))]
        check_signer(self.authority)?;
        Ok(())
    }
}
/// Apply the mass_cancel_quotes instruction to the provided accounts
pub fn process<'a, 'b: 'a>(
    program_id: &Pubkey,
    accounts: Accounts<'a, AccountInfo<'b>>,
    params: Params,
) -> ProgramResult {
    accounts.perform_checks(program_id)?;
    let market_state = MarketState::get(accounts.market)?;

    check_accounts_and_params(&accounts, &params, &market_state)?;

    let callback_info_len = market_state.callback_info_len as usize;

    let mut order_book = OrderBookState::new_safe(
        accounts.bids,
        accounts.asks,
        market_state.callback_info_len as usize,
        market_state.callback_id_len as usize,
    )?;

    let header = {
        let mut event_queue_data: &[u8] =
            &accounts.event_queue.data.borrow()[0..EVENT_QUEUE_HEADER_LEN];
        EventQueueHeader::deserialize(&mut event_queue_data).unwrap()
    };
    let mut event_queue = EventQueue::new_safe(header, accounts.event_queue, callback_info_len)?;

    order_book.mass_cancel(&params, &mut event_queue)?;
    let mut event_queue_header_data: &mut [u8] = &mut accounts.event_queue.data.borrow_mut();
    event_queue
        .header
        .serialize(&mut event_queue_header_data)
        .unwrap();
    order_book.commit_changes();

    Ok(())
}

fn check_accounts_and_params<'a, 'b: 'a>(
    accounts: &Accounts<'a, AccountInfo<'b>>,
    params: &Params,
    market_state: &MarketState,
) -> ProgramResult {
    assert(
        market_state.callback_id_len as usize == params.callback_id.len(),
        AoError::InvalidCallbackIdLength,
    )?;
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
