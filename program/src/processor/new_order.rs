use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::{
    error::AoError,
    orderbook::OrderBookState,
    state::{
        EventQueue, EventQueueHeader, MarketState, SelfTradeBehavior, Side, EVENT_QUEUE_HEADER_LEN,
    },
    utils::{check_account_key, check_account_owner, check_signer},
    CRANKER_REWARD,
};

#[derive(BorshDeserialize, BorshSerialize, Clone)]
/**
The required arguments for a new_order instruction.
*/
pub struct Params {
    /// The maximum quantity of base to be traded.
    pub max_base_qty: u64,
    /// The maximum quantity of quote to be traded.
    pub max_quote_qty: u64,
    /// The limit price of the order. This value is understood as a 32-bit fixed point number.
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
    pub callback_info: Vec<u8>,
    /// The order will not be matched against the orderbook and will be direcly written into it.
    ///
    /// The operation will fail if the order's limit_price crosses the spread.
    pub post_only: bool,
    /// The order will be matched against the orderbook, but what remains will not be written as a new order into the orderbook.
    pub post_allowed: bool,
    /// Describes what would happen if this order was matched against an order with an equal `callback_info` field.
    pub self_trade_behavior: SelfTradeBehavior,
}

struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    event_queue: &'a AccountInfo<'b>,
    bids: &'a AccountInfo<'b>,
    asks: &'a AccountInfo<'b>,
    authority: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();
        let a = Self {
            market: next_account_info(accounts_iter)?,
            event_queue: next_account_info(accounts_iter)?,
            bids: next_account_info(accounts_iter)?,
            asks: next_account_info(accounts_iter)?,
            authority: next_account_info(accounts_iter)?,
        };
        check_account_owner(a.market, program_id).unwrap();
        check_account_owner(a.event_queue, program_id).unwrap();
        check_account_owner(a.bids, program_id).unwrap();
        check_account_owner(a.asks, program_id).unwrap();
        check_signer(a.authority).unwrap();
        Ok(a)
    }
}

pub(crate) fn process(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    params: Params,
) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;
    let mut market_state = {
        let mut market_data: &[u8] = &accounts.market.data.borrow();
        MarketState::deserialize(&mut market_data)
            .unwrap()
            .check()?
    };

    check_account_key(accounts.event_queue, &market_state.event_queue)
        .map_err(|_| AoError::WrongEventQueueAccount)?;
    check_account_key(accounts.bids, &market_state.bids).map_err(|_| AoError::WrongBidsAccount)?;
    check_account_key(accounts.asks, &market_state.asks).map_err(|_| AoError::WrongAsksAccount)?;
    check_account_key(accounts.authority, &market_state.caller_authority)
        .map_err(|_| AoError::WrongCallerAuthority)?;

    let callback_info_len = market_state.callback_info_len as usize;

    let mut order_book = OrderBookState::new_safe(
        accounts.bids,
        accounts.asks,
        market_state.callback_info_len as usize,
        market_state.callback_id_len as usize,
    )?;

    if params.callback_info.len() != callback_info_len {
        msg!("Invalid callback information");
        return Err(ProgramError::InvalidArgument);
    }

    let header = {
        let mut event_queue_data: &[u8] =
            &accounts.event_queue.data.borrow()[0..EVENT_QUEUE_HEADER_LEN];
        EventQueueHeader::deserialize(&mut event_queue_data)
            .unwrap()
            .check()?
    };
    let mut event_queue = EventQueue::new_safe(header, &accounts.event_queue, callback_info_len)?;

    let order_summary = order_book.new_order(
        params,
        &mut &mut event_queue,
        market_state.min_base_order_size,
    )?;
    msg!("Order summary : {:?}", order_summary);
    event_queue.write_to_register(order_summary);

    let mut event_queue_header_data: &mut [u8] = &mut accounts.event_queue.data.borrow_mut();
    event_queue
        .header
        .serialize(&mut event_queue_header_data)
        .unwrap();
    order_book.commit_changes();

    //Verify that fees were transfered. Fees are expected to be transfered by the caller program in order
    // to reduce the CPI call stack depth.
    if accounts.market.lamports() - market_state.initial_lamports
        < market_state.fee_budget + CRANKER_REWARD
    {
        msg!("Fees were not correctly payed during caller runtime.");
        return Err(AoError::FeeNotPayed.into());
    }
    market_state.fee_budget = accounts.market.lamports() - market_state.initial_lamports;
    let mut market_state_data: &mut [u8] = &mut accounts.market.data.borrow_mut();
    market_state.serialize(&mut market_state_data).unwrap();

    Ok(())
}
