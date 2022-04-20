use agnostic_orderbook::instruction::create_market;
use agnostic_orderbook::state::event_queue::EventQueue;
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use solana_program::system_instruction::create_account;
use solana_program_test::{BanksClientError, ProgramTestContext};
use solana_sdk::signature::Signer;
use solana_sdk::{signature::Keypair, transaction::Transaction};

/// Creates the accounts needed for the AAOB market testing and returns the
/// address of the market.
pub async fn create_market_and_accounts(
    prg_test_ctx: &mut ProgramTestContext,
    register_account: Pubkey,
    agnostic_orderbook_program_id: Pubkey,
) -> Pubkey {
    let rent = prg_test_ctx.banks_client.get_rent().await.unwrap();

    // Create market state account
    let market_account = Keypair::new();
    let create_market_account_instruction = create_account(
        &prg_test_ctx.payer.pubkey(),
        &market_account.pubkey(),
        rent.minimum_balance(1_000_000),
        1_000_000,
        &agnostic_orderbook_program_id,
    );
    sign_send_instructions(
        prg_test_ctx,
        vec![create_market_account_instruction],
        vec![&market_account],
    )
    .await
    .unwrap();

    // Create event queue account
    let event_queue_account = Keypair::new();
    let space = EventQueue::<[u8; 32]>::compute_allocation_size(1000);
    let create_event_queue_account_instruction = create_account(
        &prg_test_ctx.payer.pubkey(),
        &event_queue_account.pubkey(),
        rent.minimum_balance(space),
        space as u64,
        &agnostic_orderbook_program_id,
    );
    sign_send_instructions(
        prg_test_ctx,
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
        rent.minimum_balance(1_000_000),
        1_000_000,
        &agnostic_orderbook_program_id,
    );
    sign_send_instructions(
        prg_test_ctx,
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
        rent.minimum_balance(1_000_000),
        1_000_000,
        &agnostic_orderbook_program_id,
    );
    sign_send_instructions(
        prg_test_ctx,
        vec![create_asks_account_instruction],
        vec![&asks_account],
    )
    .await
    .unwrap();

    // Create Market
    let create_market_instruction = create_market(
        create_market::Accounts {
            market: &market_account.pubkey(),
            event_queue: &event_queue_account.pubkey(),
            bids: &bids_account.pubkey(),
            asks: &asks_account.pubkey(),
        },
        register_account,
        create_market::Params {
            min_base_order_size: 10,
            tick_size: 1,
            cranker_reward: 0,
        },
    );
    sign_send_instructions(prg_test_ctx, vec![create_market_instruction], vec![])
        .await
        .unwrap();

    market_account.pubkey()
}

// Utils
pub async fn sign_send_instructions(
    ctx: &mut ProgramTestContext,
    instructions: Vec<Instruction>,
    signers: Vec<&Keypair>,
) -> Result<(), BanksClientError> {
    let mut transaction = Transaction::new_with_payer(&instructions, Some(&ctx.payer.pubkey()));
    let mut payer_signers = vec![&ctx.payer];
    for s in signers {
        payer_signers.push(s);
    }
    transaction.partial_sign(&payer_signers, ctx.last_blockhash);
    ctx.banks_client.process_transaction(transaction).await
}
