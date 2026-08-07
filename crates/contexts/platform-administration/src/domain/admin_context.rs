use chrono::{DateTime, Duration, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{DomainEvent, PlatformAdministrationDomainError, PlatformTenantId, bounded};

const MAX_REASON_BYTES: usize = 512;
const MAX_PRINCIPAL_BYTES: usize = 256;
pub const MAX_ADMIN_CONTEXT_TTL: Duration = Duration::minutes(30);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AdminContextId(Uuid);

impl std::fmt::Debug for AdminContextId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AdminContextId(<redacted>)")
    }
}

impl AdminContextId {
    pub fn try_from_uuid(value: Uuid) -> Result<Self, PlatformAdministrationDomainError> {
        if value.is_nil() {
            return Err(PlatformAdministrationDomainError::NilIdentifier {
                field: "admin_context_id",
            });
        }
        Ok(Self(value))
    }
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl TryFrom<Uuid> for AdminContextId {
    type Error = PlatformAdministrationDomainError;
    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        Self::try_from_uuid(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminContextStatus {
    Active,
    Revoked,
    Expired,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantAdminContext {
    id: AdminContextId,
    version: u64,
    tenant_id: PlatformTenantId,
    scope: TenantScope,
    principal_ref: String,
    device_id: Uuid,
    session_id: Uuid,
    reason: String,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
    #[serde(skip)]
    events: Vec<DomainEvent>,
}

impl std::fmt::Debug for TenantAdminContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TenantAdminContext")
            .field("id", &"<redacted>")
            .field("tenant_id", &self.tenant_id)
            .field("scope", &self.scope)
            .field("principal_ref", &self.principal_ref)
            .field("reason", &self.reason)
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .field("revoked_at", &self.revoked_at)
            .finish()
    }
}

impl TenantAdminContext {
    #[allow(clippy::too_many_arguments)]
    pub fn issue(
        id: AdminContextId,
        tenant_id: PlatformTenantId,
        scope: TenantScope,
        principal_ref: String,
        device_id: Uuid,
        session_id: Uuid,
        reason: String,
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<Self, PlatformAdministrationDomainError> {
        if !matches!(scope, TenantScope::Holding { .. }) {
            return Err(PlatformAdministrationDomainError::InvalidAdminScope);
        }
        scope
            .validate()
            .map_err(|_| PlatformAdministrationDomainError::InvalidAdminScope)?;
        if device_id.is_nil() {
            return Err(PlatformAdministrationDomainError::NilIdentifier { field: "device_id" });
        }
        if session_id.is_nil() {
            return Err(PlatformAdministrationDomainError::NilIdentifier {
                field: "session_id",
            });
        }
        let ttl = expires_at - issued_at;
        if ttl <= Duration::zero() || ttl > MAX_ADMIN_CONTEXT_TTL {
            return Err(PlatformAdministrationDomainError::InvalidAdminContextLifetime);
        }
        let mut context = Self {
            id,
            version: 1,
            tenant_id,
            scope,
            principal_ref: bounded(&principal_ref, "principal_ref", MAX_PRINCIPAL_BYTES)?,
            device_id,
            session_id,
            reason: bounded(&reason, "reason", MAX_REASON_BYTES)?,
            issued_at,
            expires_at,
            revoked_at: None,
            events: Vec::new(),
        };
        context.events.push(DomainEvent::AdminContextIssued {
            context_id: id,
            tenant_id,
            at: issued_at,
        });
        Ok(context)
    }

    pub fn revoke(&mut self, at: DateTime<Utc>) -> Result<(), PlatformAdministrationDomainError> {
        if self.revoked_at.is_some() {
            return Err(PlatformAdministrationDomainError::AdminContextRevoked);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(PlatformAdministrationDomainError::VersionOverflow)?;
        self.revoked_at = Some(at);
        self.events.push(DomainEvent::AdminContextRevoked {
            context_id: self.id,
            tenant_id: self.tenant_id,
            at,
        });
        Ok(())
    }

    pub fn status_at(&self, at: DateTime<Utc>) -> AdminContextStatus {
        if self.revoked_at.is_some() {
            AdminContextStatus::Revoked
        } else if at >= self.expires_at {
            AdminContextStatus::Expired
        } else {
            AdminContextStatus::Active
        }
    }

    pub const fn id(&self) -> AdminContextId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn tenant_id(&self) -> PlatformTenantId {
        self.tenant_id
    }
    pub fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub fn principal_ref(&self) -> &str {
        &self.principal_ref
    }
    pub const fn issued_at(&self) -> DateTime<Utc> {
        self.issued_at
    }
    pub const fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
    pub const fn revoked_at(&self) -> Option<DateTime<Utc>> {
        self.revoked_at
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.events)
    }
}
