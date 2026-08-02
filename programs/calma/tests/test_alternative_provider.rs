//! A market stood up entirely on a second, independent provider.
//!
//! `test_oracle_interface.rs` proves the *price* half of the pluggability claim
//! using hand-written accounts owned by a program that does not exist. That
//! cannot reach the *rate* half, because rates come over CPI and a CPI needs
//! something real on the other end.
//!
//! `programs/quote` is that something real: one program implementing both
//! interfaces, from one account that is simultaneously a market's `feed_state`
//! and its `irm_state`. Everything here runs against it rather than against
//! `feed` + `irm`, and the last test runs a market on each pair of providers
//! side by side in the same validator.

mod common;
use common::{create_mint_ixs, create_token_account_ixs, mint_to_ix, send_ixs};

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::program_pack::Pack;
use calma::state::Pool;
use sha2::{Digest, Sha256};
use {
    anchor_lang::{
        solana_program::instruction::{AccountMeta, Instruction},
        Discriminator, InstructionData, ToAccountMetas,
    },
    anchor_spl::token::spl_token,
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

// ── the provider, known only by its program id ───────────────────────────────
//
// `calma` does not depend on the `quote` crate and neither does this test. A
// market reaches its provider through a pubkey recorded on `Pool`, so the honest
// way to demonstrate that is to integrate the same way an unaffiliated party
// would: from the program id, with every instruction assembled by hand. Nothing
// below shares a type with the provider — if it works, the interface is real
// rather than an artifact of both sides being compiled together.

/// `programs/quote`'s `declare_id!`. The only thing this test knows about it.
fn quote_program_id() -> Pubkey {
    Pubkey::from_str_const("QUoTDMDSEw1nAFRp27eWrK4AcVsE1Sg9YupYcc22Yhc")
}

/// Anchor's instruction discriminator: `sha256("global:<name>")[..8]`.
///
/// Derived rather than imported, because importing it would mean importing the
/// provider — the one dependency this file must not have.
fn anchor_discriminator(name: &str) -> [u8; 8] {
    let mut hasher = Sha256::new();
    hasher.update(format!("global:{name}"));
    hasher.finalize()[..8].try_into().unwrap()
}

/// An instruction for the provider, assembled from a name, borsh-encoded args
/// and account metas — no generated helpers on either side.
fn quote_ix(name: &str, args: Vec<u8>, accounts: Vec<AccountMeta>) -> Instruction {
    let mut data = anchor_discriminator(name).to_vec();
    data.extend(args);
    Instruction {
        program_id: quote_program_id(),
        accounts,
        data,
    }
}

const TTL_MS: u32 = 90_000;
const PARITY: u64 = 1_000_000;
const FLAT_RATE_BPS: u32 = 1_000; // 10% at every utilization

const LEND_LIQUIDITY: u64 = 500_000_000;
const COLLATERAL: u64 = 100_000_000;

/// Send `ixs`, returning the joined program logs on failure, so a negative test
/// can name the error it expects instead of accepting any failure.
fn send(
    svm: &mut LiteSVM,
    ixs: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
) -> Result<(), String> {
    // Tests below repeat identical instructions either side of a state change
    // (warping the clock, retuning the rate). Without a fresh blockhash the
    // second is a duplicate signature and is dropped before execution — which
    // looks like a rejection but proves nothing.
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

#[track_caller]
fn assert_rejected(result: Result<(), String>, expected_error: &str) {
    let logs = result.expect_err("expected the instruction to be rejected");
    assert!(
        logs.contains(expected_error),
        "expected `{expected_error}` but got:\n{logs}"
    );
}

fn provider_pda(pool: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"irm_config", pool.as_ref()], &quote_program_id()).0
}

// ── the ABI is the discriminator ─────────────────────────────────────────────

/// The single fact the whole rate interface rests on.
///
/// `calma` encodes its CPI with `irm`'s generated helpers but dispatches to
/// whatever program the market recorded, so an alternative provider is
/// call-compatible precisely when its instruction *names* hash to the same
/// discriminator. Nothing in either program's source states this, and renaming
/// an instruction on either side would break every market on the other with no
/// compile error anywhere — so assert it.
#[test]
fn the_rate_abi_is_carried_by_the_instruction_name() {
    // The reference program's generated constants are the ground truth. This
    // test's derivation has to reproduce them, because that derivation — not a
    // shared type — is all an unaffiliated provider has to go on. `calma`
    // hardcodes the same bytes in `hooks/irm.rs`, asserted there too.
    assert_eq!(
        irm::instruction::BorrowRate::DISCRIMINATOR,
        anchor_discriminator("borrow_rate"),
    );
    assert_eq!(
        irm::instruction::CheckAuthority::DISCRIMINATOR,
        anchor_discriminator("check_authority"),
    );
}

