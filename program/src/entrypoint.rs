use crate::error::AOError;
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
        error.print::<AOError>();
        return Err(error);
    }
    Ok(())
}

impl PrintProgramError for AOError {
    fn print<E>(&self)
    where
        E: 'static + std::error::Error + DecodeError<E> + PrintProgramError + FromPrimitive,
    {
        match self {
            AOError::InvalidMarketFlags => todo!(),
            AOError::InvalidAskFlags => todo!(),
            AOError::InvalidBidFlags => todo!(),
            AOError::InvalidQueueLength => todo!(),
            AOError::OwnerAccountNotProvided => todo!(),
            AOError::ConsumeEventsQueueFailure => todo!(),
            AOError::AlreadyInitialized => todo!(),
            AOError::WrongAccountDataAlignment => todo!(),
            AOError::WrongAccountDataPaddingLength => todo!(),
            AOError::WrongAccountHeadPadding => todo!(),
            AOError::WrongAccountTailPadding => todo!(),
            AOError::RequestQueueEmpty => todo!(),
            AOError::EventQueueTooSmall => todo!(),
            AOError::SlabTooSmall => todo!(),
            AOError::SplAccountProgramId => todo!(),
            AOError::SplAccountLen => todo!(),
            AOError::BorrowError => todo!(),
            AOError::WrongBidsAccount => todo!(),
            AOError::WrongAsksAccount => todo!(),
            AOError::WrongEventQueueAccount => todo!(),
            AOError::EventQueueFull => todo!(),
            AOError::MarketIsDisabled => todo!(),
            AOError::WrongSigner => todo!(),
            AOError::WrongRentSysvarAccount => todo!(),
            AOError::RentNotProvided => todo!(),
            AOError::OrderNotFound => todo!(),
            AOError::OrderNotYours => todo!(),
            AOError::WouldSelfTrade => todo!(),
            AOError::AssertionError => todo!(),
            AOError::SlabOutOfSpace => todo!(),
        }
    }
}
