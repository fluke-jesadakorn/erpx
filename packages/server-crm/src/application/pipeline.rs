//! Native CRM pipeline read use case.
//!
//! Repository SQL stays on the caller's retained serializable Holding
//! transaction. Cross-owner staff and customer labels arrive through public
//! ports supplied by composition; this owner never reads another owner's
//! tables. The application enforces the current actor policy, current package
//! history entitlement, cursor scope and every bounded owner row and forecast
//! total before presentation.
use crate::domain::pipeline::{
    CrmForecastSourceRow, CrmPipelineActor, CrmPipelineCursor, CrmPipelineEntitlement,
    CrmPipelinePage, CrmPipelinePageData, CrmPipelineQuery, CrmPipelineRow, cursor_scope,
    decode_cursor, source_invalid, validate_directory_name, validate_forecast, validate_page,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;

pub type CrmPipelineFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait CrmPipelineRepository: Send {
    type Error: From<BusinessError> + Send;
    #[allow(clippy::too_many_arguments)]
    fn page<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
        search: &'a str,
        stage: Option<&'a str>,
        cursor: Option<&'a CrmPipelineCursor>,
        limit: usize,
    ) -> CrmPipelineFuture<'a, Vec<CrmPipelineRow>, Self::Error>;
    fn forecast<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
    ) -> CrmPipelineFuture<'a, Vec<CrmForecastSourceRow>, Self::Error>;
}

/// Current People staff labels through the public People boundary. A label is
/// presentation only and grants no assignment or command authority.
pub trait CrmPipelinePeopleDirectory {
    type Error: Send;
    fn names<'a>(
        &'a mut self,
        company_id: &'a str,
        user_ids: &'a [String],
    ) -> CrmPipelineFuture<'a, BTreeMap<String, String>, Self::Error>;
}

/// Current Finance customer labels through the public Finance boundary.
pub trait CrmPipelineCustomerDirectory {
    type Error: Send;
    fn names<'a>(
        &'a mut self,
        company_id: &'a str,
        customer_ids: &'a [String],
    ) -> CrmPipelineFuture<'a, BTreeMap<String, String>, Self::Error>;
}

/// Current Commercial package boundary. Disabled enforcement is the retained
/// policy; when enforced the current `sales.crm` history entitlement is
/// required before any owner row is read.
pub trait CrmPipelineEntitlementPort {
    type Error: Send;
    fn entitlement<'a>(
        &'a mut self,
        company_id: &'a str,
    ) -> CrmPipelineFuture<'a, CrmPipelineEntitlement, Self::Error>;
}

#[allow(clippy::too_many_arguments)]
pub async fn crm_pipeline<R, P, C, E>(
    actor: &CrmPipelineActor,
    query: &CrmPipelineQuery,
    repository: &mut R,
    people: &mut P,
    customers: &mut C,
    entitlement: &mut E,
    now: TimestampMillis,
) -> Result<CrmPipelinePage, R::Error>
where
    R: CrmPipelineRepository,
    P: CrmPipelinePeopleDirectory,
    C: CrmPipelineCustomerDirectory,
    E: CrmPipelineEntitlementPort,
    R::Error: From<P::Error> + From<C::Error> + From<E::Error>,
{
    actor.require_pipeline(now)?;
    query.validate()?;
    let search = query.normalized_search();
    let stage = query.normalized_stage()?;
    let entitlement = entitlement
        .entitlement(actor.company_id.as_str())
        .await
        .map_err(R::Error::from)?;
    entitlement.require_history()?;
    let scope = cursor_scope(actor, &search, stage.as_deref())?;
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| decode_cursor(value, &scope))
        .transpose()?;
    let source = repository
        .page(
            &actor.company_id,
            actor.branch_id.as_ref().map(|value| value.as_str()),
            actor.department_id.as_ref().map(|value| value.as_str()),
            &search,
            stage.as_deref(),
            cursor.as_ref(),
            crate::domain::pipeline::PAGE_SIZE + 1,
        )
        .await?;
    let CrmPipelinePageData {
        mut items,
        next_cursor,
        has_more,
    } = validate_page(
        actor,
        &search,
        stage.as_deref(),
        cursor.as_ref(),
        source,
        &scope,
    )?;
    let owner_ids = unique_ids(items.iter().map(|row| row.owner_id.as_str()));
    let customer_ids = unique_ids(items.iter().filter_map(|row| row.customer_id.as_deref()));
    let owner_names = people
        .names(actor.company_id.as_str(), &owner_ids)
        .await
        .map_err(R::Error::from)?;
    let customer_names = customers
        .names(actor.company_id.as_str(), &customer_ids)
        .await
        .map_err(R::Error::from)?;
    for name in owner_names.values().chain(customer_names.values()) {
        validate_directory_name(name)?;
    }
    for row in &mut items {
        row.owner_name = owner_names.get(&row.owner_id).cloned();
        row.customer_name = row
            .customer_id
            .as_ref()
            .and_then(|id| customer_names.get(id))
            .cloned();
    }
    let forecast = validate_forecast(
        repository
            .forecast(
                &actor.company_id,
                actor.branch_id.as_ref().map(|value| value.as_str()),
                actor.department_id.as_ref().map(|value| value.as_str()),
            )
            .await?,
    )?;
    if next_cursor.is_some() && items.is_empty() {
        return Err(source_invalid().into());
    }
    Ok(CrmPipelinePage {
        items,
        forecast,
        can_create: actor.can_create(),
        purchase: entitlement.purchase(),
        next_cursor,
        has_more,
    })
}

fn unique_ids<'a>(ids: impl Iterator<Item = &'a str>) -> Vec<String> {
    ids.collect::<BTreeSet<_>>()
        .into_iter()
        .map(str::to_owned)
        .collect()
}
