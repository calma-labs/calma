mod common;
use common::{
    create_mint_ixs, create_token_account_ixs, mint_to_ix, read_token_balance, send_ixs,
    try_send_ixs,
};

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::program_pack::Pack;
use calma::state::Pool;
use {
    anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas},
    anchor_spl::token::spl_token,
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

fn setup_svm() -> (LiteSVM, Keypair, Keypair, Keypair) {
    let mut svm = LiteSVM::new();
    svm.add_program(calma::id(), include_bytes!("../../../target/deploy/calma.so"))
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

/// Mint a fresh collateral/lend pair, both with `payer` as mint authority.
fn fresh_mints(svm: &mut LiteSVM, payer: &Keypair) -> (Pubkey, Pubkey) {
    let col = Keypair::new();
    let lend = Keypair::new();
    let rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN);
    let [cc, ci] = create_mint_ixs(&payer.pubkey(), &col.pubkey(), &payer.pubkey(), rent);
    let [lc, li] = create_mint_ixs(&payer.pubkey(), &lend.pubkey(), &payer.pubkey(), rent);
    send_ixs(svm, &[cc, ci, lc, li], payer, &[payer, &col, &lend]);
    (col.pubkey(), lend.pubkey())
}

/// Read a `Pool` out of the SVM, skipping the 8-byte discriminator.
fn read_pool(svm: &LiteSVM, pool: &Pubkey) -> Pool {
    let data = svm.get_account(pool).unwrap().data;
    bytemuck::pod_read_unaligned(&data[8..8 + std::mem::size_of::<Pool>()])
}

