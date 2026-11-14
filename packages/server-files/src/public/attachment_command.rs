//! Public generic Files attachment command contract.
//!
//! Composition owns the current Account/Holding/Company authority and the
//! owning-record check; this package owns the bounded Files link lifecycle,
//! idempotent replay receipt and withdrawal audit.
pub use crate::adapters::attachment_command_postgres::withdraw_attachment;
pub use crate::application::attachment_command::{
    AttachmentWithdrawAuthority, NormalizedWithdrawal, WithdrawAttachmentRequest,
    WithdrawAttachmentResult, WithdrawTarget, WithdrawalFacts, normalize_withdrawal,
    prepare_withdrawal,
};
pub use crate::domain::attachment_command::{
    MAX_COMMAND_KEY_BYTES, MAX_WITHDRAWAL_REASON_UNITS, MIN_COMMAND_KEY_BYTES,
    MIN_WITHDRAWAL_REASON_UNITS, WithdrawOwnerAccess, WithdrawalDecision, has_native_owner_command,
    normalize_withdrawal_reason, valid_command_key, withdraw_owner_access, withdrawal_decision,
    withdrawal_event_id,
};
