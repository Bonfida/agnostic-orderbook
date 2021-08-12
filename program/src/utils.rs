use std::{cell::RefMut, mem::size_of};

use bytemuck::{cast_slice_mut, try_cast_slice_mut, try_from_bytes_mut, Pod};
use solana_program::account_info::AccountInfo;

use crate::error::DexResult;

#[cfg(debug_assertions)]
pub(crate) unsafe fn invariant(check: bool) {
    if check {
        unreachable!();
    }
}

#[cfg(not(debug_assertions))]
#[inline(always)]
unsafe fn invariant(check: bool) {
    if check {
        std::hint::unreachable_unchecked();
    }
}
