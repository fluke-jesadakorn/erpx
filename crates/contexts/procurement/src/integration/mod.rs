use erpx_integration_contracts::{CurrencyCode, MoneyAmount, Quantity, ReferenceCode, procurement};

use crate::domain::{DomainEvent, ReleasedPurchaseOrderState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    InvalidContractValue,
}

pub(crate) fn published_events(
    events: &[DomainEvent],
) -> Result<Vec<procurement::Event>, IntegrationMappingError> {
    events.iter().filter_map(map_event).collect()
}

fn map_event(event: &DomainEvent) -> Option<Result<procurement::Event, IntegrationMappingError>> {
    match event {
        DomainEvent::Created => None,
        DomainEvent::Released { state, at } => {
            let _ = at;
            Some(fact(state).map(procurement::Event::PurchaseOrderReleased))
        }
        DomainEvent::Cancelled {
            purchase_order_id,
            at,
        } => {
            let _ = at;
            Some(
                procurement::PurchaseOrderRef::try_from_uuid(purchase_order_id.as_uuid())
                    .map(
                        |purchase_order| procurement::Event::PurchaseOrderCancelled {
                            purchase_order,
                        },
                    )
                    .map_err(|_| IntegrationMappingError::InvalidContractValue),
            )
        }
    }
}

fn fact(
    state: &ReleasedPurchaseOrderState,
) -> Result<procurement::ReleasedPurchaseOrderFact, IntegrationMappingError> {
    let supplier = procurement::SupplierRef::try_from_uuid(state.supplier_id.as_uuid())
        .map_err(|_| IntegrationMappingError::InvalidContractValue)?;
    let purchase_order =
        procurement::PurchaseOrderRef::try_from_uuid(state.purchase_order_id.as_uuid())
            .map_err(|_| IntegrationMappingError::InvalidContractValue)?;
    let order_code = ReferenceCode::try_new(state.order_code.clone())
        .map_err(|_| IntegrationMappingError::InvalidContractValue)?;
    let lines = state
        .lines
        .iter()
        .map(|line| {
            let currency = CurrencyCode::try_new(&line.currency)
                .map_err(|_| IntegrationMappingError::InvalidContractValue)?;
            Ok(procurement::ReleasedPurchaseOrderLineFact {
                line: procurement::PurchaseOrderLineRef::try_from_uuid(line.line_id.as_uuid())
                    .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                item_code: ReferenceCode::try_new(line.item_code.clone())
                    .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                ordered_quantity: Quantity::try_new(line.ordered_quantity, 0)
                    .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                line_value: MoneyAmount::new(
                    line.line_value_minor()
                        .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                    currency,
                ),
                destination_code: ReferenceCode::try_new(line.destination_code.clone())
                    .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
            })
        })
        .collect::<Result<Vec<_>, IntegrationMappingError>>()?;
    Ok(procurement::ReleasedPurchaseOrderFact {
        purchase_order,
        supplier,
        order_code,
        lines,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_integration_contracts::IntegrationEvent;
    use erpx_kernel::TenantScope;
    use uuid::Uuid;

    use super::*;
    use crate::domain::{
        PurchaseOrder, PurchaseOrderId, PurchaseOrderLine, PurchaseOrderLineId, SupplierId,
    };

    #[test]
    fn only_released_and_cancelled_states_cross_the_context_boundary() {
        let mut order = PurchaseOrder::create(
            PurchaseOrderId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            TenantScope::Company {
                holding_id: Uuid::from_u128(2),
                company_id: Uuid::from_u128(3),
            },
            "PO-001".into(),
            SupplierId::try_from_uuid(Uuid::from_u128(4)).unwrap(),
            "THB".into(),
            vec![PurchaseOrderLine {
                line_id: PurchaseOrderLineId::try_from_uuid(Uuid::from_u128(5)).unwrap(),
                item_code: "ITEM-1".into(),
                ordered_quantity: 2,
                unit_price_minor: 150,
                currency: "THB".into(),
                destination_code: "WH-1".into(),
            }],
            Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
        )
        .unwrap();
        assert!(
            published_events(&order.take_uncommitted_events())
                .unwrap()
                .is_empty()
        );
        order
            .release(Utc.with_ymd_and_hms(2026, 8, 5, 10, 0, 0).single().unwrap())
            .unwrap();
        let events = published_events(&order.take_uncommitted_events()).unwrap();
        assert!(matches!(
            events.as_slice(),
            [procurement::Event::PurchaseOrderReleased(fact)]
                if fact.lines.len() == 1 && fact.lines[0].ordered_quantity.units() == 2
        ));
        assert!(events[0].validate().is_ok());
    }
}
