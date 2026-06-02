mod common;
use common::{
    create_mint_ixs, create_token_account_ixs, mint_to_ix, read_token_balance, send_ixs,
    try_send_ixs,
};

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::program_pack::Pack;
use jbl::state::Pool;
use {
    anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas},
    anchor_spl::token::spl_token,
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

// ── PDA helpers ───────────────────────────────────────────────────────────────

fn find_state_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"state"], program_id)
}

fn find_collateral_vault_pda(pool: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"collateral_vault", pool.as_ref()], program_id)
}

fn find_lend_vault_pda(pool: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"lend_vault", pool.as_ref()], program_id)
}

fn find_lp_mint_pda(pool: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"lp_mint", pool.as_ref()], program_id)
}

fn find_user_position_pda(pool: &Pubkey, authority: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"user_position", pool.as_ref(), authority.as_ref()],
        program_id,
    )
}

// ── Account data helpers ──────────────────────────────────────────────────────

fn read_total_lend_deposited(svm: &LiteSVM, pool: &Pubkey) -> u64 {
    let data = svm.get_account(pool).unwrap().data;
    u64::from_le_bytes(data[8 + 128..8 + 136].try_into().unwrap())
}

// ── Setup ─────────────────────────────────────────────────────────────────────

struct Setup {
    svm: LiteSVM,
    program_id: Pubkey,
    payer: Keypair,
    pool_pubkey: Pubkey,
    state_pda: Pubkey,
    collateral_mint: Pubkey,
    lend_mint: Pubkey,
    collateral_vault_pda: Pubkey,
    lend_vault_pda: Pubkey,
    #[allow(dead_code)]
    lp_mint_pda: Pubkey,
    user_lend_account: Pubkey,
    user_collateral_account: Pubkey,
}

