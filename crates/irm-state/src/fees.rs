use anchor_lang::prelude::*;

/// Default flat fee applied to all new pools: 1 % APY (100 bps).
pub const DEFAULT_POOL_FEE_BPS: i64 = 100;

/// A kinked rate curve:
///   - below kink: f(u) = a·(u/10_000) + b
///   - above kink: f(kink) + a2·((u − kink)/10_000)
///
/// When `kink == 0` the curve is purely linear (only `a` and `b` matter).
/// The curve is only active when `enabled != 0`.
#[zero_copy]
#[derive(Debug, Default)]
pub struct LinearSegment {
    pub a: i64,
    pub b: i64,
    /// Post-kink slope in basis points. Ignored when `kink == 0`.
    pub a2: i64,
    /// Kink point in utilization basis points (0..=10_000). 0 = no kink.
    pub kink: u64,
    /// Non-zero → curve is active; 0 → disabled.
    pub enabled: u8,
    pub _pad: [u8; 7],
}

impl LinearSegment {
    fn eval(&self, utilization_bps: u64) -> i128 {
        let u = utilization_bps as i128;
        let kink = self.kink as i128;
        if self.kink == 0 || u <= kink {
            let slope = (self.a as i128).saturating_mul(u) / 10_000i128;
            (self.b as i128).saturating_add(slope)
        } else {
            let at_kink = (self.a as i128).saturating_mul(kink) / 10_000i128;
            let base_at_kink = (self.b as i128).saturating_add(at_kink);
            let excess = u.saturating_sub(kink);
            let extra_slope = (self.a2 as i128).saturating_mul(excess) / 10_000i128;
            base_at_kink.saturating_add(extra_slope)
        }
    }
}

/// Four-curve linear interest rate model.
/// Effective rate = max(enabled curves), clamped to ≥ 0.
#[zero_copy]
#[derive(Debug)]
pub struct PiecewiseLinearModel {
    pub curves: [LinearSegment; 4],
}

impl math::FeeModel for PiecewiseLinearModel {
    fn fee_bps(&self, utilization_bps: u64) -> u32 {
        self.get_fee_bps(utilization_bps)
    }
}

impl PiecewiseLinearModel {
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

    fn curve(a: i64, b: i64) -> LinearSegment {
        LinearSegment {
            a,
            b,
            a2: 0,
            kink: 0,
            enabled: 1,
            _pad: [0; 7],
        }
    }

    fn kinked(a: i64, b: i64, kink: u64, a2: i64) -> LinearSegment {
        LinearSegment {
            a,
            b,
            a2,
            kink,
            enabled: 1,
            _pad: [0; 7],
        }
    }

    fn disabled(a: i64, b: i64) -> LinearSegment {
        LinearSegment {
            a,
            b,
            a2: 0,
            kink: 0,
            enabled: 0,
            _pad: [0; 7],
        }
    }

    fn make_config(curves: [LinearSegment; 4]) -> PiecewiseLinearModel {
        PiecewiseLinearModel { curves }
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
        let config = make_config([curve(0, 100), curve(500, 0), disabled(0, 0), disabled(0, 0)]);
        assert_eq!(config.get_fee_bps(5000), 250);
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

    #[test]
    fn test_kink_below_kink_uses_a() {
        let config = make_config([
            kinked(200, 0, 8000, 2000),
            disabled(0, 0),
            disabled(0, 0),
            disabled(0, 0),
        ]);
        assert_eq!(config.get_fee_bps(4000), 80);
    }

    #[test]
    fn test_kink_at_kink_point() {
        let config = make_config([
            kinked(200, 0, 8000, 2000),
            disabled(0, 0),
            disabled(0, 0),
            disabled(0, 0),
        ]);
        assert_eq!(config.get_fee_bps(8000), 160);
    }

    #[test]
    fn test_kink_above_kink_uses_a2() {
        let config = make_config([
            kinked(200, 0, 8000, 2000),
            disabled(0, 0),
            disabled(0, 0),
            disabled(0, 0),
        ]);
        assert_eq!(config.get_fee_bps(9000), 360);
    }

    #[test]
    fn test_kink_zero_treated_as_linear() {
        let kinked_config = make_config([
            kinked(500, 100, 0, 9999),
            disabled(0, 0),
            disabled(0, 0),
            disabled(0, 0),
        ]);
        let linear_config = make_config([
            curve(500, 100),
            disabled(0, 0),
            disabled(0, 0),
            disabled(0, 0),
        ]);
        assert_eq!(kinked_config.get_fee_bps(0), linear_config.get_fee_bps(0));
        assert_eq!(
            kinked_config.get_fee_bps(5000),
            linear_config.get_fee_bps(5000)
        );
        assert_eq!(
            kinked_config.get_fee_bps(10000),
            linear_config.get_fee_bps(10000)
        );
    }

    #[test]
    fn test_kink_with_base_rate() {
        let config = make_config([
            kinked(100, 200, 5000, 1000),
            disabled(0, 0),
            disabled(0, 0),
            disabled(0, 0),
        ]);
        assert_eq!(config.get_fee_bps(3000), 230);
        assert_eq!(config.get_fee_bps(5000), 250);
        assert_eq!(config.get_fee_bps(7500), 500);
    }
}
