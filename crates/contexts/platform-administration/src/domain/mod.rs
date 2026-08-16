mod admin_context;
mod connector;
mod governance;
mod routing;
mod tenant;

pub use admin_context::{AdminContextId, AdminContextStatus, TenantAdminContext};
pub use connector::{
    ConnectorBinding, ConnectorBindingId, ConnectorKind, ConnectorReadinessEvidence,
    ConnectorScope, ConnectorState, ConnectorTransition,
};
pub use governance::{
    PlatformGovernanceRecord, PlatformGovernanceRecordError, PlatformGovernanceRecordKind,
    PlatformGovernanceRecordStatus,
};
pub use routing::{
    DataPlaneDeploymentMode, DataPlaneRoute, DataPlaneRouteCatalog, DataPlaneRouteState,
    DataPlaneRouteTarget, DataPlaneTopology, ROUTING_SCHEMA_GENERATION,
};
pub use tenant::{
    EnvironmentId, PlatformTenant, PlatformTenantId, TenantLifecycleAction, TenantLifecycleState,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlatformAdministrationDomainError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds the maximum length of {maximum} bytes")]
    TooLong { field: &'static str, maximum: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("tenant lifecycle transition is not allowed")]
    InvalidTenantTransition,
    #[error("connector lifecycle transition is not allowed")]
    InvalidConnectorTransition,
    #[error("connector configuration digest must be a SHA-256 hexadecimal digest")]
    InvalidConfigurationDigest,
    #[error("connector capability set is invalid")]
    InvalidCapabilities,
    #[error("connector readiness evidence is invalid")]
    InvalidReadinessEvidence,
    #[error("connector readiness evidence does not match the registered configuration")]
    ReadinessConfigurationMismatch,
    #[error("connector readiness evidence is stale or replayed")]
    StaleReadinessEvidence,
    #[error("connector cannot activate without current successful readiness evidence")]
    ConnectorNotReady,
    #[error("data-plane route target is not valid for the selected topology")]
    InvalidRouteTarget,
    #[error("data-plane route target is already reserved with different profile references")]
    RouteAlreadyReserved,
    #[error("data-plane routing reference must be opaque and secret-free")]
    InvalidRoutingReference,
    #[error("data-plane route lifecycle transition is not allowed")]
    InvalidRouteTransition,
    #[error("data-plane route observation is stale")]
    StaleRouteObservation,
    #[error("data-plane route is unavailable")]
    RouteUnavailable,
    #[error("data-plane route requires schema generation 2")]
    UnsupportedRouteSchemaGeneration,
    #[error("tenant administration context lifetime is invalid")]
    InvalidAdminContextLifetime,
    #[error("tenant administration context has already been revoked")]
    AdminContextRevoked,
    #[error("tenant administration scope must be holding-scoped")]
    InvalidAdminScope,
    #[error("aggregate version overflow")]
    VersionOverflow,
}

pub(crate) fn bounded(
    value: &str,
    field: &'static str,
    maximum: usize,
) -> Result<String, PlatformAdministrationDomainError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(PlatformAdministrationDomainError::Required { field });
    }
    if value.len() > maximum {
        return Err(PlatformAdministrationDomainError::TooLong { field, maximum });
    }
    if value.chars().any(char::is_control) {
        return Err(PlatformAdministrationDomainError::ControlCharacter { field });
    }
    Ok(value.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum DomainEvent {
    TenantChanged {
        state: PublishedTenantState,
        at: DateTime<Utc>,
    },
    ConnectorChanged {
        connector_id: ConnectorBindingId,
        state: ConnectorState,
        at: DateTime<Utc>,
    },
    ConnectorReadinessObserved {
        connector_id: ConnectorBindingId,
        ready: bool,
        at: DateTime<Utc>,
    },
    AdminContextIssued {
        context_id: AdminContextId,
        tenant_id: PlatformTenantId,
        at: DateTime<Utc>,
    },
    AdminContextRevoked {
        context_id: AdminContextId,
        tenant_id: PlatformTenantId,
        at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PublishedTenantState {
    pub tenant_id: PlatformTenantId,
    pub code: String,
    pub name: String,
    pub environment_id: EnvironmentId,
    pub status: TenantLifecycleState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "aggregate_type",
    content = "aggregate",
    rename_all = "snake_case"
)]
pub enum PlatformAggregate {
    Tenant(PlatformTenant),
    Connector(ConnectorBinding),
    AdminContext(TenantAdminContext),
}

impl PlatformAggregate {
    pub const fn id_uuid(&self) -> uuid::Uuid {
        match self {
            Self::Tenant(value) => value.id().as_uuid(),
            Self::Connector(value) => value.id().as_uuid(),
            Self::AdminContext(value) => value.id().as_uuid(),
        }
    }

    pub const fn version(&self) -> u64 {
        match self {
            Self::Tenant(value) => value.version(),
            Self::Connector(value) => value.version(),
            Self::AdminContext(value) => value.version(),
        }
    }

    pub const fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Tenant(_) => "platform_tenant",
            Self::Connector(_) => "connector_binding",
            Self::AdminContext(_) => "tenant_admin_context",
        }
    }

    pub fn scope(&self) -> erpx_kernel::TenantScope {
        match self {
            Self::Tenant(value) => value.scope(),
            Self::Connector(value) => value.scope().tenant_scope(),
            Self::AdminContext(value) => value.scope().clone(),
        }
    }

    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        match self {
            Self::Tenant(value) => value.take_uncommitted_events(),
            Self::Connector(value) => value.take_uncommitted_events(),
            Self::AdminContext(value) => value.take_uncommitted_events(),
        }
    }
}

#[cfg(test)]
mod tests;
