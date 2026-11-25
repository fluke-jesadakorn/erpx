//! Query port and projection for the native person-image read. No SQL lives here.

use super::domain::{PersonImageFacts, validate_image};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type PersonImageFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader port. Composition injects the Pg-backed implementation; the
/// router transports bounded identities and never sees a connection or a query.
pub trait PersonImageRead: Send + Sync {
    fn image<'a>(
        &'a self,
        company: &'a str,
        person: &'a str,
    ) -> PersonImageFuture<'a, Option<PersonImageFacts>>;
}

/// Validate and shape the raw scalar returned by the owner query. The string is
/// kept byte-identical to the original TS projection so it is a stable oracle;
/// invalid shapes fail closed instead of leaking a partial snapshot.
pub fn project(raw: Option<String>) -> Result<Option<PersonImageFacts>, BusinessError> {
    raw.map(|image| {
        validate_image(&image)?;
        Ok(PersonImageFacts { image })
    })
    .transpose()
}
