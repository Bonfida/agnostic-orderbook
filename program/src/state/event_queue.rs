//! The event queue is one of two information outputs of the AOB.
//!
//! It provides a linear event-based interface to propagate the matching and modifying of orders
//! to other use-case specific data structures. It is essential to bypass the need for predicting
//! an instruction's required account beforehand : the runtime can freely decide which users to
//! match together this way.
use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::{CheckedBitPattern, NoUninit, Pod, Zeroable};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use solana_program::{entrypoint::ProgramResult, msg, program_error::ProgramError};

pub use crate::state::orderbook::{OrderSummary, ORDER_SUMMARY_SIZE};
pub use crate::utils::get_spread;

use super::{AccountTag, Side};

#[derive(Clone, Zeroable, Pod, Copy, Debug, PartialEq)]
#[repr(C)]
/// Represents an order being filled, a match between two parties.
pub struct FillEvent {
    /// The u8 representation for an [`AccountTag`] enum
    pub tag: u8,
    /// The u8 representation for a [`Side`] enum
    pub taker_side: u8,
    pub(crate) _padding: [u8; 6],
    /// The total quote size of the transaction
    pub quote_size: u64,
    /// The order id of the maker order
    pub maker_order_id: u128,
    /// The total base size of the transaction
    pub base_size: u64,
}

impl FillEvent {
    /// Byte length of the FillEvent object
    pub const LEN: usize = std::mem::size_of::<Self>();
}

#[derive(Clone, Zeroable, Pod, Copy, Debug, PartialEq)]
#[repr(C)]
/// Represents an order being modified or yanked from the orderbook without being matched
pub struct OutEvent {
    /// The u8 representation for an [`AccountTag`] enum
    pub tag: u8,
    /// The u8 representation for a [`Side`] enum
    pub side: u8,
    /// The total quote size of the transaction
    pub delete: u8,
    pub(crate) _padding: [u8; 13],
    /// The order id of the maker order
    pub order_id: u128,
    /// The total base size of the transaction
    pub base_size: u64,
}

#[derive(PartialEq, Debug)]
/// An unmutable reference to an event in the EventQueue
pub enum EventRef<'a, C> {
    #[allow(missing_docs)]
    Fill(FillEventRef<'a, C>),
    #[allow(missing_docs)]
    Out(OutEventRef<'a, C>),
}

#[derive(PartialEq, Debug)]
/// An immutable reference to a Fill event in the EventQueue, as well as the associated callback information.
pub struct FillEventRef<'a, C> {
    #[allow(missing_docs)]
    pub event: &'a FillEvent,
    #[allow(missing_docs)]
    pub maker_callback_info: &'a C,
    #[allow(missing_docs)]
    pub taker_callback_info: &'a C,
}

#[derive(PartialEq, Debug)]
/// An immutable reference to an Out event in the EventQueue, as well as the associated callback information.
pub struct OutEventRef<'a, C> {
    #[allow(missing_docs)]
    pub event: &'a OutEvent,
    #[allow(missing_docs)]
    pub callback_info: &'a C,
}

#[derive(FromPrimitive, Clone, Copy, CheckedBitPattern, NoUninit)]
#[repr(u8)]
pub(crate) enum EventTag {
    Fill,
    Out,
}

pub(crate) type GenericEvent = FillEvent;

pub(crate) trait Event {
    fn to_generic(&mut self) -> &GenericEvent;
}

impl Event for FillEvent {
    fn to_generic(&mut self) -> &GenericEvent {
        self.tag = EventTag::Fill as u8;
        self
    }
}

impl Event for OutEvent {
    fn to_generic(&mut self) -> &GenericEvent {
        self.tag = EventTag::Out as u8;
        bytemuck::cast_ref(self)
    }
}

////////////////////////////////////////////////////
// Event Queue

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
/// Describes the current state of the event queue
pub struct EventQueueHeader {
    /// The current event
    pub head: u64,
    /// The current event queue length
    pub count: u64,
    seq_num: u64,
}

impl EventQueueHeader {
    /// The byte size for the EventQueueHeader object
    pub const LEN: usize = std::mem::size_of::<Self>();
}

/// The event queue account contains a serialized header, a register
/// and a circular buffer of serialized events.
///
/// This struct is used at runtime but doesn't represent a serialized event queue
pub struct EventQueue<'a, C> {
    pub(crate) header: &'a mut EventQueueHeader,
    pub(crate) events: &'a mut [FillEvent],
    pub(crate) callback_infos: &'a mut [C],
}

