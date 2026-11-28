//! SQLx execution for the exact media-publication command routes. Every
//! statement below is owner-owned, fully parameterized and runs in the caller
//! Holding's transaction. No SQL text or predicate can arrive from the client.

use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::Row;

use super::application::{CommandContext, CommandFuture, PublicationCommandStore};
use super::domain::{
    OrderInput, approval_event_id, order_event_id, request_hash, revocation_event_id,
};
use crate::adapters::native_media_publications::PublicationRow;

pub const LOCK_SQL: &str = "SELECT pg_advisory_xact_lock(hashtextextended($1,0))";
pub const ORDER_EVENT_SQL: &str = "SELECT details_json FROM attachment_events WHERE id=$1 AND company_id=$2 AND actor_id=$3 AND action='publication.reordered'";
pub const ORDER_VERSION_SQL: &str = "SELECT version FROM attachment_publication_orders WHERE company_id=$1 AND target_type=$2 AND target_id=$3 FOR UPDATE";
pub const ACTIVE_IDS_SQL: &str = "SELECT id FROM attachment_publications WHERE company_id=$1 AND target_type=$2 AND target_id=$3 AND state<>'revoked' ORDER BY id";
pub const UPDATE_ORDINAL_SQL: &str =
    "UPDATE attachment_publications SET ordinal=$1 WHERE id=$2 AND company_id=$3";
pub const BUMP_ORDER_SQL: &str = "\
INSERT INTO attachment_publication_orders(company_id,target_type,target_id,version,updated_by,updated_at) \
VALUES($1,$2,$3,2,$4,$5) ON CONFLICT(company_id,target_type,target_id) DO UPDATE \
SET version=attachment_publication_orders.version+1,updated_by=excluded.updated_by,updated_at=excluded.updated_at \
WHERE attachment_publication_orders.version=$6";
pub const INSERT_EVENT_SQL: &str = "\
INSERT INTO attachment_events(id,company_id,actor_id,action,details_json,created_at) \
VALUES($1,$2,$3,$4,$5,$6)";
pub const BY_ID_SQL: &str = "SELECT * FROM attachment_publications WHERE id=$1 AND company_id=$2";
pub const REVIEW_ROW_SQL: &str = "\
SELECT p.*,s.target_type source_target_type,s.target_id source_target_id,s.component_kind,s.purpose_key \
FROM attachment_publications p JOIN attachment_links l ON l.id=p.source_link_id \
JOIN attachment_sets s ON s.id=l.set_id AND s.company_id=p.company_id \
WHERE p.id=$1 AND p.company_id=$2";
pub const REVIEW_STATE_SQL: &str =
    "SELECT state,created_by FROM attachment_publications WHERE id=$1 AND company_id=$2 FOR UPDATE";
pub const APPROVE_SQL: &str = "\
UPDATE attachment_publications SET state='approved',approved_by=$1,approved_at=$2 \
WHERE id=$3 AND company_id=$4 AND state='draft'";
pub const REVOKE_SQL: &str = "\
UPDATE attachment_publications SET state='revoked',revoked_by=$1,revoked_at=$2,revocation_reason=$3 \
WHERE id=$4 AND company_id=$5 AND state='approved'";
pub const REVIEW_EVENT_SQL: &str = "\
SELECT details_json FROM attachment_events WHERE id=$1 AND company_id=$2 AND actor_id=$3 AND action=$4";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MEDIA_PUBLICATIONS_UNAVAILABLE",
        "The native media publication service is unavailable.",
    )
}

fn fail(kind: FailureKind, code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(kind, code, message)
}

