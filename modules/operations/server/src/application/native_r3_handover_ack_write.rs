//! Application port and command handler for the native acknowledgement POST.
//!
//! The owner module executes the recipient assertion and the acknowledgement
//! insert through `AckWriteStore`; the workflow ledger and the post authority
//! are injected ports because composition owns those units. This layer contains
//! no SQL, pool, transaction or transport.

use crate::application::native_r3_handover_ack::{
    AckFuture, HandoverAckHandler, HandoverAckRead, HandoverPostSource,
};
use crate::domain::native_r3_handover_ack::{
    not_found, validate_post_id, validate_user_id,
};
use crate::domain::native_r3_handover_ack_write::{
    ACK_OPERATION, idempotency_conflict, parse_version, recipient_required, source_changed,
    validate_key,
};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct AckReceipt {
    pub operation: String,
    pub result: Value,
}

/// Owner statement execution. Every implementation is the only layer allowed
/// to touch the acknowledgement tables.
pub trait AckWriteStore: Send + Sync {
    fn receipt<'a>(&'a self, key: &'a str) -> AckFuture<'a, Option<AckReceipt>>;

    fn assert_recipient<'a>(
        &'a self,
        post_id: &'a str,
        user: &'a str,
        version: i64,
    ) -> AckFuture<'a, ()>;

    fn insert<'a>(
        &'a self,
        post_id: &'a str,
        user: &'a str,
        version: i64,
        now: i64,
        key: &'a str,
    ) -> AckFuture<'a, ()>;
}

#[derive(Clone, Copy, Debug)]
pub struct AckCommit<'a> {
    pub key: &'a str,
    pub company: &'a str,
    pub user: &'a str,
    pub post_id: &'a str,
    pub post_version: i64,
}

/// Workflow command ledger. Composition must bind this port and `AckWriteStore`
/// to the same transaction so the receipt, audit, outbox and acknowledgement
/// insert commit atomically.
pub trait AckCommandLedger: Send + Sync {
    fn commit<'a>(&'a self, command: AckCommit<'a>) -> AckFuture<'a, ()>;
}

#[derive(Clone)]
pub struct AckWriteHandler {
    posts: Arc<dyn HandoverPostSource>,
    view: HandoverAckHandler,
    store: Arc<dyn AckWriteStore>,
    ledger: Arc<dyn AckCommandLedger>,
}

impl AckWriteHandler {
    pub fn new(
        posts: Arc<dyn HandoverPostSource>,
        read: Arc<dyn HandoverAckRead>,
        post_source: Arc<dyn HandoverPostSource>,
        store: Arc<dyn AckWriteStore>,
        ledger: Arc<dyn AckCommandLedger>,
    ) -> Self {
        Self {
            posts,
            view: HandoverAckHandler::new(read, post_source),
            store,
            ledger,
        }
    }

