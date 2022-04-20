use num_derive::FromPrimitive;
use thiserror::Error;

use solana_program::{decode_error::DecodeError, program_error::ProgramError};

pub type AoResult<T = ()> = Result<T, AoError>;

//TODO clean-up
#[derive(Clone, Debug, Error, FromPrimitive)]
pub enum AoError {
    #[error("This account is already initialized")]
    AlreadyInitialized,
    #[error("An invalid bids account has been provided.")]
    WrongBidsAccount,
    #[error("An invalid asks account has been provided.")]
    WrongAsksAccount,
    #[error("An invalid event queue account has been provided.")]
    WrongEventQueueAccount,
    #[error("An invalid caller authority account has been provided.")]
    WrongCallerAuthority,
    #[error("The event queue is full.")]
    EventQueueFull,
    #[error("The order could not be found.")]
    OrderNotFound,
    #[error("The order would self trade.")]
    WouldSelfTrade,
    #[error("The market's memory is full.")]
    SlabOutOfSpace,
    #[error("The due fee was not payed.")]
    FeeNotPayed,
    #[error("This instruction is a No-op.")]
    NoOperations,
    #[error("The market is still active")]
    MarketStillActive,
    #[error("The base quantity must be > 0")]
    InvalidBaseQuantity,
    #[error("The event queue should be owned by the AO program")]
    WrongEventQueueOwner,
    #[error("The bids account should be owned by the AO program")]
    WrongBidsOwner,
    #[error("The asks account should be owned by the AO program")]
    WrongAsksOwner,
    #[error("The market account should be owned by the AO program")]
    WrongMarketOwner,
    #[error("The MSRM token account should be owned by the cranker")]
    WrongMsrmOwner,
    #[error("An invalid MSRM mint has been provided")]
    WrongMsrmMint,
    #[error("The MSRM token account does not have enough balances")]
    WrongMsrmBalance,
    #[error("Illegal MSRM token account owner")]
    IllegalMsrmOwner,
    #[error("Limit price must be a tick size multiple")]
    InvalidLimitPrice,
    #[error("Numerical overlflow")]
    NumericalOverflow,
    #[error("Invalid callback info")]
    InvalidCallbackInfo,
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
