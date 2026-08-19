#![doc = "Platform Administration bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    GetAdminContext, GetAdminContextResult, GetConnector, GetConnectorResult, GetPlatformHolding,
    GetPlatformHoldingResult, IssueHoldingAdminContext, ListConnectors, ListPlatformHoldings,
    ObserveConnectorReadiness, PlatformAdministrationApplicationError,
    PlatformAdministrationCommandHandler, PlatformAdministrationMutationResult,
    PlatformAdministrationQueryHandler, PlatformGovernanceRecordApplicationError,
    PlatformGovernanceRecordCommandHandler, PlatformGovernanceRecordQueryHandler,
    RegisterConnector, RegisterPlatformHolding, RevokeHoldingAdminContext, TransitionConnector,
    TransitionPlatformHolding,
};
pub use domain::{
    AdminContextId, AdminContextStatus, ConnectorBinding, ConnectorBindingId, ConnectorKind,
    ConnectorReadinessEvidence, ConnectorScope, ConnectorState, ConnectorTransition,
    DataPlaneDeploymentMode, DataPlaneRoute, DataPlaneRouteCatalog, DataPlaneRouteState,
    DataPlaneRouteTarget, DataPlaneTopology, EnvironmentId, HoldingAdminContext,
    HoldingLifecycleAction, HoldingLifecycleState, PlatformAdministrationDomainError,
    PlatformAggregate, PlatformGovernanceRecord, PlatformGovernanceRecordError,
    PlatformGovernanceRecordKind, PlatformGovernanceRecordStatus, PlatformHolding,
    PlatformHoldingId, ROUTING_SCHEMA_REVISION,
};
pub use ports::{
    AdminContextView, AuthorizationGrant, AuthorizationPort, CommandFingerprint, CommandIdentity,
    CommandUnitOfWork, ConnectorView, EntitlementPort, GovernanceReadProjection,
    HoldingAuthorityPort, HoldingView, Page, PageFilter, PlatformAdministrationAction,
    PlatformAdministrationCommit, PlatformGovernanceRecordAction,
    PlatformGovernanceRecordAuthorizationPort, PlatformGovernanceRecordAuthorizationRequest,
    PlatformGovernanceRecordCommand, PlatformGovernanceRecordCommandIdentity,
    PlatformGovernanceRecordCommandRequest, PlatformGovernanceRecordCommit,
    PlatformGovernanceRecordQuery, PlatformGovernanceRecordReadRepository,
    PlatformGovernanceRecordUnitOfWork, PlatformGovernanceRecordWriteRepository,
    PlatformRecordView, PortError, ReadProjection, ReadRepository, StoredCommit,
    StoredPlatformGovernanceRecordCommit, WriteRepository,
};
