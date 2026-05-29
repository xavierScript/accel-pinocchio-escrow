use pinocchio::{
    AccountView, ProgramResult,
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{Sysvar, rent::Rent},
};
use pinocchio_pubkey::derive_address;
use pinocchio_token::instructions::{Transfer, CloseAccount};

use crate::state::Escrow;

pub fn process_take_instruction(accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let [
        maker,
        taker,
        mint_a,
        mint_b,
        escrow_account,
        maker_ata_b,
        taker_ata_a,
        taker_ata_b,
        escrow_ata,
        system_program,
        token_program,
        _associated_token_program @ ..,
    ] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

     {
        let taker_ata_a_state = pinocchio_token::state::Account::from_account_view(taker_ata_a)?;
        if taker_ata_a_state.owner() != taker.address() {
            return Err(ProgramError::IllegalOwner);
        }
        if taker_ata_a_state.mint() != mint_a.address() {
            return Err(ProgramError::InvalidAccountData);
        }
   
        let taker_ata_b_state = pinocchio_token::state::Account::from_account_view(taker_ata_b)?;
        if taker_ata_b_state.owner() != taker.address() {
            return Err(ProgramError::IllegalOwner);
        }
        if taker_ata_b_state.mint() != mint_b.address() {
            return Err(ProgramError::InvalidAccountData);
        }
    }

    let escrow_state = Escrow::from_account_info(escrow_account)?;
    let amount_to_receive = escrow_state.amount_to_receive();
    let amount_to_give = escrow_state.amount_to_give();

    let bump = escrow_state.bump;
    let seed = [b"escrow".as_ref(), maker.address().as_ref(), &[bump]];

    let escrow_account_pda = derive_address(&seed, None, &crate::ID.to_bytes());
    assert_eq!(escrow_account_pda, *escrow_account.address().as_array());

    let bump_bytes = [bump];
    let signer_seeds = [
        Seed::from(b"escrow"),
        Seed::from(maker.address().as_array()),
        Seed::from(bump_bytes.as_ref()),
    ];
    // let signer = Signer::from(&signer_seeds);

    if !escrow_account.owned_by(&crate::ID) {
        return Err(ProgramError::IllegalOwner);
    }

    Transfer::new(taker_ata_b, maker_ata_b, taker, amount_to_receive)
        .invoke()?;

    
    Transfer::new(escrow_ata, taker_ata_a, escrow_account, amount_to_give)
        .invoke_signed(&[Signer::from(&signer_seeds)])?;


    CloseAccount::new(escrow_ata, maker, escrow_account)
        .invoke_signed(&[Signer::from(&signer_seeds)])?;

    Ok(())
}
