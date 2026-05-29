#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use litesvm::LiteSVM;
    use litesvm_token::{CreateAssociatedTokenAccount, CreateMint, MintTo, spl_token};
    use solana_instruction::{AccountMeta, Instruction};
    use solana_keypair::Keypair;
    use solana_message::Message;
    use solana_native_token::LAMPORTS_PER_SOL;
    use solana_pubkey::Pubkey;
    use solana_signer::Signer;
    use solana_transaction::Transaction;

use crate::instructions::take;

    // ─── Constants

    const TOKEN_PROGRAM_ID: Pubkey = spl_token::ID;

    // Discriminators matching EscrowInstructions enum order
    const IX_MAKE: u8 = 0;
    const IX_REFUND: u8 = 1;
    const IX_TAKE: u8 = 2;

    // ─── Program helpers

    fn program_id() -> Pubkey {
        Pubkey::from(crate::ID)
    }

    fn system_program() -> Pubkey {
        solana_sdk_ids::system_program::ID
    }

    fn ata_program() -> Pubkey {
        "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
            .parse()
            .unwrap()
    }

    fn so_path() -> PathBuf {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        for subdir in &["sbpf-solana-solana", "sbf-solana-solana"] {
            let p = manifest_dir
                .join("target")
                .join(subdir)
                .join("release/accel_p_escrow.so");
            if p.exists() {
                return p;
            }
        }
        manifest_dir.join("target/deploy/accel_p_escrow.so")
    }

    fn escrow_pda(maker: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[b"escrow", maker.as_ref()], &program_id())
    }

    // ─── SVM setup

    fn setup_svm() -> (LiteSVM, Keypair) {
        let mut svm = LiteSVM::new();
        let payer = Keypair::new();
        svm.airdrop(&payer.pubkey(), 10 * LAMPORTS_PER_SOL)
            .expect("Airdrop failed");

        let program_data = std::fs::read(so_path())
            .expect("Failed to read .so — run `cargo build-sbf` first");
        svm.add_program(program_id(), &program_data)
            .expect("Failed to add program");

        (svm, payer)
    }

    // ─── Transaction helper

    fn send_ix(svm: &mut LiteSVM, payer: &Keypair, signers: &[&Keypair], ix: Instruction) -> litesvm::types::TransactionMetadata {
        let msg = Message::new(&[ix], Some(&payer.pubkey()));
        let blockhash = svm.latest_blockhash();
        let tx = Transaction::new(signers, msg, blockhash);
        svm.send_transaction(tx).expect("Transaction failed")
    }

    // ─── Token balance helper

    /// Read token account amount at bytes [64..72] of raw account data.
    fn token_balance(svm: &LiteSVM, ata: &Pubkey) -> u64 {
        let account = svm.get_account(ata).expect("token account not found");
        let bytes: [u8; 8] = account.data[64..72].try_into().unwrap();
        u64::from_le_bytes(bytes)
    }

    // ─── Instruction builders

    fn make_ix(
        maker: &Pubkey,
        mint_a: &Pubkey,
        mint_b: &Pubkey,
        escrow: &Pubkey,
        escrow_bump: u8,
        maker_ata_a: &Pubkey,
        vault: &Pubkey,
        amount_to_receive: u64,
        amount_to_give: u64,
    ) -> Instruction {
        let data = [
            vec![IX_MAKE],
            vec![escrow_bump],
            amount_to_receive.to_le_bytes().to_vec(),
            amount_to_give.to_le_bytes().to_vec(),
        ]
        .concat();

        Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(*maker, true),
                AccountMeta::new(*mint_a, false),
                AccountMeta::new(*mint_b, false),
                AccountMeta::new(*escrow, false),
                AccountMeta::new(*maker_ata_a, false),
                AccountMeta::new(*vault, false),
                AccountMeta::new_readonly(system_program(), false),
                AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
                AccountMeta::new_readonly(ata_program(), false),
            ],
            data,
        }
    }

    fn refund_ix(
        maker: &Pubkey,
        mint_a: &Pubkey,
        escrow: &Pubkey,
        maker_ata_a: &Pubkey,
        vault: &Pubkey,
    ) -> Instruction {
        Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(*maker, true),
                AccountMeta::new(*mint_a, false),
                AccountMeta::new(*escrow, false),
                AccountMeta::new(*maker_ata_a, false),
                AccountMeta::new(*vault, false),
                AccountMeta::new_readonly(system_program(), false),
                AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
                AccountMeta::new_readonly(ata_program(), false),
            ],
            data: vec![IX_REFUND],
        }
    }

    fn take_ix(
        maker: &Pubkey,
        taker: &Pubkey,
        mint_a: &Pubkey,
        mint_b: &Pubkey,
        escrow: &Pubkey,
        maker_ata_b: &Pubkey,
        taker_ata_a: &Pubkey,
        taker_ata_b: &Pubkey,
        vault: &Pubkey,
    ) -> Instruction {
        Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(*maker, false),
                AccountMeta::new(*taker, true),
                AccountMeta::new(*mint_a, false),
                AccountMeta::new(*mint_b, false),
                AccountMeta::new(*escrow, false),
                AccountMeta::new(*maker_ata_b, false),
                AccountMeta::new(*taker_ata_a, false),
                AccountMeta::new(*taker_ata_b, false),
                AccountMeta::new(*vault, false),
                AccountMeta::new_readonly(system_program(), false),
                AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
                AccountMeta::new_readonly(ata_program(), false),
            ],
            data: vec![IX_TAKE],
        }
    }

    // ─── Shared escrow state

    struct EscrowSetup {
        svm: LiteSVM,
        maker: Keypair,
        mint_a: Pubkey,
        mint_b: Pubkey,
        maker_ata_a: Pubkey,
        escrow: Pubkey,
        vault: Pubkey,
        amount_to_receive: u64,
        amount_to_give: u64,
        mint_amount: u64,
    }

    fn setup_escrow(amount_to_receive: u64, amount_to_give: u64, mint_amount: u64) -> EscrowSetup {
        let (mut svm, maker) = setup_svm();

        let mint_a = CreateMint::new(&mut svm, &maker)
            .decimals(6)
            .authority(&maker.pubkey())
            .send()
            .unwrap();

        let mint_b = CreateMint::new(&mut svm, &maker)
            .decimals(6)
            .authority(&maker.pubkey())
            .send()
            .unwrap();

        let maker_ata_a = CreateAssociatedTokenAccount::new(&mut svm, &maker, &mint_a)
            .owner(&maker.pubkey())
            .send()
            .unwrap();

        MintTo::new(&mut svm, &maker, &mint_a, &maker_ata_a, mint_amount)
            .send()
            .unwrap();

        let (escrow, escrow_bump) = escrow_pda(&maker.pubkey());
        let vault =
            spl_associated_token_account::get_associated_token_address(&escrow, &mint_a);

        let ix = make_ix(
            &maker.pubkey(),
            &mint_a,
            &mint_b,
            &escrow,
            escrow_bump,
            &maker_ata_a,
            &vault,
            amount_to_receive,
            amount_to_give,
        );

        let meta = send_ix(&mut svm, &maker, &[&maker], ix);
        println!("Make CU: {}", meta.compute_units_consumed);

        EscrowSetup {
            svm,
            maker,
            mint_a,
            mint_b,
            maker_ata_a,
            escrow,
            vault,
            amount_to_receive,
            amount_to_give,
            mint_amount,
        }
    }

    // ─── Tests

    #[test]
    fn test_make() {
        let s = setup_escrow(100_000_000, 500_000_000, 1_000_000_000);

        let escrow_account = s.svm.get_account(&s.escrow).expect("escrow not found");
        assert_eq!(escrow_account.owner, program_id());
        assert_eq!(escrow_account.data.len(), 113);

        assert_eq!(token_balance(&s.svm, &s.vault), s.amount_to_give);
        assert_eq!(
            token_balance(&s.svm, &s.maker_ata_a),
            s.mint_amount - s.amount_to_give
        );

        println!("test_make passed");
    }

    #[test]
    fn test_refund() {
        let mut s = setup_escrow(100_000_000, 500_000_000, 1_000_000_000);

        // Verify pre-conditions
        assert_eq!(token_balance(&s.svm, &s.vault), s.amount_to_give);
        assert_eq!(
            token_balance(&s.svm, &s.maker_ata_a),
            s.mint_amount - s.amount_to_give
        );

        let ix = refund_ix(
            &s.maker.pubkey(),
            &s.mint_a,
            &s.escrow,
            &s.maker_ata_a,
            &s.vault,
        );

        // let maker = Keypair::from_bytes(&s.maker.to_bytes()).unwrap();
        let maker_bytes: [u8; 32] = s.maker.to_bytes()[..32].try_into().unwrap();
        let maker = Keypair::new_from_array(maker_bytes);
        let meta = send_ix(&mut s.svm, &maker, &[&maker], ix);
        println!("Refund CU: {}", meta.compute_units_consumed);

        // Vault closed
        assert!(s.svm.get_account(&s.vault).is_none(), "vault should be closed");

        // Escrow state account closed
        // assert!(s.svm.get_account(&s.escrow).is_none(), "escrow should be closed");

        // Full balance back to maker
        assert_eq!(
            token_balance(&s.svm, &s.maker_ata_a),
            s.mint_amount,
            "maker should have all tokens back"
        );

        println!("test_refund passed");
    }

    #[test]
    fn test_take() {
        let mut s = setup_escrow(100_000_000, 500_000_000, 1_000_000_000);

        // Set up taker with mint_b tokens to pay the maker
        let taker = Keypair::new();
        s.svm
            .airdrop(&taker.pubkey(), 10 * LAMPORTS_PER_SOL)
            .expect("Airdrop failed");

        let taker_ata_b = CreateAssociatedTokenAccount::new(&mut s.svm, &taker, &s.mint_b)
            .owner(&taker.pubkey())
            .send()
            .unwrap();

        MintTo::new(&mut s.svm, &s.maker, &s.mint_b, &taker_ata_b, s.amount_to_receive)
            .send()
            .unwrap();

        let taker_ata_a = CreateAssociatedTokenAccount::new(&mut s.svm, &taker, &s.mint_a)
            .owner(&taker.pubkey())
            .send()
            .unwrap();

        let maker_ata_b = CreateAssociatedTokenAccount::new(&mut s.svm, &s.maker, &s.mint_b)
            .owner(&s.maker.pubkey())
            .send()
            .unwrap();
   
        // Verify pre-conditions
        assert_eq!(token_balance(&s.svm, &s.vault), s.amount_to_give);
        assert_eq!(token_balance(&s.svm, &taker_ata_b), s.amount_to_receive);
        assert_eq!(token_balance(&s.svm, &taker_ata_a), 0);
        assert_eq!(token_balance(&s.svm, &maker_ata_b), 0);

        let ix = take_ix(
            &s.maker.pubkey(),
            &taker.pubkey(),
            &s.mint_a,
            &s.mint_b,
            &s.escrow,
            &maker_ata_b,
            &taker_ata_a,
            &taker_ata_b,
            &s.vault,
        );

        let meta = send_ix(&mut s.svm, &taker, &[&taker], ix);
        println!("Take CU: {}", meta.compute_units_consumed);

        // Taker received mint_a from vault
        assert_eq!(
            token_balance(&s.svm, &taker_ata_a),
            s.amount_to_give,
            "taker should receive escrowed tokens"
        );

        // Maker received mint_b from taker
        assert_eq!(
            token_balance(&s.svm, &maker_ata_b),
            s.amount_to_receive,
            "maker should receive payment tokens"
        );

        // Taker's payment tokens debited
        assert_eq!(
            token_balance(&s.svm, &taker_ata_b),
            0,
            "taker's payment tokens should be debited"
        );

        // Vault closed
        assert!(s.svm.get_account(&s.vault).is_none(), "vault should be closed");

        println!("test_take passed");
    }
}