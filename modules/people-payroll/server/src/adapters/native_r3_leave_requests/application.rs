//! Owner ports for the native R3 leave-requests HTTP slice.
//!
//! No SQL is written here. The paged own-leave read port is executed by the
//! Postgres adapter in `postgres.rs`. Session authority, other-owner facts
//! (Workflow approvals, Organization routing, Operations staff), commercial
//! package admission and the retained native leave application commands are
//! injected by composition through `LeaveRequestsBackend`, which returns the
//! exact retired `data` payload. This keeps owner SQL in the owner crate while
//! leaving no TypeScript SQL or business fallback in this slice.

use super::domain::{LeaveCommandKind, LeaveCursor, LeaveListFilter, LeaveListRow};
use erpx_platform_runtime_server::public::http_support::HeaderMap;
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type LeaveFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Resolved, scoped actor. Composition fills this from the same Account session
/// the retired `requireWorkContext(holdingSlug)` used; the owner router never
/// mints authority itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaveActor {
    pub company: String,
    pub user: String,
    pub engagement: String,
    pub branch: Option<String>,
    pub department: Option<String>,
    pub evidence_reader: bool,
}

/// The one owner-owned read that had no native SQL before this slice: the
/// paged "my leave" list, including the keyset cursor and strict filter.
pub trait LeaveRequestsRead: Send + Sync {
    fn list<'a>(
        &'a self,
        actor: &'a LeaveActor,
        filter: LeaveListFilter,
        cursor: Option<&'a LeaveCursor>,
        limit: usize,
    ) -> LeaveFuture<'a, Vec<LeaveListRow>>;
}

/// Composition-owned authority and retained native application operations.
/// `detail` and `command` reuse the already-native SQLx in `adapters/leave.rs`
/// and `application/leave.rs`; returning `Value` preserves the exact retired
/// `{...row, approval, permittedActions, purchase?}` shape without a second
/// owner implementation or a TypeScript fallback.
pub trait LeaveRequestsBackend: Send + Sync {
    fn actor<'a>(
        &'a self,
        host: &'a str,
        holding_slug: &'a str,
        headers: &'a HeaderMap,
    ) -> LeaveFuture<'a, LeaveActor>;

    fn detail<'a>(
        &'a self,
        actor: &'a LeaveActor,
        id: &'a str,
    ) -> LeaveFuture<'a, Option<Value>>;

    /// Unpaged `listOwnLeave`, which is `leaveSummary(context).history`.
    fn history<'a>(&'a self, actor: &'a LeaveActor) -> LeaveFuture<'a, Vec<Value>>;

    fn command<'a>(
        &'a self,
        actor: &'a LeaveActor,
        kind: LeaveCommandKind,
        id: Option<&'a str>,
        body: &'a Value,
        key: Option<&'a str>,
    ) -> LeaveFuture<'a, Value>;
}
