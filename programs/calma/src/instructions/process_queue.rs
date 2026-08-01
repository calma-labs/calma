use crate::state::Pool;
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

/// Anyone may call this instruction to fulfil the next pending withdrawal in the
/// queue. Pass the destination token account belonging to the head entry's
/// requester.
///
/// The instruction fails **without dequeueing** when the vault still lacks the
/// liquidity to pay the head entry, so a starved queue simply waits for
/// borrowers to repay rather than losing its place. Processing is strictly FIFO:
/// the head must clear before anything behind it can.
#[derive(Accounts)]
pub struct ProcessQueueEntry<'info> {
    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: Signer-only PDA — no data stored; signs vault-transfer CPIs.
    #[account(
        seeds = [b"state"],
        bump,
    )]
    pub state: UncheckedAccount<'info>,

    /// The lend token mint — queued withdrawals always pay out lend tokens.
    #[account(constraint = lend_mint.key() == pool.load()?.lend_mint @ crate::error::ErrorCode::InvalidMint)]
    pub lend_mint: Account<'info, Mint>,

    /// Destination token account for the queued withdrawal. Ownership is checked
    /// against the head entry's `requester` in the handler, once the entry has
    /// been read.
    #[account(
        mut,
        constraint = user_token_account.mint == lend_mint.key()
            @ crate::error::ErrorCode::InvalidMint,
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    /// The pool's lend vault — source of funds.
    #[account(
        mut,
        seeds = [b"lend_vault", pool.key().as_ref()],
        bump,
        constraint = lend_vault.mint == lend_mint.key()
            @ crate::error::ErrorCode::InvalidMint,
    )]
    pub lend_vault: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

impl<'info> ProcessQueueEntry<'info> {
    fn transfer_lend_to_user(&self, amount: u64, state_bump: u8) -> Result<()> {
        let seeds = &[b"state" as &[u8], &[state_bump]];
        let signer = &[&seeds[..]];
        anchor_spl::token::transfer(
            CpiContext::new_with_signer(
                *self.token_program.to_account_info().key,
                anchor_spl::token::Transfer {
                    from: self.lend_vault.to_account_info(),
                    to: self.user_token_account.to_account_info(),
                    authority: self.state.to_account_info(),
                },
                signer,
            ),
            amount,
        )
    }
}

pub fn process_queue_entry_handler(ctx: Context<ProcessQueueEntry>) -> Result<()> {
    // ── 1. Peek at the head entry (read-only — no dequeue yet) ────────────────
    let entry = {
        let pool = ctx.accounts.pool.load()?;
        require!(
            pool.market.flash_loan_outstanding == 0,
            crate::error::ErrorCode::FlashLoanInProgress
        );
        let q = &pool.withdrawal_queue;
        require!(
            q.head != q.tail,
            crate::error::ErrorCode::WithdrawalQueueEmpty
        );
        q.entries[q.head as usize]
    };

    // ── 2. The destination must belong to the queued requester ───────────────
    require!(
        ctx.accounts.user_token_account.owner == entry.requester,
        crate::error::ErrorCode::QueueEntryMismatch
    );

    // ── 3. Liquidity check — fail without dequeueing so the entry keeps its
    //       place in line and can be retried once borrowers repay ─────────────
    require!(
        ctx.accounts.lend_vault.amount >= entry.amount,
        crate::error::ErrorCode::InsufficientFunds
    );

    // ── 4. Pay out, then dequeue ─────────────────────────────────────────────
    //
    // No interest accrual here: the LP shares were already burned and this
    // entry's claim fixed at `withdraw_lent` time, so the payout is not
    // share-price sensitive. Accruing would only move value between the lenders
    // still in the pool, which any borrow/repay does anyway.
    let state_bump = ctx.bumps.state;
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        let mut core = math::Core::new(pool.market);
        core.process_queued_withdrawal(entry.amount, |amt| {
            ctx.accounts.transfer_lend_to_user(amt, state_bump)
        })
        .map_err(|e| match e {
            math::MathError::Transfer(e) => e,
            e => crate::error::ErrorCode::from(e).into(),
        })?;
        pool.market = core.market;
        // Dequeue only after the transfer succeeded.
        pool.withdrawal_queue.pop()?;
    }

    let pool = ctx.accounts.pool.load()?;
    msg!(
        "ProcessQueueEntry: paid {} lend tokens to {}. remaining_queued={} assets_in_queue={}",
        entry.amount,
        entry.requester,
        pool.withdrawal_queue.len(),
        pool.market.assets_in_queue,
    );

    Ok(())
}
