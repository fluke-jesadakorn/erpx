//! sqlx execution for lane 16. This is the only module that owns SQL text or
//! touches the pool. The constants are the byte-identical literals retired from
//! `adapters/people-app-entrypoint-facts.ts`; `render_sql` only translates the
//! PostgreSQL `?` placeholders to sqlx's `$n`, never a predicate or bind order.

use super::application::{LaneReadFuture, PushSettingsRead, SentNoticeRead};
use super::domain::{SentNoticeDetail, SentNoticeRow, TaskOrigin};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sha2::{Digest, Sha256};
use sqlx::Row;

/// Exact retired TypeScript literal for `sentNoticeRows`.
pub const SQL_NOTICES: &str = "SELECT n.id,n.title,n.recipient_count accepted_count,n.created_at,\n        (SELECT count(*)::int FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id\n         WHERE e.company_id=n.company_id AND e.event_key='notice:'||n.id) delivered_count\n        FROM inbox_notices n WHERE n.company_id=? AND n.sender_user_id=? ORDER BY n.created_at DESC,n.id DESC LIMIT 50";

/// Exact retired TypeScript literal for `sentNoticeRow`.
pub const SQL_NOTICE: &str =
    "SELECT id,title,body,recipient_count,created_at FROM inbox_notices WHERE id=? AND company_id=? AND sender_user_id=?";

/// Exact retired TypeScript literal for `sentNoticeDeliveryCount`.
pub const SQL_DELIVERY_COUNT: &str = "SELECT count(*)::int n FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id WHERE e.company_id=? AND e.event_key=?";

/// Exact retired TypeScript literal rendered by `socialTaskOriginBy('post_id','task_id')`.
pub const SQL_TASK_ORIGIN: &str = "SELECT post_id FROM social_task_origins WHERE task_id=?";

/// The task-origin read is Company-fenced on the native side. The owner adds
/// the scope predicate; the caller cannot select another Company's origin.
pub const SQL_TASK_ORIGIN_SCOPED: &str =
    "SELECT post_id FROM social_task_origins WHERE task_id=? AND company_id=?";

pub fn render_sql(literal: &str) -> String {
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

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LANE_16_NATIVE_READ_UNAVAILABLE",
        "The native Collaboration sent-notice read is unavailable.",
    )
}

pub fn hash(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Postgres-backed owner reader. Composition supplies the pool; no caller can
/// supply SQL, a predicate or a Company outside the routing target.
pub struct PgSentNoticeRead {
    pool: DatabasePool,
}

impl PgSentNoticeRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl SentNoticeRead for PgSentNoticeRead {
    fn notices<'a>(
        &'a self,
        company: &'a str,
        sender: &'a str,
    ) -> LaneReadFuture<'a, Vec<SentNoticeRow>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let sender = sender.to_owned();
        Box::pin(async move {
            let sql = render_sql(SQL_NOTICES);
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(company.as_str())
                .bind(sender.as_str())
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(SentNoticeRow {
                        id: row.try_get("id").map_err(|_| unavailable())?,
                        title: row.try_get("title").map_err(|_| unavailable())?,
                        accepted_count: row.try_get("accepted_count").map_err(|_| unavailable())?,
                        created_at: row.try_get("created_at").map_err(|_| unavailable())?,
                        delivered_count: row.try_get("delivered_count").map_err(|_| unavailable())?,
                    })
                })
                .collect()
        })
    }

    fn notice<'a>(
        &'a self,
        company: &'a str,
        sender: &'a str,
        id: &'a str,
    ) -> LaneReadFuture<'a, Option<SentNoticeDetail>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let sender = sender.to_owned();
        let id = id.to_owned();
        Box::pin(async move {
            let sql = render_sql(SQL_NOTICE);
            let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(id.as_str())
                .bind(company.as_str())
                .bind(sender.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            let Some(row) = row else {
                return Ok(None);
            };
            let count_sql = render_sql(SQL_DELIVERY_COUNT);
            let completed: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql))
                .bind(company.as_str())
                .bind(format!("notice:{id}"))
                .fetch_one(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            let recipient_count: i64 = row.try_get("recipient_count").map_err(|_| unavailable())?;
            Ok(Some(SentNoticeDetail {
                id: row.try_get("id").map_err(|_| unavailable())?,
                title: row.try_get("title").map_err(|_| unavailable())?,
                body: row.try_get("body").map_err(|_| unavailable())?,
                recipient_count,
                created_at: row.try_get("created_at").map_err(|_| unavailable())?,
                acceptedCount: recipient_count,
                completedCount: completed,
            }))
        })
    }

    fn delivery_count<'a>(&'a self, company: &'a str, key: &'a str) -> LaneReadFuture<'a, i64> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let key = key.to_owned();
        Box::pin(async move {
            let sql = render_sql(SQL_DELIVERY_COUNT);
            sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
                .bind(company.as_str())
                .bind(key.as_str())
                .fetch_one(pool.raw())
                .await
                .map_err(|_| unavailable())
        })
    }

    fn task_origin<'a>(
        &'a self,
        company: &'a str,
        task: &'a str,
    ) -> LaneReadFuture<'a, Option<TaskOrigin>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let task = task.to_owned();
        Box::pin(async move {
            let sql = render_sql(SQL_TASK_ORIGIN_SCOPED);
            let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(task.as_str())
                .bind(company.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| {
                Ok(TaskOrigin {
                    post_id: row.try_get("post_id").map_err(|_| unavailable())?,
                })
            })
            .transpose()
        })
    }
}

