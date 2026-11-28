//! Native Customer 360 timeline activity read use case.
//!
//! Repository SQL stays on the caller's retained serializable Holding
//! transaction. The application applies the current actor policy and the
//! Support history purchase before the read, validates the bounded query and
//! every returned row, and verifies the full database ordering. A source row
//! that violates any bound fails closed.
use crate::application::timeline::{
    CustomerTimelineFuture, CustomerTimelineSupportEntitlement,
};
use crate::domain::timeline::{CustomerTimelineActor, CustomerTimelineView};
use crate::domain::timeline_activity::{
    CustomerTimelineActivityQuery, CustomerTimelineActivityRow, MAX_ACTIVITY_ROWS,
    activity_invalid,
};
use erpx_platform_shared_server::public::{BusinessError, TimestampMillis};

/// Read-only federated activity over the existing public source owners. The
/// caller's serializable Holding transaction is required; no source is written.
pub trait CustomerTimelineActivityRepository: Send {
    type Error: From<BusinessError> + Send;

    fn activity<'a>(
        &'a mut self,
        actor: &'a CustomerTimelineActor,
        view: CustomerTimelineView,
        query: &'a CustomerTimelineActivityQuery,
        limit: usize,
    ) -> CustomerTimelineFuture<'a, Vec<CustomerTimelineActivityRow>, Self::Error>;
}

/// Return every bounded activity row in database `(at, id)` order.
pub async fn customer_timeline_activity<R, E>(
    actor: &CustomerTimelineActor,
    query: &CustomerTimelineActivityQuery,
    repository: &mut R,
    entitlement: &mut E,
    now: TimestampMillis,
) -> Result<Vec<CustomerTimelineActivityRow>, R::Error>
where
    R: CustomerTimelineActivityRepository,
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
    let rows = repository
        .activity(actor, view, query, MAX_ACTIVITY_ROWS + 1)
        .await?;
    if rows.len() > MAX_ACTIVITY_ROWS {
        return Err(activity_invalid().into());
    }
    let mut previous: Option<&CustomerTimelineActivityRow> = None;
    for row in &rows {
        row.validate()?;
        if row.at < query.from || row.at >= query.to {
            return Err(activity_invalid().into());
        }
        if query
            .customer_id
            .as_deref()
            .is_some_and(|customer_id| row.customer_id != customer_id)
        {
            return Err(activity_invalid().into());
        }
        if let Some(previous) = previous {
            if row.at > previous.at || (row.at == previous.at && row.id >= previous.id) {
                return Err(activity_invalid().into());
            }
        }
        previous = Some(row);
    }
    Ok(rows)
}
