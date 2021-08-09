pub enum AgnosticOrderbookInstruction {
    /// 0. `[writable]` The market account
    /// 1. `[writable]` A zeroed out event queue account
    /// 2. `[writable]` A zeroed out bids account
    /// 3. `[writable]` A zeroed out asks account
    /// 4. `[]` The rent sysvar system program
    /// 5. `[]` The market authority (optional)
    InitializeMarket(InitializeMarketInstruction),
    /// 0. `[writable]` The market account
    /// 1. `[writable]` The event queue account
    /// 2. `[writable]` The bids account
    /// 3. `[writable]` The asks account
    /// 4. `[]` The owner of the order
    /// 5. `[]` The rent sysvar system program
    NewOrder(NewOrderInstruction),
    /// 0. `[writable]` The market account
    /// 1. `[writable]` The event queue account
    /// 2. `[writable]` The bids account
    /// 3. `[writable]` The asks account
    MatchOrders(u16),
    /// 0. `[writable]` The market account
    /// 1. `[writable]` The event queue account
    /// 2. `[signer]` The caller authority
    ConsumeEvents(u16),
    /// 0. `[writable]` The market account
    /// 1. `[signer]` The order owner
    /// 2. `[writable]` Then asks or bids account
    CancelOrder(CancelOrderInstruction),
    /// 0. `[writable]` The market account
    /// 1. `[writable]` The bids account
    /// 2. `[writable]` The asks account
    /// 3. `[signer]` The order owner
    /// 4. `[writable]` The event queue account
    CancelOrderByClientId(u64),
    /// 0. `[writable]` The market account
    /// 1. `[signer]` The market authority
    DisableMarket,
}
