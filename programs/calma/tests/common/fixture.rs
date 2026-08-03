//! A standard priced, funded market, built once.
//!
//! Every `test_*.rs` file used to stand this up itself: the same six PDA
//! derivations, the same `feed::Create` + `SetValue`, the same
//! `irm::Initialize`, the same `calma::Create`. Six copies of ~200 lines, all
//! agreeing on LTV 75, a 1:1 price, a 90 s TTL and a 0–500 bps curve — and
//! nothing keeping them agreed. `packages/test/utils.ts` has had one
//! `setupTest()` for the TypeScript suite all along; this is its counterpart.
//!
//! The knobs exist because several tests need a market that is *deliberately*
//! not standard — a guard-gated one, or one pointing at a rate or price program
//! `calma` has never linked. Anything those tests vary has to be a parameter
//! here, or they go back to hand-rolling.

#![allow(dead_code)]

use super::{create_mint_ixs, create_token_account_ixs, mint_to_ix, send_ixs};
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

/// Associated Token Program — needed to derive the ATAs `borrow` and
/// `deposit_lent` create for the user.
pub const ATP_ID: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";

/// Defaults shared with `packages/test/utils.ts`. Changing one here without
/// changing it there puts the two suites on different markets.
pub const DEFAULT_LTV_PERCENT: u8 = 75;
pub const DEFAULT_PRICE: u64 = 1_000_000;
pub const DEFAULT_PRICE_TTL_MS: u32 = 90_000;
pub const DEFAULT_RATE_POINTS: [(u16, u32); 2] = [(0, 0), (10_000, 500)];

pub struct FixtureBuilder {
    ltv_percent: u8,
    collateral_price: u64,
    lend_price: u64,
    price_ttl_ms: u32,
    rate_points: Vec<(u16, u32)>,
    airdrop: u64,
    with_guard_program: bool,
    with_faucet_program: bool,
    /// Override the program recorded as `Pool::rate_program`. `None` keeps the
    /// reference `irm`. Used by `test_alternative_provider`.
    rate_program: Option<Pubkey>,
    /// Override the account recorded as `Pool::irm_state`.
    rate_state: Option<Pubkey>,
    /// Override the account recorded as `Pool::feed_state`. When set, the
    /// fixture does **not** create a `feed` account — the caller has already put
    /// something at this address. Used by `test_oracle_interface`.
    feed_state: Option<Pubkey>,
    /// Give the market an authority that is not the payer.
    ///
    /// `calma::create` requires the market's own authority to control its rate
    /// curve, so this has to flow into `irm::initialize` as well — which is
    /// exactly the coupling worth having a test hold down.
    separate_authority: bool,
}

impl Default for FixtureBuilder {
    fn default() -> Self {
        Self {
            ltv_percent: DEFAULT_LTV_PERCENT,
            collateral_price: DEFAULT_PRICE,
            lend_price: DEFAULT_PRICE,
            price_ttl_ms: DEFAULT_PRICE_TTL_MS,
            rate_points: DEFAULT_RATE_POINTS.to_vec(),
            airdrop: 200_000_000_000,
            with_guard_program: false,
            with_faucet_program: false,
            rate_program: None,
            rate_state: None,
            feed_state: None,
            separate_authority: false,
        }
    }
}

