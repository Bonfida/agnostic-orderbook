pub enum AccountFlag {
    Initialized = 1u64 << 0,
    Market = 1u64 << 1,
    OpenOrders = 1u64 << 2,
    EventQueue = 1u64 << 3,
    Bids = 1u64 << 4,
    Asks = 1u64 << 5,
    Disabled = 1u64 << 6,
    Closed = 1u64 << 7,
    Permissioned = 1u64 << 8,
}

pub struct MarketState {
    pub account_flags: u64, // Initialized, Market
    pub own_address: Pubkey,
    pub caller_program_adr: Pubkey, // The program that consumes the event queue
    pub event_queue: Pubkey,
    pub bids: Pubkey,
    pub asks: Pubkey,
    pub open_orders_authority: Pubkey,
    pub prune_authority: Pubkey,
    // Unused bytes for future upgrades.
    padding: [u8; 1024],
}

pub struct OpenOrders {
    pub account_flags: u64, // Initialized, OpenOrders
    pub market: Pubkey,
    pub owner: Pubkey,
    pub free_slot_bits: u128,
    pub is_bid_bits: u128,
    pub orders: [u128; 128],
    // Using Option<NonZeroU64> in a pod type requires nightly
    pub client_order_ids: [u64; 128],
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

    _padding: [u8; 5],

    native_qty_released: u64,
    native_qty_paid: u64,

    order_id: u128,
    pub owner: Pubkey,
    client_order_id: u64,
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
