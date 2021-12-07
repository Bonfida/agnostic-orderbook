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
#[derive(BorshDeserialize, BorshSerialize, Clone)]
/**
The required arguments for a cancel_order instruction.
*/
pub struct Params {
    /// The order id is a unique identifier for a particular order
    pub order_id: u128,
}

pub struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    event_queue: &'a AccountInfo<'b>,
    bids: &'a AccountInfo<'b>,
    asks: &'a AccountInfo<'b>,
    authority: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();

        let a = Self {
            market: next_account_info(accounts_iter)?,
            event_queue: next_account_info(accounts_iter)?,
            bids: next_account_info(accounts_iter)?,
            asks: next_account_info(accounts_iter)?,
            authority: next_account_info(accounts_iter)?,
        };
        check_account_owner(a.market, &program_id.to_bytes(), AoError::WrongMarketOwner)?;
        check_account_owner(
            a.event_queue,
            &program_id.to_bytes(),
            AoError::WrongEventQueueOwner,
        )?;
        check_account_owner(a.bids, &program_id.to_bytes(), AoError::WrongBidsOwner)?;
        check_account_owner(a.asks, &program_id.to_bytes(), AoError::WrongAsksOwner)?;
        check_signer(a.authority)?;
        Ok(a)
    }
}

pub(crate) fn process(accounts: Accounts, params: Params) -> ProgramResult {
    let market_state = MarketState::get(&accounts.market)?;

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
    let event_queue = EventQueue::new_safe(header, &accounts.event_queue, callback_info_len)?;

    let slab = order_book.get_tree(get_side_from_order_id(params.order_id));
    let node = slab
        .remove_by_key(params.order_id)
        .ok_or(AoError::OrderNotFound)?;
    let leaf_node = node.as_leaf().unwrap();
    let total_base_qty = leaf_node.base_quantity;
    let total_quote_qty = fp32_mul(leaf_node.base_quantity, leaf_node.price());

    let order_summary = OrderSummary {
        posted_order_id: None,
        total_base_qty,
        total_quote_qty,
        total_base_qty_posted: 0,
    };

    event_queue.write_to_register(order_summary);

    order_book.commit_changes();

    Ok(())
}

fn check_accounts(accounts: &Accounts, market_state: &MarketState) -> ProgramResult {
    check_account_key(
        accounts.event_queue,
        &market_state.event_queue,
        AoError::WrongEventQueueAccount,
    )?;
    check_account_key(accounts.bids, &market_state.bids, AoError::WrongBidsAccount)?;
    check_account_key(accounts.asks, &market_state.asks, AoError::WrongAsksAccount)?;
    check_account_key(
        accounts.authority,
        &market_state.caller_authority,
        AoError::WrongCallerAuthority,
    )?;

    Ok(())
}
