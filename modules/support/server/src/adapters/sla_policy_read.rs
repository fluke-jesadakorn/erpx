//! PostgreSQL adapter for the native Support SLA policy point/work reads.
//!
//! Both entry points resolve an already-registered owner statement and execute
//! it with real bound SQL on the caller's already-open Holding unit. The adapter
//! plans no SQL for TypeScript, never opens or commits a transaction and never
//! shapes an HTTP response.
use crate::adapters::native_support_read_sql::{RD_SLA_POLICY_POINT, RD_SLA_POLICY_WORK};
use crate::application::sla_policy_read::{
    read_policy_point, read_policy_work, SlaPolicyReadFuture, SlaPolicyReadPorts,
};
use crate::domain::sla_policy_read::{SlaPolicyReadScope, SlaPolicyRow};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::{Column, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "SUPPORT_SLA_POLICY_UNAVAILABLE",
            "The Support SLA policy reader is unavailable.",
        ),
    }
}

/// Translate the owner-registered `?` bind markers to PostgreSQL `$n` markers.
/// The registry is authoritative for the bind count; an unexpected shape fails
/// closed instead of executing a diluted predicate.
fn postgres_statement(sql: &str, bind_count: usize) -> Result<String, Error> {
    let mut index = 0usize;
    let mut translated = String::with_capacity(sql.len());
    for character in sql.chars() {
        if character == '?' {
            index += 1;
            translated.push('$');
            translated.push_str(&index.to_string());
        } else {
            translated.push(character);
        }
    }
    if index != bind_count {
        return Err(BusinessError::invalid(
            "SUPPORT_SLA_READ_BINDS_INVALID",
            "The registered Support SLA read bind set does not match.",
        )
        .into());
    }
    Ok(translated)
}

fn has_column(row: &sqlx::postgres::PgRow, name: &str) -> bool {
    row.columns().iter().any(|column| column.name() == name)
}

/// Map the exact owner row columns, reading the reviewer projections only when
/// the executed statement supplies them.
fn map_row(row: &sqlx::postgres::PgRow) -> Result<SlaPolicyRow, Error> {
    Ok(SlaPolicyRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        name: row.try_get("name")?,
        policy_json: row.try_get("policy_json")?,
        status: row.try_get("status")?,
        version: row.try_get("version")?,
        created_by: row.try_get("created_by")?,
        approved_by: row.try_get("approved_by")?,
        approved_at: row.try_get("approved_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        // The registry projects this CASE as int4, not bigint.
        actor_eligible: if has_column(row, "actor_eligible") {
            row.try_get::<i32, _>("actor_eligible")? as i64
        } else {
            0
        },
        reviewers: if has_column(row, "reviewers") {
            row.try_get("reviewers")?
        } else {
            0
        },
    })
}

/// One bounded read unit bound to the caller's connection and selected scope.
pub struct PgSlaPolicyRead<'c> {
    connection: &'c mut PgConnection,
    scope: SlaPolicyReadScope,
}

impl<'c> PgSlaPolicyRead<'c> {
    pub fn new(connection: &'c mut PgConnection, scope: SlaPolicyReadScope) -> Self {
        Self { connection, scope }
    }

    pub fn scope(&self) -> &SlaPolicyReadScope {
        &self.scope
    }
}

impl SlaPolicyReadPorts for PgSlaPolicyRead<'_> {
    /// Executes the registered policy point statement with `(id, company_id)`.
    fn policy<'a>(&'a mut self, id: &'a str) -> SlaPolicyReadFuture<'a, Option<SlaPolicyRow>> {
        let company = self.scope.company_id.clone();
        Box::pin(async move {
            let result: Result<Option<SlaPolicyRow>, Error> = async {
                let sql = postgres_statement(
                    crate::adapters::native_support_read_sql::read_statement(RD_SLA_POLICY_POINT)
                        .ok_or_else(|| {
                            BusinessError::new(
                                FailureKind::Unavailable,
                                "SUPPORT_SLA_POLICY_UNAVAILABLE",
                                "The registered Support SLA policy read is unavailable.",
                            )
                        })?
                        .sql,
                    2,
                )?;
                let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                    .bind(id)
                    .bind(company.as_str())
                    .fetch_optional(&mut *self.connection)
                    .await?;
                row.as_ref().map(map_row).transpose()
            }
            .await;
            result.map_err(business)
        })
    }

    /// Executes the registered policy work statement with `(user_id, company_id)`.
    fn work_policies<'a>(&'a mut self) -> SlaPolicyReadFuture<'a, Vec<SlaPolicyRow>> {
        let company = self.scope.company_id.clone();
        let user = self.scope.user_id.clone();
        Box::pin(async move {
            let result: Result<Vec<SlaPolicyRow>, Error> = async {
                let sql = postgres_statement(
                    crate::adapters::native_support_read_sql::read_statement(RD_SLA_POLICY_WORK)
                        .ok_or_else(|| {
                            BusinessError::new(
                                FailureKind::Unavailable,
                                "SUPPORT_SLA_POLICY_UNAVAILABLE",
                                "The registered Support SLA policy read is unavailable.",
                            )
                        })?
                        .sql,
                    2,
                )?;
                let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                    .bind(user.as_str())
                    .bind(company.as_str())
                    .fetch_all(&mut *self.connection)
                    .await?;
                rows.iter().map(map_row).collect()
            }
            .await;
            result.map_err(business)
        })
    }
}

/// Composition entry point for `getSupportPolicy`: real SQL, complete row parity.
pub async fn load_policy_point(
    connection: &mut PgConnection,
    scope: SlaPolicyReadScope,
    id: &str,
) -> Result<Value, BusinessError> {
    let mut ports = PgSlaPolicyRead::new(connection, scope);
    let selected = ports.scope().clone();
    let view = read_policy_point(&mut ports, &selected, id).await?;
    serde_json::to_value(view).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "SUPPORT_SLA_RESPONSE_INVALID",
            "Invalid Support SLA policy response.",
        )
    })
}

/// Composition entry point for `supportPolicyWork`: real SQL, complete items.
pub async fn load_policy_work(
    connection: &mut PgConnection,
    scope: SlaPolicyReadScope,
) -> Result<Value, BusinessError> {
    let mut ports = PgSlaPolicyRead::new(connection, scope);
    let selected = ports.scope().clone();
    let view = read_policy_work(&mut ports, &selected).await?;
    serde_json::to_value(view).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "SUPPORT_SLA_RESPONSE_INVALID",
            "Invalid Support SLA policy response.",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_bind_markers_become_postgres_parameters() {
        let point = crate::adapters::native_support_read_sql::read_statement(RD_SLA_POLICY_POINT)
            .unwrap();
        let sql = postgres_statement(point.sql, point.bind_count).unwrap();
        assert!(sql.contains("id=$1"));
        assert!(sql.contains("company_id=$2"));
        assert!(!sql.contains('?'));
        let work =
            crate::adapters::native_support_read_sql::read_statement(RD_SLA_POLICY_WORK).unwrap();
        let sql = postgres_statement(work.sql, work.bind_count).unwrap();
        assert!(sql.contains("m.user_id=$1"));
        assert!(sql.contains("p.company_id=$2"));
        assert_eq!(sql.matches("$1").count(), 1);
        assert_eq!(sql.matches("$2").count(), 1);
        assert!(postgres_statement("SELECT 1 WHERE id=?", 2).is_err());
    }
}
