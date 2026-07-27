use anchor_lang::prelude::*;

/// Maximum number of rate points a curve may hold.
pub const MAX_POINTS: usize = 4;
/// Minimum number of rate points a curve must hold.
pub const MIN_POINTS: usize = 2;

/// One `(utilization, rate)` corner of the piecewise-linear curve.
///
/// Field order is chosen so `#[repr(C)]` inserts no implicit padding —
/// `zero_copy` / bytemuck::Pod requires an explicit layout.
#[zero_copy]
#[derive(Debug, Default)]
pub struct RatePoint {
    pub rate_bps: u32,
    pub util_bps: u16,
    pub _pad: [u8; 2],
}

impl RatePoint {
    pub const fn new(util_bps: u16, rate_bps: u32) -> Self {
        Self {
            rate_bps,
            util_bps,
            _pad: [0; 2],
        }
    }
}

/// Piecewise-linear IRM defined by 2–4 rate points.
///
/// Invariants (enforced at write time in the `irm` program and in
/// [`crate::state`] constructors; **trusted** here — the reader performs no
/// checks so hot on-chain paths stay CU-cheap):
///   * `MIN_POINTS <= len as usize <= MAX_POINTS`
///   * `points[0].util_bps == 0`
///   * `points[0..len].util_bps` is strictly increasing
///
/// Rate above `points[len-1].util_bps` extrapolates the last segment's slope.
#[zero_copy]
#[derive(Debug)]
pub struct PiecewiseLinearModel {
    pub points: [RatePoint; MAX_POINTS],
    pub len: u8,
    pub _pad: [u8; 7],
}

impl math::FeeModel for PiecewiseLinearModel {
    fn fee_bps(&self, utilization_bps: u64) -> u32 {
        self.get_fee_bps(utilization_bps)
    }
}

impl PiecewiseLinearModel {
    pub fn get_fee_bps(&self, utilization_bps: u64) -> u32 {
        let u = utilization_bps as i128;
        let len = self.len as usize;
        for i in 1..len {
            let u_hi = self.points[i].util_bps as i128;
            if u < u_hi {
                let u_lo = self.points[i - 1].util_bps as i128;
                let r_lo = self.points[i - 1].rate_bps as i128;
                let r_hi = self.points[i].rate_bps as i128;
                return clamp_u32(r_lo + (r_hi - r_lo) * (u - u_lo) / (u_hi - u_lo));
            }
        }
        let u_hi = self.points[len - 1].util_bps as i128;
        let u_lo = self.points[len - 2].util_bps as i128;
        let r_hi = self.points[len - 1].rate_bps as i128;
        let r_lo = self.points[len - 2].rate_bps as i128;
        clamp_u32(r_hi + (r_hi - r_lo) * (u - u_hi) / (u_hi - u_lo))
    }
}

#[inline]
fn clamp_u32(v: i128) -> u32 {
    if v <= 0 {
        0
    } else if v >= u32::MAX as i128 {
        u32::MAX
    } else {
        v as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(pts: &[(u16, u32)]) -> PiecewiseLinearModel {
        assert!(pts.len() >= MIN_POINTS && pts.len() <= MAX_POINTS);
        let mut points = [RatePoint::default(); MAX_POINTS];
        for (i, (u, r)) in pts.iter().enumerate() {
            points[i] = RatePoint::new(*u, *r);
        }
        PiecewiseLinearModel {
            points,
            len: pts.len() as u8,
            _pad: [0; 7],
        }
    }

    #[test]
    fn zero_utilization_returns_first_rate() {
        assert_eq!(model(&[(0, 250), (10_000, 750)]).get_fee_bps(0), 250);
    }

    #[test]
    fn two_point_linear_interpolation() {
        let m = model(&[(0, 0), (10_000, 1_000)]);
        assert_eq!(m.get_fee_bps(0), 0);
        assert_eq!(m.get_fee_bps(2_500), 250);
        assert_eq!(m.get_fee_bps(5_000), 500);
        assert_eq!(m.get_fee_bps(10_000), 1_000);
    }

    #[test]
    fn three_point_default_curve() {
        let m = model(&[(0, 0), (9_500, 428), (10_000, 828)]);
        assert_eq!(m.get_fee_bps(0), 0);
        // half-way to kink: 428 * 4750 / 9500 = 214
        assert_eq!(m.get_fee_bps(4_750), 214);
        assert_eq!(m.get_fee_bps(9_500), 428);
        // half-way between kink and 100%: (428 + 828) / 2 = 628
        assert_eq!(m.get_fee_bps(9_750), 628);
        assert_eq!(m.get_fee_bps(10_000), 828);
    }

    #[test]
    fn four_point_curve() {
        let m = model(&[(0, 100), (2_500, 300), (7_500, 500), (10_000, 1_200)]);
        assert_eq!(m.get_fee_bps(0), 100);
        assert_eq!(m.get_fee_bps(2_500), 300);
        assert_eq!(m.get_fee_bps(7_500), 500);
        assert_eq!(m.get_fee_bps(10_000), 1_200);
        // midpoint of last segment: (500 + 1_200) / 2 = 850
        assert_eq!(m.get_fee_bps(8_750), 850);
    }

    #[test]
    fn extrapolates_above_last_point() {
        let m = model(&[(0, 0), (9_500, 428), (10_000, 828)]);
        // slope of last segment: (828 - 428) / 500 = 0.8 bps per bp of util
        assert_eq!(m.get_fee_bps(10_500), 1_228);
        assert_eq!(m.get_fee_bps(11_000), 1_628);
    }

    #[test]
    fn saturates_at_u32_max() {
        let m = model(&[(0, u32::MAX - 10), (10_000, u32::MAX)]);
        assert_eq!(m.get_fee_bps(100_000), u32::MAX);
    }

}
