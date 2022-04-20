#![cfg(feature = "benchmarking")]
use std::cell::RefCell;

use agnostic_orderbook::{
    critbit::{Slab, SlabRef},
    instruction::{cancel_order, new_order},
    orderbook::OrderBookState,
    state::{
        AccountTag, EventQueue, EventQueueHeader, MarketState, SelfTradeBehavior, Side,
        MARKET_STATE_LEN,
    },
};
use bonfida_utils::bench::get_env_arg;
use borsh::BorshSerialize;
use solana_program::pubkey::Pubkey;
use solana_program_test::{processor, ProgramTest};
use solana_sdk::{account::Account, signature::Keypair, signer::Signer};
pub mod common;
use crate::common::utils::sign_send_instructions;

#[tokio::test]
async fn main() {
    let program_test = prepare().await;
    run(program_test).await;
}

pub struct Context {
    test_context: ProgramTest,
    caller_authority: Keypair,
    test_order_id: u128,
    market: Pubkey,
    event_queue: Pubkey,
    bids: Pubkey,
    asks: Pubkey,
}

async fn run(ctx: Context) {
    let Context {
        test_context,
        caller_authority,
        test_order_id,
        market,
        event_queue,
        bids,
        asks,
    } = ctx;
    let mut ctx = test_context.start_with_context().await;
    let instruction = cancel_order(
        cancel_order::Accounts {
            market: &market,
            event_queue: &event_queue,
            bids: &bids,
            asks: &asks,
            authority: &caller_authority.pubkey(),
        },
        cancel_order::Params {
            order_id: test_order_id,
        },
    );
    sign_send_instructions(&mut ctx, vec![instruction], vec![&caller_authority])
        .await
        .unwrap()
}

async fn prepare() -> Context {
    let order_capacity = get_env_arg(0).unwrap_or(1_000);
    let callback_info_len = 32;
    let callback_id_len = 32;
    let market_key = Pubkey::new_unique();
    let event_queue_key = Pubkey::new_unique();
    let bids_key = Pubkey::new_unique();
    let asks_key = Pubkey::new_unique();
    let caller_authority = Keypair::new();
    // Initialize the event queue
    let mut event_queue_buffer = (0..EventQueue::compute_allocation_size(10, callback_info_len))
        .map(|_| 0u8)
        .collect::<Vec<_>>();
    let event_queue_header = EventQueueHeader::initialize(callback_info_len);
    event_queue_header
        .serialize(&mut (&mut event_queue_buffer as &mut [u8]))
        .unwrap();
    // Initialize the orderbook
    let mut asks_buffer = (0..Slab::compute_allocation_size(order_capacity, callback_info_len))
        .map(|_| 0u8)
        .collect::<Vec<_>>();
    // Initialize the orderbook
    let mut bids_buffer = asks_buffer.clone();
    Slab::initialize(&mut asks_buffer, &mut bids_buffer, market_key).unwrap();
    let mut market_state_buffer = (0..MARKET_STATE_LEN).map(|_| 0u8).collect::<Vec<_>>();
    {
        let market_state = bytemuck::from_bytes_mut::<MarketState>(&mut market_state_buffer);
        *market_state = MarketState {
            tag: AccountTag::Market as u64,
            caller_authority: caller_authority.pubkey().to_bytes(),
            event_queue: event_queue_key.to_bytes(),
            bids: bids_key.to_bytes(),
            asks: asks_key.to_bytes(),
            callback_id_len,
            callback_info_len: callback_info_len as u64,
            fee_budget: 0,
            initial_lamports: 0,
            min_base_order_size: 1,
            tick_size: 1,
            cranker_reward: 0,
        }
    }
    let asks_cell = RefCell::new(&mut asks_buffer as &mut [u8]);
    let asks_slab =
        SlabRef::get(asks_cell.borrow_mut(), callback_info_len, AccountTag::Asks).unwrap();
    let bids_cell = RefCell::new(&mut bids_buffer as &mut [u8]);
    let bids_slab =
        SlabRef::get(bids_cell.borrow_mut(), callback_info_len, AccountTag::Bids).unwrap();
    let mut orderbook = OrderBookState {
        bids: bids_slab,
        asks: asks_slab,
        callback_id_len: callback_id_len as usize,
    };
    let mut event_queue = EventQueue::from_buffer(
        event_queue_header,
        &mut event_queue_buffer,
        callback_info_len,
    );
    let mut asks_order_ids = Vec::with_capacity(order_capacity);
    let mut bids_order_ids = Vec::with_capacity(order_capacity);
    // Input orders
    for i in 0..order_capacity as u64 {
        // println!("{}", orderbook.asks.header.bump_index);
        let o = orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 1_000_000,
                    max_quote_qty: 1_000_000,
                    limit_price: (i + 1) << 32,
                    side: Side::Bid,
                    match_limit: 10,
                    callback_info: Pubkey::new_unique().to_bytes().to_vec(),
                    post_only: true,
                    post_allowed: true,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                1,
            )
            .unwrap();
        bids_order_ids.push(o.posted_order_id.unwrap());
        let o = orderbook
            .new_order(
                new_order::Params {
                    max_base_qty: 1_000_000,
                    max_quote_qty: 1_000_000,
                    limit_price: (i + 1 + (order_capacity as u64)) << 32,
                    side: Side::Ask,
                    match_limit: 10,
                    callback_info: Pubkey::new_unique().to_bytes().to_vec(),
                    post_only: true,
                    post_allowed: true,
                    self_trade_behavior: SelfTradeBehavior::DecrementTake,
                },
                &mut event_queue,
                1,
            )
            .unwrap();
        asks_order_ids.push(o.posted_order_id.unwrap());
        event_queue.pop_n(10);
        // println!("{}", i);
    }
    event_queue
        .header
        .serialize(&mut (&mut event_queue_buffer as &mut [u8]))
        .unwrap();
    // We choose the order id with maximum depth
    let test_order_id = asks_order_ids[asks_order_ids.len() / 2];

    // We initialize the Solana testing environment
    let mut program_test = ProgramTest::new(
        "agnostic_orderbook",
        agnostic_orderbook::ID,
        processor!(agnostic_orderbook::entrypoint::process_instruction),
    );

    let lamports: u64 = 100_000_000;

    drop(orderbook);

    let accounts_to_add = vec![
        (market_key, market_state_buffer),
        (event_queue_key, event_queue_buffer),
        (bids_key, bids_cell.borrow().to_owned()),
        (asks_key, asks_cell.borrow().to_owned()),
    ];

    for (k, data) in accounts_to_add.into_iter() {
        program_test.add_account(
            k,
            Account {
                lamports,
                data,
                owner: agnostic_orderbook::ID,
                ..Account::default()
            },
        )
    }
    Context {
        test_context: program_test,
        caller_authority,
        test_order_id,
        market: market_key,
        event_queue: event_queue_key,
        bids: bids_key,
        asks: asks_key,
    }
}
