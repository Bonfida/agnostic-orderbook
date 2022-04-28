use bonfida_utils::BorshSize;
use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::{try_from_bytes_mut, Pod, Zeroable};
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program_error::ProgramError,
};
use std::{
    cell::{RefCell, RefMut},
    convert::TryInto,
    io::Write,
    mem::size_of,
    rc::Rc,
};

use crate::critbit::IoError;

pub use crate::orderbook::{OrderSummary, ORDER_SUMMARY_SIZE};
#[cfg(feature = "no-entrypoint")]
pub use crate::utils::get_spread;

#[derive(BorshDeserialize, BorshSerialize, Copy, Clone, Debug, PartialEq)]
#[allow(missing_docs)]
#[repr(u8)]
/// Warning: the account tags are bitshifted to allow for standard tag usage in the program using the aob.
pub enum AccountTag {
    Uninitialized = 1 << 7,
    Market,
    EventQueue,
    Bids,
    Asks,
}

#[derive(
    BorshDeserialize,
    BorshSerialize,
    Clone,
    Copy,
    PartialEq,
    FromPrimitive,
    ToPrimitive,
    Debug,
    BorshSize,
)]
#[repr(u8)]
#[allow(missing_docs)]
pub enum Side {
    Bid,
    Ask,
}

impl Side {
    /// Helper function to get the opposite side.
    pub fn opposite(&self) -> Self {
        match self {
            Side::Bid => Side::Ask,
            Side::Ask => Side::Bid,
        }
    }
}

#[derive(BorshDeserialize, BorshSerialize, Clone, PartialEq, FromPrimitive, BorshSize)]
/// Describes what happens when two order with identical callback informations are matched together
pub enum SelfTradeBehavior {
    /// The orders are matched together
    DecrementTake,
    /// The order on the provide side is cancelled. Matching for the current order continues and essentially bypasses
    /// the self-provided order.
    CancelProvide,
    /// The entire transaction fails and the program returns an error.
    AbortTransaction,
}

#[derive(Debug, Copy, Clone, Pod, Zeroable)]
#[repr(C)]
/// The orderbook market's central state
pub struct MarketState {
    /// Identifies the account as a [`MarketState`] object.
    pub tag: u64,
    /// The required signer for all market operations.
    pub caller_authority: [u8; 32],
    /// The public key of the orderbook's event queue account
    pub event_queue: [u8; 32],
    /// The public key of the orderbook's bids account
    pub bids: [u8; 32],
    /// The public key of the orderbook's asks account
    pub asks: [u8; 32],
    /// The length of an order actor's callback identifier.
    pub callback_id_len: u64,
    /// The length of an order's callback metadata.
    pub callback_info_len: u64,
    /// The current budget of fees that have been collected.
    /// Cranker rewards are taken from this. This value allows
    /// for a verification that the fee was payed in the caller program
    /// runtime while not having to add a CPI call to the serum-core.
    pub fee_budget: u64,
    /// The amount of lamports the market account was created with.
    pub initial_lamports: u64,
    /// The minimum order size that can be inserted into the orderbook after matching.
    pub min_base_order_size: u64,
    /// Tick size (FP32)
    pub tick_size: u64,
    /// Cranker reward (in lamports)
    pub cranker_reward: u64,
}

/// Expected size in bytes of MarketState
pub const MARKET_STATE_LEN: usize = size_of::<MarketState>();

impl MarketState {
    #[allow(missing_docs)]
    pub fn get<'a, 'b: 'a>(
        account_info: &'a AccountInfo<'b>,
    ) -> Result<RefMut<'a, Self>, ProgramError> {
        let a = Self::get_unchecked(account_info);
        if a.tag != AccountTag::Market as u64 {
            return Err(ProgramError::InvalidAccountData);
        };
        Ok(a)
    }

    #[allow(missing_docs)]
    pub fn get_unchecked<'a, 'b: 'a>(account_info: &'a AccountInfo<'b>) -> RefMut<'a, Self> {
        let a = RefMut::map(account_info.data.borrow_mut(), |s| {
            try_from_bytes_mut::<Self>(&mut s[0..MARKET_STATE_LEN]).unwrap()
        });
        a
    }
}