/// Exact retired TypeScript literal for the push preferences read.
pub const SQL_PUSH_PREFERENCES: &str =
    "SELECT * FROM push_preferences WHERE company_id=? AND user_id=?";
/// Exact retired TypeScript literal for the per-session subscription count.
pub const SQL_PUSH_SESSION_COUNT: &str =
    "SELECT count(*)::int n FROM push_subscriptions WHERE company_id=? AND user_id=? AND session_id=?";
/// Exact retired TypeScript literal for the endpoint owner lookup.
pub const SQL_PUSH_ENDPOINT_OWNER: &str =
    "SELECT id,user_id,session_id FROM push_subscriptions WHERE company_id=? AND endpoint_hash=?";

/// Postgres-backed owner reader for the push registration reads.
pub struct PgPushSettingsRead {
    pool: DatabasePool,
}

impl PgPushSettingsRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PushSettingsRead for PgPushSettingsRead {
    fn preferences<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
    ) -> LaneReadFuture<'a, Option<serde_json::Value>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let user = user.to_owned();
        Box::pin(async move {
            let sql = render_sql(SQL_PUSH_PREFERENCES);
            let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(company.as_str())
                .bind(user.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            let Some(row) = row else {
                return Ok(None);
            };
            Ok(Some(serde_json::json!({
                "enabled": row.try_get::<bool, _>("enabled").map_err(|_| unavailable())?,
                "work_enabled": row.try_get::<bool, _>("work_enabled").map_err(|_| unavailable())?,
                "mention_enabled": row.try_get::<bool, _>("mention_enabled").map_err(|_| unavailable())?,
                "system_enabled": row.try_get::<bool, _>("system_enabled").map_err(|_| unavailable())?,
                "calendar_enabled": row.try_get::<bool, _>("calendar_enabled").map_err(|_| unavailable())?,
                "notice_enabled": row.try_get::<bool, _>("notice_enabled").map_err(|_| unavailable())?,
                "quiet_start_minute": row.try_get::<Option<i32>, _>("quiet_start_minute").map_err(|_| unavailable())?,
                "quiet_end_minute": row.try_get::<Option<i32>, _>("quiet_end_minute").map_err(|_| unavailable())?,
            })))
        })
    }

    fn session_count<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
        session: &'a str,
    ) -> LaneReadFuture<'a, Option<i64>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let user = user.to_owned();
        let session = session.to_owned();
        Box::pin(async move {
            let sql = render_sql(SQL_PUSH_SESSION_COUNT);
            let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
                .bind(company.as_str())
                .bind(user.as_str())
                .bind(session.as_str())
                .fetch_one(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            Ok(Some(n))
        })
    }

    fn endpoint_owner<'a>(
        &'a self,
        company: &'a str,
        hash: &'a str,
    ) -> LaneReadFuture<'a, Option<serde_json::Value>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let hash = hash.to_owned();
        Box::pin(async move {
            let sql = render_sql(SQL_PUSH_ENDPOINT_OWNER);
            let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(company.as_str())
                .bind(hash.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| {
                Ok(serde_json::json!({
                    "id": row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                    "user_id": row.try_get::<String, _>("user_id").map_err(|_| unavailable())?,
                    "session_id": row.try_get::<String, _>("session_id").map_err(|_| unavailable())?,
                }))
            })
            .transpose()
        })
    }
}

