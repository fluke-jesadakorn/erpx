//! PostgreSQL IANA calendar for preventive-plan recurrence. PostgreSQL's
//! bundled timezone database is the single source of DST rules; the process
//! locale is never read.
use crate::application::preventive_plan::{
    PreventiveCalendar, PreventiveCalendarFuture,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;

fn calendar_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MAINTENANCE_CALENDAR_UNAVAILABLE",
        "The maintenance calendar is unavailable.",
    )
}

pub struct PgPreventiveCalendar<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgPreventiveCalendar<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

impl PreventiveCalendar for PgPreventiveCalendar<'_> {
    fn valid_zone<'a>(
        &'a mut self,
        zone: &'a str,
    ) -> PreventiveCalendarFuture<'a, bool> {
        Box::pin(async move {
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM pg_timezone_names WHERE name=$1::text)",
            )
            .bind(zone)
            .fetch_one(&mut *self.connection)
            .await
            .map_err(|_| calendar_unavailable())
        })
    }

    fn utc_offset_ms<'a>(
        &'a mut self,
        zone: &'a str,
        instant: i64,
    ) -> PreventiveCalendarFuture<'a, i64> {
        Box::pin(async move {
            let offset: f64 = sqlx::query_scalar(
                "SELECT (extract(epoch FROM \
                 ((to_timestamp($1::double precision / 1000.0) AT TIME ZONE $2::text) \
                 - (to_timestamp($1::double precision / 1000.0) AT TIME ZONE 'UTC'))) * 1000)::double precision",
            )
            .bind(instant)
            .bind(zone)
            .fetch_one(&mut *self.connection)
            .await
            .map_err(|_| calendar_unavailable())?;
            Ok(offset.round() as i64)
        })
    }

    fn local_minute<'a>(
        &'a mut self,
        zone: &'a str,
        instant: i64,
    ) -> PreventiveCalendarFuture<'a, String> {
        Box::pin(async move {
            sqlx::query_scalar(
                "SELECT to_char(to_timestamp($1::double precision / 1000.0) AT TIME ZONE $2::text, \
                 'YYYY-MM-DD\"T\"HH24:MI')",
            )
            .bind(instant)
            .bind(zone)
            .fetch_one(&mut *self.connection)
            .await
            .map_err(|_| calendar_unavailable())
        })
    }
}
