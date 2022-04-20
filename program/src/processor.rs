use bytemuck::Pod;
use num_traits::FromPrimitive;
use solana_program::{account_info::AccountInfo, msg, program_error::ProgramError, pubkey::Pubkey};

use crate::{
    instruction::AgnosticOrderbookInstruction,
    state::orderbook::{CallbackInfo, OrderSummary},
};

use borsh::BorshDeserialize;

pub mod cancel_order;
pub mod close_market;
pub mod consume_events;
pub mod create_market;
pub mod mass_cancel_orders;
pub mod new_order;

pub fn process_instruction<C: Pod + BorshDeserialize + CallbackInfo + PartialEq>(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<Option<OrderSummary>, ProgramError>
where
    <C as CallbackInfo>::CallbackId: PartialEq,
{
    msg!("Beginning processing");
    let instruction =
        FromPrimitive::from_u8(instruction_data[0]).ok_or(ProgramError::InvalidInstructionData)?;
    let instruction_data = &instruction_data[1..];
    msg!("Instruction unpacked");

    match instruction {
        AgnosticOrderbookInstruction::CreateMarket => {
            msg!("Instruction: Create Market");
            let accounts = create_market::Accounts::parse(accounts)?;
            let params = create_market::Params::try_from_slice(instruction_data)
                .map_err(|_| ProgramError::InvalidInstructionData)?;
            create_market::process::<C>(program_id, accounts, params)?;
        }
        AgnosticOrderbookInstruction::NewOrder => {
            msg!("Instruction: New Order");
            let accounts = new_order::Accounts::parse(accounts)?;
            let params = new_order::Params::<C>::try_from_slice(instruction_data)
                .map_err(|_| ProgramError::InvalidInstructionData)?;
            return new_order::process(program_id, accounts, params).map(Some);
        }
        AgnosticOrderbookInstruction::ConsumeEvents => {
            msg!("Instruction: Consume Events");
            let accounts = consume_events::Accounts::parse(accounts)?;
            let params = consume_events::Params::try_from_slice(instruction_data)
                .map_err(|_| ProgramError::InvalidInstructionData)?;
            consume_events::process::<C>(program_id, accounts, params)?;
        }
        AgnosticOrderbookInstruction::CancelOrder => {
            msg!("Instruction: Cancel Order");
            let accounts = cancel_order::Accounts::parse(accounts)?;
            let params = cancel_order::Params::try_from_slice(instruction_data)
                .map_err(|_| ProgramError::InvalidInstructionData)?;
            return cancel_order::process::<C>(program_id, accounts, params).map(Some);
        }
        AgnosticOrderbookInstruction::CloseMarket => {
            msg!("Instruction: Close Market");
            let accounts = close_market::Accounts::parse(accounts)?;
            close_market::process::<C>(program_id, accounts, close_market::Params {})?;
        }
        AgnosticOrderbookInstruction::MassCancelOrders => {
            msg!("Instruction: Mass Cancel Orders");
            let accounts = mass_cancel_orders::Accounts::parse(accounts)?;
            let params = mass_cancel_orders::Params::try_from_slice(instruction_data)
                .map_err(|_| ProgramError::InvalidInstructionData)?;
            return mass_cancel_orders::process::<C>(program_id, accounts, params).map(Some);
        }
    }
    Ok(None)
}
