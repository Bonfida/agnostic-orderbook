//! Execute a new order on the orderbook

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

#[derive(BorshDeserialize, BorshSerialize, BorshSize)]
/**
The required arguments for a new_order instruction.
*/
pub struct Params {}

/// The required accounts for a new_order instruction.
#[derive(InstructionsAccount)]
pub struct Accounts<'a, T> {
    #[allow(missing_docs)]
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
}

impl<'a, 'b: 'a> Accounts<'a, AccountInfo<'b>> {
    pub(crate) fn parse(accounts: &'a [AccountInfo<'b>]) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();

        let a = Self {
            market: next_account_info(accounts_iter)?,
            event_queue: next_account_info(accounts_iter)?,
            bids: next_account_info(accounts_iter)?,
            asks: next_account_info(accounts_iter)?,
        };
        Ok(a)
    }
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
        Ok(())
    }
}

/// Apply the new_order instruction to the provided accounts
pub fn process<'a, 'b: 'a, C: Pod + CallbackInfo + PartialEq>(
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

    let mut order_book = OrderBookState::<C>::new_safe(&mut bids_guard, &mut asks_guard)?;

    let mut event_queue_guard = accounts.event_queue.data.borrow_mut();
    let mut event_queue =
        EventQueue::<C>::from_buffer(&mut event_queue_guard, AccountTag::EventQueue)?;

    match order_book.match_existing_orders(&mut event_queue, market_state.min_base_order_size) {
        Ok(completed) => {
            if completed {
                msg!("All sitting orders have been filled. Regular mathcing behavior resumed.");
                market_state.pause_matching = 1;
            } else {
                msg!("Unmatched orders remain. Please run this instruction again to clear them before resuming regular behavior.");
            }

            Ok(())
        }
        Err(e) => Err(e.into()),
    }
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
