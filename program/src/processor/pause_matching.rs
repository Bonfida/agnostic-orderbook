//! Close an existing market.
use crate::{
    error::AoError,
    state::{market_state::MarketState, orderbook::CallbackInfo, AccountTag},
    utils::check_account_owner,
};
use bonfida_utils::{BorshSize, InstructionsAccount};
use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::Pod;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
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
}

impl<'a, 'b: 'a> Accounts<'a, AccountInfo<'b>> {
    pub(crate) fn parse(accounts: &'a [AccountInfo<'b>]) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();

        let a = Self {
            market: next_account_info(accounts_iter)?,
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

    // bytemuck does not like boolean values
    market_state.pause_matching = 1;

    Ok(())
}
