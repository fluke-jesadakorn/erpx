//! Pure domain contract for the native handover-acknowledgement GET read.
//!
//! This is the native read half of
//! `GET /api/v1/holdings/{holdingSlug}/handover/{postId}/acknowledgement`.
//! The retained TypeScript `handoverAcknowledgement` executes three reads and
//! projects one six-field view; this module owns the exact owner SQL shapes,
//! the identity bounds, the visibility invariant and that projection. No axum,
//! sqlx, pool or I/O lives here.

use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use serde::Serialize;

/// Byte-identical to the retired `social_handover_recipients` owner statement
/// in `notification_statement.rs`: ordered binds are post_id, author_id.
pub const RECIPIENTS_SQL: &str =
    "SELECT user_id FROM social_post_participants WHERE post_id=? AND user_id<>?";

/// Byte-identical head of the retired `social_handover_acknowledgements` owner
/// statement. The caller renders exactly one `?` per closed recipient id; the
/// ordered binds are post_id, post_version, then the recipient ids.
pub const ACKNOWLEDGEMENTS_HEAD: &str = concat!(
    "SELECT user_id FROM social_handover_acknowledgements\n",
    "    WHERE post_id=? AND post_version=? AND user_id IN ("
);

/// Company-scoped, published, recipient-visible handover post facts. The
/// composition source resolves the same audience authority as the retained
/// Social `readablePost` port before handing these facts to the native read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandoverPostFacts {
    pub id: String,
    pub author_id: String,
    pub version: i64,
    pub purpose: String,
    pub audience_kind: String,
}

/// The exact `{data}` payload the retained Next route returns. Every field is
/// camelCase and there is no destination/href field, so
/// `canonicalDestinationLinks` is identity on this payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverAcknowledgementView {
    pub post_id: String,
    pub post_version: i64,
    pub can_acknowledge: bool,
    pub acknowledged: bool,
    pub recipient_count: usize,
    pub acknowledged_count: usize,
}

/// Render the owner acknowledgement statement for a closed recipient list.
/// No request value reaches the SQL text: only the count of `?` placeholders.
pub fn render_acknowledgements_sql(recipient_count: usize) -> String {
    let mut sql = String::with_capacity(ACKNOWLEDGEMENTS_HEAD.len() + recipient_count * 2 + 1);
    sql.push_str(ACKNOWLEDGEMENTS_HEAD);
    for index in 0..recipient_count {
        if index > 0 {
            sql.push(',');
        }
        sql.push('?');
    }
    sql.push(')');
    sql
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_HANDOVER_ACK_INVALID",
        "Use one bounded handover acknowledgement read.",
    )
}

/// Path and context identities use the same bounded character set as other
/// Operations owner reads: never empty, never longer than 200 bytes, and no
/// whitespace, SQL metacharacters or placeholders.
fn validate_identity(value: &str) -> Result<(), BusinessError> {
    if value.is_empty()
        || value.len() > 200
        || RecordId::new(value).is_err()
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
        })
    {
        return Err(invalid());
    }
    Ok(())
}

pub fn validate_post_id(value: &str) -> Result<(), BusinessError> {
    validate_identity(value)
}

pub fn validate_user_id(value: &str) -> Result<(), BusinessError> {
    validate_identity(value)
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "HANDOVER_NOT_FOUND",
        "This handover is unavailable.",
    )
}

/// Project the six-field view from the already-authorized post facts and the
/// two owner read results. The purpose/audience guard is the retained
/// `HANDOVER_NOT_FOUND` rule; acknowledgement counts and flags are byte-for-byte
/// the retained application projection.
pub fn project(
    post: &HandoverPostFacts,
    post_id: &str,
    actor_user_id: &str,
    recipients: &[String],
    acknowledgements: &[String],
) -> Result<HandoverAcknowledgementView, BusinessError> {
    if post.purpose != "handover" || post.audience_kind != "participants" {
        return Err(not_found());
    }
    let can_acknowledge = recipients.iter().any(|id| id == actor_user_id);
    let acknowledged = can_acknowledge && acknowledgements.iter().any(|id| id == actor_user_id);
    Ok(HandoverAcknowledgementView {
        post_id: post_id.to_owned(),
        post_version: post.version,
        can_acknowledge,
        acknowledged,
        recipient_count: recipients.len(),
        acknowledged_count: acknowledgements.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post() -> HandoverPostFacts {
        HandoverPostFacts {
            id: "post-1".into(),
            author_id: "author-1".into(),
            version: 3,
            purpose: "handover".into(),
            audience_kind: "participants".into(),
        }
    }

    #[test]
    fn owner_sql_keeps_the_exact_retired_bytes() {
        assert_eq!(
            RECIPIENTS_SQL,
            "SELECT user_id FROM social_post_participants WHERE post_id=? AND user_id<>?"
        );
        assert_eq!(
            render_acknowledgements_sql(2),
            "SELECT user_id FROM social_handover_acknowledgements\n    WHERE post_id=? AND post_version=? AND user_id IN (?,?)"
        );
        assert_eq!(
            render_acknowledgements_sql(0),
            "SELECT user_id FROM social_handover_acknowledgements\n    WHERE post_id=? AND post_version=? AND user_id IN ()"
        );
    }

    #[test]
    fn identities_are_bounded() {
        assert!(validate_post_id("post-1").is_ok());
        assert!(validate_user_id("user:1_2.3").is_ok());
        assert!(validate_post_id("").is_err());
        assert!(validate_user_id("bad id").is_err());
        assert!(validate_post_id("x' OR 1=1").is_err());
        assert!(validate_user_id(&"x".repeat(201)).is_err());
    }

    #[test]
    fn projection_matches_the_retained_application_view() {
        let recipients = vec!["u-1".to_owned(), "u-2".to_owned()];
        let acks = vec!["u-2".to_owned()];
        let view = project(&post(), "post-1", "u-2", &recipients, &acks).unwrap();
        assert_eq!(
            serde_json::to_value(&view).unwrap(),
            serde_json::json!({
                "postId": "post-1",
                "postVersion": 3,
                "canAcknowledge": true,
                "acknowledged": true,
                "recipientCount": 2,
                "acknowledgedCount": 1
            })
        );
        let stranger = project(&post(), "post-1", "u-9", &recipients, &acks).unwrap();
        assert!(!stranger.can_acknowledge);
        assert!(!stranger.acknowledged);
        assert_eq!(stranger.recipient_count, 2);
    }

    #[test]
    fn non_handover_posts_keep_the_not_found_rule() {
        let mut post = post();
        post.purpose = "update".into();
        let error = project(
            &post,
            "post-1",
            "u-1",
            &["u-1".to_owned()],
            &["u-1".to_owned()],
        )
        .unwrap_err();
        assert_eq!(error.kind, FailureKind::NotFound);
        assert_eq!(error.code, "HANDOVER_NOT_FOUND");
    }
}
