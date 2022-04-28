//! Create and initialize a new orderbook market
use bonfida_utils::{
    checks::check_rent_exempt,
    {BorshSize, InstructionsAccount},
};
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
    error::AoError,
    state::{AccountTag, EventQueue, EventQueueHeader, MarketState},
    utils::{check_account_owner, check_unitialized},
};

#[derive(BorshDeserialize, BorshSerialize, BorshSize)]
/**
The required arguments for a create_market instruction.
*/
pub struct Params {
    /// The caller authority will be the required signer for all market instructions.
    ///
    /// In practice, it will almost always be a program-derived address..
    pub caller_authority: [u8; 32],
    /// Callback information can be used by the caller to attach specific information to all orders.
    ///
    /// An example of this would be to store a public key to uniquely identify the owner of a particular order.
    /// This example would thus require a value of 32
    pub callback_info_len: u64,
    /// The prefix length of callback information which is used to identify self-trading
    pub callback_id_len: u64,
    /// The minimum order size that can be inserted into the orderbook after matching.
    pub min_base_order_size: u64,
    /// Enables the limiting of price precision on the orderbook (price ticks)
    pub tick_size: u64,
    /// Fixed fee for every new order operation. A higher fee increases incentives for cranking.
    pub cranker_reward: u64,
}

/// The required accounts for a create_market instruction.
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
            self.event_queue,
            &program_id.to_bytes(),
            AoError::WrongEventQueueOwner,
        )?;
        check_account_owner(self.bids, &program_id.to_bytes(), AoError::WrongBidsOwner)?;
        check_account_owner(self.asks, &program_id.to_bytes(), AoError::WrongAsksOwner)?;
        Ok(())
    }
}

/// Apply the create_market instruction to the provided accounts
pub fn process<'a, 'b: 'a>(
    program_id: &Pubkey,
    accounts: Accounts<'a, AccountInfo<'b>>,
    params: Params,
) -> ProgramResult {
    accounts.perform_checks(program_id)?;
    let Params {
        caller_authority,
        callback_info_len,
        callback_id_len,
        min_base_order_size,
        tick_size,
        cranker_reward,
    } = params;

    check_initialization(&accounts)?;
    check_rent(&accounts)?;

    if min_base_order_size == 0 {
        msg!("min_base_order_size must be > 0");
        return Err(ProgramError::InvalidArgument);
    }

    EventQueue::check_buffer_size(accounts.event_queue, params.callback_info_len)?;

    let mut market_state = MarketState::get_unchecked(accounts.market);

    *market_state = MarketState {
        tag: AccountTag::Market as u64,
        caller_authority,
        event_queue: accounts.event_queue.key.to_bytes(),
        bids: accounts.bids.key.to_bytes(),
        asks: accounts.asks.key.to_bytes(),
        callback_info_len,
        callback_id_len,
        fee_budget: 0,
        initial_lamports: accounts.market.lamports(),
        min_base_order_size,
        tick_size,
        cranker_reward,
    };

    let event_queue_header = EventQueueHeader::initialize(params.callback_info_len as usize);
    event_queue_header
        .serialize(&mut (&mut accounts.event_queue.data.borrow_mut() as &mut [u8]))
        .unwrap();

    Slab::initialize(
        &mut accounts.bids.data.borrow_mut(),
        &mut accounts.asks.data.borrow_mut(),
        *accounts.market.key,
        callback_info_len as usize,
    );

    Ok(())
}

fn check_initialization(accounts: &Accounts<AccountInfo>) -> ProgramResult {
    check_unitialized(accounts.event_queue)?;
    check_unitialized(accounts.bids)?;
    check_unitialized(accounts.asks)?;
    check_unitialized(accounts.market)?;

    Ok(())
}

fn check_rent(accounts: &Accounts<AccountInfo>) -> ProgramResult {
    check_rent_exempt(accounts.asks)?;
    check_rent_exempt(accounts.bids)?;
    check_rent_exempt(accounts.event_queue)?;
    check_rent_exempt(accounts.market)?;

    Ok(())
}
