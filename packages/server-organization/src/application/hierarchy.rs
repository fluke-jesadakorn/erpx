use crate::domain::hierarchy::{
    CompanyContext, HierarchyLineage, WorkspaceRouteQuery, company_context, workspace_missing,
};
use erpx_server_shared::public::{BusinessError, CompanyId, HoldingId};
use std::{future::Future, pin::Pin};
pub type OrganizationReadFuture<'a, T, E = BusinessError> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;
/// Bound to exactly one trusted Holding resource at composition, never selected by a Company string.
pub trait OrganizationReadPort: Send {
    type Error: From<BusinessError> + Send;
    fn holding_id(&self) -> &HoldingId;
    fn workspace_candidates<'a>(
        &'a mut self,
        query: &'a WorkspaceRouteQuery,
    ) -> OrganizationReadFuture<'a, Vec<HierarchyLineage>, Self::Error>;
    fn company<'a>(
        &'a mut self,
        id: &'a CompanyId,
    ) -> OrganizationReadFuture<'a, Option<HierarchyLineage>, Self::Error>;
}
pub async fn resolve_workspace<P: OrganizationReadPort + ?Sized>(
    port: &mut P,
    query: &WorkspaceRouteQuery,
) -> Result<Option<CompanyContext>, P::Error> {
    if query.is_empty() {
        return Ok(None);
    }
    let holding = port.holding_id().clone();
    let mut candidates = port.workspace_candidates(query).await?;
    if candidates.len() > 1 {
        return Err(workspace_missing().into());
    }
    candidates
        .pop()
        .map(|lineage| {
            if query.matches(&lineage) {
                company_context(holding, lineage)
            } else {
                Err(workspace_missing())
            }
        })
        .transpose()
        .map_err(Into::into)
}
pub async fn read_company_context<P: OrganizationReadPort + ?Sized>(
    port: &mut P,
    id: &CompanyId,
) -> Result<CompanyContext, P::Error> {
    let holding = port.holding_id().clone();
    let lineage = port.company(id).await?.ok_or_else(workspace_missing)?;
    if lineage.company.company_id != *id {
        return Err(workspace_missing().into());
    }
    company_context(holding, lineage).map_err(Into::into)
}
