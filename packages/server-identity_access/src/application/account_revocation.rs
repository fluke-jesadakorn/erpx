use super::account_ports::AccountFuture;
use crate::domain::account_revocation::{AccountRevocationTarget, RetiredAccountChild};

/// Required by every native child-deleting Account command. A successful call
/// means all current Holding fences committed, not that Control has committed.
/// Implementations must be idempotent: an eventual Control rollback cannot undo
/// a prior Holding tombstone, and must never cause an acknowledgement or grant.
pub trait AccountWorkContextInvalidation: Send + Sync {
    fn invalidate<'a>(
        &'a self,
        targets: &'a [AccountRevocationTarget],
        children: &'a [RetiredAccountChild],
    ) -> AccountFuture<'a, ()>;
}
