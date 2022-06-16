//! Cancel an existing order in the orderbook.

use bonfida_utils::fp_math::fp32_mul;
use bonfida_utils::{BorshSize, InstructionsAccount};
use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::Pod;
use solana_program::account_info::next_account_info;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::state::orderbook::{CallbackInfo, OrderBookState, OrderSummary};
use crate::state::AccountTag;
use crate::{
    error::AoError,
    state::{get_side_from_order_id, market_state::MarketState},
    utils::{check_account_key, check_account_owner},
};
#[derive(BorshDeserialize, BorshSerialize, Clone, BorshSize)]
/**
The required arguments for a cancel_order instruction.
*/
pub struct Params {
    /// The order id is a unique identifier for a particular order
    pub order_id: u128,
}

/// The required accounts for a cancel_order instruction.
#[derive(InstructionsAccount)]
pub struct Accounts<'a, T> {
    #[allow(missing_docs)]
    pub market: &'a T,
    #[allow(missing_docs)]
    #[cons(writable)]
    pub event_queue: &'a T,
    #[allow(missing_docs)]
    #[cons(writable)]
    pub bids: &'a T,
    #[allow(missing_docs)]
    #[cons(writable)]
    pub asks: &'a T,
}

impl<'a, 'b: 'a> Accounts<'a, AccountInfo<'b>> {
    pub(crate) fn parse(accounts: &'a [AccountInfo<'b>]) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();

        let a = Self {
            market: next_account_info(accounts_iter)?,
            event_queue: next_account_info(accounts_iter)?,
            bids: next_account_info(accounts_iter)?,
            asks: next_account_info(accounts_iter)?,
        };
        Ok(a)
    }

    /// Perform basic security checks on the accounts
    pub(crate) fn perform_checks(&self, program_id: &Pubkey) -> Result<(), ProgramError> {
        check_account_owner(
            self.market,
            &program_id.to_bytes(),
            AoError::WrongMarketOwner,
        )?;
        check_account_owner(
            self.event_queue,
            &program_id.to_bytes(),
            AoError::WrongEventQueueOwner,
        )?;
        check_account_owner(self.bids, &program_id.to_bytes(), AoError::WrongBidsOwner)?;
        check_account_owner(self.asks, &program_id.to_bytes(), AoError::WrongAsksOwner)?;
        Ok(())
    }
}
/// Apply the cancel_order instruction to the provided accounts
pub fn process<'a, 'b: 'a, C: CallbackInfo + Pod + PartialEq>(
    program_id: &Pubkey,
    accounts: Accounts<'a, AccountInfo<'b>>,
    params: Params,
) -> Result<OrderSummary, ProgramError>
where
    <C as CallbackInfo>::CallbackId: PartialEq,
{
    accounts.perform_checks(program_id)?;
    let mut market_state_data = accounts.market.data.borrow_mut();
    let market_state = MarketState::from_buffer(&mut market_state_data, AccountTag::Market)?;

    check_accounts(&accounts, market_state)?;

    let mut bids_guard = accounts.bids.data.borrow_mut();
    let mut asks_guard = accounts.asks.data.borrow_mut();

    let mut order_book = OrderBookState::<C>::new_safe(&mut bids_guard, &mut asks_guard)?;

    let slab = order_book.get_tree(get_side_from_order_id(params.order_id));
    let (leaf_node, _) = slab
        .remove_by_key(params.order_id)
        .ok_or(AoError::OrderNotFound)?;
    let total_base_qty = leaf_node.base_quantity;
    let total_quote_qty =
        fp32_mul(leaf_node.base_quantity, leaf_node.price()).ok_or(AoError::NumericalOverflow)?;

    let order_summary = OrderSummary {
        posted_order_id: None,
        total_base_qty,
        total_quote_qty,
        total_base_qty_posted: 0,
    };

    Ok(order_summary)
}

fn check_accounts<'a, 'b: 'a>(
    accounts: &Accounts<'a, AccountInfo<'b>>,
    market_state: &MarketState,
) -> ProgramResult {
    check_account_key(
        accounts.event_queue,
        &market_state.event_queue,
        AoError::WrongEventQueueAccount,
    )?;
    check_account_key(accounts.bids, &market_state.bids, AoError::WrongBidsAccount)?;
    check_account_key(accounts.asks, &market_state.asks, AoError::WrongAsksAccount)?;

    Ok(())
}
