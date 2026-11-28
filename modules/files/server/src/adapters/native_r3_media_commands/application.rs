//! Owner command application for the three exact media-publication command
//! routes. Validation and authority decisions are pure; every statement is
//! executed by the SQLx store behind `PublicationCommandStore`.

use std::{future::Future, pin::Pin};

use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;

use super::domain::{OrderInput, can_review};

pub type CommandFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct CommandContext {
    pub company: String,
    pub user: String,
    pub holding_id: String,
    pub permissions: Vec<String>,
    pub key: String,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct CommandTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
}

pub trait PublicationCommandStore: Send + Sync {
    fn reorder(&self, ctx: &CommandContext, input: &OrderInput) -> CommandFuture<'_, Value>;
    fn approve(&self, ctx: &CommandContext, id: &str) -> CommandFuture<'_, Value>;
    fn revoke(&self, ctx: &CommandContext, id: &str, reason: &str) -> CommandFuture<'_, Value>;
}

fn approver_required() -> BusinessError {
    BusinessError::new(
        erpx_platform_shared_server::public::FailureKind::Forbidden,
        "PUBLICATION_APPROVER_REQUIRED",
        "An authorized reviewer must review public media.",
    )
}

/// POST /media-publications/order. The store owns the lock, replay witness,
/// version/set guards, ordinal writes and audit event in one Holding unit.
pub fn reorder<'a, S: PublicationCommandStore + ?Sized>(
    store: &'a S,
    ctx: &'a CommandContext,
    input: &'a OrderInput,
) -> CommandFuture<'a, Value> {
    store.reorder(ctx, input)
}

/// POST /media-publications/{id}/approve. Independent review is decided here,
/// before any statement runs, exactly as the TS application decides it.
pub fn approve<'a, S: PublicationCommandStore + ?Sized>(
    store: &'a S,
    ctx: &'a CommandContext,
    id: &'a str,
) -> CommandFuture<'a, Value> {
    if !can_review(&ctx.permissions) {
        return Box::pin(async { Err(approver_required()) });
    }
    store.approve(ctx, id)
}

/// POST /media-publications/{id}/revoke. A reviewer is required for both the
/// first revocation and its replay.
pub fn revoke<'a, S: PublicationCommandStore + ?Sized>(
    store: &'a S,
    ctx: &'a CommandContext,
    id: &'a str,
    reason: &'a str,
) -> CommandFuture<'a, Value> {
    if !can_review(&ctx.permissions) {
        return Box::pin(async { Err(approver_required()) });
    }
    store.revoke(ctx, id, reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reviewer_permission_gates_both_review_commands() {
        let ctx = |permissions: &[&str]| CommandContext {
            company: "c".into(),
            user: "u".into(),
            holding_id: "h".into(),
            permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
            key: "key-12345".into(),
            now: 1,
        };
        let denied = approve(&Noop, &ctx(&[]), "p").await.unwrap_err();
        assert_eq!(denied.code, "PUBLICATION_APPROVER_REQUIRED");
        let denied = revoke(&Noop, &ctx(&["work.read"]), "p", "because").await.unwrap_err();
        assert_eq!(denied.code, "PUBLICATION_APPROVER_REQUIRED");
        assert!(approve(&Noop, &ctx(&["records.review"]), "p").await.is_ok());
        assert!(revoke(&Noop, &ctx(&["commerce.approve"]), "p", "because").await.is_ok());
    }

    struct Noop;
    impl PublicationCommandStore for Noop {
        fn reorder(&self, _: &CommandContext, _: &OrderInput) -> CommandFuture<'_, Value> {
            Box::pin(async { Ok(Value::Null) })
        }
        fn approve(&self, _: &CommandContext, _: &str) -> CommandFuture<'_, Value> {
            Box::pin(async { Ok(Value::Null) })
        }
        fn revoke(&self, _: &CommandContext, _: &str, _: &str) -> CommandFuture<'_, Value> {
            Box::pin(async { Ok(Value::Null) })
        }
    }
}