    /// Exact retained POST behaviour: header-bound version check, idempotent
    /// replay, recipient-only authority, one owner insert, then the six-field
    /// acknowledgement view.
    pub fn acknowledge<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
        post_id: &'a str,
        key: &'a str,
        version_header: Option<&'a str>,
        now: i64,
    ) -> AckFuture<'a, Value> {
        let company = company.to_owned();
        let user = user.to_owned();
        let post_id = post_id.to_owned();
        let key = key.to_owned();
        let version_header = version_header.map(str::to_owned);
        Box::pin(async move {
            validate_post_id(&post_id)?;
            validate_user_id(&user)?;
            validate_user_id(&company)?;
            validate_key(&key)?;
            let expected = parse_version(version_header.as_deref())?;

            let post = self
                .posts
                .published_handover(&company, &post_id, &user)
                .await?
                .ok_or_else(not_found)?;
            if expected.is_some_and(|version| version != post.version) {
                return Err(source_changed());
            }

            if let Some(receipt) = self.store.receipt(&key).await? {
                if receipt.operation == ACK_OPERATION
                    && receipt.result.get("postId").and_then(Value::as_str) == Some(post_id.as_str())
                {
                    return Ok(receipt.result);
                }
                return Err(idempotency_conflict());
            }

            let view = self.view.read(&company, &user, &post_id).await?;
            if !view.can_acknowledge {
                return Err(recipient_required());
            }
            if view.acknowledged {
                return Ok(json!(view));
            }

            self.store
                .assert_recipient(&post_id, &user, post.version)
                .await?;
            self.ledger
                .commit(AckCommit {
                    key: &key,
                    company: &company,
                    user: &user,
                    post_id: &post_id,
                    post_version: post.version,
                })
                .await?;
            self.store
                .insert(&post_id, &user, post.version, now, &key)
                .await?;
            Ok(json!(
                self.view.read(&company, &user, &post_id).await?
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::native_r3_handover_ack::HandoverPostFacts;
    use std::sync::Mutex;

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
    struct Read {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl HandoverAckRead for Read {
        fn recipients<'a>(
            &'a self,
            _post_id: &'a str,
            _author: &'a str,
        ) -> AckFuture<'a, Vec<String>> {
            Box::pin(async move { Ok(vec!["u-1".into(), "u-2".into()]) })
        }

        fn acknowledgements<'a>(
            &'a self,
            _post_id: &'a str,
            _version: i64,
            _ids: &'a [String],
        ) -> AckFuture<'a, Vec<String>> {
            let written = !self.seen.lock().unwrap().is_empty();
            Box::pin(async move {
                Ok(if written {
                    vec!["u-2".into()]
                } else {
                    Vec::new()
                })
            })
        }
    }

    #[derive(Clone, Default)]
    struct Store {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl AckWriteStore for Store {
        fn receipt<'a>(&'a self, _key: &'a str) -> AckFuture<'a, Option<AckReceipt>> {
            Box::pin(async { Ok(None) })
        }

        fn assert_recipient<'a>(
            &'a self,
            post_id: &'a str,
            user: &'a str,
            version: i64,
        ) -> AckFuture<'a, ()> {
            assert_eq!((post_id, user, version), ("post-1", "u-2", 3));
            Box::pin(async { Ok(()) })
        }

        fn insert<'a>(
            &'a self,
            _post_id: &'a str,
            _user: &'a str,
            _version: i64,
            _now: i64,
            key: &'a str,
        ) -> AckFuture<'a, ()> {
            self.seen.lock().unwrap().push(key.to_owned());
            Box::pin(async { Ok(()) })
        }
    }

    #[derive(Clone)]
    struct Ledger;

    impl AckCommandLedger for Ledger {
        fn commit<'a>(&'a self, _command: AckCommit<'a>) -> AckFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }
    }

    fn handler(store: Store) -> AckWriteHandler {
        let seen = store.seen.clone();
        AckWriteHandler::new(
            Arc::new(Source),
            Arc::new(Read { seen }),
            Arc::new(Source),
            Arc::new(store),
            Arc::new(Ledger),
        )
    }

    const KEY: &str = "01234567-89ab-cdef-0123-456789abcdef";

    #[tokio::test]
    async fn acknowledge_runs_owner_commands_and_returns_the_view() {
        let store = Store::default();
        let view = handler(store.clone())
            .acknowledge("acme", "u-2", "post-1", KEY, Some("3"), 1)
            .await
            .expect("acknowledged");
        assert_eq!(view["postId"], "post-1");
        assert_eq!(view["postVersion"], 3);
        assert_eq!(view["acknowledged"], true);
        assert_eq!(view["recipientCount"], 2);
        assert_eq!(store.seen.lock().unwrap().as_slice(), [KEY]);
    }

    #[tokio::test]
    async fn stale_version_and_missing_recipient_fail_before_the_write() {
        let store = Store::default();
        let error = handler(store.clone())
            .acknowledge("acme", "u-2", "post-1", KEY, Some("4"), 1)
            .await
            .unwrap_err();
        assert_eq!(error.code, "HANDOVER_SOURCE_CHANGED");
        assert!(store.seen.lock().unwrap().is_empty());

        let error = handler(store.clone())
            .acknowledge("acme", "u-9", "post-1", KEY, None, 1)
            .await
            .unwrap_err();
        assert_eq!(error.code, "HANDOVER_RECIPIENT_REQUIRED");
        assert!(store.seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn invalid_command_key_fails_before_any_owner_read() {
        let error = handler(Store::default())
            .acknowledge("acme", "u-2", "post-1", "bad", None, 1)
            .await
            .unwrap_err();
        assert_eq!(error.code, "COMMAND_KEY_REQUIRED");
    }
}
