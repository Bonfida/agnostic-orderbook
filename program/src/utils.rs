use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
};

use crate::{
    error::{AoError, AoResult},
    state::Side,
};

#[cfg(feature = "no-entrypoint")]
use crate::{orderbook::OrderBookState, state::MarketState};

#[cfg(not(debug_assertions))]
#[inline(always)]
unsafe fn invariant(check: bool) {
    if check {
        std::hint::unreachable_unchecked();
    }
}

// Safety verification functions
pub(crate) fn check_account_key(
    account: &AccountInfo,
    key: &[u8],
    error: AoError,
) -> Result<(), AoError> {
    if account.key.to_bytes() != key {
        return Err(error);
    }
    Ok(())
}

pub(crate) fn check_account_owner(
    account: &AccountInfo,
    owner: &[u8],
    error: AoError,
) -> Result<(), AoError> {
    if account.owner.to_bytes() != owner {
        return Err(error);
    }
    Ok(())
}

pub(crate) fn check_signer(account: &AccountInfo) -> ProgramResult {
    if !(account.is_signer) {
        return Err(ProgramError::MissingRequiredSignature);
    }
    Ok(())
}

pub(crate) fn check_unitialized(account: &AccountInfo) -> AoResult {
    if account.data.borrow()[0] != 0 {
        return Err(AoError::AlreadyInitialized);
    }
    Ok(())
}

#[cfg(feature = "no-entrypoint")]
/// This util is used to return the orderbook's spread (best_bid_price, best_ask_price) with both values in FP32 format
pub fn get_spread<'ob>(
    market_state_account: &AccountInfo<'ob>,
    bids_account: &AccountInfo<'ob>,
    asks_account: &AccountInfo<'ob>,
) -> (Option<u64>, Option<u64>) {
    let market_state = MarketState::get(market_state_account).unwrap();
    let orderbook = OrderBookState::new_safe(
        bids_account,
        asks_account,
        market_state.callback_info_len as usize,
        market_state.callback_id_len as usize,
    )
    .unwrap();
    orderbook.get_spread()
}

/// Rounds a given price the nearest tick size according to the rules of the AOB
pub fn round_price(tick_size: u64, limit_price: u64, side: Side) -> u64 {
    match side {
        // Round down
        Side::Bid => tick_size * (limit_price / tick_size),
        // Round up
        Side::Ask => tick_size * ((limit_price + tick_size - 1) / tick_size),
    }
}