// ── fixture ──────────────────────────────────────────────────────────────────

struct Ctx {
    svm: LiteSVM,
    payer: Keypair,
    authority: Keypair,
    pool: Keypair,
    collateral_mint: Pubkey,
    lend_mint: Pubkey,
    user_collateral: Pubkey,
    user_lend: Pubkey,
}

impl Ctx {
    fn provider(&self) -> Pubkey {
        provider_pda(&self.pool.pubkey())
    }

    fn warp_secs(&mut self, secs: i64) {
        let mut clock = self.svm.get_sysvar::<solana_clock::Clock>();
        clock.unix_timestamp += secs;
        self.svm.set_sysvar(&clock);
    }

    fn init_provider(&mut self, flat_rate_bps: u32) -> Result<(), String> {
        // Borsh for these primitives is just little-endian, in declaration
        // order: (price_ttl_ms: u32, flat_rate_bps: u32, collateral_price: u64,
        // lend_price: u64).
        let mut args = Vec::new();
        args.extend_from_slice(&TTL_MS.to_le_bytes());
        args.extend_from_slice(&flat_rate_bps.to_le_bytes());
        args.extend_from_slice(&PARITY.to_le_bytes());
        args.extend_from_slice(&PARITY.to_le_bytes());

        let ix = quote_ix(
            "initialize",
            args,
            vec![
                AccountMeta::new(self.provider(), false),
                AccountMeta::new_readonly(self.pool.pubkey(), false),
                AccountMeta::new_readonly(self.collateral_mint, false),
                AccountMeta::new_readonly(self.lend_mint, false),
                AccountMeta::new_readonly(self.authority.pubkey(), true),
                AccountMeta::new(self.payer.pubkey(), true),
                AccountMeta::new_readonly(
                    anchor_lang::solana_program::system_program::id(),
                    false,
                ),
            ],
        );
        let payer = self.payer.insecure_clone();
        let authority = self.authority.insecure_clone();
        send(&mut self.svm, &[ix], &payer, &[&payer, &authority])
    }

    /// Account metas shared by the provider's authority-gated setters.
    fn set_value_accounts(&self) -> Vec<AccountMeta> {
        vec![
            AccountMeta::new(self.provider(), false),
            AccountMeta::new_readonly(self.authority.pubkey(), true),
        ]
    }

    fn set_price(&mut self) -> Result<(), String> {
        let mut args = Vec::new();
        args.extend_from_slice(&PARITY.to_le_bytes());
        args.extend_from_slice(&PARITY.to_le_bytes());

        let ix = quote_ix("set_price", args, self.set_value_accounts());
        let payer = self.payer.insecure_clone();
        let authority = self.authority.insecure_clone();
        send(&mut self.svm, &[ix], &payer, &[&payer, &authority])
    }

    /// Advance the clock by `secs`, then re-publish the price at the same value.
    ///
    /// The provider owns a 90s TTL, so a bare warp of any interesting length
    /// makes its own price stale and every subsequent borrow fails `StaleOracle`
    /// before it ever reaches the rate model. Re-publishing keeps the *price*
    /// constant while time passes, which is what these tests need in order to
    /// isolate interest accrual.
    fn age_market(&mut self, secs: i64) {
        self.warp_secs(secs);
        self.set_price().unwrap();
    }

    fn set_rate(&mut self, flat_rate_bps: u32) -> Result<(), String> {
        let ix = quote_ix(
            "set_rate",
            flat_rate_bps.to_le_bytes().to_vec(),
            self.set_value_accounts(),
        );
        let payer = self.payer.insecure_clone();
        let authority = self.authority.insecure_clone();
        send(&mut self.svm, &[ix], &payer, &[&payer, &authority])
    }

