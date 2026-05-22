use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Custom error message")]
    CustomError,
    #[msg("Invalid amount provided")]
    InvalidAmount,
    #[msg("Mathematical operation overflow")]
    MathOverflow,
    #[msg("Insufficient funds")]
    InsufficientFunds,
    #[msg("An open borrow position already exists; repay before borrowing again")]
    AlreadyBorrowed,
    #[msg("No open borrow to repay")]
    NoBorrowFound,
    #[msg("Withdrawal queue is full; try again later")]
    WithdrawalQueueFull,
    #[msg("Withdrawal queue is empty")]
    WithdrawalQueueEmpty,
    #[msg("Provided account does not match the queued withdrawal entry")]
    QueueEntryMismatch,
    #[msg("min_duration must be > 0 and <= max_duration")]
    InvalidDurationRange,
    #[msg("Provided mint does not match the pool's expected mint")]
    InvalidMint,
    #[msg("Signer is not authorized for this account")]
    Unauthorized,
    #[msg("Hedge duration has not yet elapsed; cannot settle")]
    HedgeNotYetMatured,
    #[msg("Flash loan: repay amount is less than borrowed amount plus fee")]
    FlashLoanFeeNotCovered,
    #[msg("Flash loan: no matching flash_repay instruction found in this transaction")]
    FlashRepayMissing,
    #[msg("Flash loan: no matching flash_borrow instruction found in this transaction")]
    FlashBorrowMissing,
}
