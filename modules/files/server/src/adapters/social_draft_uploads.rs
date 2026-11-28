//! Files-owned projections and cleanup for Social upload drafts.
//!
//! Collaboration owns the social draft row and its cancellation receipt. The
//! Files owner owns the attachment revocation and upload-intent expiry that
//! follow it. This module exposes those two effects on the caller's retained
//! connection so no other owner writes `attachments` or `upload_intents`; it
//! never begins, commits or retries a transaction and mints no authority.
use erpx_collaboration_server::public::social_upload_files::SocialCommentDraftWitness;
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::BusinessError;
use serde::Serialize;
use sqlx::{Connection, PgConnection};
use std::collections::HashSet;

/// Exact bounded facts the Files owner needs. Only the canceled draft's live
/// original attachments and ready upload intents change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SocialDraftUploadRevokeRequest<'a> {
    pub draft_id: &'a str,
    pub now: i64,
}

pub struct SocialCommentFilesRequest<'a> {
    pub draft: &'a SocialCommentDraftWitness,
    pub selected_attachment_ids: &'a [String],
    pub now: i64,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct SocialHomeImage {
    pub post_id: String,
    pub id: String,
    pub original_name: String,
    pub media_type: String,
}

/// Project the visible draft images for one already authorized Social post.
/// The caller runs this inside the transaction that authorized the post.
pub async fn read_social_home_images(
    connection: &mut PgConnection,
    company_id: &str,
    post_id: &str,
) -> Result<Vec<SocialHomeImage>, PgTransactionError<BusinessError>> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("SOCIAL_IMAGE_TRANSACTION_REQUIRED").into());
    }
    let rows = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT p.id,a.id,a.original_name,a.media_type FROM social_posts p
         JOIN social_post_drafts d ON d.company_id=p.company_id
           AND (d.published_post_id=p.id OR (d.edit_post_id=p.id AND d.edit_saved_version IS NOT NULL))
         JOIN attachments a ON a.owner_id=d.id AND a.owner_type='social_draft'
         WHERE p.id=$1 AND p.company_id=$2 AND p.state='published' AND a.revoked_at IS NULL
         ORDER BY a.created_at,a.id",
    )
    .bind(post_id)
    .bind(company_id)
    .fetch_all(connection)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(post_id, id, original_name, media_type)| SocialHomeImage {
            post_id,
            id,
            original_name,
            media_type,
        })
        .collect())
}

/// Validate the exact selected draft images and revoke only unused draft files.
/// This must run after the Social owner's locked draft/authority check, on the
/// same serializable transaction, before binding the published comment. The
/// caller owns commit and serialization retries; Files never opens a unit.
pub async fn retain_social_comment_images(
    connection: &mut PgConnection,
    request: SocialCommentFilesRequest<'_>,
) -> Result<(), PgTransactionError<BusinessError>> {
    let witness = request.draft;
    let selected = request.selected_attachment_ids;
    if selected.len() > 10
        || selected.iter().any(|id| id.is_empty())
        || selected.iter().collect::<HashSet<_>>().len() != selected.len()
    {
        return Err(BusinessError::invalid(
            "COMMENT_IMAGES_INVALID",
            "Choose up to ten unique uploaded images.",
        )
        .into());
    }
    witness.require_current(connection).await?;
    let matched: Vec<String> = sqlx::query_scalar(
        "SELECT a.id FROM attachments a JOIN upload_intents u ON u.id=a.upload_id \
         WHERE a.id=ANY($1) AND a.owner_type='social_draft' AND a.owner_id=$2 \
           AND a.revoked_at IS NULL AND a.media_type IN ('image/jpeg','image/png','image/webp') \
           AND u.owner_type='social_draft' AND u.owner_id=$2 AND u.status='ready' \
         FOR UPDATE OF a,u",
    )
    .bind(selected)
    .bind(witness.draft_id())
    .fetch_all(&mut *connection)
    .await?;
    if matched.len() != selected.len() {
        return Err(BusinessError::invalid(
            "COMMENT_IMAGE_UNAVAILABLE",
            "An image is unavailable. Upload it again.",
        )
        .into());
    }
    sqlx::query(
        "UPDATE attachments SET revoked_at=$1 WHERE owner_type='social_draft' AND owner_id=$2 \
         AND revoked_at IS NULL AND NOT (id=ANY($3))",
    )
    .bind(request.now)
    .bind(witness.draft_id())
    .bind(selected)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

fn require_request(
    request: &SocialDraftUploadRevokeRequest<'_>,
) -> Result<(), PgTransactionError<BusinessError>> {
    if request.draft_id.is_empty() {
        return Err(BusinessError::invalid(
            "SOCIAL_DRAFT_ID_REQUIRED",
            "A canceled Social draft identity is required.",
        )
        .into());
    }
    Ok(())
}

/// Withdraw every still-live attachment and expire every ready upload intent
/// owned by one canceled `social_draft`.
pub async fn revoke_social_draft_uploads(
    connection: &mut PgConnection,
    request: SocialDraftUploadRevokeRequest<'_>,
) -> Result<(), PgTransactionError<BusinessError>> {
    require_request(&request)?;
    sqlx::query(
        "UPDATE attachments SET revoked_at=$1 \
         WHERE owner_type='social_draft' AND owner_id=$2 AND revoked_at IS NULL",
    )
    .bind(request.now)
    .bind(request.draft_id)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "UPDATE upload_intents SET status='expired',updated_at=$1 \
         WHERE owner_type='social_draft' AND owner_id=$2 AND status='ready'",
    )
    .bind(request.now)
    .bind(request.draft_id)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_draft_identity_is_rejected_before_sql() {
        let request = SocialDraftUploadRevokeRequest {
            draft_id: "",
            now: 10,
        };
        match require_request(&request) {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "SOCIAL_DRAFT_ID_REQUIRED")
            }
            _ => panic!("empty draft identity was accepted"),
        }
        assert!(
            require_request(&SocialDraftUploadRevokeRequest {
                draft_id: "comment-upload-key",
                now: 10,
            })
            .is_ok()
        );
    }
}
