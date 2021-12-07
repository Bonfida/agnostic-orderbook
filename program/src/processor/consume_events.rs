use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::{
    error::AoError,
    state::{EventQueue, EventQueueHeader, MarketState, EVENT_QUEUE_HEADER_LEN},
    utils::{check_account_key, check_account_owner, check_signer},
};

#[derive(BorshDeserialize, BorshSerialize, Clone)]
/**
The required arguments for a consume_events instruction.
*/
pub struct Params {
    /// Depending on applications, it might be optimal to process several events at a time
    pub number_of_entries_to_consume: u64,
}

pub struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    event_queue: &'a AccountInfo<'b>,
    authority: &'a AccountInfo<'b>,
    reward_target: &'a AccountInfo<'b>,
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
            reward_target: next_account_info(&mut accounts_iter)?,
        };
        check_account_owner(a.market, &program_id.to_bytes(), AoError::WrongMarketOwner)?;
        check_account_owner(
            a.event_queue,
            &program_id.to_bytes(),
            AoError::WrongEventQueueOwner,
        )?;
        check_signer(a.authority).map_err(|e| {
            msg!("The market authority should be a signer for this instruction!");
            e
        })?;

        Ok(a)
    }
}

pub(crate) fn process(accounts: Accounts, params: Params) -> ProgramResult {
    let mut market_state = MarketState::get(&accounts.market)?;

    check_accounts(&accounts, &market_state)?;

    let header = {
        let mut event_queue_data: &[u8] =
            &accounts.event_queue.data.borrow()[0..EVENT_QUEUE_HEADER_LEN];
        EventQueueHeader::deserialize(&mut event_queue_data).unwrap()
    };
    let mut event_queue = EventQueue::new_safe(
        header,
        accounts.event_queue,
        market_state.callback_info_len as usize,
    )?;

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
    let mut event_queue_data: &mut [u8] = &mut accounts.event_queue.data.borrow_mut();
    event_queue.header.serialize(&mut event_queue_data).unwrap();

    msg!(
        "Number of events consumed: {:?}",
        capped_number_of_entries_consumed
    );

    Ok(())
}

fn check_accounts(accounts: &Accounts, market_state: &MarketState) -> ProgramResult {
    check_account_key(
        accounts.event_queue,
        &market_state.event_queue,
        AoError::WrongEventQueueAccount,
    )?;
    check_account_key(
        accounts.authority,
        &market_state.caller_authority,
        AoError::WrongCallerAuthority,
    )?;
    check_account_key(
        accounts.event_queue,
        &market_state.event_queue,
        AoError::WrongEventQueueAccount,
    )?;
    Ok(())
}
