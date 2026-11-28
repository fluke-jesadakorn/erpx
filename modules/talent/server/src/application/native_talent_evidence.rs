//! Application query and owner port for the native Talent evidence read.
//!
//! SQL, connections and row mapping stay in the Postgres adapter; this layer
//! owns validation and the read-only query boundary. There is no command key
//! and no write ledger: the read is one snapshot of an owner row.

use crate::domain::native_talent_evidence::{
    EvidenceMetric, invalid, range_required, validate_id, validate_optional_scope,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;
use std::{future::Future, pin::Pin, sync::Arc};

pub type EvidenceFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner port injected by composition. The reader executes the owner-authored
/// SELECT and returns the raw owner row; there is no TypeScript preparation or
/// execution behind this port.
pub trait TalentEvidenceRead: Send + Sync {
    fn evidence<'a>(&'a self, query: &'a EvidenceQuery) -> EvidenceFuture<'a, Option<Value>>;
}

/// The update window for `application-stage-counts`, in epoch milliseconds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvidenceRange {
    pub from: i64,
    pub to_exclusive: i64,
}

/// Read-only query carrying only validated wire facts. `company_id`, `branch_id`
/// and `department_id` are the caller's already-resolved scope facts; the owner
/// does not mint scope, it only binds them into the owner SELECT.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceQuery {
    pub metric: EvidenceMetric,
    pub record_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub range: Option<EvidenceRange>,
}

impl EvidenceQuery {
    pub fn new(
        metric: EvidenceMetric,
        record_id: &str,
        company_id: &str,
        branch_id: Option<&str>,
        department_id: Option<&str>,
        range: Option<EvidenceRange>,
    ) -> Result<Self, BusinessError> {
        validate_id(record_id)?;
        validate_optional_scope(Some(company_id))?;
        validate_optional_scope(branch_id)?;
        validate_optional_scope(department_id)?;
        if company_id.is_empty() {
            return Err(invalid());
        }
        if metric.requires_range() && range.is_none() {
            return Err(range_required());
        }
        if let Some(value) = range {
            if value.from < 0 || value.to_exclusive < value.from {
                return Err(invalid());
            }
        }
        Ok(Self {
            metric,
            record_id: record_id.to_owned(),
            company_id: company_id.to_owned(),
            branch_id: branch_id.map(str::to_owned),
            department_id: department_id.map(str::to_owned),
            range,
        })
    }
}

#[derive(Clone)]
pub struct EvidenceHandler {
    reader: Arc<dyn TalentEvidenceRead>,
}

impl EvidenceHandler {
    pub fn new(reader: Arc<dyn TalentEvidenceRead>) -> Self {
        Self { reader }
    }

    pub fn handle<'a>(&'a self, query: EvidenceQuery) -> EvidenceFuture<'a, Option<Value>> {
        let reader = self.reader.clone();
        Box::pin(async move { reader.evidence(&query).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Probe;

    impl TalentEvidenceRead for Probe {
        fn evidence<'a>(&'a self, query: &'a EvidenceQuery) -> EvidenceFuture<'a, Option<Value>> {
            let id = query.record_id.clone();
            let metric = query.metric;
            Box::pin(async move {
                if id == "app-missing" {
                    return Ok(None);
                }
                Ok(Some(if metric == EvidenceMetric::OpenRequisitions {
                    serde_json::json!({
                        "id": id, "job_title": "Engineer", "status": "open",
                        "version": 1, "updated_at": 1_800_000_000_000i64,
                        "branch_id": "branch-1", "department_id": null
                    })
                } else {
                    serde_json::json!({
                        "id": id, "requisition_id": "req-1", "status": "applied",
                        "version": 2, "updated_at": 1_800_000_000_000i64,
                        "job_title": "Engineer", "branch_id": "branch-1",
                        "department_id": null
                    })
                }))
            })
        }
    }

    #[test]
    fn query_validates_every_wire_fact_before_reaching_the_owner() {
        let range = Some(EvidenceRange { from: 1, to_exclusive: 2 });
        assert!(EvidenceQuery::new(
            EvidenceMetric::OpenRequisitions,
            "req-1",
            "company-1",
            None,
            None,
            None,
        )
        .is_ok());
        assert!(EvidenceQuery::new(
            EvidenceMetric::ApplicationStageCounts,
            "app-1",
            "company-1",
            Some("branch-1"),
            None,
            range,
        )
        .is_ok());
        assert!(EvidenceQuery::new(
            EvidenceMetric::ApplicationStageCounts,
            "app-1",
            "company-1",
            None,
            None,
            None,
        )
        .is_err());
        assert!(EvidenceQuery::new(
            EvidenceMetric::OpenRequisitions,
            "bad id!",
            "company-1",
            None,
            None,
            None,
        )
        .is_err());
        assert!(EvidenceQuery::new(
            EvidenceMetric::OpenRequisitions,
            "req-1",
            "",
            None,
            None,
            None,
        )
        .is_err());
        assert!(EvidenceQuery::new(
            EvidenceMetric::ApplicationStageCounts,
            "app-1",
            "company-1",
            None,
            None,
            Some(EvidenceRange { from: 2, to_exclusive: 1 }),
        )
        .is_err());
    }

    #[tokio::test]
    async fn handler_returns_the_complete_owner_row_without_derivation() {
        let handler = EvidenceHandler::new(Arc::new(Probe));
        let query = EvidenceQuery::new(
            EvidenceMetric::OpenRequisitions,
            "req-1",
            "company-1",
            None,
            None,
            None,
        )
        .unwrap();
        let row = handler.handle(query).await.unwrap().expect("row");
        assert_eq!(row["id"], "req-1");
        assert_eq!(row["job_title"], "Engineer");
        assert_eq!(row["status"], "open");
        assert_eq!(row["version"], 1);
        assert_eq!(row["branch_id"], "branch-1");
        assert!(row["department_id"].is_null());
    }

    #[tokio::test]
    async fn handler_returns_none_for_a_missing_row() {
        let handler = EvidenceHandler::new(Arc::new(Probe));
        let query = EvidenceQuery::new(
            EvidenceMetric::ApplicationStageCounts,
            "app-missing",
            "company-1",
            None,
            None,
            Some(EvidenceRange { from: 1, to_exclusive: 2 }),
        )
        .unwrap();
        assert!(handler.handle(query).await.unwrap().is_none());
    }
}
