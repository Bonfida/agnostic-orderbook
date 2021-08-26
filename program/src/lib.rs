#![warn(missing_docs)]
/*!
Orderbook program which can be used with generic assets.

## Overview

There are two ways to interact with an asset agnostic orderbook :
- creating a new order
- cancelling an existing order

The AAOB program outputs information through the event queue account in of two ways:
- instantaneous order information through the event queue's register (accessible through the [`read_register`][`state::EventQueue::read_register`] primitive).
- the queue itself

## Creating an order

The [`new_order`][`fn@instruction::new_order`] primitive will push a new order to the orderbook which will optionally match with existing orders if its limit price crosses
the spread. The result of this is a series of matching events pushed to the event queue, as well as the writing of a new order to the orderbook, which will become
immediately available to be matched agains other orders. An [`OrderSummary`][`state::OrderSummary`] object is also written to the event queue's register, yielding
a unique order identifier which will be valid for the whole lifetime of the order : until it is completely matched or cancelled (if it posted).

More information about different parameters for this primitive can be found [here][`instruction`].

## Cancelling an order

The [`cancel_order`][`fn@instruction::cancel_order`] primitive will act on orders which are posted to the orderbook. It will completely erase a posted order
from the orderbook. The instruction only requires the `order_id`.

## Processing the queue

On the caller program's side, the queue can be parsed as an [`EventQueue`][`state::EventQueue] object. Its [`peek_at`][`state::EventQueue::peek_at`] method can be used
to retrieve particular events. Alternatively, the events can be iterated through with the object's `iter` method.

Once event processing is over, it is essential to pop the processed events off the queue. This can be done through the [`consume_events`][`fn@instruction::consume_events`]
primitive. In general, the event processing logic should be handled by a dedicated cranker on the caller program's side.
*/

#[doc(hidden)]
pub mod entrypoint;
/// Program instructions and their CPI-compatible bindings
pub mod instruction;
/// Describes the different data structres that the program uses to encode state
pub mod state;

#[doc(hidden)]
pub mod critbit;
#[doc(hidden)]
pub mod error;

pub(crate) mod orderbook;
pub(crate) mod processor;
pub(crate) mod utils;
