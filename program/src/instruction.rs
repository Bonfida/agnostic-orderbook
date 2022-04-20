use borsh::{BorshDeserialize, BorshSerialize};
use num_derive::FromPrimitive;
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use bonfida_utils::{BorshSize, InstructionsAccount};

pub use crate::processor::{
    cancel_order, close_market, consume_events, create_market, mass_cancel_orders, new_order,
};
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
    /// Cancel a series of existing orders in the orderbook.
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
    MassCancelOrders,
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
    accounts: create_market::Accounts<Pubkey>,
    register_account: Pubkey,
    params: create_market::Params,
) -> Instruction {
    let mut i = accounts.get_instruction(
        crate::id(),
        AgnosticOrderbookInstruction::CreateMarket as u8,
        params,
    );

    i.accounts.push(AccountMeta {
        pubkey: register_account,
        is_signer: false,
        is_writable: true,
    });
    i
}
/**
Execute a new order on the orderbook.

Depending on the provided parameters, the program will attempt to match the order with existing entries
in the orderbook, and then optionally post the remaining order.
*/
pub fn new_order<C: BorshSerialize + BorshSize>(
    accounts: new_order::Accounts<Pubkey>,
    register_account: Pubkey,
    params: new_order::Params<C>,
) -> Instruction {
    let mut i = accounts.get_instruction(
        crate::id(),
        AgnosticOrderbookInstruction::NewOrder as u8,
        params,
    );

    i.accounts.push(AccountMeta {
        pubkey: register_account,
        is_signer: false,
        is_writable: true,
    });
    i
}

/// Cancel an existing order in the orderbook.
pub fn cancel_order(
    accounts: cancel_order::Accounts<Pubkey>,
    register_account: Pubkey,
    params: cancel_order::Params,
) -> Instruction {
    let mut i = accounts.get_instruction(
        crate::id(),
        AgnosticOrderbookInstruction::CancelOrder as u8,
        params,
    );
    i.accounts.push(AccountMeta {
        pubkey: register_account,
        is_signer: false,
        is_writable: true,
    });
    i
}

/// Pop a series of events off the event queue.
pub fn consume_events(
    accounts: consume_events::Accounts<Pubkey>,
    register_account: Pubkey,
    params: consume_events::Params,
) -> Instruction {
    let mut i = accounts.get_instruction(
        crate::id(),
        AgnosticOrderbookInstruction::ConsumeEvents as u8,
        params,
    );

    i.accounts.push(AccountMeta {
        pubkey: register_account,
        is_signer: false,
        is_writable: true,
    });
    i
}

/// Close an existing market.
pub fn close_market(
    accounts: close_market::Accounts<Pubkey>,
    register_account: Pubkey,
    params: close_market::Params,
) -> Instruction {
    let mut i = accounts.get_instruction(
        crate::id(),
        AgnosticOrderbookInstruction::CloseMarket as u8,
        params,
    );

    i.accounts.push(AccountMeta {
        pubkey: register_account,
        is_signer: false,
        is_writable: true,
    });
    i
}

/// Create and initialize a new orderbook market
pub fn mass_cancel_orders(
    accounts: mass_cancel_orders::Accounts<Pubkey>,
    register_account: Pubkey,
    params: mass_cancel_orders::Params,
) -> Instruction {
    let mut i = accounts.get_instruction(
        crate::id(),
        AgnosticOrderbookInstruction::CloseMarket as u8,
        params,
    );
    i.accounts.push(AccountMeta {
        pubkey: register_account,
        is_signer: false,
        is_writable: true,
    });
    i
}
