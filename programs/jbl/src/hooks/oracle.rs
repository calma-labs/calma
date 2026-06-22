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
    pub fn new<'info>(
        feed_program: AccountInfo<'info>,
        feed_state: AccountInfo<'info>,
    ) -> Result<Self> {
        let current_ts = Clock::get()?.unix_timestamp;
        let price = feed::cpi::get_value(CpiContext::new(
            feed_program.key(),
            feed::cpi::accounts::GetValue { feed: feed_state },
        ))?
        .get();
        Ok(Self { current_ts, price })
    }
}
