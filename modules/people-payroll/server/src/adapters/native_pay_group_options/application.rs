//! Query port for the native active pay-group options read. No SQL here: the
//! Postgres adapter owns the statement and the domain owns validation.

use super::domain::PayGroupOption;
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type OptionsFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader behind the axum router. Composition injects the Pg-backed
/// implementation; the HTTP layer transports only the bounded Company value.
pub trait PayGroupOptionsRead: Send + Sync {
    fn options<'a>(&'a self, company: &'a str) -> OptionsFuture<'a, Vec<PayGroupOption>>;
}
