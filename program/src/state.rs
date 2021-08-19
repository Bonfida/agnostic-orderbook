use borsh::{BorshDeserialize, BorshSerialize};
use enumflags2::BitFlags;
use solana_program::pubkey::Pubkey;
use std::{cell::RefCell, mem::size_of, rc::Rc};

#[derive(BorshDeserialize, BorshSerialize)]
pub enum AccountFlag {
    Initialized,
    Market,
    EventQueue,
    Bids,
    Asks,
    Disabled,
    Permissioned,
}

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub enum Side {
    Bid,
    Ask,
}

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub enum SelfTradeBehavior {
    DecrementTake,
    CancelProvide,
    AbortTransaction,
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct MarketState {
    pub account_flags: AccountFlag,
    pub caller_authority: Pubkey, // The program that consumes the event queue via CPIs
    pub event_queue: Pubkey,
    pub bids: Pubkey,
    pub asks: Pubkey,
    pub market_authority: Pubkey, // The authority for disabling the market //TODO make caller
                                  //TODO cranked_accs
}

// Holds the results of a new_order transaction for the caller to receive
pub struct RequestProceeds {
    pub native_pc_unlocked: u64,

    pub coin_credit: u64,
    pub native_pc_credit: u64,

    pub coin_debit: u64,
    pub native_pc_debit: u64,
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

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy)]
pub struct Event {
    event_flags: u8,
    owner: Pubkey,
    order_id: u128, // Is composed of price and unique id, acts as key in critbit
    native_qty_released: u64,
    native_qty_paid: u64,
}

const EVENT_LEN: usize = size_of::<Event>();

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
                order_id,
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
                    order_id,
                    native_qty_released: native_qty_received,
                    native_qty_paid,
                    owner,
                }
            }

            EventView::Out {
                side,
                order_id,
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
                    order_id,
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
    //TODO comment
    Fill {
        side: Side,
        maker: bool,
        order_id: u128,
        native_qty_paid: u64,
        native_qty_received: u64,
        owner: Pubkey,
    },
    Out {
        side: Side,
        release_funds: bool,
        order_id: u128,
        native_qty_unlocked: u64, //TODO rename
        native_qty_still_locked: u64,
        owner: Pubkey,
    },
}

////////////////////////////////////////////////////
// Event Queue

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy)]
pub struct EventQueueHeader {
    account_flags: u64, // Initialized, EventQueue
    head: u64,
    count: u64,
    seq_num: u64, //TODO needed?
}
pub const EVENT_QUEUE_HEADER_LEN: usize = size_of::<EventQueueHeader>();

pub struct EventQueue<'a> {
    // The event queue account contains a serialized header
    // and a circular buffer of serialized events
    pub(crate) header: EventQueueHeader,
    pub(crate) buffer: Rc<RefCell<&'a mut [u8]>>, //The whole account data
}

impl EventQueue<'_> {
    pub fn get_buf_len(&self) -> usize {
        self.buffer.borrow().len() - EVENT_QUEUE_HEADER_LEN
    }

    pub fn full(&self) -> bool {
        self.header.count as usize == (self.get_buf_len() / EVENT_LEN)
        //TODO check
    }

    pub fn push_back(&mut self, event: Event) -> Result<(), Event> {
        if self.full() {
            return Err(event);
        }
        let offset = EVENT_QUEUE_HEADER_LEN
            + ((self.header.head + self.header.count * EVENT_LEN as u64) as usize)
                % self.get_buf_len();
        let mut queue_event_data = &mut self.buffer.borrow_mut()[offset..offset + EVENT_LEN];
        event.serialize(&mut queue_event_data).unwrap();

        self.header.count += 1;
        self.header.seq_num += 1;

        Ok(())
    }

    pub fn peek_front(&self) -> Option<Event> {
        if self.header.count == 0 {
            return None;
        }
        let offset = EVENT_QUEUE_HEADER_LEN + self.header.head as usize;
        let mut event_data = &self.buffer.borrow()[offset..offset + EVENT_LEN];
        Some(Event::deserialize(&mut event_data).unwrap())
    }

    pub fn pop_front(&mut self) -> Result<Event, ()> {
        if self.header.count == 0 {
            return Err(());
        }
        let offset = EVENT_QUEUE_HEADER_LEN + self.header.head as usize;
        let mut event_data = &self.buffer.borrow()[offset..offset + EVENT_LEN];
        let event = Event::deserialize(&mut event_data).unwrap();

        self.header.count -= 1;
        self.header.head = (self.header.head + 1) % self.get_buf_len() as u64;

        Ok(event)
    }

    pub fn pop_n(&mut self, number_of_entries_to_pop: u64) {
        let capped_number_of_entries_to_pop =
            std::cmp::min(self.header.count, number_of_entries_to_pop);
        self.header.count -= capped_number_of_entries_to_pop;
        self.header.head =
            (self.header.head + capped_number_of_entries_to_pop) % self.get_buf_len() as u64;
    }

    // #[inline]
    // pub fn revert_pushes(&mut self, desired_len: u64) -> DexResult<()> {
    //     check_assert!(desired_len <= self.header.count())?;
    //     let len_diff = self.header.count() - desired_len;
    //     self.header.set_count(desired_len);
    //     self.header.decr_event_id(len_diff);
    //     Ok(())
    // }

    // pub fn iter(&self) -> impl Iterator<Item = &H::Item> {
    //     QueueIterator {
    //         queue: self,
    //         index: 0,
    //     }
    // }
}