////////////////////////////////////////////////////
// Events
#[derive(BorshDeserialize, BorshSerialize, Debug)]
/// Events are the primary output of the asset agnostic orderbook
pub enum Event {
    /// A fill event describes a match between a taker order and a provider order
    Fill {
        #[allow(missing_docs)]
        taker_side: Side,
        /// The order id of the maker order
        maker_order_id: u128,
        /// The total quote size of the transaction
        quote_size: u64,
        /// The total base size of the transaction
        base_size: u64,
        /// The callback information for the maker
        maker_callback_info: Vec<u8>,
        /// The callback information for the taker
        taker_callback_info: Vec<u8>,
    },
    /// An out event describes an order which has been taken out of the orderbook
    Out {
        #[allow(missing_docs)]
        side: Side,
        #[allow(missing_docs)]
        order_id: u128,
        #[allow(missing_docs)]
        base_size: u64,
        #[allow(missing_docs)]
        delete: bool,
        #[allow(missing_docs)]
        callback_info: Vec<u8>,
    },
}

impl Event {
    /// Used to serialize an event object into a generic byte writer.
    pub fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), IoError> {
        match self {
            Event::Fill {
                taker_side,
                maker_order_id,
                quote_size,
                base_size,
                maker_callback_info,
                taker_callback_info,
            } => {
                writer.write_all(&[0])?;
                writer.write_all(&[taker_side.to_u8().unwrap()])?;
                writer.write_all(&maker_order_id.to_le_bytes())?;
                writer.write_all(&quote_size.to_le_bytes())?;
                writer.write_all(&base_size.to_le_bytes())?;
                writer.write_all(maker_callback_info)?;
                writer.write_all(taker_callback_info)?;
            }
            Event::Out {
                side,
                order_id,
                base_size,
                delete,
                callback_info,
            } => {
                writer.write_all(&[1])?;
                writer.write_all(&[side.to_u8().unwrap()])?;
                writer.write_all(&order_id.to_le_bytes())?;
                writer.write_all(&base_size.to_le_bytes())?;
                writer.write_all(&[(*delete as u8)])?;
                writer.write_all(callback_info)?;
            }
        };
        Ok(())
    }

    /// Used to deserialize an event object from bytes.
    pub fn deserialize(buf: &mut &[u8], callback_info_len: usize) -> Self {
        match buf[0] {
            0 => Event::Fill {
                taker_side: Side::from_u8(buf[1]).unwrap(),
                maker_order_id: u128::from_le_bytes(buf[2..18].try_into().unwrap()),
                quote_size: u64::from_le_bytes(buf[18..26].try_into().unwrap()),
                base_size: u64::from_le_bytes(buf[26..34].try_into().unwrap()),
                maker_callback_info: buf[34..34 + callback_info_len].to_owned(),
                taker_callback_info: buf[34 + callback_info_len..34 + (callback_info_len << 1)]
                    .to_owned(),
            },
            1 => Event::Out {
                side: Side::from_u8(buf[1]).unwrap(),
                order_id: u128::from_le_bytes(buf[2..18].try_into().unwrap()),
                base_size: u64::from_le_bytes(buf[18..26].try_into().unwrap()),
                delete: buf[26] == 1,
                callback_info: buf[27..27 + callback_info_len].to_owned(),
            },
            _ => unreachable!(),
        }
    }

    /// An event queue is divided into slots. The size of these slots depend on the particular market's `callback_info_len` constant.
    pub fn compute_slot_size(callback_info_len: usize) -> usize {
        1 + 33 + 2 * callback_info_len
    }
}

////////////////////////////////////////////////////
// Event Queue

#[derive(BorshDeserialize, BorshSerialize, Clone)]
/// Describes the current state of the event queue
pub struct EventQueueHeader {
    tag: AccountTag, // Initialized, EventQueue
    /// The current event
    pub head: u64,
    /// The current event queue length
    pub count: u64,
    event_size: u64,
    seq_num: u64,
}
#[allow(missing_docs)]
pub const EVENT_QUEUE_HEADER_LEN: usize = 33;
#[allow(missing_docs)]
pub const REGISTER_SIZE: usize = ORDER_SUMMARY_SIZE as usize + 1; // Option<OrderSummary>

impl EventQueueHeader {
    #[doc(hidden)]
    pub fn initialize(callback_info_len: usize) -> Self {
        Self {
            tag: AccountTag::EventQueue,
            head: 0,
            count: 0,
            event_size: Event::compute_slot_size(callback_info_len) as u64,
            seq_num: 0,
        }
    }

