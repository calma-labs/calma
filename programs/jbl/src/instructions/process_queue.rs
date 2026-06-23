use crate::state::{Pool, UserPosition};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

/// Anyone may call this instruction to attempt to fulfil the next pending
/// withdrawal in the queue.  Pass the accounts that match the head entry.
/// The instruction returns `InsufficientFunds` (without dequeueing) if the
/// vault still does not have enough liquidity.
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

    /// The mint of the token being withdrawn.
    pub mint: Account<'info, Mint>,

    /// Destination token account for the queued withdrawal.
    /// Must be owned by the requester recorded in the head queue entry.
    #[account(
        mut,
        constraint = user_token_account.mint == mint.key(),
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    /// The user's position PDA derived from the pool and the requester.
    /// Seeds: ["user_position", pool, requester].
    #[account(
        mut,
        seeds = [
            b"user_position",
            pool.key().as_ref(),
            user_token_account.owner.as_ref(),
        ],
        bump = user_position.load()?.bump,
        constraint = user_position.load()?.pool == pool.key()
            @ crate::error::ErrorCode::QueueEntryMismatch,
    )]
    pub user_position: AccountLoader<'info, UserPosition>,

    /// The pool's token vault (source of funds).
    #[account(
        mut,
        seeds = [b"pool", pool.key().as_ref()],
        bump,
    )]
    pub vault: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn process_queue_entry_handler(_ctx: Context<ProcessQueueEntry>) -> Result<()> {
    todo!("Move queue to vault.");

    //     // ── 1. Peek at head entry (read-only, no dequeue yet) ─────────────────────
    //     let entry = {
    //         let pool = ctx.accounts.pool.load()?;
    //         let q = &pool.withdrawal_queue;
    //         require!(
    //             q.head != q.tail,
    //             crate::error::ErrorCode::WithdrawalQueueEmpty
    //         );
    //         q.entries[q.head as usize]
    //     };

    //     // ── 2. Validate accounts match the queued requester ───────────────────────
    //     require!(
    //         ctx.accounts.user_token_account.owner == entry.requester,
    //         crate::error::ErrorCode::QueueEntryMismatch
    //     );

    //     let amount = entry.amount;

    //     // ── 3. Accrue interest ────────────────────────────────────────────────────
    //     let current_ts = Clock::get()?.unix_timestamp;
    //     ctx.accounts.pool.load_mut()?.accrue_interest(current_ts)?;

    //     // ── 4. Check vault liquidity (do NOT dequeue on failure) ──────────────────
    //     require!(
    //         ctx.accounts.vault.amount >= amount,
    //         crate::error::ErrorCode::InsufficientFunds
    //     );

    //     // ── 5. LTV check ──────────────────────────────────────────────────────────
    //     let (remaining_deposit, lp_issued_before, total_deposited_before) = {
    //         let pool = ctx.accounts.pool.load()?;
    //         let position = &ctx.accounts.user_position;

    //         let remaining_deposit = position
    //             .deposited_amount
    //             .checked_sub(amount)
    //             .ok_or(crate::error::ErrorCode::MathOverflow)?;
    //         let max_borrowable = remaining_deposit
    //             .checked_mul(pool.market.ltv_percent as u64)
    //             .ok_or(crate::error::ErrorCode::MathOverflow)?
    //             .checked_div(100)
    //             .ok_or(crate::error::ErrorCode::MathOverflow)?;
    //         let current_debt = if pool.total_debt_shares > 0 {
    //             shares_to_amount(
    //                 position.debt_shares,
    //                 pool.total_borrowed,
    //                 pool.total_debt_shares,
    //             )
    //             .ok_or(crate::error::ErrorCode::MathOverflow)?
    //         } else {
    //             0
    //         };
    //         require!(
    //             current_debt <= max_borrowable,
    //             crate::error::ErrorCode::InsufficientFunds
    //         );

    //         (
    //             remaining_deposit,
    //             pool.total_lp_issued,
    //             pool.total_deposited,
    //         )
    //     };

    //     // ── 6. Transfer tokens to the user ────────────────────────────────────────
    //     let seeds = &[b"state" as &[u8], &[ctx.bumps.state]];
    //     let signer = &[&seeds[..]];

    //     token::transfer(
    //         CpiContext::new_with_signer(
    //             ctx.accounts.token_program.to_account_info().key.clone(),
    //             Transfer {
    //                 from: ctx.accounts.vault.to_account_info(),
    //                 to: ctx.accounts.user_token_account.to_account_info(),
    //                 authority: ctx.accounts.state.to_account_info(),
    //             },
    //             signer,
    //         ),
    //         amount,
    //     )?;

    //     // ── 7. Update state and dequeue ───────────────────────────────────────────
    //     let lp_issued_reduction = if lp_issued_before > 0 {
    //         (amount as u128)
    //             .checked_mul(lp_issued_before as u128)
    //             .ok_or(crate::error::ErrorCode::MathOverflow)?
    //             .checked_div(total_deposited_before as u128)
    //             .ok_or(crate::error::ErrorCode::MathOverflow)? as u64
    //     } else {
    //         0
    //     };

    //     {
    //         let mut pool = ctx.accounts.pool.load_mut()?;
    //         pool.total_deposited = pool
    //             .total_deposited
    //             .checked_sub(amount)
    //             .ok_or(crate::error::ErrorCode::MathOverflow)?;
    //         pool.total_lp_issued = pool
    //             .total_lp_issued
    //             .checked_sub(lp_issued_reduction)
    //             .ok_or(crate::error::ErrorCode::MathOverflow)?;
    //         // pool.withdrawal_queue.pop()?; // dequeue only after successful transfer
    //     }

    //     let position = &mut ctx.accounts.user_position;
    //     position.deposited_amount = remaining_deposit;

    //     msg!(
    //         "Processed queued withdrawal: {} tokens for requester {}.",
    //         amount,
    //         entry.requester,
    //     );

    //     Ok(())
}
