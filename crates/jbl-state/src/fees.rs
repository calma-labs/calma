use anchor_lang::prelude::*;

#[zero_copy]
#[derive(Debug)]
pub struct UtilizationFeeConfig {
    pub m1: u64,
    pub c1: i64,
    pub m2: u64,
    pub c2: i64,
}

impl UtilizationFeeConfig {
    pub fn get_fee_bps(&self, utilization_bps: u64) -> u32 {
        // y = (m * x) / 10,000 + c
        // utilization_bps is 0..10,000
        // Use i128 for calculations to handle negative intercepts
        let y1_i128 = (self.m1 as i128)
            .saturating_mul(utilization_bps as i128)
            .checked_div(10_000)
            .unwrap_or(0)
            .saturating_add(self.c1 as i128);

        let y2_i128 = (self.m2 as i128)
            .saturating_mul(utilization_bps as i128)
            .checked_div(10_000)
            .unwrap_or(0)
            .saturating_add(self.c2 as i128);

        // Rate should always be positive, use max of the two curves and ensure non-negative
        let max_y = y1_i128.max(y2_i128);
        let positive_y = max_y.max(0);

        // Use try_from to safely convert to u32
        u32::try_from(positive_y).unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flat_fee() {
        let config = UtilizationFeeConfig {
            m1: 0,
            c1: 500, // 5% flat
            m2: 0,
            c2: 0,
        };
        assert_eq!(config.get_fee_bps(0), 500);
        assert_eq!(config.get_fee_bps(5000), 500);
        assert_eq!(config.get_fee_bps(10000), 500);
    }

    #[test]
    fn test_sloped_fee() {
        let config = UtilizationFeeConfig {
            m1: 1000, // +10% slope
            c1: 200,  // 2% base
            m2: 0,
            c2: 0,
        };
        // 0% util -> 2% (200 bps)
        assert_eq!(config.get_fee_bps(0), 200);
        // 50% util -> 2% + 0.5 * 10% = 7% (700 bps)
        assert_eq!(config.get_fee_bps(5000), 700);
        // 100% util -> 2% + 10% = 12% (1200 bps)
        assert_eq!(config.get_fee_bps(10000), 1200);
    }

    #[test]
    fn test_kink_fee() {
        // Typical model: flat until 80%, then steep slope
        // Line 1: y = 200 (2% flat)
        // Line 2: y = 10000 * (util - 80%) / 20% + 200  => steep slope after 80%
        // To make Line 2 cross 200 at 80% (8000 bps):
        // m2 * 8000 / 10000 + c2 = 200
        // Let's say we want 30% fee at 100% util (10000 bps)
        // m2 * 10000 / 10000 + c2 = 3000
        // m2 + c2 = 3000
        // 0.8 * m2 + c2 = 200
        // 0.2 * m2 = 2800 => m2 = 14000
        // c2 = 3000 - 14000 = -11000 (now supported with i64!)
        
        // Test with negative intercept c2
        let config_negative = UtilizationFeeConfig {
            m1: 0,
            c1: 200,     // 2% floor
            m2: 14000,   // Steep slope
            c2: -11000,  // Negative intercept (now supported!)
        };
        // At 80% util: y1=200, y2=14000 * 0.8 - 11000 = 11200 - 11000 = 200. Max=200
        assert_eq!(config_negative.get_fee_bps(8000), 200);
        // At 100% util: y1=200, y2=14000 * 1.0 - 11000 = 3000. Max=3000
        assert_eq!(config_negative.get_fee_bps(10000), 3000);
        
        // Alternatively, just pick two lines that intersect at 80%
        // Line 1: m1=0, c1=200 (2% flat)
        // Line 2: m2=20000, c2=0. At 80% it is 20000 * 0.8 = 16000 (way above 200)
        
        // Let's use the provided logic: max(y1, y2)
        let config = UtilizationFeeConfig {
            m1: 0,
            c1: 200,    // 2% floor
            m2: 20000,  // Steep slope
            c2: 0,
        };
        // At 1% util: y1=200, y2=200. Max=200
        assert_eq!(config.get_fee_bps(100), 200);
        // At 10% util: y1=200, y2=20000 * 0.1 = 2000. Max=2000
        assert_eq!(config.get_fee_bps(1000), 2000);
        
        // Example from instructions: m1=0, c1=200, m2=1000, c2=0
        let config2 = UtilizationFeeConfig {
            m1: 0,
            c1: 200,
            m2: 1000,
            c2: 0,
        };
        // util=10%: y1=200, y2=100. Max=200
        assert_eq!(config2.get_fee_bps(1000), 200);
        // util=30%: y1=200, y2=300. Max=300
        assert_eq!(config2.get_fee_bps(3000), 300);
    }

    #[test]
    fn test_negative_intercept() {
        // Test negative c1 (low intercept)
        let config = UtilizationFeeConfig {
            m1: 1000,  // 10% slope
            c1: -500,  // -5% base (negative!)
            m2: 0,
            c2: 0,
        };
        // At 10% util: y = 1000 * 0.1 - 500 = 100 - 500 = -400, but clamped to 0
        assert_eq!(config.get_fee_bps(1000), 0);
        // At 50% util: y = 1000 * 0.5 - 500 = 500 - 500 = 0
        assert_eq!(config.get_fee_bps(5000), 0);
        // At 60% util: y = 1000 * 0.6 - 500 = 600 - 500 = 100
        assert_eq!(config.get_fee_bps(6000), 100);

        // Test both negative intercepts
        let config2 = UtilizationFeeConfig {
            m1: 0,
            c1: -100,  // negative
            m2: 5000,  // 50% slope
            c2: -2000, // negative
        };
        // At 0% util: both negative, clamped to 0
        assert_eq!(config2.get_fee_bps(0), 0);
        // At 50% util: y1=-100, y2=5000*0.5-2000=500. Max=500
        assert_eq!(config2.get_fee_bps(5000), 500);
    }
}
