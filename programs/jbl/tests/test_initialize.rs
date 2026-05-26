use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::program_pack::Pack;
use jbl::state::Pool;
use {
    anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas},
    anchor_spl::token::spl_token,
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

fn find_collateral_vault_pda(pool: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"collateral_vault", pool.as_ref()], program_id)
}

fn find_lend_vault_pda(pool: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"lend_vault", pool.as_ref()], program_id)
}

fn find_lp_mint_pda(pool: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"lp_mint", pool.as_ref()], program_id)
}

fn create_mint_ixs(
    payer: &Pubkey,
    mint: &Pubkey,
    mint_authority: &Pubkey,
    rent: u64,
) -> [anchor_lang::solana_program::instruction::Instruction; 2] {
    use anchor_lang::solana_program::program_pack::Pack;
    let create_ix = anchor_lang::solana_program::system_instruction::create_account(
        payer,
        mint,
        rent,
        spl_token::state::Mint::LEN as u64,
        &spl_token::id(),
    );
    let init_ix = spl_token::instruction::initialize_mint(
        &spl_token::id(),
        mint,
        mint_authority,
        None,
        6,
    )
    .unwrap();
    [create_ix, init_ix]
}

#[test]
fn test_create() {
    let program_id = jbl::id();
    let payer = Keypair::new();
    let authority = Keypair::new();
    let collateral_mint_keypair = Keypair::new();
    let lend_mint_keypair = Keypair::new();

    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/jbl.so");
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();

    // ── Create collateral and lend SPL mints ─────────────────────────────────
    let mint_rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN);
    let collateral_mint_ixs =
        create_mint_ixs(&payer.pubkey(), &collateral_mint_keypair.pubkey(), &payer.pubkey(), mint_rent);
    let lend_mint_ixs =
        create_mint_ixs(&payer.pubkey(), &lend_mint_keypair.pubkey(), &payer.pubkey(), mint_rent);

    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(
        &[
            collateral_mint_ixs[0].clone(),
            collateral_mint_ixs[1].clone(),
            lend_mint_ixs[0].clone(),
            lend_mint_ixs[1].clone(),
        ],
        Some(&payer.pubkey()),
        &bh,
    );
    let tx = VersionedTransaction::try_new(
        VersionedMessage::Legacy(msg),
        &[&payer, &collateral_mint_keypair, &lend_mint_keypair],
    )
    .unwrap();
    svm.send_transaction(tx).unwrap();

    // ── Create pool keypair and pre-allocate account ──────────────────────────
    let pool_keypair = Keypair::new();
    let pool_pubkey = pool_keypair.pubkey();

    let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &program_id);
    let (collateral_vault_pda, _) = find_collateral_vault_pda(&pool_pubkey, &program_id);
    let (lend_vault_pda, _) = find_lend_vault_pda(&pool_pubkey, &program_id);
    let (lp_mint_pda, _) = find_lp_mint_pda(&pool_pubkey, &program_id);

    // Pre-allocate pool account (too large for CPI creation)
    let pool_space = 8 + std::mem::size_of::<Pool>();
    let pool_rent = svm.minimum_balance_for_rent_exemption(pool_space);
    let create_pool_account_ix = anchor_lang::solana_program::system_instruction::create_account(
        &payer.pubkey(),
        &pool_pubkey,
        pool_rent,
        pool_space as u64,
        &program_id,
    );

    // ── Build create instruction ──────────────────────────────────────────────
    let instruction = Instruction::new_with_bytes(
        program_id,
        &jbl::instruction::Create {
            ltv_percent: 75,
        }
        .data(),
        jbl::accounts::Create {
            pool: pool_pubkey,
            state: state_pda,
            collateral_vault: collateral_vault_pda,
            lend_vault: lend_vault_pda,
            lp_mint: lp_mint_pda,
            collateral_mint: collateral_mint_keypair.pubkey(),
            lend_mint: lend_mint_keypair.pubkey(),
            authority: authority.pubkey(),
            payer: payer.pubkey(),
            token_program: spl_token::id(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );

    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(
        &[create_pool_account_ix, instruction],
        Some(&payer.pubkey()),
        &bh,
    );
    let tx = VersionedTransaction::try_new(
        VersionedMessage::Legacy(msg),
        &[&payer, &pool_keypair, &authority],
    )
    .unwrap();

    let res = svm.send_transaction(tx);
    assert!(res.is_ok(), "create failed: {:?}", res.err());
}

#[test]
fn test_pool_size() {
    // Pool layout (repr(C), no implicit padding):
    //   4 × Pubkey (32)                        = 128
    //   6 × u64/i64 (8)                        = 48
    //   fee_config 4 curves × 24 bytes         = 96
    //   ltv_percent, lp_mint_bump (1 each)     = 2
    //   _pad [u8;6]                            = 6
    //   WithdrawalQueue:
    //     head (u16) + tail (u16) + _pad [u8;4] = 8
    //     entries [WithdrawalQueueEntry; 1024], each 40 bytes = 40960
    //                                                 total = 41248
    assert_eq!(
        std::mem::size_of::<Pool>(),
        41264,
        "Pool size changed — update POOL_SPACE in create.rs and the TS test helper if intentional",
    );
}
