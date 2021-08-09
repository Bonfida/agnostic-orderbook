use std::cell::RefMut;

use bytemuck::Pod;
use solana_program::pubkey::Pubkey;

pub enum AccountFlag {
    Initialized,
    Market,
    EventQueue,
    Bids,
    Asks,
    Disabled,
    Permissioned,
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
    Fill,
    Out,
    Bid,
    Maker,
    ReleaseFunds,
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
