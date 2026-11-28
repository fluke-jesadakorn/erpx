//! Query port for the work-review person route. No SQL lives here.

use super::domain::RoutePerson;
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type RouteFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader port. Composition injects the Pg-backed implementation; the
/// router transports only bounded Company and user-id values.
pub trait PersonRouteRead: Send + Sync {
    fn route<'a>(&'a self, company: &'a str, users: &'a [String]) -> RouteFuture<'a, Vec<RoutePerson>>;
}
