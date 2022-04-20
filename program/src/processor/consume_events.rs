//! Pop a series of events off the event queue.
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
        event_queue::EventQueue, market_state::MarketState, orderbook::CallbackInfo, AccountTag,
    },
    utils::{check_account_key, check_account_owner},
};

#[derive(BorshDeserialize, BorshSerialize, Clone, BorshSize)]
/**
The required arguments for a consume_events instruction.
*/
pub struct Params {
    /// Depending on applications, it might be optimal to process several events at a time
    pub number_of_entries_to_consume: u64,
}

/// The required accounts for a consume_events instruction.
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
    pub reward_target: &'a T,
}

impl<'a, 'b: 'a> Accounts<'a, AccountInfo<'b>> {
    pub(crate) fn parse(accounts: &'a [AccountInfo<'b>]) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();

        let a = Self {
            market: next_account_info(accounts_iter)?,
            event_queue: next_account_info(accounts_iter)?,
            reward_target: next_account_info(accounts_iter)?,
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

        Ok(())
    }
}

/// Apply the consume_events instruction to the provided accounts
pub fn process<'a, 'b: 'a, C: CallbackInfo + Pod + PartialEq>(
    program_id: &Pubkey,
    accounts: Accounts<'a, AccountInfo<'b>>,
    params: Params,
) -> ProgramResult {
    accounts.perform_checks(program_id)?;
    let mut market_data = accounts.market.data.borrow_mut();
    let mut market_state = MarketState::from_buffer(&mut market_data, AccountTag::Market)?;

    check_accounts(&accounts, market_state)?;
    let mut event_queue_guard = accounts.event_queue.data.borrow_mut();
    let mut event_queue =
        EventQueue::<C>::from_buffer(&mut event_queue_guard, AccountTag::EventQueue)?;

    // Reward payout
    let capped_number_of_entries_consumed = std::cmp::min(
        event_queue.header.count,
        params.number_of_entries_to_consume,
    );
    let reward = (market_state.fee_budget * capped_number_of_entries_consumed)
        .checked_div(event_queue.header.count)
        .ok_or(AoError::NoOperations)
        .unwrap();
    market_state.fee_budget -= reward;
    **accounts.market.try_borrow_mut_lamports().unwrap() = accounts.market.lamports() - reward;
    **accounts.reward_target.try_borrow_mut_lamports().unwrap() =
        accounts.reward_target.lamports() + reward;

    // Pop Events
    event_queue.pop_n(params.number_of_entries_to_consume);

    msg!(
        "Number of events consumed: {:?}",
        capped_number_of_entries_consumed
    );

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
    check_account_key(
        accounts.event_queue,
        &market_state.event_queue,
        AoError::WrongEventQueueAccount,
    )?;
    Ok(())
}
