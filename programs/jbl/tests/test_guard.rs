mod common;
use common::{create_mint_ixs, send_ixs, try_send_ixs};

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

fn setup_svm() -> (LiteSVM, Keypair, Keypair, Keypair) {
    let mut svm = LiteSVM::new();
    svm.add_program(jbl::id(), include_bytes!("../../../target/deploy/jbl.so"))
        .unwrap();
    svm.add_program(feed::id(), include_bytes!("../../../target/deploy/feed.so"))
        .unwrap();
    svm.add_program(irm::id(), include_bytes!("../../../target/deploy/irm.so"))
        .unwrap();
    svm.add_program(
        guard::id(),
        include_bytes!("../../../target/deploy/guard.so"),
    )
    .unwrap();

    let payer = Keypair::new();
    let collateral_mint_kp = Keypair::new();
    let lend_mint_kp = Keypair::new();

    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();

    let mint_rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN);
    let [cc, ci] = create_mint_ixs(
        &payer.pubkey(),
        &collateral_mint_kp.pubkey(),
        &payer.pubkey(),
        mint_rent,
    );
    let [lc, li] = create_mint_ixs(
        &payer.pubkey(),
        &lend_mint_kp.pubkey(),
        &payer.pubkey(),
        mint_rent,
    );
    send_ixs(
        &mut svm,
        &[cc, ci, lc, li],
        &payer,
        &[&payer, &collateral_mint_kp, &lend_mint_kp],
    );

    (svm, payer, collateral_mint_kp, lend_mint_kp)
}

