use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::Pod;
use enumflags2::BitFlags;
use solana_program::pubkey::Pubkey;
use std::cell::RefMut;

pub enum AccountFlag {
    Initialized,
    Market,
    EventQueue,
    Bids,
    Asks,
    Disabled,
    Permissioned,
}

pub enum Side {
    Bid,
    Ask,
}

#[derive(BorshDeserialize, BorshSerialize)]
pub enum SelfTradeBehavior {
    DecrementTake,
    CancelProvide,
    AbortTransaction,
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct MarketState {
    pub account_flags: u64,
    pub own_address: Pubkey,
    pub caller_authority: Pubkey, // The program that consumes the event queue via CPIs
    pub event_queue: Pubkey,
    pub bids: Pubkey,
    pub asks: Pubkey,
    pub market_authority: Pubkey, // The authority for disabling the market
}

////////////////////////////////////////////////////
// Events
//TODO refactor eventviews, remove bitflags

#[derive(Copy, Clone, BitFlags, Debug)]
#[repr(u8)]
enum EventFlag {
    Fill = 0x1,
    Out = 0x2,
    Bid = 0x4,
    Maker = 0x8,
    ReleaseFunds = 0x10,
}

pub struct Event {
    event_flags: u8,
    owner: Pubkey,
    native_qty_released: u64,
    native_qty_paid: u64,
}

impl EventFlag {
    fn from_side(side: Side) -> BitFlags<Self> {
        match side {
            Side::Bid => EventFlag::Bid.into(),
            Side::Ask => BitFlags::empty(),
        }
    }

    fn flags_to_side(flags: BitFlags<Self>) -> Side {
        if flags.contains(EventFlag::Bid) {
            Side::Bid
        } else {
            Side::Ask
        }
    }
}

impl Event {
    #[inline(always)]
    pub fn new(view: EventView) -> Self {
        match view {
            EventView::Fill {
                side,
                maker,
                native_qty_paid,
                native_qty_received,
                owner,
            } => {
                let maker_flag = if maker {
                    BitFlags::from_flag(EventFlag::Maker).bits()
                } else {
                    0
                };
                let event_flags =
                    (EventFlag::from_side(side) | EventFlag::Fill).bits() | maker_flag;
                Event {
                    event_flags,
                    native_qty_released: native_qty_received,
                    native_qty_paid,
                    owner,
                }
            }

            EventView::Out {
                side,
                release_funds,
                native_qty_unlocked,
                native_qty_still_locked,
                owner,
            } => {
                let release_funds_flag = if release_funds {
                    BitFlags::from_flag(EventFlag::ReleaseFunds).bits()
                } else {
                    0
                };
                let event_flags =
                    (EventFlag::from_side(side) | EventFlag::Out).bits() | release_funds_flag;
                Event {
                    event_flags,
                    native_qty_released: native_qty_unlocked,
                    native_qty_paid: native_qty_still_locked,
                    owner,
                }
            }
        }
    }
}

pub enum EventView {
    Fill {
        side: Side,
        maker: bool,
        native_qty_paid: u64,
        native_qty_received: u64,
        owner: Pubkey,
    },
    Out {
        side: Side,
        release_funds: bool,
        native_qty_unlocked: u64,
        native_qty_still_locked: u64,
        owner: Pubkey,
    },
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
