use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
};

use crate::error::{AoError, AoResult};

#[cfg(not(debug_assertions))]
#[inline(always)]
unsafe fn invariant(check: bool) {
    if check {
        std::hint::unreachable_unchecked();
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

/// a is fp0, b is fp32 and result is a/b fp0
pub(crate) fn fp32_div(a: u64, b_fp32: u64) -> u64 {
    (((a as u128) << 32) / (b_fp32 as u128)) as u64
}

/// a is fp0, b is fp32 and result is a*b fp0
pub(crate) fn fp32_mul(a: u64, b_fp32: u64) -> u64 {
    (((a as u128) * (b_fp32 as u128)) >> 32) as u64
}
