//! Query port for the native R3 project-worker-startup people-options read.
//! No SQL here: the Postgres adapter owns the statement and the domain owns
//! validation and the response DTO.

use super::domain::WorkerPersonOption;
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type PeopleFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader behind the axum router. Composition injects the Pg-backed
/// implementation; the HTTP layer transports only the bounded Company value
/// and one optional trimmed display-name query.
pub trait WorkerPeopleOptionsRead: Send + Sync {
    fn people<'a>(
        &'a self,
        company: &'a str,
        query: &'a str,
    ) -> PeopleFuture<'a, Vec<WorkerPersonOption>>;
}
