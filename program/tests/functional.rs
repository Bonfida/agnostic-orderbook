use agnostic_orderbook::instruction::{create_market, new_order};
use agnostic_orderbook::processor::new_order::NewOrderParams;
use agnostic_orderbook::state::SelfTradeBehavior;
use solana_program::pubkey::Pubkey;
use solana_program::system_instruction::create_account;
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

    // Create Market
    let caller_authority = Keypair::new();
    let create_market_instruction = create_market(
        agnostic_orderbook_program_id,
        market_account.pubkey(),
        caller_authority.pubkey(),
        event_queue_account.pubkey(),
        bids_account.pubkey(),
        asks_account.pubkey(),
        None,
    );
    sign_send_instructions(&mut prg_test_ctx, vec![create_market_instruction], vec![])
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
        NewOrderParams {
            max_base_qty: 1000,
            max_quote_qty: 1000,
            limit_price: 1000,
            owner: Pubkey::new_unique(),
            post_only: false,
            post_allowed: true,
            self_trade_behavior: SelfTradeBehavior::CancelProvide,
            order_id: 1000 << 64,
        },
    );
    sign_send_instructions(
        &mut prg_test_ctx,
        vec![new_order_instruction],
        vec![&caller_authority],
    )
    .await
    .unwrap();
}
