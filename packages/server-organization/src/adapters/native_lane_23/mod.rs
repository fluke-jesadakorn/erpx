//! Native migration lane 23 for organization.
//!
//! The directory hierarchy read is owned end-to-end here: owner-local SQL and
//! projection in `postgres`, invariants in `domain`, a SQL-free application
//! handler, and an axum ingress that only transports the validated response.
//! TypeScript keeps a strict, fail-closed client and no SQL or calculation.
mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    read_directory_hierarchy, read_executive_evidence, read_node_references,
    DirectoryHierarchyRead, ExecutiveEvidenceRead, ExecutiveEvidenceRequest, NodeReferenceRead,
};
pub use domain::{
    AttachmentNodeFact, AudienceNode, DirectoryHierarchy, DirectoryLevel, DirectoryNode,
    DirectoryPosition, ExecutiveEvidence, ExecutiveEvidenceKind, NodeReferences, UnitReference,
    MAX_ROWS,
};
pub use http::{LaneTarget, router, router_for_holdings};
pub use postgres::{
    PgDirectoryHierarchyRead, ATTACHMENT_NODE_SQL, AUDIENCE_NODE_SQL, CONNECTION_SCOPES_SQL,
    CONTEXT_NODES_SQL, CONTEXT_POSITIONS_SQL, LEVELS_SQL, NODE_EVIDENCE_SQL, NODES_SQL,
    PENDING_EVIDENCE_SQL, POSITIONS_SQL, UNIT_REFERENCE_SQL,
};
