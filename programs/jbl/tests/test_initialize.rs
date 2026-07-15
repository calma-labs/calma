mod common;
use common::{create_mint_ixs, send_ixs};

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

fn find_collateral_vault_pda(pool: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"collateral_vault", pool.as_ref()], program_id)
}

fn find_lend_vault_pda(pool: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"lend_vault", pool.as_ref()], program_id)
}

fn find_lp_mint_pda(pool: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"lp_mint", pool.as_ref()], program_id)
}

#[test]
fn test_create() {
    let program_id = jbl::id();
    let feed_id = feed::id();
    let payer = Keypair::new();
    let authority = Keypair::new();
    let collateral_mint_keypair = Keypair::new();
    let lend_mint_keypair = Keypair::new();

    let irm_id = irm::id();

    let mut svm = LiteSVM::new();
    svm.add_program(program_id, include_bytes!("../../../target/deploy/jbl.so"))
        .unwrap();
    svm.add_program(feed_id, include_bytes!("../../../target/deploy/feed.so"))
        .unwrap();
    svm.add_program(irm_id, include_bytes!("../../../target/deploy/irm.so"))
        .unwrap();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();

    // ── Create collateral and lend SPL mints ─────────────────────────────────
    let mint_rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN);
    let [cc, ci] = create_mint_ixs(
        &payer.pubkey(),
        &collateral_mint_keypair.pubkey(),
        &payer.pubkey(),
        mint_rent,
    );
    let [lc, li] = create_mint_ixs(
        &payer.pubkey(),
        &lend_mint_keypair.pubkey(),
        &payer.pubkey(),
        mint_rent,
    );
    send_ixs(
        &mut svm,
        &[cc, ci, lc, li],
        &payer,
        &[&payer, &collateral_mint_keypair, &lend_mint_keypair],
    );

    // ── Create pool keypair and pre-allocate account ──────────────────────────
    let pool_keypair = Keypair::new();
    let pool_pubkey = pool_keypair.pubkey();

    let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &program_id);
    let (collateral_vault_pda, _) = find_collateral_vault_pda(&pool_pubkey, &program_id);
    let (lend_vault_pda, _) = find_lend_vault_pda(&pool_pubkey, &program_id);
    let (lp_mint_pda, _) = find_lp_mint_pda(&pool_pubkey, &program_id);

    // ── Create feed account ───────────────────────────────────────────────────
    // `payer` is used as the feed authority so it can sign set_value.
    let (feed_pda, _) = Pubkey::find_program_address(
        &[b"feed", collateral_mint_keypair.pubkey().as_ref(), lend_mint_keypair.pubkey().as_ref(), &[0u8]],
        &feed_id,
    );
    let feed_create_ix = Instruction::new_with_bytes(
        feed_id,
        &feed::instruction::Create {
            id: 0,
            source: feed::state::PriceSource::Manual,
            collateral_feed_id: [0u8; 32],
            lend_feed_id: [0u8; 32],
            rules: feed::state::FeedRules::default(),
        }
        .data(),
        feed::accounts::Create {
            feed: feed_pda,
            authority: payer.pubkey(),
            collateral_mint: collateral_mint_keypair.pubkey(),
            lend_mint: lend_mint_keypair.pubkey(),
            payer: payer.pubkey(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
    let (irm_config, _) =
        Pubkey::find_program_address(&[b"irm_config", pool_pubkey.as_ref()], &irm_id);

    let feed_set_value_ix = Instruction::new_with_bytes(
        feed_id,
        &feed::instruction::SetValue {
            collateral_price: 1_000_000,
            lend_price: 1_000_000,
        }
        .data(),
        feed::accounts::SetValue {
            feed: feed_pda,
            authority: payer.pubkey(),
        }
        .to_account_metas(None),
    );
    send_ixs(
        &mut svm,
        &[feed_create_ix, feed_set_value_ix],
        &payer,
        &[&payer],
    );

    // Pre-allocate pool account and initialize IRM
    let pool_space = 8 + std::mem::size_of::<Pool>();
    let pool_rent = svm.minimum_balance_for_rent_exemption(pool_space);
    let create_pool_account_ix = anchor_lang::solana_program::system_instruction::create_account(
        &payer.pubkey(),
        &pool_pubkey,
        pool_rent,
        pool_space as u64,
        &program_id,
    );
    let irm_init_ix = Instruction::new_with_bytes(
        irm_id,
        &irm::instruction::Initialize {
            points: vec![
                irm::RatePointArgs { util_bps: 0, rate_bps: 0 },
                irm::RatePointArgs { util_bps: 10_000, rate_bps: 500 },
            ],
        }
        .data(),
        irm::accounts::Initialize {
            irm_config,
            pool: pool_pubkey,
            authority: payer.pubkey(),
            payer: payer.pubkey(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
    send_ixs(
        &mut svm,
        &[create_pool_account_ix, irm_init_ix],
        &payer,
        &[&payer, &pool_keypair],
    );

    // ── Build create instruction ──────────────────────────────────────────────
    let instruction = Instruction::new_with_bytes(
        program_id,
        &jbl::instruction::Create {
            ltv_percent: 75,
            max_feed_age_secs: 90u32,
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
            feed_program: feed_id,
            feed_state: feed_pda,
            rate_program: irm_id,
            irm_state: irm_config,
            guard_program: None,
            guard_state: None,
            token_program: spl_token::id(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );

    send_ixs(&mut svm, &[instruction], &payer, &[&payer, &authority]);
}
