//! `irm`'s own instruction handlers.
//!
//! `crates/irm-state` unit-tests the curve arithmetic, and
//! `programs/calma/tests/*` exercise the program only as a market fixture. What
//! neither covers is this program's own gates: that `initialize` actually
//! rejects a malformed curve, that `borrow_rate` puts the right bytes in return
//! data, and that `check_authority` — the thing standing between a market and a
//! rate curve someone else controls (see `docs/known-issues.md` K-9) — answers
//! correctly.

use anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas};
use anchor_lang::prelude::Pubkey;
use irm::RatePointArgs;
use irm_state::{IrmState, MAX_RATE_BPS};
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

const IRM_SO: &[u8] = include_bytes!("../../../target/deploy/irm.so");

/// The reference curve every fixture uses: 0 bps at no utilization, 500 at full.
const LINEAR_0_TO_500: [(u16, u32); 2] = [(0, 0), (10_000, 500)];

struct Ctx {
    svm: LiteSVM,
    payer: Keypair,
    authority: Keypair,
    pool: Pubkey,
    irm_config: Pubkey,
}

fn args(points: &[(u16, u32)]) -> Vec<RatePointArgs> {
    points
        .iter()
        .map(|&(util_bps, rate_bps)| RatePointArgs { util_bps, rate_bps })
        .collect()
}

fn build(
    svm: &mut LiteSVM,
    ixs: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
) -> VersionedTransaction {
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &bh);
    VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap()
}

