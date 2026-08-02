//! Pluggable dependencies: what `calma` will and won't accept as the oracle and
//! the rate model a market is built on.
//!
//! `calma` no longer CPIs a specific oracle program — it reads an
//! `interface::PriceFeedHeader` out of whatever account the market pinned, and
//! trusts it because the account's *owner* matches the program the market
//! recorded at creation. Two properties follow, and both are load-bearing enough
//! to test directly rather than through the reference feed program:
//!
//!   * any program can be an oracle, so long as its account leads with the
//!     header — proven here with accounts owned by a program that does not exist
//!     on chain at all;
//!   * the owner check is what replaces the old compile-time `feed::ID` pin, so
//!     an account that stops being owned by the recorded program must stop being
//!     accepted.
//!
//! Building the price accounts by hand rather than via `feed::create` is
//! deliberate: it exercises the interface itself instead of one implementation
//! of it, and it can express states the reference program refuses to write
//! (a zero TTL, a foreign owner).

mod common;
use common::{create_mint_ixs, create_token_account_ixs, mint_to_ix, send_ixs};

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::program_pack::Pack;
use calma::state::Pool;
use interface::PriceFeedHeader;
use {
    anchor_lang::{
        solana_program::instruction::Instruction, AnchorSerialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::token::spl_token,
    litesvm::LiteSVM,
    solana_account::Account,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

/// A program id that is deliberately never deployed. Nothing in `calma` requires
/// the oracle to be executable or reachable — only that it owns the account —
/// which is exactly what makes the interface pluggable.
const FOREIGN_ORACLE: Pubkey = Pubkey::new_from_array([7u8; 32]);

/// An 8-byte discriminator that is *not* `feed::Feed`'s. `read_price_feed` skips
/// the discriminator without checking it, because with pluggable oracles there is
/// no single value to check against; this asserts it really is skipped.
const FOREIGN_DISCRIMINATOR: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

/// Send `ixs`, returning the joined program logs on failure.
///
/// The shared `try_send_ixs` only reports success/failure, which would let a
/// negative test pass because the borrow ran out of liquidity rather than
/// because the oracle was refused. Every rejection below names its error.
fn send(
    svm: &mut LiteSVM,
    ixs: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
) -> Result<(), String> {
    // Several tests repeat an identical instruction either side of a state
    // change (re-owning the price account, crossing the TTL). Without a fresh
    // blockhash the second one is a duplicate signature and is dropped before
    // execution — which looks like a rejection but proves nothing.
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = solana_message::Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &bh);
    let tx = solana_transaction::versioned::VersionedTransaction::try_new(
        solana_message::VersionedMessage::Legacy(msg),
        signers,
    )
    .unwrap();
    svm.send_transaction(tx)
        .map(|_| ())
        .map_err(|e| e.meta.logs.join("\n"))
}

/// Assert a call failed *and* that it failed for the stated reason.
#[track_caller]
fn assert_rejected(result: Result<(), String>, expected_error: &str) {
    let logs = result.expect_err("expected the instruction to be rejected");
    assert!(
        logs.contains(expected_error),
        "expected `{expected_error}` but got:\n{logs}"
    );
}

const TTL_MS: u32 = 90_000;
const PARITY: u64 = 1_000_000;

fn header(collateral_mint: Pubkey, lend_mint: Pubkey, now: i64) -> PriceFeedHeader {
    PriceFeedHeader {
        collateral_mint,
        lend_mint,
        collateral_price: PARITY,
        lend_price: PARITY,
        collateral_decimals: 6,
        lend_decimals: 6,
        last_updated_ts: now,
        price_ttl_ms: TTL_MS,
    }
}

/// Serialize a header into account bytes, with `tail` appended to stand in for
/// the implementation-specific fields a real oracle would keep below the header.
fn price_account(header: &PriceFeedHeader, owner: Pubkey, tail: &[u8]) -> Account {
    let mut data = FOREIGN_DISCRIMINATOR.to_vec();
    header.serialize(&mut data).unwrap();
    data.extend_from_slice(tail);
    Account {
        lamports: 1_000_000_000,
        data,
        owner,
        executable: false,
        rent_epoch: 0,
    }
}

struct Ctx {
    svm: LiteSVM,
    payer: Keypair,
    authority: Keypair,
    pool: Keypair,
    collateral_mint: Pubkey,
    lend_mint: Pubkey,
    price_account: Pubkey,
    user_collateral: Pubkey,
    user_lend: Pubkey,
}

impl Ctx {
    fn now(&self) -> i64 {
        self.svm.get_sysvar::<solana_clock::Clock>().unix_timestamp
    }

    /// Move the validator clock forward so a price ages without anything else
    /// changing.
    fn warp_secs(&mut self, secs: i64) {
        let mut clock = self.svm.get_sysvar::<solana_clock::Clock>();
        clock.unix_timestamp += secs;
        self.svm.set_sysvar(&clock);
    }

    fn write_price(&mut self, header: &PriceFeedHeader, owner: Pubkey) {
        self.svm
            .set_account(self.price_account, price_account(header, owner, &[0xAB; 48]))
            .unwrap();
    }

    fn create_pool(&mut self) -> Result<(), String> {
        let authority = self.authority.insecure_clone();
        let irm_state = Pubkey::find_program_address(
            &[b"irm_config", self.pool.pubkey().as_ref()],
            &irm::id(),
        )
        .0;
        self.create_pool_as(&authority, irm_state)
    }

    fn create_pool_as(&mut self, authority: &Keypair, irm_state: Pubkey) -> Result<(), String> {
        let program_id = calma::id();
        let irm_id = irm::id();
        let pool_pubkey = self.pool.pubkey();

        let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &program_id);
        let (collateral_vault, _) =
            Pubkey::find_program_address(&[b"collateral_vault", pool_pubkey.as_ref()], &program_id);
        let (lend_vault, _) =
            Pubkey::find_program_address(&[b"lend_vault", pool_pubkey.as_ref()], &program_id);
        let (lp_mint, _) =
            Pubkey::find_program_address(&[b"lp_mint", pool_pubkey.as_ref()], &program_id);

        let ix = Instruction::new_with_bytes(
            program_id,
            &calma::instruction::Create { ltv_percent: 75 }.data(),
            calma::accounts::Create {
                pool: pool_pubkey,
                state: state_pda,
                collateral_vault,
                lend_vault,
                lp_mint,
                collateral_mint: self.collateral_mint,
                lend_mint: self.lend_mint,
                authority: authority.pubkey(),
                payer: self.payer.pubkey(),
                feed_state: self.price_account,
                rate_program: irm_id,
                irm_state,
                guard_program: None,
                guard_state: None,
                token_program: spl_token::id(),
                system_program: anchor_lang::solana_program::system_program::id(),
            }
            .to_account_metas(None),
        );
        let payer = self.payer.insecure_clone();
        let pool = self.pool.insecure_clone();
        send(&mut self.svm, &[ix], &payer, &[&payer, authority, &pool])
    }

    fn deposit_collateral(&mut self, amount: u64) -> Result<(), String> {
        let program_id = calma::id();
        let pool_pubkey = self.pool.pubkey();
        let (collateral_vault, _) =
            Pubkey::find_program_address(&[b"collateral_vault", pool_pubkey.as_ref()], &program_id);
        let (user_position, _) = Pubkey::find_program_address(
            &[
                b"user_position",
                pool_pubkey.as_ref(),
                self.authority.pubkey().as_ref(),
            ],
            &program_id,
        );

        let ix = Instruction::new_with_bytes(
            program_id,
            &calma::instruction::DepositCollateral { amount }.data(),
            calma::accounts::DepositCollateral {
                pool: pool_pubkey,
                collateral_mint: self.collateral_mint,
                authority: self.authority.pubkey(),
                user_token_account: self.user_collateral,
                collateral_vault,
                user_position,
                guard_program: None,
                guard_state: None,
                token_program: spl_token::id(),
                system_program: anchor_lang::solana_program::system_program::id(),
            }
            .to_account_metas(None),
        );
        let payer = self.payer.insecure_clone();
        let authority = self.authority.insecure_clone();
        send(&mut self.svm, &[ix], &payer, &[&payer, &authority])
    }

    fn deposit_lent(&mut self, amount: u64) -> Result<(), String> {
        let program_id = calma::id();
        let pool_pubkey = self.pool.pubkey();
        let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &program_id);
        let (lend_vault, _) =
            Pubkey::find_program_address(&[b"lend_vault", pool_pubkey.as_ref()], &program_id);
        let (lp_mint, _) =
            Pubkey::find_program_address(&[b"lp_mint", pool_pubkey.as_ref()], &program_id);
        let (irm_config, _) =
            Pubkey::find_program_address(&[b"irm_config", pool_pubkey.as_ref()], &irm::id());
        let user_lp = anchor_spl::associated_token::get_associated_token_address(
            &self.authority.pubkey(),
            &lp_mint,
        );

        let ix = Instruction::new_with_bytes(
            program_id,
            &calma::instruction::DepositLent { amount }.data(),
            calma::accounts::DepositLent {
                pool: pool_pubkey,
                state: state_pda,
                lend_mint: self.lend_mint,
                lp_mint,
                authority: self.authority.pubkey(),
                user_lend_token_account: self.user_lend,
                user_lp_token_account: user_lp,
                lend_vault,
                rate_program: irm::id(),
                irm_state: irm_config,
                guard_program: None,
                guard_state: None,
                token_program: spl_token::id(),
                associated_token_program: anchor_spl::associated_token::ID,
                system_program: anchor_lang::solana_program::system_program::id(),
            }
            .to_account_metas(None),
        );
        let payer = self.payer.insecure_clone();
        let authority = self.authority.insecure_clone();
        send(&mut self.svm, &[ix], &payer, &[&payer, &authority])
    }

    fn borrow(&mut self, amount: u64) -> Result<(), String> {
        let program_id = calma::id();
        let pool_pubkey = self.pool.pubkey();
        let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &program_id);
        let (lend_vault, _) =
            Pubkey::find_program_address(&[b"lend_vault", pool_pubkey.as_ref()], &program_id);
        let (user_position, _) = Pubkey::find_program_address(
            &[
                b"user_position",
                pool_pubkey.as_ref(),
                self.authority.pubkey().as_ref(),
            ],
            &program_id,
        );
        let (irm_config, _) =
            Pubkey::find_program_address(&[b"irm_config", pool_pubkey.as_ref()], &irm::id());
        let user_lend_ata = anchor_spl::associated_token::get_associated_token_address(
            &self.authority.pubkey(),
            &self.lend_mint,
        );

        let ix = Instruction::new_with_bytes(
            program_id,
            &calma::instruction::Borrow { amount }.data(),
            calma::accounts::Borrow {
                pool: pool_pubkey,
                state: state_pda,
                lend_mint: self.lend_mint,
                authority: self.authority.pubkey(),
                user_token_account: user_lend_ata,
                lend_vault,
                user_position,
                rate_program: irm::id(),
                irm_state: irm_config,
                feed_state: self.price_account,
                guard_program: None,
                guard_state: None,
                token_program: spl_token::id(),
                associated_token_program: anchor_spl::associated_token::ID,
                system_program: anchor_lang::solana_program::system_program::id(),
            }
            .to_account_metas(None),
        );
        let payer = self.payer.insecure_clone();
        let authority = self.authority.insecure_clone();
        send(&mut self.svm, &[ix], &payer, &[&payer, &authority])
    }

    fn pool_account(&self) -> Pool {
        let data = self.svm.get_account(&self.pool.pubkey()).unwrap().data;
        *bytemuck::from_bytes::<Pool>(&data[8..8 + std::mem::size_of::<Pool>()])
    }
}

