//! sqlx/Postgres execution for the native handover acknowledgement read.
//!
//! This is the only layer that owns SQL text or touches the pool. The two
//! acknowledgement statements are the byte-identical owner literals retired
//! from the TypeScript read; `render_placeholders` only translates PostgreSQL
//! `?` placeholders to sqlx `$n`, never a predicate or bind order. The post
//! facts statement narrows the retained Social `sourceAccessSql` to the
//! `purpose='handover'`, `audience_kind='participants'` case this read guards.

use crate::application::native_r3_handover_ack::{
    AckFuture, HandoverAckRead, HandoverPostSource,
};
use crate::domain::native_r3_handover_ack::{
    RECIPIENTS_SQL, HandoverPostFacts, render_acknowledgements_sql,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::Row;

/// Company-scoped, published, participant-visible handover post facts. The
/// participant arm is the retained `sourceAccessSql` arm for
/// `audience_kind='participants'`; purpose and audience are rechecked by the
/// domain invariant before any acknowledgement read is executed.
pub const POST_SQL: &str = concat!(
    "SELECT p.id,p.author_id,p.version::bigint,p.purpose,p.audience_kind ",
    "FROM social_posts p WHERE p.id=$1 AND p.company_id=$2 AND p.state='published' ",
    "AND EXISTS(SELECT 1 FROM social_post_participants sp ",
    "WHERE sp.post_id=p.id AND sp.user_id=$3)"
);

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "OPERATIONS_HANDOVER_ACK_UNAVAILABLE",
        "The native Collaboration handover acknowledgement read is unavailable.",
    )
}

/// Translate the owner `?` placeholders to sqlx `$n` without changing any
/// other byte of the registered statement.
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

/// Postgres-backed owner reader. Composition supplies the pool; no caller can
/// supply SQL, a predicate or a Company outside the routing target.
pub struct PgHandoverAckSource {
    pool: DatabasePool,
}

impl PgHandoverAckSource {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl HandoverPostSource for PgHandoverAckSource {
    fn published_handover<'a>(
        &'a self,
        company: &'a str,
        post_id: &'a str,
        user: &'a str,
    ) -> AckFuture<'a, Option<HandoverPostFacts>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let post_id = post_id.to_owned();
        let user = user.to_owned();
        Box::pin(async move {
            let row = sqlx::query(sqlx::AssertSqlSafe(POST_SQL))
                .bind(post_id.as_str())
                .bind(company.as_str())
                .bind(user.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            let Some(row) = row else {
                return Ok(None);
            };
            Ok(Some(HandoverPostFacts {
                id: row.try_get("id").map_err(|_| unavailable())?,
                author_id: row.try_get("author_id").map_err(|_| unavailable())?,
                version: row.try_get("version").map_err(|_| unavailable())?,
                purpose: row.try_get("purpose").map_err(|_| unavailable())?,
                audience_kind: row
                    .try_get::<Option<String>, _>("audience_kind")
                    .map_err(|_| unavailable())?
                    .unwrap_or_default(),
            }))
        })
    }
}

impl HandoverAckRead for PgHandoverAckSource {
    fn recipients<'a>(
        &'a self,
        post_id: &'a str,
        author: &'a str,
    ) -> AckFuture<'a, Vec<String>> {
        let pool = self.pool.clone();
        let post_id = post_id.to_owned();
        let author = author.to_owned();
        Box::pin(async move {
            let rows = sqlx::query_as::<_, (String,)>(sqlx::AssertSqlSafe(
                render_placeholders(RECIPIENTS_SQL),
            ))
            .bind(post_id.as_str())
            .bind(author.as_str())
            .fetch_all(pool.raw())
            .await
            .map_err(|_| unavailable())?;
            Ok(rows.into_iter().map(|(user,)| user).collect())
        })
    }

    fn acknowledgements<'a>(
        &'a self,
        post_id: &'a str,
        version: i64,
        ids: &'a [String],
    ) -> AckFuture<'a, Vec<String>> {
        let pool = self.pool.clone();
        let post_id = post_id.to_owned();
        let ids = ids.to_vec();
        Box::pin(async move {
            let sql = render_placeholders(&render_acknowledgements_sql(ids.len()));
            let mut query = sqlx::query_as::<_, (String,)>(sqlx::AssertSqlSafe(sql))
                .bind(post_id.as_str())
                .bind(version);
            for id in &ids {
                query = query.bind(id.as_str());
            }
            let rows = query
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            Ok(rows.into_iter().map(|(user,)| user).collect())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::native_r3_handover_ack::ACKNOWLEDGEMENTS_HEAD;

    #[test]
    fn postgres_rendering_keeps_owner_bytes_and_bind_order() {
        assert_eq!(render_placeholders(RECIPIENTS_SQL).matches('$').count(), 2);
        assert_eq!(
            render_placeholders(&render_acknowledgements_sql(2)),
            "SELECT user_id FROM social_handover_acknowledgements\n    WHERE post_id=$1 AND post_version=$2 AND user_id IN ($3,$4)"
        );
        assert!(ACKNOWLEDGEMENTS_HEAD.contains("post_version=?"));
        assert!(POST_SQL.contains("p.company_id=$2"));
        assert!(POST_SQL.contains("sp.user_id=$3"));
    }
}