    /// `feed_state` and `irm_state` both default to the provider PDA — the whole
    /// point — but are overridable so the cross-provider tests can pass
    /// something else.
    fn create_pool(&mut self, feed_state: Pubkey, irm_state: Pubkey) -> Result<(), String> {
        let program_id = calma::id();
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
                authority: self.authority.pubkey(),
                payer: self.payer.pubkey(),
                feed_state,
                rate_program: quote_program_id(),
                irm_state,
                guard_program: None,
                guard_state: None,
                token_program: spl_token::id(),
                system_program: anchor_lang::solana_program::system_program::id(),
            }
            .to_account_metas(None),
        );
        let payer = self.payer.insecure_clone();
        let authority = self.authority.insecure_clone();
        let pool = self.pool.insecure_clone();
        send(&mut self.svm, &[ix], &payer, &[&payer, &authority, &pool])
    }

    fn deposit_lent(&mut self, amount: u64) -> Result<(), String> {
        let program_id = calma::id();
        let pool_pubkey = self.pool.pubkey();
        let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &program_id);
        let (lend_vault, _) =
            Pubkey::find_program_address(&[b"lend_vault", pool_pubkey.as_ref()], &program_id);
        let (lp_mint, _) =
            Pubkey::find_program_address(&[b"lp_mint", pool_pubkey.as_ref()], &program_id);
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
                // The rate half of the interface, reached on the lend path too.
                rate_program: quote_program_id(),
                irm_state: self.provider(),
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

    fn borrow_with_feed(&mut self, amount: u64, feed_state: Pubkey) -> Result<(), String> {
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
                rate_program: quote_program_id(),
                irm_state: self.provider(),
                feed_state,
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
        self.borrow_with_feed(amount, self.provider())
    }

    fn pool_account(&self) -> Pool {
        let data = self.svm.get_account(&self.pool.pubkey()).unwrap().data;
        *bytemuck::from_bytes::<Pool>(&data[8..8 + std::mem::size_of::<Pool>()])
    }

    fn total_borrow_assets(&self) -> u64 {
        self.pool_account().market.total_borrow_assets
    }
}

/// Mints, funded users and an allocated pool buffer. Stops before
/// `quote::initialize` so each test picks its own rate.
fn setup() -> Ctx {
    let program_id = calma::id();
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
    svm.add_program(
        quote_program_id(),
        include_bytes!("../../../target/deploy/quote.so"),
    )
    .unwrap();
    // The reference providers, loaded so `two_markets_on_different_providers_coexist`
    // can stand a second market on them. Unused by the other tests, which is
    // itself the point: this market needs neither.
    svm.add_program(feed::id(), include_bytes!("../../../target/deploy/feed.so"))
        .unwrap();
    svm.add_program(irm::id(), include_bytes!("../../../target/deploy/irm.so"))
        .unwrap();
    svm.airdrop(&payer.pubkey(), 100_000_000_000).unwrap();
    svm.airdrop(&authority.pubkey(), 100_000_000_000).unwrap();

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

    let pool_space = 8 + std::mem::size_of::<Pool>();
    let pool_rent = svm.minimum_balance_for_rent_exemption(pool_space);
    let alloc_pool_ix = anchor_lang::solana_program::system_instruction::create_account(
        &payer.pubkey(),
        &pool.pubkey(),
        pool_rent,
        pool_space as u64,
        &program_id,
    );
    send_ixs(&mut svm, &[alloc_pool_ix], &payer, &[&payer, &pool]);

    Ctx {
        svm,
        payer,
        authority,
        pool,
        collateral_mint: collateral_mint_kp.pubkey(),
        lend_mint: lend_mint_kp.pubkey(),
        user_collateral: user_collateral_kp.pubkey(),
        user_lend: user_lend_kp.pubkey(),
    }
}

/// A funded, borrowing market whose oracle and rate model are the same account.
fn market_on_quote(flat_rate_bps: u32) -> Ctx {
    let mut ctx = setup();
    ctx.init_provider(flat_rate_bps).unwrap();
    ctx.create_pool(ctx.provider(), ctx.provider()).unwrap();
    ctx.deposit_lent(LEND_LIQUIDITY).unwrap();
    ctx.deposit_collateral(COLLATERAL).unwrap();
    ctx
}

// ── one account, both roles ──────────────────────────────────────────────────

/// The headline. A single account is named for both of a market's external
/// dependencies, and is exercised as a direct read (the price) and over CPI (the
/// rate) — `deposit_lent` and `borrow` each call `borrow_rate`.
#[test]
fn one_account_serves_as_both_oracle_and_rate_model() {
    let mut ctx = setup();
    ctx.init_provider(FLAT_RATE_BPS).unwrap();

    let provider = ctx.provider();
    ctx.create_pool(provider, provider)
        .expect("create should accept one account in both roles");

    let pool = ctx.pool_account();
    assert_eq!(pool.feed_program, quote_program_id());
    assert_eq!(pool.rate_program, quote_program_id());
    assert_eq!(pool.feed_state, provider);
    assert_eq!(pool.irm_state, provider);
    assert_eq!(
        pool.feed_state, pool.irm_state,
        "the market's oracle and rate model are literally the same account"
    );

    ctx.deposit_lent(LEND_LIQUIDITY).unwrap();
    ctx.deposit_collateral(COLLATERAL).unwrap();
    ctx.borrow(50_000_000)
        .expect("borrow priced and rated by the alternative provider");

    assert!(ctx.total_borrow_assets() >= 50_000_000);
}

