use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::{CheckedBitPattern, NoUninit, Pod, Zeroable};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program_error::ProgramError,
};

pub use crate::state::orderbook::{OrderSummary, ORDER_SUMMARY_SIZE};
#[cfg(feature = "no-entrypoint")]
pub use crate::utils::get_spread;

use super::{AccountTag, Side};

#[derive(Zeroable, Clone, Pod, Copy)]
#[repr(C)]
pub struct FillEvent {
    pub(crate) tag: u8,
    #[allow(missing_docs)]
    pub(crate) taker_side: u8,
    pub(crate) _padding: [u8; 6],
    /// The total quote size of the transaction
    pub(crate) quote_size: u64,
    /// The order id of the maker order
    pub(crate) maker_order_id: u128,
    /// The total base size of the transaction
    pub(crate) base_size: u64,
}

impl FillEvent {
    pub const LEN: usize = std::mem::size_of::<Self>();
}

#[derive(Clone, CheckedBitPattern, NoUninit, Copy)]
#[repr(C)]
pub struct OutEvent {
    pub(crate) tag: EventTag,
    #[allow(missing_docs)]
    pub(crate) side: Side,
    /// The total quote size of the transaction
    pub(crate) delete: bool,
    pub(crate) _padding: [u8; 13],
    /// The order id of the maker order
    pub(crate) order_id: u128,
    /// The total base size of the transaction
    pub(crate) base_size: u64,
}

pub enum EventRef<'a> {
    Fill(&'a FillEvent),
    Out(&'a OutEvent),
}

#[derive(FromPrimitive, Clone, Copy, CheckedBitPattern, NoUninit)]
#[repr(u8)]
pub enum EventTag {
    Fill,
    Out,
}

impl<'a> EventRef<'a> {
    pub(crate) fn from_event(event: &'a FillEvent) -> Self {
        match EventTag::from_u8(event.tag).unwrap() {
            EventTag::Fill => EventRef::Fill(event),
            EventTag::Out => EventRef::Out(bytemuck::checked::cast_ref(event)),
        }
    }
}

pub type GenericEvent = FillEvent;

pub trait Event {
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
        self.tag = EventTag::Out;
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
        let event_idx = (self.header.count as usize) % self.events.len();
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

    pub(crate) fn check_buffer_size(account: &AccountInfo) -> ProgramResult {
        const HEADER_OFFSET: usize = EventQueueHeader::LEN + 8;
        let event_size: usize = FillEvent::LEN + 2 * std::mem::size_of::<C>();
        let account_len = account.data.borrow().len();
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

    /// Retrieves the event at position index in the queue.
    pub fn peek_at(&self, index: u64) -> Option<EventRef<'_>> {
        if self.header.count <= index {
            return None;
        }

        let event_idx = (self.header.head.checked_add(index).unwrap() as usize) % self.events.len();
        let event = &self.events[event_idx];
        Some(EventRef::from_event(event))
    }

    #[doc(hidden)]
    /// Pop n entries from the event queue
    pub fn pop_n(&mut self, number_of_entries_to_pop: u64) {
        let capped_number_of_entries_to_pop =
            std::cmp::min(self.header.count, number_of_entries_to_pop);
        self.header.count -= capped_number_of_entries_to_pop;
        self.header.head =
            (self.header.head + capped_number_of_entries_to_pop) % (self.events.len() as u64);
    }

    /// Returns an iterator over all the queue's events
    #[cfg(feature = "no-entrypoint")]
    pub fn iter(&self) -> QueueIterator<'_, C> {
        QueueIterator {
            queue: self,
            current_index: self.header.head as usize,
            remaining: self.header.count,
        }
    }
}

#[cfg(feature = "no-entrypoint")]
/// Utility struct for iterating over a queue
pub struct QueueIterator<'a, C> {
    queue: &'a EventQueue<'a, C>,
    current_index: usize,
    remaining: u64,
}

#[cfg(feature = "no-entrypoint")]
impl<'a, C> Iterator for QueueIterator<'a, C> {
    type Item = EventRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let event = &self.queue.events[self.queue.header.head as usize + self.current_index];
        self.current_index = (self.current_index + 1) % self.queue.events.len();
        self.remaining -= 1;
        Some(EventRef::from_event(event))
    }
}
