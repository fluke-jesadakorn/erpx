#![doc = "Platform Administration bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    GetAdminContext, GetAdminContextResult, GetConnector, GetConnectorResult, GetPlatformTenant,
    GetPlatformTenantResult, IssueTenantAdminContext, ListConnectors, ListPlatformTenants,
    ObserveConnectorReadiness, PlatformAdministrationApplicationError,
    PlatformAdministrationCommandHandler, PlatformAdministrationMutationResult,
    PlatformAdministrationQueryHandler, PlatformGovernanceRecordApplicationError,
    PlatformGovernanceRecordCommandHandler, PlatformGovernanceRecordQueryHandler,
    RegisterConnector, RegisterPlatformTenant, RevokeTenantAdminContext, TransitionConnector,
    TransitionPlatformTenant,
};
pub use domain::{
    AdminContextId, AdminContextStatus, ConnectorBinding, ConnectorBindingId, ConnectorKind,
    ConnectorReadinessEvidence, ConnectorScope, ConnectorState, ConnectorTransition,
    DataPlaneDeploymentMode, DataPlaneRoute, DataPlaneRouteCatalog, DataPlaneRouteState,
    DataPlaneRouteTarget, DataPlaneTopology, EnvironmentId, PlatformAdministrationDomainError,
    PlatformAggregate, PlatformGovernanceRecord, PlatformGovernanceRecordError,
    PlatformGovernanceRecordKind, PlatformGovernanceRecordStatus, PlatformTenant, PlatformTenantId,
    ROUTING_SCHEMA_GENERATION, TenantAdminContext, TenantLifecycleAction, TenantLifecycleState,
};
pub use ports::{
    AdminContextView, AuthorizationGrant, AuthorizationPort, CommandFingerprint, CommandIdentity,
    CommandUnitOfWork, ConnectorView, EntitlementPort, GovernanceReadProjection, Page, PageFilter,
    PlatformAdministrationAction, PlatformAdministrationCommit, PlatformGovernanceRecordAction,
    PlatformGovernanceRecordAuthorizationPort, PlatformGovernanceRecordAuthorizationRequest,
    PlatformGovernanceRecordCommand, PlatformGovernanceRecordCommandIdentity,
    PlatformGovernanceRecordCommandRequest, PlatformGovernanceRecordCommit,
    PlatformGovernanceRecordQuery, PlatformGovernanceRecordReadRepository,
    PlatformGovernanceRecordUnitOfWork, PlatformGovernanceRecordWriteRepository,
    PlatformRecordView, PortError, ReadProjection, ReadRepository, StoredCommit,
    StoredPlatformGovernanceRecordCommit, TenantAuthorityPort, TenantView, WriteRepository,
};