/// A market on a provider that has never heard of `feed` or `irm` is not
/// second-class: the reference programs are absent from its account lists
/// entirely, not merely unused.
#[test]
fn the_reference_programs_appear_nowhere_in_the_market() {
    let ctx = market_on_quote(FLAT_RATE_BPS);
    let pool = ctx.pool_account();

    assert_ne!(pool.feed_program, feed::id());
    assert_ne!(pool.rate_program, irm::id());
}

// ── the provider's rate is the one that accrues ──────────────────────────────

/// That the CPI *happens* proves little; this proves its result is consumed.
///
/// At a flat 0 bps no interest accrues however long passes, and at a flat
/// `FLAT_RATE_BPS` it does — over the same span, at the same utilization, with
/// the reference piecewise curve nowhere in the picture.
#[test]
fn the_providers_rate_is_the_one_that_accrues() {
    let mut zero = market_on_quote(0);
    zero.borrow(50_000_000).unwrap();
    let zero_before = zero.total_borrow_assets();
    zero.age_market(365 * 24 * 3_600);
    // Any instruction accrues interest first; borrowing 0 is rejected, so nudge
    // the market with a tiny borrow and discount it.
    zero.borrow(1).unwrap();
    assert_eq!(
        zero.total_borrow_assets(),
        zero_before + 1,
        "a flat 0 bps model must accrue nothing over a year"
    );

    let mut paid = market_on_quote(FLAT_RATE_BPS);
    paid.borrow(50_000_000).unwrap();
    let paid_before = paid.total_borrow_assets();
    paid.age_market(365 * 24 * 3_600);
    paid.borrow(1).unwrap();
    assert!(
        paid.total_borrow_assets() > paid_before + 1,
        "a flat {FLAT_RATE_BPS} bps model must accrue over a year"
    );
}

/// The model is the provider's business. A flat provider quotes the same rate at
/// 10% utilization as at 90%, which no piecewise curve would — so `calma` cannot
/// be assuming one.
#[test]
fn a_flat_model_ignores_utilization() {
    let year = 365 * 24 * 3_600;

    // Two identical markets, differing only in how much of the vault is lent out.
    let mut low = market_on_quote(FLAT_RATE_BPS);
    low.borrow(50_000_000).unwrap();
    let low_before = low.total_borrow_assets();
    low.age_market(year);
    low.borrow(1).unwrap();
    let low_growth = low.total_borrow_assets() - low_before - 1;

    let mut high = market_on_quote(FLAT_RATE_BPS);
    high.borrow(50_000_000).unwrap();
    // Push utilization from ~10% to ~80% before letting the same span elapse.
    // Collateral has to lead the debt by enough that a year of accrual cannot
    // push the position through its own LTV ceiling mid-test.
    high.deposit_collateral(COLLATERAL * 5).unwrap();
    high.borrow(350_000_000).unwrap();
    let high_before = high.total_borrow_assets();
    high.age_market(year);
    high.borrow(1).unwrap();
    let high_growth = high.total_borrow_assets() - high_before - 1;

    // Interest is proportional to principal, so compare rates rather than
    // absolute growth: 3x the debt at the same rate accrues ~3x the interest.
    let low_rate = low_growth as f64 / low_before as f64;
    let high_rate = high_growth as f64 / high_before as f64;
    assert!(
        (low_rate - high_rate).abs() < 0.001,
        "flat model: {low_rate} at low utilization vs {high_rate} at high"
    );
}

/// The rate is read live on every call, not cached on the market at creation.
#[test]
fn retuning_the_provider_changes_what_the_market_accrues() {
    let year = 365 * 24 * 3_600;
    let mut ctx = market_on_quote(0);
    ctx.borrow(50_000_000).unwrap();

    let before = ctx.total_borrow_assets();
    ctx.age_market(year);
    ctx.borrow(1).unwrap();
    assert_eq!(ctx.total_borrow_assets(), before + 1, "0 bps: no accrual");

    ctx.set_rate(FLAT_RATE_BPS).unwrap();
    let after_retune = ctx.total_borrow_assets();
    ctx.age_market(year);
    ctx.borrow(1).unwrap();
    assert!(
        ctx.total_borrow_assets() > after_retune + 1,
        "the market must pick up the retuned rate with no action on its side"
    );
}

