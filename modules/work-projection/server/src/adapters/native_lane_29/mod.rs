//! Native migration lane 29 (work_projection).
//!
//! Owner-local calendar availability/occurrence reads are executed by axum and
//! sqlx here. The TypeScript adapters keep only a strict private transport.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{Lane29Future, Lane29Read, Lane29Service};
pub use domain::{
    AnnouncementCandidate, AppointmentCandidate, AttendeeRow, AvailabilitySource, ConflictProbe,
    ConflictScope, MovedOccurrence, OccurrenceException, ReadKind, ReadRequest, SeriesRow,
    valid_id, valid_scope_json,
};
pub use http::{LaneTarget, router, router_for_holdings};
pub use postgres::{
    PgLane29Read, SQL_ANNOUNCEMENT_CANDIDATES, SQL_APPOINTMENT_CANDIDATES, SQL_ATTENDEE_ROWS,
    SQL_AVAILABILITY_HEAD, SQL_CONFLICT_ATTENDEE, SQL_CONFLICT_PARTICIPANT,
    SQL_MOVED_OCCURRENCES, SQL_OCCURRENCE_EXCEPTION, SQL_SERIES_BATCH, sql_availability_sources,
};