    pub(crate) fn check(self) -> Result<Self, ProgramError> {
        if self.tag != AccountTag::EventQueue {
            return Err(ProgramError::InvalidAccountData);
        };
        Ok(self)
    }
}

/// The event queue account contains a serialized header, a register
/// and a circular buffer of serialized events.
///
/// This struct is used at runtime but doesn't represent a serialized event queue
pub struct EventQueue<'a> {
    #[doc(hidden)]
    pub header: EventQueueHeader,
    pub(crate) buffer: Rc<RefCell<&'a mut [u8]>>, //The whole account data
    callback_info_len: usize,
}

/// The event queue register can hold arbitrary data returned by the AAOB. Currently only used to return [`OrderSummary`] objects.
pub type Register<T> = Option<T>;

impl<'a> EventQueue<'a> {
    pub(crate) fn new_safe(
        header: EventQueueHeader,
        account: &AccountInfo<'a>,
        callback_info_len: usize,
    ) -> Result<Self, ProgramError> {
        let q = Self {
            header: header.check()?,
            buffer: Rc::clone(&account.data),
            callback_info_len,
        };
        q.clear_register();
        Ok(q)
    }

    /// Initialize a new EventQueue object.
    ///
    /// Within a CPI context, the account parameter can be supplied through
    /// ```ignore
    /// use std::rc::Rc;
    /// let a: AccountInfo;
    ///
    /// let event_queue_header = EventQueueHeader::deserialize(&mut &a.data.borrow()[..EVENT_QUEUE_HEADER_LEN]).unwrap();
    /// let event_queue = EventQueue::new(event_queue_header, Rc::clone(&a.data), callback_info_len);
    ///
    /// ```
    pub fn new(
        header: EventQueueHeader,
        account: Rc<RefCell<&'a mut [u8]>>,
        callback_info_len: usize,
    ) -> Self {
        Self {
            header,
            buffer: account,
            callback_info_len,
        }
    }

    /// Load a new EventQueue object from a bytes buffer
    pub fn from_buffer(
        header: EventQueueHeader,
        buffer: &'a mut [u8],
        callback_info_len: usize,
    ) -> Self {
        Self {
            header,
            buffer: Rc::new(RefCell::new(buffer)),
            callback_info_len,
        }
    }

    /// Compute the allocation size for an event queue of a desired capacity
    pub fn compute_allocation_size(
        desired_event_capacity: usize,
        callback_info_len: usize,
    ) -> usize {
        desired_event_capacity * Event::compute_slot_size(callback_info_len)
            + EVENT_QUEUE_HEADER_LEN
    }
}

