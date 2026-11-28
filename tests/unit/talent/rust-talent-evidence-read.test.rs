//! Native Talent executive-evidence point-read public-surface tests.
//!
//! The production SQL is executed only by the sqlx adapter; this file pins the
//! stable owner port and the exact wire projection against a probe reader.
use erpx_talent_server::public::native_talent_evidence::{
    EVIDENCE_APPLICATION_SQL, EVIDENCE_REQUISITION_SQL, EvidenceFuture, EvidenceHandler,
    EvidenceMetric, EvidenceQuery, EvidenceRange, TalentEvidence, TalentEvidenceRead,
};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone)]
struct Probe;

impl TalentEvidenceRead for Probe {
    fn evidence<'a>(&'a self, query: &'a EvidenceQuery) -> EvidenceFuture<'a, Option<Value>> {
        let id = query.record_id.clone();
        let metric = query.metric;
        let missing = id == "req-missing";
        Box::pin(async move {
            if missing {
                return Ok(None);
            }
            Ok(Some(if metric == EvidenceMetric::OpenRequisitions {
                json!({"id": id, "status": "open", "version": 1})
            } else {
                json!({"id": id, "status": "applied", "version": 2})
            }))
        })
    }
}

#[tokio::test]
async fn owner_row_payload_keeps_every_projected_field() {
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
    let body = serde_json::to_value(TalentEvidence { data: row }).unwrap();
    assert_eq!(body["data"]["id"], "req-1");
    assert_eq!(body["data"]["status"], "open");
    assert_eq!(body["data"]["version"], 1);
    assert_eq!(body.as_object().unwrap().len(), 1);
}

#[tokio::test]
async fn missing_row_is_null_and_range_is_bound() {
    let handler = EvidenceHandler::new(Arc::new(Probe));
    let missing = EvidenceQuery::new(
        EvidenceMetric::OpenRequisitions,
        "req-missing",
        "company-1",
        None,
        None,
        None,
    )
    .unwrap();
    let body = serde_json::to_value(TalentEvidence {
        data: handler.handle(missing).await.unwrap().unwrap_or(Value::Null),
    })
    .unwrap();
    assert!(body["data"].is_null());

    let ranged = EvidenceQuery::new(
        EvidenceMetric::ApplicationStageCounts,
        "app-1",
        "company-1",
        Some("branch-1"),
        None,
        Some(EvidenceRange { from: 10, to_exclusive: 20 }),
    )
    .unwrap();
    let row = handler.handle(ranged).await.unwrap().expect("row");
    assert_eq!(row["status"], "applied");
}

#[test]
fn owner_statements_are_select_only_with_postgres_binds() {
    for sql in [EVIDENCE_REQUISITION_SQL, EVIDENCE_APPLICATION_SQL] {
        assert!(sql.starts_with("SELECT "));
        assert!(!sql.contains('?'));
        assert!(sql.contains("talent_"));
        assert!(!sql.contains("INSERT"));
        assert!(!sql.contains("UPDATE "));
        assert!(!sql.contains("DELETE"));
    }
}
