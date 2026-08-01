use crate::{hooks::oracle::OracleState, state::Pool};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[constant]
pub const POOL_SPACE: u64 = (8 + std::mem::size_of::<Pool>()) as u64;

#[derive(Accounts)]
pub struct Create<'info> {
    /// The pool data account. Must be allocated (size = POOL_SPACE) and owned by
    /// this program before `create` runs.
    ///
    /// Allocation has to be its own **top-level instruction**: `init` here would
    /// create the account by CPI, and CPI allocation is capped at 10 KB, well
    /// under `POOL_SPACE`. A top-level `system_instruction::create_account` has
    /// no such limit, and it can sit in the same transaction as `create` — see
    /// the `preInstructions` in `packages/test/utils.ts`.
    ///
    /// **Must sign.** `zero` alone only asserts the account is an unused,
    /// program-owned buffer — it binds it to nobody. A caller who splits
    /// allocation and `create` across transactions would otherwise let anyone
    /// watching land `create` on the allocated buffer first, with themselves as
    /// `authority` and a feed and LTV of their choosing. Requiring the pool
    /// keypair's signature ties creation to whoever allocated it either way.
    #[account(zero, signer)]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: Signer-only PDA — no data stored; used as authority for vault token accounts and LP mint.
    #[account(
        seeds = [b"state"],
        bump,
    )]
    pub state: UncheckedAccount<'info>,

    /// Token vault for holding deposited collateral tokens.
    #[account(
        init,
        payer = payer,
        token::mint = collateral_mint,
        token::authority = state,
        seeds = [b"collateral_vault", pool.key().as_ref()],
        bump
    )]
    pub collateral_vault: Account<'info, TokenAccount>,

    /// Token vault for holding deposited lend tokens.
    #[account(
        init,
        payer = payer,
        token::mint = lend_mint,
        token::authority = state,
        seeds = [b"lend_vault", pool.key().as_ref()],
        bump
    )]
    pub lend_vault: Account<'info, TokenAccount>,

    /// The LP token mint for the lend side of this pool.
    #[account(
        init,
        payer = payer,
        mint::decimals = lend_mint.decimals,
        mint::authority = state,
        seeds = [b"lp_mint", pool.key().as_ref()],
        bump
    )]
    pub lp_mint: Account<'info, Mint>,

    /// The collateral token mint.
    pub collateral_mint: Account<'info, Mint>,

    /// The lend token mint (deposited by lenders; borrowed by borrowers).
    pub lend_mint: Account<'info, Mint>,

    /// The authority that will control this pool.
    pub authority: Signer<'info>,

    /// The account that pays for account creation.
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: Feed program — key stored in the pool.
    pub feed_program: UncheckedAccount<'info>,

    /// The feed this pool prices against. Typed so Anchor enforces owner and
    /// discriminator, and so the handler can check the feed actually covers this
    /// pool's token pair.
    pub feed_state: Account<'info, feed::Feed>,

    /// CHECK: IRM program — invoked via CPI to fetch the initial borrow rate.
    pub rate_program: UncheckedAccount<'info>,

    /// The rate curve this market prices against. Typed so Anchor enforces owner
    /// and discriminator, and so the handler can check who controls it — the
    /// address alone says nothing about that.
    pub irm_state: AccountLoader<'info, irm::IrmState>,

    /// CHECK: optional guard program — when supplied alongside `guard_state`,
    /// CPI'd to verify the creator is whitelisted. Markets are permissionless to
    /// create; gating is opt-in and chosen here, once.
    pub guard_program: Option<UncheckedAccount<'info>>,

    /// CHECK: optional guard state — the whitelist this market binds itself to
    /// for its entire life. Recorded in `Pool::guard_state` and enforced on
    /// every entry point thereafter. The `guard` program re-derives it from its
    /// own recorded authority, so it is provably a canonical
    /// `["guard", authority]` PDA; *which* authority is the creator's choice and
    /// is what depositors should inspect before entering the market.
    pub guard_state: Option<UncheckedAccount<'info>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn create_handler(
    ctx: Context<Create>,
    ltv_percent: u8,
    max_feed_age_ms: u32,
) -> Result<()> {
    require!(
        max_feed_age_ms > 0,
        crate::error::ErrorCode::InvalidMaxFeedAge
    );
    require!(
        math::is_valid_ltv_percent(ltv_percent),
        crate::error::ErrorCode::InvalidLtv
    );

    // ── Pin the pool's dependencies to the canonical programs ─────────────────
    //
    // `feed_program` and `rate_program` are stored verbatim and trusted by every
    // later borrow / LTV check, so accepting arbitrary program IDs here would let
    // anyone stand up a pool backed by a price oracle and rate model they
    // control, then drain whoever deposited into it.
    require!(
        ctx.accounts.feed_program.key() == feed::ID,
        crate::error::ErrorCode::InvalidProgramId
    );
    require!(
        ctx.accounts.rate_program.key() == irm::ID,
        crate::error::ErrorCode::InvalidProgramId
    );

    // The feed must price *this* pool's pair. Without this a pool could be
    // created against a perfectly legitimate feed for unrelated tokens, and
    // every LTV check would then run on a price that has nothing to do with the
    // collateral being posted.
    require!(
        ctx.accounts.feed_state.collateral_mint == ctx.accounts.collateral_mint.key()
            && ctx.accounts.feed_state.lend_mint == ctx.accounts.lend_mint.key(),
        crate::error::ErrorCode::FeedMintMismatch
    );

    // The IRM state must be the canonical PDA for *this* pool, so a pool cannot
    // be pointed at another pool's (or an attacker's) rate curve.
    let (expected_irm_state, _) = Pubkey::find_program_address(
        &[b"irm_config", ctx.accounts.pool.key().as_ref()],
        &irm::ID,
    );
    require!(
        ctx.accounts.irm_state.key() == expected_irm_state,
        crate::error::ErrorCode::InvalidIrmState
    );

    // ...and the market's own authority must control it. The address check above
    // proves *which* account holds the curve, not who may rewrite it:
    // `irm::initialize` is permissionless and first-come-first-served on that
    // PDA, and whoever claims it names themselves the authority that `set_fee_points`
    // answers to, for the life of the market.
    //
    // Nothing else establishes that relationship. A market whose IRM was
    // initialised under some other key — by a deploy script, by accident, or by
    // someone who saw the pool pubkey before the creator's own
    // `irm::initialize` landed — is permanently bound to a rate curve that key
    // controls, and no field on `Pool` would let a depositor notice. Interest
    // accrues to `total_supply_assets` as well as to borrowers, so a hostile
    // curve inflates the LP share price against debt that cannot be serviced,
    // and there is no liquidation path to close the resulting positions.
    require!(
        ctx.accounts.irm_state.load()?.authority == ctx.accounts.authority.key(),
        crate::error::ErrorCode::InvalidIrmAuthority
    );

    // ── Bind the market's whitelist (opt-in, then permanent) ──────────────────
    //
    // Market creation is permissionless by design, so the guard accounts are
    // optional — omitting them yields an open market. Supplying them gates the
    // market on that whitelist for the rest of its life: the address is stored
    // below and every later entry point compares against the *stored* value, so
    // no caller can swap in a list of their own afterwards.
    //
    // The creator is checked against the list they chose, so a market cannot be
    // stood up on a whitelist its own creator is not a member of.
    let guard_state_key = match (&ctx.accounts.guard_program, &ctx.accounts.guard_state) {
        (Some(guard_program), Some(guard_state)) => {
            require!(
                guard_program.key() == guard::ID,
                crate::error::ErrorCode::InvalidProgramId
            );
            crate::hooks::guard::check_whitelist(
                guard_program.to_account_info(),
                guard_state.to_account_info(),
                ctx.accounts.authority.key(),
            )?;
            guard_state.key()
        }
        // Half a pair is a malformed request, not an open market — refuse rather
        // than silently creating an ungated market the caller did not ask for.
        (None, None) => Pubkey::default(),
        _ => return Err(crate::error::ErrorCode::GuardRequired.into()),
    };

    // ── CPI to feed::get_state to confirm the feed is reachable + fresh. ─────
    let _oracle = OracleState::new(
        ctx.accounts.feed_program.to_account_info(),
        ctx.accounts.feed_state.to_account_info(),
        max_feed_age_ms,
    )?;

    // ── Fetch initial IRM rate before load_init ───────────────────────────────
    // In Anchor 1.0, load_init() does NOT write the discriminator — that happens
    // in AccountsExit::exit() after the handler returns. Calling load() after
    // load_init() in the same instruction sees zeros and returns error 3002.
    // Utilization is 0 because the pool is brand-new with no borrows.
    let irm = crate::hooks::irm::IrmState::new(
        ctx.accounts.rate_program.to_account_info(),
        0,
        ctx.accounts.pool.to_account_info(),
        ctx.accounts.irm_state.to_account_info(),
    )?;

    // ── Initialise pool fields and accrue interest ────────────────────────────
    let mut pool = ctx.accounts.pool.load_init()?;

    pool.authority = ctx.accounts.authority.key();
    pool.collateral_mint = ctx.accounts.collateral_mint.key();
    pool.lend_mint = ctx.accounts.lend_mint.key();
    pool.lp_mint = ctx.accounts.lp_mint.key();
    pool.market.total_supply_assets = 0;
    pool.market.total_supply_shares = 0;
    pool.market.total_borrow_assets = 0;
    pool.market.total_borrow_shares = 0;
    // Stamp explicitly rather than relying on the `accrue_interest` side effect
    // below — a 0 `last_update` on a pool that later accrues borrow assets makes
    // `elapsed` span the entire unix epoch and overflows `compute_interest`.
    pool.market.last_update = Clock::get()?.unix_timestamp;
    // Permanent: there is no setter, so the protocol fee stays 0 for the life of
    // the pool and `accrue_interest` never mints fee shares.
    pool.market.fee = 0;
    pool.market.accrued_fee_shares = 0;
    pool.market.assets_in_queue = 0;
    pool.market.ltv_percent = ltv_percent;
    pool.rate_program = ctx.accounts.rate_program.key();
    pool.irm_state = ctx.accounts.irm_state.key();
    pool.feed_program = ctx.accounts.feed_program.key();
    pool.feed_state = ctx.accounts.feed_state.key();
    pool.lp_mint_bump = ctx.bumps.lp_mint;
    pool.max_feed_age_ms = max_feed_age_ms;
    pool.guard_state = guard_state_key;
    pool.market.flash_loan_outstanding = 0;
    // withdrawal_queue is zero-initialised by load_init (head=0, tail=0)

    {
        let core = math::Core::new(pool.market)
            .with_irm(irm)
            .accrue_interest()
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
        pool.market = core.market;
    }

    msg!(
        "Created pool for authority: {} collateral_mint: {} lend_mint: {} lp_mint: {} guard: {} at slot: {}",
        ctx.accounts.authority.key(),
        ctx.accounts.collateral_mint.key(),
        ctx.accounts.lend_mint.key(),
        ctx.accounts.lp_mint.key(),
        guard_state_key,
        Clock::get()?.slot
    );

    Ok(())
}
