use agnostic_orderbook::instruction::{cancel_order, close_market, consume_events, new_order};
use agnostic_orderbook::state::{market_state::MarketState, OrderSummary};
use agnostic_orderbook::state::{SelfTradeBehavior, Side};
use bonfida_utils::BorshSize;
use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::try_from_bytes_mut;
use solana_program::program_option::COption;
use solana_program::program_pack::Pack;
use solana_program::pubkey::Pubkey;
use solana_program::system_instruction::{create_account, transfer};
use solana_program::system_program;
use solana_program_test::{processor, ProgramTest};
use solana_sdk::account::Account;
use solana_sdk::signature::Keypair;
use solana_sdk::signature::Signer;
pub mod common;
use crate::common::utils::{create_market_and_accounts, sign_send_instructions};

#[derive(BorshDeserialize, BorshSerialize)]
pub struct C([u8; 32]);

impl BorshSize for C {
    fn borsh_len(&self) -> usize {
        32
    }
}

#[tokio::test]
async fn test_agnostic_orderbook() {
    // Create program and test environment

    let mut program_test = ProgramTest::new(
        "agnostic_orderbook",
        agnostic_orderbook::ID,
        processor!(agnostic_orderbook::entrypoint::process_instruction),
    );

    let cranker_reward = 1_000;
    // Initialize MSRM mint

    let mut mint_data = vec![0; spl_token::state::Mint::LEN];

    let register_account = Pubkey::new_unique();

    spl_token::state::Mint {
        mint_authority: COption::None,
        supply: 1,
        decimals: 0,
        is_initialized: true,
        freeze_authority: COption::None,
    }
    .pack_into_slice(&mut mint_data);

    program_test.add_account(
        register_account,
        Account {
            lamports: 1_000_000,
            data: vec![0; 42],
            owner: agnostic_orderbook::ID,
            ..Account::default()
        },
    );

    // Create Market context
    let mut prg_test_ctx = program_test.start_with_context().await;
    let rent = prg_test_ctx.banks_client.get_rent().await.unwrap();

    // Create market state account
    let market_account = Keypair::new();
    let create_market_account_instruction = create_account(
        &prg_test_ctx.payer.pubkey(),
        &market_account.pubkey(),
        rent.minimum_balance(1_000_000),
        1_000_000,
        &agnostic_orderbook::ID,
    );
    sign_send_instructions(
        &mut prg_test_ctx,
        vec![create_market_account_instruction],
        vec![&market_account],
    )
    .await
    .unwrap();
    let market_account =
        create_market_and_accounts(&mut prg_test_ctx, register_account, agnostic_orderbook::ID)
            .await;

    let mut market_state_data = prg_test_ctx
        .banks_client
        .get_account(market_account)
        .await
        .unwrap()
        .unwrap();
    let market_state =
        try_from_bytes_mut::<MarketState>(&mut market_state_data.data[..MarketState::LEN]).unwrap();
    println!("{:#?}", market_state);

    // Transfer the cranking fee
    let transfer_new_order_fee_instruction = transfer(
        &prg_test_ctx.payer.pubkey(),
        &market_account,
        cranker_reward,
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
        new_order::Accounts {
            market: &market_account,
            event_queue: &market_state.event_queue,
            bids: &market_state.bids,
            asks: &market_state.asks,
        },
        register_account,
        new_order::Params {
            max_base_qty: 100000,
            max_quote_qty: 100000,
            limit_price: 1000 << 32,
            side: Side::Bid,
            callback_info: C(Pubkey::new_unique().to_bytes()),
            post_only: false,
            post_allowed: true,
            self_trade_behavior: SelfTradeBehavior::CancelProvide,
            match_limit: 3,
        },
    );
    sign_send_instructions(&mut prg_test_ctx, vec![new_order_instruction], vec![])
        .await
        .unwrap();

    // Transfer the fee, again
    let transfer_new_order_fee_instruction = transfer(
        &prg_test_ctx.payer.pubkey(),
        &market_account,
        cranker_reward + 1,
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
        new_order::Accounts {
            market: &market_account,
            event_queue: &market_state.event_queue,
            bids: &market_state.bids,
            asks: &market_state.asks,
        },
        register_account,
        new_order::Params {
            max_base_qty: 110000,
            max_quote_qty: 1000000,
            limit_price: 1000 << 32,
            side: Side::Ask,
            callback_info: C(Pubkey::new_unique().to_bytes()),
            post_only: false,
            post_allowed: true,
            self_trade_behavior: SelfTradeBehavior::CancelProvide,
            match_limit: 3,
        },
    );
    sign_send_instructions(&mut prg_test_ctx, vec![new_order_instruction], vec![])
        .await
        .unwrap();

    let mut market_data = prg_test_ctx
        .banks_client
        .get_account(market_account)
        .await
        .unwrap()
        .unwrap();
    let market_state =
        try_from_bytes_mut::<MarketState>(&mut market_data.data[..MarketState::LEN]).unwrap();
    println!("{:#?}", market_state);

    let mut register_acc = &prg_test_ctx
        .banks_client
        .get_account(register_account)
        .await
        .unwrap()
        .unwrap()
        .data as &[u8];
    let order_summary: Option<OrderSummary> = Option::deserialize(&mut register_acc).unwrap();
    println!("Parsed order summary {:#?}", order_summary);

    // Cancel order
    let cancel_order_instruction = cancel_order(
        cancel_order::Accounts {
            market: &market_account,
            event_queue: &market_state.event_queue,
            bids: &market_state.bids,
            asks: &market_state.asks,
        },
        register_account,
        cancel_order::Params {
            order_id: order_summary.unwrap().posted_order_id.unwrap(),
        },
    );
    sign_send_instructions(&mut prg_test_ctx, vec![cancel_order_instruction], vec![])
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
        consume_events::Accounts {
            market: &market_account,
            event_queue: &market_state.event_queue,
            reward_target: &reward_target.pubkey(),
        },
        register_account,
        consume_events::Params {
            number_of_entries_to_consume: 10,
        },
    );
    sign_send_instructions(&mut prg_test_ctx, vec![consume_events_instruction], vec![])
        .await
        .unwrap();

    // Close Market
    let close_market_instruction = close_market(
        close_market::Accounts {
            market: &market_account,
            event_queue: &market_state.event_queue,
            bids: &market_state.bids,
            asks: &market_state.asks,
            lamports_target_account: &reward_target.pubkey(),
        },
        register_account,
        close_market::Params {},
    );
    sign_send_instructions(&mut prg_test_ctx, vec![close_market_instruction], vec![])
        .await
        .unwrap();
}
