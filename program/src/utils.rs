use crate::{
    error::{AoError, AoResult},
    state::Side,
};

use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use crate::state::orderbook::{CallbackInfo, OrderBookState};

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
    key: &Pubkey,
    error: AoError,
) -> Result<(), AoError> {
    if account.key != key {
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

pub(crate) fn check_unitialized(account: &AccountInfo) -> AoResult {
    if account.data.borrow()[0] != 0 {
        return Err(AoError::AlreadyInitialized);
    }
    Ok(())
}

/// This util is used to return the orderbook's spread (best_bid_price, best_ask_price) with both values in FP32 format
pub fn get_spread<'ob, 'b: 'ob, C: CallbackInfo + PartialEq>(
    bids_account: &'ob AccountInfo<'b>,
    asks_account: &'ob AccountInfo<'b>,
) -> (Option<u64>, Option<u64>)
where
    <C as CallbackInfo>::CallbackId: PartialEq,
{
    let mut bids = bids_account.data.borrow_mut();
    let mut asks = asks_account.data.borrow_mut();

    let orderbook = OrderBookState::<C>::new_safe(&mut bids, &mut asks).unwrap();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_price() {
        let price = (5.01 * 2.0f64.powi(32)) as u64;
        let tick_size = (0.5 * 2.0f64.powi(32)) as u64;
        let rounded_price_bid = round_price(tick_size, price, Side::Bid);
        assert_eq!(rounded_price_bid, 5 << 32);

        let rounded_price_ask = round_price(tick_size, price, Side::Ask);
        assert_eq!(rounded_price_ask, (5.5 * 2.0f64.powi(32)) as u64);
    }
}
