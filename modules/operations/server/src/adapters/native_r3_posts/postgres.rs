//! sqlx/Postgres execution for the native L06 post sub-resources.
//!
//! This is the only layer that owns SQL text or touches the pool. Statements
//! are the byte-identical owner literals retired from TypeScript; `renumber`
//! translates `?` placeholders to sqlx `$n` without changing a predicate or a
//! bind order. Provider and work-authority decisions are injected by
//! composition as `AiProvider` / `TaskCreator` and never inferred from wire data.

use crate::application::native_r3_posts::{
    AiDraftPort, AiOutput, AiProvider, CommentFacts, PostsFuture, PriorDraft, TaskCommandPort,
    TaskCreator, TaskFacts, TaskSourcePort,
};
use crate::domain::native_r3_posts::{
    AI_DRAFT_CAPTURE_ACTOR_SQL, AI_DRAFT_CAPTURE_PROVIDER_SQL, AI_DRAFT_FAIL_SQL,
    AI_DRAFT_FINISH_SQL, AI_DRAFT_HEAD, AI_DRAFT_PREPARE_SQL, AI_USAGE_FAIL_SQL,
    AI_USAGE_FINISH_SQL, AI_USAGE_RESERVE_SQL, POST_SOURCE_SQL, POST_TASK_SQL,
    SHA256_SQL, render_comments_sql,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::Row;
use std::sync::Arc;

fn unavailable(code: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        code,
        "The native owner read is unavailable.",
    )
}

/// Translate the owner `?` placeholders to sqlx `$n` without changing any
/// other byte of the registered statement.
pub fn renumber(literal: &str) -> String {
    let mut next = 0usize;
    literal
        .chars()
        .map(|c| {
            if c == '?' {
                next += 1;
                format!("${next}")
            } else {
                c.to_string()
            }
        })
        .collect()
}

#[derive(Clone)]
pub struct PgPosts {
    pool: DatabasePool,
    creator: Arc<dyn TaskCreator>,
    provider: Arc<dyn AiProvider>,
}

impl PgPosts {
    pub fn new(
        pool: DatabasePool,
        creator: Arc<dyn TaskCreator>,
        provider: Arc<dyn AiProvider>,
    ) -> Self {
        Self {
            pool,
            creator,
            provider,
        }
    }
}

impl TaskSourcePort for PgPosts {
    fn published_post<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
        branch: Option<&'a str>,
        department: Option<&'a str>,
        post: &'a str,
        now: i64,
    ) -> PostsFuture<'a, Option<Value>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let user = user.to_owned();
        let branch = branch.map(str::to_owned);
        let department = department.map(str::to_owned);
        let post = post.to_owned();
        Box::pin(async move {
            let raw: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(POST_SOURCE_SQL))
                .bind(post.as_str())
                .bind(company.as_str())
                .bind(user.as_str())
                .bind(branch.as_deref())
                .bind(department.as_deref())
                .bind(now)
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable("OPERATIONS_POSTS_UNAVAILABLE"))?;
            raw.map(|text| serde_json::from_str(&text))
                .transpose()
                .map_err(|_| unavailable("OPERATIONS_POSTS_UNAVAILABLE"))
        })
    }

    fn comments<'a>(&'a self, post: &'a str, ids: &'a [String]) -> PostsFuture<'a, Vec<CommentFacts>> {
        let pool = self.pool.clone();
        let post = post.to_owned();
        let ids = ids.to_vec();
        Box::pin(async move {
            if ids.is_empty() {
                return Ok(Vec::new());
            }
            let sql = renumber(&render_comments_sql(ids.len()));
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(post.as_str());
            for id in &ids {
                query = query.bind(id.as_str());
            }
            let rows = query
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable("OPERATIONS_POSTS_UNAVAILABLE"))?;
            rows.into_iter()
                .map(|row| {
                    Ok(CommentFacts {
                        id: row.try_get("id").map_err(|_| unavailable("OPERATIONS_POSTS_UNAVAILABLE"))?,
                        body: row.try_get("body").map_err(|_| unavailable("OPERATIONS_POSTS_UNAVAILABLE"))?,
                        author: row
                            .try_get("author_name")
                            .map_err(|_| unavailable("OPERATIONS_POSTS_UNAVAILABLE"))?,
                        version: row
                            .try_get("version")
                            .map_err(|_| unavailable("OPERATIONS_POSTS_UNAVAILABLE"))?,
                    })
                })
                .collect()
        })
    }

    fn hash<'a>(&'a self, serialized: String) -> PostsFuture<'a, String> {
        let pool = self.pool.clone();
        Box::pin(async move {
            sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(SHA256_SQL))
                .bind(serialized)
                .fetch_one(pool.raw())
                .await
                .map_err(|_| unavailable("OPERATIONS_POSTS_UNAVAILABLE"))
        })
    }
}

