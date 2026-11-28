//! Query port for the native R3 country-packs read. No SQL here: the Postgres
//! adapter owns the statement and the domain owns validation and the DTO.

use super::domain::CountryPackRow;
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type CountryPackFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader behind the axum router. Composition injects the Pg-backed
/// implementation; the HTTP layer transports only the bounded Company value.
pub trait CountryPacksRead: Send + Sync {
    fn packs<'a>(&'a self, company: &'a str) -> CountryPackFuture<'a, Vec<CountryPackRow>>;
}
