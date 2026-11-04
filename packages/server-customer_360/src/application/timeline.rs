//! Native Customer 360 federated timeline read use case.
//!
//! Repository SQL stays on the caller's retained serializable Holding
//! transaction. Every source owner remains authoritative; this owner never
//! writes or revises a source record. The application enforces the current
//! actor policy and Support history purchase before the customer or event
//! read, validates query/cursor scope before the source read, validates every
//! bounded event and derives the next cursor only from the requested page plus
//! one overflow row.
use crate::domain::timeline::{
    CustomerAccount, CustomerTimelineActor, CustomerTimelineCursor, CustomerTimelineItem,
    CustomerTimelinePage, CustomerTimelinePageData, CustomerTimelineQuery, CustomerTimelineView,
    cursor_scope, decode_cursor, source_invalid, validate_page,
};
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId, TimestampMillis};
use std::future::Future;
use std::pin::Pin;

pub type CustomerTimelineFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Read-only federation over the existing public source owners. The caller's
/// serializable Holding transaction is required; no source row is written.
pub trait CustomerTimelineRepository: Send {
    type Error: From<BusinessError> + Send;

    fn customer<'a>(
        &'a mut self,
        actor: &'a CustomerTimelineActor,
        customer_id: &'a str,
        view: CustomerTimelineView,
    ) -> CustomerTimelineFuture<'a, Option<CustomerAccount>, Self::Error>;

    fn events<'a>(
        &'a mut self,
        actor: &'a CustomerTimelineActor,
        customer_id: &'a str,
        view: CustomerTimelineView,
        cursor: Option<&'a CustomerTimelineCursor>,
        limit: usize,
    ) -> CustomerTimelineFuture<'a, Vec<CustomerTimelineItem>, Self::Error>;
}

/// Current Commercial Support history entitlement through a public port. A
/// denied history hides the Support source; it does not fail the whole
/// timeline or interrupt other visible owners.
pub trait CustomerTimelineSupportEntitlement {
    type Error: Send;

    fn support_history<'a>(
        &'a mut self,
        company_id: &'a str,
    ) -> CustomerTimelineFuture<'a, bool, Self::Error>;
}

pub async fn customer_timeline<R, E>(
    actor: &CustomerTimelineActor,
    customer_id: &str,
    query: &CustomerTimelineQuery,
    repository: &mut R,
    entitlement: &mut E,
    now: TimestampMillis,
) -> Result<CustomerTimelinePage, R::Error>
where
    R: CustomerTimelineRepository,
    E: CustomerTimelineSupportEntitlement,
    R::Error: From<E::Error>,
{
    actor.require_read(now)?;
    query.validate()?;
    if RecordId::new(customer_id.to_owned()).is_err() {
        return Err(BusinessError::invalid(
            "CUSTOMER_TIMELINE_CUSTOMER_INVALID",
            "Choose a customer available in this workspace.",
        )
        .into());
    }
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
    let scope = cursor_scope(actor, customer_id)?;
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| decode_cursor(value, &scope))
        .transpose()?;
    let limit = query.normalized_limit()?;
    let customer = repository
        .customer(actor, customer_id, view)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "CUSTOMER_TIMELINE_NOT_FOUND",
                "Customer not found in this scope.",
            )
        })?;
    customer.validate(customer_id)?;
    let source = repository
        .events(actor, customer_id, view, cursor.as_ref(), limit + 1)
        .await?;
    let CustomerTimelinePageData {
        items,
        next_cursor,
        has_more,
    } = validate_page(cursor.as_ref(), source, limit, &scope, view)?;
    if has_more && items.is_empty() {
        return Err(source_invalid().into());
    }
    Ok(CustomerTimelinePage {
        customer,
        items,
        has_more,
        next_cursor,
        coverage: view.coverage(),
        limitations: crate::domain::timeline::LIMITATIONS,
    })
}
