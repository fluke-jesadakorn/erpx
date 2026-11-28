//! Application port for the native CRM/marketing report read.
//!
//! Composition owns the Account session, current Company scope, purchased
//! history and the serializable owner unit. This port only names the future
//! returned by that owner read; no transaction is opened or retried here.
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type CrmMarketingReportFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, erpx_server_shared::public::BusinessError>> + Send + 'a>>;

pub type CrmMarketingReportPayload = Value;