impl FixtureBuilder {
    pub fn ltv_percent(mut self, v: u8) -> Self {
        self.ltv_percent = v;
        self
    }
    /// Both sides of the oracle price, in `PRICE_SCALE` units.
    pub fn price(mut self, collateral: u64, lend: u64) -> Self {
        self.collateral_price = collateral;
        self.lend_price = lend;
        self
    }
    pub fn price_ttl_ms(mut self, v: u32) -> Self {
        self.price_ttl_ms = v;
        self
    }
    /// `(util_bps, rate_bps)` corners. Must satisfy `irm_state::validate_rate_points`.
    pub fn rate_points(mut self, pts: &[(u16, u32)]) -> Self {
        self.rate_points = pts.to_vec();
        self
    }
    pub fn with_guard_program(mut self) -> Self {
        self.with_guard_program = true;
        self
    }
    pub fn with_faucet_program(mut self) -> Self {
        self.with_faucet_program = true;
        self
    }
    pub fn rate_provider(mut self, program: Pubkey, state: Pubkey) -> Self {
        self.rate_program = Some(program);
        self.rate_state = Some(state);
        self
    }
    pub fn feed_state(mut self, state: Pubkey) -> Self {
        self.feed_state = Some(state);
        self
    }
    /// Use a market authority distinct from the payer.
    pub fn separate_authority(mut self) -> Self {
        self.separate_authority = true;
        self
    }

    /// Build the SVM, mints, feed, IRM and pool, but stop short of `calma::create`.
    ///
    /// For tests that need to assert on how `create` itself behaves, or to run it
    /// with accounts of their own choosing.
    pub fn build_uncreated(self) -> Fixture {
        let calma_id = calma::id();
        let irm_id = irm::id();
        let feed_id = feed::id();
        let atp_id: Pubkey = ATP_ID.parse().unwrap();

        let payer = Keypair::new();
        let col_mint_kp = Keypair::new();
        let lend_mint_kp = Keypair::new();
        let pool_kp = Keypair::new();
        let pool = pool_kp.pubkey();
        let authority_kp = self.separate_authority.then(Keypair::new);
        let authority = authority_kp
            .as_ref()
            .map_or_else(|| payer.pubkey(), |kp| kp.pubkey());

        let mut svm = LiteSVM::new();
        svm.add_program(calma_id, include_bytes!("../../../../target/deploy/calma.so"))
            .unwrap();
        svm.add_program(irm_id, include_bytes!("../../../../target/deploy/irm.so"))
            .unwrap();
        svm.add_program(feed_id, include_bytes!("../../../../target/deploy/feed.so"))
            .unwrap();
        if self.with_guard_program {
            svm.add_program(
                guard::id(),
                include_bytes!("../../../../target/deploy/guard.so"),
            )
            .unwrap();
        }
        if self.with_faucet_program {
            svm.add_program(
                faucet::id(),
                include_bytes!("../../../../target/deploy/faucet.so"),
            )
            .unwrap();
        }
        svm.airdrop(&payer.pubkey(), self.airdrop).unwrap();

        // ── Mints ────────────────────────────────────────────────────────────
        let mint_rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN);
        let [cc, ci] = create_mint_ixs(
            &payer.pubkey(),
            &col_mint_kp.pubkey(),
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
            &[&payer, &col_mint_kp, &lend_mint_kp],
        );
        let col_mint = col_mint_kp.pubkey();
        let lend_mint = lend_mint_kp.pubkey();

        // ── PDAs ─────────────────────────────────────────────────────────────
        // Derived from the same constants the program uses, so a seed rename
        // cannot leave the tests deriving a stale address.
        let (state, _) = Pubkey::find_program_address(&[::state::seeds::STATE], &calma_id);
        let (col_vault, _) = Pubkey::find_program_address(
            &[::state::seeds::COLLATERAL_VAULT, pool.as_ref()],
            &calma_id,
        );
        let (lend_vault, _) =
            Pubkey::find_program_address(&[::state::seeds::LEND_VAULT, pool.as_ref()], &calma_id);
        let (lp_mint, _) =
            Pubkey::find_program_address(&[::state::seeds::LP_MINT, pool.as_ref()], &calma_id);
        let (irm_config, _) =
            Pubkey::find_program_address(&[::interface::IRM_CONFIG_SEED, pool.as_ref()], &irm_id);
        let (user_position, _) = Pubkey::find_program_address(
            &[
                ::state::seeds::USER_POSITION,
                pool.as_ref(),
                payer.pubkey().as_ref(),
            ],
            &calma_id,
        );
        let (default_feed, _) = Pubkey::find_program_address(
            &[
                feed_state::FEED_SEED,
                col_mint.as_ref(),
                lend_mint.as_ref(),
                &[0u8],
            ],
            &feed_id,
        );
        let feed_state_key = self.feed_state.unwrap_or(default_feed);

