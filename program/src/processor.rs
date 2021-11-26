use borsh::BorshDeserialize;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::instruction::AgnosticOrderbookInstruction;

#[allow(missing_docs)]
pub mod cancel_order;
#[allow(missing_docs)]
pub mod close_market;
#[allow(missing_docs)]
pub mod consume_events;
#[allow(missing_docs)]
pub mod create_market;
#[allow(missing_docs)]
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
                let accounts = create_market::Accounts::parse(program_id, accounts)?;
                create_market::process(accounts, params)?;
            }
            AgnosticOrderbookInstruction::NewOrder(params) => {
                msg!("Instruction: New Order");
                let accounts = new_order::Accounts::parse(program_id, accounts)?;
                new_order::process(accounts, params)?;
            }
            AgnosticOrderbookInstruction::ConsumeEvents(params) => {
                msg!("Instruction: Consume Events");
                let accounts = consume_events::Accounts::parse(program_id, accounts)?;
                consume_events::process(accounts, params)?;
            }
            AgnosticOrderbookInstruction::CancelOrder(params) => {
                msg!("Instruction: Cancel Order");
                let accounts = cancel_order::Accounts::parse(program_id, accounts)?;
                cancel_order::process(accounts, params)?;
            }
            AgnosticOrderbookInstruction::CloseMarket => {
                msg!("Instruction: Close Market");
                let accounts = close_market::Accounts::parse(program_id, accounts)?;
                close_market::process(accounts)?;
            }
        }
        Ok(())
    }
}
