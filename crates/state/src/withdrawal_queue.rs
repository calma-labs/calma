use anchor_lang::prelude::*;

pub const WITHDRAWAL_QUEUE_LEN: usize = 1024;

/// Most pending entries one authority may hold at a time.
///
/// Slots are the scarce resource: there are 1023 of them, they carry no rent,
/// and a full queue means no further exit can be queued at all. Without a cap,
/// one party could hold every slot. Splitting an exit across a handful of
/// entries is normal, holding hundreds is not, so the limit is set well above
/// ordinary use and well below the point where one authority crowds out the
/// rest.
pub const MAX_ENTRIES_PER_AUTHORITY: usize = 8;

/// A single pending withdrawal request.
///
/// Only the requester is recorded, not a destination account. `process_queue_entry`
/// takes the payout account as an argument and checks that its **owner** equals
/// `requester` — it does not derive an ATA, and any token account that requester
/// owns is accepted. That is sound (the tokens reach the right party either way)
/// but it is a weaker statement than "the destination is the requester's ATA",
/// which this doc used to claim as the reason for not storing one. Anything
/// added later that assumes a canonical destination has to store it.
#[zero_copy]
pub struct WithdrawalQueueEntry {
    /// The user authority who requested the withdrawal.
    pub requester: Pubkey,
    /// Amount of underlying tokens to withdraw.
    pub amount: u64,
    _reserved: u64,
}

/// Fixed-capacity circular-buffer queue of withdrawal requests embedded
/// directly in `Pool` account data.  Exposed as zero-copy to avoid copying
/// the ~40 KB array onto the stack during Borsh deserialization.
#[zero_copy]
pub struct WithdrawalQueue {
    pub head: u16,
    pub tail: u16,
    _pad: [u8; 4], // explicit padding so entries[] is 8-byte aligned (no implicit padding bytes)
    pub entries: [WithdrawalQueueEntry; WITHDRAWAL_QUEUE_LEN],
}

impl WithdrawalQueueEntry {
    pub fn new(requester: Pubkey, amount: u64) -> Self {
        Self {
            requester,
            amount,
            _reserved: 0,
        }
    }
}

impl WithdrawalQueue {
    fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    fn is_full(&self) -> bool {
        (self.tail as usize + 1) % WITHDRAWAL_QUEUE_LEN == self.head as usize
    }

    /// Append `entry` to the back of the queue.
    /// Returns `WithdrawalQueueFull` if the queue has no free slots.
    ///
    /// A zero-amount entry is refused. It would pay out nothing when processed,
    /// yet still consume one of the 1023 slots and — because a non-empty queue
    /// forces every later `withdraw_lent` onto the queued path — hold the
    /// immediate-withdrawal path shut for everyone. Slots are the scarce
    /// resource here, so an entry that cannot move any tokens must not take one.
    /// The caller-facing guard is in `withdraw_lent_handler`; this is the
    /// backstop that makes the invariant true for every push site.
    pub fn push(&mut self, entry: WithdrawalQueueEntry) -> Result<()> {
        require!(
            entry.amount > 0,
            crate::error::ErrorCode::ZeroValueWithdrawal
        );
        require!(
            !self.is_full(),
            crate::error::ErrorCode::WithdrawalQueueFull
        );
        self.entries[self.tail as usize] = entry;
        self.tail = ((self.tail as usize + 1) % WITHDRAWAL_QUEUE_LEN) as u16;
        Ok(())
    }

    /// Remove and return the front entry.
    /// Returns `WithdrawalQueueEmpty` if there are no pending entries.
    pub fn pop(&mut self) -> Result<WithdrawalQueueEntry> {
        require!(
            !self.is_empty(),
            crate::error::ErrorCode::WithdrawalQueueEmpty
        );
        let entry = self.entries[self.head as usize];
        self.head = ((self.head as usize + 1) % WITHDRAWAL_QUEUE_LEN) as u16;
        Ok(entry)
    }

    /// Pending entries belonging to `requester`, counted no further than
    /// `cap`.
    ///
    /// The early exit is what bounds the cost: the caller only needs to know
    /// whether the limit is already reached, so a party at the cap stops the
    /// scan after `cap` matches instead of walking the whole queue. The
    /// unavoidable case is a caller with *no* entries and a near-full queue,
    /// which walks every occupied slot — an equality test on 32 bytes per
    /// entry, and the queue is bounded at 1023.
    pub fn count_for_capped(&self, requester: &Pubkey, cap: usize) -> usize {
        let mut seen = 0usize;
        for entry in self.iter() {
            if entry.requester == *requester {
                seen += 1;
                if seen >= cap {
                    break;
                }
            }
        }
        seen
    }

    /// Number of entries currently in the queue.
    pub fn len(&self) -> usize {
        (self.tail as usize + WITHDRAWAL_QUEUE_LEN - self.head as usize) % WITHDRAWAL_QUEUE_LEN
    }

    /// Iterate over entries in FIFO order (head → tail).
    pub fn iter(&self) -> impl Iterator<Item = &WithdrawalQueueEntry> + '_ {
        let head = self.head as usize;
        let len = self.len();
        (0..len).map(move |i| &self.entries[(head + i) % WITHDRAWAL_QUEUE_LEN])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::prelude::Pubkey;

    fn make_queue() -> WithdrawalQueue {
        bytemuck::Zeroable::zeroed()
    }

    fn make_entry(seed: u8, amount: u64) -> WithdrawalQueueEntry {
        let mut bytes = [0u8; 32];
        bytes[0] = seed;
        WithdrawalQueueEntry::new(Pubkey::new_from_array(bytes), amount)
    }

