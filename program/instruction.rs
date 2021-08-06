#[repr(C)]
#[cfg_attr(feature = "fuzz", derive(Arbitrary))]
#[derive(Clone, Debug, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum AgnosticOrderbookInstruction {
    /// 0. `[writable]` the market to initialize
    /// 1. `[writable]` zeroed out request queue
    /// 2. `[writable]` zeroed out event queue
    /// 3. `[writable]` zeroed out bids
    /// 4. `[writable]` zeroed out asks
    /// 9. `[]` the rent sysvar
    /// 10. `[]` open orders market authority (optional)
    /// 11. `[]` prune authority (optional, requires open orders market authority)
    InitializeMarket(InitializeMarketInstruction),
    /// 0. `[writable]` the market
    /// 1. `[writable]` the OpenOrders account to use
    /// 3. `[writable]` the event queue
    /// 4. `[writable]` bids
    /// 5. `[writable]` asks
    /// 6. `[writable]` the (coin or price currency) account paying for the order
    /// 7. `[signer]` owner of the OpenOrders account
    /// 11. `[]` the rent sysvar
    NewOrder(NewOrderInstruction),
    /// 0. `[writable]` market
    /// 2. `[writable]` event_q
    /// 3. `[writable]` bids
    /// 4. `[writable]` asks
    MatchOrders(u16),
    /// ... `[writable]` OpenOrders
    /// accounts.len() - 2 `[writable]` market
    /// accounts.len() - 1 `[writable]` event queue
    ConsumeEvents(u16),
    /// 0. `[]` market
    /// 1. `[writable]` OpenOrders
    /// 2. `[signer]` the OpenOrders owner
    /// 3. `[writable]` asks or bids
    CancelOrder(CancelOrderInstruction),
    /// 0. `[writable]` market
    /// 1. `[writable]` bids
    /// 2. `[writable]` asks
    /// 3. `[writable]` OpenOrders
    /// 4. `[signer]` the OpenOrders owner
    /// 5. `[writable]` event_q
    CancelOrderByClientId(u64),
    /// 0. `[writable]` market
    /// 1. `[signer]` disable authority
    DisableMarket,
    /// 0. `[writable]` market
    /// 1. `[writable]` bids
    /// 2. `[writable]` asks
    /// 3. `[writable]` OpenOrders
    SendTake(SendTakeInstruction),
    /// 0. `[writable]` OpenOrders
    /// 1. `[signer]` the OpenOrders owner
    /// 2. `[writable]` the destination account to send rent exemption SOL to
    /// 3. `[]` market
    CloseOpenOrders,
    /// 0. `[writable]` OpenOrders
    /// 1. `[signer]` the OpenOrders owner
    /// 2. `[]` market
    /// 3. `[]` the rent sysvar
    /// 4. `[signer]` open orders market authority (optional).
    InitOpenOrders,
    /// Removes all orders for a given open orders account from the orderbook.
    ///
    /// 0. `[writable]` market
    /// 1. `[writable]` bids
    /// 2. `[writable]` asks
    /// 3. `[signer]` prune authority
    /// 4. `[]` open orders.
    /// 5. `[]` open orders owner.
    /// 6. `[writable]` event queue.
    Prune(u16),
}
