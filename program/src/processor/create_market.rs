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
    state::{AccountTag, EventQueueHeader, MarketState},
    utils::{check_account_owner, check_unitialized},
};

#[derive(BorshDeserialize, BorshSerialize)]
/**
The required arguments for a create_market instruction.
*/
pub struct Params {
    /// The caller authority will be the required signer for all market instructions.
    ///
    /// In practice, it will almost always be a program-derived address..
    pub caller_authority: Pubkey,
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
    pub price_bitmask: u64,
}

struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    event_queue: &'a AccountInfo<'b>,
    bids: &'a AccountInfo<'b>,
    asks: &'a AccountInfo<'b>,
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
        };

        check_account_owner(
            a.event_queue,
            &program_id.to_bytes(),
            AoError::WrongEventQueueOwner,
        )?;
        check_account_owner(a.bids, &program_id.to_bytes(), AoError::WrongBidsOwner)?;
        check_account_owner(a.asks, &program_id.to_bytes(), AoError::WrongAsksOwner)?;

        Ok(a)
    }
}

pub(crate) fn process(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    params: Params,
) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    let Params {
        caller_authority,
        callback_info_len,
        callback_id_len,
        min_base_order_size,
        price_bitmask,
    } = params;

    check_unitialized(accounts.event_queue)?;
    check_unitialized(accounts.bids)?;
    check_unitialized(accounts.asks)?;
    check_unitialized(accounts.market)?;

    let mut market_state = MarketState::get_unchecked(accounts.market);
    // Checks that the bitmask is of the form 1111...11100...00 (all ones then all zeros)
    if price_bitmask.leading_ones() + price_bitmask.trailing_zeros() != 64 {
        msg!("The provided bitmask is invalid");
        return Err(ProgramError::InvalidArgument);
    }

    *market_state = MarketState {
        tag: AccountTag::Market as u64,
        caller_authority: caller_authority.to_bytes(),
        event_queue: accounts.event_queue.key.to_bytes(),
        bids: accounts.bids.key.to_bytes(),
        asks: accounts.asks.key.to_bytes(),
        callback_info_len,
        callback_id_len,
        fee_budget: 0,
        initial_lamports: accounts.market.lamports(),
        min_base_order_size,
        price_bitmask,
    };

    let event_queue_header = EventQueueHeader::initialize(params.callback_info_len as usize);
    event_queue_header
        .serialize(&mut (&mut accounts.event_queue.data.borrow_mut() as &mut [u8]))
        .unwrap();

    Slab::initialize(
        accounts.bids,
        accounts.asks,
        *accounts.market.key,
        callback_info_len as usize,
    );

    Ok(())
}
