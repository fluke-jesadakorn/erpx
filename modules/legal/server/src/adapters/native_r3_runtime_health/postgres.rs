//! sqlx execution for the native Legal runtime-health read.
//!
//! Both statements are fixed and code-owned. The task read has no request
//! binds; the capability probe has exactly one Company bind plus the trusted
//! scheduled-feature predicate validated in `domain`. No request value selects
//! a table, a column or a predicate, and no TypeScript fallback is involved.

use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{AssertSqlSafe, Row};

use super::application::{RuntimeHealthFuture, RuntimeHealthSource};
use super::domain::RuntimeHealthTaskRow;
use crate::adapters::legal_runtime_gate_sources::{
    legal_runtime_health_sql, render_legal_capability_probe, render_legal_runtime_predicate,
};

/// Pg-backed owner reader. Composition owns the pool and mounts the HTTP router.
pub struct PgRuntimeHealthSource {
    pool: DatabasePool,
}

impl PgRuntimeHealthSource {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_LEGAL_RUNTIME_HEALTH_UNAVAILABLE",
        "The native Legal runtime-health service is unavailable.",
    )
}

fn corrupt() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEGAL_RUNTIME_HEALTH_CORRUPT_RECORD",
        "The Legal runtime-health record is corrupt.",
    )
}

impl RuntimeHealthSource for PgRuntimeHealthSource {
    fn tasks<'a>(&'a self) -> RuntimeHealthFuture<'a, Vec<RuntimeHealthTaskRow>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let rows = sqlx::query(legal_runtime_health_sql())
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|r| {
                    Ok(RuntimeHealthTaskRow {
                        task: r.try_get("task").map_err(|_| corrupt())?,
                        heartbeat_at: r.try_get("heartbeat_at").map_err(|_| corrupt())?,
                        last_success_at: r
                            .try_get("last_success_at")
                            .map_err(|_| corrupt())?,
                        failure_count: i64::from(
                            r.try_get::<i32, _>("failure_count")
                                .map_err(|_| corrupt())?,
                        ),
                        last_error: r.try_get("last_error").map_err(|_| corrupt())?,
                    })
                })
                .collect()
        })
    }

    fn company_ready<'a>(
        &'a self,
        company: &'a str,
        feature_predicate: &'a str,
    ) -> RuntimeHealthFuture<'a, bool> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let predicate = feature_predicate.to_owned();
        Box::pin(async move {
            let predicate = render_legal_runtime_predicate("$1", false, &predicate);
            let sql = render_legal_capability_probe(&predicate);
            // Audited: the only dynamic part is the code-owned predicate from
            // composition, validated by `valid_feature_predicate` (no `;`,
            // `?`, `$` or comments). The single bind is the Company id.
            let row = sqlx::query(AssertSqlSafe(sql.as_str()))
                .bind(company.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            Ok(row.is_some())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn hash(sql: &str) -> String {
        format!("{:x}", Sha256::digest(sql.as_bytes()))
    }

    #[test]
    fn task_statement_is_fixed_and_read_only() {
        let sql = legal_runtime_health_sql();
        assert_eq!(
            hash(sql),
            "24b42750591b26877384aaff9e21042f746f068fafbe9086b0c69c06747fc869"
        );
        assert_eq!(
            sql,
            "SELECT task,heartbeat_at,last_success_at,failure_count,last_error FROM legal_runtime_health ORDER BY task"
        );
        for write in [
            "INSERT INTO",
            "DELETE FROM",
            "UPDATE ",
            "FOR UPDATE",
            "FOR SHARE",
            "LOCK TABLE",
            "pg_advisory",
        ] {
            assert!(!sql.contains(write), "{write}");
        }
    }

    #[test]
    fn capability_probe_binds_only_the_company() {
        let dev = render_legal_capability_probe(&render_legal_runtime_predicate(
            "$1",
            false,
            "TRUE",
        ));
        assert_eq!(
            dev,
            "SELECT 1 WHERE EXISTS(SELECT 1 FROM legal_settings runtime_legal_settings WHERE runtime_legal_settings.company_id=$1 AND runtime_legal_settings.enabled AND TRUE)"
        );
        assert_eq!(dev.matches('$').count(), 1);
        assert!(!dev.contains('?'));
        let prod = render_legal_capability_probe(&render_legal_runtime_predicate(
            "$1",
            false,
            "EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id='x')",
        ));
        assert_eq!(prod.matches('$').count(), 1);
        assert!(prod.contains("company_feature_rollouts"));
    }
}
