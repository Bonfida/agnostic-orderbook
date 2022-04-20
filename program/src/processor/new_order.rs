//! Execute a new order on the orderbook

use bonfida_utils::{BorshSize, InstructionsAccount};
use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::Pod;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::{
    error::AoError,
    state::{
        event_queue::EventQueue,
        market_state::MarketState,
        orderbook::{CallbackInfo, OrderBookState},
        AccountTag, OrderSummary, SelfTradeBehavior, Side,
    },
    utils::{check_account_key, check_account_owner},
};

#[derive(Clone, BorshSerialize, BorshDeserialize)]
/**
The required arguments for a new_order instruction.
*/
pub struct Params<C> {
    /// The maximum quantity of base to be traded.
    pub max_base_qty: u64,
    /// The maximum quantity of quote to be traded.
    pub max_quote_qty: u64,
    /// The limit price of the order. This value is understood as a 32-bit fixed point number.
    /// Must be rounded to the nearest tick size multiple (see [`round_price`][`crate::utils::round_price`])
    pub limit_price: u64,
    /// The order's side.
    pub side: Side,
    /// The maximum number of orders to match against before performing a partial fill.
    ///
    /// It is then possible for a caller program to detect a partial fill by reading the [`OrderSummary`][`crate::orderbook::OrderSummary`]
    /// in the event queue register.
    pub match_limit: u64,
    /// The callback information is used to attach metadata to an order. This callback information will be transmitted back through the event queue.
    ///
    /// The size of this vector should not exceed the current market's [`callback_info_len`][`MarketState::callback_info_len`].
    pub callback_info: C,
    /// The order will not be matched against the orderbook and will be direcly written into it.
    ///
    /// The operation will fail if the order's limit_price crosses the spread.
    pub post_only: bool,
    /// The order will be matched against the orderbook, but what remains will not be written as a new order into the orderbook.
    pub post_allowed: bool,
    /// Describes what would happen if this order was matched against an order with an equal `callback_info` field.
    pub self_trade_behavior: SelfTradeBehavior,
}

impl<C: BorshSize> BorshSize for Params<C> {
    fn borsh_len(&self) -> usize {
        self.max_base_qty.borsh_len()
            + self.max_quote_qty.borsh_len()
            + self.limit_price.borsh_len()
            + self.side.borsh_len()
            + self.match_limit.borsh_len()
            + self.callback_info.borsh_len()
            + self.post_only.borsh_len()
            + self.post_allowed.borsh_len()
            + self.self_trade_behavior.borsh_len()
    }
}

/// The required accounts for a new_order instruction.
#[derive(InstructionsAccount)]
pub struct Accounts<'a, T> {
    #[allow(missing_docs)]
    #[cons(writable)]
    pub market: &'a T,
    #[allow(missing_docs)]
    #[cons(writable)]
    pub event_queue: &'a T,
    #[allow(missing_docs)]
    #[cons(writable)]
    pub bids: &'a T,
    #[allow(missing_docs)]
    #[cons(writable)]
    pub asks: &'a T,
}

impl<'a, 'b: 'a> Accounts<'a, AccountInfo<'b>> {
    pub(crate) fn parse(accounts: &'a [AccountInfo<'b>]) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();

        let a = Self {
            market: next_account_info(accounts_iter)?,
            event_queue: next_account_info(accounts_iter)?,
            bids: next_account_info(accounts_iter)?,
            asks: next_account_info(accounts_iter)?,
        };
        Ok(a)
    }
    pub(crate) fn perform_checks(&self, program_id: &Pubkey) -> Result<(), ProgramError> {
        check_account_owner(
            self.market,
            &program_id.to_bytes(),
            AoError::WrongMarketOwner,
        )?;
        check_account_owner(
            self.event_queue,
            &program_id.to_bytes(),
            AoError::WrongEventQueueOwner,
        )?;
        check_account_owner(self.bids, &program_id.to_bytes(), AoError::WrongBidsOwner)?;
        check_account_owner(self.asks, &program_id.to_bytes(), AoError::WrongAsksOwner)?;
        Ok(())
    }
}

/// Apply the new_order instruction to the provided accounts
pub fn process<'a, 'b: 'a, C: Pod + CallbackInfo + PartialEq>(
    program_id: &Pubkey,
    accounts: Accounts<'a, AccountInfo<'b>>,
    params: Params<C>,
) -> Result<OrderSummary, ProgramError>
where
    <C as CallbackInfo>::CallbackId: PartialEq,
{
    accounts.perform_checks(program_id)?;
    let mut market_data = accounts.market.data.borrow_mut();
    let mut market_state = MarketState::from_buffer(&mut market_data, AccountTag::Market)?;

    check_accounts(&accounts, market_state)?;

    if params.limit_price % market_state.tick_size != 0 {
        return Err(AoError::InvalidLimitPrice.into());
    }

    let mut bids_guard = accounts.bids.data.borrow_mut();
    let mut asks_guard = accounts.asks.data.borrow_mut();

    let mut order_book = OrderBookState::new_safe(&mut bids_guard, &mut asks_guard)?;

    let mut event_queue_guard = accounts.event_queue.data.borrow_mut();
    let mut event_queue = EventQueue::from_buffer(&mut event_queue_guard, AccountTag::EventQueue)?;

    let order_summary =
        order_book.new_order(params, &mut event_queue, market_state.min_base_order_size)?;
    msg!("Order summary : {:?}", order_summary);

    //Verify that fees were transfered. Fees are expected to be transfered by the caller program in order
    // to reduce the CPI call stack depth.
    if accounts.market.lamports() - market_state.initial_lamports
        < market_state
            .fee_budget
            .checked_add(market_state.cranker_reward)
            .unwrap()
    {
        msg!("Fees were not correctly payed during caller runtime.");
        return Err(AoError::FeeNotPayed.into());
    }
    market_state.fee_budget = accounts.market.lamports() - market_state.initial_lamports;

    Ok(order_summary)
}

fn check_accounts<'a, 'b: 'a>(
    accounts: &Accounts<'a, AccountInfo<'b>>,
    market_state: &MarketState,
) -> ProgramResult {
    check_account_key(
        accounts.event_queue,
        &market_state.event_queue,
        AoError::WrongEventQueueAccount,
    )?;
    check_account_key(accounts.bids, &market_state.bids, AoError::WrongBidsAccount)?;
    check_account_key(accounts.asks, &market_state.asks, AoError::WrongAsksAccount)?;

    Ok(())
}
