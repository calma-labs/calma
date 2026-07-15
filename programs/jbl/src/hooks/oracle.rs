use anchor_lang::prelude::*;

pub struct OracleState {
    pub current_ts: i64,
    pub price: u64,
}

impl math::Oracle for OracleState {
    fn price(&self) -> u64 {
        self.price
    }
}

impl OracleState {
    /// CPI into the feed program to read the current price.
    ///
    /// `max_age_secs` is the staleness window configured on the pool; comes from
    /// `Pool::max_feed_age_secs` at borrow/withdraw time, or from the create
    /// instruction's parameter when first initialising a pool.
    pub fn new<'info>(
        feed_program: AccountInfo<'info>,
        feed_state: AccountInfo<'info>,
        max_age_secs: u32,
    ) -> Result<Self> {
        let current_ts = Clock::get()?.unix_timestamp;
        let snap = feed::cpi::get_state(CpiContext::new(
            feed_program.key(),
            feed::cpi::accounts::GetValue { feed: feed_state },
        ))?
        .get();
        require!(
            !state::Pool::snapshot_stale_at(snap.last_updated_ts, current_ts, max_age_secs),
            crate::error::JblError::StaleOracle
        );
        Ok(Self {
            current_ts,
            price: snap.ratio,
        })
    }
}
