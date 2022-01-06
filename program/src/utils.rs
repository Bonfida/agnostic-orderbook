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

pub fn assert(statement: bool, err: AoError) -> Result<(), AoError> {
    if !statement {
        Err(err)
    } else {
        Ok(())
    }
}

// Safety verification functions
pub fn check_account_key(account: &AccountInfo, key: &[u8], error: AoError) -> Result<(), AoError> {
    if account.key.to_bytes() != key {
        return Err(error);
    }
    Ok(())
}

pub fn check_account_owner(
    account: &AccountInfo,
    owner: &[u8],
    error: AoError,
) -> Result<(), AoError> {
    if account.owner.to_bytes() != owner {
        return Err(error);
    }
    Ok(())
}

pub fn check_signer(account: &AccountInfo) -> ProgramResult {
    if !(account.is_signer) {
        return Err(ProgramError::MissingRequiredSignature);
    }
    Ok(())
}

pub fn check_unitialized(account: &AccountInfo) -> AoResult {
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

/// a is fp0, b is fp32 and result is a/b fp0
pub(crate) fn fp32_div(a: u64, b_fp32: u64) -> u64 {
    (((a as u128) << 32) / (b_fp32 as u128)) as u64
}

/// a is fp0, b is fp32 and result is a*b fp0
pub(crate) fn fp32_mul(a: u64, b_fp32: u64) -> u64 {
    (((a as u128) * (b_fp32 as u128)) >> 32) as u64
}

pub(crate) fn round_price(tick_size: u64, limit_price: u64, side: Side) -> u64 {
    match side {
        // Round down
        Side::Bid => tick_size * (limit_price / tick_size),
        // Round up
        Side::Ask => tick_size * ((limit_price + tick_size - 1) / tick_size),
    }
}
