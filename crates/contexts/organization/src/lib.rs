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
    OrganizationSharingCommandHandler, RenameOrganizationNode, ReparentOrganizationNode,
    SharingApplicationError, SuspendOrganizationNode,
};
pub use domain::{
    DirectoryVisibility, OrganizationDomainError, OrganizationNode, OrganizationNodeId,
    OrganizationNodeKind, OrganizationNodeState, OrganizationNodeStatus, OrganizationPlacement,
    OrganizationSharingAgreement, ProposeSharingAgreement, SharingAgreementId, SharingCapability,
    SharingDirection, SharingMutation, SharingStatus, can_discover_across_hierarchy,
};
pub use ports::{
    AuthorizationPort, CommandFingerprint, CommandIdentity, CommandUnitOfWork, EntitlementPort,
    HierarchyGuard, HierarchyPort, NodeList, NodeListFilter, NodeView, OrganizationAction,
    OrganizationCommit, PortError, ReadProjection, ReadRepository, SharingAction,
    SharingAgreementList, SharingAuthorizationPort, SharingAuthorizationRequest,
    SharingCommandIdentity, SharingCommandPayload, SharingCommandRequest, SharingCommandUnitOfWork,
    SharingCommit, SharingListFilter, SharingReadRepository, SharingWriteRepository, StoredCommit,
    StoredSharingCommit, WriteRepository,
};