fn transaction_error(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

fn project(row: sqlx::postgres::PgRow) -> Result<PublicationRow, BusinessError> {
    Ok(PublicationRow {
        id: row.try_get("id").map_err(|_| unavailable())?,
        company_id: Some(row.try_get("company_id").map_err(|_| unavailable())?),
        source_link_id: row.try_get("source_link_id").map_err(|_| unavailable())?,
        target_type: row.try_get("target_type").map_err(|_| unavailable())?,
        target_id: row.try_get("target_id").map_err(|_| unavailable())?,
        derivative_object_key: row
            .try_get("derivative_object_key")
            .map_err(|_| unavailable())?,
        derivative_media_type: row
            .try_get("derivative_media_type")
            .map_err(|_| unavailable())?,
        derivative_checksum_sha256: row
            .try_get("derivative_checksum_sha256")
            .map_err(|_| unavailable())?,
        derivative_bytes: row.try_get("derivative_bytes").map_err(|_| unavailable())?,
        alt_text: row.try_get("alt_text").map_err(|_| unavailable())?,
        ordinal: row.try_get("ordinal").map_err(|_| unavailable())?,
        revision: row.try_get("revision").map_err(|_| unavailable())?,
        state: row.try_get("state").map_err(|_| unavailable())?,
        created_by: row.try_get("created_by").map_err(|_| unavailable())?,
        created_at: row.try_get("created_at").map_err(|_| unavailable())?,
        approved_by: row.try_get("approved_by").map_err(|_| unavailable())?,
        approved_at: row.try_get("approved_at").map_err(|_| unavailable())?,
        revoked_by: row.try_get("revoked_by").map_err(|_| unavailable())?,
        revoked_at: row.try_get("revoked_at").map_err(|_| unavailable())?,
        revocation_reason: row.try_get("revocation_reason").map_err(|_| unavailable())?,
    })
}

fn payload(row: sqlx::postgres::PgRow) -> Result<Value, BusinessError> {
    serde_json::to_value(project(row)?)
        .map_err(|_| unavailable())
}

/// One Postgres store per registered Holding pool.
pub struct PgPublicationCommandStore {
    pool: DatabasePool,
}

impl PgPublicationCommandStore {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PublicationCommandStore for PgPublicationCommandStore {
    fn reorder(&self, ctx: &CommandContext, input: &OrderInput) -> CommandFuture<'_, Value> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let ctx = ctx.clone();
        let input = input.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let ctx = ctx.clone();
                let input = input.clone();
                Box::pin(async move {
                    let lock = format!(
                        "publication-order:{}:{}:{}",
                        ctx.company, ctx.user, ctx.key
                    );
                    sqlx::query(LOCK_SQL)
                        .bind(lock.as_str())
                        .execute(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let event_id = order_event_id(&ctx.company, &ctx.user, &ctx.key);
                    let replay: Option<String> = sqlx::query_scalar(ORDER_EVENT_SQL)
                        .bind(event_id.as_str())
                        .bind(ctx.company.as_str())
                        .bind(ctx.user.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    if let Some(details) = replay {
                        let value: Value =
                            serde_json::from_str(&details).map_err(|_| unavailable())?;
                        if value.get("requestHash").and_then(Value::as_str)
                            != Some(request_hash(&input).as_str())
                            || value.get("result").is_none()
                        {
                            return Err(PgTransactionError::Business(fail(
                                FailureKind::Conflict,
                                "IDEMPOTENCY_CONFLICT",
                                "This command key was already used for another public-media order.",
                            )));
                        }
                        return Ok(value.get("result").cloned().unwrap_or(Value::Null));
                    }
                    let version: Option<i64> = sqlx::query_scalar(ORDER_VERSION_SQL)
                        .bind(ctx.company.as_str())
                        .bind(input.target_type.as_str())
                        .bind(input.target_id.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let current = version.unwrap_or(1);
                    if current != input.expected_version {
                        return Err(PgTransactionError::Business(fail(
                            FailureKind::Conflict,
                            "PUBLICATION_ORDER_CHANGED",
                            "Public media order changed. Refresh and retry.",
                        )));
                    }
                    let active: Vec<String> = sqlx::query_scalar(ACTIVE_IDS_SQL)
                        .bind(ctx.company.as_str())
                        .bind(input.target_type.as_str())
                        .bind(input.target_id.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let mut submitted = input
                        .items
                        .iter()
                        .map(|item| item.id.clone())
                        .collect::<Vec<_>>();
                    submitted.sort_unstable();
                    if active != submitted {
                        return Err(PgTransactionError::Business(fail(
                            FailureKind::Conflict,
                            "PUBLICATION_SET_CHANGED",
                            "Public media changed. Refresh and retry.",
                        )));
                    }
                    for item in &input.items {
                        sqlx::query(UPDATE_ORDINAL_SQL)
                            .bind(item.ordinal)
                            .bind(item.id.as_str())
                            .bind(ctx.company.as_str())
                            .execute(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?;
                    }
                    sqlx::query(BUMP_ORDER_SQL)
                        .bind(ctx.company.as_str())
                        .bind(input.target_type.as_str())
                        .bind(input.target_id.as_str())
                        .bind(ctx.user.as_str())
                        .bind(ctx.now)
                        .bind(current)
                        .execute(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let result = serde_json::json!({
                        "targetType": input.target_type,
                        "targetId": input.target_id,
                        "orderVersion": current + 1,
                    });
                    let mut items = input.items.clone();
                    items.sort_by(|left, right| left.id.cmp(&right.id));
                    let details = serde_json::json!({
                        "requestHash": request_hash(&input),
                        "request": OrderInput {
                            target_type: input.target_type.clone(),
                            target_id: input.target_id.clone(),
                            expected_version: input.expected_version,
                            items,
                        },
                        "result": result,
                    })
                    .to_string();
                    sqlx::query(INSERT_EVENT_SQL)
                        .bind(event_id)
                        .bind(ctx.company.as_str())
                        .bind(ctx.user.as_str())
                        .bind("publication.reordered")
                        .bind(details)
                        .bind(ctx.now)
                        .execute(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(result)
                })
            })
            .await
            .map_err(transaction_error)
        })
    }

    fn approve(&self, ctx: &CommandContext, id: &str) -> CommandFuture<'_, Value> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let ctx = ctx.clone();
        let id = id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let ctx = ctx.clone();
                let id = id.clone();
                Box::pin(async move {
                    let row = sqlx::query(REVIEW_ROW_SQL)
                        .bind(id.as_str())
                        .bind(ctx.company.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .ok_or_else(|| {
                            PgTransactionError::Business(fail(
                                FailureKind::NotFound,
                                "PUBLICATION_NOT_FOUND",
                                "Media publication not found.",
                            ))
                        })?;
                    let created_by: String = row.try_get("created_by").map_err(|_| unavailable())?;
                    let state: String = row.try_get("state").map_err(|_| unavailable())?;
                    if created_by == ctx.user {
                        return Err(PgTransactionError::Business(fail(
                            FailureKind::Forbidden,
                            "INDEPENDENT_PUBLICATION_REVIEW",
                            "Another authorized person must approve public media.",
                        )));
                    }
                    if state == "approved" {
                        return payload(row).map_err(PgTransactionError::Business);
                    }
                    if state != "draft" {
                        return Err(PgTransactionError::Business(fail(
                            FailureKind::Conflict,
                            "PUBLICATION_LOCKED",
                            "This publication can no longer be approved.",
                        )));
                    }
                    let current: (String, String) = sqlx::query_as(REVIEW_STATE_SQL)
                        .bind(id.as_str())
                        .bind(ctx.company.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    if current.0 != "draft" {
                        return Err(PgTransactionError::Business(fail(
                            FailureKind::Conflict,
                            "PUBLICATION_LOCKED",
                            "This publication can no longer be approved.",
                        )));
                    }
                    sqlx::query(APPROVE_SQL)
                        .bind(ctx.user.as_str())
                        .bind(ctx.now)
                        .bind(id.as_str())
                        .bind(ctx.company.as_str())
                        .execute(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let link_id: String = row.try_get("source_link_id").map_err(|_| unavailable())?;
                    let revision: i64 = row.try_get("revision").map_err(|_| unavailable())?;
                    let details = serde_json::json!({"publicationId": id, "revision": revision})
                        .to_string();
                    sqlx::query("INSERT INTO attachment_events(id,company_id,link_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,'publication.approved',$5,$6)")
                        .bind(approval_event_id(&ctx.company, &id, &ctx.key))
                        .bind(ctx.company.as_str())
                        .bind(link_id)
                        .bind(ctx.user.as_str())
                        .bind(details)
                        .bind(ctx.now)
                        .execute(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let row = sqlx::query(BY_ID_SQL)
                        .bind(id.as_str())
                        .bind(ctx.company.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    payload(row).map_err(PgTransactionError::Business)
                })
            })
            .await
            .map_err(transaction_error)
        })
    }

    fn revoke(&self, ctx: &CommandContext, id: &str, reason: &str) -> CommandFuture<'_, Value> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let ctx = ctx.clone();
        let id = id.to_owned();
        let reason = reason.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let ctx = ctx.clone();
                let id = id.clone();
                let reason = reason.clone();
                Box::pin(async move {
                    let row = sqlx::query(REVIEW_ROW_SQL)
                        .bind(id.as_str())
                        .bind(ctx.company.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .ok_or_else(|| {
                            PgTransactionError::Business(fail(
                                FailureKind::NotFound,
                                "PUBLICATION_NOT_FOUND",
                                "Media publication not found.",
                            ))
                        })?;
                    let current: (String, String) = sqlx::query_as(REVIEW_STATE_SQL)
                        .bind(id.as_str())
                        .bind(ctx.company.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let existing_reason: Option<String> =
                        row.try_get("revocation_reason").map_err(|_| unavailable())?;
                    if current.0 == "revoked" {
                        if existing_reason.as_deref() != Some(reason.as_str()) {
                            return Err(PgTransactionError::Business(fail(
                                FailureKind::Conflict,
                                "IDEMPOTENCY_CONFLICT",
                                "This publication was revoked for a different reason.",
                            )));
                        }
                        return payload(row).map_err(PgTransactionError::Business);
                    }
                    if current.0 != "approved" {
                        return Err(PgTransactionError::Business(fail(
                            FailureKind::Conflict,
                            "PUBLICATION_NOT_APPROVED",
                            "Only approved public media can be revoked.",
                        )));
                    }
                    sqlx::query(REVOKE_SQL)
                        .bind(ctx.user.as_str())
                        .bind(ctx.now)
                        .bind(reason.as_str())
                        .bind(id.as_str())
                        .bind(ctx.company.as_str())
                        .execute(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let link_id: String = row.try_get("source_link_id").map_err(|_| unavailable())?;
                    let details = serde_json::json!({"publicationId": id, "reason": reason})
                        .to_string();
                    let event_id = revocation_event_id(&ctx.company, &id, &ctx.key);
                    let replay: Option<String> = sqlx::query_scalar(REVIEW_EVENT_SQL)
                        .bind(event_id.as_str())
                        .bind(ctx.company.as_str())
                        .bind(ctx.user.as_str())
                        .bind("publication.revoked")
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    if replay.is_none() {
                        sqlx::query("INSERT INTO attachment_events(id,company_id,link_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,'publication.revoked',$5,$6)")
                            .bind(event_id)
                            .bind(ctx.company.as_str())
                            .bind(link_id)
                            .bind(ctx.user.as_str())
                            .bind(details)
                            .bind(ctx.now)
                            .execute(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?;
                    }
                    let row = sqlx::query(BY_ID_SQL)
                        .bind(id.as_str())
                        .bind(ctx.company.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    payload(row).map_err(PgTransactionError::Business)
                })
            })
            .await
            .map_err(transaction_error)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_sql_is_fully_parameterized() {
        assert!(LOCK_SQL.contains("hashtextextended($1,0)"));
        assert!(ORDER_VERSION_SQL.contains("FOR UPDATE"));
        assert!(BUMP_ORDER_SQL.contains("version=$6"));
        assert!(APPROVE_SQL.contains("state='draft'"));
        assert!(REVOKE_SQL.contains("state='approved'"));
        assert!(!REVIEW_ROW_SQL.contains('?'));
    }
}