/// Create the guard state PDA and return its address.
fn create_guard(svm: &mut LiteSVM, guard_authority: &Keypair, payer: &Keypair) -> Pubkey {
    let guard_id = guard::id();
    let (guard_pda, _) =
        Pubkey::find_program_address(&[b"guard", guard_authority.pubkey().as_ref()], &guard_id);

    let ix = Instruction::new_with_bytes(
        guard_id,
        &guard::instruction::Create {}.data(),
        guard::accounts::Create {
            guard_state: guard_pda,
            authority: guard_authority.pubkey(),
            payer: payer.pubkey(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
    let signers: Vec<&Keypair> = if guard_authority.pubkey() == payer.pubkey() {
        vec![payer]
    } else {
        vec![payer, guard_authority]
    };
    send_ixs(svm, &[ix], payer, &signers);
    guard_pda
}

/// Add `pubkey` to an existing guard whitelist.
fn guard_add(svm: &mut LiteSVM, guard_pda: &Pubkey, guard_authority: &Keypair, pubkey: Pubkey) {
    let ix = Instruction::new_with_bytes(
        guard::id(),
        &guard::instruction::Add { pubkey }.data(),
        guard::accounts::Add {
            guard_state: *guard_pda,
            authority: guard_authority.pubkey(),
        }
        .to_account_metas(None),
    );
    send_ixs(svm, &[ix], guard_authority, &[guard_authority]);
}

/// Build a `jbl::create` instruction, optionally including a guard.
fn create_pool_ix(
    program_id: Pubkey,
    feed_id: Pubkey,
    feed_pda: Pubkey,
    irm_id: Pubkey,
    irm_config: Pubkey,
    pool_pubkey: Pubkey,
    state_pda: Pubkey,
    collateral_mint: Pubkey,
    lend_mint: Pubkey,
    collateral_vault: Pubkey,
    lend_vault: Pubkey,
    lp_mint: Pubkey,
    authority: Pubkey,
    payer: Pubkey,
    guard: Option<(Pubkey, Pubkey)>,
) -> Instruction {
    let (guard_program, guard_state) = match guard {
        Some((gp, gs)) => (Some(gp), Some(gs)),
        None => (None, None),
    };
    Instruction::new_with_bytes(
        program_id,
        &jbl::instruction::Create {
            ltv_percent: 75,
            max_feed_age_secs: 90u32,
        }
        .data(),
        jbl::accounts::Create {
            pool: pool_pubkey,
            state: state_pda,
            collateral_vault,
            lend_vault,
            lp_mint,
            collateral_mint,
            lend_mint,
            authority,
            payer,
            feed_program: feed_id,
            feed_state: feed_pda,
            rate_program: irm_id,
            irm_state: irm_config,
            guard_program,
            guard_state,
            token_program: spl_token::id(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    )
}

struct PoolSetup {
    pool_keypair: Keypair,
    state_pda: Pubkey,
    collateral_vault: Pubkey,
    lend_vault: Pubkey,
    lp_mint: Pubkey,
    feed_pda: Pubkey,
    irm_config: Pubkey,
}

fn prepare_pool(
    svm: &mut LiteSVM,
    payer: &Keypair,
    collateral_mint: Pubkey,
    lend_mint: Pubkey,
) -> PoolSetup {
    let program_id = jbl::id();
    let feed_id = feed::id();
    let irm_id = irm::id();

    let pool_keypair = Keypair::new();
    let pool_pubkey = pool_keypair.pubkey();

    let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &program_id);
    let (collateral_vault, _) =
        Pubkey::find_program_address(&[b"collateral_vault", pool_pubkey.as_ref()], &program_id);
    let (lend_vault, _) =
        Pubkey::find_program_address(&[b"lend_vault", pool_pubkey.as_ref()], &program_id);
    let (lp_mint, _) =
        Pubkey::find_program_address(&[b"lp_mint", pool_pubkey.as_ref()], &program_id);
    let (feed_pda, _) = Pubkey::find_program_address(
        &[b"feed", collateral_mint.as_ref(), lend_mint.as_ref(), &[0u8]],
        &feed_id,
    );
    let (irm_config, _) =
        Pubkey::find_program_address(&[b"irm_config", pool_pubkey.as_ref()], &irm_id);

    // Create feed
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
            collateral_mint,
            lend_mint,
            payer: payer.pubkey(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
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
    send_ixs(svm, &[feed_create_ix, feed_set_value_ix], payer, &[payer]);

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
        svm,
        &[create_pool_account_ix, irm_init_ix],
        payer,
        &[payer, &pool_keypair],
    );

    PoolSetup {
        pool_keypair,
        state_pda,
        collateral_vault,
        lend_vault,
        lp_mint,
        feed_pda,
        irm_config,
    }
}

#[test]
fn test_create_with_guard_whitelisted() {
    let (mut svm, payer, col_mint_kp, lend_mint_kp) = setup_svm();
    let authority = Keypair::new();

    // Create a guard owned by payer and whitelist the pool authority.
    let guard_pda = create_guard(&mut svm, &payer, &payer);
    guard_add(&mut svm, &guard_pda, &payer, authority.pubkey());

    let setup = prepare_pool(&mut svm, &payer, col_mint_kp.pubkey(), lend_mint_kp.pubkey());

    let ix = create_pool_ix(
        jbl::id(),
        feed::id(),
        setup.feed_pda,
        irm::id(),
        setup.irm_config,
        setup.pool_keypair.pubkey(),
        setup.state_pda,
        col_mint_kp.pubkey(),
        lend_mint_kp.pubkey(),
        setup.collateral_vault,
        setup.lend_vault,
        setup.lp_mint,
        authority.pubkey(),
        payer.pubkey(),
        Some((guard::id(), guard_pda)),
    );

    send_ixs(&mut svm, &[ix], &payer, &[&payer, &authority]);
}

#[test]
fn test_create_with_guard_not_whitelisted() {
    let (mut svm, payer, col_mint_kp, lend_mint_kp) = setup_svm();
    let authority = Keypair::new();

    // Create a guard but do NOT add the pool authority to the whitelist.
    let guard_pda = create_guard(&mut svm, &payer, &payer);

    let setup = prepare_pool(&mut svm, &payer, col_mint_kp.pubkey(), lend_mint_kp.pubkey());

    let ix = create_pool_ix(
        jbl::id(),
        feed::id(),
        setup.feed_pda,
        irm::id(),
        setup.irm_config,
        setup.pool_keypair.pubkey(),
        setup.state_pda,
        col_mint_kp.pubkey(),
        lend_mint_kp.pubkey(),
        setup.collateral_vault,
        setup.lend_vault,
        setup.lp_mint,
        authority.pubkey(),
        payer.pubkey(),
        Some((guard::id(), guard_pda)),
    );

    assert!(
        !try_send_ixs(&mut svm, &[ix], &payer, &[&payer, &authority]),
        "expected pool creation to fail for non-whitelisted authority"
    );
}
