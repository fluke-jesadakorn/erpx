#![doc = "Platform Administration bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    GetAdminContext, GetAdminContextResult, GetConnector, GetConnectorResult, GetPlatformTenant,
    GetPlatformTenantResult, IssueTenantAdminContext, ListConnectors, ListPlatformTenants,
    PlatformAdministrationApplicationError, PlatformAdministrationCommandHandler,
    PlatformAdministrationMutationResult, PlatformAdministrationQueryHandler, RegisterConnector,
    RegisterPlatformTenant, RevokeTenantAdminContext, TransitionConnector,
    TransitionPlatformTenant,
};
pub use domain::{
    AdminContextId, AdminContextStatus, ConnectorBinding, ConnectorBindingId, ConnectorKind,
    ConnectorScope, ConnectorState, ConnectorTransition, EnvironmentId,
    PlatformAdministrationDomainError, PlatformAggregate, PlatformTenant, PlatformTenantId,
    TenantAdminContext, TenantLifecycleAction, TenantLifecycleState,
};
pub use ports::{
    AdminContextView, AuthorizationGrant, AuthorizationPort, CommandFingerprint, CommandIdentity,
    CommandUnitOfWork, ConnectorView, EntitlementPort, Page, PageFilter,
    PlatformAdministrationAction, PlatformAdministrationCommit, PlatformRecordView, PortError,
    ReadProjection, ReadRepository, StoredCommit, TenantAuthorityPort, TenantView, WriteRepository,
};
