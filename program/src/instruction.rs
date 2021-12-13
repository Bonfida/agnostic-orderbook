use bonfida_utils::InstructionsAccount;
use borsh::{BorshDeserialize, BorshSerialize};
use num_derive::FromPrimitive;
use solana_program::{instruction::Instruction, pubkey::Pubkey};

pub use crate::processor::{cancel_order, close_market, consume_events, create_market, new_order};
#[derive(BorshDeserialize, BorshSerialize, FromPrimitive)]
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
    CreateMarket,
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
    NewOrder,
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
    ConsumeEvents,
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
    CancelOrder,
    /// Close an existing market.
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
    program_id: Pubkey,
    accounts: create_market::Accounts<Pubkey>,
    params: create_market::Params,
) -> Instruction {
    accounts.get_instruction(
        program_id,
        AgnosticOrderbookInstruction::CreateMarket as u8,
        params,
    )
}
/**
Execute a new order on the orderbook.

Depending on the provided parameters, the program will attempt to match the order with existing entries
in the orderbook, and then optionally post the remaining order.
*/
pub fn new_order(
    program_id: Pubkey,
    accounts: new_order::Accounts<Pubkey>,
    params: new_order::Params,
) -> Instruction {
    accounts.get_instruction(
        program_id,
        AgnosticOrderbookInstruction::NewOrder as u8,
        params,
    )
}

/// Cancel an existing order in the orderbook.
pub fn cancel_order(
    program_id: Pubkey,
    accounts: cancel_order::Accounts<Pubkey>,
    params: cancel_order::Params,
) -> Instruction {
    accounts.get_instruction(
        program_id,
        AgnosticOrderbookInstruction::CancelOrder as u8,
        params,
    )
}

/// Pop a series of events off the event queue.
pub fn consume_events(
    program_id: Pubkey,
    accounts: consume_events::Accounts<Pubkey>,
    params: consume_events::Params,
) -> Instruction {
    accounts.get_instruction(
        program_id,
        AgnosticOrderbookInstruction::ConsumeEvents as u8,
        params,
    )
}

/// Close an existing market.
pub fn close_market(
    program_id: Pubkey,
    accounts: close_market::Accounts<Pubkey>,
    params: close_market::Params,
) -> Instruction {
    accounts.get_instruction(
        program_id,
        AgnosticOrderbookInstruction::CloseMarket as u8,
        params,
    )
}
