//! The market state struct tracks metadata and security information about the agnostic orderbook system and its
//! relevant accounts
use bytemuck::{Pod, Zeroable};
use solana_program::{program_error::ProgramError, pubkey::Pubkey};
use std::mem::size_of;

pub use crate::state::orderbook::{OrderSummary, ORDER_SUMMARY_SIZE};
#[cfg(feature = "no-entrypoint")]
pub use crate::utils::get_spread;

use super::AccountTag;

#[derive(Debug, Copy, Clone, Pod, Zeroable)]
#[repr(C)]
/// The orderbook market's central state
pub struct MarketState {
    /// The public key of the orderbook's event queue account
    pub event_queue: Pubkey,
    /// The public key of the orderbook's bids account
    pub bids: Pubkey,
    /// The public key of the orderbook's asks account
    pub asks: Pubkey,
    /// The minimum order size that can be inserted into the orderbook after matching.
    pub min_base_order_size: u64,
    /// Tick size (FP32)
    pub tick_size: u64,
    /// When set to '1' orders will remain on their respective slabs without matching
    pub pause_matching: u8,
    _padding: [u8; 7],
}

impl MarketState {
    /// Expected size in bytes of MarketState
    pub const LEN: usize = size_of::<Self>();
    #[allow(missing_docs)]
    pub fn init_new(
        event_queue: &Pubkey,
        bids: &Pubkey,
        asks: &Pubkey,
        min_base_order_size: u64,
        tick_size: u64,
    ) -> Self {
        Self {
            event_queue: *event_queue,
            bids: *bids,
            asks: *asks,
            min_base_order_size,
            tick_size,
            pause_matching: 0,
            _padding: [0u8; 7],
        }
    }

    #[allow(missing_docs)]
    pub fn from_buffer(
        account_data: &mut [u8],
        expected_tag: AccountTag,
    ) -> Result<&mut Self, ProgramError> {
        let tag = bytemuck::from_bytes_mut::<u64>(&mut account_data[0..8]);
        if tag != &(expected_tag as u64) {
            return Err(ProgramError::InvalidAccountData);
        };
        *tag = AccountTag::Market as u64;

        let (_, data) = account_data.split_at_mut(8);

        Ok(bytemuck::from_bytes_mut(data))
    }
}

#[test]
fn market_cast() {
    let mut buffer = [0u8; MarketState::LEN + 8];
    let r = MarketState::from_buffer(&mut buffer, AccountTag::Market);
    assert!(r.is_err());
    assert_eq!(r.unwrap_err(), ProgramError::InvalidAccountData)
}
