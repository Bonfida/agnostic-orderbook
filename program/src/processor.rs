use borsh::BorshDeserialize;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::instruction::AgnosticOrderbookInstruction;

pub mod cancel_order;
pub mod close_market;
pub mod consume_events;
pub mod create_market;
pub mod new_order;

#[allow(missing_docs)]
pub mod msrm_token {
    use solana_program::declare_id;

    declare_id!("MSRMcoVyrFxnSgo5uXwone5SKcGhT1KEJMFEkMEWf9L");
}

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
                let accounts = create_market::Accounts::parse(accounts)?;
                create_market::process(program_id, accounts, params)?;
            }
            AgnosticOrderbookInstruction::NewOrder(params) => {
                msg!("Instruction: New Order");
                let accounts = new_order::Accounts::parse(accounts)?;
                new_order::process(program_id, accounts, params)?;
            }
            AgnosticOrderbookInstruction::ConsumeEvents(params) => {
                msg!("Instruction: Consume Events");
                let accounts = consume_events::Accounts::parse(accounts)?;
                consume_events::process(program_id, accounts, params)?;
            }
            AgnosticOrderbookInstruction::CancelOrder(params) => {
                msg!("Instruction: Cancel Order");
                let accounts = cancel_order::Accounts::parse(accounts)?;
                cancel_order::process(program_id, accounts, params)?;
            }
            AgnosticOrderbookInstruction::CloseMarket => {
                msg!("Instruction: Close Market");
                let accounts = close_market::Accounts::parse(accounts)?;
                close_market::process(program_id, accounts)?;
            }
        }
        Ok(())
    }
}
