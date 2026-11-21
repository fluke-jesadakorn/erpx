//! Collaboration-owned writes for the Workflow PMS social projector.
//!
//! Exact port of `packages/server-collaboration/src/adapters/pms-social-event-facts.ts`.
//! Every statement runs on the caller's `&mut sqlx::PgConnection`; the caller
//! owns the transaction and commit, so the post/source/receipt batch is atomic.
use erpx_server_runtime::public::postgres::PersistenceError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;

/// The retained `PmsSocialPost` row body. `source_id`, `event_id` and
/// `received_at` are explicit publish arguments in the native owner port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PmsSocialPost {
    pub post_id: String,
    pub company_id: String,
    pub branch_id: String,
    pub actor_id: String,
    pub body: String,
    pub command_id: String,
    pub at: i64,
}

/// The retained `PmsSocialNotice` row body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PmsSocialNotice {
    pub id: String,
    pub company_id: String,
    pub user_id: String,
    pub post_id: String,
    pub version: Option<i64>,
    pub body: String,
    pub at: i64,
}

/// Exact retained Social post upsert plus source link and PMS delivery receipt.
pub async fn pms_social_publish(
    connection: &mut PgConnection,
    post: PmsSocialPost,
    source_id: &str,
    event_id: &str,
    received_at: i64,
) -> Result<(), BusinessError> {
    sqlx::query(
        "INSERT INTO social_posts(id,kind,company_id,branch_id,author_id,author_name,body,\
         audience_kind,purpose,last_command_id,created_at,updated_at)\n\
         VALUES($1,'system',$2,$3,$4,'PMS',$5,'branch','update',$6,$7,$8) ON CONFLICT(id) DO UPDATE SET\n\
         body=excluded.body,last_command_id=excluded.last_command_id,version=social_posts.version+1,\
         updated_at=excluded.updated_at\n\
         WHERE social_posts.updated_at<=excluded.updated_at",
    )
    .bind(&post.post_id)
    .bind(&post.company_id)
    .bind(&post.branch_id)
    .bind(&post.actor_id)
    .bind(&post.body)
    .bind(&post.command_id)
    .bind(post.at)
    .bind(post.at)
    .execute(&mut *connection)
    .await
    .map_err(write_failed)?;

    sqlx::query(
        "INSERT INTO social_post_sources(post_id,ordinal,source_kind,source_id) \
         VALUES($1,0,'pms_record',$2) ON CONFLICT DO NOTHING",
    )
    .bind(&post.post_id)
    .bind(source_id)
    .execute(&mut *connection)
    .await
    .map_err(write_failed)?;

    sqlx::query(
        "INSERT INTO pms_social_event_receipts(event_id,post_id,created_at) VALUES($1,$2,$3)",
    )
    .bind(event_id)
    .bind(&post.post_id)
    .bind(received_at)
    .execute(&mut *connection)
    .await
    .map_err(write_failed)?;
    Ok(())
}

/// Exact retained advisory transaction lock for one business-day post.
pub async fn pms_social_lock(
    connection: &mut PgConnection,
    post_id: &str,
) -> Result<(), BusinessError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(post_id)
        .execute(&mut *connection)
        .await
        .map_err(write_failed)?;
    Ok(())
}

/// Exact retained receipt existence check.
pub async fn pms_social_received(
    connection: &mut PgConnection,
    event_id: &str,
) -> Result<bool, BusinessError> {
    let row = sqlx::query("SELECT 1 FROM pms_social_event_receipts WHERE event_id=$1")
        .bind(event_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(read_failed)?;
    Ok(row.is_some())
}

/// Exact retained current Social post version.
pub async fn pms_social_version(
    connection: &mut PgConnection,
    post_id: &str,
) -> Result<Option<i64>, BusinessError> {
    sqlx::query_scalar("SELECT version FROM social_posts WHERE id=$1")
        .bind(post_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(read_failed)
}

/// Exact retained staff notification upsert.
pub async fn pms_social_notify(
    connection: &mut PgConnection,
    notice: PmsSocialNotice,
) -> Result<(), BusinessError> {
    sqlx::query(
        "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
         record_version,message,created_at)\n\
         VALUES($1,$2,$3,'post',$4,$5,$6,$7) ON CONFLICT(id) DO UPDATE SET \
         record_version=excluded.record_version,message=excluded.message,\n\
         created_at=excluded.created_at,read_at=NULL \
         WHERE staff_notifications.created_at<=excluded.created_at",
    )
    .bind(&notice.id)
    .bind(&notice.company_id)
    .bind(&notice.user_id)
    .bind(&notice.post_id)
    .bind(notice.version)
    .bind(&notice.body)
    .bind(notice.at)
    .execute(&mut *connection)
    .await
    .map_err(write_failed)?;
    Ok(())
}

fn read_failed(error: sqlx::Error) -> BusinessError {
    let persistence = PersistenceError::from(error);
    if persistence.retryable() {
        BusinessError::new(
            FailureKind::Conflict,
            "COLLABORATION_SOCIAL_RETRY_REQUIRED",
            "The PMS social write must be retried.",
        )
    } else {
        BusinessError::new(
            FailureKind::Unavailable,
            "COLLABORATION_SOCIAL_READ_FAILED",
            "The PMS social write could not be read.",
        )
    }
}

fn write_failed(error: sqlx::Error) -> BusinessError {
    let persistence = PersistenceError::from(error);
    if persistence.retryable() {
        BusinessError::new(
            FailureKind::Conflict,
            "COLLABORATION_SOCIAL_RETRY_REQUIRED",
            "The PMS social write must be retried.",
        )
    } else {
        BusinessError::new(
            FailureKind::Unavailable,
            "COLLABORATION_SOCIAL_WRITE_FAILED",
            "The PMS social write could not be stored.",
        )
    }
}
