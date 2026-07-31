use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::IdentityError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRecord {
    id: Uuid,
    principal_ref: String,
    label: String,
    created_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

impl DeviceRecord {
    pub fn new(
        principal_ref: impl Into<String>,
        label: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<Self, IdentityError> {
        let principal_ref = principal_ref.into();
        if principal_ref.trim().is_empty() {
            return Err(IdentityError::InvalidIdentityReference);
        }
        Ok(Self {
            id: Uuid::new_v4(),
            principal_ref,
            label: label.into(),
            created_at: at,
            last_seen_at: at,
            revoked_at: None,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn principal_ref(&self) -> &str {
        &self.principal_ref
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn last_seen_at(&self) -> DateTime<Utc> {
        self.last_seen_at
    }
    pub fn revoked_at(&self) -> Option<DateTime<Utc>> {
        self.revoked_at
    }

    pub fn touch(&mut self, at: DateTime<Utc>) {
        self.last_seen_at = at;
    }

    pub fn revoke(&mut self, at: DateTime<Utc>) {
        self.revoked_at = Some(at);
    }

    pub fn is_active_at(&self, at: DateTime<Utc>) -> bool {
        self.revoked_at.is_none_or(|revoked_at| at < revoked_at)
    }
}
