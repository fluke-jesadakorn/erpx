//! Files-owned upload cleanup for canceled Social drafts.
//!
//! Composition binds this port to
//! [`erpx_files_server::public::social_draft_uploads::revoke_social_draft_uploads`]
//! so the Collaboration Social unit never writes Files-owned `attachments` or
//! `upload_intents` rows. It runs on the caller's retained transaction and
//! must not commit, retry or open a nested unit.
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
use std::future::Future;
use std::pin::Pin;

type Unit = (String, i32, String, String);
type Error = PgTransactionError<BusinessError>;

async fn unit(connection: &mut PgConnection) -> Result<Unit, Error> {
    if !connection.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *connection)
            .await?
            != "serializable"
    {
        return Err(BusinessError::invalid(
            "SOCIAL_FILES_TRANSACTION_REQUIRED",
            "Social files require a serializable owner transaction.",
        )
        .into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text")
        .fetch_one(&mut *connection).await?)
}

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "COMMENT_UPLOAD_UNAVAILABLE",
        "Comment images belong to an unavailable draft.",
    )
    .into()
}

/// Locked, transaction-bound Social facts for Files; this value is not an IAM
/// grant. The caller must have resolved current actor/post access separately.
#[derive(Debug)]
pub struct SocialCommentDraftWitness {
    draft_id: String,
    company_id: String,
    author_id: String,
    post_id: String,
    version: i64,
    unit: Unit,
}

impl SocialCommentDraftWitness {
    pub fn draft_id(&self) -> &str {
        &self.draft_id
    }

    /// Recheck the same locked draft and transaction before Files reads/writes.
    pub async fn require_current(&self, connection: &mut PgConnection) -> Result<(), Error> {
        if unit(connection).await? != self.unit {
            return Err(unavailable());
        }
        let version = sqlx::query_scalar::<_, i64>("SELECT version FROM social_post_drafts WHERE id=$1 AND company_id=$2 AND author_id=$3 AND comment_parent_post_id=$4 AND published_post_id IS NULL AND published_comment_id IS NULL AND edit_post_id IS NULL AND cancelled_at IS NULL FOR UPDATE")
            .bind(&self.draft_id).bind(&self.company_id).bind(&self.author_id).bind(&self.post_id)
            .fetch_optional(&mut *connection).await?;
        if version != Some(self.version) {
            return Err(unavailable());
        }
        Ok(())
    }
}

/// Social owns and checks draft identity/state; lock it on the existing owner
/// unit. Call only after current actor, post visibility and purchase authority
/// have been checked by the Social application.
pub async fn lock_social_comment_draft(
    connection: &mut PgConnection,
    draft_id: &str,
    company_id: &str,
    author_id: &str,
    post_id: &str,
) -> Result<SocialCommentDraftWitness, Error> {
    if [draft_id, company_id, author_id, post_id]
        .iter()
        .any(|value| value.is_empty())
    {
        return Err(unavailable());
    }
    let current_unit = unit(connection).await?;
    let version = sqlx::query_scalar::<_, i64>("SELECT version FROM social_post_drafts WHERE id=$1 AND company_id=$2 AND author_id=$3 AND comment_parent_post_id=$4 AND published_post_id IS NULL AND published_comment_id IS NULL AND edit_post_id IS NULL AND cancelled_at IS NULL FOR UPDATE")
        .bind(draft_id).bind(company_id).bind(author_id).bind(post_id)
        .fetch_optional(&mut *connection).await?.ok_or_else(unavailable)?;
    Ok(SocialCommentDraftWitness {
        draft_id: draft_id.into(),
        company_id: company_id.into(),
        author_id: author_id.into(),
        post_id: post_id.into(),
        version,
        unit: current_unit,
    })
}

pub type SocialUploadFilesFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), BusinessError>> + Send + 'a>>;

/// Files owner port for canceled Social upload drafts. The implementation
/// must revoke only the retained draft's live attachments and expire only its
/// ready upload intents, never mint authority or begin a transaction.
pub trait SocialUploadFilesPort: Send {
    fn revoke_draft_uploads<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        draft_id: &'a str,
        now: i64,
    ) -> SocialUploadFilesFuture<'a>;
}

/// Fail-closed port used when composition has not bound the Files owner.
/// A cancel command must not commit a draft cancellation without the Files
/// cleanup, so this returns an error rather than silently skipping it.
pub struct UnavailableSocialUploadFiles;

impl SocialUploadFilesPort for UnavailableSocialUploadFiles {
    fn revoke_draft_uploads<'a>(
        &'a mut self,
        _connection: &'a mut PgConnection,
        _draft_id: &'a str,
        _now: i64,
    ) -> SocialUploadFilesFuture<'a> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "SOCIAL_UPLOAD_FILES_PORT_UNAVAILABLE",
                "The Files upload cleanup port is unavailable.",
            ))
        })
    }
}
