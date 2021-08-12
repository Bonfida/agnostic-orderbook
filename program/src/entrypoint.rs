use crate::error::DexErrorCode;
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
        error.print::<DexErrorCode>();
        return Err(error);
    }
    Ok(())
}

impl PrintProgramError for DexErrorCode {
    fn print<E>(&self)
    where
        E: 'static + std::error::Error + DecodeError<E> + PrintProgramError + FromPrimitive,
    {
        match self {
            DexErrorCode::InvalidMarketFlags => todo!(),
            DexErrorCode::InvalidAskFlags => todo!(),
            DexErrorCode::InvalidBidFlags => todo!(),
            DexErrorCode::InvalidQueueLength => todo!(),
            DexErrorCode::OwnerAccountNotProvided => todo!(),
            DexErrorCode::ConsumeEventsQueueFailure => todo!(),
            DexErrorCode::AlreadyInitialized => todo!(),
            DexErrorCode::WrongAccountDataAlignment => todo!(),
            DexErrorCode::WrongAccountDataPaddingLength => todo!(),
            DexErrorCode::WrongAccountHeadPadding => todo!(),
            DexErrorCode::WrongAccountTailPadding => todo!(),
            DexErrorCode::RequestQueueEmpty => todo!(),
            DexErrorCode::EventQueueTooSmall => todo!(),
            DexErrorCode::SlabTooSmall => todo!(),
            DexErrorCode::SplAccountProgramId => todo!(),
            DexErrorCode::SplAccountLen => todo!(),
            DexErrorCode::BorrowError => todo!(),
            DexErrorCode::WrongBidsAccount => todo!(),
            DexErrorCode::WrongAsksAccount => todo!(),
            DexErrorCode::WrongEventQueueAccount => todo!(),
            DexErrorCode::EventQueueFull => todo!(),
            DexErrorCode::MarketIsDisabled => todo!(),
            DexErrorCode::WrongSigner => todo!(),
            DexErrorCode::WrongRentSysvarAccount => todo!(),
            DexErrorCode::RentNotProvided => todo!(),
            DexErrorCode::OrderNotFound => todo!(),
            DexErrorCode::OrderNotYours => todo!(),
            DexErrorCode::WouldSelfTrade => todo!(),
            DexErrorCode::AssertionError => todo!(),
            DexErrorCode::SlabOutOfSpace => todo!(),
        }
    }
}
