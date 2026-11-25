//! sqlx execution for the native Files work-evidence read.
//!
//! The statements below are byte-identical to the two SELECTs retired from
//! `adapters/work-read-facts.ts`, except that `?` bind markers are replaced by
//! Postgres `$n` markers. Bind order is preserved: `$1` = record id, `$2` =
//! company id. The rollback flag selects between two complete SQL constants
//! owned here; no caller supplies SQL, a predicate or a relation name.

use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::Row;

use super::application::{WorkEvidenceFuture, WorkEvidenceRead};
use super::domain::{WorkEvidenceKind, WorkEvidenceResponse, WorkEvidenceRow};

/// Normal (draft+bound) evidence-set read.
pub const EVIDENCE_SETS_SQL: &str = "\
SELECT link.id,link.set_id,set_row.purpose_key,
             COALESCE(source.original_name,intent.original_name,'Pending file') original_name,
             COALESCE(source.bytes,intent.actual_bytes,intent.declared_bytes) bytes,
             COALESCE(source.detected_media_type,source.media_type,intent.detected_media_type,intent.media_type) media_type,
             source.source_state,link.state link_state,
             (source.preview_object_key IS NOT NULL) preview_available,
             link.created_at
           FROM attachment_sets set_row
           JOIN attachment_links link ON link.set_id=set_row.id
           LEFT JOIN attachments source
             ON source.id=link.attachment_id AND source.company_id=set_row.company_id
           LEFT JOIN upload_intents intent
             ON intent.id=link.upload_intent_id AND intent.company_id=set_row.company_id
           WHERE set_row.target_type='leave_request' AND set_row.target_id=$1
             AND set_row.company_id=$2
             AND (link.attachment_id IS NULL OR source.id IS NOT NULL)
             AND (link.upload_intent_id IS NULL OR intent.id IS NOT NULL)
             AND set_row.state IN ('draft','bound')
             AND link.state IN ('draft','bound')
             
           ORDER BY link.created_at DESC,link.id DESC";

/// Rollback (bound-only, available-only) evidence-set read.
pub const EVIDENCE_SETS_READ_ONLY_SQL: &str = "\
SELECT link.id,link.set_id,set_row.purpose_key,
             COALESCE(source.original_name,intent.original_name,'Pending file') original_name,
             COALESCE(source.bytes,intent.actual_bytes,intent.declared_bytes) bytes,
             COALESCE(source.detected_media_type,source.media_type,intent.detected_media_type,intent.media_type) media_type,
             source.source_state,link.state link_state,
             (source.preview_object_key IS NOT NULL) preview_available,
             link.created_at
           FROM attachment_sets set_row
           JOIN attachment_links link ON link.set_id=set_row.id
           LEFT JOIN attachments source
             ON source.id=link.attachment_id AND source.company_id=set_row.company_id
           LEFT JOIN upload_intents intent
             ON intent.id=link.upload_intent_id AND intent.company_id=set_row.company_id
           WHERE set_row.target_type='leave_request' AND set_row.target_id=$1
             AND set_row.company_id=$2
             AND (link.attachment_id IS NULL OR source.id IS NOT NULL)
             AND (link.upload_intent_id IS NULL OR intent.id IS NOT NULL)
             AND set_row.state='bound'
             AND link.state='bound'
             AND source.source_state='available'
           ORDER BY link.created_at DESC,link.id DESC";

/// Legacy attachment evidence read.
pub const LEGACY_EVIDENCE_SQL: &str = "\
SELECT id,NULL::text set_id,purpose purpose_key,original_name,bytes,COALESCE(detected_media_type,media_type) media_type,source_state,'bound' link_state,(preview_object_key IS NOT NULL) preview_available,created_at FROM attachments WHERE owner_type='leave_request' AND owner_id=$1 AND company_id=$2 AND revoked_at IS NULL ORDER BY created_at DESC";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "WORK_EVIDENCE_READ_UNAVAILABLE",
        "The native Files work-evidence service is unavailable.",
    )
}

