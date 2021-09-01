use std::cell::RefCell;
use std::rc::Rc;

use agnostic_orderbook::instruction::{cancel_order, consume_events, create_market, new_order};
use agnostic_orderbook::state::{EventQueue, EventQueueHeader, SelfTradeBehavior, Side};
use agnostic_orderbook::state::{MarketState, OrderSummary};
use agnostic_orderbook::CRANKER_REWARD;
use borsh::BorshDeserialize;
use solana_program::pubkey::Pubkey;
use solana_program::system_instruction::{create_account, transfer};
use solana_program::system_program;
use solana_program_test::{processor, ProgramTest};
use solana_sdk::signature::Keypair;
use solana_sdk::signature::Signer;
// use spl_token::{
//     instruction::mint_to,
//     state::{Account, AccountState},
// };
pub mod common;
use crate::common::utils::sign_send_instructions;

//TODO factorize into context

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

    // Create bids account
    let bids_account = Keypair::new();
    let create_bids_account_instruction = create_account(
        &prg_test_ctx.payer.pubkey(),
        &bids_account.pubkey(),
        1_000_000,
        1_000_000,
        &agnostic_orderbook_program_id,
    );
    sign_send_instructions(
        &mut prg_test_ctx,
        vec![create_bids_account_instruction],
        vec![&bids_account],
    )
    .await
    .unwrap();

    // Create asks account
    let asks_account = Keypair::new();
    let create_asks_account_instruction = create_account(
        &prg_test_ctx.payer.pubkey(),
        &asks_account.pubkey(),
        1_000_000,
        1_000_000,
        &agnostic_orderbook_program_id,
    );
    sign_send_instructions(
        &mut prg_test_ctx,
        vec![create_asks_account_instruction],
        vec![&asks_account],
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

    // Create Market
    let caller_authority = Keypair::new();
    let create_market_instruction = create_market(
        agnostic_orderbook_program_id,
        market_account.pubkey(),
        event_queue_account.pubkey(),
        bids_account.pubkey(),
        asks_account.pubkey(),
        create_market::Params {
            caller_authority: caller_authority.pubkey(),
            callback_info_len: 32,
        },
    );
    sign_send_instructions(&mut prg_test_ctx, vec![create_market_instruction], vec![])
        .await
        .unwrap();

    // Transfer the fee
    let transfer_new_order_fee_instruction = transfer(
        &prg_test_ctx.payer.pubkey(),
        &market_account.pubkey(),
        CRANKER_REWARD,
    );
    sign_send_instructions(
        &mut prg_test_ctx,
        vec![transfer_new_order_fee_instruction],
        vec![],
    )
    .await
    .unwrap();

    let market_state = MarketState::deserialize(
        &mut &prg_test_ctx
            .banks_client
            .get_account(market_account.pubkey())
            .await
            .unwrap()
            .unwrap()
            .data[..],
    )
    .unwrap();
    println!("{:?}", market_state);

    // New Order
    let new_order_instruction = new_order(
        agnostic_orderbook_program_id,
        market_account.pubkey(),
        caller_authority.pubkey(),
        event_queue_account.pubkey(),
        bids_account.pubkey(),
        asks_account.pubkey(),
        new_order::Params {
            max_asset_qty: 1000,
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
        &market_account.pubkey(),
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
        market_account.pubkey(),
        caller_authority.pubkey(),
        event_queue_account.pubkey(),
        bids_account.pubkey(),
        asks_account.pubkey(),
        new_order::Params {
            max_asset_qty: 1000,
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

    let market_state = MarketState::deserialize(
        &mut &prg_test_ctx
            .banks_client
            .get_account(market_account.pubkey())
            .await
            .unwrap()
            .unwrap()
            .data[..],
    )
    .unwrap();
    println!("{:?}", market_state);

    let mut event_queue_acc = prg_test_ctx
        .banks_client
        .get_account(event_queue_account.pubkey())
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
        market_account.pubkey(),
        caller_authority.pubkey(),
        event_queue_account.pubkey(),
        bids_account.pubkey(),
        asks_account.pubkey(),
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

    // Consume events
    let consume_events_instruction = consume_events(
        agnostic_orderbook_program_id,
        market_account.pubkey(),
        caller_authority.pubkey(),
        event_queue_account.pubkey(),
        reward_target.pubkey(),
        consume_events::Params {
            number_of_entries_to_consume: 1,
        },
    );
    sign_send_instructions(
        &mut prg_test_ctx,
        vec![consume_events_instruction],
        vec![&caller_authority],
    )
    .await
    .unwrap();
}
