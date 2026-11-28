//! sqlx execution for the Files commercial-usage and tax-signature read families.
//!
//! Statements start byte-identical to the retired TypeScript literals in
//! `adapters/commercial-usage-facts.ts` (the `*_SQL` oracle constants); bind
//! order and predicates are preserved. The `*_PG_SQL` execution constants are
//! the same text with `?` rewritten to sqlx Postgres `$n` placeholders —
//! sqlx only rewrites `?` for the `Any` driver, not for a direct Pg pool.
//! Only the owner's Pg pool and this adapter can render them.

use crate::adapters::native_lane_21::application::{
    CommercialUsageRead, ReviewFilesRead, TaxSignatureRead, UsageReadFuture,
};
use crate::adapters::native_lane_21::domain::{
    ReviewFileRow, ReviewFilesFacts, ReviewFilesRequest, StorageRow, TaxSignatureFacts,
    TaxSignatureMetadata, TaxSignatureReadKind, TaxSignatureRequest, TaxSignatureRow, UsageFacts,
    UsageReadKind, UsageReadRequest, review_unavailable, tax_unavailable, unavailable,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::Row;

pub const STORED_ORIGINALS_SQL: &str = "SELECT l.id,l.company_id,s.branch_id,l.bytes,l.created_at,l.destroyed_at\n      FROM attachment_storage_ledger l\n      JOIN attachments a ON a.id=l.attachment_id\n      LEFT JOIN upload_intents u ON u.id=a.upload_id\n      LEFT JOIN attachment_sets s ON s.id=u.attachment_set_id AND s.company_id=l.company_id\n      WHERE l.variant='original' AND l.state IN ('stored','destroyed')\n        AND l.created_at<? AND (l.destroyed_at IS NULL OR l.destroyed_at>?)\n        AND (?::text IS NULL OR s.branch_id=?)\n      ORDER BY l.created_at DESC,l.id DESC";

pub const UNRECONCILED_ORIGINALS_SQL: &str = "SELECT COUNT(*)::int AS count FROM attachments a\n      LEFT JOIN upload_intents u ON u.id=a.upload_id\n      LEFT JOIN attachment_sets s ON s.id=u.attachment_set_id AND s.company_id=a.company_id\n      WHERE a.source_state<>'destroyed' AND a.created_at<?\n        AND NOT EXISTS(SELECT 1 FROM attachment_storage_ledger l WHERE l.attachment_id=a.id AND l.variant='original')\n        AND (?::text IS NULL OR s.branch_id=?)";

pub const RECONCILIATION_SQL: &str =
    "SELECT ready FROM attachment_reconciliation_runs ORDER BY created_at DESC LIMIT 1";

pub const EARLIEST_ORIGINAL_SQL: &str =
    "SELECT MIN(created_at) earliest FROM attachment_storage_ledger WHERE variant='original'";

// Execution text: identical predicates, projection, ordering and bind order;
// only `?` -> `$n` differs so sqlx can prepare against a direct Pg pool.
const STORED_ORIGINALS_PG_SQL: &str = "SELECT l.id,l.company_id,s.branch_id,l.bytes,l.created_at,l.destroyed_at\n      FROM attachment_storage_ledger l\n      JOIN attachments a ON a.id=l.attachment_id\n      LEFT JOIN upload_intents u ON u.id=a.upload_id\n      LEFT JOIN attachment_sets s ON s.id=u.attachment_set_id AND s.company_id=l.company_id\n      WHERE l.variant='original' AND l.state IN ('stored','destroyed')\n        AND l.created_at<$1 AND (l.destroyed_at IS NULL OR l.destroyed_at>$2)\n        AND ($3::text IS NULL OR s.branch_id=$4)\n      ORDER BY l.created_at DESC,l.id DESC";

const UNRECONCILED_ORIGINALS_PG_SQL: &str = "SELECT COUNT(*)::int AS count FROM attachments a\n      LEFT JOIN upload_intents u ON u.id=a.upload_id\n      LEFT JOIN attachment_sets s ON s.id=u.attachment_set_id AND s.company_id=a.company_id\n      WHERE a.source_state<>'destroyed' AND a.created_at<$1\n        AND NOT EXISTS(SELECT 1 FROM attachment_storage_ledger l WHERE l.attachment_id=a.id AND l.variant='original')\n        AND ($2::text IS NULL OR s.branch_id=$3)";

/// Pg-backed owner reader. No other crate supplies a `CommercialUsageRead`.
pub struct PgCommercialUsageRead {
    pool: DatabasePool,
}

impl PgCommercialUsageRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl CommercialUsageRead for PgCommercialUsageRead {
    fn read<'a>(
        &'a self,
        company: Option<&'a str>,
        kind: UsageReadKind,
        request: &'a UsageReadRequest,
    ) -> UsageReadFuture<'a, UsageFacts> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        // These reads are holding-wide: the pool is the authority boundary and
        // a company scope is optional. When supplied it must still be bounded.
        let company = company.map(str::to_owned);
        let start = request.start;
        let end = request.end;
        let branch_id = request.branch_id.clone();
        Box::pin(async move {
            if company
                .as_deref()
                .is_some_and(|value| value.is_empty() || value.len() > 200)
            {
                return Err(unavailable());
            }
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let branch_id = branch_id.clone();
                Box::pin(async move {
                    // Keep the caller scope observable inside the unit. The
                    // statements below are holding-scoped by the pool itself;
                    // no SQL is accepted from the transport.
                    let _scope = company.as_deref();
                    match kind {
                        UsageReadKind::StoredOriginals => {
                            let end = end.ok_or_else(unavailable)?;
                            let start = start.ok_or_else(unavailable)?;
                            let branch = branch_id.as_deref();
                            let rows = sqlx::query(STORED_ORIGINALS_PG_SQL)
                                .bind(end)
                                .bind(start)
                                .bind(branch)
                                .bind(branch)
                                .fetch_all(&mut *connection)
                                .await
                                .map_err(|_| unavailable())?;
                            let rows = rows
                                .into_iter()
                                .map(|r| {
                                    Ok(StorageRow {
                                        id: r
                                            .try_get::<String, _>("id")
                                            .map_err(|_| unavailable())?,
                                        company_id: r
                                            .try_get::<String, _>("company_id")
                                            .map_err(|_| unavailable())?,
                                        branch_id: r
                                            .try_get::<Option<String>, _>("branch_id")
                                            .map_err(|_| unavailable())?,
                                        bytes: r
                                            .try_get::<i64, _>("bytes")
                                            .map_err(|_| unavailable())?,
                                        created_at: r
                                            .try_get::<i64, _>("created_at")
                                            .map_err(|_| unavailable())?,
                                        destroyed_at: r
                                            .try_get::<Option<i64>, _>("destroyed_at")
                                            .map_err(|_| unavailable())?,
                                    })
                                })
                                .collect::<Result<Vec<_>, BusinessError>>()?;
                            Ok(UsageFacts::StoredOriginals { rows })
                        }
                        UsageReadKind::UnreconciledOriginals => {
                            let end = end.ok_or_else(unavailable)?;
                            let branch = branch_id.as_deref();
                            let count = sqlx::query(UNRECONCILED_ORIGINALS_PG_SQL)
                                .bind(end)
                                .bind(branch)
                                .bind(branch)
                                .fetch_one(&mut *connection)
                                .await
                                .map_err(|_| unavailable())?
                                .try_get::<i32, _>("count")
                                .map_err(|_| unavailable())?;
                            Ok(UsageFacts::UnreconciledOriginals { count })
                        }
                        UsageReadKind::Reconciliation => {
                            let ready = sqlx::query(RECONCILIATION_SQL)
                                .fetch_optional(&mut *connection)
                                .await
                                .map_err(|_| unavailable())?
                                .map(|r| r.try_get::<bool, _>("ready"))
                                .transpose()
                                .map_err(|_| unavailable())?
                                .unwrap_or(false);
                            Ok(UsageFacts::Reconciliation { ready })
                        }
                        UsageReadKind::EarliestOriginal => {
                            let earliest = sqlx::query(EARLIEST_ORIGINAL_SQL)
                                .fetch_one(&mut *connection)
                                .await
                                .map_err(|_| unavailable())?
                                .try_get::<Option<i64>, _>("earliest")
                                .map_err(|_| unavailable())?;
                            Ok(UsageFacts::EarliestOriginal { earliest })
                        }
                    }
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

// --- Finance tax-signature reads ------------------------------------------

/// Byte-identical to the retired TypeScript literals; the execution text below
/// only rewrites `?` to sqlx Postgres `$n`.
#[allow(dead_code)] // byte-oracle evidence for the contract test, not executed directly
pub const TAX_METADATA_SQL: &str = "SELECT a.checksum_sha256,a.media_type,a.bytes FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id\n      WHERE l.id=? AND s.company_id=? AND s.target_type='finance_tax_identity' AND s.target_id=? AND s.purpose_key='signer_signature' AND s.component_kind='record' AND s.state='bound' AND l.state='bound' AND a.company_id=s.company_id AND a.source_state='available'";
#[allow(dead_code)] // byte-oracle evidence for the contract test, not executed directly
pub const TAX_WORKSPACE_SQL: &str = "SELECT l.id,s.target_id,a.original_name,a.checksum_sha256 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id WHERE s.company_id=? AND s.target_type='finance_tax_identity' AND s.purpose_key='signer_signature' AND s.component_kind='record' AND s.state='bound' AND l.state='bound' AND a.source_state='available'";

const TAX_METADATA_PG_SQL: &str = "SELECT a.checksum_sha256,a.media_type,a.bytes FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id\n      WHERE l.id=$1 AND s.company_id=$2 AND s.target_type='finance_tax_identity' AND s.target_id=$3 AND s.purpose_key='signer_signature' AND s.component_kind='record' AND s.state='bound' AND l.state='bound' AND a.company_id=s.company_id AND a.source_state='available'";
const TAX_WORKSPACE_PG_SQL: &str = "SELECT l.id,s.target_id,a.original_name,a.checksum_sha256 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id WHERE s.company_id=$1 AND s.target_type='finance_tax_identity' AND s.purpose_key='signer_signature' AND s.component_kind='record' AND s.state='bound' AND l.state='bound' AND a.source_state='available'";

pub struct PgTaxSignatureRead {
    pool: DatabasePool,
}

impl PgTaxSignatureRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl TaxSignatureRead for PgTaxSignatureRead {
    fn read<'a>(
        &'a self,
        company: &'a str,
        kind: TaxSignatureReadKind,
        request: &'a TaxSignatureRequest,
    ) -> UsageReadFuture<'a, TaxSignatureFacts> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let id = request.id.clone();
        let link_id = request.link_id.clone();
        Box::pin(async move {
            if company.is_empty() || company.len() > 200 {
                return Err(tax_unavailable());
            }
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let id = id.clone();
                let link_id = link_id.clone();
                Box::pin(async move {
                    match kind {
                        TaxSignatureReadKind::Metadata => {
                            let id = id.ok_or_else(tax_unavailable)?;
                            let link = link_id.ok_or_else(tax_unavailable)?;
                            let row = sqlx::query(TAX_METADATA_PG_SQL)
                                .bind(link.as_str())
                                .bind(company.as_str())
                                .bind(id.as_str())
                                .fetch_optional(&mut *connection)
                                .await
                                .map_err(|_| tax_unavailable())?;
                            let metadata = row
                                .map(|r| -> Result<TaxSignatureMetadata, BusinessError> {
                                    Ok(TaxSignatureMetadata {
                                        checksum_sha256: r
                                            .try_get::<String, _>("checksum_sha256")
                                            .map_err(|_| tax_unavailable())?,
                                        media_type: r
                                            .try_get::<String, _>("media_type")
                                            .map_err(|_| tax_unavailable())?,
                                        bytes: r
                                            .try_get::<i64, _>("bytes")
                                            .map_err(|_| tax_unavailable())?,
                                    })
                                })
                                .transpose()?;
                            Ok(TaxSignatureFacts::Metadata { metadata })
                        }
                        TaxSignatureReadKind::Workspace => {
                            let rows = sqlx::query(TAX_WORKSPACE_PG_SQL)
                                .bind(company.as_str())
                                .fetch_all(&mut *connection)
                                .await
                                .map_err(|_| tax_unavailable())?;
                            let rows = rows
                                .into_iter()
                                .map(|r| {
                                    Ok(TaxSignatureRow {
                                        id: r
                                            .try_get::<String, _>("id")
                                            .map_err(|_| tax_unavailable())?,
                                        target_id: r
                                            .try_get::<String, _>("target_id")
                                            .map_err(|_| tax_unavailable())?,
                                        original_name: r
                                            .try_get::<String, _>("original_name")
                                            .map_err(|_| tax_unavailable())?,
                                        checksum_sha256: r
                                            .try_get::<String, _>("checksum_sha256")
                                            .map_err(|_| tax_unavailable())?,
                                    })
                                })
                                .collect::<Result<Vec<_>, BusinessError>>()?;
                            Ok(TaxSignatureFacts::Workspace { rows })
                        }
                    }
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => tax_unavailable(),
            })
        })
    }
}

// --- Finance review-file list ---------------------------------------------

#[allow(dead_code)] // byte-oracle evidence for the contract test, not executed directly
pub const REVIEW_FILES_SQL: &str = "SELECT l.id link_id,l.attachment_id source_id,l.state link_state,s.state set_state,s.purpose_key,a.checksum_sha256,a.media_type,a.bytes,a.source_state\n FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id\n WHERE s.company_id=? AND s.target_type=? AND s.target_id=? AND l.state IN ('draft','bound') ORDER BY s.purpose_key,l.id";

const REVIEW_FILES_PG_SQL: &str = "SELECT l.id link_id,l.attachment_id source_id,l.state link_state,s.state set_state,s.purpose_key,a.checksum_sha256,a.media_type,a.bytes,a.source_state\n FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id\n WHERE s.company_id=$1 AND s.target_type=$2 AND s.target_id=$3 AND l.state IN ('draft','bound') ORDER BY s.purpose_key,l.id";

pub struct PgReviewFilesRead {
    pool: DatabasePool,
}

impl PgReviewFilesRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl ReviewFilesRead for PgReviewFilesRead {
    fn read<'a>(
        &'a self,
        company: &'a str,
        request: &'a ReviewFilesRequest,
    ) -> UsageReadFuture<'a, ReviewFilesFacts> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let target_type = request.target_type.clone();
        let target_id = request.target_id.clone();
        Box::pin(async move {
            if company.is_empty() || company.len() > 200 {
                return Err(review_unavailable());
            }
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let target_type = target_type.clone();
                let target_id = target_id.clone();
                Box::pin(async move {
                    let rows = sqlx::query(REVIEW_FILES_PG_SQL)
                        .bind(company.as_str())
                        .bind(target_type.as_str())
                        .bind(target_id.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| review_unavailable())?;
                    let rows = rows
                        .into_iter()
                        .map(|r| {
                            Ok(ReviewFileRow {
                                link_id: r.try_get::<String, _>("link_id").map_err(|_| review_unavailable())?,
                                source_id: r
                                    .try_get::<Option<String>, _>("source_id")
                                    .map_err(|_| review_unavailable())?,
                                link_state: r
                                    .try_get::<String, _>("link_state")
                                    .map_err(|_| review_unavailable())?,
                                set_state: r
                                    .try_get::<String, _>("set_state")
                                    .map_err(|_| review_unavailable())?,
                                purpose_key: r
                                    .try_get::<String, _>("purpose_key")
                                    .map_err(|_| review_unavailable())?,
                                checksum_sha256: r
                                    .try_get::<Option<String>, _>("checksum_sha256")
                                    .map_err(|_| review_unavailable())?,
                                media_type: r
                                    .try_get::<Option<String>, _>("media_type")
                                    .map_err(|_| review_unavailable())?,
                                bytes: r.try_get::<Option<i64>, _>("bytes").map_err(|_| review_unavailable())?,
                                source_state: r
                                    .try_get::<Option<String>, _>("source_state")
                                    .map_err(|_| review_unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    Ok(ReviewFilesFacts::ReviewFiles { rows })
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => review_unavailable(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn hash(sql: &str) -> String {
        hex::encode(Sha256::digest(sql.as_bytes()))
    }

    /// Rendered-SQL oracle: any byte change to the ported literals is caught.
    #[test]
    fn pins_rendered_sql_oracles() {
        assert_eq!(
            hash(STORED_ORIGINALS_SQL),
            "4a3af19994e9d01af11ec5a82a3480d16eeb8537a1c17b9c9f245f22339b4762"
        );
        assert_eq!(
            hash(UNRECONCILED_ORIGINALS_SQL),
            "824715ff1ace2188416ed4d3dc13ef013e4709d2b82f73eb23ea0281f093f279"
        );
        assert_eq!(
            hash(RECONCILIATION_SQL),
            "53484a30d6df87142910df0b0a1449e150671ac6cc5bce28360286f920da6eea"
        );
        assert_eq!(
            hash(EARLIEST_ORIGINAL_SQL),
            "2034c1b593d2f47ff60a5f3eba5f12f32477731d33daec817740033045821fc3"
        );
        // Distinct statements never collide in the oracle set.
        let set = [
            hash(STORED_ORIGINALS_SQL),
            hash(UNRECONCILED_ORIGINALS_SQL),
            hash(RECONCILIATION_SQL),
            hash(EARLIEST_ORIGINAL_SQL),
        ];
        for (i, left) in set.iter().enumerate() {
            for right in set.iter().skip(i + 1) {
                assert_ne!(left, right);
            }
        }
    }

    fn normalize(sql: &str) -> String {
        let bytes = sql.as_bytes();
        let mut out = String::with_capacity(sql.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'$' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                out.push('?');
                i += 2;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            } else {
                out.push(bytes[i] as char);
                i += 1;
            }
        }
        out
    }

    /// The execution text is the byte-identical oracle with only the
    /// placeholder spelling changed; no predicate, projection or bind moves.
    #[test]
    fn pg_text_matches_oracle_after_placeholder_normalization() {
        assert_eq!(normalize(STORED_ORIGINALS_PG_SQL), STORED_ORIGINALS_SQL);
        assert_eq!(
            normalize(UNRECONCILED_ORIGINALS_PG_SQL),
            UNRECONCILED_ORIGINALS_SQL
        );
        assert_eq!(normalize(RECONCILIATION_SQL), RECONCILIATION_SQL);
        assert_eq!(normalize(EARLIEST_ORIGINAL_SQL), EARLIEST_ORIGINAL_SQL);
    }

    #[test]
    fn statements_are_select_only_and_owner_local() {
        for sql in [
            STORED_ORIGINALS_SQL,
            UNRECONCILED_ORIGINALS_SQL,
            RECONCILIATION_SQL,
            EARLIEST_ORIGINAL_SQL,
        ] {
            assert!(sql.trim_start().to_ascii_uppercase().starts_with("SELECT"));
            assert!(!sql.contains("LOCK TABLE"));
            assert!(!sql.contains("pg_advisory"));
        }
        assert!(STORED_ORIGINALS_SQL.contains("attachment_storage_ledger"));
        assert!(UNRECONCILED_ORIGINALS_SQL.contains("attachment_storage_ledger"));
        assert!(RECONCILIATION_SQL.contains("attachment_reconciliation_runs"));
        assert!(EARLIEST_ORIGINAL_SQL.contains("attachment_storage_ledger"));
    }

    #[test]
    fn pins_review_file_sql_oracle_and_placeholders() {
        assert_eq!(
            hash(REVIEW_FILES_SQL),
            "d89356c560e5e94faf920278fc68624d8e5990768be221e335f63c258e20c6a1"
        );
        assert_eq!(normalize(REVIEW_FILES_PG_SQL), REVIEW_FILES_SQL);
        assert!(REVIEW_FILES_SQL.contains("attachment_sets"));
        assert!(!REVIEW_FILES_SQL.contains("LOCK TABLE"));
    }

    #[test]
    fn pins_tax_signature_sql_oracles_and_placeholders() {
        assert_eq!(
            hash(TAX_METADATA_SQL),
            "acc12fbcd5bc55acf57e8327ad6b64e189b71042e5c2ca0f83a580477b14becb"
        );
        assert_eq!(
            hash(TAX_WORKSPACE_SQL),
            "d803d114be22810298402bec38fa887966f78d449cede647561cafa66fd81f80"
        );
        assert_eq!(normalize(TAX_METADATA_PG_SQL), TAX_METADATA_SQL);
        assert_eq!(normalize(TAX_WORKSPACE_PG_SQL), TAX_WORKSPACE_SQL);
        for sql in [TAX_METADATA_SQL, TAX_WORKSPACE_SQL] {
            assert!(sql.trim_start().to_ascii_uppercase().starts_with("SELECT"));
            assert!(sql.contains("finance_tax_identity"));
            assert!(!sql.contains("LOCK TABLE"));
            assert!(!sql.contains("pg_advisory"));
        }
    }
}
