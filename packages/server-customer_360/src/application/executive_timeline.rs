//! Native Executive customer-activity metric source use case.
//!
//! The Commerce executive metric engine calls this read once for the selected
//! window and breakdown. The caller supplies the current actor, Support history
//! purchase state and the serializable Holding transaction; this owner only
//! gates the same federated source view as the timeline and bounds the groups.
use crate::application::timeline::CustomerTimelineSupportEntitlement;
use crate::domain::executive_timeline::{
    CustomerActivityMetricGroup, CustomerActivityMetricQuery, validate_groups,
};
use crate::domain::timeline::{CustomerTimelineActor, CustomerTimelineView};
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use std::future::Future;
use std::pin::Pin;

pub type CustomerActivityMetricFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait CustomerActivityMetricRepository: Send {
    type Error: From<BusinessError> + Send;

    fn metric<'a>(
        &'a mut self,
        actor: &'a CustomerTimelineActor,
        view: CustomerTimelineView,
        query: &'a CustomerActivityMetricQuery,
    ) -> CustomerActivityMetricFuture<'a, Vec<CustomerActivityMetricGroup>, Self::Error>;
}

pub async fn customer_activity_metric<R, E>(
    actor: &CustomerTimelineActor,
    query: &CustomerActivityMetricQuery,
    repository: &mut R,
    entitlement: &mut E,
    now: TimestampMillis,
) -> Result<Vec<CustomerActivityMetricGroup>, R::Error>
where
    R: CustomerActivityMetricRepository,
    E: CustomerTimelineSupportEntitlement,
    R::Error: From<E::Error>,
{
    actor.require_read(now)?;
    query.validate()?;
    let support_purchased = if actor.has("support.read") {
        entitlement
            .support_history(actor.company_id.as_str())
            .await
            .map_err(R::Error::from)?
    } else {
        false
    };
    let view = CustomerTimelineView {
        support: actor.support_visible(support_purchased),
        field_service: actor.field_service_visible(),
        finance: actor.finance_visible(),
        legal: actor.legal_visible(),
    };
    let groups = repository.metric(actor, view, query).await?;
    validate_groups(&groups, query.breakdown).map_err(business_error::<R::Error>)?;
    Ok(groups)
}

fn business_error<E: From<BusinessError>>(error: BusinessError) -> E {
    E::from(error)
}