impl TaskCommandPort for PgPosts {
    fn post_task<'a>(&'a self, post: &'a str) -> PostsFuture<'a, Option<String>> {
        let pool = self.pool.clone();
        let post = post.to_owned();
        Box::pin(async move {
            sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(POST_TASK_SQL))
                .bind(post.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable("OPERATIONS_POSTS_UNAVAILABLE"))
        })
    }

    fn task<'a>(&'a self, id: &'a str) -> PostsFuture<'a, Option<TaskFacts>> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        Box::pin(async move {
            let row = sqlx::query(sqlx::AssertSqlSafe(
                "SELECT id,state FROM staff_tasks WHERE id=$1",
            ))
            .bind(id.as_str())
            .fetch_optional(pool.raw())
            .await
            .map_err(|_| unavailable("OPERATIONS_POSTS_UNAVAILABLE"))?;
            let Some(row) = row else { return Ok(None) };
            Ok(Some(TaskFacts {
                id: row.try_get("id").map_err(|_| unavailable("OPERATIONS_POSTS_UNAVAILABLE"))?,
                state: row.try_get("state").map_err(|_| unavailable("OPERATIONS_POSTS_UNAVAILABLE"))?,
            }))
        })
    }

    fn create_task<'a>(
        &'a self,
        input: Value,
        source: Value,
        key: &'a str,
    ) -> PostsFuture<'a, TaskFacts> {
        self.creator.create(input, source, key)
    }
}

