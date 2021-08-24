use std::rc::Rc;

use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::{
    critbit::Slab,
    error::AOError,
    orderbook::{OrderBookState, OrderSummary},
    state::{
        Event, EventQueue, EventQueueHeader, MarketState, SelfTradeBehavior, Side,
        EVENT_QUEUE_HEADER_LEN,
    },
    utils::{check_account_key, check_account_owner, check_signer, fp32_mul},
};

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct Params {
    pub order_id: u128,
    pub side: Side,
}

struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    admin: &'a AccountInfo<'b>,
    asks: &'a AccountInfo<'b>,
    bids: &'a AccountInfo<'b>,
    event_queue: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();
        let market = next_account_info(accounts_iter)?;
        let event_queue = next_account_info(accounts_iter)?;
        let bids = next_account_info(accounts_iter)?;
        let asks = next_account_info(accounts_iter)?;
        let admin = next_account_info(accounts_iter)?;
        check_account_owner(market, program_id)?;
        check_account_owner(event_queue, program_id)?;
        check_account_owner(bids, program_id)?;
        check_account_owner(asks, program_id)?;
        //TODO check if caller auth signs?
        Ok(Self {
            market,
            admin,
            asks,
            bids,
            event_queue,
        })
    }
}

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], params: Params) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    let mut market_data: &[u8] = &accounts.market.data.borrow();
    let market_state = MarketState::deserialize(&mut market_data).unwrap();

    check_account_key(accounts.event_queue, &market_state.event_queue).unwrap();
    check_account_key(accounts.bids, &market_state.bids).unwrap();
    check_account_key(accounts.asks, &market_state.asks).unwrap();

    let callback_info_len = market_state.callback_info_len as usize;

    let mut order_book = OrderBookState {
        bids: Slab::new_from_acc_info(accounts.bids, callback_info_len),
        asks: Slab::new_from_acc_info(accounts.asks, callback_info_len),
        market_state,
    };

    let header = {
        let mut event_queue_data: &[u8] =
            &accounts.event_queue.data.borrow()[0..EVENT_QUEUE_HEADER_LEN];
        EventQueueHeader::deserialize(&mut event_queue_data).unwrap()
    };
    let event_queue = EventQueue::new_safe(header, &accounts.event_queue, callback_info_len);

    let slab = order_book.get_tree(params.side);
    let node = slab
        .remove_by_key(params.order_id)
        .ok_or(AOError::OrderNotFound)?;
    let leaf_node = node.as_leaf().unwrap();
    let total_asset_qty = leaf_node.asset_quantity;
    let total_quote_qty = fp32_mul(leaf_node.asset_quantity, leaf_node.price());

    let order_summary = OrderSummary {
        total_asset_qty,
        total_quote_qty,
    };

    event_queue.write_to_register(order_summary);

    let mut event_queue_header_data: &mut [u8] = &mut accounts.event_queue.data.borrow_mut();
    event_queue
        .header
        .serialize(&mut event_queue_header_data)
        .unwrap();
    order_book.commit_changes();

    Ok(())
}
