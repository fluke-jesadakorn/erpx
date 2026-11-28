//! Application port and handler for the native handover acknowledgement read.
//!
//! The two owner statements and the post-facts read are executed only by the
//! reader/source ports below. This layer validates identities, applies the
//! retained not-found and visibility rules and assembles the exact six-field
//! view; it contains no SQL, connection or transport.

use crate::domain::native_r3_handover_ack::{
    HandoverAcknowledgementView, HandoverPostFacts, not_found, project, validate_post_id,
    validate_user_id,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin, sync::Arc};

pub type AckFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Authorized published-post facts. The composition source resolves the same
/// audience authority as the retained Social `readablePost` port; the owner
/// never mints a post or a permission from the wire.
pub trait HandoverPostSource: Send + Sync {
    fn published_handover<'a>(
        &'a self,
        company: &'a str,
        post_id: &'a str,
        user: &'a str,
    ) -> AckFuture<'a, Option<HandoverPostFacts>>;
}

/// Owner reader for the two acknowledgement statements. Implementations are
/// the only layer allowed to execute owner SQL.
pub trait HandoverAckRead: Send + Sync {
    fn recipients<'a>(&'a self, post_id: &'a str, author: &'a str)
        -> AckFuture<'a, Vec<String>>;

    fn acknowledgements<'a>(
        &'a self,
        post_id: &'a str,
        version: i64,
        ids: &'a [String],
    ) -> AckFuture<'a, Vec<String>>;
}

/// Validation, visibility and response assembly live here; the axum handler
/// only transports the resulting view.
#[derive(Clone)]
pub struct HandoverAckHandler {
    reader: Arc<dyn HandoverAckRead>,
    posts: Arc<dyn HandoverPostSource>,
}

impl HandoverAckHandler {
    pub fn new(
        reader: Arc<dyn HandoverAckRead>,
        posts: Arc<dyn HandoverPostSource>,
    ) -> Self {
        Self { reader, posts }
    }

    pub fn read<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
        post_id: &'a str,
    ) -> AckFuture<'a, HandoverAcknowledgementView> {
        let company = company.to_owned();
        let user = user.to_owned();
        let post_id = post_id.to_owned();
        Box::pin(async move {
            validate_identity_or_error(&company)?;
            validate_identity_or_error(&user)?;
            validate_post_id(&post_id)?;
            let Some(post) = self.posts.published_handover(&company, &post_id, &user).await?
            else {
                return Err(not_found());
            };
            let recipients = self.reader.recipients(&post_id, &post.author_id).await?;
            let acknowledgements = if recipients.is_empty() {
                Vec::new()
            } else {
                self.reader
                    .acknowledgements(&post_id, post.version, &recipients)
                    .await?
            };
            project(&post, &post_id, &user, &recipients, &acknowledgements)
        })
    }
}

fn validate_identity_or_error(value: &str) -> Result<(), BusinessError> {
    validate_user_id(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Source;

    impl HandoverPostSource for Source {
        fn published_handover<'a>(
            &'a self,
            _company: &'a str,
            _post_id: &'a str,
            _user: &'a str,
        ) -> AckFuture<'a, Option<HandoverPostFacts>> {
            Box::pin(async move {
                Ok(Some(HandoverPostFacts {
                    id: "post-1".into(),
                    author_id: "author-1".into(),
                    version: 3,
                    purpose: "handover".into(),
                    audience_kind: "participants".into(),
                }))
            })
        }
    }

    #[derive(Clone)]
    struct Reader;

    impl HandoverAckRead for Reader {
        fn recipients<'a>(
            &'a self,
            post_id: &'a str,
            author: &'a str,
        ) -> AckFuture<'a, Vec<String>> {
            assert_eq!(post_id, "post-1");
            assert_eq!(author, "author-1");
            Box::pin(async move { Ok(vec!["u-1".into(), "u-2".into()]) })
        }

        fn acknowledgements<'a>(
            &'a self,
            _post_id: &'a str,
            version: i64,
            ids: &'a [String],
        ) -> AckFuture<'a, Vec<String>> {
            assert_eq!(version, 3);
            assert_eq!(ids, &["u-1".to_owned(), "u-2".to_owned()]);
            Box::pin(async move { Ok(vec!["u-2".into()]) })
        }
    }

    fn handler() -> HandoverAckHandler {
        HandoverAckHandler::new(Arc::new(Reader), Arc::new(Source))
    }

    #[tokio::test]
    async fn read_builds_the_retained_view_with_ordered_owner_reads() {
        let view = handler()
            .read("acme", "u-2", "post-1")
            .await
            .expect("readable handover");
        assert_eq!(view.post_id, "post-1");
        assert_eq!(view.post_version, 3);
        assert!(view.can_acknowledge);
        assert!(view.acknowledged);
        assert_eq!(view.recipient_count, 2);
        assert_eq!(view.acknowledged_count, 1);
    }

    #[tokio::test]
    async fn invalid_identity_and_post_id_fail_before_the_owner_read() {
        assert!(handler().read("acme", "u-2", "bad id").await.is_err());
        assert!(handler().read("", "u-2", "post-1").await.is_err());
    }

    #[tokio::test]
    async fn missing_post_keeps_the_not_found_rule() {
        #[derive(Clone)]
        struct Missing;
        impl HandoverPostSource for Missing {
            fn published_handover<'a>(
                &'a self,
                _company: &'a str,
                _post_id: &'a str,
                _user: &'a str,
            ) -> AckFuture<'a, Option<HandoverPostFacts>> {
                Box::pin(async move { Ok(None) })
            }
        }
        let handler = HandoverAckHandler::new(Arc::new(Reader), Arc::new(Missing));
        let error = handler.read("acme", "u-2", "post-1").await.unwrap_err();
        assert_eq!(error.code, "HANDOVER_NOT_FOUND");
    }
}
