//! Files-owned upload cleanup for canceled Social drafts.
//!
//! Composition binds this port to
//! [`erpx_server_files::public::social_draft_uploads::revoke_social_draft_uploads`]
//! so the Collaboration Social unit never writes Files-owned `attachments` or
//! `upload_intents` rows. It runs on the caller's retained transaction and
//! must not commit, retry or open a nested unit.
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

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


