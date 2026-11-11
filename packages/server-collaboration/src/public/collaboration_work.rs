//! Collaboration mutation port; no commit or source authority is supplied here.
pub const CONTRACT_VERSION: &str = "collaboration.work.v1";
pub use crate::adapters::collaboration_work::apply_plan;
pub use crate::adapters::collaboration_work::{CurrentCollaborationEffects, retain_effects};
pub use crate::adapters::collaboration_work::{
    CurrentCollaborationRow, StoredCard, lock_collaboration_work_sources, read_card, read_relation,
};
pub use crate::adapters::collaboration_work_pg::{
    CollaborationScope, CollaborationWorkspace, PgCollaborationUnit, ResolvedSource, VisibleSource,
    WORKSPACE_CARD_LIMIT, WORKSPACE_PEOPLE_LIMIT, WORKSPACE_RELATION_LIMIT, execute_scope,
    hidden_source, read_scope_card, read_scope_relation, read_scope_workspace, resolve_source,
};
pub use crate::application::collaboration_work::{
    CollaborationSourcePort, CollaborationUnit, SourcePresentation, execute,
};
pub use crate::domain::collaboration_work::{
    ActionWork, ActionWorkInput, AuthorizedActor, CardAction, Change, CurrentCard, CurrentRelation,
    OwnerPlan, Receipt, RelationKind, WORKSPACE_SOURCE_TYPES, WriterPartition, action_work,
    authorize_card_change, card_partition, plan_receipt, relation_partition, relation_sources,
};

pub use crate::adapters::collaboration_work::{FieldWorkRows, read_field_work_rows};
pub use crate::adapters::collaboration_work_wire::{DecodedWorkCommand, decode_work_command};
