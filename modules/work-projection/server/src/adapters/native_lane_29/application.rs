//! Application port and projection for the native lane 29 reads.
//!
//! The handler owns validation and the response shape. The injected reader is
//! the only component allowed to touch PostgreSQL; this module contains no SQL.

use erpx_platform_runtime_server::public::http_support::serde_json::{self, Value};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

use super::domain::{
    AnnouncementCandidate, AppointmentCandidate, AttendeeRow, AvailabilitySource, ConflictProbe,
    ConflictScope, MovedOccurrence, OccurrenceException, ReadKind, ReadRequest, SeriesRow, validate,
};

pub type Lane29Future<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner read port. The SQL, bind order and Company predicate live behind this
/// trait in `postgres.rs`; callers only supply already-validated identities.
pub trait Lane29Read: Send + Sync {
    fn occurrence_exception<'a>(
        &'a self,
        company: &'a str,
        appointment: &'a str,
        occurrence_key: &'a str,
    ) -> Lane29Future<'a, Option<OccurrenceException>>;

    fn moved_occurrences<'a>(
        &'a self,
        company: &'a str,
        appointment: &'a str,
    ) -> Lane29Future<'a, Vec<MovedOccurrence>>;

    fn availability_sources<'a>(
        &'a self,
        company: &'a str,
        before: i64,
        user_ids: &'a [String],
    ) -> Lane29Future<'a, Vec<AvailabilitySource>>;

    fn series_batch<'a>(
        &'a self,
        company: &'a str,
        appointment_ids: &'a [String],
    ) -> Lane29Future<'a, Vec<SeriesRow>>;

    fn conflict_probe<'a>(
        &'a self,
        company: &'a str,
        scope: ConflictScope,
        exclude: &'a str,
        before: i64,
        after: i64,
        ids: &'a [String],
    ) -> Lane29Future<'a, ConflictProbe>;

    fn appointment_candidates<'a>(&'a self, values: &'a [Value]) -> Lane29Future<'a, Vec<AppointmentCandidate>>;

    fn attendee_rows<'a>(&'a self, values: &'a [Value]) -> Lane29Future<'a, Vec<AttendeeRow>>;

    fn announcement_candidates<'a>(&'a self, values: &'a [Value]) -> Lane29Future<'a, Vec<AnnouncementCandidate>>;
}

/// Maps a validated request onto the owner port and pins the exact JSON shape
/// returned to the TypeScript transport. No field is dropped or synthesized.
pub struct Lane29Service<R: Lane29Read> {
    reader: R,
}

