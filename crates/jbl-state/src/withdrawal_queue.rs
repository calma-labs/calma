use anchor_lang::prelude::*;

pub const WITHDRAWAL_QUEUE_LEN: usize = 1024;

/// A single pending withdrawal request.
/// `user_position` and destination ATA are derived from `requester` at
/// execution time so they do not need to be stored here.
#[zero_copy]
pub struct WithdrawalQueueEntry {
    /// The user authority who requested the withdrawal.
    pub requester: Pubkey,
    /// Amount of underlying tokens to withdraw.
    pub amount: u64,
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

impl WithdrawalQueue {
    fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    fn is_full(&self) -> bool {
        (self.tail as usize + 1) % WITHDRAWAL_QUEUE_LEN == self.head as usize
    }

    /// Append `entry` to the back of the queue.
    /// Returns `WithdrawalQueueFull` if the queue has no free slots.
    pub fn push(&mut self, entry: WithdrawalQueueEntry) -> Result<()> {
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

    /// Number of entries currently in the queue.
    pub fn len(&self) -> usize {
        (self.tail as usize + WITHDRAWAL_QUEUE_LEN - self.head as usize) % WITHDRAWAL_QUEUE_LEN
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
        WithdrawalQueueEntry {
            requester: Pubkey::new_from_array(bytes),
            amount,
        }
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
        for i in 0..10u8 {
            q.push(make_entry(i, i as u64 * 100)).unwrap();
        }
        for i in 0..10u8 {
            let out = q.pop().unwrap();
            assert_eq!(out.amount, i as u64 * 100);
        }
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
            q.push(make_entry(0, i as u64)).unwrap();
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
            q.push(make_entry(0, i as u64)).unwrap();
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
