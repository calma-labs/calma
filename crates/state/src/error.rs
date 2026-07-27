use anchor_lang::prelude::*;

impl From<math::MathError<Error>> for ErrorCode {
    fn from(e: math::MathError<Error>) -> Self {
        match e {
            math::MathError::Arithmetic => ErrorCode::MathOverflow,
            math::MathError::Undercollateralized => ErrorCode::Undercollateralized,
            math::MathError::InsufficientBalance => ErrorCode::InsufficientFunds,
            math::MathError::AmountTooSmall => ErrorCode::InvalidAmount,
            math::MathError::Transfer(_) => ErrorCode::MathOverflow,
        }
    }
}

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
    #[msg("Borrow amount exceeds collateral LTV limit")]
    Undercollateralized,
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
    #[msg("Curve index must be 0–3")]
    InvalidCurveIndex,
    #[msg("Rate program account missing from remaining accounts")]
    MissingRateProgram,
    #[msg("Rate state account missing from remaining accounts")]
    MissingRateState,
    #[msg("Create pool: no matching feed set_value instruction found before this transaction")]
    FeedSetValueMissing,
    #[msg("max_feed_age_secs must be greater than zero")]
    InvalidMaxFeedAge,
    #[msg("Interest accrual computation overflowed")]
    InterestAccrualOverflow,
    #[msg("Debt share-to-amount valuation overflowed")]
    DebtValuationOverflow,
    #[msg("Protocol fee exceeds the maximum allowed rate")]
    FeeTooHigh,
    #[msg("No accrued protocol fees to claim")]
    NoFeesToClaim,
}
