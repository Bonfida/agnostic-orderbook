use crate::{error::AoError, processor};
use borsh::BorshSerialize;
use num_traits::FromPrimitive;
use solana_program::{
    account_info::AccountInfo, decode_error::DecodeError, entrypoint::ProgramResult, msg,
    program_error::PrintProgramError, pubkey::Pubkey,
};

#[cfg(feature = "entrypoint")]
use solana_program::entrypoint;
#[cfg(feature = "entrypoint")]
entrypoint!(process_instruction);

/// The entrypoint to the test AAOB program
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    msg!("Entrypoint");
    let (register_account, accounts) = accounts.split_last().unwrap();
    match processor::process_instruction::<[u8; 32]>(program_id, accounts, instruction_data) {
        Err(error) => {
            // catch the error so we can print it
            error.print::<AoError>();
            return Err(error);
        }
        Ok(r) => {
            let mut a: &mut [u8] = &mut register_account.data.borrow_mut();
            r.serialize(&mut a).unwrap();
        }
    }
    Ok(())
}

impl PrintProgramError for AoError {
    fn print<E>(&self)
    where
        E: 'static + std::error::Error + DecodeError<E> + PrintProgramError + FromPrimitive,
    {
        msg!("Error: {}", self)
    }
}
