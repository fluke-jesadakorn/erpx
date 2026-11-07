//! Native Manufacturing BOM register use case.
//!
//! Repository SQL stays on the caller's retained serializable Holding
//! transaction. The application enforces the current actor policy and
//! validates every bounded owner row before presentation.
use crate::domain::bom::{BomActor, BomRegister, BomSpecification, validate_register};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use std::future::Future;

pub type BomReadFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait BomReadRepository: Send {
    type Error: From<BusinessError> + Send;

    fn specifications<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> BomReadFuture<'a, Vec<BomSpecification>, Self::Error>;
}

pub async fn bom_register<R: BomReadRepository>(
    actor: &BomActor,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<BomRegister, R::Error> {
    actor.require_read(now)?;
    let specifications = repository.specifications(&actor.company_id).await?;
    validate_register(actor, specifications).map_err(Into::into)
}
