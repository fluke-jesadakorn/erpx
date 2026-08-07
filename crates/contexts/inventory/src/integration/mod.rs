use erpx_integration_contracts::{Quantity, ReferenceCode, inventory, procurement};

use crate::domain::{DomainEvent, StockItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    InvalidContractValue,
}

pub(crate) fn published_events(
    aggregate: &StockItem,
    events: &[DomainEvent],
) -> Result<Vec<inventory::Event>, IntegrationMappingError> {
    events
        .iter()
        .filter_map(|event| map_event(aggregate, event))
        .collect()
}

fn map_event(
    aggregate: &StockItem,
    event: &DomainEvent,
) -> Option<Result<inventory::Event, IntegrationMappingError>> {
    match event {
        DomainEvent::Created => None,
        DomainEvent::StockMoved {
            movement_id,
            quantity_delta,
            procurement_line,
            at,
        } => {
            let _ = at;
            Some((|| {
                Ok(inventory::Event::StockMoved(inventory::StockMovementFact {
                    movement: inventory::StockMovementRef::try_from_uuid(movement_id.as_uuid())
                        .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                    stock_item: inventory::StockItemRef::try_from_uuid(aggregate.id().as_uuid())
                        .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                    warehouse: inventory::WarehouseRef::try_from_uuid(
                        aggregate.warehouse_id().as_uuid(),
                    )
                    .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                    item_code: ReferenceCode::try_new(aggregate.item_code().to_owned())
                        .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                    quantity_delta: Quantity::try_new(*quantity_delta, 0)
                        .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                    procurement_line: procurement_line
                        .map(|line| {
                            procurement::PurchaseOrderLineRef::try_from_uuid(line.as_uuid())
                                .map_err(|_| IntegrationMappingError::InvalidContractValue)
                        })
                        .transpose()?,
                }))
            })())
        }
        DomainEvent::Reserved { reservation, at } => {
            let _ = at;
            Some((|| {
                Ok(inventory::Event::InventoryReserved(
                    inventory::ReservationFact {
                        reservation: inventory::ReservationRef::try_from_uuid(
                            reservation.reservation_id.as_uuid(),
                        )
                        .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                        stock_item: inventory::StockItemRef::try_from_uuid(
                            aggregate.id().as_uuid(),
                        )
                        .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                        warehouse: inventory::WarehouseRef::try_from_uuid(
                            aggregate.warehouse_id().as_uuid(),
                        )
                        .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                        quantity: Quantity::try_new(reservation.quantity, 0)
                            .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                        demand_reference: ReferenceCode::try_new(
                            reservation.demand_reference.clone(),
                        )
                        .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                    },
                ))
            })())
        }
        DomainEvent::ReservationReleased { reservation_id, at } => {
            let _ = at;
            Some(
                inventory::ReservationRef::try_from_uuid(reservation_id.as_uuid())
                    .map(
                        |reservation| inventory::Event::InventoryReservationReleased {
                            reservation,
                        },
                    )
                    .map_err(|_| IntegrationMappingError::InvalidContractValue),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_kernel::TenantScope;
    use uuid::Uuid;

    use super::*;
    use crate::domain::{ProcurementLineReference, StockItemId, StockMovementId, WarehouseId};

    #[test]
    fn movement_exposes_typed_procurement_line_without_procurement_aggregate() {
        let at = Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap();
        let mut item = StockItem::register(
            StockItemId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            TenantScope::Company {
                holding_id: Uuid::from_u128(2),
                company_id: Uuid::from_u128(3),
            },
            WarehouseId::try_from_uuid(Uuid::from_u128(4)).unwrap(),
            "ITEM-1".into(),
            at,
        )
        .unwrap();
        item.take_uncommitted_events();
        item.receive(
            StockMovementId::try_from_uuid(Uuid::from_u128(5)).unwrap(),
            3,
            Some(ProcurementLineReference::try_from_uuid(Uuid::from_u128(6)).unwrap()),
            at,
        )
        .unwrap();
        let domain_events = item.take_uncommitted_events();
        let events = published_events(&item, &domain_events).unwrap();
        assert!(matches!(
            events.as_slice(),
            [inventory::Event::StockMoved(fact)]
                if fact.quantity_delta.units() == 3 && fact.procurement_line.is_some()
        ));
    }
}
