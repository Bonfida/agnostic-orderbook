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
    state::{AccountTag, EventQueueHeader, MarketState},
    utils::check_unitialized,
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
}

struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    event_queue: &'a AccountInfo<'b>,
    bids: &'a AccountInfo<'b>,
    asks: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        _program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();

        Ok(Self {
            market: next_account_info(accounts_iter)?,
            event_queue: next_account_info(accounts_iter)?,
            bids: next_account_info(accounts_iter)?,
            asks: next_account_info(accounts_iter)?,
        })
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
    } = params;

    if accounts.event_queue.owner != program_id {
        msg!("The event queue should be owned by the AO program");
        return Err(ProgramError::InvalidArgument);
    }
    if accounts.bids.owner != program_id {
        msg!("The bids account should be owned by the AO program");
        return Err(ProgramError::InvalidArgument);
    }
    if accounts.asks.owner != program_id {
        msg!("The asks account should be owned by the AO program");
        return Err(ProgramError::InvalidArgument);
    }

    check_unitialized(accounts.event_queue)?;
    check_unitialized(accounts.bids)?;
    check_unitialized(accounts.asks)?;
    check_unitialized(accounts.market)?;

    let market_state = MarketState {
        tag: AccountTag::Market,
        caller_authority,
        event_queue: *accounts.event_queue.key,
        bids: *accounts.bids.key,
        asks: *accounts.asks.key,
        callback_info_len,
        callback_id_len,
        fee_budget: 0,
        initial_lamports: accounts.market.lamports(),
    };

    let event_queue_header = EventQueueHeader::initialize(params.callback_info_len as usize);
    event_queue_header
        .serialize(&mut (&mut accounts.event_queue.data.borrow_mut() as &mut [u8]))
        .unwrap();

    Slab::initialize(accounts.bids, accounts.asks, *accounts.market.key);

    let mut market_data: &mut [u8] = &mut accounts.market.data.borrow_mut();
    market_state.serialize(&mut market_data).unwrap();

    Ok(())
}
