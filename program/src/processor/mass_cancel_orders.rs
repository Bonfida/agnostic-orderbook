//! Cancel a series of existing orders in the orderbook.

use bonfida_utils::{BorshSize, InstructionsAccount};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::{
    error::AoError,
    orderbook::{OrderBookState, OrderSummary},
    state::{
        get_side_from_order_id, EventQueue, EventQueueHeader, MarketState, EVENT_QUEUE_HEADER_LEN,
    },
    utils::{check_account_key, check_account_owner, check_signer, fp32_mul},
};
#[derive(BorshDeserialize, BorshSerialize, Clone, BorshSize)]
/**
The required arguments for a cancel_order instruction.
*/
pub struct Params {
    /// The order id is a unique identifier for a particular order
    pub order_ids: Vec<u128>,
}

/// The required accounts for a cancel_order instruction.
#[derive(InstructionsAccount)]
pub struct Accounts<'a, T> {
    #[allow(missing_docs)]
    #[cons(writable)]
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
    #[allow(missing_docs)]
    #[cons(signer)]
    #[cfg(not(feature = "lib"))]
    pub authority: &'a T,
}

impl<'a, 'b: 'a> Accounts<'a, AccountInfo<'b>> {
    pub(crate) fn parse(accounts: &'a [AccountInfo<'b>]) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();

        let a = Self {
            market: next_account_info(accounts_iter)?,
            event_queue: next_account_info(accounts_iter)?,
            bids: next_account_info(accounts_iter)?,
            asks: next_account_info(accounts_iter)?,
            #[cfg(not(feature = "lib"))]
            authority: next_account_info(accounts_iter)?,
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
        #[cfg(not(feature = "lib"))]
        check_signer(self.authority)?;
        Ok(())
    }
}
/// Apply the cancel_order instruction to the provided accounts
pub fn process<'a, 'b: 'a>(
    program_id: &Pubkey,
    accounts: Accounts<'a, AccountInfo<'b>>,
    params: Params,
) -> ProgramResult {
    accounts.perform_checks(program_id)?;
    let market_state = MarketState::get(accounts.market)?;

    check_accounts(&accounts, &market_state)?;

    let callback_info_len = market_state.callback_info_len as usize;

    let mut order_book = OrderBookState::new_safe(
        accounts.bids,
        accounts.asks,
        market_state.callback_info_len as usize,
        market_state.callback_id_len as usize,
    )?;

    let header = {
        let mut event_queue_data: &[u8] =
            &accounts.event_queue.data.borrow()[0..EVENT_QUEUE_HEADER_LEN];
        EventQueueHeader::deserialize(&mut event_queue_data).unwrap()
    };
    let event_queue = EventQueue::new_safe(header, accounts.event_queue, callback_info_len)?;

    let mut total_base_qty = 0;
    let mut total_quote_qty = 0;

    for order_id in params.order_ids {
        let slab = order_book.get_tree(get_side_from_order_id(order_id));
        let (leaf_node, _) = slab.remove_by_key(order_id).ok_or(AoError::OrderNotFound)?;
        total_base_qty += leaf_node.base_quantity;
        total_quote_qty += fp32_mul(leaf_node.base_quantity, leaf_node.price());
    }

    let order_summary = OrderSummary {
        posted_order_id: None,
        total_base_qty,
        total_quote_qty,
        total_base_qty_posted: 0,
    };

    event_queue.write_to_register(order_summary);

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
    #[cfg(not(feature = "lib"))]
    check_account_key(
        accounts.authority,
        &market_state.caller_authority,
        AoError::WrongCallerAuthority,
    )?;

    Ok(())
}
