#![doc = "Identity & Access bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    ActivatePrincipal, AssignRole, CreatePrincipal, CreatePrincipalResult, DefineRole,
    DefineRoleResult, DisablePrincipal, GetPrincipal, GetPrincipalResult, GetRole, GetRoleResult,
    IdentityAccessApplicationError, IdentityAccessCommandHandler, IdentityAccessQueryHandler,
    ListPrincipals, ListRoles, PrincipalMutationResult, ReactivatePrincipal, RevokeRole,
    RoleMutationResult, SetRolePermissions, SuspendPrincipal,
};
pub use domain::{
    AccessPrincipal, AccessRole, IdentityAccessAggregate, IdentityAccessDomainError, PermissionKey,
    PrincipalId, PrincipalStatus, RiskTier, RoleAssignment, RoleAssignmentId, RoleCode, RoleId,
};
pub use ports::{
    AggregateKey, AuthorizationPort, CommandFingerprint, CommandIdentity, CommandUnitOfWork,
    EntitlementPort, IdentityAccessAction, IdentityAccessCommit, PortError, PrincipalList,
    PrincipalListFilter, PrincipalView, ReadProjection, ReadRepository, RoleAssignmentView,
    RoleList, RoleListFilter, RoleView, StoredCommit, WriteRepository,
};