impl AiDraftPort for PgPosts {
    fn prior<'a>(&'a self, id: &'a str, user: &'a str) -> PostsFuture<'a, Option<PriorDraft>> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        let user = user.to_owned();
        Box::pin(async move {
            let row = sqlx::query(sqlx::AssertSqlSafe(AI_DRAFT_HEAD))
                .bind(id.as_str())
                .bind(user.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable("OPERATIONS_AI_DRAFT_UNAVAILABLE"))?;
            let Some(row) = row else { return Ok(None) };
            let err = || unavailable("OPERATIONS_AI_DRAFT_UNAVAILABLE");
            Ok(Some(PriorDraft {
                actor_json: row.try_get("actor_json").map_err(|_| err())?,
                post_id: row.try_get("post_id").map_err(|_| err())?,
                kind: row.try_get("kind").map_err(|_| err())?,
                source_hash: row.try_get("source_hash").map_err(|_| err())?,
                result_json: row.try_get("result_json").map_err(|_| err())?,
                state: row.try_get("state").map_err(|_| err())?,
                source_version: row.try_get("source_version").map_err(|_| err())?,
                comment_ids_json: row.try_get("comment_ids_json").map_err(|_| err())?,
                expires_at: row.try_get("expires_at").map_err(|_| err())?,
                provider_json: row.try_get("provider_json").map_err(|_| err())?,
            }))
        })
    }

    fn reserve<'a>(&'a self, values: Value, _now: i64) -> PostsFuture<'a, ()> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let get = |key: &str| values.get(key).cloned().unwrap_or(Value::Null);
            let id = get("id").as_str().unwrap_or_default().to_owned();
            let post = get("postId").as_str().unwrap_or_default().to_owned();
            let user = get("userId").as_str().unwrap_or_default().to_owned();
            let kind = get("kind").as_str().unwrap_or_default().to_owned();
            let version = get("version").as_i64().unwrap_or_default();
            let hash = get("hash").as_str().unwrap_or_default().to_owned();
            let comments = serde_json::to_string(&get("commentIds")).unwrap_or_default();
            let sources = serde_json::to_string(&get("sources")).unwrap_or_default();
            let company = get("companyId").as_str().unwrap_or_default().to_owned();
            let now = get("now").as_i64().unwrap_or_default();
            let expires = get("expiresAt").as_i64().unwrap_or_default();
            let mut tx = pool
                .raw()
                .begin()
                .await
                .map_err(|_| unavailable("OPERATIONS_AI_DRAFT_UNAVAILABLE"))?;
            sqlx::query(sqlx::AssertSqlSafe(AI_DRAFT_PREPARE_SQL))
                .bind(id.as_str())
                .bind(post.as_str())
                .bind(user.as_str())
                .bind(kind.as_str())
                .bind(version)
                .bind(hash.as_str())
                .bind(comments.as_str())
                .bind(sources.as_str())
                .bind("unconfigured")
                .bind(now)
                .bind(expires)
                .execute(&mut *tx)
                .await
                .map_err(|_| unavailable("OPERATIONS_AI_DRAFT_UNAVAILABLE"))?;
            sqlx::query(sqlx::AssertSqlSafe(AI_USAGE_RESERVE_SQL))
                .bind(id.as_str())
                .bind(company.as_str())
                .bind(user.as_str())
                .bind(now + 60_000)
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(|_| unavailable("OPERATIONS_AI_DRAFT_UNAVAILABLE"))?;
            tx.commit()
                .await
                .map_err(|_| unavailable("OPERATIONS_AI_DRAFT_UNAVAILABLE"))
        })
    }

    fn capture<'a>(&'a self, column: &'a str, value: &'a str, id: &'a str) -> PostsFuture<'a, ()> {
        let pool = self.pool.clone();
        let sql = match column {
            "provider_json" => AI_DRAFT_CAPTURE_PROVIDER_SQL,
            _ => AI_DRAFT_CAPTURE_ACTOR_SQL,
        };
        let value = value.to_owned();
        let id = id.to_owned();
        Box::pin(async move {
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(value.as_str())
                .bind(id.as_str())
                .execute(pool.raw())
                .await
                .map_err(|_| unavailable("OPERATIONS_AI_DRAFT_UNAVAILABLE"))?;
            Ok(())
        })
    }

    fn finish<'a>(
        &'a self,
        id: &'a str,
        result: &'a str,
        input: i64,
        output: i64,
    ) -> PostsFuture<'a, ()> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        let result = result.to_owned();
        Box::pin(async move {
            let mut tx = pool
                .raw()
                .begin()
                .await
                .map_err(|_| unavailable("OPERATIONS_AI_DRAFT_UNAVAILABLE"))?;
            sqlx::query(sqlx::AssertSqlSafe(AI_DRAFT_FINISH_SQL))
                .bind(result.as_str())
                .bind(id.as_str())
                .execute(&mut *tx)
                .await
                .map_err(|_| unavailable("OPERATIONS_AI_DRAFT_UNAVAILABLE"))?;
            sqlx::query(sqlx::AssertSqlSafe(AI_USAGE_FINISH_SQL))
                .bind(input)
                .bind(output)
                .bind(id.as_str())
                .execute(&mut *tx)
                .await
                .map_err(|_| unavailable("OPERATIONS_AI_DRAFT_UNAVAILABLE"))?;
            tx.commit()
                .await
                .map_err(|_| unavailable("OPERATIONS_AI_DRAFT_UNAVAILABLE"))
        })
    }

    fn fail<'a>(&'a self, id: &'a str) -> PostsFuture<'a, ()> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        Box::pin(async move {
            let mut tx = pool
                .raw()
                .begin()
                .await
                .map_err(|_| unavailable("OPERATIONS_AI_DRAFT_UNAVAILABLE"))?;
            for sql in [AI_DRAFT_FAIL_SQL, AI_USAGE_FAIL_SQL] {
                sqlx::query(sqlx::AssertSqlSafe(sql))
                    .bind(id.as_str())
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| unavailable("OPERATIONS_AI_DRAFT_UNAVAILABLE"))?;
            }
            tx.commit()
                .await
                .map_err(|_| unavailable("OPERATIONS_AI_DRAFT_UNAVAILABLE"))
        })
    }

    fn generate<'a>(&'a self, request: Value) -> PostsFuture<'a, AiOutput> {
        self.provider.generate(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_rendering_keeps_owner_bind_order() {
        assert_eq!(
            renumber("SELECT ? AND ?"),
            "SELECT $1 AND $2"
        );
        assert_eq!(
            renumber(&render_comments_sql(2)).matches('$').count(),
            3
        );
        assert!(POST_TASK_SQL.contains("post_id=$1"));
        assert!(AI_DRAFT_PREPARE_SQL.contains("$11"));
    }

    #[test]
    fn capture_selects_the_exact_owner_column() {
        assert!(AI_DRAFT_CAPTURE_ACTOR_SQL.contains("actor_json=$1"));
        assert!(AI_DRAFT_CAPTURE_PROVIDER_SQL.contains("provider_json=$1"));
    }
}
