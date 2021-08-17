use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::state::{AccountFlag, MarketState};

#[derive(BorshDeserialize, BorshSerialize)]
pub struct CreateMarketParams {
    pub caller_authority: Pubkey,
    pub event_queue: Pubkey,
    pub bids: Pubkey,
    pub asks: Pubkey,
}

struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    event_queue: &'a AccountInfo<'b>,
    asks: &'a AccountInfo<'b>,
    bids: &'a AccountInfo<'b>,
    market_authority: Option<&'a AccountInfo<'b>>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();
        let market = next_account_info(accounts_iter)?;
        let event_queue = next_account_info(accounts_iter)?;
        let bids = next_account_info(accounts_iter)?;
        let asks = next_account_info(accounts_iter)?;
        let market_authority = next_account_info(accounts_iter).ok();

        Ok(Self {
            market,
            event_queue,
            bids,
            asks,
            market_authority,
        })
    }
}

pub fn process_create_market(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    params: CreateMarketParams,
) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    let CreateMarketParams {
        caller_authority,
        event_queue,
        bids,
        asks,
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

    let market_state = MarketState {
        account_flags: AccountFlag::Market as u64,
        own_address: *accounts.market.key,
        caller_authority,
        event_queue,
        bids,
        asks,
        market_authority: match accounts.market_authority {
            Some(market_authority) => *market_authority.key,
            None => Pubkey::default(),
        },
    };

    let mut market_data: &mut [u8] = &mut accounts.market.data.borrow_mut();
    market_state.serialize(&mut market_data).unwrap();

    Ok(())
}