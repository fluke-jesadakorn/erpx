//! Collaboration-owned task-origin binding for native Staff Work commands.
//!
//! The caller owns the serializable Holding transaction. This adapter writes
//! only `social_task_origins`; reading the published post and its source-access
//! policy remains the Collaboration social-source port composed by the caller.
use erpx_server_runtime::public::postgres::{PgTransactionError, PersistenceError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

const SOURCE_INVALID: BusinessError = BusinessError::new(
    FailureKind::Conflict,
    "SOURCE_CHANGED",
    "The conversation changed. Review it and prepare the task again.",
);

pub struct TaskOriginBinding<'a> {
    pub post_id: &'a str,
    pub task_id: &'a str,
    pub source_version: i64,
    pub source_hash: &'a str,
    pub ai_draft_id: Option<&'a str>,
    pub actor_id: &'a str,
    pub now: i64,
}

async fn ready(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("TASK_ORIGIN_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("TASK_ORIGIN_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Bind one task to its reviewed social-post source. The guard rechecks the
/// post version and source hash inside the caller transaction so a concurrent
/// post revision cannot leave a stale origin behind.
pub async fn bind_task_origin(
    connection: &mut PgConnection,
    binding: &TaskOriginBinding<'_>,
) -> Result<(), Error> {
    ready(connection).await?;
    let current: Option<(i64, String)> = sqlx::query_as(
        "SELECT version::bigint,source_hash FROM social_posts WHERE id=$1 AND state='published'",
    )
    .bind(binding.post_id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some((version, hash)) = current else {
        return Err(SOURCE_INVALID.into());
    };
    if version != binding.source_version || hash != binding.source_hash {
        return Err(SOURCE_INVALID.into());
    }
    sqlx::query(
        "INSERT INTO social_task_origins(post_id,task_id,source_version,source_hash,\
         ai_draft_id,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7) \
         ON CONFLICT(post_id,task_id) DO NOTHING",
    )
    .bind(binding.post_id)
    .bind(binding.task_id)
    .bind(binding.source_version)
    .bind(binding.source_hash)
    .bind(binding.ai_draft_id)
    .bind(binding.actor_id)
    .bind(binding.now)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

pub fn into_business(error: Error) -> BusinessError {
    match error {
        Error::Business(error) => error,
        Error::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "TASK_ORIGIN_UNAVAILABLE",
            "The Collaboration task-origin port could not complete.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{SOURCE_INVALID, into_business};
    use erpx_server_runtime::public::postgres::PgTransactionError;

    #[test]
    fn stale_source_is_portable() {
        assert_eq!(SOURCE_INVALID.code, "SOURCE_CHANGED");
        assert_eq!(
            into_business(PgTransactionError::Business(SOURCE_INVALID.clone())),
            SOURCE_INVALID
        );
    }
}
