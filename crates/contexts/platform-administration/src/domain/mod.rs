mod admin_context;
mod connector;
mod governance;
mod holding;
mod routing;

pub use admin_context::{AdminContextId, AdminContextStatus, HoldingAdminContext};
pub use connector::{
    ConnectorBinding, ConnectorBindingId, ConnectorKind, ConnectorReadinessEvidence,
    ConnectorScope, ConnectorState, ConnectorTransition,
};
pub use governance::{
    PlatformGovernanceRecord, PlatformGovernanceRecordError, PlatformGovernanceRecordKind,
    PlatformGovernanceRecordStatus,
};
pub use holding::{
    EnvironmentId, HoldingLifecycleAction, HoldingLifecycleState, PlatformHolding,
    PlatformHoldingId,
};
pub use routing::{
    DataPlaneDeploymentMode, DataPlaneRoute, DataPlaneRouteCatalog, DataPlaneRouteState,
    DataPlaneRouteTarget, DataPlaneTopology, ROUTING_SCHEMA_REVISION,
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
    #[error("holding lifecycle transition is not allowed")]
    InvalidHoldingTransition,
    #[error("connector lifecycle transition is not allowed")]
    InvalidConnectorTransition,
    #[error("connector kind is not supported")]
    InvalidConnectorKind,
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
    #[error("data-plane route requires schema revision 2")]
    UnsupportedRouteSchemaRevision,
    #[error("holding administration context lifetime is invalid")]
    InvalidAdminContextLifetime,
    #[error("holding administration context has already been revoked")]
    AdminContextRevoked,
    #[error("holding administration scope must be holding-scoped")]
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
    HoldingChanged {
        state: PublishedHoldingState,
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
        holding_id: PlatformHoldingId,
        at: DateTime<Utc>,
    },
    AdminContextRevoked {
        context_id: AdminContextId,
        holding_id: PlatformHoldingId,
        at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PublishedHoldingState {
    pub holding_id: PlatformHoldingId,
    pub code: String,
    pub name: String,
    pub environment_id: EnvironmentId,
    pub status: HoldingLifecycleState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "aggregate_type",
    content = "aggregate",
    rename_all = "snake_case"
)]
pub enum PlatformAggregate {
    Holding(PlatformHolding),
    Connector(ConnectorBinding),
    AdminContext(HoldingAdminContext),
}

impl PlatformAggregate {
    pub const fn id_uuid(&self) -> uuid::Uuid {
        match self {
            Self::Holding(value) => value.id().as_uuid(),
            Self::Connector(value) => value.id().as_uuid(),
            Self::AdminContext(value) => value.id().as_uuid(),
        }
    }

    pub const fn version(&self) -> u64 {
        match self {
            Self::Holding(value) => value.version(),
            Self::Connector(value) => value.version(),
            Self::AdminContext(value) => value.version(),
        }
    }

    pub const fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Holding(_) => "platform_holding",
            Self::Connector(_) => "connector_binding",
            Self::AdminContext(_) => "holding_admin_context",
        }
    }

    pub fn scope(&self) -> erpx_kernel::AuthorityScope {
        match self {
            Self::Holding(value) => value.scope(),
            Self::Connector(value) => value.scope().active_scope(),
            Self::AdminContext(value) => value.scope().clone(),
        }
    }

    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        match self {
            Self::Holding(value) => value.take_uncommitted_events(),
            Self::Connector(value) => value.take_uncommitted_events(),
            Self::AdminContext(value) => value.take_uncommitted_events(),
        }
    }
}

#[cfg(test)]
mod tests;
