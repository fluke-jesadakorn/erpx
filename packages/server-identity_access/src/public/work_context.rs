//! Inspection of the current selected grant; effective IAM permission and owner policy are separate.
pub use crate::adapters::holding_work_context::HoldingWorkContextInspector;
pub use crate::adapters::postgres_work_context::PgWorkContextReader;
pub use crate::application::work_context::{
    CurrentWorkContextPort, WorkContextFuture, inspect_current_work_selection,
};
pub use crate::domain::work_context::HoldingWorkspaceRequest;
pub use crate::domain::work_context::{
    ActiveSelectionFact, CurrentWorkSelection, ExpectedWorkSelection, PublishedRoleFact,
    WorkAssignmentFact, WorkContextRequest, WorkContextSnapshot, WorkScopeKind,
    WorkspaceLineageFact, assignment_applies,
};
