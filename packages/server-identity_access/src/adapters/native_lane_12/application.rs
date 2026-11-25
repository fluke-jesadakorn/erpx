//! Owner read port for lane 12. No SQL and no transport here: the port only
//! names a bounded read and returns its already-projected response envelope.

use crate::adapters::native_lane_12::domain::{AttributePage, AttributeRead};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type AttributeReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the owner reader behind this port. The owner owns the
/// statement, bind order, company scope and projection; the router transports.
pub trait PeopleTemplateRead: Send + Sync {
    fn read<'a>(&'a self, read: &'a AttributeRead) -> AttributeReadFuture<'a, AttributePage>;
}
