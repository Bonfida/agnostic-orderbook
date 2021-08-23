use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use crate::processor::{cancel_order, consume_event, create_market, new_order};

#[derive(BorshDeserialize, BorshSerialize)]
pub enum AgnosticOrderbookInstruction {
    /// 0. `[writable]` The market account
    /// 1. `[writable]` A zeroed out event queue account
    /// 2. `[writable]` A zeroed out bids account
    /// 3. `[writable]` A zeroed out asks account
    /// 5. `[]` The market authority (optional)
    CreateMarket(create_market::Params),
    /// 0. `[writable]` The market account
    /// 1. `[writable]` The event queue account
    /// 2. `[writable]` The bids account
    /// 3. `[writable]` The asks account
    /// 4. `[]` The owner of the order
    /// 5. `[signer]` The caller authority
    NewOrder(new_order::Params),
    /// 0. `[writable]` The market account
    /// 1. `[writable]` The event queue account
    /// 2. `[signer]` The caller authority
    ConsumeEvents(consume_event::Params),
    /// 0. `[writable]` The market account
    /// 1. `[signer]` The order owner
    /// 2. `[writable]` Then asks or bids account
    CancelOrder(cancel_order::Params),
    /// 0. `[writable]` The market account
    /// 1. `[signer]` The market authority
    DisableMarket,
}

pub fn create_market(
    agnostic_orderbook_program_id: Pubkey,
    market_account: Pubkey,
    caller_authority: Pubkey,
    event_queue: Pubkey,
    bids: Pubkey,
    asks: Pubkey,
    market_authority: Option<Pubkey>,
    callback_info_len: u64,
) -> Instruction {
    let instruction_data = AgnosticOrderbookInstruction::CreateMarket(create_market::Params {
        caller_authority,
        event_queue,
        bids,
        asks,
        callback_info_len,
    });
    let data = instruction_data.try_to_vec().unwrap();
    let mut accounts = vec![
        AccountMeta::new(market_account, false),
        AccountMeta::new(event_queue, false),
        AccountMeta::new(bids, false),
        AccountMeta::new(asks, false),
    ];
    if let Some(market_auth) = market_authority {
        accounts.push(AccountMeta::new_readonly(market_auth, false))
    }

    Instruction {
        program_id: agnostic_orderbook_program_id,
        accounts,
        data,
    }
}

pub fn new_order(
    agnostic_orderbook_program_id: Pubkey,
    market_account: Pubkey,
    caller_authority: Pubkey,
    event_queue: Pubkey,
    bids: Pubkey,
    asks: Pubkey,
    new_order_params: new_order::Params,
) -> Instruction {
    let data = AgnosticOrderbookInstruction::NewOrder(new_order_params.clone())
        .try_to_vec()
        .unwrap();
    let accounts = vec![
        AccountMeta::new(market_account, false),
        AccountMeta::new(event_queue, false),
        AccountMeta::new(bids, false),
        AccountMeta::new(asks, false),
        AccountMeta::new_readonly(caller_authority, true),
    ];

    Instruction {
        program_id: agnostic_orderbook_program_id,
        accounts,
        data,
    }
}
