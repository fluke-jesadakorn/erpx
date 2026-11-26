//! Owner-local execution contract for the three shared public route families.
//!
//! The composition root implements `NativeRouteBackend` with its already-owned
//! services (Account proof, selected Work context, assignment/Company ports).
//! This crate owns the allow-list, the request validation and the response
//! envelope; no SQL, plan fragment or TypeScript fallback is involved.

use super::domain::{IamOperation, RouteCookie, RouteFamily, RouteInput, RouteMethod};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type RouteFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// One validated request handed to the owner backend. The backend never reads
/// the body as authority: identity, Company and assignment come from the
/// Account proof and the forwarded private context headers.
#[derive(Debug)]
pub struct RouteRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub family: RouteFamily,
    pub operation: Option<IamOperation>,
    pub method: RouteMethod,
    pub headers: &'a HeaderMap,
    pub input: RouteInput,
}

/// Payload parity envelope. `data` is the exact object the public route returns
/// as `{ data }`; `context` is included only for transports that pin it.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteReply {
    pub data: Value,
    pub context: Option<Value>,
    pub cookies: Vec<RouteCookie>,
}

impl RouteReply {
    pub fn data(data: Value) -> Self {
        Self {
            data,
            context: None,
            cookies: Vec::new(),
        }
    }

    pub fn with_context(mut self, context: Value) -> Self {
        self.context = Some(context);
        self
    }

    pub fn with_cookie(mut self, cookie: RouteCookie) -> Self {
        self.cookies.push(cookie);
        self
    }
}

pub trait NativeRouteBackend: Send + Sync + 'static {
    fn execute<'a>(&'a self, request: RouteRequest<'a>) -> RouteFuture<'a, RouteReply>;
}

/// Single shared error for a missing or ambiguous command key.
pub fn missing_key() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::InvalidInput,
        "COMMAND_KEY_REQUIRED",
        "Idempotency-Key is required.",
    )
}

/// Owner-owned key shape, identical across the IAM and active Company routes.
pub fn valid_key(key: &str) -> bool {
    (8..=180).contains(&key.len())
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b":_.-".contains(&byte))
}