        let ata = |mint: &Pubkey| {
            Pubkey::find_program_address(
                &[
                    payer.pubkey().as_ref(),
                    spl_token::id().as_ref(),
                    mint.as_ref(),
                ],
                &atp_id,
            )
            .0
        };
        let user_lend_ata = ata(&lend_mint);
        let user_lp_ata = ata(&lp_mint);

        // ── Feed, pool allocation, IRM ───────────────────────────────────────
        let mut setup_ixs = Vec::new();
        if self.feed_state.is_none() {
            setup_ixs.push(Instruction::new_with_bytes(
                feed_id,
                &feed::instruction::Create {
                    id: 0,
                    source: feed::state::PriceSource::Manual,
                    collateral_feed_id: [0u8; 32],
                    lend_feed_id: [0u8; 32],
                    price_ttl_ms: self.price_ttl_ms,
                    rules: feed::state::FeedRules::default(),
                }
                .data(),
                feed::accounts::Create {
                    feed: default_feed,
                    authority: payer.pubkey(),
                    collateral_mint: col_mint,
                    lend_mint,
                    payer: payer.pubkey(),
                    system_program: anchor_lang::solana_program::system_program::id(),
                }
                .to_account_metas(None),
            ));
            setup_ixs.push(Instruction::new_with_bytes(
                feed_id,
                &feed::instruction::SetValue {
                    collateral_price: self.collateral_price,
                    lend_price: self.lend_price,
                }
                .data(),
                feed::accounts::SetValue {
                    feed: default_feed,
                    authority: payer.pubkey(),
                }
                .to_account_metas(None),
            ));
        }

        let pool_space = 8 + std::mem::size_of::<Pool>();
        let pool_rent = svm.minimum_balance_for_rent_exemption(pool_space);
        setup_ixs.push(
            anchor_lang::solana_program::system_instruction::create_account(
                &payer.pubkey(),
                &pool,
                pool_rent,
                pool_space as u64,
                &calma_id,
            ),
        );

        // Only initialize the reference IRM when the market will actually use it.
        if self.rate_program.is_none() {
            setup_ixs.push(Instruction::new_with_bytes(
                irm_id,
                &irm::instruction::Initialize {
                    points: self
                        .rate_points
                        .iter()
                        .map(|&(util_bps, rate_bps)| irm::RatePointArgs { util_bps, rate_bps })
                        .collect(),
                }
                .data(),
                irm::accounts::Initialize {
                    irm_config,
                    pool,
                    // Must match the pool authority — `calma::create` requires the
                    // market's own authority to control its rate curve.
                    authority,
                    payer: payer.pubkey(),
                    system_program: anchor_lang::solana_program::system_program::id(),
                }
                .to_account_metas(None),
            ));
        }
        let mut signers: Vec<&Keypair> = vec![&payer, &pool_kp];
        if let Some(kp) = authority_kp.as_ref() {
            signers.push(kp);
        }
        send_ixs(&mut svm, &setup_ixs, &payer, &signers);

        Fixture {
            svm,
            payer,
            authority_kp,
            pool_kp,
            pool,
            col_mint,
            lend_mint,
            state,
            col_vault,
            lend_vault,
            lp_mint,
            irm_config,
            feed: feed_state_key,
            user_position,
            user_lend_ata,
            user_lp_ata,
            atp_id,
            ltv_percent: self.ltv_percent,
            rate_program: self.rate_program.unwrap_or(irm_id),
            rate_state: self.rate_state.unwrap_or(irm_config),
        }
    }

    /// Build everything and run `calma::create`, leaving a live market.
    pub fn build(self) -> Fixture {
        let mut f = self.build_uncreated();
        f.create_pool(None, None);
        f
    }
}

