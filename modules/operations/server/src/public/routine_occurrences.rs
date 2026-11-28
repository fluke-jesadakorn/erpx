//! Native Operations scheduled routine occurrence generator.
pub const CONTRACT_VERSION: &str = "operations.routine-occurrences.v1";
pub use crate::adapters::routine_occurrences::{
    LatestScheduleCursor, RoutineOccurrenceFuture, RoutineOccurrenceUnit, authorization,
    due_schedules, effects_hash, generate_due_occurrences, lag, latest, lock_occurrence_sources,
    occurrence_image, production_enabled, production_forbidden, record_error,
    require_authorization, require_authorization_source, require_current_authority,
    require_lineage, source, source_facts, stage_occurrence, steps,
};
pub use crate::domain::routine_occurrences::{
    OccurrenceOutcome, OccurrencePass, OccurrencePlan, OccurrencePolicy, OccurrenceSource,
    OccurrenceSourceFacts, RoutineExecutionAuthorization, RoutineExecutionStep,
    StoredRoutineAuthority, StoredRoutineIam, StoredRoutineSession, authorization_required,
    due_at_iso, plan_occurrence, source_changed,
};
