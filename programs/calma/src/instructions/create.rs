use crate::state::Pool;
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
        seeds = [::state::seeds::STATE],
        bump,
    )]
    pub state: UncheckedAccount<'info>,

    /// Token vault for holding deposited collateral tokens.
    #[account(
        init,
        payer = payer,
        token::mint = collateral_mint,
        token::authority = state,
        seeds = [::state::seeds::COLLATERAL_VAULT, pool.key().as_ref()],
        bump
    )]
    pub collateral_vault: Account<'info, TokenAccount>,

    /// Token vault for holding deposited lend tokens.
    #[account(
        init,
        payer = payer,
        token::mint = lend_mint,
        token::authority = state,
        seeds = [::state::seeds::LEND_VAULT, pool.key().as_ref()],
        bump
    )]
    pub lend_vault: Account<'info, TokenAccount>,

    /// The LP token mint for the lend side of this pool.
    #[account(
        init,
        payer = payer,
        mint::decimals = lend_mint.decimals,
        mint::authority = state,
        seeds = [::state::seeds::LP_MINT, pool.key().as_ref()],
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

    /// CHECK: the price account this market binds itself to, for life. Cannot be
    /// typed — `Account<'info, T>` resolves its owner check from the defining
    /// crate's program ID, which is exactly the single-oracle coupling this
    /// design removes. The handler reads it through `interface::read_price_feed`
    /// instead, and records its owner below as `pool.feed_program`.
    pub feed_state: UncheckedAccount<'info>,

    /// CHECK: IRM program — invoked via CPI to fetch the initial borrow rate and
    /// to verify the curve's authority. Recorded as `pool.rate_program`.
    pub rate_program: UncheckedAccount<'info>,

    /// CHECK: the rate curve this market prices against. Untyped for the same
    /// reason as `feed_state`; the handler pins it to the canonical
    /// `["irm_config", pool]` PDA under `rate_program` and asks that program to
    /// vouch for its authority over CPI.
    pub irm_state: UncheckedAccount<'info>,

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

pub fn create_handler(ctx: Context<Create>, ltv_percent: u8) -> Result<()> {
    require!(
        math::is_valid_ltv_percent(ltv_percent),
        crate::error::ErrorCode::InvalidLtv
    );

    // ── Bind the market to its oracle and rate model, for life ────────────────
    //
    // Both are pluggable: any program that writes an `interface::PriceFeedHeader`
    // can price a market, and any program implementing the IRM ABI can rate one.
    // So there is no canonical program ID to compare against here — the market's
    // *choice* is what gets recorded, and every later borrow / LTV check is
    // pinned to it.
    //
    // That makes `feed_program` and `guard_state` the two fields a depositor has
    // to inspect before entering a market: they name who is trusted to price it
    // and who is allowed into it. Neither can change afterwards.
    //
    // `pool.feed_program` is taken from the account's actual owner rather than
    // from a separate caller-supplied argument, so the recorded value cannot
    // disagree with the account it describes.
    let feed_program_key = *ctx.accounts.feed_state.owner;
    let feed_header = crate::hooks::oracle::read_feed(
        &ctx.accounts.feed_state,
        ctx.accounts.feed_state.key(),
        feed_program_key,
    )?;

    // The feed must price *this* pool's pair. Without this a pool could be
    // created against a perfectly legitimate feed for unrelated tokens, and
    // every LTV check would then run on a price that has nothing to do with the
    // collateral being posted.
    require!(
        feed_header.collateral_mint == ctx.accounts.collateral_mint.key()
            && feed_header.lend_mint == ctx.accounts.lend_mint.key(),
        crate::error::ErrorCode::FeedMintMismatch
    );

    // The IRM state must be the canonical PDA for *this* pool under the chosen
    // rate program, so a pool cannot be pointed at another pool's (or an
    // attacker's) rate curve. The seed convention is part of the IRM ABI — the
    // reference implementation enforces the same derivation on its own side.
    let (expected_irm_state, _) = Pubkey::find_program_address(
        &[
            ::interface::IRM_CONFIG_SEED,
            ctx.accounts.pool.key().as_ref(),
        ],
        &ctx.accounts.rate_program.key(),
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
    //
    // Asked over CPI rather than read off the account: with pluggable IRMs,
    // `calma` does not know the layout of whatever program this market chose.
    crate::hooks::irm::check_irm_authority(
        ctx.accounts.rate_program.to_account_info(),
        ctx.accounts.irm_state.to_account_info(),
        ctx.accounts.pool.to_account_info(),
        ctx.accounts.authority.key(),
    )?;

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
    //
    // Both the program and the state are recorded. There is no canonical guard
    // program to compare against — a guard is any program exposing
    // `check(Pubkey)`, as with the oracle and the rate model — so the market's
    // choice *is* the pin, and every later gate compares against these stored
    // values rather than against what a caller passes.
    let (guard_state_key, guard_program_key) =
        match (&ctx.accounts.guard_program, &ctx.accounts.guard_state) {
            (Some(guard_program), Some(guard_state)) => {
                crate::hooks::guard::check_whitelist(
                    guard_program.to_account_info(),
                    guard_state.to_account_info(),
                    ctx.accounts.authority.key(),
                )?;
                (guard_state.key(), guard_program.key())
            }
            // Half a pair is a malformed request, not an open market — refuse
            // rather than silently creating an ungated market the caller did not
            // ask for.
            (None, None) => (Pubkey::default(), Pubkey::default()),
            _ => return Err(crate::error::ErrorCode::GuardRequired.into()),
        };

    // ── Fetch initial IRM rate before load_init ───────────────────────────────
    // In Anchor 1.0, load_init() does NOT write the discriminator — that happens
    // in AccountsExit::exit() after the handler returns. Calling load() after
    // load_init() in the same instruction sees zeros and returns error 3002.
    // Utilization is 0 because the pool is brand-new with no borrows.
    let irm = crate::hooks::irm::IrmState::new_with_utilization(
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
    pool.feed_program = feed_program_key;
    pool.feed_state = ctx.accounts.feed_state.key();
    pool.lp_mint_bump = ctx.bumps.lp_mint;
    pool.guard_state = guard_state_key;
    pool.guard_program = guard_program_key;
    pool.market.flash_loan_outstanding = 0;
    // withdrawal_queue is zero-initialised by load_init (head=0, tail=0)

    {
        math::Core::new(&mut pool.market)
            .with_irm(irm)
            .accrue_interest()
            .ok_or(crate::error::ErrorCode::MathOverflow)?;
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
