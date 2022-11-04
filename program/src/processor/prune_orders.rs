//! Prune all the orders in the orderbook. Puts them on the event queue as cancelled orders.

use bonfida_utils::{BorshSize, InstructionsAccount};
use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::Pod;
use solana_program::account_info::next_account_info;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program_error::ProgramError,
    pubkey::Pubkey,
};
use std::cmp;

use crate::state::event_queue::EventQueue;
use crate::state::orderbook::{CallbackInfo, OrderBookState};
use crate::state::{AccountTag, Side};
use crate::{
    error::AoError,
    state::market_state::MarketState,
    utils::{check_account_key, check_account_owner},
};
#[derive(BorshDeserialize, BorshSerialize, Clone, BorshSize)]
/**
The required arguments for a prune_orders instruction.
*/
pub struct Params {
    /// Depending on available compute or space on the event queue, there may
    /// be a limit to the amount of orders that can be pruned in one transaction
    pub num_orders_to_prune: u64,
}

/// The required accounts for a prune_orders instruction.
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
/// Apply the prune_orders instruction to the provided accounts
pub fn process<'a, 'b: 'a, C: CallbackInfo + Pod + PartialEq>(
    program_id: &Pubkey,
    accounts: Accounts<'a, AccountInfo<'b>>,
    params: Params,
) -> ProgramResult
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

    let mut event_queue_guard = accounts.event_queue.data.borrow_mut();
    let mut event_queue = EventQueue::from_buffer(&mut event_queue_guard, AccountTag::EventQueue)?;
    let num_bids = u64::from(order_book.get_tree(Side::Bid).header.leaf_count);
    // Number of bids/asks to prune is bounded by: number of bids, param with max number of orders to prune
    let num_bids_to_prune = cmp::min(num_bids, params.num_orders_to_prune);
    order_book.prune_orders(num_bids_to_prune, Side::Bid, &mut event_queue)?;
    let remaining_num_orders = params
        .num_orders_to_prune
        .checked_sub(num_bids_to_prune)
        .unwrap();
    let num_asks = u64::from(order_book.get_tree(Side::Ask).header.leaf_count);
    let num_asks_to_prune = cmp::min(num_asks, remaining_num_orders);
    order_book.prune_orders(num_asks_to_prune, Side::Ask, &mut event_queue)?;

    msg!(
        "num bids pruned: {}, num asks pruned: {}",
        num_bids_to_prune,
        num_asks_to_prune
    );
    Ok(())
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