/// Build a fresh environment:
///  - loads jbl.so
///  - creates payer (mint authority + user), pool authority
///  - initialises two mints with payer as mint authority
///  - creates the pool (pre-allocate + Create)
///  - seeds the lend vault with `seed_lend_amount` tokens directly and patches
///    pool.total_lend_deposited to match
///  - creates empty user token accounts (collateral + lend)
fn setup(seed_lend_amount: u64) -> Setup {
    let program_id = jbl::id();
    let feed_id = feed::id();
    let irm_id = irm::id();
    let payer = Keypair::new();
    let authority = Keypair::new();
    let collateral_mint_kp = Keypair::new();
    let lend_mint_kp = Keypair::new();

    let mut svm = LiteSVM::new();
    svm.add_program(program_id, include_bytes!("../../../target/deploy/jbl.so")).unwrap();
    svm.add_program(feed_id, include_bytes!("../../../target/deploy/feed.so")).unwrap();
    svm.add_program(irm_id, include_bytes!("../../../target/deploy/irm.so")).unwrap();
    svm.airdrop(&payer.pubkey(), 100_000_000_000).unwrap();
    svm.airdrop(&authority.pubkey(), 10_000_000_000).unwrap();

    // ── Create mints ─────────────────────────────────────────────────────────
    let mint_rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN);
    let col_mint_ixs = create_mint_ixs(
        &payer.pubkey(),
        &collateral_mint_kp.pubkey(),
        &payer.pubkey(),
        mint_rent,
    );
    let lend_mint_ixs = create_mint_ixs(
        &payer.pubkey(),
        &lend_mint_kp.pubkey(),
        &payer.pubkey(),
        mint_rent,
    );

    send_ixs(
        &mut svm,
        &[
            col_mint_ixs[0].clone(),
            col_mint_ixs[1].clone(),
            lend_mint_ixs[0].clone(),
            lend_mint_ixs[1].clone(),
        ],
        &payer,
        &[&payer, &collateral_mint_kp, &lend_mint_kp],
    );

    // ── Create pool (pre-allocate + Create instruction) ───────────────────────
    let pool_kp = Keypair::new();
    let pool_pubkey = pool_kp.pubkey();

    let (state_pda, _) = find_state_pda(&program_id);
    let (collateral_vault_pda, _) = find_collateral_vault_pda(&pool_pubkey, &program_id);
    let (lend_vault_pda, _) = find_lend_vault_pda(&pool_pubkey, &program_id);
    let (lp_mint_pda, _) = find_lp_mint_pda(&pool_pubkey, &program_id);

    // ── Feed account (payer is authority so it can sign set_value) ────────────
    let (feed_pda, _) = Pubkey::find_program_address(
        &[b"feed", payer.pubkey().as_ref()],
        &feed_id,
    );
    let feed_create_ix = Instruction::new_with_bytes(
        feed_id,
        &feed::instruction::Create {}.data(),
        feed::accounts::Create {
            feed: feed_pda,
            authority: payer.pubkey(),
            payer: payer.pubkey(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
    send_ixs(&mut svm, &[feed_create_ix], &payer, &[&payer]);

    let (irm_config, _) =
        Pubkey::find_program_address(&[b"irm_config", pool_pubkey.as_ref()], &irm_id);

    let pool_space = 8 + std::mem::size_of::<Pool>();
    let pool_rent = svm.minimum_balance_for_rent_exemption(pool_space);
    let alloc_pool_ix = anchor_lang::solana_program::system_instruction::create_account(
        &payer.pubkey(),
        &pool_pubkey,
        pool_rent,
        pool_space as u64,
        &program_id,
    );
    let irm_init_ix = Instruction::new_with_bytes(
        irm_id,
        &irm::instruction::Initialize {}.data(),
        irm::accounts::Initialize {
            irm_config,
            pool: pool_pubkey,
            authority: payer.pubkey(),
            payer: payer.pubkey(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
    send_ixs(&mut svm, &[alloc_pool_ix, irm_init_ix], &payer, &[&payer, &pool_kp]);

    // set_value must precede jbl::create in the same transaction.
    let set_value_ix = Instruction::new_with_bytes(
        feed_id,
        &feed::instruction::SetValue { value: 1_000_000 }.data(),
        feed::accounts::SetValue {
            feed: feed_pda,
            authority: payer.pubkey(),
        }
        .to_account_metas(None),
    );

    let create_ix = Instruction::new_with_bytes(
        program_id,
        &jbl::instruction::Create {
            ltv_percent: 75,
            feed_program: feed_id,
            feed_state: feed_pda,
        }
        .data(),
        jbl::accounts::Create {
            pool: pool_pubkey,
            state: state_pda,
            collateral_vault: collateral_vault_pda,
            lend_vault: lend_vault_pda,
            lp_mint: lp_mint_pda,
            collateral_mint: collateral_mint_kp.pubkey(),
            lend_mint: lend_mint_kp.pubkey(),
            authority: authority.pubkey(),
            payer: payer.pubkey(),
            sysvar_instructions: solana_sdk_ids::sysvar::instructions::ID,
            rate_program: irm_id,
            irm_state: irm_config,
            guard_program: None,
            guard_state: None,
            token_program: spl_token::id(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );

    send_ixs(
        &mut svm,
        &[set_value_ix, create_ix],
        &payer,
        &[&payer, &authority],
    );

    // ── Seed lend vault directly + patch pool state ───────────────────────────
    if seed_lend_amount > 0 {
        let seed_ix = mint_to_ix(
            &lend_mint_kp.pubkey(),
            &lend_vault_pda,
            &payer.pubkey(),
            seed_lend_amount,
        );
        send_ixs(&mut svm, &[seed_ix], &payer, &[&payer]);

        // Patch pool.market.total_supply_assets so flash_borrow accounting doesn't
        // underflow.  Layout: 8-byte disc + 4×32-byte Pubkeys + market at 128;
        // total_supply_assets is the first field of market (offset 128).
        let mut acct = svm.get_account(&pool_pubkey).unwrap();
        acct.data[8 + 128..8 + 136].copy_from_slice(&seed_lend_amount.to_le_bytes());
        svm.set_account(pool_pubkey, acct).unwrap();
    }

    // ── Create user token accounts (empty) ────────────────────────────────────
    let user_lend_kp = Keypair::new();
    let user_col_kp = Keypair::new();
    let ta_rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Account::LEN);

    let lend_ta_ixs = create_token_account_ixs(
        &payer.pubkey(),
        &user_lend_kp.pubkey(),
        &lend_mint_kp.pubkey(),
        &payer.pubkey(),
        ta_rent,
    );
    let col_ta_ixs = create_token_account_ixs(
        &payer.pubkey(),
        &user_col_kp.pubkey(),
        &collateral_mint_kp.pubkey(),
        &payer.pubkey(),
        ta_rent,
    );

    send_ixs(
        &mut svm,
        &[
            lend_ta_ixs[0].clone(),
            lend_ta_ixs[1].clone(),
            col_ta_ixs[0].clone(),
            col_ta_ixs[1].clone(),
        ],
        &payer,
        &[&payer, &user_lend_kp, &user_col_kp],
    );

    Setup {
        svm,
        program_id,
        payer,
        pool_pubkey,
        state_pda,
        collateral_mint: collateral_mint_kp.pubkey(),
        lend_mint: lend_mint_kp.pubkey(),
        collateral_vault_pda,
        lend_vault_pda,
        lp_mint_pda,
        user_lend_account: user_lend_kp.pubkey(),
        user_collateral_account: user_col_kp.pubkey(),
    }
}

// ── Flash loan instruction builders ──────────────────────────────────────────

fn flash_borrow_ix(s: &Setup, amount: u64) -> Instruction {
    Instruction::new_with_bytes(
        s.program_id,
        &jbl::instruction::FlashBorrow { amount }.data(),
        jbl::accounts::FlashBorrow {
            pool: s.pool_pubkey,
            state: s.state_pda,
            lend_mint: s.lend_mint,
            lend_vault: s.lend_vault_pda,
            user_destination: s.user_lend_account,
            sysvar_instructions: solana_sdk_ids::sysvar::instructions::ID,
            token_program: spl_token::id(),
        }
        .to_account_metas(None),
    )
}

fn flash_repay_ix(s: &Setup, amount: u64) -> Instruction {
    Instruction::new_with_bytes(
        s.program_id,
        &jbl::instruction::FlashRepay { amount }.data(),
        jbl::accounts::FlashRepay {
            pool: s.pool_pubkey,
            lend_mint: s.lend_mint,
            lend_vault: s.lend_vault_pda,
            user_source: s.user_lend_account,
            authority: s.payer.pubkey(),
            sysvar_instructions: solana_sdk_ids::sysvar::instructions::ID,
            token_program: spl_token::id(),
        }
        .to_account_metas(None),
    )
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn test_flash_loan_borrow_repay_same_tx() {
    const SEED: u64 = 1_000_000;
    const BORROW: u64 = 500_000;
    const FEE: u64 = BORROW * 9 / 10_000; // = 45
    const REPAY: u64 = BORROW + FEE;

    let mut s = setup(SEED);

    // Pre-fund user with enough to repay (borrow proceeds + fee).
    // The borrow deposits BORROW into user_lend_account; user needs FEE extra.
    let fund_ix = mint_to_ix(&s.lend_mint, &s.user_lend_account, &s.payer.pubkey(), FEE);
    send_ixs(&mut s.svm, &[fund_ix], &s.payer, &[&s.payer]);

    let borrow_ix = flash_borrow_ix(&s, BORROW);
    let repay_ix = flash_repay_ix(&s, REPAY);

    // Both instructions in the same transaction.
    send_ixs(&mut s.svm, &[borrow_ix, repay_ix], &s.payer, &[&s.payer]);

    // lend_vault balance should be SEED + FEE (net +45 tokens).
    let vault_balance = read_token_balance(&s.svm, &s.lend_vault_pda);
    assert_eq!(vault_balance, SEED + FEE, "lend_vault balance mismatch");

    // pool.total_lend_deposited = SEED - BORROW + REPAY = SEED + FEE
    let total_lend = read_total_lend_deposited(&s.svm, &s.pool_pubkey);
    assert_eq!(total_lend, SEED + FEE, "pool.total_lend_deposited mismatch");
}

#[test]
fn test_flash_loan_repay_without_borrow_fails() {
    const SEED: u64 = 1_000_000;
    const BORROW: u64 = 500_000;
    const FEE: u64 = BORROW * 9 / 10_000;
    const REPAY: u64 = BORROW + FEE;

    let mut s = setup(SEED);

    // Give user enough tokens to repay.
    let fund_ix = mint_to_ix(&s.lend_mint, &s.user_lend_account, &s.payer.pubkey(), REPAY);
    send_ixs(&mut s.svm, &[fund_ix], &s.payer, &[&s.payer]);

    // Transaction with only flash_repay — no preceding flash_borrow.
    let repay_ix = flash_repay_ix(&s, REPAY);
    let succeeded = try_send_ixs(&mut s.svm, &[repay_ix], &s.payer, &[&s.payer]);
    assert!(!succeeded, "expected flash_repay without borrow to fail");
}

#[test]
fn test_flash_loan_borrow_without_repay_fails() {
    const SEED: u64 = 1_000_000;
    const BORROW: u64 = 500_000;

    let mut s = setup(SEED);

    // Transaction with only flash_borrow — no following flash_repay.
    let borrow_ix = flash_borrow_ix(&s, BORROW);
    let succeeded = try_send_ixs(&mut s.svm, &[borrow_ix], &s.payer, &[&s.payer]);
    assert!(!succeeded, "expected flash_borrow without repay to fail");
}

#[test]
fn test_flash_loan_underpay_fails() {
    const SEED: u64 = 1_000_000;
    const BORROW: u64 = 500_000;
    // Repay exact principal — no fee — flash_borrow should reject this.
    const REPAY: u64 = BORROW;

    let mut s = setup(SEED);

    let fund_ix = mint_to_ix(&s.lend_mint, &s.user_lend_account, &s.payer.pubkey(), REPAY);
    send_ixs(&mut s.svm, &[fund_ix], &s.payer, &[&s.payer]);

    let borrow_ix = flash_borrow_ix(&s, BORROW);
    let repay_ix = flash_repay_ix(&s, REPAY);

    let succeeded = try_send_ixs(&mut s.svm, &[borrow_ix, repay_ix], &s.payer, &[&s.payer]);
    assert!(!succeeded, "expected underpay to fail");
}

#[test]
fn test_flash_loan_leveraged_swap() {
    const SEED: u64 = 2_000_000;
    const INITIAL_COLLATERAL: u64 = 100_000;
    const BORROW: u64 = 500_000;
    const FEE: u64 = BORROW * 9 / 10_000; // = 45
    const REPAY: u64 = BORROW + FEE;

    let mut s = setup(SEED);

    let program_id = s.program_id;
    let payer_pk = s.payer.pubkey();

    // ── Fund user ─────────────────────────────────────────────────────────────
    // User needs:
    //   - INITIAL_COLLATERAL collateral tokens for the initial deposit
    //   - REPAY lend tokens so that after flash_borrow (adds BORROW) and
    //     mock_swap (burns BORROW), they still have REPAY = BORROW + FEE left.
    //     i.e. net pre-fund = REPAY (500_045) lend tokens
    let fund_col_ix = mint_to_ix(
        &s.collateral_mint,
        &s.user_collateral_account,
        &payer_pk,
        INITIAL_COLLATERAL,
    );
    let fund_lend_ix = mint_to_ix(&s.lend_mint, &s.user_lend_account, &payer_pk, REPAY);
    send_ixs(
        &mut s.svm,
        &[fund_col_ix, fund_lend_ix],
        &s.payer,
        &[&s.payer],
    );

    // ── Initial collateral deposit (separate transaction) ─────────────────────
    let (user_position_pda, _) = find_user_position_pda(&s.pool_pubkey, &payer_pk, &program_id);
    let deposit_initial_ix = Instruction::new_with_bytes(
        program_id,
        &jbl::instruction::DepositCollateral {
            amount: INITIAL_COLLATERAL,
        }
        .data(),
        jbl::accounts::DepositCollateral {
            pool: s.pool_pubkey,
            collateral_mint: s.collateral_mint,
            authority: payer_pk,
            user_token_account: s.user_collateral_account,
            collateral_vault: s.collateral_vault_pda,
            user_position: user_position_pda,
            token_program: spl_token::id(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
    send_ixs(&mut s.svm, &[deposit_initial_ix], &s.payer, &[&s.payer]);

    // ── Leveraged swap transaction ─────────────────────────────────────────────
    // 1. flash_borrow(BORROW)  — receive BORROW lend tokens into user_lend_account
    // 2. mock_swap(BORROW)     — burn BORROW lend, mint BORROW collateral
    // 3. deposit(BORROW)       — deposit BORROW collateral into pool
    // 4. flash_repay(REPAY)    — repay BORROW + FEE from user_lend_account
    //
    // User lend account balance trace:
    //   pre-funded:    REPAY  (500_045)
    //   +flash_borrow: REPAY + BORROW  (1_000_045)
    //   -mock_swap:    REPAY  (500_045)
    //   -flash_repay:  0

    let borrow_ix = flash_borrow_ix(&s, BORROW);

    let swap_ix = Instruction::new_with_bytes(
        program_id,
        &jbl::instruction::MockSwap { amount: BORROW }.data(),
        jbl::accounts::MockSwap {
            mint_authority: payer_pk,
            token_owner: payer_pk,
            mint_in: s.lend_mint,
            mint_out: s.collateral_mint,
            user_token_in: s.user_lend_account,
            user_token_out: s.user_collateral_account,
            token_program: spl_token::id(),
        }
        .to_account_metas(None),
    );

    let deposit_leveraged_ix = Instruction::new_with_bytes(
        program_id,
        &jbl::instruction::DepositCollateral { amount: BORROW }.data(),
        jbl::accounts::DepositCollateral {
            pool: s.pool_pubkey,
            collateral_mint: s.collateral_mint,
            authority: payer_pk,
            user_token_account: s.user_collateral_account,
            collateral_vault: s.collateral_vault_pda,
            user_position: user_position_pda,
            token_program: spl_token::id(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );

    let repay_ix = flash_repay_ix(&s, REPAY);

    send_ixs(
        &mut s.svm,
        &[borrow_ix, swap_ix, deposit_leveraged_ix, repay_ix],
        &s.payer,
        &[&s.payer],
    );

}
