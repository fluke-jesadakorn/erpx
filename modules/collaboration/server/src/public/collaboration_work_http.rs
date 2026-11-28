//! Collaboration action-card/record-relation HTTP ingress.
pub const CONTRACT_VERSION: &str = "collaboration.work.http.v1";

pub use crate::adapters::collaboration_work_http::{
    CollaborationCardData, CollaborationRelationData, CollaborationWorkHttpBackend,
    CollaborationWorkHttpCommandRequest, CollaborationWorkHttpFuture,
    CollaborationWorkHttpReadReply, CollaborationWorkHttpReadRequest, CollaborationWorkHttpState,
    CollaborationWorkHttpTarget, CollaborationWorkReadKind, CollaborationWorkspaceData, ReadQuery,
    collaboration_work_router, command, read_card, read_relation, read_workspace,
};
