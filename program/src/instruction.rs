use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

pub use crate::processor::{cancel_order, close_market, consume_events, create_market, new_order};
#[derive(BorshDeserialize, BorshSerialize)]
/// Describes all possible instructions and their required accounts
pub enum AgnosticOrderbookInstruction {
    /// Create and initialize a new orderbook market
    ///
    /// Required accounts
    ///
    /// | index | writable | signer | description                      |
    /// |-------|----------|--------|----------------------------------|
    /// | 0     | ✅       | ❌     | The market account               |
    /// | 1     | ✅       | ❌     | A zeroed out event queue account |
    /// | 2     | ✅       | ❌     | A zeroed out bids account        |
    /// | 3     | ✅       | ❌     | A zeroed out asks account        |
    CreateMarket(create_market::Params),
    /// Execute a new order on the orderbook.
    ///
    /// Depending on the provided parameters, the program will attempt to match the order with existing entries
    /// in the orderbook, and then optionally post the remaining order.
    ///
    /// Required accounts
    ///
    ///
    /// | index | writable | signer | description             |
    /// |-------|----------|--------|-------------------------|
    /// | 0     | ✅       | ❌     | The market account      |
    /// | 1     | ✅       | ❌     | The event queue account |
    /// | 2     | ✅       | ❌     | The bids account        |
    /// | 3     | ✅       | ❌     | The asks account        |
    /// | 4     | ❌       | ✅     | The caller authority    |
    NewOrder(new_order::Params),
    /// Pop a series of events off the event queue.
    ///
    /// Required accounts
    ///
    /// | index | writable | signer | description                  |
    /// |-------|----------|--------|------------------------------|
    /// | 0     | ✅       | ❌     | The market account           |
    /// | 1     | ✅       | ❌     | The event queue account      |
    /// | 3     | ❌       | ✅     | The caller authority         |
    /// | 4     | ✅       | ❌     | The reward target account    |
    /// | 5     | ❌       | ❌     | The MSRM token account       |
    /// | 6     | ❌       | ✅     | The MSRM token account owner |
    ConsumeEvents(consume_events::Params),
    /// Cancel an existing order in the orderbook.
    ///
    /// Required accounts
    ///
    /// | index | writable | signer | description             |
    /// |-------|----------|--------|-------------------------|
    /// | 0     | ✅       | ❌     | The market account      |
    /// | 1     | ✅       | ❌     | The event queue account |
    /// | 2     | ✅       | ❌     | The bids account        |
    /// | 3     | ✅       | ❌     | The asks account        |
    /// | 4     | ❌       | ✅     | The caller authority    |
    CancelOrder(cancel_order::Params),
    /// Close and existing market.
    ///
    /// Required accounts
    ///
    /// | index | writable | signer   | description                 |
    /// |-------|----------|----------|-----------------------------|
    /// | 0     | ✅        | ❌      | The market account          |
    /// | 1     | ✅        | ❌      | The event queue account     |
    /// | 2     | ✅        | ❌      | The bids account            |
    /// | 3     | ✅        | ❌      | The asks account            |
    /// | 4     | ❌        | ✅      | The caller authority        |
    /// | 5     | ✅        | ❌      | The lamports target account |
    CloseMarket,
}

