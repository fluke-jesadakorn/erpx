use erpx_integration_contracts::{ReferenceCode, maintenance};

use crate::domain::{DomainEvent, WorkOrderStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    InvalidWorkOrderIdentifier,
    InvalidWorkOrderReference,
}

pub(crate) fn published_events(
    events: &[DomainEvent],
    work_order_code: &str,
) -> Result<Vec<maintenance::Event>, IntegrationMappingError> {
    events
        .iter()
        .filter_map(|event| map_event(event, work_order_code))
        .collect()
}

fn map_event(
    event: &DomainEvent,
    work_order_code: &str,
) -> Option<Result<maintenance::Event, IntegrationMappingError>> {
    let (work_order_id, asset, status, scheduled_for) = match event {
        DomainEvent::Opened {
            work_order_id,
            asset,
            request_code,
            at,
        } => {
            let _ = (request_code, at);
            (*work_order_id, *asset, WorkOrderStatus::Planned, None)
        }
        DomainEvent::StatusChanged {
            work_order_id,
            asset,
            status,
            scheduled_for,
            at,
        } => {
            let _ = at;
            (*work_order_id, *asset, *status, *scheduled_for)
        }
        DomainEvent::DowntimeRecorded {
            work_order_id,
            downtime_id,
            at,
        } => {
            let _ = (work_order_id, downtime_id, at);
            return None;
        }
    };
    Some(
        maintenance::WorkOrderRef::try_from_uuid(work_order_id.as_uuid())
            .map_err(|_| IntegrationMappingError::InvalidWorkOrderIdentifier)
            .and_then(|work_order| {
                ReferenceCode::try_new(work_order_code.to_owned())
                    .map_err(|_| IntegrationMappingError::InvalidWorkOrderReference)
                    .map(|work_order_code| {
                        maintenance::Event::WorkOrderChanged(maintenance::WorkOrderFact {
                            work_order,
                            work_order_code,
                            asset,
                            status: map_status(status),
                            scheduled_for,
                        })
                    })
            }),
    )
}

const fn map_status(status: WorkOrderStatus) -> maintenance::WorkOrderStatus {
    match status {
        WorkOrderStatus::Planned => maintenance::WorkOrderStatus::Planned,
        WorkOrderStatus::Scheduled => maintenance::WorkOrderStatus::Scheduled,
        WorkOrderStatus::InProgress => maintenance::WorkOrderStatus::InProgress,
        WorkOrderStatus::Blocked => maintenance::WorkOrderStatus::Blocked,
        WorkOrderStatus::Completed => maintenance::WorkOrderStatus::Completed,
        WorkOrderStatus::Cancelled => maintenance::WorkOrderStatus::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_integration_contracts::{assets::AssetRef, maintenance::Event};
    use uuid::Uuid;

    use super::*;
    use crate::domain::WorkOrderId;

    #[test]
    fn status_change_maps_without_exposing_the_maintenance_aggregate() {
        let event = DomainEvent::StatusChanged {
            work_order_id: WorkOrderId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            asset: AssetRef::try_from_uuid(Uuid::from_u128(2)).unwrap(),
            status: WorkOrderStatus::Blocked,
            scheduled_for: None,
            at: Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
        };
        let mapped = published_events(&[event], "MWO-1").unwrap();
        assert!(matches!(
            mapped.as_slice(),
            [Event::WorkOrderChanged(fact)]
                if fact.status == maintenance::WorkOrderStatus::Blocked
        ));
    }
}