fn project(row: sqlx::postgres::PgRow) -> Result<WorkEvidenceRow, BusinessError> {
    Ok(WorkEvidenceRow {
        id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
        set_id: row.try_get::<Option<String>, _>("set_id").map_err(|_| unavailable())?,
        purpose_key: row
            .try_get::<String, _>("purpose_key")
            .map_err(|_| unavailable())?,
        original_name: row
            .try_get::<String, _>("original_name")
            .map_err(|_| unavailable())?,
        bytes: row.try_get::<Option<i64>, _>("bytes").map_err(|_| unavailable())?,
        media_type: row
            .try_get::<Option<String>, _>("media_type")
            .map_err(|_| unavailable())?,
        source_state: row
            .try_get::<Option<String>, _>("source_state")
            .map_err(|_| unavailable())?,
        link_state: row
            .try_get::<String, _>("link_state")
            .map_err(|_| unavailable())?,
        preview_available: row
            .try_get::<bool, _>("preview_available")
            .map_err(|_| unavailable())?,
        created_at: row
            .try_get::<i64, _>("created_at")
            .map_err(|_| unavailable())?,
    })
}

/// Postgres-backed owner reader. Every statement is executed on the caller's
/// pool inside one bounded unit; no TypeScript code can issue these reads.
pub struct PgWorkEvidenceRead {
    pool: DatabasePool,
}

impl PgWorkEvidenceRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl WorkEvidenceRead for PgWorkEvidenceRead {
    fn read<'a>(
        &'a self,
        company: &'a str,
        record_id: &'a str,
        kind: WorkEvidenceKind,
        read_only: bool,
    ) -> WorkEvidenceFuture<'a, WorkEvidenceResponse> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let record_id = record_id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let record_id = record_id.clone();
                Box::pin(async move {
                    let sql = match kind {
                        WorkEvidenceKind::Sets if read_only => EVIDENCE_SETS_READ_ONLY_SQL,
                        WorkEvidenceKind::Sets => EVIDENCE_SETS_SQL,
                        WorkEvidenceKind::Legacy => LEGACY_EVIDENCE_SQL,
                    };
                    let rows = sqlx::query(sql)
                        .bind(record_id.as_str())
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let rows = rows
                        .into_iter()
                        .map(project)
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(WorkEvidenceResponse {
                        kind: kind.as_str().to_owned(),
                        rows,
                    })
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn hash(value: &str) -> String {
        Sha256::digest(value.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// SQL oracles: hashes of the exact rendered statements executed by sqlx.
    /// A change to any predicate, projection or bind position changes a hash.
    #[test]
    fn sql_oracle_sha256() {
        assert_eq!(
            hash(EVIDENCE_SETS_SQL),
            "ae52cad228e444d88686254b19ca7e980c19c489e5d52de8bec8ac374fa185e4"
        );
        assert_eq!(
            hash(EVIDENCE_SETS_READ_ONLY_SQL),
            "e376be2ebee0b6c176045f3aa67098be4fbc4137806a055ca5e7fb441daa426c"
        );
        assert_eq!(
            hash(LEGACY_EVIDENCE_SQL),
            "cc8a48cf37fc9785374fd62da249df3f017675797d3901b5b6523e5608d207a4"
        );
    }

    /// Bind order and relation ownership are pinned independently of the SQL
    /// text: the record id binds first, the Company scope second.
    #[test]
    fn every_statement_binds_record_then_company_exactly_once() {
        for sql in [
            EVIDENCE_SETS_SQL,
            EVIDENCE_SETS_READ_ONLY_SQL,
            LEGACY_EVIDENCE_SQL,
        ] {
            assert_eq!(sql.matches("$1").count(), 1, "record bind");
            assert_eq!(sql.matches("$2").count(), 1, "company bind");
            assert!(!sql.contains('?'), "no untyped bind markers");
        }
        assert!(EVIDENCE_SETS_SQL.contains("set_row.company_id=$2"));
        assert!(EVIDENCE_SETS_READ_ONLY_SQL.contains("AND source.source_state='available'"));
        assert!(LEGACY_EVIDENCE_SQL.contains("owner_id=$1 AND company_id=$2"));
    }
}
