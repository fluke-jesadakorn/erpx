//! Native Customer 360 customer directory read facts.
//!
//! Customer 360 owns the bounded read projection only. The directory lists the
//! customer parties visible through the same Company/branch/department scope
//! and source gates as the federated timeline; it never writes or revises a
//! source record.
use crate::domain::timeline::CustomerTimelineActor;
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::Serialize;

pub const MAX_DIRECTORY: usize = 201;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomerDirectoryView {
    pub field_service: bool,
    pub finance: bool,
    pub legal: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CustomerDirectoryEntry {
    pub id: String,
    pub name: String,
    pub code: String,
}

impl CustomerDirectoryEntry {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if RecordId::new(self.id.clone()).is_err()
            || self.name.is_empty()
            || self.name.len() > 300
            || self.code.is_empty()
            || self.code.len() > 200
            || self.name.chars().any(char::is_control)
            || self.code.chars().any(char::is_control)
        {
            return Err(BusinessError::invalid(
                "CUSTOMER_DIRECTORY_ENTRY_INVALID",
                "Customer directory entries must be complete and scoped.",
            ));
        }
        Ok(())
    }
}

pub(crate) fn directory_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_DIRECTORY_SOURCE_INVALID",
        "The current customer directory source is invalid.",
    )
}

impl CustomerTimelineActor {
    pub fn require_directory_read(
        &self,
        now: erpx_server_shared::public::TimestampMillis,
    ) -> Result<(), BusinessError> {
        if !self.has("commerce.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "CUSTOMER_DIRECTORY_PERMISSION_REQUIRED",
                "Your selected role does not grant customer read access.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }
}

pub fn directory_view(actor: &CustomerTimelineActor) -> CustomerDirectoryView {
    CustomerDirectoryView {
        field_service: actor.field_service_visible(),
        finance: actor.finance_visible(),
        legal: actor.legal_visible(),
    }
}
