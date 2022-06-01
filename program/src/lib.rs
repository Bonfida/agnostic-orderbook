#![warn(missing_docs)]
/*!
Solana orderbook library which can be used with generic assets.

## Overview

This library is intended to enable other programs to implement use-case specific on-chain orderbooks.
These programs can use the agnostic orderbook as an underlying blochain-aware datastructure.

There are two ways to interact with an asset agnostic orderbook :
- creating a new order
- cancelling an existing order

AAOB instructions should be called directly from a program. These can return instantaneous order information as an [`OrderSummary`][`state::OrderSummary`] object.

The AAOB also outputs order matching information through the event queue account.

The AAOB library is also generic over the nature of the callback information to transmit from order creation to order matching.
A custom object can be used, with the only requirements being that it implements the bytemuck [`Pod`][`bytemuck::Pod`], [`PartialEq`],
and [`CallbackInfo`][`state::orderbook::CallbackInfo`] traits.


## Creating an order

The [`new_order`][`fn@instruction::new_order::process`] primitive will push a new order to the orderbook which will optionally match with existing orders if its limit price crosses
the spread. The result of this is a series of matching events pushed to the event queue, as well as the writing of a new order to the orderbook, which will become
immediately available to be matched against other orders. The primitive returns an [`OrderSummary`][`state::OrderSummary`] object, yielding
a unique order identifier which will be valid for the whole lifetime of the order, which is to say until it is completely matched or cancelled (if posted).

More information about different parameters for this primitive can be found [here][`instruction`].

## Cancelling an order

The [`cancel_order`][`fn@instruction::cancel_order::process`] primitive will act on orders which are posted to the orderbook. It will completely erase a posted order
from the orderbook. The instruction only requires the associated `order_id`.

## Processing the queue

On the caller program's side, the queue can be parsed as an [`EventQueue`][`state::event_queue::EventQueue`] object. Its [`peek_at`][`state::event_queue::EventQueue::peek_at`] method can be used
to retrieve particular events. Alternatively, the events can be iterated through with the object's `iter` method.

An [`Event`][`state::event_queue::Event`] object describes matching operations as well as the purging of orders from the orderbook. Information about the matched parties is provided
through the `callback_info` fields. An example of such information would be a user account or user wallet's public key, enabling the caller program to perform a transfer of assets between
those accounts. A prefix of len [`callback_id_len`][`state::market_state::MarketState`] of this information is also used by the program to detect matches which would result in self trading.

Once event processing is over, it is essential to pop the processed events off the queue. This can be done through the [`consume_events`][`fn@instruction::consume_events`]
primitive. In general, the event processing logic should be handled by a dedicated cranker on the caller program's side.
*/

#[doc(hidden)]
pub mod entrypoint;
#[doc(hidden)]
pub mod error;
/// Program instructions and their CPI-compatible bindings
pub mod instruction;
/// Describes the different data structres that the program uses to encode state
pub mod state;

use solana_program::declare_id;

#[doc(hidden)]
pub(crate) mod processor;
/// Utility functions
pub mod utils;

declare_id!("aaobKniTtDGvCZces7GH5UReLYP671bBkB96ahr9x3e");
