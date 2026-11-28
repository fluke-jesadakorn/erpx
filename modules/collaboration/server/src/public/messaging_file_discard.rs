//! Public Messaging discard command contract.
//!
//! Composition owns the current Account/Holding/Company authority, the
//! purchase admission and the Files withdraw port; this package owns the
//! conversation guard, the idempotent receipt and the file-discard policy.
pub use crate::adapters::messaging_file_discard::{
    DISCARD_OPERATION, DiscardReceipt, MessagingFileDiscardFuture, MessagingFileDiscardPort,
    MessagingFileWithdrawRequest, discard_file,
};
pub use crate::domain::messaging_file_discard::{
    MAX_DISCARD_ID_BYTES, MAX_DISCARD_REASON_UNITS, MIN_DISCARD_REASON_UNITS,
    normalize_discard_reason,
};
