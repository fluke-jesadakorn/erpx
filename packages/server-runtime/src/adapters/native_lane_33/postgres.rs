//! PostgreSQL execution for the lane 33 holding-binding reads.
//!
//! The executable statements start from the byte-identical text of the retired
//! TypeScript builder (`SELECT holding_id, slug, display_name FROM
//! holding_bindings` plus its fixed suffix) with only `?` binds renamed to
//! `$n`. The registry scope is a Rust-side projection, not caller SQL.

use super::application::{
    HoldingBindingRead, HoldingBindingReadFuture, unavailable,
};
use super::domain::{HoldingBinding, select_all, select_one};
use crate::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;

/// Byte-identical to `holdingBindingsSql('holding_id, slug, display_name')`.
pub const BASE_SQL: &str = "SELECT holding_id, slug, display_name FROM holding_bindings";
/// Byte-identical to the retired `holdingBindingBySlug` statement.
pub const BY_SLUG_SQL: &str =
    "SELECT holding_id, slug, display_name FROM holding_bindings WHERE slug=$1 AND status='active'";
/// Byte-identical to the retired `defaultHoldingBinding` statement.
pub const DEFAULT_SQL: &str =
    "SELECT holding_id, slug, display_name FROM holding_bindings WHERE status='active' ORDER BY holding_id LIMIT 1";
/// Byte-identical to the retired `activeHoldingBindings` statement.
pub const ACTIVE_SQL: &str =
    "SELECT holding_id, slug, display_name FROM holding_bindings WHERE status='active' ORDER BY holding_id";

/// The `?`-placeholder renderings used as the TypeScript result oracle.
pub fn rendered_by_slug_sql() -> String {
    format!("{BASE_SQL} WHERE slug=? AND status='active'")
}
pub fn rendered_default_sql() -> String {
    format!("{BASE_SQL} WHERE status='active' ORDER BY holding_id LIMIT 1")
}
pub fn rendered_active_sql() -> String {
    format!("{BASE_SQL} WHERE status='active' ORDER BY holding_id")
}

type Row = (String, String, String);

fn project(rows: Vec<Row>) -> Vec<HoldingBinding> {
    rows.into_iter()
        .map(|(holding_id, slug, display_name)| HoldingBinding {
            holding_id,
            slug,
            display_name,
        })
        .collect()
}

/// Pool-backed owner reader. No TypeScript SQL or business fallback remains.
pub struct PgHoldingBindingRead {
    pool: DatabasePool,
}

impl PgHoldingBindingRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    fn unit(&self) -> PgUnitOfWork {
        PgUnitOfWork::new(self.pool.clone())
    }
}

fn persistence(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

impl HoldingBindingRead for PgHoldingBindingRead {
    fn by_slug<'a>(
        &'a self,
        scope: &'a [String],
        slug: &'a str,
    ) -> HoldingBindingReadFuture<'a, Option<HoldingBinding>> {
        let unit = self.unit();
        let scope = scope.to_vec();
        let slug = slug.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let scope = scope.clone();
                let slug = slug.clone();
                Box::pin(async move {
                    let rows: Vec<Row> = sqlx::query_as::<_, Row>(BY_SLUG_SQL)
                        .bind(slug)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(select_one(project(rows), &scope))
                })
            })
            .await
            .map_err(persistence)
        })
    }

    fn default_binding<'a>(
        &'a self,
        scope: &'a [String],
    ) -> HoldingBindingReadFuture<'a, Option<HoldingBinding>> {
        let unit = self.unit();
        let scope = scope.to_vec();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let scope = scope.clone();
                Box::pin(async move {
                    let rows: Vec<Row> = sqlx::query_as::<_, Row>(DEFAULT_SQL)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(select_one(project(rows), &scope))
                })
            })
            .await
            .map_err(persistence)
        })
    }

    fn active<'a>(
        &'a self,
        scope: &'a [String],
    ) -> HoldingBindingReadFuture<'a, Vec<HoldingBinding>> {
        let unit = self.unit();
        let scope = scope.to_vec();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let scope = scope.clone();
                Box::pin(async move {
                    let rows: Vec<Row> = sqlx::query_as::<_, Row>(ACTIVE_SQL)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(select_all(project(rows), &scope))
                })
            })
            .await
            .map_err(persistence)
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
    fn executable_sql_is_the_retired_sql_with_only_bind_names_changed() {
        assert_eq!(rendered_by_slug_sql().replace("=?", "=$1"), BY_SLUG_SQL);
        assert_eq!(rendered_default_sql(), "SELECT holding_id, slug, display_name FROM holding_bindings WHERE status='active' ORDER BY holding_id LIMIT 1");
        assert_eq!(rendered_active_sql(), "SELECT holding_id, slug, display_name FROM holding_bindings WHERE status='active' ORDER BY holding_id");
        for sql in [BY_SLUG_SQL, DEFAULT_SQL, ACTIVE_SQL] {
            assert!(sql.starts_with(BASE_SQL));
            assert!(sql.contains("status='active'"));
            assert!(!sql.contains("FOR UPDATE") && !sql.contains("FOR SHARE"));
        }
    }

    #[test]
    fn rendered_sql_oracles_are_pinned() {
        // sha256 of the exact `?`-placeholder TypeScript renderings.
        assert_eq!(
            hash(&rendered_by_slug_sql()),
            "970fca1744181cbe731ff19a6064edf58107fff6d27801c9a1b86d1241367add"
        );
        assert_eq!(
            hash(&rendered_default_sql()),
            "424a3df615015a7d7144f38f13da25895a324b8a8666abcadbd7433bfd131b71"
        );
        assert_eq!(
            hash(&rendered_active_sql()),
            "dcf90998e6a224401ff3403edf31a57d54d4cbb5d70a921000d68469d552c48a"
        );
    }

    #[test]
    fn projection_maps_rows_and_applies_scope() {
        let rows = vec![
            ("h003".into(), "c".into(), "C".into()),
            ("h001".into(), "a".into(), "A".into()),
        ];
        let scope = vec!["h001".to_owned(), "h002".to_owned()];
        let all = select_all(project(rows.clone()), &scope);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].slug, "a");
        assert_eq!(select_one(project(rows), &scope).unwrap().holding_id, "h001");
    }
}
