//! sqlx execution for the native Collaboration beta-facts read family.
//!
//! Every statement is the byte-identical successor of the statement that used
//! to live in `adapters/beta-management-facts.ts`: only the `?` binds were
//! renumbered to `$1..$n` in the original order. All four statements touch
//! collaboration-owned `social_*` tables and carry the Company bind where the
//! original did. No caller SQL or predicate is accepted.

use crate::adapters::native_lane_15::{
    application::{BetaFactsFuture, BetaFactsQuery},
    domain::{unavailable, PostDraft, PostReport, TaskOrigin},
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use sqlx::Row;

/// `SELECT id,body,... FROM social_post_drafts ...` with `?` → `$1,$2`.
pub const POST_DRAFTS_SQL: &str = "SELECT id,body,branch_id,department_id,audience_kind,audience_id,participants_json,sources_json,purpose,version,mentions_json,content_json,rich_content_json FROM social_post_drafts WHERE author_id=$1 AND company_id=$2 AND published_post_id IS NULL AND comment_parent_post_id IS NULL AND edit_post_id IS NULL ORDER BY updated_at DESC LIMIT 20";

/// `SELECT r.id,r.reason,r.post_id,p.state FROM social_reports ...` with `?` → `$1`.
pub const REPORTS_SQL: &str = "SELECT r.id,r.reason,r.post_id,p.state FROM social_reports r JOIN social_posts p ON p.id=r.post_id WHERE p.company_id=$1 ORDER BY r.created_at DESC LIMIT 100";

/// `socialTaskOriginBy('task_id','post_id')` with `?` → `$1`. Shared by both
/// task read kinds, exactly as the TypeScript helper was.
pub const TASK_ORIGIN_SQL: &str = "SELECT task_id FROM social_task_origins WHERE post_id=$1";

/// Postgres-backed owner query. Composition owns the pool; the reader never
/// accepts a caller-supplied statement, predicate or bind.
pub struct PgBetaFactsQuery {
    pool: DatabasePool,
}

impl PgBetaFactsQuery {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl BetaFactsQuery for PgBetaFactsQuery {
    fn post_drafts<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
    ) -> BetaFactsFuture<'a, Vec<PostDraft>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let user = user.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let user = user.clone();
                Box::pin(async move {
                    let rows = sqlx::query(POST_DRAFTS_SQL)
                        .bind(user.as_str())
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    rows.into_iter()
                        .map(|r| {
                            Ok(PostDraft {
                                id: r.try_get("id").map_err(|_| unavailable())?,
                                body: r.try_get("body").map_err(|_| unavailable())?,
                                branch_id: r.try_get("branch_id").map_err(|_| unavailable())?,
                                department_id: r
                                    .try_get("department_id")
                                    .map_err(|_| unavailable())?,
                                audience_kind: r
                                    .try_get("audience_kind")
                                    .map_err(|_| unavailable())?,
                                audience_id: r.try_get("audience_id").map_err(|_| unavailable())?,
                                participants_json: r
                                    .try_get("participants_json")
                                    .map_err(|_| unavailable())?,
                                sources_json: r
                                    .try_get("sources_json")
                                    .map_err(|_| unavailable())?,
                                purpose: r.try_get("purpose").map_err(|_| unavailable())?,
                                version: r.try_get("version").map_err(|_| unavailable())?,
                                mentions_json: r
                                    .try_get("mentions_json")
                                    .map_err(|_| unavailable())?,
                                content_json: r
                                    .try_get("content_json")
                                    .map_err(|_| unavailable())?,
                                rich_content_json: r
                                    .try_get("rich_content_json")
                                    .map_err(|_| unavailable())?,
                            })
                        })
                        .collect()
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }

    fn reports<'a>(&'a self, company: &'a str) -> BetaFactsFuture<'a, Vec<PostReport>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                Box::pin(async move {
                    let rows = sqlx::query(REPORTS_SQL)
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    rows.into_iter()
                        .map(|r| {
                            Ok(PostReport {
                                id: r.try_get("id").map_err(|_| unavailable())?,
                                reason: r.try_get("reason").map_err(|_| unavailable())?,
                                post_id: r.try_get("post_id").map_err(|_| unavailable())?,
                                state: r.try_get("state").map_err(|_| unavailable())?,
                            })
                        })
                        .collect()
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }

    fn task_origin<'a>(&'a self, post: &'a str) -> BetaFactsFuture<'a, Option<TaskOrigin>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let post = post.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let post = post.clone();
                Box::pin(async move {
                    let row = sqlx::query(TASK_ORIGIN_SQL)
                        .bind(post.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    row.map(|r| {
                        Ok(TaskOrigin {
                            task_id: r.try_get("task_id").map_err(|_| unavailable())?,
                        })
                    })
                    .transpose()
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

    /// The exact TypeScript statements before migration: helpers expanded,
    /// `?` placeholders left intact. `postTask` and `recoverPostTask` both used
    /// `socialTaskOriginBy('task_id','post_id')`.
    const RENDERED_TS_POST_DRAFTS: &str = "SELECT id,body,branch_id,department_id,audience_kind,audience_id,participants_json,sources_json,purpose,version,mentions_json,content_json,rich_content_json FROM social_post_drafts WHERE author_id=? AND company_id=? AND published_post_id IS NULL AND comment_parent_post_id IS NULL AND edit_post_id IS NULL ORDER BY updated_at DESC LIMIT 20";
    const RENDERED_TS_REPORTS: &str = "SELECT r.id,r.reason,r.post_id,p.state FROM social_reports r JOIN social_posts p ON p.id=r.post_id WHERE p.company_id=? ORDER BY r.created_at DESC LIMIT 100";
    const RENDERED_TS_TASK_ORIGIN: &str = "SELECT task_id FROM social_task_origins WHERE post_id=?";

    /// Renumber `?` binds in original order; the result must be the native SQL.
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
        assert_eq!(renumber(RENDERED_TS_POST_DRAFTS), POST_DRAFTS_SQL);
        assert_eq!(renumber(RENDERED_TS_REPORTS), REPORTS_SQL);
        assert_eq!(renumber(RENDERED_TS_TASK_ORIGIN), TASK_ORIGIN_SQL);
        assert_eq!(POST_DRAFTS_SQL.matches('$').count(), 2);
        assert_eq!(REPORTS_SQL.matches('$').count(), 1);
        assert_eq!(TASK_ORIGIN_SQL.matches('$').count(), 1);
    }

    #[test]
    fn every_read_stays_on_collaboration_owned_tables() {
        for sql in [POST_DRAFTS_SQL, REPORTS_SQL, TASK_ORIGIN_SQL] {
            assert!(sql.to_ascii_lowercase().contains("social_"));
        }
    }

    #[test]
    fn projected_rows_have_a_stable_wire_shape() {
        let draft = PostDraft {
            id: "d1".into(),
            body: "hello".into(),
            branch_id: None,
            department_id: Some("dept".into()),
            audience_kind: Some("company".into()),
            audience_id: None,
            participants_json: "[]".into(),
            sources_json: "[]".into(),
            purpose: "update".into(),
            version: 2,
            mentions_json: "[]".into(),
            content_json: None,
            rich_content_json: None,
        };
        let json = serde_json::to_string(&draft).expect("serialize");
        assert_eq!(
            json,
            r#"{"id":"d1","body":"hello","branch_id":null,"department_id":"dept","audience_kind":"company","audience_id":null,"participants_json":"[]","sources_json":"[]","purpose":"update","version":2,"mentions_json":"[]","content_json":null,"rich_content_json":null}"#
        );
        let report = PostReport {
            id: "r1".into(),
            reason: "spam".into(),
            post_id: "p1".into(),
            state: "published".into(),
        };
        assert_eq!(
            serde_json::to_string(&report).expect("serialize"),
            r#"{"id":"r1","reason":"spam","post_id":"p1","state":"published"}"#
        );
    }
}
