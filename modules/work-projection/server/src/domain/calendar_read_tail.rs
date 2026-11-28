//! Domain records for the owner-local Work calendar read tail.
//!
//! These are the typed shapes of the retained TypeScript read authorities for
//! the three hot families: appointment row, occurrence exception/response and
//! reminder authority. They carry no SQL and no transport: the application port
//! decodes allow-listed native read results into them and callers consume the
//! records while the TypeScript facts stay in place.
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The bounded read families this slice ports. Appointment row was already
/// native (`READ_12/26/37`); the other two are added here without touching
/// composition or deleting a TypeScript consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarReadFamily {
    AppointmentRow,
    Occurrence,
    ReminderAuthority,
}

/// One calendar appointment row, exactly as returned by the existing native
/// appointment read and consumed by the calendar command application.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AppointmentRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub organizer_person_id: String,
    pub title: String,
    pub description: String,
    pub starts_at: i64,
    pub ends_at: i64,
    pub timezone: String,
    pub location: String,
    pub meeting_url: String,
    pub private: bool,
    pub guests: Value,
    pub status: String,
    pub version: i32,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub all_day: bool,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

/// `calendar-availability-facts.ts::readOccurrenceException` projection.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct OccurrenceExceptionChange {
    pub start_local: Option<String>,
    pub end_local: Option<String>,
    pub cancelled: bool,
}

/// `calendar-availability-facts.ts::readOccurrenceResponse` projection.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct OccurrenceResponseValue {
    pub response: String,
}

/// `calendar-appointments-facts.ts::read4` full exception row.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct OccurrenceException {
    pub appointment_id: String,
    pub occurrence_key: String,
    pub start_local: Option<String>,
    pub end_local: Option<String>,
    pub cancelled: bool,
}

/// `calendar-appointments-facts.ts::read5` response row.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct OccurrenceResponse {
    pub appointment_id: String,
    pub occurrence_key: String,
    pub user_id: String,
    pub response: String,
}

/// `calendar-availability-facts.ts::readMovedOccurrences` projection.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct MovedOccurrence {
    pub occurrence_key: String,
    pub start_local: String,
    pub end_local: String,
}

/// `reminder-authority.ts::appointmentVersionSql` projection.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AppointmentVersion {
    pub version: i32,
}

/// `reminder-authority.ts::calendarReminderConfiguration` image.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ReminderConfiguration {
    pub image: Option<String>,
}

/// `calendar_reminder_execution_authorizations` row selected by the delivery
/// path in `calendar/reminder-purchases.ts`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ReminderAuthorization {
    pub command_id: String,
    pub appointment_id: String,
    pub recipient_id: String,
    pub company_id: String,
    pub actor_id: String,
    pub assignment_id: String,
    pub action: String,
    pub authority_json: String,
    pub source_hash: String,
    pub source_version: i64,
    pub created_at: i64,
}
