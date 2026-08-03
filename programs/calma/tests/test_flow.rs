mod common;
use common::{fixture::Fixture, read_token_balance, send_ixs};

use {
    anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas},
    anchor_spl::token::spl_token,
};

const LEND_DEPOSIT: u64 = 10_000_000;
const COL_DEPOSIT: u64 = 10_000_000;
const BORROW_AMOUNT: u64 = 5_000_000; // 50% LTV — within the 75% cap

/// The full round trip: supply, collateralize, borrow, repay, and take both
/// sides back out again. Every balance returns to where it started, which is the
/// property that catches an accounting error anywhere along the path.
#[test]
fn test_flow() {
    let mut f = Fixture::builder().build();
    let calma_id = calma::id();
    let system_program = anchor_lang::solana_program::system_program::id();

    // A non-ATA source for DepositLent; the borrow/withdraw side uses the ATAs
    // the program creates.
    let user_lend_src = f.funded_token_account(f.lend_mint, LEND_DEPOSIT);
    let user_col_account = f.funded_token_account(f.col_mint, COL_DEPOSIT);

    // ── 1. Deposit lend tokens ────────────────────────────────────────────────
    let ix = Instruction::new_with_bytes(
        calma_id,
        &calma::instruction::DepositLent {
            amount: LEND_DEPOSIT,
        }
        .data(),
        calma::accounts::DepositLent {
            guard_program: None,
            guard_state: None,
            pool: f.pool,
            state: f.state,
            lend_mint: f.lend_mint,
            lp_mint: f.lp_mint,
            authority: f.authority(),
            user_lend_token_account: user_lend_src,
            user_lp_token_account: f.user_lp_ata,
            lend_vault: f.lend_vault,
            rate_program: f.rate_program,
            irm_state: f.rate_state,
            token_program: spl_token::id(),
            associated_token_program: f.atp_id,
            system_program,
        }
        .to_account_metas(None),
    );
    let payer = f.payer.insecure_clone();
    send_ixs(&mut f.svm, &[ix], &payer, &[&payer]);

    // ── 2. Deposit collateral ─────────────────────────────────────────────────
    let ix = Instruction::new_with_bytes(
        calma_id,
        &calma::instruction::DepositCollateral {
            amount: COL_DEPOSIT,
        }
        .data(),
        calma::accounts::DepositCollateral {
            guard_program: None,
            guard_state: None,
            pool: f.pool,
            collateral_mint: f.col_mint,
            authority: f.authority(),
            user_token_account: user_col_account,
            collateral_vault: f.col_vault,
            user_position: f.user_position,
            token_program: spl_token::id(),
            system_program,
        }
        .to_account_metas(None),
    );
    send_ixs(&mut f.svm, &[ix], &payer, &[&payer]);

    // ── 3. Borrow ─────────────────────────────────────────────────────────────
    let ix = Instruction::new_with_bytes(
        calma_id,
        &calma::instruction::Borrow {
            amount: BORROW_AMOUNT,
        }
        .data(),
        calma::accounts::Borrow {
            guard_program: None,
            guard_state: None,
            pool: f.pool,
            state: f.state,
            lend_mint: f.lend_mint,
            authority: f.authority(),
            user_token_account: f.user_lend_ata,
            lend_vault: f.lend_vault,
            user_position: f.user_position,
            rate_program: f.rate_program,
            irm_state: f.rate_state,
            feed_state: f.feed,
            token_program: spl_token::id(),
            associated_token_program: f.atp_id,
            system_program,
        }
        .to_account_metas(None),
    );
    send_ixs(&mut f.svm, &[ix], &payer, &[&payer]);

    assert_eq!(read_token_balance(&f.svm, &f.user_lend_ata), BORROW_AMOUNT);
    assert_eq!(
        read_token_balance(&f.svm, &f.lend_vault),
        LEND_DEPOSIT - BORROW_AMOUNT
    );

    // ── 4. Repay ──────────────────────────────────────────────────────────────
    // u64::MAX lets the handler cap to total_due, covering any accrued interest.
    let ix = Instruction::new_with_bytes(
        calma_id,
        &calma::instruction::Repay { amount: u64::MAX }.data(),
        calma::accounts::Repay {
            pool: f.pool,
            lend_mint: f.lend_mint,
            authority: f.authority(),
            user_token_account: f.user_lend_ata,
            lend_vault: f.lend_vault,
            user_position: f.user_position,
            rate_program: f.rate_program,
            irm_state: f.rate_state,
            token_program: spl_token::id(),
            associated_token_program: f.atp_id,
            system_program,
        }
        .to_account_metas(None),
    );
    send_ixs(&mut f.svm, &[ix], &payer, &[&payer]);

    assert_eq!(read_token_balance(&f.svm, &f.user_lend_ata), 0);
    assert_eq!(read_token_balance(&f.svm, &f.lend_vault), LEND_DEPOSIT);

    // ── 5. Withdraw collateral ────────────────────────────────────────────────
    let ix = Instruction::new_with_bytes(
        calma_id,
        &calma::instruction::WithdrawCollateral {
            amount: COL_DEPOSIT,
        }
        .data(),
        calma::accounts::WithdrawCollateral {
            pool: f.pool,
            state: f.state,
            collateral_mint: f.col_mint,
            authority: f.authority(),
            user_token_account: user_col_account,
            collateral_vault: f.col_vault,
            user_position: f.user_position,
            rate_program: f.rate_program,
            irm_state: f.rate_state,
            feed_state: f.feed,
            token_program: spl_token::id(),
            system_program,
        }
        .to_account_metas(None),
    );
    send_ixs(&mut f.svm, &[ix], &payer, &[&payer]);

    assert_eq!(read_token_balance(&f.svm, &user_col_account), COL_DEPOSIT);
    assert_eq!(read_token_balance(&f.svm, &f.col_vault), 0);

    // ── 6. Withdraw lent ──────────────────────────────────────────────────────
    let ix = Instruction::new_with_bytes(
        calma_id,
        &calma::instruction::WithdrawLent {
            shares: LEND_DEPOSIT,
        }
        .data(),
        calma::accounts::WithdrawLent {
            pool: f.pool,
            state: f.state,
            lend_mint: f.lend_mint,
            lp_mint: f.lp_mint,
            authority: f.authority(),
            user_lp_token_account: f.user_lp_ata,
            user_lend_token_account: f.user_lend_ata,
            lend_vault: f.lend_vault,
            rate_program: f.rate_program,
            irm_state: f.rate_state,
            token_program: spl_token::id(),
            associated_token_program: f.atp_id,
            system_program,
        }
        .to_account_metas(None),
    );
    send_ixs(&mut f.svm, &[ix], &payer, &[&payer]);

    assert_eq!(read_token_balance(&f.svm, &f.user_lend_ata), LEND_DEPOSIT);
    assert_eq!(read_token_balance(&f.svm, &f.lend_vault), 0);
}