impl<'a> EventQueue<'a> {
    pub(crate) fn check_buffer_size(
        account: &AccountInfo,
        callback_info_len: u64,
    ) -> ProgramResult {
        let event_size = Event::compute_slot_size(callback_info_len as usize);
        if (account.data_len() - EVENT_QUEUE_HEADER_LEN - REGISTER_SIZE) % event_size != 0 {
            msg!("Event queue buffer size must be a multiple of the event size");
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

    pub(crate) fn get_buf_len(&self) -> usize {
        self.buffer.borrow().len() - EVENT_QUEUE_HEADER_LEN - REGISTER_SIZE
    }

    pub(crate) fn full(&self) -> bool {
        self.header.count as usize == (self.get_buf_len() / (self.header.event_size as usize))
    }

    pub(crate) fn push_back(&mut self, event: Event) -> Result<(), Event> {
        if self.full() {
            return Err(event);
        }
        let offset = EVENT_QUEUE_HEADER_LEN
            + (REGISTER_SIZE)
            + (((self.header.head + self.header.count * self.header.event_size) as usize)
                % self.get_buf_len());
        let mut queue_event_data =
            &mut self.buffer.borrow_mut()[offset..offset + (self.header.event_size as usize)];
        event.serialize(&mut queue_event_data).unwrap();

        self.header.count += 1;
        self.header.seq_num += 1;

        Ok(())
    }

    /// Retrieves the event at position index in the queue.
    pub fn peek_at(&self, index: u64) -> Option<Event> {
        if self.header.count <= index {
            return None;
        }

        let header_offset = EVENT_QUEUE_HEADER_LEN + REGISTER_SIZE;
        let offset = ((self
            .header
            .head
            .checked_add(index.checked_mul(self.header.event_size).unwrap())
            .unwrap()) as usize
            % self.get_buf_len())
            + header_offset;
        let mut event_data =
            &self.buffer.borrow()[offset..offset + (self.header.event_size as usize)];
        Some(Event::deserialize(&mut event_data, self.callback_info_len))
    }

    #[doc(hidden)]
    /// Pop n entries from the event queue
    pub fn pop_n(&mut self, number_of_entries_to_pop: u64) {
        let capped_number_of_entries_to_pop =
            std::cmp::min(self.header.count, number_of_entries_to_pop);
        self.header.count -= capped_number_of_entries_to_pop;
        self.header.head = (self.header.head
            + capped_number_of_entries_to_pop * self.header.event_size)
            % self.get_buf_len() as u64;
    }

    pub(crate) fn write_to_register<T: BorshSerialize + BorshDeserialize>(&self, object: T) {
        let mut register = &mut self.buffer.borrow_mut()
            [EVENT_QUEUE_HEADER_LEN..EVENT_QUEUE_HEADER_LEN + (REGISTER_SIZE)];
        Register::Some(object).serialize(&mut register).unwrap();
    }

    pub(crate) fn clear_register(&self) {
        let mut register = &mut self.buffer.borrow_mut()
            [EVENT_QUEUE_HEADER_LEN..EVENT_QUEUE_HEADER_LEN + (REGISTER_SIZE)];
        Register::<u8>::None.serialize(&mut register).unwrap();
    }

    /// This method is used to deserialize the event queue's register
    ///
    /// The nature of the serialized object should be deductible from caller context
    pub fn read_register<T: BorshSerialize + BorshDeserialize>(
        &self,
    ) -> Result<Register<T>, IoError> {
        let mut register =
            &self.buffer.borrow()[EVENT_QUEUE_HEADER_LEN..EVENT_QUEUE_HEADER_LEN + (REGISTER_SIZE)];
        Register::deserialize(&mut register)
    }

    /// Returns an iterator over all the queue's events
    #[cfg(feature = "no-entrypoint")]
    pub fn iter<'b>(&'b self) -> QueueIterator<'a, 'b> {
        QueueIterator {
            queue_header: &self.header,
            buffer: Rc::clone(&self.buffer),
            current_index: self.header.head as usize,
            callback_info_len: self.callback_info_len,
            buffer_length: self.get_buf_len(),
            header_offset: EVENT_QUEUE_HEADER_LEN + REGISTER_SIZE,
            remaining: self.header.count,
        }
    }
}

/// This method is used to deserialize the event queue's register
/// without constructing an EventQueue instance
///
/// The nature of the serialized object should be deductible from caller context
pub fn read_register<T: BorshSerialize + BorshDeserialize>(
    event_q_acc: &AccountInfo,
) -> Result<Register<T>, IoError> {
    let mut register =
        &event_q_acc.data.borrow()[EVENT_QUEUE_HEADER_LEN..EVENT_QUEUE_HEADER_LEN + REGISTER_SIZE];
    Register::deserialize(&mut register)
}

#[cfg(feature = "no-entrypoint")]
impl<'a, 'b> IntoIterator for &'b EventQueue<'a> {
    type Item = Event;

    type IntoIter = QueueIterator<'a, 'b>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
#[cfg(feature = "no-entrypoint")]
/// Utility struct for iterating over a queue
pub struct QueueIterator<'a, 'b> {
    queue_header: &'b EventQueueHeader,
    buffer: Rc<RefCell<&'a mut [u8]>>, //The whole account data
    current_index: usize,
    callback_info_len: usize,
    buffer_length: usize,
    header_offset: usize,
    remaining: u64,
}

#[cfg(feature = "no-entrypoint")]
impl<'a, 'b> Iterator for QueueIterator<'a, 'b> {
    type Item = Event;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let result = Event::deserialize(
            &mut &self.buffer.borrow()[self.header_offset + self.current_index..],
            self.callback_info_len,
        );
        self.current_index =
            (self.current_index + self.queue_header.event_size as usize) % self.buffer_length;
        self.remaining -= 1;
        Some(result)
    }
}

/// This byte flag is set for order_ids with side Bid, and unset for side Ask
pub const ORDER_ID_SIDE_FLAG: u128 = 1 << 63;

/// This helper function deduces an order's side from its order_id
pub fn get_side_from_order_id(order_id: u128) -> Side {
    if ORDER_ID_SIDE_FLAG & order_id != 0 {
        Side::Bid
    } else {
        Side::Ask
    }
}
