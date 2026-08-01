use anchor_lang::prelude::*;

impl From<math::MathError<Error>> for ErrorCode {
    fn from(e: math::MathError<Error>) -> Self {
        match e {
            math::MathError::Arithmetic => ErrorCode::MathOverflow,
            math::MathError::Undercollateralized => ErrorCode::Undercollateralized,
            math::MathError::InsufficientBalance => ErrorCode::InsufficientFunds,
            math::MathError::AmountTooSmall => ErrorCode::InvalidAmount,
            math::MathError::InvalidAmount => ErrorCode::InvalidAmount,
            math::MathError::InsufficientLiquidity => ErrorCode::InsufficientFunds,
            math::MathError::FlashLoanOutstanding => ErrorCode::FlashLoanAlreadyOutstanding,
            math::MathError::NoFlashLoan => ErrorCode::FlashBorrowMissing,
            math::MathError::FlashLoanUnderRepaid => ErrorCode::FlashLoanFeeNotCovered,
            math::MathError::Transfer(_) => ErrorCode::MathOverflow,
        }
    }
}

/// Anchor assigns error numbers by **declaration order**, so variants are never
/// deleted from this enum — removing one silently renumbers every variant after
/// it and breaks any client that matches on a code. Variants left over from the
/// removed rate-hedge subsystem, and from the removed `set_fee` instruction, are
/// marked `RESERVED` and kept as placeholders; see `docs/rate-hedge-removal.md`.
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
    /// RESERVED — rate-hedge subsystem removed; kept to preserve error numbering.
    #[msg("min_duration must be > 0 and <= max_duration")]
    InvalidDurationRange,
    #[msg("Provided mint does not match the pool's expected mint")]
    InvalidMint,
    #[msg("Signer is not authorized for this account")]
    Unauthorized,
    /// RESERVED — rate-hedge subsystem removed; kept to preserve error numbering.
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
    #[msg("max_feed_age_ms must be greater than zero")]
    InvalidMaxFeedAge,
    #[msg("Interest accrual computation overflowed")]
    InterestAccrualOverflow,
    #[msg("Debt share-to-amount valuation overflowed")]
    DebtValuationOverflow,
    /// RESERVED — the `set_fee` instruction was removed; the fee is fixed at 0
    /// and nothing validates a caller-supplied rate any more.
    #[msg("Protocol fee exceeds the maximum allowed rate")]
    FeeTooHigh,
    #[msg("No accrued protocol fees to claim")]
    NoFeesToClaim,
    #[msg("Flash loan: this pool already has an outstanding flash loan")]
    FlashLoanAlreadyOutstanding,
    #[msg("Operation unavailable while a flash loan is outstanding on this pool")]
    FlashLoanInProgress,
    #[msg("LTV percent must be between 1 and the protocol maximum")]
    InvalidLtv,
    #[msg("Account is not the canonical program expected by the pool")]
    InvalidProgramId,
    #[msg("IRM state is not the canonical PDA for this pool")]
    InvalidIrmState,
    /// RESERVED — rate-hedge subsystem removed; kept to preserve error numbering.
    #[msg("Offer still backs an unsettled match; settle it before cancelling")]
    OfferHasActiveMatch,
    #[msg("Interest rate point exceeds the maximum allowed rate")]
    RateTooHigh,
    #[msg("Feed does not price this pool's collateral/lend pair")]
    FeedMintMismatch,
    #[msg("Guard state is not the canonical whitelist PDA")]
    InvalidGuardState,
    #[msg("This market is whitelist-gated; guard_program and guard_state must be supplied")]
    GuardRequired,
    #[msg("IRM state is controlled by a different authority than this pool")]
    InvalidIrmAuthority,
}
