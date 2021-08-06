pub enum AccountFlag {
    Initialized = 1u64 << 0,
    Market = 1u64 << 1,
    EventQueue = 1u64 << 2,
    Bids = 1u64 << 3,
    Asks = 1u64 << 4,
    Disabled = 1u64 << 5,
    Permissioned = 1u64 << 6,
}

pub struct MarketState {
    pub account_flags: u64, // Initialized, Market
    pub own_address: Pubkey,
    pub caller_authority: Pubkey, // The program that consumes the event queue via CPIs
    pub event_queue: Pubkey,
    pub bids: Pubkey,
    pub asks: Pubkey,
    pub market_authority: Pubkey, // The authority for disabling the market
}

enum EventFlag {
    Fill = 0x1,
    Out = 0x2,
    Bid = 0x4,
    Maker = 0x8,
    ReleaseFunds = 0x10,
}

pub struct Event {
    event_flags: u8,
    owner_slot: u8,
    native_qty_released: u64,
    native_qty_paid: u64,
}

////////////////////////////////////////////////////
// Queues

pub trait QueueHeader: Pod {
    type Item: Pod + Copy;

    fn head(&self) -> u64;
    fn set_head(&mut self, value: u64);
    fn count(&self) -> u64;
    fn set_count(&mut self, value: u64);

    fn incr_event_id(&mut self);
    fn decr_event_id(&mut self, n: u64);
}

pub struct Queue<'a, H: QueueHeader> {
    header: RefMut<'a, H>,
    buf: RefMut<'a, [H::Item]>,
}

pub struct EventQueueHeader {
    account_flags: u64, // Initialized, EventQueue
    head: u64,
    count: u64,
    seq_num: u64,
}

pub type EventQueue<'a> = Queue<'a, EventQueueHeader>;

////////////////////////////////////////////////////
// Critbit (mostly remains untouched, the Asks and Bids slabs should contain a header
// that references the market as its no longer via the OpenOrders Account which is removed.)

pub struct LeafNode {
    tag: u32,
    key: u128,
    owner: Pubkey,
    quantity: u64,
}
