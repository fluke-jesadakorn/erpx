//! sqlx/Postgres execution for the native acknowledgement POST. This is the
//! only layer that owns SQL text or touches the pool. The two owner statements
//! are byte-identical to the retired TypeScript literals; placeholder rendering
//! only translates `?` to `$n` and never changes a predicate or bind order.

use crate::application::native_r3_handover_ack::AckFuture;
use crate::application::native_r3_handover_ack_write::{AckReceipt, AckWriteStore};
use crate::domain::native_r3_handover_ack_write::{
    ACK_INSERT_SQL, RECEIPT_SQL, RECIPIENT_GUARD_SQL,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "OPERATIONS_HANDOVER_ACK_WRITE_UNAVAILABLE",
        "The native Collaboration handover acknowledgement write is unavailable.",
    )
}

/// Translate owner `?` placeholders to sqlx `$n` without touching other bytes.
pub fn render_placeholders(literal: &str) -> String {
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

pub struct PgAckWriteStore {
    pool: DatabasePool,
}

impl PgAckWriteStore {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl AckWriteStore for PgAckWriteStore {
    fn receipt<'a>(&'a self, key: &'a str) -> AckFuture<'a, Option<AckReceipt>> {
        let pool = self.pool.clone();
        let key = key.to_owned();
        Box::pin(async move {
            let row = sqlx::query_as::<_, (String, String)>(sqlx::AssertSqlSafe(RECEIPT_SQL))
                .bind(key.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            Ok(row.map(|(operation, result)| AckReceipt {
                operation,
                result: serde_json::from_str(&result).unwrap_or(serde_json::Value::Null),
            }))
        })
    }

    fn assert_recipient<'a>(
        &'a self,
        post_id: &'a str,
        user: &'a str,
        version: i64,
    ) -> AckFuture<'a, ()> {
        let pool = self.pool.clone();
        let post_id = post_id.to_owned();
        let user = user.to_owned();
        Box::pin(async move {
            let held: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(render_placeholders(
                RECIPIENT_GUARD_SQL,
            )))
            .bind(post_id.as_str())
            .bind(user.as_str())
            .bind(post_id.as_str())
            .bind(user.as_str())
            .bind(version)
            .fetch_one(pool.raw())
            .await
            .map_err(|_| unavailable())?;
            if !held {
                return Err(crate::domain::native_r3_handover_ack_write::recipient_required());
            }
            Ok(())
        })
    }

    fn insert<'a>(
        &'a self,
        post_id: &'a str,
        user: &'a str,
        version: i64,
        now: i64,
        key: &'a str,
    ) -> AckFuture<'a, ()> {
        let pool = self.pool.clone();
        let post_id = post_id.to_owned();
        let user = user.to_owned();
        let key = key.to_owned();
        Box::pin(async move {
            sqlx::query(sqlx::AssertSqlSafe(ACK_INSERT_SQL))
                .bind(post_id.as_str())
                .bind(user.as_str())
                .bind(version)
                .bind(now)
                .bind(key.as_str())
                .execute(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_placeholders_keep_bind_order() {
        assert_eq!(render_placeholders(RECEIPT_SQL).matches('$').count(), 1);
        assert_eq!(
            render_placeholders(RECIPIENT_GUARD_SQL),
            "EXISTS(SELECT 1 FROM social_post_participants WHERE post_id=$1 AND user_id=$2) AND NOT EXISTS(SELECT 1 FROM social_handover_acknowledgements WHERE post_id=$3 AND user_id=$4 AND post_version=$5)"
        );
        assert_eq!(render_placeholders(ACK_INSERT_SQL), ACK_INSERT_SQL);
    }
}
