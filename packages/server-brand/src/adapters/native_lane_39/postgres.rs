//! Lane 39 Brand postgres adapter: sqlx execution of the owner-local
//! workspace-choice publication read.
//!
//! The statement starts byte-identical to the retired owner literal and only
//! swaps `?` binds for positional `$n`. Placeholders are generated from the
//! validated bind count, never from request text; the relation, projection and
//! Company predicate stay owner-controlled.

use crate::adapters::native_lane_39::application::{WorkspaceChoiceFuture, WorkspaceChoiceRead};
use crate::adapters::native_lane_39::domain::{
    validate_company_ids, ScopeKind, WorkspaceChoice,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::Row;

/// Retired owner literal head. The tail is a comma list of positional binds.
pub const WORKSPACE_CHOICE_SQL_HEAD: &str =
    "SELECT scope_kind,scope_id,primary_publication_id FROM organization_brand_profiles \
     WHERE scope_kind='holding' OR company_id IN (";

pub const WORKSPACE_CHOICE_SQL_TAIL: &str = ")";

/// Render the exact statement for `n` bounded Company identities. The bind
/// list is generated from the validated count only, never from request text.
pub fn workspace_choice_sql(n: usize) -> String {
    let placeholders = (1..=n)
        .map(|index| format!("${index}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{WORKSPACE_CHOICE_SQL_HEAD}{placeholders}{WORKSPACE_CHOICE_SQL_TAIL}")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BRAND_WORKSPACE_CHOICE_UNAVAILABLE",
        "The native Brand workspace choice service is unavailable.",
    )
}

/// Pool-backed owner reader. The pool is the registered private Holding pool;
/// no TypeScript unit, lock, receipt or outbox is involved in this read.
pub struct PgWorkspaceChoiceRead {
    pool: DatabasePool,
}

impl PgWorkspaceChoiceRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl WorkspaceChoiceRead for PgWorkspaceChoiceRead {
    fn workspace_choices<'a>(
        &'a self,
        company_ids: &'a [String],
    ) -> WorkspaceChoiceFuture<'a, Vec<WorkspaceChoice>> {
        let pool = self.pool.clone();
        let ids = company_ids.to_vec();
        Box::pin(async move {
            validate_company_ids(&ids)?;
            // QueryBuilder keeps the literal owner head and tail and appends
            // one positional bind per validated Company identity. No caller
            // text ever becomes SQL; only bind values are user-controlled.
            let mut query = sqlx::QueryBuilder::new(WORKSPACE_CHOICE_SQL_HEAD);
            let mut binds = query.separated(", ");
            for id in &ids {
                binds.push_bind(id.as_str());
            }
            query.push(WORKSPACE_CHOICE_SQL_TAIL);
            let rows = query
                .build()
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    let scope_kind = row.try_get::<String, _>("scope_kind").map_err(|_| unavailable())?;
                    let scope_id = row.try_get::<String, _>("scope_id").map_err(|_| unavailable())?;
                    let primary_publication_id = row
                        .try_get::<Option<String>, _>("primary_publication_id")
                        .map_err(|_| unavailable())?;
                    Ok(WorkspaceChoice {
                        scope_kind: ScopeKind::parse(&scope_kind)?,
                        scope_id,
                        primary_publication_id,
                    })
                })
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendered_sql_is_byte_identical_to_the_owner_literal() {
        assert_eq!(
            workspace_choice_sql(3),
            "SELECT scope_kind,scope_id,primary_publication_id FROM organization_brand_profiles \
             WHERE scope_kind='holding' OR company_id IN ($1, $2, $3)"
        );
        assert_eq!(workspace_choice_sql(1).matches('$').count(), 1);
        assert_eq!(workspace_choice_sql(1000).matches('$').count(), 1000);
    }

    #[test]
    fn sql_never_contains_caller_text() {
        let sql = workspace_choice_sql(2);
        assert!(!sql.contains("co-a"));
        assert!(sql.starts_with(WORKSPACE_CHOICE_SQL_HEAD));
        assert!(sql.ends_with(WORKSPACE_CHOICE_SQL_TAIL));
    }
}
