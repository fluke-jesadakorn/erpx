//! sqlx execution for the native lane 11 person-identity read family.
//!
//! The statements are the byte-identical successors of the three literals that
//! used to live in `adapters/iam-person.ts`: only `?` placeholders became
//! positional `$n` binds in the original order. Both tables are Identity
//! Access-owned and every statement carries the Company bind. The dedupe and
//! successor projection live in `domain.rs`, not in SQL.

use crate::adapters::native_lane_11::{
    application::{PersonIdentityFuture, PersonIdentityQuery},
    domain::{RoleRevision, SuccessorRow, unavailable},
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
#[cfg(test)]
use sha2::{Digest, Sha256};
use serde_json::Value;
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

/// Owner SQL for the four directory configuration reads. `to_jsonb` / the
/// fixed `jsonb_build_object` preserve the exact row shape of the deleted
/// TypeScript `SELECT *` / restricted-column literals; only `?` became `$1`
/// and every statement keeps its Company bind.
pub fn directory_sql(kind: &str, can_manage: bool) -> Option<&'static str> {
    match (kind, can_manage) {
        ("connections", true) => Some(
            "SELECT to_jsonb(c)::text FROM iam_connections c WHERE c.company_id=$1 ORDER BY c.name",
        ),
        ("connections", false) => Some(
            "SELECT jsonb_build_object('id',c.id,'name',c.name,'provider',c.provider,'status',c.status,'health',c.health,'last_sync_at',c.last_sync_at,'last_error',c.last_error,'version',c.version)::text FROM iam_connections c WHERE c.company_id=$1 ORDER BY c.name",
        ),
        ("groups", _) => Some(
            "SELECT to_jsonb(g)::text FROM iam_groups g JOIN iam_connections c ON c.id=g.connection_id WHERE c.company_id=$1 ORDER BY g.name",
        ),
        ("previews", _) => Some(
            "SELECT to_jsonb(p)::text FROM iam_connection_previews p JOIN iam_connections c ON c.id=p.connection_id AND c.version=p.version WHERE c.company_id=$1",
        ),
        ("identities", _) => Some(
            "SELECT to_jsonb(i)::text FROM iam_identities i WHERE i.company_id=$1 ORDER BY i.name",
        ),
        _ => None,
    }
}

/// Published role catalog statements: the exact `iam-store.ts` and
/// `iam-service-reads.ts` literals with `?` renumbered to `$n` in bind order.
pub const ROLE_ONE_SQL: &str = "SELECT r.*,v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',$1) AND r.id=$2 AND r.status='active'";
pub const ROLE_IDS_SQL: &str = "SELECT id FROM iam_roles WHERE company_id IN ('*',$1) AND status='active' AND published_revision>0 ORDER BY built_in DESC,name";

pub fn role_list_sql(count: usize) -> String {
    format!(
        "SELECT r.id,r.name,r.company_id,r.published_revision,r.built_in,r.status,\n    v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r\n    JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision\n    WHERE r.company_id IN ('*',$1) AND r.id IN ({}) AND r.status='active'",
        numbered(2, count)
    )
}

fn json_strings(row: &sqlx::postgres::PgRow, column: &str) -> Result<Vec<String>, BusinessError> {
    if let Ok(value) = row.try_get::<Value, _>(column) {
        return value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .ok_or_else(unavailable);
    }
    row.try_get::<String, _>(column)
        .map_err(|_| unavailable())
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).map_err(|_| unavailable()))
}

