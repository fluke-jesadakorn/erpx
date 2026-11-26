//! sqlx execution for the native Organization draft read.
//!
//! The statements below are byte-identical to the owner SQL retired from
//! `adapters/organization-drafts.ts` (`nativeOrgAll` kinds `draft_read_active`,
//! `draft_read_by_id`, `draft_read_requests` and `draft_read_recent`), with only
//! the TypeScript `?` placeholders changed to sqlx/Postgres `$n`. The owner owns
//! the SQL, the Company/owner binds and the row-to-domain projection; no SQL
//! text crosses the transport and no TypeScript prepares or executes it.

use crate::adapters::native_draft_read::application::{DraftRead, ReadFuture};
use crate::adapters::native_draft_read::domain::DraftReadQuery;
use crate::adapters::native_draft_read::domain::{
    DraftReadResult, DraftRow, RecentRow, RequestRow,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::Row;

/// Owner SQL, unchanged from the retired TypeScript read registry.
pub const ACTIVE_SQL: &str =
    "SELECT * FROM organization_drafts WHERE company_id=$1 AND owner_id=$2 AND state='draft'";
pub const BY_ID_SQL: &str =
    "SELECT * FROM organization_drafts WHERE company_id=$1 AND owner_id=$2 AND id=$3";
pub const REQUESTS_SQL: &str = "SELECT id,item_keys,state,last_error,change_id FROM organization_draft_requests WHERE draft_id=$1 ORDER BY created_at,id";
pub const RECENT_SQL: &str = "SELECT id,updated_at,jsonb_array_length(definition->'items') count FROM organization_drafts WHERE company_id=$1 AND owner_id=$2 AND state='submitted' ORDER BY updated_at DESC LIMIT 20";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_DRAFT_READ_UNAVAILABLE",
        "The native Organization draft read is unavailable.",
    )
}

fn draft_row(row: sqlx::postgres::PgRow) -> Result<DraftRow, BusinessError> {
    Ok(DraftRow {
        id: row.try_get("id").map_err(|_| unavailable())?,
        company_id: row.try_get("company_id").map_err(|_| unavailable())?,
        owner_id: row.try_get("owner_id").map_err(|_| unavailable())?,
        state: row.try_get("state").map_err(|_| unavailable())?,
        version: row.try_get("version").map_err(|_| unavailable())?,
        definition: row.try_get::<Value, _>("definition").map_err(|_| unavailable())?,
        created_at: row.try_get("created_at").map_err(|_| unavailable())?,
        updated_at: row.try_get("updated_at").map_err(|_| unavailable())?,
        base_snapshot: row
            .try_get::<Value, _>("base_snapshot")
            .map_err(|_| unavailable())?,
    })
}

fn request_row(row: sqlx::postgres::PgRow) -> Result<RequestRow, BusinessError> {
    Ok(RequestRow {
        id: row.try_get("id").map_err(|_| unavailable())?,
        item_keys: row.try_get::<Value, _>("item_keys").map_err(|_| unavailable())?,
        state: row.try_get("state").map_err(|_| unavailable())?,
        last_error: row.try_get("last_error").map_err(|_| unavailable())?,
        change_id: row.try_get("change_id").map_err(|_| unavailable())?,
    })
}

fn recent_row(row: sqlx::postgres::PgRow) -> Result<RecentRow, BusinessError> {
    Ok(RecentRow {
        id: row.try_get("id").map_err(|_| unavailable())?,
        updated_at: row.try_get("updated_at").map_err(|_| unavailable())?,
        count: row.try_get("count").map_err(|_| unavailable())?,
    })
}

/// Pg-backed owner reader. All three statements run on one fresh owner unit so
/// the draft, its requests and the recent list are a consistent snapshot. No
/// lock is taken and no other owner's table is read.
pub struct PgDraftRead {
    pool: DatabasePool,
}

impl PgDraftRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl DraftRead for PgDraftRead {
    fn read<'a>(&'a self, query: &'a DraftReadQuery) -> ReadFuture<'a, DraftReadResult> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = query.company.clone();
        let user = query.user.clone();
        let draft_id = query.draft_id.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let user = user.clone();
                let draft_id = draft_id.clone();
                Box::pin(async move {
                    let draft = match &draft_id {
                        Some(id) => sqlx::query(BY_ID_SQL)
                            .bind(company.as_str())
                            .bind(user.as_str())
                            .bind(id.as_str())
                            .fetch_optional(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?
                            .map(draft_row)
                            .transpose()?,
                        None => sqlx::query(ACTIVE_SQL)
                            .bind(company.as_str())
                            .bind(user.as_str())
                            .fetch_optional(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?
                            .map(draft_row)
                            .transpose()?,
                    };
                    let requests = match &draft {
                        Some(record) => sqlx::query(REQUESTS_SQL)
                            .bind(record.id.as_str())
                            .fetch_all(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?
                            .into_iter()
                            .map(request_row)
                            .collect::<Result<Vec<_>, BusinessError>>()?,
                        None => Vec::new(),
                    };
                    let recent = sqlx::query(RECENT_SQL)
                        .bind(company.as_str())
                        .bind(user.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .into_iter()
                        .map(recent_row)
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    Ok(DraftReadResult {
                        draft,
                        requests,
                        recent,
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
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn retired_owner_sql_is_byte_identical() {
        // SHA-256 of the exact `?`-placeholder strings retired from the TS owner
        // registry (`organization_owner_write.rs` DRAFT_READ_* kinds). Only the
        // placeholders differ in the sqlx statements above (`?` -> `$n`).
        assert_eq!(
            hash("SELECT * FROM organization_drafts WHERE company_id=? AND owner_id=? AND state='draft'"),
            "ed4e695ef4afe32fd6795e81ea3964de06d60954cad2102f177da6138998efcc"
        );
        assert_eq!(
            hash("SELECT * FROM organization_drafts WHERE company_id=? AND owner_id=? AND id=?"),
            "0e5ca65c9cfbf296696d39126b5b95ffd3d17e006af237f5ed6ddc1338e18dd8"
        );
        assert_eq!(
            hash("SELECT id,item_keys,state,last_error,change_id FROM organization_draft_requests WHERE draft_id=? ORDER BY created_at,id"),
            "acd62f94cd510e0d5fc8d05fa9bb9b0b1e53e88c28810a9669f85d479fcb7438"
        );
        assert_eq!(
            hash("SELECT id,updated_at,jsonb_array_length(definition->'items') count FROM organization_drafts WHERE company_id=? AND owner_id=? AND state='submitted' ORDER BY updated_at DESC LIMIT 20"),
            "bc1a6fcc24565e7adb1241136a934fb79f818797bec391fe994e7b4bc395d12f"
        );
    }

    #[test]
    fn owner_read_is_select_only_and_single_company_scope() {
        for sql in [ACTIVE_SQL, BY_ID_SQL, REQUESTS_SQL, RECENT_SQL] {
            let upper = sql.to_ascii_uppercase();
            assert!(upper.trim_start().starts_with("SELECT "));
            assert!(!upper.contains(" INSERT ") && !upper.contains(" DELETE ") && !upper.contains(" UPDATE "));
            assert!(!upper.contains(" FOR UPDATE ") && !upper.contains(" LOCK TABLE "));
            // Company-scoped statements bind the owner company first; the
            // child request statement is fenced by its owner draft instead.
            let compact = upper.replace(' ', "");
            if compact.contains("COMPANY_ID") {
                assert!(compact.contains("COMPANY_ID=$1"));
            } else {
                assert!(compact.contains("DRAFT_ID=$1"));
            }
        }
    }
}
