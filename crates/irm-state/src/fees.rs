use anchor_lang::prelude::*;

/// Maximum number of rate points a curve may hold.
pub const MAX_POINTS: usize = 4;
/// Minimum number of rate points a curve must hold.
pub const MIN_POINTS: usize = 2;
/// Maximum borrow rate any single curve point may specify: 50% APY.
///
/// Bounds what an IRM authority can do to outstanding debt. The curve
/// extrapolates past its last point, so `get_fee_bps` can still exceed this;
/// `math::compute_interest` clamps that at `MAX_RATE_BPS`, and this keeps the
/// configured points well inside that ceiling.
pub const MAX_RATE_BPS: u32 = 5_000;

/// Why a proposed rate curve was rejected.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum RatePointError {
    /// Wrong number of points, first point not at zero utilization, or
    /// utilizations not strictly increasing.
    InvalidPointList,
    /// A point's rate exceeds [`MAX_RATE_BPS`].
    RateTooHigh,
}

/// Validate a proposed curve before it is written.
///
/// The reader ([`PiecewiseLinearModel::get_fee_bps`]) trusts these invariants
/// and performs no runtime checks, so they must hold at every write site. Living
/// here rather than in the program keeps the guarantee next to the code that
/// depends on it.
pub fn validate_rate_points(
    points: &[(u16, u32)],
) -> core::result::Result<(), RatePointError> {
    if points.len() < MIN_POINTS || points.len() > MAX_POINTS {
        return Err(RatePointError::InvalidPointList);
    }
    if points[0].0 != 0 {
        return Err(RatePointError::InvalidPointList);
    }
    for i in 1..points.len() {
        if points[i].0 <= points[i - 1].0 {
            return Err(RatePointError::InvalidPointList);
        }
    }
    for p in points {
        if p.1 > MAX_RATE_BPS {
            return Err(RatePointError::RateTooHigh);
        }
    }
    Ok(())
}

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
/// Rate above `points[len-1].util_bps` extrapolates the last segment's slope,
/// clamped to [`MAX_RATE_BPS`] — see [`Self::get_fee_bps`].
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
    /// Borrow rate at `utilization_bps`, never above [`MAX_RATE_BPS`].
    ///
    /// # Why the result is clamped, not just the points
    ///
    /// `validate_rate_points` caps each configured *point* at `MAX_RATE_BPS`,
    /// but the curve extrapolates past its last point with the final segment's
    /// slope — and the slope is set by the *gap* between the last two
    /// utilizations, which nothing bounds. `[(0, 0), (1, 5_000)]` passes
    /// validation with both points inside the cap, yet rises 5_000 bps for every
    /// 1 bp of utilization: at 100% it evaluates to 50_000_000 bps, ten thousand
    /// times the advertised ceiling. `math::compute_interest` caught that at
    /// `math::MAX_RATE_BPS` (10_000% APR), which is a backstop against overflow,
    /// not a risk limit — it still charges ~27% of principal per day.
    ///
    /// Clamping the output makes `MAX_RATE_BPS` the real ceiling for every
    /// reachable utilization, so a curve's shape can no longer smuggle a rate
    /// past the bound its points were checked against.
    pub fn get_fee_bps(&self, utilization_bps: u64) -> u32 {
        let u = utilization_bps as i128;
        let len = self.len as usize;
        for i in 1..len {
            let u_hi = self.points[i].util_bps as i128;
            if u < u_hi {
                let u_lo = self.points[i - 1].util_bps as i128;
                let r_lo = self.points[i - 1].rate_bps as i128;
                let r_hi = self.points[i].rate_bps as i128;
                return clamp_rate(r_lo + (r_hi - r_lo) * (u - u_lo) / (u_hi - u_lo));
            }
        }
        let u_hi = self.points[len - 1].util_bps as i128;
        let u_lo = self.points[len - 2].util_bps as i128;
        let r_hi = self.points[len - 1].rate_bps as i128;
        let r_lo = self.points[len - 2].rate_bps as i128;
        clamp_rate(r_hi + (r_hi - r_lo) * (u - u_hi) / (u_hi - u_lo))
    }
}

/// Clamp an interpolated rate into `0..=MAX_RATE_BPS`.
#[inline]
fn clamp_rate(v: i128) -> u32 {
    if v <= 0 {
        0
    } else if v >= MAX_RATE_BPS as i128 {
        MAX_RATE_BPS
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
    fn never_exceeds_max_rate_however_steep_the_curve() {
        // A curve whose points are individually within the cap but whose last
        // segment is near-vertical: 5_000 bps of rate across 1 bp of utilization.
        // Both points validate, so this is a curve an IRM authority can actually
        // set — the clamp is what stops it reaching 50_000_000 bps at full
        // utilization.
        let steep = model(&[(0, 0), (1, MAX_RATE_BPS)]);
        assert_eq!(validate_rate_points(&[(0, 0), (1, MAX_RATE_BPS)]), Ok(()));
        assert_eq!(steep.get_fee_bps(10_000), MAX_RATE_BPS);
        assert_eq!(steep.get_fee_bps(1_000_000), MAX_RATE_BPS);

        // Points beyond the cap can't be written by the program, but the reader
        // must stay bounded even if one somehow existed.
        let m = model(&[(0, u32::MAX - 10), (10_000, u32::MAX)]);
        assert_eq!(m.get_fee_bps(100_000), MAX_RATE_BPS);
    }

}

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn accepts_a_well_formed_curve() {
        assert_eq!(validate_rate_points(&[(0, 0), (10_000, 500)]), Ok(()));
    }

    #[test]
    fn rejects_wrong_point_count() {
        assert_eq!(
            validate_rate_points(&[(0, 0)]),
            Err(RatePointError::InvalidPointList)
        );
        assert_eq!(
            validate_rate_points(&[(0, 0), (1, 1), (2, 2), (3, 3), (4, 4)]),
            Err(RatePointError::InvalidPointList)
        );
    }

    #[test]
    fn rejects_curve_not_anchored_at_zero_utilization() {
        assert_eq!(
            validate_rate_points(&[(1, 0), (10_000, 500)]),
            Err(RatePointError::InvalidPointList)
        );
    }

    #[test]
    fn rejects_non_increasing_utilization() {
        assert_eq!(
            validate_rate_points(&[(0, 0), (5_000, 100), (5_000, 200)]),
            Err(RatePointError::InvalidPointList)
        );
    }

    #[test]
    fn rejects_a_rate_above_the_ceiling() {
        assert_eq!(
            validate_rate_points(&[(0, 0), (10_000, MAX_RATE_BPS + 1)]),
            Err(RatePointError::RateTooHigh)
        );
        assert_eq!(validate_rate_points(&[(0, 0), (10_000, MAX_RATE_BPS)]), Ok(()));
    }
}

