//! Native Organization position-save contract.
//!
//! Consumers use these named exports; no SQL relation or private adapter type
//! escapes the crate. The retained TypeScript writer remains in place until the
//! composition root mounts this router and retires that path.
pub use crate::adapters::position_http::{
    OrganizationPositionHttpBackend, OrganizationPositionHttpFuture, OrganizationPositionHttpState,
    OrganizationPositionHttpTarget, SaveOrganizationPositionHttpRequest,
    organization_position_router,
};
pub use crate::adapters::postgres_position::PgOrganizationPosition;
pub use crate::application::position::{
    ClockSource, OrganizationPositionRepository, RosterExitPort, save_position,
};
pub use crate::domain::position::{
    PositionGraphGuard, PositionInput, PositionManagerGrant, PositionRoster, PositionRosterPerson,
    PositionRosterPosition, added_reporting_issues, candidate_position, ceo_position_invalid,
    level_required, manager_issues, manager_level_invalid, position_in_use, position_level_invalid,
    position_not_found, position_reporting_issues, reporting_cycle, reporting_position_invalid,
    reporting_position_required, validate_archive_in_use, validate_ceo_and_reporting,
    validate_level_required, validate_reporting_chain, validate_reporting_issues,
    validate_reporting_levels,
};