/// Create the guard state PDA owned by `guard_authority` and return its address.
fn create_guard(svm: &mut LiteSVM, guard_authority: &Keypair, payer: &Keypair) -> Pubkey {
    let guard_id = guard::id();
    // Whitelists are per-authority: ["guard", authority]. Several may coexist,
    // each covering a different subset. See guard::instructions::initialize.
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

/// Build a `calma::create` instruction, optionally including a guard.
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
        &calma::instruction::Create {
            ltv_percent: 75,
            max_feed_age_ms: 90_000u32,
        }
        .data(),
        calma::accounts::Create {
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
    authority: &Keypair,
    collateral_mint: Pubkey,
    lend_mint: Pubkey,
) -> PoolSetup {
    let program_id = calma::id();
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
            // `calma::create` requires the market's authority to own its rate
            // curve, so the IRM must be initialised under the same key.
            authority: authority.pubkey(),
            payer: payer.pubkey(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
    send_ixs(
        svm,
        &[create_pool_account_ix, irm_init_ix],
        payer,
        &[payer, &pool_keypair, authority],
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

    let setup = prepare_pool(&mut svm, &payer, &authority, col_mint_kp.pubkey(), lend_mint_kp.pubkey());

    let ix = create_pool_ix(
        calma::id(),
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

    send_ixs(&mut svm, &[ix], &payer, &[&payer, &authority, &setup.pool_keypair]);
}

#[test]
fn test_create_with_guard_not_whitelisted() {
    let (mut svm, payer, col_mint_kp, lend_mint_kp) = setup_svm();
    let authority = Keypair::new();

    // Create a guard but do NOT add the pool authority to the whitelist.
    let guard_pda = create_guard(&mut svm, &payer, &payer);

    let setup = prepare_pool(&mut svm, &payer, &authority, col_mint_kp.pubkey(), lend_mint_kp.pubkey());

    let ix = create_pool_ix(
        calma::id(),
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
        !try_send_ixs(&mut svm, &[ix], &payer, &[&payer, &authority, &setup.pool_keypair]),
        "expected pool creation to fail for non-whitelisted authority"
    );
}

// ── whitelist gating on entry points ─────────────────────────────────────────

const COL_DEPOSIT: u64 = 1_000_000;

/// Prepare + create a market, optionally bound to `guard`. Returns the pool.
fn create_pool(
    svm: &mut LiteSVM,
    payer: &Keypair,
    authority: &Keypair,
    col_mint: Pubkey,
    lend_mint: Pubkey,
    guard: Option<(Pubkey, Pubkey)>,
) -> (Pubkey, PoolSetup) {
    let setup = prepare_pool(svm, payer, authority, col_mint, lend_mint);
    let ix = create_pool_ix(
        calma::id(),
        feed::id(),
        setup.feed_pda,
        irm::id(),
        setup.irm_config,
        setup.pool_keypair.pubkey(),
        setup.state_pda,
        col_mint,
        lend_mint,
        setup.collateral_vault,
        setup.lend_vault,
        setup.lp_mint,
        authority.pubkey(),
        payer.pubkey(),
        guard,
    );
    send_ixs(svm, &[ix], payer, &[payer, authority, &setup.pool_keypair]);
    (setup.pool_keypair.pubkey(), setup)
}

/// Fund `owner` with collateral and return their token account.
fn fund_collateral(
    svm: &mut LiteSVM,
    payer: &Keypair,
    col_mint: Pubkey,
    owner: &Keypair,
) -> Pubkey {
    // Fresh blockhash first: LiteSVM dedupes byte-identical transactions, and an
    // owner that was already airdropped the same amount would replay as
    // `AlreadyProcessed`.
    svm.expire_blockhash();
    if svm.get_balance(&owner.pubkey()).unwrap_or(0) == 0 {
        svm.airdrop(&owner.pubkey(), 10_000_000_000).unwrap();
    }
    let account_kp = Keypair::new();
    let rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Account::LEN);
    let [ca, ia] = create_token_account_ixs(
        &payer.pubkey(),
        &account_kp.pubkey(),
        &col_mint,
        &owner.pubkey(),
        rent,
    );
    let mint_ix = mint_to_ix(
        &col_mint,
        &account_kp.pubkey(),
        &payer.pubkey(),
        COL_DEPOSIT * 10,
    );
    send_ixs(svm, &[ca, ia, mint_ix], payer, &[payer, &account_kp]);
    account_kp.pubkey()
}

fn deposit_collateral_ix(
    pool: Pubkey,
    col_mint: Pubkey,
    depositor: Pubkey,
    user_token_account: Pubkey,
    collateral_vault: Pubkey,
    guard: Option<(Pubkey, Pubkey)>,
) -> Instruction {
    let (user_position, _) = Pubkey::find_program_address(
        &[b"user_position", pool.as_ref(), depositor.as_ref()],
        &calma::id(),
    );
    let (guard_program, guard_state) = match guard {
        Some((gp, gs)) => (Some(gp), Some(gs)),
        None => (None, None),
    };
    Instruction::new_with_bytes(
        calma::id(),
        &calma::instruction::DepositCollateral {
            amount: COL_DEPOSIT,
        }
        .data(),
        calma::accounts::DepositCollateral {
            pool,
            collateral_mint: col_mint,
            authority: depositor,
            user_token_account,
            collateral_vault,
            user_position,
            guard_program,
            guard_state,
            token_program: spl_token::id(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    )
}

/// Whitelists are per-authority, so several coexist over different subsets.
#[test]
fn test_guards_are_per_authority() {
    let (mut svm, payer, _c, _l) = setup_svm();

    let authority_b = Keypair::new();
    svm.airdrop(&authority_b.pubkey(), 10_000_000_000).unwrap();

    let guard_a = create_guard(&mut svm, &payer, &payer);
    let guard_b = create_guard(&mut svm, &authority_b, &payer);

    assert_ne!(guard_a, guard_b, "distinct authorities must get distinct PDAs");
    assert!(svm.get_account(&guard_a).is_some());
    assert!(svm.get_account(&guard_b).is_some());

    // A member of one list is not a member of the other.
    let member = Keypair::new();
    guard_add(&mut svm, &guard_a, &payer, member.pubkey());

    let state_a: guard::GuardState = {
        let data = svm.get_account(&guard_a).unwrap().data;
        anchor_lang::AccountDeserialize::try_deserialize(&mut data.as_slice()).unwrap()
    };
    let state_b: guard::GuardState = {
        let data = svm.get_account(&guard_b).unwrap().data;
        anchor_lang::AccountDeserialize::try_deserialize(&mut data.as_slice()).unwrap()
    };
    assert!(state_a.whitelist.contains(&member.pubkey()));
    assert!(state_b.whitelist.is_empty());
}

/// The market records the guard it was created against, and an ungated market
/// records the default pubkey.
#[test]
fn test_pool_records_its_guard() {
    let (mut svm, payer, col_kp, lend_kp) = setup_svm();
    let authority = Keypair::new();
    let guard_pda = create_guard(&mut svm, &payer, &payer);
    guard_add(&mut svm, &guard_pda, &payer, authority.pubkey());

    let (gated, _) = create_pool(
        &mut svm,
        &payer,
        &authority,
        col_kp.pubkey(),
        lend_kp.pubkey(),
        Some((guard::id(), guard_pda)),
    );
    assert_eq!(read_pool(&svm, &gated).guard_state, guard_pda);

    // A second market on fresh mints, ungated.
    let (open_col, open_lend) = fresh_mints(&mut svm, &payer);
    let (open, _) = create_pool(&mut svm, &payer, &authority, open_col, open_lend, None);
    assert_eq!(read_pool(&svm, &open).guard_state, Pubkey::default());
}

/// A gated market refuses a deposit that omits the guard accounts. This is the
/// security property: an optional account a caller can leave out is not a gate.
#[test]
fn test_gated_pool_rejects_deposit_without_guard_accounts() {
    let (mut svm, payer, col_kp, lend_kp) = setup_svm();
    let authority = Keypair::new();
    let guard_pda = create_guard(&mut svm, &payer, &payer);
    guard_add(&mut svm, &guard_pda, &payer, authority.pubkey());

    let depositor = Keypair::new();
    guard_add(&mut svm, &guard_pda, &payer, depositor.pubkey());

    let (pool, setup) = create_pool(
        &mut svm,
        &payer,
        &authority,
        col_kp.pubkey(),
        lend_kp.pubkey(),
        Some((guard::id(), guard_pda)),
    );
    let user_account = fund_collateral(&mut svm, &payer, col_kp.pubkey(), &depositor);

    // Whitelisted, but the guard accounts are missing.
    let ix = deposit_collateral_ix(
        pool,
        col_kp.pubkey(),
        depositor.pubkey(),
        user_account,
        setup.collateral_vault,
        None,
    );
    assert!(
        !try_send_ixs(&mut svm, &[ix], &payer, &[&payer, &depositor]),
        "gated market must reject a deposit with no guard accounts"
    );
}

/// A caller cannot substitute a whitelist of their own making — guards are
/// permissionless to create, so only the pinned address is accepted.
#[test]
fn test_gated_pool_rejects_substituted_guard() {
    let (mut svm, payer, col_kp, lend_kp) = setup_svm();
    let authority = Keypair::new();
    let guard_pda = create_guard(&mut svm, &payer, &payer);
    guard_add(&mut svm, &guard_pda, &payer, authority.pubkey());

    let (pool, setup) = create_pool(
        &mut svm,
        &payer,
        &authority,
        col_kp.pubkey(),
        lend_kp.pubkey(),
        Some((guard::id(), guard_pda)),
    );

    // The attacker stands up their own canonical guard and adds themselves.
    let attacker = Keypair::new();
    svm.airdrop(&attacker.pubkey(), 10_000_000_000).unwrap();
    let rogue_guard = create_guard(&mut svm, &attacker, &payer);
    guard_add(&mut svm, &rogue_guard, &attacker, attacker.pubkey());

    let user_account = fund_collateral(&mut svm, &payer, col_kp.pubkey(), &attacker);
    let ix = deposit_collateral_ix(
        pool,
        col_kp.pubkey(),
        attacker.pubkey(),
        user_account,
        setup.collateral_vault,
        Some((guard::id(), rogue_guard)),
    );
    assert!(
        !try_send_ixs(&mut svm, &[ix], &payer, &[&payer, &attacker]),
        "gated market must reject a guard other than the one it pinned"
    );
}

/// End-to-end: on the list deposits, off the list does not.
#[test]
fn test_gated_pool_admits_only_whitelisted_depositors() {
    let (mut svm, payer, col_kp, lend_kp) = setup_svm();
    let authority = Keypair::new();
    let guard_pda = create_guard(&mut svm, &payer, &payer);
    guard_add(&mut svm, &guard_pda, &payer, authority.pubkey());

    let (pool, setup) = create_pool(
        &mut svm,
        &payer,
        &authority,
        col_kp.pubkey(),
        lend_kp.pubkey(),
        Some((guard::id(), guard_pda)),
    );

    let outsider = Keypair::new();
    let outsider_account = fund_collateral(&mut svm, &payer, col_kp.pubkey(), &outsider);
    let ix = deposit_collateral_ix(
        pool,
        col_kp.pubkey(),
        outsider.pubkey(),
        outsider_account,
        setup.collateral_vault,
        Some((guard::id(), guard_pda)),
    );
    assert!(
        !try_send_ixs(&mut svm, &[ix], &payer, &[&payer, &outsider]),
        "non-whitelisted depositor must be rejected"
    );

    // Same caller, once added to the list. The retry is byte-identical to the
    // rejected attempt, so it needs a fresh blockhash to avoid LiteSVM's
    // duplicate-transaction check masking the result.
    guard_add(&mut svm, &guard_pda, &payer, outsider.pubkey());
    svm.expire_blockhash();
    let ix = deposit_collateral_ix(
        pool,
        col_kp.pubkey(),
        outsider.pubkey(),
        outsider_account,
        setup.collateral_vault,
        Some((guard::id(), guard_pda)),
    );
    send_ixs(&mut svm, &[ix], &payer, &[&payer, &outsider]);
    assert_eq!(read_token_balance(&svm, &setup.collateral_vault), COL_DEPOSIT);
}