impl<R: Lane29Read> Lane29Service<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    pub async fn read(&self, request: ReadRequest) -> Result<Value, BusinessError> {
        validate(&request)?;
        match request.kind {
            ReadKind::OccurrenceException => {
                let exception = self
                    .reader
                    .occurrence_exception(
                        &request.company_id,
                        &request.appointment_id,
                        &request.occurrence_key,
                    )
                    .await?;
                Ok(serde_json::json!({ "exception": exception }))
            }
            ReadKind::MovedOccurrences => {
                let occurrences = self
                    .reader
                    .moved_occurrences(&request.company_id, &request.appointment_id)
                    .await?;
                Ok(serde_json::json!({ "occurrences": occurrences }))
            }
            ReadKind::AvailabilitySources => {
                let appointments = self
                    .reader
                    .availability_sources(&request.company_id, request.before, &request.user_ids)
                    .await?;
                Ok(serde_json::json!({ "appointments": appointments }))
            }
            ReadKind::SeriesBatch => {
                let series = self
                    .reader
                    .series_batch(&request.company_id, &request.appointment_ids)
                    .await?;
                Ok(serde_json::json!({ "series": series }))
            }
            ReadKind::ConflictProbe => {
                let probe = self
                    .reader
                    .conflict_probe(
                        &request.company_id,
                        request.conflict_scope,
                        &request.exclude_id,
                        request.before,
                        request.after,
                        &request.probe_ids,
                    )
                    .await?;
                Ok(serde_json::json!({ "conflict": probe.conflict }))
            }
            ReadKind::AppointmentCandidates => {
                let rows = self.reader.appointment_candidates(&request.values).await?;
                Ok(serde_json::json!({ "appointments": rows }))
            }
            ReadKind::AttendeeRows => {
                let rows = self.reader.attendee_rows(&request.values).await?;
                Ok(serde_json::json!({ "attendees": rows }))
            }
            ReadKind::AnnouncementCandidates => {
                let rows = self.reader.announcement_candidates(&request.values).await?;
                Ok(serde_json::json!({ "announcements": rows }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Stub;

    impl Lane29Read for Stub {
        fn occurrence_exception<'a>(
            &'a self,
            _company: &'a str,
            _appointment: &'a str,
            _key: &'a str,
        ) -> Lane29Future<'a, Option<OccurrenceException>> {
            Box::pin(async {
                Ok(Some(OccurrenceException {
                    start_local: Some("2026-11-25T09:00".into()),
                    end_local: Some("2026-11-25T10:00".into()),
                    cancelled: false,
                }))
            })
        }

        fn moved_occurrences<'a>(
            &'a self,
            _company: &'a str,
            _appointment: &'a str,
        ) -> Lane29Future<'a, Vec<MovedOccurrence>> {
            Box::pin(async {
                Ok(vec![MovedOccurrence {
                    occurrence_key: "2026-11-25T09:00".into(),
                    start_local: "2026-11-25T10:00".into(),
                    end_local: "2026-11-25T11:00".into(),
                }])
            })
        }

        fn series_batch<'a>(
            &'a self,
            _company: &'a str,
            _ids: &'a [String],
        ) -> Lane29Future<'a, Vec<SeriesRow>> {
            Box::pin(async {
                Ok(vec![SeriesRow {
                    appointment_id: "appointment-1".into(),
                    company_id: "company-1".into(),
                    frequency: "weekly".into(),
                    interval_count: 1,
                    weekdays: "1".into(),
                    end_kind: "never".into(),
                    occurrence_count: None,
                    until_date: None,
                    version: 1,
                }])
            })
        }

        fn appointment_candidates<'a>(
            &'a self,
            _values: &'a [Value],
        ) -> Lane29Future<'a, Vec<AppointmentCandidate>> {
            Box::pin(async {
                Ok(vec![AppointmentCandidate {
                    id: "appointment-1".into(),
                    company_id: "company-1".into(),
                    branch_id: None,
                    department_id: None,
                    organizer_person_id: Some("person-1".into()),
                    title: "Standup".into(),
                    description: String::new(),
                    starts_at: 100,
                    ends_at: 200,
                    timezone: "Asia/Bangkok".into(),
                    location: String::new(),
                    meeting_url: String::new(),
                    private: true,
                    guests: serde_json::json!([]),
                    status: "scheduled".into(),
                    version: 1,
                    created_by: "user-1".into(),
                    created_at: 1,
                    updated_at: 2,
                    all_day: false,
                    start_date: None,
                    end_date: None,
                    full_detail: true,
                    organizer_name: Some("Owner".into()),
                }])
            })
        }

        fn attendee_rows<'a>(
            &'a self,
            _values: &'a [Value],
        ) -> Lane29Future<'a, Vec<AttendeeRow>> {
            Box::pin(async {
                Ok(vec![AttendeeRow {
                    appointment_id: "appointment-1".into(),
                    user_id: "user-1".into(),
                    person_id: Some("person-1".into()),
                    name: Some("Attendee".into()),
                    in_scope: true,
                    attendance: "required".into(),
                    response: "accepted".into(),
                }])
            })
        }

        fn announcement_candidates<'a>(
            &'a self,
            _values: &'a [Value],
        ) -> Lane29Future<'a, Vec<AnnouncementCandidate>> {
            Box::pin(async {
                Ok(vec![AnnouncementCandidate {
                    id: "announcement-1".into(),
                    company_id: "company-1".into(),
                    branch_id: None,
                    department_id: None,
                    title: "Notice".into(),
                    content: "Body".into(),
                    start_date: "2026-11-25".into(),
                    end_date: "2026-11-26".into(),
                    status: "published".into(),
                    version: 1,
                    created_by: "user-1".into(),
                    created_at: 1,
                    updated_at: 2,
                    owner_name: "Publisher".into(),
                }])
            })
        }

        fn conflict_probe<'a>(
            &'a self,
            _company: &'a str,
            _scope: ConflictScope,
            _exclude: &'a str,
            _before: i64,
            _after: i64,
            _ids: &'a [String],
        ) -> Lane29Future<'a, ConflictProbe> {
            Box::pin(async {
                Ok(ConflictProbe {
                    conflict: true,
                })
            })
        }

        fn availability_sources<'a>(
            &'a self,
            _company: &'a str,
            _before: i64,
            _users: &'a [String],
        ) -> Lane29Future<'a, Vec<AvailabilitySource>> {
            Box::pin(async {
                Ok(vec![AvailabilitySource {
                    id: "appointment-1".into(),
                    starts_at: 100,
                    ends_at: 200,
                    timezone: "Asia/Bangkok".into(),
                    status: "scheduled".into(),
                    all_day: false,
                    frequency: Some("weekly".into()),
                    interval_count: Some(1),
                    weekdays: Some("1,2".into()),
                    end_kind: Some("never".into()),
                    occurrence_count: None,
                    until_date: None,
                    user_id: "user-1".into(),
                    response: "accepted".into(),
                }])
            })
        }
    }

    fn request(kind: ReadKind) -> ReadRequest {
        ReadRequest {
            kind,
            company_id: "company-1".into(),
            appointment_id: "appointment-1".into(),
            occurrence_key: "2026-11-25T09:00".into(),
            before: 1_764_000_000_000,
            user_ids: vec!["user-1".into()],
            appointment_ids: vec!["appointment-1".into()],
            exclude_id: "appointment-2".into(),
            after: 1_000,
            probe_ids: vec!["person-1".into()],
            conflict_scope: ConflictScope::Participant,
            values: Vec::new(),
        }
    }

    /// Bounded positional facts that satisfy domain validation; the Stub reader
    /// ignores them, but `Lane29Service::read` validates before the port call.
    fn candidate_values(kind: ReadKind) -> Vec<Value> {
        let scope = || serde_json::json!("[\"company-1\"]");
        let null = || Value::Null;
        match kind {
            ReadKind::AppointmentCandidates => vec![
                serde_json::json!(true), scope(), serde_json::json!("person-1"),
                serde_json::json!("user-1"), serde_json::json!("company-1"),
                serde_json::json!(100), serde_json::json!(200), serde_json::json!(false),
                null(), serde_json::json!("user-1"), scope(),
                serde_json::json!("mine"), serde_json::json!(50),
                null(), null(), null(), null(), null(), null(), null(),
            ],
            ReadKind::AttendeeRows => vec![scope(), scope()],
            ReadKind::AnnouncementCandidates => vec![
                serde_json::json!("company-1"), scope(), serde_json::json!(false),
                scope(), scope(), serde_json::json!("2026-01-01"),
                serde_json::json!("2026-12-31"), serde_json::json!(""),
                serde_json::json!(50), null(),
            ],
            _ => vec![],
        }
    }

    #[tokio::test]
    async fn pins_occurrence_projection() {
        let service = Lane29Service::new(Stub);
        let value = service
            .read(request(ReadKind::OccurrenceException))
            .await
            .unwrap();
        assert_eq!(
            value,
            serde_json::json!({"exception":{"start_local":"2026-11-25T09:00","end_local":"2026-11-25T10:00","cancelled":false}})
        );
    }

    #[tokio::test]
    async fn pins_moved_and_availability_projection() {
        let service = Lane29Service::new(Stub);
        let value = service
            .read(request(ReadKind::MovedOccurrences))
            .await
            .unwrap();
        assert_eq!(
            value,
            serde_json::json!({"occurrences":[{"occurrence_key":"2026-11-25T09:00","start_local":"2026-11-25T10:00","end_local":"2026-11-25T11:00"}]})
        );
        let value = service
            .read(request(ReadKind::AvailabilitySources))
            .await
            .unwrap();
        let appointments = value["appointments"].as_array().unwrap();
        assert_eq!(appointments.len(), 1);
        let row = &appointments[0];
        assert_eq!(row["id"], "appointment-1");
        assert_eq!(row["starts_at"], 100);
        assert_eq!(row["all_day"], false);
        assert_eq!(row["frequency"], "weekly");
        assert_eq!(row["user_id"], "user-1");
        assert_eq!(row.as_object().unwrap().len(), 14);
    }

    #[tokio::test]
    async fn pins_series_projection() {
        let service = Lane29Service::new(Stub);
        let value = service.read(request(ReadKind::SeriesBatch)).await.unwrap();
        let series = value["series"].as_array().unwrap();
        assert_eq!(series.len(), 1);
        assert_eq!(series[0]["appointment_id"], "appointment-1");
        assert_eq!(series[0].as_object().unwrap().len(), 9);
    }

    #[tokio::test]
    async fn pins_conflict_projection() {
        let service = Lane29Service::new(Stub);
        let value = service.read(request(ReadKind::ConflictProbe)).await.unwrap();
        assert_eq!(value, serde_json::json!({"conflict": true}));
    }

    #[tokio::test]
    async fn pins_candidate_list_projections() {
        let service = Lane29Service::new(Stub);
        for (kind, key) in [
            (ReadKind::AppointmentCandidates, "appointments"),
            (ReadKind::AttendeeRows, "attendees"),
            (ReadKind::AnnouncementCandidates, "announcements"),
        ] {
            let mut r = request(kind);
            r.values = candidate_values(kind);
            let value = service.read(r).await.unwrap();
            assert_eq!(value[key].as_array().unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn rejects_before_port_call() {
        let service = Lane29Service::new(Stub);
        let mut invalid = request(ReadKind::AvailabilitySources);
        invalid.user_ids.clear();
        assert!(service.read(invalid).await.is_err());
    }
}