impl<'queue, C: Pod> EventQueue<'queue, C> {
    /// Instantiates an event queue object from an account's buffer
    pub fn from_buffer(
        buf: &'queue mut [u8],
        expected_tag: AccountTag,
    ) -> Result<Self, ProgramError> {
        let callback_info_len = std::mem::size_of::<C>();

        let capacity =
            (buf.len() - 8 - EventQueueHeader::LEN) / (FillEvent::LEN + 2 * callback_info_len);
        let account_tag: &mut u64 = bytemuck::from_bytes_mut(&mut buf[0..8]);

        if *account_tag != expected_tag as u64 {
            return Err(ProgramError::InvalidAccountData);
        }
        *account_tag = AccountTag::EventQueue as u64;

        let (header, remaining) = buf[8..].split_at_mut(EventQueueHeader::LEN);

        let (events, callback_infos) = remaining.split_at_mut(capacity * FillEvent::LEN);
        Ok(Self {
            header: bytemuck::from_bytes_mut(header),
            events: bytemuck::cast_slice_mut(events),
            callback_infos: bytemuck::cast_slice_mut(callback_infos),
        })
    }
}

impl<'queue, C: Clone> EventQueue<'queue, C> {
    pub(crate) fn push_back<Ev: Event>(
        &mut self,
        mut event: Ev,
        maker_callback_info: Option<&C>,
        taker_callback_info: Option<&C>,
    ) -> Result<(), Ev> {
        if self.full() {
            return Err(event);
        }
        let generic_event = event.to_generic();
        let event_idx =
            (self.header.head as usize + self.header.count as usize) % self.events.len();
        self.events[event_idx] = *generic_event;

        self.header.count += 1;

        if let Some(c) = maker_callback_info {
            self.callback_infos[event_idx * 2] = c.clone();
        }

        if let Some(c) = taker_callback_info {
            self.callback_infos[event_idx * 2 + 1] = c.clone();
        }

        Ok(())
    }
}

impl<'queue, C> EventQueue<'queue, C> {
    /// Compute the allocation size for an event queue of a desired capacity
    pub fn compute_allocation_size(desired_event_capacity: usize) -> usize {
        desired_event_capacity * (FillEvent::LEN + 2 * std::mem::size_of::<C>())
            + EventQueueHeader::LEN
            + 8
    }

    pub(crate) fn check_buffer_size(buffer: &[u8]) -> ProgramResult {
        const HEADER_OFFSET: usize = EventQueueHeader::LEN + 8;
        let event_size: usize = FillEvent::LEN + 2 * std::mem::size_of::<C>();
        let account_len = buffer.len();
        if account_len < HEADER_OFFSET + 5 * event_size {
            msg!("The event queue account is too small!");
            return Err(ProgramError::InvalidAccountData);
        }
        if (account_len - HEADER_OFFSET) % event_size != 0 {
            msg!("Event queue account size is invalid!");
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(())
    }

    pub(crate) fn gen_order_id(&mut self, limit_price: u64, side: Side) -> u128 {
        let seq_num = self.gen_seq_num();
        let upper = (limit_price as u128) << 64;
        let lower = match side {
            Side::Bid => !seq_num,
            Side::Ask => seq_num,
        };
        upper | (lower as u128)
    }

    fn gen_seq_num(&mut self) -> u64 {
        let seq_num = self.header.seq_num;
        self.header.seq_num += 1;
        seq_num
    }

    pub(crate) fn full(&self) -> bool {
        self.header.count as usize == self.events.len()
    }

    /// Retrieves the event at position `index` in the queue.
    pub fn peek_at(&self, index: u64) -> Option<EventRef<'_, C>> {
        if self.header.count <= index {
            return None;
        }

        let event_idx = (self.header.head.checked_add(index).unwrap() as usize) % self.events.len();
        Some(self.get_event(event_idx))
    }

    fn get_event(&self, event_idx: usize) -> EventRef<'_, C> {
        let event = &self.events[event_idx];
        match EventTag::from_u8(event.tag).unwrap() {
            EventTag::Fill => EventRef::Fill(FillEventRef {
                event,
                maker_callback_info: &self.callback_infos[2 * event_idx],
                taker_callback_info: &self.callback_infos[2 * event_idx + 1],
            }),
            EventTag::Out => EventRef::Out(OutEventRef {
                event: bytemuck::cast_ref(event),
                callback_info: &self.callback_infos[2 * event_idx],
            }),
        }
    }

    /// Pop n entries from the event queue
    pub fn pop_n(&mut self, number_of_entries_to_pop: u64) {
        let capped_number_of_entries_to_pop =
            std::cmp::min(self.header.count, number_of_entries_to_pop);
        self.header.count -= capped_number_of_entries_to_pop;
        self.header.head =
            (self.header.head + capped_number_of_entries_to_pop) % (self.events.len() as u64);
    }

    /// Returns an iterator over all the queue's events
    pub fn iter(&self) -> QueueIterator<'_, C> {
        QueueIterator {
            queue: self,
            current_index: 0,
            remaining: self.header.count,
        }
    }

    /// Checks whether the event queue is currently empty
    pub fn is_empty(&self) -> bool {
        self.header.count == 0
    }

    /// Returns the current length of the event queue
    pub fn len(&self) -> u64 {
        self.header.count
    }
}

