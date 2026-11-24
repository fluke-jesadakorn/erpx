//! Typed application port for the owner-local Work calendar read tail.
//!
//! Every method executes one allow-listed native read on the caller's Holding
//! connection and decodes the exact row shape the retained TypeScript facts
//! return. No SQL, no composition mount and no TypeScript fallback live here;
//! the TypeScript consumers stay reachable until the lead's cutover.
use crate::{
    adapters::calendar_read::{
        CalendarReadCommand, CalendarReadError, READ_12, READ_26, READ_37, READ_60, READ_61,
        READ_62, READ_63, READ_64, READ_65, READ_66, READ_67, execute_calendar_read,
    },
    domain::calendar_read_tail::{
        AppointmentRow, AppointmentVersion, CalendarReadFamily, MovedOccurrence,
        OccurrenceException, OccurrenceExceptionChange, OccurrenceResponse, OccurrenceResponseValue,
        ReminderAuthorization, ReminderConfiguration,
    },
};
use erpx_server_shared::public::BusinessError;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sqlx::PgConnection;

/// Which of the three ported families an allow-listed read belongs to.
pub fn family_of(kind: &str) -> Option<CalendarReadFamily> {
    match kind {
        READ_12 | READ_26 | READ_37 => Some(CalendarReadFamily::AppointmentRow),
        READ_60 | READ_61 | READ_62 | READ_63 | READ_64 => Some(CalendarReadFamily::Occurrence),
        READ_65 | READ_66 | READ_67 => Some(CalendarReadFamily::ReminderAuthority),
        _ => None,
    }
}

fn invalid() -> CalendarReadError {
    CalendarReadError::from(BusinessError::invalid(
        "WORK_CALENDAR_READ_INVALID",
        "The native calendar read tail returned an unexpected row.",
    ))
}

fn one<T: DeserializeOwned>(value: Value) -> Result<Option<T>, CalendarReadError> {
    if value.is_null() {
        Ok(None)
    } else {
        serde_json::from_value(value).map(Some).map_err(|_| invalid())
    }
}

fn many<T: DeserializeOwned>(value: Value) -> Result<Vec<T>, CalendarReadError> {
    serde_json::from_value(value).map_err(|_| invalid())
}

/// Scoped, typed reads bound to one transaction and one Company.
pub struct CalendarReadTail<'a> {
    connection: &'a mut PgConnection,
    company_id: &'a str,
}

impl<'a> CalendarReadTail<'a> {
    pub fn new(connection: &'a mut PgConnection, company_id: &'a str) -> Self {
        Self {
            connection,
            company_id,
        }
    }

    async fn read(&mut self, kind: &'static str, values: Vec<Value>) -> Result<Value, CalendarReadError> {
        let command = CalendarReadCommand {
            kind,
            values: &values,
            company_id: self.company_id,
        };
        execute_calendar_read(&mut *self.connection, &command).await
    }

    /// Appointment-row family. The existing `READ_12` fact is re-exposed here
    /// so all three families share one typed owner-local surface.
    pub async fn appointment_row(&mut self, id: &str) -> Result<Option<AppointmentRow>, CalendarReadError> {
        let result = self.read(READ_12, vec![json!(id), json!(self.company_id)]).await?;
        one(result)
    }

    /// `calendar-availability-facts.ts::readOccurrenceException`.
    pub async fn occurrence_exception(
        &mut self,
        appointment_id: &str,
        occurrence_key: &str,
    ) -> Result<Option<OccurrenceExceptionChange>, CalendarReadError> {
        let result = self
            .read(
                READ_60,
                vec![json!(appointment_id), json!(self.company_id), json!(occurrence_key)],
            )
            .await?;
        one(result)
    }

    /// `calendar-availability-facts.ts::readOccurrenceResponse`.
    pub async fn occurrence_response(
        &mut self,
        appointment_id: &str,
        occurrence_key: &str,
        user_id: &str,
    ) -> Result<Option<OccurrenceResponseValue>, CalendarReadError> {
        let result = self
            .read(
                READ_61,
                vec![
                    json!(appointment_id),
                    json!(self.company_id),
                    json!(occurrence_key),
                    json!(user_id),
                ],
            )
            .await?;
        one(result)
    }

    /// `calendar-availability-facts.ts::readMovedOccurrences`.
    pub async fn moved_occurrences(
        &mut self,
        appointment_id: &str,
    ) -> Result<Vec<MovedOccurrence>, CalendarReadError> {
        let result = self
            .read(READ_62, vec![json!(appointment_id), json!(self.company_id)])
            .await?;
        many(result)
    }

    /// `calendar-appointments-facts.ts::read4`.
    pub async fn occurrence_exceptions(
        &mut self,
        appointment_ids: &[&str],
    ) -> Result<Vec<OccurrenceException>, CalendarReadError> {
        let result = self
            .read(
                READ_63,
                vec![json!(self.company_id), json!(appointment_ids)],
            )
            .await?;
        many(result)
    }

    /// `calendar-appointments-facts.ts::read5`.
    pub async fn occurrence_responses(
        &mut self,
        appointment_ids: &[&str],
    ) -> Result<Vec<OccurrenceResponse>, CalendarReadError> {
        let result = self
            .read(
                READ_64,
                vec![json!(self.company_id), json!(appointment_ids)],
            )
            .await?;
        many(result)
    }

    /// `reminder-authority.ts::appointmentVersionSql`.
    pub async fn appointment_version(
        &mut self,
        id: &str,
    ) -> Result<Option<AppointmentVersion>, CalendarReadError> {
        let result = self
            .read(READ_65, vec![json!(id), json!(self.company_id)])
            .await?;
        one(result)
    }

    /// `reminder-authority.ts::calendarReminderConfiguration`.
    pub async fn reminder_configuration(
        &mut self,
        id: &str,
        user_id: &str,
    ) -> Result<Option<ReminderConfiguration>, CalendarReadError> {
        let result = self
            .read(
                READ_66,
                vec![json!(id), json!(self.company_id), json!(user_id)],
            )
            .await?;
        one(result)
    }

    /// Newest execution authorization in `calendar/reminder-purchases.ts`.
    pub async fn reminder_authorization(
        &mut self,
        id: &str,
        recipient_id: &str,
    ) -> Result<Option<ReminderAuthorization>, CalendarReadError> {
        let result = self
            .read(
                READ_67,
                vec![json!(id), json!(self.company_id), json!(recipient_id)],
            )
            .await?;
        one(result)
    }
}
