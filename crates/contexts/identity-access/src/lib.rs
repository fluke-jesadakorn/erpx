#![doc = "Identity & Access bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    ActivatePrincipal, AddGroupMember, AssignRole, CreateAuthorityGroup, CreatePrincipal,
    CreatePrincipalResult, DefineRole, DefineRoleResult, DisablePrincipal, GetPrincipal,
    GetPrincipalResult, GetRole, GetRoleResult, IdentityAccessApplicationError,
    IdentityAccessCommandHandler, IdentityAccessQueryHandler, IdentityGovernanceApplicationError,
    IdentityGovernanceCommandHandler, ListPrincipals, ListRoles, PrincipalMutationResult,
    ReactivatePrincipal, RecoverRole, RenewRole, RevokeGroupMember, RevokeRole, RoleMutationResult,
    SetRolePermissions, SuspendPrincipal,
};
pub use domain::{
    AccessCertification, AccessPrincipal, AccessPrincipalState, AccessRole, AccessRoleState,
    BreakGlassSession, BreakGlassStatus, CertificationDecision, CertificationItem,
    FRESH_MFA_WINDOW, GroupMembership, GroupMembershipState, HighRiskAuthority,
    IdentityAccessAggregate, IdentityAccessDomainError, IdentityGovernanceRecord,
    MAX_BREAK_GLASS_DURATION, NonHumanPrincipalGovernance, PermissionKey, PrincipalId,
    PrincipalKind, PrincipalRealm, PrincipalStatus, RiskTier, RoleAssignment, RoleAssignmentId,
    RoleAssignmentState, RoleCode, RoleId,
};
pub use ports::{
    AggregateKey, AuthorizationPort, CommandFingerprint, CommandIdentity, CommandUnitOfWork,
    EntitlementPort, IdentityAccessAction, IdentityAccessCommit, IdentityGovernanceAction,
    IdentityGovernanceAuthorizationPort, IdentityGovernanceAuthorizationRequest,
    IdentityGovernanceCommand, IdentityGovernanceCommandIdentity, IdentityGovernanceCommandRequest,
    IdentityGovernanceCommit, IdentityGovernanceReadRepository, IdentityGovernanceUnitOfWork,
    IdentityGovernanceWriteRepository, PortError, PrincipalList, PrincipalListFilter,
    PrincipalView, ReadProjection, ReadRepository, RoleAssignmentView, RoleList, RoleListFilter,
    RoleView, StoredCommit, StoredIdentityGovernanceCommit, WriteRepository,
};