/// Stands up mints, funded user accounts, an IRM curve and a price account owned
/// by [`FOREIGN_ORACLE`]. Stops short of `create` so each test can choose the
/// header the market is created against.
fn setup() -> Ctx {
    let program_id = calma::id();
    let irm_id = irm::id();
    let payer = Keypair::new();
    let authority = Keypair::new();
    let pool = Keypair::new();
    let collateral_mint_kp = Keypair::new();
    let lend_mint_kp = Keypair::new();
    let user_collateral_kp = Keypair::new();
    let user_lend_kp = Keypair::new();

    let mut svm = LiteSVM::new();
    svm.add_program(program_id, include_bytes!("../../../target/deploy/calma.so"))
        .unwrap();
    svm.add_program(irm_id, include_bytes!("../../../target/deploy/irm.so"))
        .unwrap();
    svm.airdrop(&payer.pubkey(), 100_000_000_000).unwrap();
    svm.airdrop(&authority.pubkey(), 100_000_000_000).unwrap();

    // ── Mints and funded user token accounts ─────────────────────────────────
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

    let ta_rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Account::LEN);
    let [ucc, uci] = create_token_account_ixs(
        &payer.pubkey(),
        &user_collateral_kp.pubkey(),
        &collateral_mint_kp.pubkey(),
        &authority.pubkey(),
        ta_rent,
    );
    let [ulc, uli] = create_token_account_ixs(
        &payer.pubkey(),
        &user_lend_kp.pubkey(),
        &lend_mint_kp.pubkey(),
        &authority.pubkey(),
        ta_rent,
    );
    send_ixs(
        &mut svm,
        &[ucc, uci, ulc, uli],
        &payer,
        &[&payer, &user_collateral_kp, &user_lend_kp],
    );
    send_ixs(
        &mut svm,
        &[
            mint_to_ix(
                &collateral_mint_kp.pubkey(),
                &user_collateral_kp.pubkey(),
                &payer.pubkey(),
                1_000_000_000,
            ),
            mint_to_ix(
                &lend_mint_kp.pubkey(),
                &user_lend_kp.pubkey(),
                &payer.pubkey(),
                1_000_000_000,
            ),
        ],
        &payer,
        &[&payer],
    );

    // ── Pool buffer + IRM curve, both prerequisites of `create` ──────────────
    let pool_space = 8 + std::mem::size_of::<Pool>();
    let pool_rent = svm.minimum_balance_for_rent_exemption(pool_space);
    let alloc_pool_ix = anchor_lang::solana_program::system_instruction::create_account(
        &payer.pubkey(),
        &pool.pubkey(),
        pool_rent,
        pool_space as u64,
        &program_id,
    );
    let (irm_config, _) =
        Pubkey::find_program_address(&[b"irm_config", pool.pubkey().as_ref()], &irm_id);
    let irm_init_ix = Instruction::new_with_bytes(
        irm_id,
        &irm::instruction::Initialize {
            points: vec![
                irm::RatePointArgs {
                    util_bps: 0,
                    rate_bps: 0,
                },
                irm::RatePointArgs {
                    util_bps: 10_000,
                    rate_bps: 500,
                },
            ],
        }
        .data(),
        irm::accounts::Initialize {
            irm_config,
            pool: pool.pubkey(),
            authority: authority.pubkey(),
            payer: payer.pubkey(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
    send_ixs(
        &mut svm,
        &[alloc_pool_ix, irm_init_ix],
        &payer,
        &[&payer, &pool, &authority],
    );

    let mut ctx = Ctx {
        svm,
        payer,
        authority,
        pool,
        collateral_mint: collateral_mint_kp.pubkey(),
        lend_mint: lend_mint_kp.pubkey(),
        price_account: Pubkey::new_unique(),
        user_collateral: user_collateral_kp.pubkey(),
        user_lend: user_lend_kp.pubkey(),
    };
    let now = ctx.now();
    let h = header(ctx.collateral_mint, ctx.lend_mint, now);
    ctx.write_price(&h, FOREIGN_ORACLE);
    ctx
}

// ── pluggability ─────────────────────────────────────────────────────────────

/// The headline property: an account owned by a program `calma` has never heard
/// of — and which is not even deployed — prices a market end to end.
#[test]
fn any_program_can_serve_as_the_oracle() {
    let mut ctx = setup();
    ctx.create_pool().expect("create should accept a foreign oracle");

    // The market records the owner it actually observed, so the stored value can
    // never disagree with the account it describes.
    assert_eq!(ctx.pool_account().feed_program, FOREIGN_ORACLE);

    ctx.deposit_lent(500_000_000).unwrap();
    ctx.deposit_collateral(100_000_000).unwrap();
    // At parity and 75% LTV, 50 lend tokens is comfortably inside capacity.
    ctx.borrow(50_000_000).expect("borrow should read the foreign price");
}

/// The header is a *prefix*: implementation-specific bytes below it are ignored,
/// and the discriminator above it is skipped without being checked (there is no
/// single value to check once oracles are pluggable). `setup` already appends a
/// junk tail and uses a non-`feed::Feed` discriminator, so a passing borrow is
/// the assertion — this test pins that intent explicitly.
#[test]
fn trailing_bytes_and_a_foreign_discriminator_are_ignored() {
    let mut ctx = setup();
    let now = ctx.now();
    let h = header(ctx.collateral_mint, ctx.lend_mint, now);
    ctx.svm
        .set_account(
            ctx.price_account,
            price_account(&h, FOREIGN_ORACLE, &[0xFF; 512]),
        )
        .unwrap();

    ctx.create_pool().unwrap();
    ctx.deposit_lent(500_000_000).unwrap();
    ctx.deposit_collateral(100_000_000).unwrap();
    ctx.borrow(10_000_000).unwrap();
}

// ── the owner check that replaces the compile-time program pin ───────────────

/// Ownership is what authenticates the price. If the account stops being owned
/// by the program the market recorded, it must stop being believed — this is the
/// runtime check standing in for the `Account<'info, Feed>` owner check that
/// pinned the protocol to a single oracle program.
#[test]
fn a_reowned_price_account_is_rejected() {
    let mut ctx = setup();
    ctx.create_pool().unwrap();
    ctx.deposit_lent(500_000_000).unwrap();
    ctx.deposit_collateral(100_000_000).unwrap();
    ctx.borrow(10_000_000).expect("sanity: borrow works before re-owning");

    // Same address, same bytes, same prices — only the owner changed.
    let now = ctx.now();
    let h = header(ctx.collateral_mint, ctx.lend_mint, now);
    ctx.write_price(&h, Pubkey::new_from_array([9u8; 32]));

    assert_rejected(ctx.borrow(10_000_000), "ConstraintOwner");
}

/// The mirror case: an attacker cannot point a market at an account they own,
/// because the address itself is pinned at creation.
#[test]
fn a_substituted_price_account_is_rejected() {
    let mut ctx = setup();
    ctx.create_pool().unwrap();
    ctx.deposit_lent(500_000_000).unwrap();
    ctx.deposit_collateral(100_000_000).unwrap();

    // A perfectly well-formed price account, owned by the same program, quoting
    // collateral at 1000x — and irrelevant, because it is not the pinned address.
    let now = ctx.now();
    let mut inflated = header(ctx.collateral_mint, ctx.lend_mint, now);
    inflated.collateral_price = PARITY * 1_000;
    let impostor = Pubkey::new_unique();
    ctx.svm
        .set_account(impostor, price_account(&inflated, FOREIGN_ORACLE, &[]))
        .unwrap();

    ctx.price_account = impostor;
    assert_rejected(ctx.borrow(10_000_000), "InvalidFeedState");
}

// ── the TTL, owned by the oracle ─────────────────────────────────────────────

/// `price_ttl_ms == 0` is the fail-closed sentinel, and it is also what an
/// account written before the field existed decodes as. It must refuse, not
/// silently run with no freshness requirement at all.
#[test]
fn a_zero_ttl_refuses_rather_than_disabling_the_gate() {
    let mut ctx = setup();
    let now = ctx.now();
    let mut h = header(ctx.collateral_mint, ctx.lend_mint, now);
    h.price_ttl_ms = 0;
    ctx.write_price(&h, FOREIGN_ORACLE);

    assert_rejected(ctx.create_pool(), "StaleOracle");
}

/// The boundary is inclusive, and it is the *feed's* budget that sets it — no
/// market-side configuration is involved any more.
#[test]
fn the_ttl_boundary_is_inclusive() {
    let mut ctx = setup();
    ctx.create_pool().unwrap();
    ctx.deposit_lent(500_000_000).unwrap();
    ctx.deposit_collateral(100_000_000).unwrap();

    // TTL is 90_000 ms; age the price to exactly 90 s.
    ctx.warp_secs(90);
    ctx.borrow(1_000_000)
        .expect("a price exactly at its TTL is still fresh");

    // One more second puts it over.
    ctx.warp_secs(1);
    assert_rejected(ctx.borrow(1_000_000), "StaleOracle");
}

/// A market inherits whatever budget its feed declares, so a tighter feed
/// tightens every market pricing against it.
#[test]
fn a_tighter_feed_ttl_binds_sooner() {
    let mut ctx = setup();
    let now = ctx.now();
    let mut h = header(ctx.collateral_mint, ctx.lend_mint, now);
    h.price_ttl_ms = 5_000;
    ctx.write_price(&h, FOREIGN_ORACLE);

    ctx.create_pool().unwrap();
    ctx.deposit_lent(500_000_000).unwrap();
    ctx.deposit_collateral(100_000_000).unwrap();

    ctx.warp_secs(5);
    ctx.borrow(1_000_000).unwrap();
    ctx.warp_secs(1);
    // 6s is stale under a 5s feed TTL, where the retired 90s pool budget allowed it.
    assert_rejected(ctx.borrow(1_000_000), "StaleOracle");
}

// ── degenerate prices ────────────────────────────────────────────────────────

/// A zero lend price yields no ratio at all. Accepting it would report a price
/// of 0 — indistinguishable from "the collateral is worthless" — and silently
/// collapse every position's borrow capacity instead of erroring.
#[test]
fn a_zero_lend_price_is_refused_rather_than_read_as_worthless() {
    let mut ctx = setup();
    let now = ctx.now();
    let mut h = header(ctx.collateral_mint, ctx.lend_mint, now);
    h.lend_price = 0;
    ctx.write_price(&h, FOREIGN_ORACLE);

    assert_rejected(ctx.create_pool(), "ZeroPrice");
}

/// The feed must price *this* market's pair, or every LTV check would run on a
/// price that has nothing to do with the collateral posted.
#[test]
fn a_feed_for_another_pair_is_refused() {
    let mut ctx = setup();
    let now = ctx.now();
    let mut h = header(ctx.collateral_mint, ctx.lend_mint, now);
    h.lend_mint = Pubkey::new_unique();
    ctx.write_price(&h, FOREIGN_ORACLE);

    assert_rejected(ctx.create_pool(), "FeedMintMismatch");
}

/// An account too short to hold a header is a parse failure, not a panic or a
/// read of adjacent memory.
#[test]
fn a_truncated_price_account_is_refused() {
    let mut ctx = setup();
    ctx.svm
        .set_account(
            ctx.price_account,
            Account {
                lamports: 1_000_000_000,
                data: vec![0u8; 8 + 16],
                owner: FOREIGN_ORACLE,
                executable: false,
                rent_epoch: 0,
            },
        )
        .unwrap();

    assert_rejected(ctx.create_pool(), "AccountDidNotDeserialize");
}

// ── the rate model, also pluggable ───────────────────────────────────────────
//
// The IRM stays a CPI — the borrow rate depends on live utilization, so it is a
// computation, not stored state — but `create` no longer pins `rate_program` to
// one program id either. What it still guarantees is that the market's own
// authority controls the curve, and it now asks the rate program to vouch for
// that over CPI rather than reading a layout it can no longer assume.

/// `irm::initialize` is permissionless and first-come-first-served on the
/// `["irm_config", pool]` PDA, so whoever claims it names themselves the
/// authority for the life of the market. Without this check a market could be
/// created already bound to a curve someone else controls, with nothing on
/// `Pool` revealing it.
#[test]
fn a_market_cannot_be_built_on_a_curve_it_does_not_control() {
    let mut ctx = setup();
    // The IRM curve was claimed by `ctx.authority` in `setup`; this stranger
    // tries to create the market on top of it.
    let stranger = Keypair::new();
    ctx.svm.airdrop(&stranger.pubkey(), 10_000_000_000).unwrap();
    let irm_state =
        Pubkey::find_program_address(&[b"irm_config", ctx.pool.pubkey().as_ref()], &irm::id()).0;

    assert_rejected(ctx.create_pool_as(&stranger, irm_state), "Unauthorized");
}

/// The curve must be *this* market's canonical PDA under the chosen rate
/// program, so a market cannot be pointed at another market's rate model.
#[test]
fn a_curve_belonging_to_another_market_is_refused() {
    let mut ctx = setup();
    let other_pool = Pubkey::new_unique();
    let (foreign_irm, _) =
        Pubkey::find_program_address(&[b"irm_config", other_pool.as_ref()], &irm::id());
    let authority = ctx.authority.insecure_clone();

    assert_rejected(
        ctx.create_pool_as(&authority, foreign_irm),
        "InvalidIrmState",
    );
}
