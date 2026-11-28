//! sqlx execution of the migrated `routePeople` read. The statement keeps the
//! owner projection, Company scope and `merged_into_id` guard; the D1 `?` binds
//! are rewritten for Postgres and the caller's id list becomes a bound JSON
//! array rather than interpolated SQL.

use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::BusinessError;
use serde_json;
use sha2::Digest;
use sqlx::Row;

use super::application::{PersonRouteRead, RouteFuture};
use super::domain::{MAX_ROWS, RoutePerson, unavailable};

/// `routePeople` in `adapters/work-review-facts.ts`.
pub const SQL_ROUTE_PEOPLE: &str = "SELECT user_id,legal_name FROM hr_people WHERE company_id=$1 AND user_id IN (SELECT jsonb_array_elements_text($2::jsonb)) AND merged_into_id IS NULL";

pub struct PgPersonRouteRead {
    pool: DatabasePool,
}

impl PgPersonRouteRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PersonRouteRead for PgPersonRouteRead {
    fn route<'a>(&'a self, company: &'a str, users: &'a [String]) -> RouteFuture<'a, Vec<RoutePerson>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let users = users.to_vec();
        Box::pin(async move {
            let image = serde_json::to_value(&users).map_err(|_| unavailable())?;
            let rows = sqlx::query(SQL_ROUTE_PEOPLE)
                .bind(company.as_str())
                .bind(sqlx::types::Json(image))
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            if rows.len() > MAX_ROWS {
                return Err(unavailable());
            }
            rows.into_iter()
                .map(|row| {
                    Ok(RoutePerson {
                        user_id: row.try_get::<String, _>("user_id").map_err(|_| unavailable())?,
                        legal_name: row.try_get::<String, _>("legal_name").map_err(|_| unavailable())?,
                    })
                })
                .collect::<Result<Vec<_>, BusinessError>>()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_keeps_owner_scope_and_guard() {
        assert!(SQL_ROUTE_PEOPLE.starts_with("SELECT user_id,legal_name FROM hr_people"));
        assert!(SQL_ROUTE_PEOPLE.contains("company_id=$1"));
        assert!(SQL_ROUTE_PEOPLE.contains("user_id IN (SELECT jsonb_array_elements_text($2::jsonb))"));
        assert!(SQL_ROUTE_PEOPLE.contains("merged_into_id IS NULL"));
        assert!(!SQL_ROUTE_PEOPLE.contains('?'));
    }

    #[test]
    fn pins_rendered_sql_oracle() {
        let hex = sha2::Sha256::digest(SQL_ROUTE_PEOPLE.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex,
            "0acbcc7c016d3d0aa05fc555575bc458ad4af3759fb51fc7fb677e841df7a589",
            "update the oracle in Rust and in native-lane-9/person-route-client.ts"
        );
    }
}