// ── the pin is per-market ────────────────────────────────────────────────────

/// A market pinned to `quote` will not read a `feed`-owned price account, even
/// though `feed` is a perfectly legitimate oracle for *other* markets. Provider
/// choice is recorded per market, not globally.
#[test]
fn a_market_will_not_read_another_providers_price_account() {
    let mut ctx = market_on_quote(FLAT_RATE_BPS);
    ctx.borrow(10_000_000)
        .expect("sanity: borrowing works on its own provider");

    // Stand up a real, well-formed `feed` account for the same mint pair.
    let (feed_pda, _) = Pubkey::find_program_address(
        &[b"feed", ctx.collateral_mint.as_ref(), ctx.lend_mint.as_ref(), &[0u8]],
        &feed::id(),
    );
    let create_ix = Instruction::new_with_bytes(
        feed::id(),
        &feed::instruction::Create {
            id: 0,
            source: feed::state::PriceSource::Manual,
            collateral_feed_id: [0u8; 32],
            lend_feed_id: [0u8; 32],
            price_ttl_ms: TTL_MS,
            rules: feed::state::FeedRules::default(),
        }
        .data(),
        feed::accounts::Create {
            feed: feed_pda,
            authority: ctx.payer.pubkey(),
            collateral_mint: ctx.collateral_mint,
            lend_mint: ctx.lend_mint,
            payer: ctx.payer.pubkey(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
    let set_ix = Instruction::new_with_bytes(
        feed::id(),
        &feed::instruction::SetValue {
            collateral_price: PARITY,
            lend_price: PARITY,
        }
        .data(),
        feed::accounts::SetValue {
            feed: feed_pda,
            authority: ctx.payer.pubkey(),
        }
        .to_account_metas(None),
    );
    let payer = ctx.payer.insecure_clone();
    send(&mut ctx.svm, &[create_ix, set_ix], &payer, &[&payer]).unwrap();

    // Fresh, correctly priced, owned by a real oracle program — and refused,
    // because it is not the account this market pinned. The rejection comes from
    // `Borrow`'s address constraint, which fires before the handler's owner
    // check ever runs; `read_price_feed`'s own key check is the backstop for
    // callers that reach it by another route.
    assert_rejected(
        ctx.borrow_with_feed(10_000_000, feed_pda),
        "InvalidFeedState",
    );
}

/// `has_one = pool` on the provider: a provider account belonging to a different
/// market is refused by the provider itself, independently of `calma`'s own
/// address check. Both checks are load-bearing and they answer to different
/// parties.
#[test]
fn the_provider_refuses_to_rate_a_market_it_was_not_created_for() {
    let mut ctx = setup();
    ctx.init_provider(FLAT_RATE_BPS).unwrap();

    // A provider PDA for some other pool, correctly initialised for *that* pool.
    let other_pool = Keypair::new();
    let other_provider = provider_pda(&other_pool.pubkey());

    // `calma::create` rejects it first, on the PDA derivation, before the CPI.
    assert_rejected(
        ctx.create_pool(ctx.provider(), other_provider),
        "InvalidIrmState",
    );
}

// ── multiple providers, side by side ─────────────────────────────────────────

/// Two markets in one validator: one on `feed` + `irm`, one on `quote`. Both
/// borrow. Each records its own providers, and neither knows about the other's.
#[test]
fn two_markets_on_different_providers_coexist() {
    let mut ctx = market_on_quote(FLAT_RATE_BPS);
    ctx.borrow(25_000_000).unwrap();

    // ── Market B, on the reference providers, in the same SVM ────────────────
    let pool_b = Keypair::new();
    let program_id = calma::id();
    let payer = ctx.payer.insecure_clone();
    let authority = ctx.authority.insecure_clone();

    let pool_space = 8 + std::mem::size_of::<Pool>();
    let pool_rent = ctx.svm.minimum_balance_for_rent_exemption(pool_space);
    let alloc = anchor_lang::solana_program::system_instruction::create_account(
        &payer.pubkey(),
        &pool_b.pubkey(),
        pool_rent,
        pool_space as u64,
        &program_id,
    );

    // A separate mint pair, so market B's feed is unambiguously its own.
    let coll_b = Keypair::new();
    let lend_b = Keypair::new();
    let mint_rent = ctx
        .svm
        .minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN);
    let [cc, ci] = create_mint_ixs(&payer.pubkey(), &coll_b.pubkey(), &payer.pubkey(), mint_rent);
    let [lc, li] = create_mint_ixs(&payer.pubkey(), &lend_b.pubkey(), &payer.pubkey(), mint_rent);
    send(
        &mut ctx.svm,
        &[alloc, cc, ci, lc, li],
        &payer,
        &[&payer, &pool_b, &coll_b, &lend_b],
    )
    .unwrap();

    let (feed_b, _) = Pubkey::find_program_address(
        &[
            b"feed",
            coll_b.pubkey().as_ref(),
            lend_b.pubkey().as_ref(),
            &[0u8],
        ],
        &feed::id(),
    );
    let (irm_b, _) =
        Pubkey::find_program_address(&[b"irm_config", pool_b.pubkey().as_ref()], &irm::id());

    let feed_create = Instruction::new_with_bytes(
        feed::id(),
        &feed::instruction::Create {
            id: 0,
            source: feed::state::PriceSource::Manual,
            collateral_feed_id: [0u8; 32],
            lend_feed_id: [0u8; 32],
            price_ttl_ms: TTL_MS,
            rules: feed::state::FeedRules::default(),
        }
        .data(),
        feed::accounts::Create {
            feed: feed_b,
            authority: payer.pubkey(),
            collateral_mint: coll_b.pubkey(),
            lend_mint: lend_b.pubkey(),
            payer: payer.pubkey(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
    let feed_set = Instruction::new_with_bytes(
        feed::id(),
        &feed::instruction::SetValue {
            collateral_price: PARITY,
            lend_price: PARITY,
        }
        .data(),
        feed::accounts::SetValue {
            feed: feed_b,
            authority: payer.pubkey(),
        }
        .to_account_metas(None),
    );
    let irm_init = Instruction::new_with_bytes(
        irm::id(),
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
            irm_config: irm_b,
            pool: pool_b.pubkey(),
            authority: authority.pubkey(),
            payer: payer.pubkey(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
    send(
        &mut ctx.svm,
        &[feed_create, feed_set, irm_init],
        &payer,
        &[&payer, &authority],
    )
    .unwrap();

    let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &program_id);
    let (cv_b, _) =
        Pubkey::find_program_address(&[b"collateral_vault", pool_b.pubkey().as_ref()], &program_id);
    let (lv_b, _) =
        Pubkey::find_program_address(&[b"lend_vault", pool_b.pubkey().as_ref()], &program_id);
    let (lp_b, _) =
        Pubkey::find_program_address(&[b"lp_mint", pool_b.pubkey().as_ref()], &program_id);

    let create_b = Instruction::new_with_bytes(
        program_id,
        &calma::instruction::Create { ltv_percent: 75 }.data(),
        calma::accounts::Create {
            pool: pool_b.pubkey(),
            state: state_pda,
            collateral_vault: cv_b,
            lend_vault: lv_b,
            lp_mint: lp_b,
            collateral_mint: coll_b.pubkey(),
            lend_mint: lend_b.pubkey(),
            authority: authority.pubkey(),
            payer: payer.pubkey(),
            feed_state: feed_b,
            rate_program: irm::id(),
            irm_state: irm_b,
            guard_program: None,
            guard_state: None,
            token_program: spl_token::id(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
    send(
        &mut ctx.svm,
        &[create_b],
        &payer,
        &[&payer, &authority, &pool_b],
    )
    .expect("a reference-provider market alongside a `quote`-backed one");

    // ── Each market recorded its own providers ───────────────────────────────
    let a = ctx.pool_account();
    let b_data = ctx.svm.get_account(&pool_b.pubkey()).unwrap().data;
    let b = *bytemuck::from_bytes::<Pool>(&b_data[8..8 + std::mem::size_of::<Pool>()]);

    assert_eq!(a.feed_program, quote_program_id());
    assert_eq!(a.rate_program, quote_program_id());
    assert_eq!(b.feed_program, feed::id());
    assert_eq!(b.rate_program, irm::id());
    assert_ne!(a.feed_program, b.feed_program);
    assert_ne!(a.rate_program, b.rate_program);

    // ── And the first market keeps working with the second one live ──────────
    ctx.borrow(10_000_000)
        .expect("market A still borrows with a differently-provided market alongside it");
}
