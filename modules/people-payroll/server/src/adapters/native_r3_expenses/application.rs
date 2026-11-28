//! Ports for the native R3 expenses ingress. No SQL and no HTTP here: the
//! Postgres adapter owns `finance_expenses` statements; workflow, staff,
//! approval, route, settlement, Files and commercial facts stay behind ports
//! supplied by composition (the owner public surface already declares them).

use super::domain::{ExpenseActor, ExpensePath};
use crate::domain::expense::ExpenseRow;
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type OwnerFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// People-owned expense claim reads. Both methods execute real SQLx in the
/// Pg adapter and return the exact retired TypeScript row shape.
pub trait ExpenseOwnerRead: Send + Sync {
    fn list<'a>(&'a self, actor: &'a ExpenseActor) -> OwnerFuture<'a, Vec<ExpenseRow>>;
    fn detail<'a>(&'a self, actor: &'a ExpenseActor, id: &'a str)
        -> OwnerFuture<'a, ExpenseRow>;
}

/// Root GET also serves the options tree. People owns the claimant slice;
/// Finance accounts, Commerce periods and IAM assignment facts are composed
/// by the caller, so this port stays outside the owner crate.
pub trait ExpenseOptionsPort: Send + Sync {
    fn options<'a>(&'a self, actor: &'a ExpenseActor) -> OwnerFuture<'a, Value>;
}

/// Preview calculation (claimant selection, staff scope, named approval route)
/// is a cross-owner composition of options, Operations staff and Records routes.
pub trait ExpensePreviewPort: Send + Sync {
    fn preview<'a>(
        &'a self,
        actor: &'a ExpenseActor,
        body: &'a Value,
        current_id: Option<&'a str>,
    ) -> OwnerFuture<'a, Value>;
}

/// All POST command branches of the catch-all route. The key is the validated
/// Idempotency-Key; the port owns transaction, receipts and cross-owner ports.
pub trait ExpenseCommandPort: Send + Sync {
    fn execute<'a>(
        &'a self,
        actor: &'a ExpenseActor,
        path: &'a ExpensePath,
        body: &'a Value,
        key: &'a str,
    ) -> OwnerFuture<'a, Value>;
}

/// AI status read and AI enqueue. The worker heartbeat is a Runtime fact, so
/// both stay behind a port rather than being duplicated in the owner crate.
pub trait ExpenseAiPort: Send + Sync {
    fn status<'a>(
        &'a self,
        actor: &'a ExpenseActor,
        id: &'a str,
        payment: Option<&'a str>,
    ) -> OwnerFuture<'a, Value>;
    fn request<'a>(
        &'a self,
        actor: &'a ExpenseActor,
        id: &'a str,
        body: &'a Value,
        key: &'a str,
    ) -> OwnerFuture<'a, Value>;
}
