use std::cell::RefMut;

use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    log::sol_log_compute_units,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::{
    critbit::{LeafNode, NodeHandle, Slab},
    orderbook::OrderBookState,
    state::{MarketState, SelfTradeBehavior, Side},
};

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct NewOrderParams {
    pub max_base_qty: u64,
    pub max_quote_qty_locked: u64,
    pub limit_price: u64,
    pub owner: Pubkey,
    pub post_only: bool,
    pub post_allowed: u64,
    pub self_trade_behavior: SelfTradeBehavior,
}

struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    asks: &'a AccountInfo<'b>,
    bids: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();
        let market = next_account_info(accounts_iter)?;
        let asks = next_account_info(accounts_iter)?;
        let bids = next_account_info(accounts_iter)?;
        Ok(Self { market, asks, bids })
    }
}

pub fn process_new_order(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    params: NewOrderParams,
) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    let mut market_data: &[u8] = &accounts.market.data.borrow();
    let market_state = MarketState::deserialize(&mut market_data).unwrap();
    // let bids: RefMut<&[u8]> = accounts.bids.data.try_borrow_mut().unwrap();
    let mut order_book = OrderBookState {
        bids: &mut RefMut::map(accounts.bids.data.try_borrow_mut().unwrap(), |s| {
            Slab::new(*s)
        }),
        asks: &mut RefMut::map(accounts.asks.data.borrow_mut(), |s| Slab::new(*s)),
        market_state,
    };

    // Critbit test
    let leafnode = LeafNode {
        tag: 2,
        owner_slot: 0,
        fee_tier: 0,
        padding: [0, 0],
        key: 0,
        owner: [0, 0, 0, 0],
        quantity: 0,
        client_order_id: 0,
    };

    msg!("Pre insertion");
    sol_log_compute_units();
    order_book
        .orders_mut(Side::Bid)
        .insert_leaf(&leafnode)
        .unwrap();
    msg!("Post insertion");
    sol_log_compute_units();

    Ok(())
}