pub struct Fixture {
    pub svm: LiteSVM,
    pub payer: Keypair,
    /// `Some` only when the market authority is not the payer.
    pub authority_kp: Option<Keypair>,
    pub pool_kp: Keypair,
    pub pool: Pubkey,
    pub col_mint: Pubkey,
    pub lend_mint: Pubkey,
    pub state: Pubkey,
    pub col_vault: Pubkey,
    pub lend_vault: Pubkey,
    pub lp_mint: Pubkey,
    pub irm_config: Pubkey,
    /// Whatever the market prices against — the reference feed unless overridden.
    pub feed: Pubkey,
    pub user_position: Pubkey,
    pub user_lend_ata: Pubkey,
    pub user_lp_ata: Pubkey,
    pub atp_id: Pubkey,
    pub ltv_percent: u8,
    pub rate_program: Pubkey,
    pub rate_state: Pubkey,
}

impl Fixture {
    pub fn builder() -> FixtureBuilder {
        FixtureBuilder::default()
    }

    pub fn authority(&self) -> Pubkey {
        self.authority_kp
            .as_ref()
            .map_or_else(|| self.payer.pubkey(), |kp| kp.pubkey())
    }

    /// Run `calma::create`, optionally behind a whitelist.
    pub fn create_pool(&mut self, guard_program: Option<Pubkey>, guard_state: Option<Pubkey>) {
        let ix = Instruction::new_with_bytes(
            calma::id(),
            &calma::instruction::Create {
                ltv_percent: self.ltv_percent,
            }
            .data(),
            calma::accounts::Create {
                pool: self.pool,
                state: self.state,
                collateral_vault: self.col_vault,
                lend_vault: self.lend_vault,
                lp_mint: self.lp_mint,
                collateral_mint: self.col_mint,
                lend_mint: self.lend_mint,
                authority: self.authority(),
                payer: self.payer.pubkey(),
                feed_state: self.feed,
                rate_program: self.rate_program,
                irm_state: self.rate_state,
                guard_program,
                guard_state,
                token_program: spl_token::id(),
                system_program: anchor_lang::solana_program::system_program::id(),
            }
            .to_account_metas(None),
        );
        let payer = self.payer.insecure_clone();
        let pool_kp = self.pool_kp.insecure_clone();
        let authority = self.authority_kp.as_ref().map(|kp| kp.insecure_clone());
        let mut signers: Vec<&Keypair> = vec![&payer, &pool_kp];
        if let Some(kp) = authority.as_ref() {
            signers.push(kp);
        }
        send_ixs(&mut self.svm, &[ix], &payer, &signers);
    }

    /// A freshly created token account for `mint`, owned by the payer, holding
    /// `amount`. Returns its address.
    pub fn funded_token_account(&mut self, mint: Pubkey, amount: u64) -> Pubkey {
        let kp = Keypair::new();
        let rent = self
            .svm
            .minimum_balance_for_rent_exemption(spl_token::state::Account::LEN);
        let payer = self.payer.insecure_clone();
        let [create, init] =
            create_token_account_ixs(&payer.pubkey(), &kp.pubkey(), &mint, &payer.pubkey(), rent);
        send_ixs(&mut self.svm, &[create, init], &payer, &[&payer, &kp]);
        if amount > 0 {
            let mint_ix = mint_to_ix(&mint, &kp.pubkey(), &payer.pubkey(), amount);
            send_ixs(&mut self.svm, &[mint_ix], &payer, &[&payer]);
        }
        kp.pubkey()
    }

    /// Advance the SVM clock by `secs`, so interest accrues.
    pub fn warp_seconds(&mut self, secs: i64) {
        let mut clock = self.svm.get_sysvar::<solana_clock::Clock>();
        clock.unix_timestamp += secs;
        self.svm.set_sysvar(&clock);
    }
}
