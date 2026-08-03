use crate::state::{Pool, UserPosition};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use bytemuck::Zeroable;

#[derive(Accounts)]
pub struct DepositCollateral<'info> {
    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// The collateral token mint.
    pub collateral_mint: Account<'info, Mint>,

    /// The depositor
    #[account(mut)]
    pub authority: Signer<'info>,

    /// The user's collateral token account (source)
    #[account(
        mut,
        constraint = user_token_account.owner == authority.key(),
        constraint = user_token_account.mint == collateral_mint.key(),
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    /// The pool's collateral vault (destination)
    #[account(
        mut,
        seeds = [::state::seeds::COLLATERAL_VAULT, pool.key().as_ref()],
        bump,
    )]
    pub collateral_vault: Account<'info, TokenAccount>,

    /// PDA that records the user's collateral deposit and borrow position.
    #[account(
        init_if_needed,
        payer = authority,
        space = 8 + std::mem::size_of::<UserPosition>(),
        seeds = [::state::seeds::USER_POSITION, pool.key().as_ref(), authority.key().as_ref()],
        bump,
    )]
    pub user_position: AccountLoader<'info, UserPosition>,

    /// CHECK: required iff `pool.guard_state` is set; validated in the handler.
    pub guard_program: Option<UncheckedAccount<'info>>,

    /// CHECK: required iff `pool.guard_state` is set; must equal it exactly.
    pub guard_state: Option<UncheckedAccount<'info>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

impl<'info> DepositCollateral<'info> {
    pub fn transfer_collateral_to_vault(&self, amount: u64) -> Result<()> {
        anchor_spl::token::transfer(
            CpiContext::new(
                *self.token_program.to_account_info().key,
                anchor_spl::token::Transfer {
                    from: self.user_token_account.to_account_info(),
                    to: self.collateral_vault.to_account_info(),
                    authority: self.authority.to_account_info(),
                },
            ),
            amount,
        )
    }
}

pub fn deposit_collateral_handler(ctx: Context<DepositCollateral>, amount: u64) -> Result<()> {
    require!(amount > 0, crate::error::ErrorCode::InvalidAmount);

    // Validate the mint matches the pool's collateral_mint.
    let (pool_guard_state, pool_guard_program) = {
        let pool = ctx.accounts.pool.load()?;
        require!(
            ctx.accounts.collateral_mint.key() == pool.collateral_mint,
            crate::error::ErrorCode::InvalidAmount
        );
        (pool.guard_state, pool.guard_program)
    };

    // Whitelist gate — entry only. `withdraw_collateral` is deliberately never
    // gated: a depositor removed from the list afterwards must still be able to
    // get their collateral out.
    crate::hooks::guard::enforce_pool_guard(
        pool_guard_state,
        pool_guard_program,
        &ctx.accounts.guard_program,
        &ctx.accounts.guard_state,
        ctx.accounts.authority.key(),
    )?;

    let needs_init = {
        let account_info = ctx.accounts.user_position.to_account_info();
        let data = account_info.try_borrow_data()?;
        data.len() >= 8 && data[..8].iter().all(|&b| b == 0)
    };

    // Transfer + update position via Core.
    // For new positions use a zeroed starting value — load_init() in Anchor 1.0
    // doesn't write the discriminator until AccountsExit, so calling load() on the
    // same account in the same handler would fail with discriminator mismatch.
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        let starting_position = if needs_init {
            UserPosition::zeroed()
        } else {
            *ctx.accounts.user_position.load()?
        };
        let mut core = math::Core::new(pool.market).with_position(starting_position);
        core.deposit_collateral(amount, |amt| ctx.accounts.transfer_collateral_to_vault(amt))
            .map_err(crate::error::ErrorCode::from)?;
        pool.market = core.market;
        if needs_init {
            let mut position = ctx.accounts.user_position.load_init()?;
            position.authority = ctx.accounts.authority.key();
            position.pool = ctx.accounts.pool.key();
            position.collateral_deposited = core.position.collateral_deposited;
            position.bump = ctx.bumps.user_position;
        } else {
            ctx.accounts.user_position.load_mut()?.collateral_deposited =
                core.position.collateral_deposited;
        }
    }

    Ok(())
}
