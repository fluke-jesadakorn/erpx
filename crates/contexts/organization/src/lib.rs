#![doc = "Organization bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    ActivateOrganizationNode, ArchiveOrganizationNode, ChangeOrganizationCode,
    CreateOrganizationNode, CreateOrganizationNodeResult, GetOrganizationNode,
    GetOrganizationNodeResult, ListOrganizationNodes, OrganizationApplicationError,
    OrganizationCommandHandler, OrganizationMutationResult, OrganizationQueryHandler,
    RenameOrganizationNode, ReparentOrganizationNode, SuspendOrganizationNode,
};
pub use domain::{
    OrganizationDomainError, OrganizationNode, OrganizationNodeId, OrganizationNodeKind,
    OrganizationNodeStatus, OrganizationPlacement,
};
pub use ports::{
    AuthorizationPort, CommandFingerprint, CommandIdentity, CommandUnitOfWork, EntitlementPort,
    HierarchyGuard, HierarchyPort, NodeList, NodeListFilter, NodeView, OrganizationAction,
    OrganizationCommit, PortError, ReadProjection, ReadRepository, StoredCommit, WriteRepository,
};
