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
