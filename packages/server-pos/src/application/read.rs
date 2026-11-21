//! Native POS workspace/day read use case.
//!
//! Repository SQL stays on the caller's retained serializable Holding
//! transaction. The application checks the current POS read policy, the exact
//! bounded query and the composition-supplied package decision before any owner
//! read, then validates and narrows the source into the exact owner payload.
use crate::domain::read::{
    PosReadActor, PosWorkspace, PosWorkspaceEntitlements, PosWorkspaceQuery, PosWorkspaceSource,
    assemble, business_date,
};
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use std::future::Future;

pub type PosReadFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait PosWorkspaceRepository: Send {
    type Error: From<BusinessError> + Send;

    fn workspace<'a>(
        &'a mut self,
        actor: &'a PosReadActor,
        query: &'a PosWorkspaceQuery,
        business_date: &'a str,
    ) -> PosReadFuture<'a, PosWorkspaceSource, Self::Error>;
}

pub async fn pos_workspace<R: PosWorkspaceRepository>(
    actor: &PosReadActor,
    query: &PosWorkspaceQuery,
    entitlements: &PosWorkspaceEntitlements,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<PosWorkspace, R::Error> {
    actor.require_read(now)?;
    query.validate()?;
    entitlements.require_capability(query.tab)?;
    let business_date = business_date(now);
    let source = repository.workspace(actor, query, &business_date).await?;
    assemble(actor, query, entitlements, &business_date, source).map_err(Into::into)
}
