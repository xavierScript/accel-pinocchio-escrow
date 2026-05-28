use pinocchio::{
    AccountView, ProgramResult,
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{Sysvar, rent::Rent},
};
use pinocchio_pubkey::derive_address;
use pinocchio_token::instructions::{Transfer, CloseAccount};

use crate::state::Escrow;

pub fn process_refund_instruction(accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let [
        maker,
        mint_a,
        escrow_account,
        maker_ata_a,
        escrow_ata,
        system_program,
        token_program,
        _associated_token_program @ ..,
    ] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    {
        let maker_ata_a_state = pinocchio_token::state::Account::from_account_view(maker_ata_a)?;
        if maker_ata_a_state.owner() != maker.address() {
            return Err(ProgramError::IllegalOwner);
        }
        if maker_ata_a_state.mint() != mint_a.address() {
            return Err(ProgramError::InvalidAccountData);
        }
    }

    let escrow_state = Escrow::from_account_info(escrow_account)?;
    let bump = escrow_state.bump;
    // let amount_to_receive = escrow_state.amount_to_receive();
    let amount_to_give = escrow_state.amount_to_give();

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
        return Err(ProgramError::AccountAlreadyInitialized);
    }  

    Transfer::new(escrow_ata, maker_ata_a, escrow_account, amount_to_give)
        .invoke_signed(&[Signer::from(&signer_seeds)])?;

    CloseAccount::new(escrow_ata, maker, escrow_account)
    .invoke_signed(&[Signer::from(&signer_seeds)])?;

//    *maker.try_borrow_mut_lamports()? += *escrow_ata.try_borrow_lamports()?;
//     escrow_ata.realloc(1, true)?;
//     escrow_ata.close()?;

    Ok(())
}
