//! Native Projects work-structure and handoff command interface.
pub use crate::application::project_structure::{
    ProjectStructureCommandRepository, ProjectStructureFuture, execute_structure_command,
};
pub use crate::adapters::project_structure::PgProjectStructureCommands;
pub use crate::domain::project_structure::{
    ActualSourceKind, ApprovedCost, ApprovedTime, ProjectHandoffFact, ProjectPackageFact,
    ProjectStructureActor, ProjectStructureCommand, ProjectStructureReceipt, ProjectStructureTask,
    ProjectTaskBinding, StoredStructureReceipt, StructurePlan, StructurePlanAction,
    structure_fingerprint,
};
