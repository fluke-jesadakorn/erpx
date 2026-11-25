//! Lane 35 application layer. The owner query port and the handler that
//! validates, executes and projects. No SQL and no transport in this module.

use crate::adapters::native_lane_35::domain::{
    CandidateRow, EventRow, LaneQuery, LaneResponse, SubmitTime, parse_ids, project_candidates,
    project_events, validate_company,
};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type LaneFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner-local query port. Composition may replace the Postgres adapter in
/// tests; the handler never builds SQL and never learns a connection.
pub trait WorkReportSource: Send + Sync {
    fn candidate_rows<'a>(&'a self, company: &'a str) -> LaneFuture<'a, Vec<CandidateRow>>;
    fn submit_times<'a>(
        &'a self,
        company: &'a str,
        ids: &'a [String],
    ) -> LaneFuture<'a, Vec<SubmitTime>>;
    fn event_rows<'a>(&'a self, company: &'a str, ids: &'a str) -> LaneFuture<'a, Vec<EventRow>>;
}

pub struct LaneQueryHandler<S: WorkReportSource> {
    source: S,
}

impl<S: WorkReportSource> LaneQueryHandler<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }

    pub async fn execute(&self, query: LaneQuery) -> Result<LaneResponse, BusinessError> {
        let company = query.company_id();
        validate_company(company)?;
        match query {
            LaneQuery::Candidates { company_id } => {
                let rows = self.source.candidate_rows(&company_id).await?;
                let ids = rows
                    .iter()
                    .map(|row| row.id.clone())
                    .collect::<Vec<_>>();
                let submits = if ids.is_empty() {
                    Vec::new()
                } else {
                    self.source.submit_times(&company_id, &ids).await?
                };
                Ok(LaneResponse::Candidates(project_candidates(rows, submits)))
            }
            LaneQuery::Events { company_id, ids } => {
                // Validate strictly before the value reaches a JSON cast.
                parse_ids(&ids)?;
                let rows = self.source.event_rows(&company_id, &ids).await?;
                Ok(LaneResponse::Events(project_events(rows)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_35::domain::{invalid, unavailable};

    struct Fake {
        candidates: Vec<CandidateRow>,
        submits: Vec<SubmitTime>,
        events: Vec<EventRow>,
        fail: bool,
    }

    impl WorkReportSource for Fake {
        fn candidate_rows<'a>(&'a self, company: &'a str) -> LaneFuture<'a, Vec<CandidateRow>> {
            let fail = self.fail || company != "acme";
            let rows = self.candidates.clone();
            Box::pin(async move {
                if fail {
                    Err(unavailable())
                } else {
                    Ok(rows)
                }
            })
        }
        fn submit_times<'a>(
            &'a self,
            _company: &'a str,
            _ids: &'a [String],
        ) -> LaneFuture<'a, Vec<SubmitTime>> {
            let rows = self.submits.clone();
            Box::pin(async move { Ok(rows) })
        }
        fn event_rows<'a>(&'a self, company: &'a str, ids: &'a str) -> LaneFuture<'a, Vec<EventRow>> {
            let fail = self.fail || company != "acme" || ids != "[\"r1\"]";
            let rows = self.events.clone();
            Box::pin(async move {
                if fail {
                    Err(invalid())
                } else {
                    Ok(rows)
                }
            })
        }
    }

    #[tokio::test]
    async fn handler_projects_candidates() {
        let source = Fake {
            candidates: vec![CandidateRow {
                approval_json: None,
                items_json: None,
                id: "r1".to_owned(),
                kind: "purchase".to_owned(),
                title: "t".to_owned(),
                state: "queued".to_owned(),
                requester_user_id: "u".to_owned(),
                branch_id: None,
                department_id: None,
                currency: None,
                created_at: 1,
                updated_at: 2,
            }],
            submits: vec![SubmitTime {
                request_id: "r1".to_owned(),
                submitted_at: 3,
            }],
            events: vec![],
            fail: false,
        };
        let handler = LaneQueryHandler::new(source);
        let response = handler
            .execute(LaneQuery::Candidates {
                company_id: "acme".to_owned(),
            })
            .await
            .unwrap();
        let value = response.value();
        assert_eq!(value["data"]["candidates"][0]["submitted_at"], 3);
    }

    #[tokio::test]
    async fn handler_fails_closed_on_rejected_ids() {
        let source = Fake {
            candidates: vec![],
            submits: vec![],
            events: vec![],
            fail: false,
        };
        let handler = LaneQueryHandler::new(source);
        let error = handler
            .execute(LaneQuery::Events {
                company_id: "acme".to_owned(),
                ids: "' OR TRUE --".to_owned(),
            })
            .await
            .unwrap_err();
        assert_eq!(error.code, "NATIVE_RECORDS_WORK_REPORT_REQUEST_INVALID");
    }
}
