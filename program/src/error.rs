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
