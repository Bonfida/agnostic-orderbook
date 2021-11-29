use std::cell::RefCell;
use std::rc::Rc;

use agnostic_orderbook::instruction::{cancel_order, close_market, consume_events, new_order};
use agnostic_orderbook::state::{
    EventQueue, EventQueueHeader, SelfTradeBehavior, Side, MARKET_STATE_LEN,
};
use agnostic_orderbook::state::{MarketState, OrderSummary};
use agnostic_orderbook::CRANKER_REWARD;
use borsh::BorshDeserialize;
use bytemuck::try_from_bytes_mut;
use solana_program::pubkey::Pubkey;
use solana_program::system_instruction::{create_account, transfer};
use solana_program::system_program;
use solana_program_test::{processor, ProgramTest};
use solana_sdk::signature::Keypair;
use solana_sdk::signature::Signer;
pub mod common;
use crate::common::utils::{create_market_and_accounts, sign_send_instructions};

#[tokio::test]
async fn test_agnostic_orderbook() {
    // Create program and test environment
    let agnostic_orderbook_program_id = Pubkey::new_unique();

    let program_test = ProgramTest::new(
        "agnostic_orderbook",
        agnostic_orderbook_program_id,
        processor!(agnostic_orderbook::entrypoint::process_instruction),
    );

    // Create Market context
    let mut prg_test_ctx = program_test.start_with_context().await;

    // Create market state account
    let market_account = Keypair::new();
    let create_market_account_instruction = create_account(
        &prg_test_ctx.payer.pubkey(),
        &market_account.pubkey(),
        1_000_000,
        1_000_000,
        &agnostic_orderbook_program_id,
    );
    sign_send_instructions(
        &mut prg_test_ctx,
        vec![create_market_account_instruction],
        vec![&market_account],
    )
    .await
    .unwrap();

    // Create event queue account
    let event_queue_account = Keypair::new();
    let create_event_queue_account_instruction = create_account(
        &prg_test_ctx.payer.pubkey(),
        &event_queue_account.pubkey(),
        1_000_000,
        1_000_000,
        &agnostic_orderbook_program_id,
    );
    sign_send_instructions(
        &mut prg_test_ctx,
        vec![create_event_queue_account_instruction],
        vec![&event_queue_account],
    )
    .await
    .unwrap();

    let caller_authority = Keypair::new();
    let market_account = create_market_and_accounts(
        &mut prg_test_ctx,
        agnostic_orderbook_program_id,
        &caller_authority,
    )
    .await;

    let mut market_state_data = prg_test_ctx
        .banks_client
        .get_account(market_account)
        .await
        .unwrap()
        .unwrap();
    let market_state =
        try_from_bytes_mut::<MarketState>(&mut market_state_data.data[..MARKET_STATE_LEN]).unwrap();
    println!("{:?}", market_state);

    // Transfer the cranking fee
    let transfer_new_order_fee_instruction = transfer(
        &prg_test_ctx.payer.pubkey(),
        &market_account,
        CRANKER_REWARD,
    );
    sign_send_instructions(
        &mut prg_test_ctx,
        vec![transfer_new_order_fee_instruction],
        vec![],
    )
    .await
    .unwrap();

    // New Order
    let new_order_instruction = new_order(
        agnostic_orderbook_program_id,
        market_account,
        Pubkey::new_from_array(market_state.caller_authority),
        Pubkey::new_from_array(market_state.event_queue),
        Pubkey::new_from_array(market_state.bids),
        Pubkey::new_from_array(market_state.asks),
        new_order::Params {
            max_base_qty: 1000,
            max_quote_qty: 1000,
            limit_price: 1000,
            side: Side::Bid,
            callback_info: Pubkey::new_unique().to_bytes().to_vec(),
            post_only: false,
            post_allowed: true,
            self_trade_behavior: SelfTradeBehavior::CancelProvide,
            match_limit: 3,
        },
    );
    sign_send_instructions(
        &mut prg_test_ctx,
        vec![new_order_instruction],
        vec![&caller_authority],
    )
    .await
    .unwrap();

    // Transfer the fee, again
    let transfer_new_order_fee_instruction = transfer(
        &prg_test_ctx.payer.pubkey(),
        &market_account,
        CRANKER_REWARD + 1,
    );
    sign_send_instructions(
        &mut prg_test_ctx,
        vec![transfer_new_order_fee_instruction],
        vec![],
    )
    .await
    .unwrap();

    // New Order
    let new_order_instruction = new_order(
        agnostic_orderbook_program_id,
        market_account,
        Pubkey::new_from_array(market_state.caller_authority),
        Pubkey::new_from_array(market_state.event_queue),
        Pubkey::new_from_array(market_state.bids),
        Pubkey::new_from_array(market_state.asks),
        new_order::Params {
            max_base_qty: 1100,
            max_quote_qty: 1000,
            limit_price: 1000,
            side: Side::Ask,
            callback_info: Pubkey::new_unique().to_bytes().to_vec(),
            post_only: false,
            post_allowed: true,
            self_trade_behavior: SelfTradeBehavior::CancelProvide,
            match_limit: 3,
        },
    );
    sign_send_instructions(
        &mut prg_test_ctx,
        vec![new_order_instruction],
        vec![&caller_authority],
    )
    .await
    .unwrap();

    let mut market_data = prg_test_ctx
        .banks_client
        .get_account(market_account)
        .await
        .unwrap()
        .unwrap();
    let market_state =
        try_from_bytes_mut::<MarketState>(&mut market_data.data[..MARKET_STATE_LEN]).unwrap();
    println!("{:?}", market_state);

    let mut event_queue_acc = prg_test_ctx
        .banks_client
        .get_account(Pubkey::new_from_array(market_state.event_queue))
        .await
        .unwrap()
        .unwrap();
    let event_queue_header =
        EventQueueHeader::deserialize(&mut (&event_queue_acc.data as &[u8])).unwrap();
    let event_queue = EventQueue::new(
        event_queue_header,
        Rc::new(RefCell::new(&mut event_queue_acc.data)),
        32,
    );
    let order_summary: OrderSummary = event_queue.read_register().unwrap().unwrap();
    println!("Parsed order summary {:?}", order_summary);

    // Cancel order
    let cancel_order_instruction = cancel_order(
        agnostic_orderbook_program_id,
        market_account,
        Pubkey::new_from_array(market_state.caller_authority),
        Pubkey::new_from_array(market_state.event_queue),
        Pubkey::new_from_array(market_state.bids),
        Pubkey::new_from_array(market_state.asks),
        cancel_order::Params {
            order_id: order_summary.posted_order_id.unwrap(),
        },
    );
    sign_send_instructions(
        &mut prg_test_ctx,
        vec![cancel_order_instruction],
        vec![&caller_authority],
    )
    .await
    .unwrap();

    // Create reward target account
    let reward_target = Keypair::new();
    let create_reward_target_account_instruction = create_account(
        &prg_test_ctx.payer.pubkey(),
        &reward_target.pubkey(),
        1_000_000_000,
        1,
        &system_program::id(),
    );
    sign_send_instructions(
        &mut prg_test_ctx,
        vec![create_reward_target_account_instruction],
        vec![&reward_target],
    )
    .await
    .unwrap();

    // Consume events
    let consume_events_instruction = consume_events(
        agnostic_orderbook_program_id,
        market_account,
        Pubkey::new_from_array(market_state.caller_authority),
        Pubkey::new_from_array(market_state.event_queue),
        reward_target.pubkey(),
        consume_events::Params {
            number_of_entries_to_consume: 10,
        },
    );
    sign_send_instructions(
        &mut prg_test_ctx,
        vec![consume_events_instruction],
        vec![&caller_authority],
    )
    .await
    .unwrap();

    // Close Market
    let close_market_instruction = close_market(
        agnostic_orderbook_program_id,
        market_account,
        Pubkey::new_from_array(market_state.event_queue),
        Pubkey::new_from_array(market_state.bids),
        Pubkey::new_from_array(market_state.asks),
        Pubkey::new_from_array(market_state.caller_authority),
        reward_target.pubkey(),
    );
    sign_send_instructions(
        &mut prg_test_ctx,
        vec![close_market_instruction],
        vec![&caller_authority],
    )
    .await
    .unwrap();
}
