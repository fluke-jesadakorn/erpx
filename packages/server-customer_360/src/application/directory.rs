//! Native Customer 360 customer directory read use case.
use crate::domain::directory::{
    CustomerDirectoryEntry, CustomerDirectoryView, directory_invalid, directory_view,
};
use crate::domain::timeline::CustomerTimelineActor;
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use std::future::Future;
use std::pin::Pin;

pub type CustomerDirectoryFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait CustomerDirectoryRepository: Send {
    type Error: From<BusinessError> + Send;

    fn customers<'a>(
        &'a mut self,
        actor: &'a CustomerTimelineActor,
        view: CustomerDirectoryView,
    ) -> CustomerDirectoryFuture<'a, Vec<CustomerDirectoryEntry>, Self::Error>;
}

pub async fn customer_directory<R>(
    actor: &CustomerTimelineActor,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Vec<CustomerDirectoryEntry>, R::Error>
where
    R: CustomerDirectoryRepository,
{
    actor.require_directory_read(now)?;
    let view = directory_view(actor);
    let entries = repository.customers(actor, view).await?;
    for entry in &entries {
        entry.validate().map_err(R::Error::from)?;
    }
    if entries.len() > 201 {
        return Err(directory_invalid().into());
    }
    Ok(entries)
}
