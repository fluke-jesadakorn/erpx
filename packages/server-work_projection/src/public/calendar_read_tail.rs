//! Owner-local public port for the Work calendar read tail.
//!
//! The typed application service and its domain records are exposed here for a
//! future composition mount. This slice deliberately stops at the owner
//! boundary: nothing is wired into `src/composition/**` and no TypeScript
//! consumer is deleted.
pub use crate::application::calendar_read_tail::{CalendarReadTail, family_of};
pub use crate::domain::calendar_read_tail::{
    AppointmentRow, AppointmentVersion, CalendarReadFamily, MovedOccurrence, OccurrenceException,
    OccurrenceExceptionChange, OccurrenceResponse, OccurrenceResponseValue, ReminderAuthorization,
    ReminderConfiguration,
};