    // ── basic state ───────────────────────────────────────────────────────────

    #[test]
    fn new_queue_is_empty() {
        let q = make_queue();
        assert!(q.is_empty());
        assert!(!q.is_full());
        assert_eq!(q.len(), 0);
    }

    // ── push / pop round-trip ─────────────────────────────────────────────────

    #[test]
    fn push_then_pop_returns_same_entry() {
        let mut q = make_queue();
        let entry = make_entry(1, 500);
        q.push(entry).unwrap();

        assert!(!q.is_empty());
        assert_eq!(q.len(), 1);

        let out = q.pop().unwrap();
        assert_eq!(out.requester, entry.requester);
        assert_eq!(out.amount, entry.amount);
        assert!(q.is_empty());
    }

    #[test]
    fn fifo_ordering_preserved() {
        let mut q = make_queue();
        for i in 1..=10u8 {
            q.push(make_entry(i, i as u64 * 100)).unwrap();
        }
        for i in 1..=10u8 {
            let out = q.pop().unwrap();
            assert_eq!(out.amount, i as u64 * 100);
        }
        assert!(q.is_empty());
    }

    #[test]
    fn count_for_capped_counts_only_the_named_requester() {
        let mut q = make_queue();
        for i in 0..5u8 {
            q.push(make_entry(1, i as u64 + 1)).unwrap(); // requester A
        }
        q.push(make_entry(2, 99)).unwrap(); // requester B

        let a = make_entry(1, 0).requester;
        let b = make_entry(2, 0).requester;
        let c = make_entry(3, 0).requester;
        assert_eq!(q.count_for_capped(&a, MAX_ENTRIES_PER_AUTHORITY), 5);
        assert_eq!(q.count_for_capped(&b, MAX_ENTRIES_PER_AUTHORITY), 1);
        assert_eq!(q.count_for_capped(&c, MAX_ENTRIES_PER_AUTHORITY), 0);
    }

    #[test]
    fn count_for_capped_stops_at_the_cap() {
        // The early exit is the cost bound: a party already at the limit must
        // not force a walk of the whole queue to discover it.
        let mut q = make_queue();
        for i in 0..50u8 {
            q.push(make_entry(1, i as u64 + 1)).unwrap();
        }
        let a = make_entry(1, 0).requester;
        assert_eq!(q.count_for_capped(&a, 3), 3);
        assert_eq!(q.count_for_capped(&a, MAX_ENTRIES_PER_AUTHORITY), MAX_ENTRIES_PER_AUTHORITY);
    }

    #[test]
    fn zero_amount_entry_is_refused() {
        // A zero-amount entry pays out nothing but still burns a queue slot and
        // keeps the queue non-empty, which forces every later withdrawal onto
        // the queued path. Refusing it is what keeps slot-spam from being free.
        let mut q = make_queue();
        let err = q.push(make_entry(1, 0)).err().expect("expected error");
        assert_eq!(
            err,
            anchor_lang::error!(crate::error::ErrorCode::ZeroValueWithdrawal)
        );
        assert!(q.is_empty());
    }

    // ── error paths ───────────────────────────────────────────────────────────

    #[test]
    fn pop_empty_returns_error() {
        let mut q = make_queue();
        let err = q.pop().err().expect("expected error");
        assert_eq!(
            err,
            anchor_lang::error!(crate::error::ErrorCode::WithdrawalQueueEmpty)
        );
    }

    #[test]
    fn push_full_returns_error() {
        let mut q = make_queue();
        // The queue holds WITHDRAWAL_QUEUE_LEN - 1 items at most (one slot reserved).
        for i in 0..(WITHDRAWAL_QUEUE_LEN - 1) {
            q.push(make_entry(0, i as u64 + 1)).unwrap();
        }
        assert!(q.is_full());

        let err = q.push(make_entry(0, 9999)).err().expect("expected error");
        assert_eq!(
            err,
            anchor_lang::error!(crate::error::ErrorCode::WithdrawalQueueFull)
        );
    }

    // ── circular wrap-around ──────────────────────────────────────────────────

    #[test]
    fn circular_wraparound() {
        let mut q = make_queue();
        // Fill the queue halfway, then drain it, then fill again past the
        // original end to exercise the modular index wrap.
        let half = WITHDRAWAL_QUEUE_LEN / 2;
        for i in 0..half {
            q.push(make_entry(0, i as u64 + 1)).unwrap();
        }
        for _ in 0..half {
            q.pop().unwrap();
        }
        // head and tail are now both at `half`; push past WITHDRAWAL_QUEUE_LEN boundary
        for i in 0..half {
            q.push(make_entry(1, i as u64 + 1000)).unwrap();
        }
        assert_eq!(q.len(), half);
        for i in 0..half {
            let out = q.pop().unwrap();
            assert_eq!(out.amount, i as u64 + 1000);
        }
        assert!(q.is_empty());
    }

    // ── len tracking ─────────────────────────────────────────────────────────

    #[test]
    fn len_tracks_pushes_and_pops() {
        let mut q = make_queue();
        assert_eq!(q.len(), 0);
        q.push(make_entry(0, 1)).unwrap();
        assert_eq!(q.len(), 1);
        q.push(make_entry(1, 2)).unwrap();
        assert_eq!(q.len(), 2);
        q.pop().unwrap();
        assert_eq!(q.len(), 1);
        q.pop().unwrap();
        assert_eq!(q.len(), 0);
    }
}
