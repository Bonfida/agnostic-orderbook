use solana_program::{decode_error::DecodeError, program_error::ProgramError};

pub type AOResult<T = ()> = Result<T, AOError>;

pub enum AOError {
    InvalidMarketFlags,
    InvalidAskFlags,
    InvalidBidFlags,
    InvalidQueueLength,
    OwnerAccountNotProvided,

    ConsumeEventsQueueFailure,

    AlreadyInitialized,
    WrongAccountDataAlignment,
    WrongAccountDataPaddingLength,
    WrongAccountHeadPadding,
    WrongAccountTailPadding,

    RequestQueueEmpty,
    EventQueueTooSmall,
    SlabTooSmall,

    SplAccountProgramId,
    SplAccountLen,

    BorrowError,

    WrongBidsAccount,
    WrongAsksAccount,
    WrongEventQueueAccount,

    EventQueueFull,
    MarketIsDisabled,
    WrongSigner,

    WrongRentSysvarAccount,
    RentNotProvided,
    OrderNotFound,
    OrderNotYours,

    WouldSelfTrade,

    AssertionError,
    SlabOutOfSpace,
}

impl From<AOError> for ProgramError {
    fn from(e: AOError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl<T> DecodeError<T> for AOError {
    fn type_of() -> &'static str {
        "AOError"
    }
}
