use borsh::BorshDeserialize;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::instruction::AgnosticOrderbookInstruction;

////////////////////////////////////////////////////////////
// Constants

////////////////////////////////////////////////////////////

pub mod consume_event;
pub mod create_market;
pub mod new_order;

pub struct Processor {}

impl Processor {
    pub fn process_instruction(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        instruction_data: &[u8],
    ) -> ProgramResult {
        msg!("Beginning processing");
        let instruction = AgnosticOrderbookInstruction::try_from_slice(instruction_data)
            .map_err(|_| ProgramError::InvalidInstructionData)?;
        msg!("Instruction unpacked");

        match instruction {
            AgnosticOrderbookInstruction::CreateMarket(params) => {
                msg!("Instruction: Create Market");
                create_market::process(program_id, accounts, params)?;
            }
            AgnosticOrderbookInstruction::NewOrder(params) => {
                msg!("Instruction: New Order");
                new_order::process(program_id, accounts, params)?;
            }
            AgnosticOrderbookInstruction::ConsumeEvents => todo!(),
            AgnosticOrderbookInstruction::CancelOrder => todo!(),
            AgnosticOrderbookInstruction::CancelOrderByClientId => todo!(),
            AgnosticOrderbookInstruction::DisableMarket => todo!(),
        }
        Ok(())
    }
}