/**
Create and initialize a new orderbook market

The event_queue, bids, and asks accounts should be freshly allocated or zeroed out accounts.

* The market account will only contain a [`MarketState`](`crate::state::MarketState`) object and should be sized appropriately.

* The event queue will contain an [`EventQueueHeader`](`crate::state::EventQueueHeader`) object followed by a return register sized for a [`OrderSummary`](`crate::orderbook::OrderSummary`)
(size of [`ORDER_SUMMARY_SIZE`](`crate::orderbook::ORDER_SUMMARY_SIZE`)) and then a series of events [`Event`](`crate::state::Event`). The serialized size of an [`Event`](`crate::state::Event`) object
is given by [`compute_slot_size`](`crate::state::Event::compute_slot_size`) The size of the queue should be determined
accordingly.

* The asks and bids accounts will contain a header of size [`SLAB_HEADER_LEN`][`crate::critbit::SLAB_HEADER_LEN`] followed by a series of slots of size
[`compute_slot_size(callback_info_len)`][`crate::critbit::Slab::compute_slot_size`].
*/
pub fn create_market(
    agnostic_orderbook_program_id: Pubkey,
    market_account: Pubkey,
    event_queue: Pubkey,
    bids: Pubkey,
    asks: Pubkey,
    create_market_params: create_market::Params,
) -> Instruction {
    let instruction_data = AgnosticOrderbookInstruction::CreateMarket(create_market_params);
    let data = instruction_data.try_to_vec().unwrap();
    let accounts = vec![
        AccountMeta::new(market_account, false),
        AccountMeta::new(event_queue, false),
        AccountMeta::new(bids, false),
        AccountMeta::new(asks, false),
    ];

    Instruction {
        program_id: agnostic_orderbook_program_id,
        accounts,
        data,
    }
}
/**
Execute a new order on the orderbook.

Depending on the provided parameters, the program will attempt to match the order with existing entries
in the orderbook, and then optionally post the remaining order.
*/
pub fn new_order(
    agnostic_orderbook_program_id: Pubkey,
    market_account: Pubkey,
    caller_authority: Pubkey,
    event_queue: Pubkey,
    bids: Pubkey,
    asks: Pubkey,
    new_order_params: new_order::Params,
) -> Instruction {
    let data = AgnosticOrderbookInstruction::NewOrder(new_order_params)
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

/// Cancel an existing order in the orderbook.
pub fn cancel_order(
    agnostic_orderbook_program_id: Pubkey,
    market_account: Pubkey,
    caller_authority: Pubkey,
    event_queue: Pubkey,
    bids: Pubkey,
    asks: Pubkey,
    cancel_order_params: cancel_order::Params,
) -> Instruction {
    let data = AgnosticOrderbookInstruction::CancelOrder(cancel_order_params)
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

/// Pop a series of events off the event queue.
#[allow(clippy::clippy::too_many_arguments)]
pub fn consume_events(
    agnostic_orderbook_program_id: Pubkey,
    market_account: Pubkey,
    caller_authority: Pubkey,
    event_queue: Pubkey,
    reward_target: Pubkey,
    msrm_token_account: Pubkey,
    msrm_token_account_owner: Pubkey,
    consume_events_params: consume_events::Params,
) -> Instruction {
    let data = AgnosticOrderbookInstruction::ConsumeEvents(consume_events_params)
        .try_to_vec()
        .unwrap();
    let accounts = vec![
        AccountMeta::new(market_account, false),
        AccountMeta::new(event_queue, false),
        AccountMeta::new_readonly(caller_authority, true),
        AccountMeta::new(reward_target, false),
        AccountMeta::new_readonly(msrm_token_account, false),
        AccountMeta::new_readonly(msrm_token_account_owner, true),
    ];

    Instruction {
        program_id: agnostic_orderbook_program_id,
        accounts,
        data,
    }
}

/// Close and existing market.
#[allow(clippy::clippy::too_many_arguments)]
pub fn close_market(
    agnostic_orderbook_program_id: Pubkey,
    market: Pubkey,
    event_queue: Pubkey,
    bids: Pubkey,
    asks: Pubkey,
    authority: Pubkey,
    lamports_target_account: Pubkey,
) -> Instruction {
    let data = AgnosticOrderbookInstruction::CloseMarket
        .try_to_vec()
        .unwrap();
    let accounts = vec![
        AccountMeta::new(market, false),
        AccountMeta::new(event_queue, false),
        AccountMeta::new(bids, false),
        AccountMeta::new(asks, false),
        AccountMeta::new_readonly(authority, true),
        AccountMeta::new(lamports_target_account, false),
    ];

    Instruction {
        program_id: agnostic_orderbook_program_id,
        accounts,
        data,
    }
}
