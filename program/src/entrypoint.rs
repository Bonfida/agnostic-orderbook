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

// TODO: cleanup
impl PrintProgramError for AoError {
    fn print<E>(&self)
    where
        E: 'static + std::error::Error + DecodeError<E> + PrintProgramError + FromPrimitive,
    {
        match self {
            AoError::InvalidMarketFlags => todo!(),
            AoError::InvalidAskFlags => todo!(),
            AoError::InvalidBidFlags => todo!(),
            AoError::InvalidQueueLength => todo!(),
            AoError::OwnerAccountNotProvided => todo!(),
            AoError::ConsumeEventsQueueFailure => todo!(),
            AoError::AlreadyInitialized => todo!(),
            AoError::WrongAccountDataAlignment => todo!(),
            AoError::WrongAccountDataPaddingLength => todo!(),
            AoError::WrongAccountHeadPadding => todo!(),
            AoError::WrongAccountTailPadding => todo!(),
            AoError::EventQueueEmpty => todo!(),
            AoError::EventQueueTooSmall => todo!(),
            AoError::SlabTooSmall => todo!(),
            AoError::SplAccountProgramId => todo!(),
            AoError::SplAccountLen => todo!(),
            AoError::BorrowError => todo!(),
            AoError::WrongBidsAccount => todo!(),
            AoError::WrongAsksAccount => todo!(),
            AoError::WrongEventQueueAccount => todo!(),
            AoError::EventQueueFull => todo!(),
            AoError::MarketIsDisabled => todo!(),
            AoError::WrongSigner => todo!(),
            AoError::WrongRentSysvarAccount => todo!(),
            AoError::RentNotProvided => todo!(),
            AoError::OrderNotFound => todo!(),
            AoError::OrderNotYours => todo!(),
            AoError::WouldSelfTrade => todo!(),
            AoError::AssertionError => todo!(),
            AoError::SlabOutOfSpace => todo!(),
        }
    }
}
