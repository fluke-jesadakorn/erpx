//! Application use case for native Billing owner writes.
//!
//! The owner boundary resolves the typed kind and builds the authoritative
//! owner statement. The composition adapter owns session/authority; the TS
//! caller owns the serializable unit in which the returned statement and the
//! receipt/audit/outbox effects are executed together.

use crate::adapters::billing_owner_write::{
    BillingOwnerStatement, BillingOwnerWriteInput, billing_owner_statement,
};
use crate::domain::billing_owner_write::OwnerWriteKind;
use erpx_platform_shared_server::public::{BusinessError, FailureKind, HoldingId};

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "BILLING_OWNER_WRITE_INVALID",
        "Review the Billing owner write request.",
    )
}

/// Resolve a typed owner write kind to its owner-authored statement.
pub fn build_owner_write(
    holding: &HoldingId,
    command: &BillingOwnerWriteInput,
) -> Result<BillingOwnerStatement, BusinessError> {
    // Reject unknown kinds at the use-case boundary before any statement is
    // built; the adapter repeats the check as defence in depth.
    OwnerWriteKind::parse(&command.kind).map_err(|_| invalid())?;
    if holding.as_str() == "h003" {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ERPX_NOT_CUSTOMER",
            "ERPX is not a customer billing account.",
        ));
    }
    billing_owner_statement(holding, command)
}
