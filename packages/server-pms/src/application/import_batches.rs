//! Native PMS import-batch list use case.
//!
//! The repository is a bounded, read-only PMS port. Composition supplies the
//! Company/Property selection and the current session authority; this use case
//! applies the retained validation and ordering guards and projects the full
//! payload. No SQL, no lock and no cross-owner value is involved.
use crate::domain::import_batches::{
    PmsImportBatchFacts, PmsImportBatchRow, PmsImportBatchView, ensure_bounded, ensure_ordered,
    project_batches, validate,
};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type PmsImportBatchFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Read-only PMS import-batch repository. One method, no authority of its own.
pub trait PmsImportBatchRepository: Send {
    type Error: From<BusinessError> + Send;

    fn batches<'a>(
        &'a mut self,
        facts: &'a PmsImportBatchFacts,
    ) -> PmsImportBatchFuture<'a, Vec<PmsImportBatchRow>, Self::Error>;
}

/// Retained `pmsImportBatches` read. The produced rows are already projected to
/// the complete response shape; the caller does not append or reshape fields.
pub async fn pms_import_batches<R: PmsImportBatchRepository>(
    facts: &PmsImportBatchFacts,
    repository: &mut R,
) -> Result<Vec<PmsImportBatchView>, R::Error> {
    validate(facts)?;
    let rows = repository.batches(facts).await?;
    ensure_bounded(&rows)?;
    ensure_ordered(&rows)?;
    Ok(project_batches(rows)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::import_batches::MAX_BATCHES;
    use serde_json::json;

    struct Fake(Vec<PmsImportBatchRow>);

    impl PmsImportBatchRepository for Fake {
        type Error = BusinessError;

        fn batches<'a>(
            &'a mut self,
            _facts: &'a PmsImportBatchFacts,
        ) -> PmsImportBatchFuture<'a, Vec<PmsImportBatchRow>, Self::Error> {
            let rows = self.0.clone();
            Box::pin(async move { Ok(rows) })
        }
    }

    fn facts() -> PmsImportBatchFacts {
        PmsImportBatchFacts {
            company_id: "company".into(),
            property_id: "property-1".into(),
        }
    }

    fn row(id: &str, created_at: i64, report: &str) -> PmsImportBatchRow {
        PmsImportBatchRow {
            id: id.into(),
            cutover_date: "2026-11-01".into(),
            state: "preview".into(),
            version: 1,
            created_by: "manager".into(),
            approved_by: None,
            opening_journal_id: None,
            report_json: report.into(),
            created_at,
        }
    }

    #[tokio::test]
    async fn full_payload_parity_and_order() {
        let mut repo = Fake(vec![
            row("batch-2", 2, r#"{"counts":{"rooms":2}}"#),
            row("batch-1", 1, r#"{"counts":{"rooms":1}}"#),
        ]);
        let rows = pms_import_batches(&facts(), &mut repo).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "batch-2");
        assert_eq!(rows[1].id, "batch-1");
        let value = serde_json::to_value(&rows).unwrap();
        assert_eq!(value[0]["cutover_date"], "2026-11-01");
        assert_eq!(value[0]["state"], "preview");
        assert_eq!(value[0]["version"], 1);
        assert_eq!(value[0]["created_by"], "manager");
        assert_eq!(value[0]["report"], json!({"counts":{"rooms":2}}));
        assert!(value[0].get("report_json").is_none());
    }

    #[tokio::test]
    async fn foreign_property_or_empty_read_is_empty() {
        let mut repo = Fake(vec![]);
        assert!(pms_import_batches(&facts(), &mut repo)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn malformed_report_json_fails_closed() {
        let mut repo = Fake(vec![row("bad", 1, "{not json")]);
        assert_eq!(
            pms_import_batches(&facts(), &mut repo)
                .await
                .unwrap_err()
                .code,
            "PMS_IMPORT_BATCHES_READ_INVALID"
        );
    }

    #[tokio::test]
    async fn invalid_property_is_rejected() {
        let mut repo = Fake(vec![]);
        let bad = PmsImportBatchFacts {
            company_id: "company".into(),
            property_id: "x".repeat(161),
        };
        assert_eq!(
            pms_import_batches(&bad, &mut repo).await.unwrap_err().code,
            "PMS_IMPORT_BATCHES_REQUEST_INVALID"
        );
    }

    #[tokio::test]
    async fn unordered_and_oversized_reads_are_rejected() {
        let mut unordered = Fake(vec![row("a", 1, "{}"), row("b", 2, "{}")]);
        assert_eq!(
            pms_import_batches(&facts(), &mut unordered)
                .await
                .unwrap_err()
                .code,
            "PMS_IMPORT_BATCHES_READ_INVALID"
        );
        let rows = (0..=MAX_BATCHES)
            .map(|n| row(&format!("b-{n}"), MAX_BATCHES as i64 - n as i64 + 1, "{}"))
            .collect();
        let mut oversized = Fake(rows);
        assert_eq!(
            pms_import_batches(&facts(), &mut oversized)
                .await
                .unwrap_err()
                .code,
            "PMS_IMPORT_BATCHES_READ_INVALID"
        );
    }
}
