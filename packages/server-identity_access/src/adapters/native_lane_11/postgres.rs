//! sqlx execution for the native lane 11 person-identity read family.
//!
//! The statements are the byte-identical successors of the three literals that
//! used to live in `adapters/iam-person.ts`: only `?` placeholders became
//! positional `$n` binds in the original order. Both tables are Identity
//! Access-owned and every statement carries the Company bind. The dedupe and
//! successor projection live in `domain.rs`, not in SQL.

use crate::adapters::native_lane_11::{
    application::{PersonIdentityFuture, PersonIdentityQuery},
    domain::{SuccessorRow, unavailable},
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
#[cfg(test)]
use sha2::{Digest, Sha256};
use sqlx::Row;

fn numbered(start: usize, count: usize) -> String {
    (0..count)
        .map(|index| format!("${}", start + index))
        .collect::<Vec<_>>()
        .join(",")
}

/// `SELECT DISTINCT linked.user_id ... WHERE original.company_id=$1 AND
/// original.user_id IN ($2,..)`.
pub fn identity_ids_sql(count: usize) -> String {
    format!(
        "SELECT DISTINCT linked.user_id FROM iam_members original \
         JOIN iam_members linked ON linked.company_id=original.company_id \
         AND linked.person_id=original.person_id \
         WHERE original.company_id=$1 AND original.user_id IN ({})",
        numbered(2, count)
    )
}

/// Policy-snapshot form: every active successor in the Company, no id filter.
pub fn principals_snapshot_sql() -> &'static str {
    "SELECT original.user_id original_id,linked.user_id current_id FROM iam_members original \
     JOIN iam_members linked ON linked.company_id=original.company_id \
     AND linked.person_id=original.person_id AND linked.status='active' \
     WHERE original.company_id=$1 AND original.status='superseded'"
}

/// Filtered form: the snapshot statement with the caller's id bind list.
pub fn principals_sql(count: usize) -> String {
    format!(
        "{} AND original.user_id IN ({})",
        principals_snapshot_sql(),
        numbered(2, count)
    )
}

/// Pg-backed owner reader. The pool is owned by composition; no caller SQL,
/// predicate or projection reaches this adapter.
pub struct PgPersonIdentityQuery {
    pool: DatabasePool,
}

impl PgPersonIdentityQuery {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PersonIdentityQuery for PgPersonIdentityQuery {
    fn aliases<'a>(
        &'a self,
        company: &'a str,
        ids: &'a [String],
    ) -> PersonIdentityFuture<'a, Vec<String>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let ids = ids.to_vec();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let ids = ids.clone();
                Box::pin(async move {
                    let mut statement =
                        sqlx::query(sqlx::AssertSqlSafe(identity_ids_sql(ids.len())))
                            .bind(company.as_str());
                    for id in &ids {
                        statement = statement.bind(id.as_str());
                    }
                    let rows = statement
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let values = rows
                        .iter()
                        .map(|row| row.try_get::<String, _>("user_id").map_err(|_| unavailable()))
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    Ok(values)
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }

    fn successors<'a>(
        &'a self,
        company: &'a str,
        ids: Option<&'a [String]>,
    ) -> PersonIdentityFuture<'a, Vec<SuccessorRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let ids = ids.map(<[String]>::to_vec);
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let ids = ids.clone();
                Box::pin(async move {
                    let sql = match &ids {
                        Some(values) => principals_sql(values.len()),
                        None => principals_snapshot_sql().to_owned(),
                    };
                    let mut statement = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(company.as_str());
                    for id in ids.iter().flatten() {
                        statement = statement.bind(id.as_str());
                    }
                    let rows = statement
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let values = rows
                        .iter()
                        .map(|row| {
                            Ok(SuccessorRow {
                                original_id: row
                                    .try_get::<String, _>("original_id")
                                    .map_err(|_| unavailable())?,
                                current_id: row
                                    .try_get::<String, _>("current_id")
                                    .map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    Ok(values)
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

    /// Exact text rendered by `iam-person.ts` before migration, with the two
    /// bound ids left as `?` placeholders.
    const RENDERED_TS_IDENTITY_IDS_SQL: &str = "SELECT DISTINCT linked.user_id FROM iam_members original JOIN iam_members linked ON linked.company_id=original.company_id AND linked.person_id=original.person_id WHERE original.company_id=? AND original.user_id IN (?,?)";
    const RENDERED_TS_PRINCIPALS_SNAPSHOT_SQL: &str = "SELECT original.user_id original_id,linked.user_id current_id FROM iam_members original JOIN iam_members linked ON linked.company_id=original.company_id AND linked.person_id=original.person_id AND linked.status='active' WHERE original.company_id=? AND original.status='superseded'";
    const RENDERED_TS_PRINCIPALS_FILTERED_SQL: &str = "SELECT original.user_id original_id,linked.user_id current_id FROM iam_members original JOIN iam_members linked ON linked.company_id=original.company_id AND linked.person_id=original.person_id AND linked.status='active' WHERE original.company_id=? AND original.status='superseded' AND original.user_id IN (?,?)";

    fn hash(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// Renumber `?` binds in order; the result must be the native statement.
    fn renumber(sql: &str) -> String {
        let mut n = 0;
        sql.split('?')
            .map(|part| {
                if n == 0 {
                    n += 1;
                    part.to_owned()
                } else {
                    let out = format!("${n}{part}");
                    n += 1;
                    out
                }
            })
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn native_sql_is_the_rendered_ts_sql_with_numbered_binds() {
        assert_eq!(renumber(RENDERED_TS_IDENTITY_IDS_SQL), identity_ids_sql(2));
        assert_eq!(
            renumber(RENDERED_TS_PRINCIPALS_SNAPSHOT_SQL),
            principals_snapshot_sql()
        );
        assert_eq!(renumber(RENDERED_TS_PRINCIPALS_FILTERED_SQL), principals_sql(2));
        assert_eq!(identity_ids_sql(2).matches('$').count(), 3);
        assert_eq!(principals_sql(2).matches('$').count(), 3);
    }

    #[test]
    fn sql_oracles_are_stable() {
        assert_eq!(
            hash(RENDERED_TS_IDENTITY_IDS_SQL),
            "a3d0b68ebcfcba00321a62935ca0b82c342bab5ca6a7a88d09ce4a0d4cdd70c1"
        );
        assert_eq!(
            hash(RENDERED_TS_PRINCIPALS_SNAPSHOT_SQL),
            "4c844f98bfc5957585e4b11c071a1f6d5acf56d7daa94fb1a8124b454fbf426e"
        );
        assert_eq!(
            hash(RENDERED_TS_PRINCIPALS_FILTERED_SQL),
            "3e798b8e0bc9c869fc75814767409e60938e1b3f3fccaa0ca3c8ab99358bd011"
        );
    }

    #[test]
    fn result_oracle_is_stable() {
        // Field order matches the serde struct declarations in `domain.rs`.
        const RESULT: &str = r#"{"identityIds":["user","alias"],"principals":[{"original":"old","current":"new"},{"original":"self","current":"self"}]}"#;
        assert_eq!(
            hash(RESULT),
            "54e7120defc300cdeb7bf99504f4599e9f69a05506d91ec6cf6a99e8fe729db0"
        );
    }
}
