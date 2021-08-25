use num_derive::FromPrimitive;
use thiserror::Error;

use solana_program::{decode_error::DecodeError, program_error::ProgramError};

pub type AoResult<T = ()> = Result<T, AoError>;

//TODO clean-up
#[derive(Clone, Debug, Error, FromPrimitive)]
pub enum AoError {
    #[error("TODO Place-holder.")]
    InvalidMarketFlags,
    #[error("TODO Place-holder.")]
    InvalidAskFlags,
    #[error("TODO Place-holder.")]
    InvalidBidFlags,
    #[error("TODO Place-holder.")]
    InvalidQueueLength,
    #[error("TODO Place-holder.")]
    OwnerAccountNotProvided,

    #[error("TODO Place-holder.")]
    ConsumeEventsQueueFailure,

    #[error("TODO Place-holder.")]
    AlreadyInitialized,
    #[error("TODO Place-holder.")]
    WrongAccountDataAlignment,
    #[error("TODO Place-holder.")]
    WrongAccountDataPaddingLength,
    #[error("TODO Place-holder.")]
    WrongAccountHeadPadding,
    #[error("TODO Place-holder.")]
    WrongAccountTailPadding,

    #[error("TODO Place-holder.")]
    EventQueueEmpty,
    #[error("TODO Place-holder.")]
    EventQueueTooSmall,
    #[error("TODO Place-holder.")]
    SlabTooSmall,

    #[error("TODO Place-holder.")]
    SplAccountProgramId,
    #[error("TODO Place-holder.")]
    SplAccountLen,

    #[error("TODO Place-holder.")]
    BorrowError,

    #[error("TODO Place-holder.")]
    WrongBidsAccount,
    #[error("TODO Place-holder.")]
    WrongAsksAccount,
    #[error("TODO Place-holder.")]
    WrongEventQueueAccount,

    #[error("TODO Place-holder.")]
    EventQueueFull,
    #[error("TODO Place-holder.")]
    MarketIsDisabled,
    #[error("TODO Place-holder.")]
    WrongSigner,

    #[error("TODO Place-holder.")]
    WrongRentSysvarAccount,
    #[error("TODO Place-holder.")]
    RentNotProvided,
    #[error("TODO Place-holder.")]
    OrderNotFound,
    #[error("TODO Place-holder.")]
    OrderNotYours,

    #[error("TODO Place-holder.")]
    WouldSelfTrade,

    #[error("TODO Place-holder.")]
    AssertionError,
    #[error("TODO Place-holder.")]
    SlabOutOfSpace,
}

impl From<AoError> for ProgramError {
    fn from(e: AoError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl<T> DecodeError<T> for AoError {
    fn type_of() -> &'static str {
        "AOError"
    }
}