/// Utility struct for iterating over a queue
pub struct QueueIterator<'a, C> {
    queue: &'a EventQueue<'a, C>,
    current_index: usize,
    remaining: u64,
}

impl<'a, C> Iterator for QueueIterator<'a, C> {
    type Item = EventRef<'a, C>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let event_idx =
            (self.queue.header.head as usize + self.current_index) % self.queue.events.len();
        self.current_index += 1;
        self.remaining -= 1;
        Some(self.queue.get_event(event_idx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type EventQueueTest<'a> = EventQueue<'a, [u8; 32]>;

    #[test]
    fn test_event_queue_0() {
        let allocation_size = EventQueue::<[u8; 32]>::compute_allocation_size(100);
        let mut buffer = vec![0; allocation_size];

        assert!(EventQueueTest::from_buffer(&mut buffer, AccountTag::EventQueue).is_err());

        assert!(EventQueueTest::check_buffer_size(&[0; 10]).is_err());
        assert!(EventQueueTest::check_buffer_size(&[0; 1000]).is_err());

        let mut event_queue =
            EventQueueTest::from_buffer(&mut buffer, AccountTag::Uninitialized).unwrap();

        let mut seq_gen = 0..;
        let mut parity_gen = 0..;

        for _ in 0..100 {
            if parity_gen.next().unwrap() % 7 != 3 {
                event_queue
                    .push_back(
                        FillEvent {
                            tag: EventTag::Fill as u8,
                            taker_side: Side::Ask as u8,
                            _padding: [0; 6],
                            quote_size: seq_gen.next().unwrap(),
                            maker_order_id: seq_gen.next().unwrap() as u128,
                            base_size: seq_gen.next().unwrap(),
                        },
                        Some(&[seq_gen.next().unwrap() as u8; 32]),
                        Some(&[seq_gen.next().unwrap() as u8; 32]),
                    )
                    .unwrap();
            } else {
                event_queue
                    .push_back(
                        OutEvent {
                            tag: EventTag::Out as u8,
                            side: Side::Ask as u8,
                            _padding: [0; 13],
                            base_size: seq_gen.next().unwrap(),
                            delete: true as u8,
                            order_id: seq_gen.next().unwrap() as u128,
                        },
                        Some(&[seq_gen.next().unwrap() as u8; 32]),
                        None,
                    )
                    .unwrap();
            }
        }
        let extra_event = FillEvent {
            tag: EventTag::Fill as u8,
            taker_side: Side::Ask as u8,
            _padding: [0; 6],
            quote_size: seq_gen.next().unwrap(),
            maker_order_id: seq_gen.next().unwrap() as u128,
            base_size: seq_gen.next().unwrap(),
        };
        assert_eq!(
            extra_event,
            event_queue.push_back(extra_event, None, None).unwrap_err()
        );
        let mut number_of_events = 0;
        let mut seq_gen = 0..;
        let mut parity_gen = 0..;

        assert!(event_queue.peek_at(100).is_none());

        for (i, e) in event_queue.iter().enumerate() {
            let is_fill = parity_gen.next().unwrap() % 7 != 3;
            match e {
                EventRef::Out(o) => {
                    assert!(!is_fill);
                    assert_eq!(
                        o,
                        OutEventRef {
                            event: &OutEvent {
                                tag: EventTag::Out as u8,
                                side: Side::Ask as u8,
                                _padding: [0; 13],
                                base_size: seq_gen.next().unwrap(),
                                delete: true as u8,
                                order_id: seq_gen.next().unwrap() as u128,
                            },
                            callback_info: &[seq_gen.next().unwrap() as u8; 32]
                        }
                    );
                }
                EventRef::Fill(e) => {
                    assert!(is_fill);
                    assert_eq!(
                        e,
                        FillEventRef {
                            event: &FillEvent {
                                tag: EventTag::Fill as u8,
                                taker_side: Side::Ask as u8,
                                _padding: [0; 6],
                                quote_size: seq_gen.next().unwrap(),
                                maker_order_id: seq_gen.next().unwrap() as u128,
                                base_size: seq_gen.next().unwrap(),
                            },
                            maker_callback_info: &[seq_gen.next().unwrap() as u8; 32],
                            taker_callback_info: &[seq_gen.next().unwrap() as u8; 32]
                        }
                    );
                    assert_eq!(EventRef::Fill(e), event_queue.peek_at(i as u64).unwrap());
                }
            }
            number_of_events = i + 1;
        }
        assert_eq!(number_of_events, 100);
    }
}
