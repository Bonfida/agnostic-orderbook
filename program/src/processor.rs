use borsh::BorshDeserialize;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::{instruction::AgnosticOrderbookInstruction, processor::new_order::process_new_order};

////////////////////////////////////////////////////////////
// Constants

////////////////////////////////////////////////////////////

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
            AgnosticOrderbookInstruction::NewOrder(params) => {
                msg!("Instruction: New Order");
                process_new_order(program_id, accounts, params)?;
            }
            AgnosticOrderbookInstruction::InitializeMarket => todo!(),
            AgnosticOrderbookInstruction::ConsumeEvents => todo!(),
            AgnosticOrderbookInstruction::CancelOrder => todo!(),
            AgnosticOrderbookInstruction::CancelOrderByClientId => todo!(),
            AgnosticOrderbookInstruction::DisableMarket => todo!(),
        }
        Ok(())
    }
}
