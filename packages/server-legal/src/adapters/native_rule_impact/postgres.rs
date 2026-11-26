//! sqlx execution for the native Legal rule-impact read.
//!
//! Every statement is a fixed constant and Company-scoped by bind. No request
//! value selects a table, a column or a predicate; the only bound values are
//! opaque identities, the current scope and the Rust-owned row limit.

use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::Row;

use super::application::RuleImpactSource;
use super::application::RuleImpactFuture;
use super::domain::{IMPACT_LIMIT, RuleHead, RuleImpactItem};

/// Bounded point witness for the selected rule. The Company bind is the owner's
/// scope, not a trusted request value.
pub const SQL_RULE: &str =
    "SELECT kind,version::bigint version FROM legal_items WHERE id=$1 AND company_id=$2";

/// Current Company-enabled decision, re-checked in the owner before release.
pub const SQL_ENABLED: &str = "SELECT enabled FROM legal_settings WHERE company_id=$1";

/// Linked-record impact set. The Company, branch, department and confidential
/// membership visibility is identical to the retained owner read; the rule id
/// is bound twice for the two link directions and the limit is a bound constant.
pub const SQL_IMPACTS: &str = "SELECT i.id,i.kind,i.title,i.state,i.due_date,i.version::bigint version FROM legal_items i JOIN legal_links l ON (l.source_id=$5 AND l.target_id=i.id) OR (l.target_id=$5 AND l.source_id=i.id) WHERE i.company_id=$1 AND ($2::text IS NULL OR i.branch_id=$2) AND ($3::text IS NULL OR i.department_id=$3) AND (i.classification='internal' OR EXISTS(SELECT 1 FROM legal_members lm WHERE lm.item_id=i.id AND lm.user_id=$4)) ORDER BY i.id LIMIT $6";

/// Pg-backed owner reader. Composition owns the pool and mounts the HTTP router.
pub struct PgRuleImpactSource {
    pool: DatabasePool,
}

impl PgRuleImpactSource {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_LEGAL_RULE_IMPACT_UNAVAILABLE",
        "The native Legal rule-impact service is unavailable.",
    )
}

impl RuleImpactSource for PgRuleImpactSource {
    fn enabled<'a>(&'a self, company: &'a str) -> RuleImpactFuture<'a, bool> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        Box::pin(async move {
            sqlx::query_scalar::<_, bool>(SQL_ENABLED)
                .bind(company.as_str())
                .fetch_optional(pool.raw())
                .await
                .map(|value| value.unwrap_or(false))
                .map_err(|_| unavailable())
        })
    }

    fn rule<'a>(&'a self, company: &'a str, id: &'a str) -> RuleImpactFuture<'a, Option<RuleHead>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let id = id.to_owned();
        Box::pin(async move {
            let row = sqlx::query(SQL_RULE)
                .bind(id.as_str())
                .bind(company.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|r| {
                Ok(RuleHead {
                    kind: r.try_get("kind").map_err(|_| unavailable())?,
                    version: r.try_get("version").map_err(|_| unavailable())?,
                })
            })
            .transpose()
        })
    }

    fn impacts<'a>(
        &'a self,
        company: &'a str,
        branch: Option<&'a str>,
        department: Option<&'a str>,
        user: &'a str,
        id: &'a str,
    ) -> RuleImpactFuture<'a, Vec<RuleImpactItem>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let branch = branch.map(str::to_owned);
        let department = department.map(str::to_owned);
        let user = user.to_owned();
        let id = id.to_owned();
        Box::pin(async move {
            let rows = sqlx::query(SQL_IMPACTS)
                .bind(company.as_str())
                .bind(branch.as_deref())
                .bind(department.as_deref())
                .bind(user.as_str())
                .bind(id.as_str())
                .bind(IMPACT_LIMIT)
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|r| {
                    Ok(RuleImpactItem {
                        id: r.try_get("id").map_err(|_| unavailable())?,
                        kind: r.try_get("kind").map_err(|_| unavailable())?,
                        title: r.try_get("title").map_err(|_| unavailable())?,
                        state: r.try_get("state").map_err(|_| unavailable())?,
                        due_date: r.try_get("due_date").map_err(|_| unavailable())?,
                        version: r.try_get("version").map_err(|_| unavailable())?,
                    })
                })
                .collect()
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
    fn statements_are_fixed_and_company_scoped() {
        for sql in [SQL_RULE, SQL_ENABLED, SQL_IMPACTS] {
            assert!(sql.contains("company_id=$"), "missing Company scope: {sql}");
            assert!(!sql.contains('?'), "placeholder must be $n: {sql}");
        }
        assert!(SQL_RULE.contains("version::bigint"));
        assert!(SQL_IMPACTS.contains("i.classification='internal'"));
        assert!(SQL_IMPACTS.contains("legal_members"));
        assert!(SQL_IMPACTS.contains("LIMIT $6"));
        for (sql, expected) in [
            (
                SQL_RULE,
                "bd1f4f0e515545bb7851fae60407ecde4ffa72dd3325486c2f8cdab7ca6c66e9",
            ),
            (
                SQL_ENABLED,
                "2f0197b69c4b468562744771a4d5d354ac2fd2a344072b98ac9baf6d732faffe",
            ),
            (
                SQL_IMPACTS,
                "c316d0db41418e97b0832e3befc6909f261b6cb4b9410a5990ec4538b5191916",
            ),
        ] {
            assert_eq!(hash(sql), expected);
        }
    }
}