/// The exact response envelope the TS contract test pins. Its digest is shared
/// between the Rust and TypeScript oracles.
pub const RESULT_ORACLE_JSON: &str = r#"{"kind":"notices","rows":[{"id":"notice-1","title":"Hello","accepted_count":2,"created_at":5,"delivered_count":1}]}"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_oracles_are_pinned() {
        assert_eq!(
            hash(SQL_NOTICES),
            "bbe9f92531ee4e811581dd0ea3be5694208a251773270ca54d14f60dc27d81d9"
        );
        assert_eq!(
            hash(SQL_NOTICE),
            "3c600b56ca7e7d793fb77778f18c3494145825402e3f05035113f56e10bdd914"
        );
        assert_eq!(
            hash(SQL_DELIVERY_COUNT),
            "70e04278860ac51d948b29b676e563b0b8d9d63f53e742ee89ca99e83ab83b19"
        );
        assert_eq!(
            hash(SQL_TASK_ORIGIN),
            "908c925016c228ff04f3934739a7da0877bccda710e3107bc8588642034d6f14"
        );
    }

    #[test]
    fn rendered_sql_translates_placeholders_without_changing_predicates() {
        assert_eq!(
            render_sql(SQL_NOTICES),
            "SELECT n.id,n.title,n.recipient_count accepted_count,n.created_at,\n        (SELECT count(*)::int FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id\n         WHERE e.company_id=n.company_id AND e.event_key='notice:'||n.id) delivered_count\n        FROM inbox_notices n WHERE n.company_id=$1 AND n.sender_user_id=$2 ORDER BY n.created_at DESC,n.id DESC LIMIT 50"
        );
        assert_eq!(
            render_sql(SQL_NOTICE),
            "SELECT id,title,body,recipient_count,created_at FROM inbox_notices WHERE id=$1 AND company_id=$2 AND sender_user_id=$3"
        );
        assert_eq!(
            render_sql(SQL_DELIVERY_COUNT),
            "SELECT count(*)::int n FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id WHERE e.company_id=$1 AND e.event_key=$2"
        );
        assert_eq!(
            render_sql(SQL_TASK_ORIGIN_SCOPED),
            "SELECT post_id FROM social_task_origins WHERE task_id=$1 AND company_id=$2"
        );
    }

    #[test]
    fn push_sql_oracles_are_pinned() {
        assert_eq!(
            hash(SQL_PUSH_PREFERENCES),
            "9d142bf18f7f9e8dd45b4266defc08836c026ec66ddc3a3c6470294bbd64ea43"
        );
        assert_eq!(
            hash(SQL_PUSH_SESSION_COUNT),
            "f9c7702f9fa7ab486acbddc5a6dab47d31fd74b2d27b5c14a7516acfc9081b0e"
        );
        assert_eq!(
            hash(SQL_PUSH_ENDPOINT_OWNER),
            "c155babcdfa2c74da37ab76d7bdf8af8f2e07c4abd0d86d173be9b1865de7592"
        );
        assert_eq!(
            render_sql(SQL_PUSH_SESSION_COUNT),
            "SELECT count(*)::int n FROM push_subscriptions WHERE company_id=$1 AND user_id=$2 AND session_id=$3"
        );
    }

    #[test]
    fn result_oracle_hash_is_pinned() {
        assert_eq!(
            hash(RESULT_ORACLE_JSON),
            "2ceaa6b93c6b879c5e5a718e3f8fbc7ae75994ab09a0ec73727807b0cdd3857b"
        );
    }
}
