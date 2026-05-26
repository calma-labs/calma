use anchor_lang::prelude::*;

/// Default flat fee applied to all new pools: 1 % APY (100 bps).
pub const DEFAULT_POOL_FEE_BPS: i64 = 100;

/// A linear rate curve: f(u) = a·(u/10_000) + b
///
/// `a` is the slope and `b` is the base rate, both in basis points.
/// The curve is only active when `enabled != 0`.
#[zero_copy]
#[derive(Debug, Default)]
pub struct PolynomialCurve {
    pub a: i64,
    pub b: i64,
    /// Non-zero → curve is active; 0 → disabled.
    pub enabled: u8,
    pub _pad: [u8; 7],
}

impl PolynomialCurve {
    fn eval(&self, utilization_bps: u64) -> i128 {
        let u = utilization_bps as i128;
        let slope = (self.a as i128).saturating_mul(u) / 10_000i128;
        (self.b as i128).saturating_add(slope)
    }
}

/// Four-curve linear interest rate model.
/// Effective rate = max(enabled curves), clamped to ≥ 0.
#[zero_copy]
#[derive(Debug)]
pub struct UtilizationFeeConfig {
    pub curves: [PolynomialCurve; 4],
}

impl UtilizationFeeConfig {
    pub fn get_fee_bps(&self, utilization_bps: u64) -> u32 {
        let max_y = self
            .curves
            .iter()
            .filter(|c| c.enabled != 0)
            .map(|curve| curve.eval(utilization_bps))
            .max()
            .unwrap_or(0);
        u32::try_from(max_y.max(0)).unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curve(a: i64, b: i64) -> PolynomialCurve {
        PolynomialCurve { a, b, enabled: 1, _pad: [0; 7] }
    }

    fn disabled(a: i64, b: i64) -> PolynomialCurve {
        PolynomialCurve { a, b, enabled: 0, _pad: [0; 7] }
    }

    fn make_config(curves: [PolynomialCurve; 4]) -> UtilizationFeeConfig {
        UtilizationFeeConfig { curves }
    }

    #[test]
    fn test_flat_fee() {
        let config = make_config([
            curve(0, 500),
            disabled(0, 0),
            disabled(0, 0),
            disabled(0, 0),
        ]);
        assert_eq!(config.get_fee_bps(0), 500);
        assert_eq!(config.get_fee_bps(5000), 500);
        assert_eq!(config.get_fee_bps(10000), 500);
    }

    #[test]
    fn test_linear_fee() {
        // y = 1000*(u/10000) + 200
        let config = make_config([
            curve(1000, 200),
            disabled(0, 0),
            disabled(0, 0),
            disabled(0, 0),
        ]);
        assert_eq!(config.get_fee_bps(0), 200);
        assert_eq!(config.get_fee_bps(5000), 700);
        assert_eq!(config.get_fee_bps(10000), 1200);
    }

    #[test]
    fn test_negative_values_clamped() {
        // y = 1000*(u/10000) - 500, negative until u=5000
        let config = make_config([
            curve(1000, -500),
            disabled(0, 0),
            disabled(0, 0),
            disabled(0, 0),
        ]);
        assert_eq!(config.get_fee_bps(1000), 0);
        assert_eq!(config.get_fee_bps(5000), 0);
        assert_eq!(config.get_fee_bps(6000), 100);
    }

    #[test]
    fn test_two_curves_max() {
        let config = make_config([
            curve(0, 100),    // flat 100 bps
            curve(500, 0),    // linear, 250 at u=5000
            disabled(0, 0),
            disabled(0, 0),
        ]);
        // At u=5000: max(100, 250) = 250
        assert_eq!(config.get_fee_bps(5000), 250);
        // At u=0: max(100, 0) = 100
        assert_eq!(config.get_fee_bps(0), 100);
    }

    #[test]
    fn test_disabled_curve_ignored() {
        let config = make_config([
            curve(0, 100),
            disabled(0, 9999),
            disabled(0, 0),
            disabled(0, 0),
        ]);
        assert_eq!(config.get_fee_bps(5000), 100);
        assert_eq!(config.get_fee_bps(10000), 100);
    }

    #[test]
    fn test_all_disabled_returns_zero() {
        let config = make_config([
            disabled(0, 100),
            disabled(0, 200),
            disabled(0, 0),
            disabled(0, 0),
        ]);
        assert_eq!(config.get_fee_bps(0), 0);
        assert_eq!(config.get_fee_bps(10000), 0);
    }

    #[test]
    fn test_default_pool_fee() {
        let config = make_config([
            curve(0, DEFAULT_POOL_FEE_BPS),
            disabled(0, 0),
            disabled(0, 0),
            disabled(0, 0),
        ]);
        assert_eq!(config.get_fee_bps(0), 100);
        assert_eq!(config.get_fee_bps(10000), 100);
    }
}