fn role_from(row: &sqlx::postgres::PgRow) -> Result<RoleRevision, BusinessError> {
    let revision: i64 = row
        .try_get::<i32, _>("published_revision")
        .map(i64::from)
        .or_else(|_| row.try_get::<i64, _>("published_revision"))
        .map_err(|_| unavailable())?;
    Ok(RoleRevision {
        id: row.try_get("id").map_err(|_| unavailable())?,
        name: row.try_get("name").map_err(|_| unavailable())?,
        company_id: row.try_get("company_id").map_err(|_| unavailable())?,
        revision,
        built_in: row.try_get("built_in").map_err(|_| unavailable())?,
        permissions: json_strings(row, "permissions_json")?,
        grantable_permissions: json_strings(row, "grantable_json")?,
        scopes: json_strings(row, "scopes_json")?,
        status: row.try_get("status").map_err(|_| unavailable())?,
    })
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

    fn directory_rows<'a>(
        &'a self,
        company: &'a str,
        kind: &'a str,
        can_manage: bool,
    ) -> PersonIdentityFuture<'a, Vec<Value>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let kind = kind.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let kind = kind.clone();
                Box::pin(async move {
                    let sql = directory_sql(&kind, can_manage).ok_or_else(unavailable)?;
                    let rows = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let values = rows
                        .into_iter()
                        .map(|row| serde_json::from_str::<Value>(&row).map_err(|_| unavailable()))
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

    fn role_one<'a>(
        &'a self,
        company: &'a str,
        id: &'a str,
    ) -> PersonIdentityFuture<'a, Option<RoleRevision>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let id = id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let id = id.clone();
                Box::pin(async move {
                    let row = sqlx::query(sqlx::AssertSqlSafe(ROLE_ONE_SQL))
                        .bind(company.as_str())
                        .bind(id.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    row.as_ref().map(role_from).transpose().map_err(Into::into)
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }

    fn role_list<'a>(
        &'a self,
        company: &'a str,
        ids: &'a [String],
    ) -> PersonIdentityFuture<'a, Vec<RoleRevision>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let ids = ids.to_vec();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let ids = ids.clone();
                Box::pin(async move {
                    let mut statement =
                        sqlx::query(sqlx::AssertSqlSafe(role_list_sql(ids.len())))
                            .bind(company.as_str());
                    for id in &ids {
                        statement = statement.bind(id.as_str());
                    }
                    let rows = statement
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    rows.iter()
                        .map(role_from)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(Into::into)
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }

    fn role_ids<'a>(&'a self, company: &'a str) -> PersonIdentityFuture<'a, Vec<String>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                Box::pin(async move {
                    sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(ROLE_IDS_SQL))
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| PgTransactionError::Business(unavailable()))
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

    #[test]
    fn directory_sql_oracles_are_stable() {
        // Exact literals rendered by `iam-service-reads.ts` before migration.
        const RENDERED: [&str; 5] = [
            "SELECT * FROM iam_connections WHERE company_id=? ORDER BY name",
            "SELECT id,name,provider,status,health,last_sync_at,last_error,version FROM iam_connections WHERE company_id=? ORDER BY name",
            "SELECT g.* FROM iam_groups g JOIN iam_connections c ON c.id=g.connection_id WHERE c.company_id=? ORDER BY g.name",
            "SELECT p.* FROM iam_connection_previews p JOIN iam_connections c ON c.id=p.connection_id AND c.version=p.version WHERE c.company_id=?",
            "SELECT * FROM iam_identities WHERE company_id=? ORDER BY name",
        ];
        const HASHES: [&str; 5] = [
            "3144f4820586196b219737843f911a07965fdb0113804cdad91d7f6fda977773",
            "209ddd712e3a4b843bdb750a771d4c73b48b9aa00aa8561a6fbdeb1b53792068",
            "7f160d7b5618894adb149c34e65ce7199377ea0163fa6af3cf3bc4ea10ba14ed",
            "782717bcc057fdd4ca3dfa3c9d74ce8331f9135b1f56e915dc4055c24a78eee0",
            "0a39de165486dd1c7d27197770daaedcd368839f4783b0f50fadb6ac7aa45946",
        ];
        for (sql, expected) in RENDERED.iter().zip(HASHES.iter()) {
            assert_eq!(hash(sql), *expected);
        }
        assert!(directory_sql("connections", true).unwrap().contains("$1"));
        assert!(directory_sql("connections", false).unwrap().contains("jsonb_build_object"));
        assert!(directory_sql("groups", false).is_some());
        assert!(directory_sql("previews", false).is_some());
        assert!(directory_sql("identities", false).is_some());
        assert!(directory_sql("unknown", false).is_none());
    }

    #[test]
    fn role_sql_oracles_are_stable() {
        // Exact literals rendered by `iam-store.ts` and `iam-service-reads.ts`.
        let one = "SELECT r.*,v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',?) AND r.id=? AND r.status='active'";
        let list = "SELECT r.id,r.name,r.company_id,r.published_revision,r.built_in,r.status,\n    v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r\n    JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision\n    WHERE r.company_id IN ('*',?) AND r.id IN (?,?) AND r.status='active'";
        let ids = "SELECT id FROM iam_roles WHERE company_id IN ('*',?) AND status='active' AND published_revision>0 ORDER BY built_in DESC,name";
        assert_eq!(
            hash(one),
            "aee8e70c4f14551dfe728a88768ec3ca7bbfdde9d8fe5ddb2d958955b48d5115"
        );
        assert_eq!(
            hash(list),
            "055d8101054d58144f8ac966d236d92cf23d450e5d111bd346c78b478485df5b"
        );
        assert_eq!(
            hash(ids),
            "8cd0010d3cbd15edebb0b2e5b1d2fd47ddfc07802f8d2c05d415484202cec765"
        );
        assert_eq!(renumber(one), ROLE_ONE_SQL);
        assert_eq!(renumber(ids), ROLE_IDS_SQL);
        assert_eq!(role_list_sql(2).matches('$').count(), 3);
    }
}
