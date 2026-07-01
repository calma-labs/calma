use crate::error::ErrorCode;
use anchor_lang::prelude::*;

// Decimal places represented by PRICE_SCALE (1e6).
const TARGET_DECIMALS: u32 = 6;

pub fn normalize(price: i64, exponent: i32) -> Result<u64> {
    require!(price > 0, ErrorCode::NonPositivePrice);
    let mantissa = price as u128;

    let scaled: u128 = if exponent >= 0 {
        let e = (exponent as i64)
            .checked_add(TARGET_DECIMALS as i64)
            .ok_or(ErrorCode::PriceOverflow)?;
        let exp_u32 = u32::try_from(e).map_err(|_| ErrorCode::PriceOverflow)?;
        let pow = 10u128
            .checked_pow(exp_u32)
            .ok_or(ErrorCode::PriceOverflow)?;
        mantissa.checked_mul(pow).ok_or(ErrorCode::PriceOverflow)?
    } else {
        let e_abs = (-(exponent as i64)) as u32;
        if e_abs >= TARGET_DECIMALS {
            let div = 10u128
                .checked_pow(e_abs - TARGET_DECIMALS)
                .ok_or(ErrorCode::PriceOverflow)?;
            mantissa
                .checked_div(div)
                .ok_or(ErrorCode::PriceOverflow)?
        } else {
            let mul = 10u128
                .checked_pow(TARGET_DECIMALS - e_abs)
                .ok_or(ErrorCode::PriceOverflow)?;
            mantissa.checked_mul(mul).ok_or(ErrorCode::PriceOverflow)?
        }
    };

    require!(scaled > 0, ErrorCode::PriceOverflow);
    u64::try_from(scaled).map_err(|_| ErrorCode::PriceOverflow.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_exponent_smaller_than_target_scales_up() {
        // price = 12345, exponent = -2 ⇒ real value 123.45.
        // Target scale is 1e6, so result = 123.45 * 1_000_000 = 123_450_000.
        assert_eq!(normalize(12_345, -2).unwrap(), 123_450_000);
    }

    #[test]
    fn negative_exponent_equal_to_target_is_passthrough() {
        // Pyth's native USD scale: exponent = -6 already matches PRICE_SCALE.
        assert_eq!(normalize(1_000_000, -6).unwrap(), 1_000_000);
    }

    #[test]
    fn negative_exponent_larger_than_target_scales_down() {
        // exponent = -9 ⇒ divide by 10^3 to reach 1e6 target.
        assert_eq!(normalize(1_234_567_000, -9).unwrap(), 1_234_567);
    }

    #[test]
    fn positive_exponent_scales_up() {
        // price = 1, exponent = 2 ⇒ real 100 ⇒ * 1e6 = 100_000_000.
        assert_eq!(normalize(1, 2).unwrap(), 100_000_000);
    }

    #[test]
    fn non_positive_price_rejected() {
        assert!(normalize(0, -6).is_err());
        assert!(normalize(-1, -6).is_err());
    }

    #[test]
    fn overflow_rejected() {
        // i64::MAX with positive exponent overflows u64 trivially.
        assert!(normalize(i64::MAX, 6).is_err());
    }
}