fn initialize_ix(ctx: &Ctx, points: &[(u16, u32)]) -> Instruction {
    Instruction::new_with_bytes(
        irm::id(),
        &irm::instruction::Initialize {
            points: args(points),
        }
        .data(),
        irm::accounts::Initialize {
            irm_config: ctx.irm_config,
            pool: ctx.pool,
            authority: ctx.authority.pubkey(),
            payer: ctx.payer.pubkey(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    )
}

/// An SVM with the program loaded and PDAs derived, but nothing initialized.
fn setup() -> Ctx {
    let payer = Keypair::new();
    let authority = Keypair::new();
    let pool = Pubkey::new_unique();
    let mut svm = LiteSVM::new();
    svm.add_program(irm::id(), IRM_SO).unwrap();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();

    let irm_config =
        Pubkey::find_program_address(&[irm_state::IRM_CONFIG_SEED, pool.as_ref()], &irm::id()).0;

    Ctx {
        svm,
        payer,
        authority,
        pool,
        irm_config,
    }
}

fn setup_initialized() -> Ctx {
    let mut ctx = setup();
    let ix = initialize_ix(&ctx, &LINEAR_0_TO_500);
    let (payer, authority) = (ctx.payer.insecure_clone(), ctx.authority.insecure_clone());
    let tx = build(&mut ctx.svm, &[ix], &payer, &[&payer, &authority]);
    ctx.svm.send_transaction(tx).expect("initialize should succeed");
    ctx
}

fn read_state(ctx: &Ctx) -> IrmState {
    let data = ctx.svm.get_account(&ctx.irm_config).unwrap().data;
    *bytemuck::from_bytes::<IrmState>(&data[8..8 + std::mem::size_of::<IrmState>()])
}

// ── initialize ───────────────────────────────────────────────────────────────

#[test]
fn initialize_writes_the_curve_and_binds_it_to_pool_and_authority() {
    let ctx = setup_initialized();
    let state = read_state(&ctx);
    assert_eq!(state.pool, ctx.pool);
    assert_eq!(state.authority, ctx.authority.pubkey());
    assert_eq!(state.model.len, 2);
    assert_eq!(state.model.points[0].util_bps, 0);
    assert_eq!(state.model.points[1].rate_bps, 500);
}

/// `initialize` is the only gate on the curve — `get_fee_bps` performs no
/// runtime checks and trusts these invariants, so a malformed curve written here
/// is a malformed curve read forever.
#[test]
fn initialize_rejects_every_malformed_curve() {
    let cases: [(&str, &[(u16, u32)]); 5] = [
        ("only one point", &[(0, 100)]),
        ("five points exceeds MAX_POINTS", &[(0, 0), (1, 1), (2, 2), (3, 3), (4, 4)]),
        ("not anchored at zero utilization", &[(1, 0), (10_000, 500)]),
        ("utilization not strictly increasing", &[(0, 0), (0, 500)]),
        ("rate above MAX_RATE_BPS", &[(0, 0), (10_000, MAX_RATE_BPS + 1)]),
    ];

    for (label, points) in cases {
        let mut ctx = setup();
        let ix = initialize_ix(&ctx, points);
        let (payer, authority) = (ctx.payer.insecure_clone(), ctx.authority.insecure_clone());
        let tx = build(&mut ctx.svm, &[ix], &payer, &[&payer, &authority]);
        assert!(
            ctx.svm.send_transaction(tx).is_err(),
            "initialize must reject: {label}"
        );
    }
}

// ── borrow_rate ──────────────────────────────────────────────────────────────

/// `calma` reads this answer out of return data as a little-endian `u32` and
/// believes it. The value and its encoding are the whole rate ABI.
#[test]
fn borrow_rate_returns_the_interpolated_rate_as_le_u32() {
    let mut ctx = setup_initialized();
    let payer = ctx.payer.insecure_clone();

    for (utilization, expected) in [(0u64, 0u32), (5_000, 250), (10_000, 500)] {
        let ix = Instruction::new_with_bytes(
            irm::id(),
            &irm::instruction::BorrowRate {
                utilization_bps: utilization,
            }
            .data(),
            irm::accounts::BorrowRate {
                irm_state: ctx.irm_config,
                pool: ctx.pool,
            }
            .to_account_metas(None),
        );
        let tx = build(&mut ctx.svm, &[ix], &payer, &[&payer]);
        let meta = ctx.svm.send_transaction(tx).expect("borrow_rate should succeed");
        let bytes = &meta.return_data.data;
        let got = u32::from_le_bytes(bytes[..4].try_into().unwrap());
        assert_eq!(got, expected, "rate at {utilization} bps utilization");
    }
}

// ── check_authority ──────────────────────────────────────────────────────────

/// The check `calma::create` relies on. If it answered `Ok` for the wrong key, a
/// market could be created already bound to a curve a third party controls.
#[test]
fn check_authority_accepts_only_the_key_that_claimed_the_pda() {
    let mut ctx = setup_initialized();
    let payer = ctx.payer.insecure_clone();

    let check = |who: Pubkey| {
        Instruction::new_with_bytes(
            irm::id(),
            &irm::instruction::CheckAuthority { authority: who }.data(),
            irm::accounts::CheckAuthority {
                irm_state: ctx.irm_config,
                pool: ctx.pool,
            }
            .to_account_metas(None),
        )
    };

    let ix = check(ctx.authority.pubkey());
    let tx = build(&mut ctx.svm, &[ix], &payer, &[&payer]);
    assert!(ctx.svm.send_transaction(tx).is_ok(), "the real authority must pass");

    let ix = check(Pubkey::new_unique());
    let tx = build(&mut ctx.svm, &[ix], &payer, &[&payer]);
    assert!(
        ctx.svm.send_transaction(tx).is_err(),
        "any other key must be refused"
    );
}

// ── set_fee_points ───────────────────────────────────────────────────────────

#[test]
fn only_the_authority_may_reprice_the_curve() {
    let mut ctx = setup_initialized();
    let payer = ctx.payer.insecure_clone();
    let authority = ctx.authority.insecure_clone();
    let intruder = Keypair::new();
    ctx.svm.airdrop(&intruder.pubkey(), 1_000_000_000).unwrap();

    let set = |signer: &Keypair, points: &[(u16, u32)]| {
        Instruction::new_with_bytes(
            irm::id(),
            &irm::instruction::SetFeePoints {
                points: args(points),
            }
            .data(),
            irm::accounts::SetFeePoints {
                irm_state: ctx.irm_config,
                pool: ctx.pool,
                authority: signer.pubkey(),
            }
            .to_account_metas(None),
        )
    };

    let new_curve = [(0u16, 100u32), (10_000u16, 900u32)];

    let ix = set(&intruder, &new_curve);
    let tx = build(&mut ctx.svm, &[ix], &payer, &[&payer, &intruder]);
    assert!(
        ctx.svm.send_transaction(tx).is_err(),
        "a non-authority must not be able to reprice outstanding debt"
    );
    assert_eq!(read_state(&ctx).model.points[1].rate_bps, 500, "curve unchanged");

    let ix = set(&authority, &new_curve);
    let tx = build(&mut ctx.svm, &[ix], &payer, &[&payer, &authority]);
    ctx.svm.send_transaction(tx).expect("the authority may reprice");
    assert_eq!(read_state(&ctx).model.points[1].rate_bps, 900);
}

/// The same validation as `initialize` — a curve cannot be smuggled past the
/// bounds by going through the setter instead.
#[test]
fn set_fee_points_applies_the_same_validation_as_initialize() {
    let mut ctx = setup_initialized();
    let payer = ctx.payer.insecure_clone();
    let authority = ctx.authority.insecure_clone();

    let ix = Instruction::new_with_bytes(
        irm::id(),
        &irm::instruction::SetFeePoints {
            points: args(&[(0, 0), (10_000, MAX_RATE_BPS + 1)]),
        }
        .data(),
        irm::accounts::SetFeePoints {
            irm_state: ctx.irm_config,
            pool: ctx.pool,
            authority: authority.pubkey(),
        }
        .to_account_metas(None),
    );
    let tx = build(&mut ctx.svm, &[ix], &payer, &[&payer, &authority]);
    assert!(ctx.svm.send_transaction(tx).is_err());
    assert_eq!(read_state(&ctx).model.points[1].rate_bps, 500, "curve unchanged");
}
