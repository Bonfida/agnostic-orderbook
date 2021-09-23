use crate::error::AoError;
use crate::processor::Processor;
use num_traits::FromPrimitive;
use solana_program::{
    account_info::AccountInfo, decode_error::DecodeError, entrypoint::ProgramResult, msg,
    program_error::PrintProgramError, pubkey::Pubkey,
};

#[cfg(not(feature = "no-entrypoint"))]
use solana_program::entrypoint;
#[cfg(not(feature = "no-entrypoint"))]
entrypoint!(process_instruction);

/// The entrypoint to the AAOB program
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    msg!("Entrypoint");
    if let Err(error) = Processor::process_instruction(program_id, accounts, instruction_data) {
        // catch the error so we can print it
        error.print::<AoError>();
        return Err(error);
    }
    Ok(())
}

impl PrintProgramError for AoError {
    fn print<E>(&self)
    where
        E: 'static + std::error::Error + DecodeError<E> + PrintProgramError + FromPrimitive,
    {
        match self {
            AoError::AlreadyInitialized => msg!("Error: This account is already initialized"),
            AoError::WrongBidsAccount => msg!("Error: An invalid bids account has been provided."),
            AoError::WrongAsksAccount => msg!("Error: An invalid asks account has been provided."),
            AoError::WrongEventQueueAccount => {
                msg!("Error: An invalid event queue account has been provided.")
            }
            AoError::WrongCallerAuthority => {
                msg!("Error: An invalid caller authority account has been provided.")
            }
            AoError::EventQueueFull => msg!("Error: The event queue is full. "),
            AoError::OrderNotFound => msg!("Error: The order could not be found."),
            AoError::WouldSelfTrade => msg!("Error: The order would self trade."),
            AoError::SlabOutOfSpace => msg!("Error: The market's memory is full."),
            AoError::FeeNotPayed => msg!("Error: The fee was not correctly payed."),
            AoError::NoOperations => msg!("Error: This instruction is a No-op."),
            AoError::MarketStillActive => msg!("Error: The market is still active"),
        }
    }
}
