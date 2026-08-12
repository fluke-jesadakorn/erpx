use erpx_integration_contracts::{
    DisplayName, IntegrationEventEnvelope, ReferenceCode, assets, commercial, crm, customer,
    finance, inventory, maintenance, marketing, operations, people_payroll, pos, procurement,
    quality, sales, support,
};

use crate::domain::{DomainEvent, OperationsDomainError, SourceFactMessage};

/// Closed inventory of events that currently have an actual source-context
/// publisher and an Operations source-fact contract. The worker/process
/// manager owns this inbound type; it is intentionally absent from the public
/// command service.
#[derive(Debug, Clone, PartialEq)]
pub enum PublishedSourceEvent {
    Commercial(IntegrationEventEnvelope<commercial::Event>),
    Customer(IntegrationEventEnvelope<customer::Event>),
    Marketing(IntegrationEventEnvelope<marketing::Event>),
    Crm(IntegrationEventEnvelope<crm::Event>),
    Procurement(IntegrationEventEnvelope<procurement::Event>),
    Inventory(IntegrationEventEnvelope<inventory::Event>),
    Assets(IntegrationEventEnvelope<assets::Event>),
    Maintenance(IntegrationEventEnvelope<maintenance::Event>),
    Quality(IntegrationEventEnvelope<quality::Event>),
    Sales(IntegrationEventEnvelope<sales::Event>),
    PeoplePayroll(IntegrationEventEnvelope<people_payroll::Event>),
    Pos(IntegrationEventEnvelope<pos::Event>),
    Finance(IntegrationEventEnvelope<finance::Event>),
    Support(IntegrationEventEnvelope<support::Event>),
}

impl PublishedSourceEvent {
    /// Converts only orchestration-relevant facts. Other valid events from the
    /// same owner are acknowledged as irrelevant rather than coerced into an
    /// unrelated Operations fact.
    pub fn try_into_message(self) -> Result<Option<SourceFactMessage>, OperationsDomainError> {
        match self {
            Self::Commercial(envelope) => match envelope.payload() {
                commercial::Event::SubscriptionChanged(fact) => SourceFactMessage::from_envelope(
                    &envelope,
                    operations::SourceFact::Commercial(fact.clone()),
                )
                .map(Some),
                commercial::Event::EntitlementChanged(_) => Ok(None),
            },
            Self::Customer(envelope) => match envelope.payload() {
                customer::Event::CustomerProfileChanged(fact) => SourceFactMessage::from_envelope(
                    &envelope,
                    operations::SourceFact::Customer(fact.clone()),
                )
                .map(Some),
                customer::Event::CustomerMerged { .. } => Ok(None),
            },
            Self::Marketing(envelope) => match envelope.payload() {
                marketing::Event::EngagementCaptured(fact) => SourceFactMessage::from_envelope(
                    &envelope,
                    operations::SourceFact::Marketing(fact.clone()),
                )
                .map(Some),
                marketing::Event::CampaignBudgetApprovalRequested(fact) => {
                    SourceFactMessage::from_envelope(
                        &envelope,
                        operations::SourceFact::MarketingBudgetApproval(fact.clone()),
                    )
                    .map(Some)
                }
                marketing::Event::LeadQualified(_)
                | marketing::Event::CampaignBudgetApproved(_)
                | marketing::Event::CampaignSpendRequested(_)
                | marketing::Event::AttributionRevisionPublished(_) => Ok(None),
            },
            Self::Crm(envelope) => match envelope.payload() {
                crm::Event::OpportunityChanged(fact) => SourceFactMessage::from_envelope(
                    &envelope,
                    operations::SourceFact::Crm(fact.clone()),
                )
                .map(Some),
                crm::Event::LeadAccepted(_) | crm::Event::OpportunityForecastChanged(_) => Ok(None),
            },
            Self::Procurement(envelope) => match envelope.payload() {
                procurement::Event::PurchaseOrderReleased(fact) => {
                    SourceFactMessage::from_envelope(
                        &envelope,
                        operations::SourceFact::Procurement(fact.clone()),
                    )
                    .map(Some)
                }
                procurement::Event::PurchaseOrderCancelled { .. } => Ok(None),
            },
            Self::Inventory(envelope) => match envelope.payload() {
                inventory::Event::StockMoved(fact) => SourceFactMessage::from_envelope(
                    &envelope,
                    operations::SourceFact::Inventory(fact.clone()),
                )
                .map(Some),
                inventory::Event::InventoryReserved(_)
                | inventory::Event::InventoryReservationReleased { .. } => Ok(None),
            },
            Self::Assets(envelope) => match envelope.payload() {
                assets::Event::AssetConditionChanged(fact) => SourceFactMessage::from_envelope(
                    &envelope,
                    operations::SourceFact::Assets(fact.clone()),
                )
                .map(Some),
                assets::Event::MaintenanceRequested { .. } => Ok(None),
            },
            Self::Maintenance(envelope) => match envelope.payload() {
                maintenance::Event::WorkOrderChanged(fact) => SourceFactMessage::from_envelope(
                    &envelope,
                    operations::SourceFact::Maintenance(fact.clone()),
                )
                .map(Some),
            },
            Self::Quality(envelope) => match envelope.payload() {
                quality::Event::InspectionCompleted(fact) => SourceFactMessage::from_envelope(
                    &envelope,
                    operations::SourceFact::Quality(fact.clone()),
                )
                .map(Some),
                quality::Event::NonconformanceRaised { .. }
                | quality::Event::NonconformanceResolved { .. } => Ok(None),
            },
            Self::Sales(envelope) => match envelope.payload() {
                sales::Event::SalesOrderChanged(fact) => SourceFactMessage::from_envelope(
                    &envelope,
                    operations::SourceFact::Sales(fact.clone()),
                )
                .map(Some),
                sales::Event::RevenueArrangementActivated(_)
                | sales::Event::FulfillmentAccepted(_)
                | sales::Event::BillingEligible(_) => Ok(None),
            },
            Self::PeoplePayroll(envelope) => match envelope.payload() {
                people_payroll::Event::PayrollFinalized(fact) => SourceFactMessage::from_envelope(
                    &envelope,
                    operations::SourceFact::PeoplePayroll(fact.clone()),
                )
                .map(Some),
                people_payroll::Event::EmployeeStatusChanged(_) => Ok(None),
            },
            Self::Pos(envelope) => match envelope.payload() {
                pos::Event::SaleCompleted(fact) => SourceFactMessage::from_envelope(
                    &envelope,
                    operations::SourceFact::PosSale(fact.clone()),
                )
                .map(Some),
                pos::Event::RefundApproved(fact) => SourceFactMessage::from_envelope(
                    &envelope,
                    operations::SourceFact::PosRefund(fact.clone()),
                )
                .map(Some),
                pos::Event::SettlementClosed(_) => Ok(None),
            },
            Self::Finance(envelope) => match envelope.payload() {
                finance::Event::JournalPosted(fact) => SourceFactMessage::from_envelope(
                    &envelope,
                    operations::SourceFact::Finance(fact.clone()),
                )
                .map(Some),
                finance::Event::ReceivableChanged(_)
                | finance::Event::CustomerInvoicePosted(_)
                | finance::Event::RevenueRecognized(_)
                | finance::Event::CashApplied(_)
                | finance::Event::MarketingSpendPosted(_) => Ok(None),
            },
            Self::Support(envelope) => match envelope.payload() {
                support::Event::SupportCaseChanged(fact) => SourceFactMessage::from_envelope(
                    &envelope,
                    operations::SourceFact::Support(fact.clone()),
                )
                .map(Some),
                support::Event::IncidentChanged(fact) => SourceFactMessage::from_envelope(
                    &envelope,
                    operations::SourceFact::SupportIncident(fact.clone()),
                )
                .map(Some),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    InvalidReference,
}

pub(crate) fn published_events(
    events: &[DomainEvent],
) -> Result<Vec<operations::Event>, IntegrationMappingError> {
    events.iter().map(map_event).collect()
}

fn map_event(event: &DomainEvent) -> Result<operations::Event, IntegrationMappingError> {
    match event {
        DomainEvent::ExceptionChanged {
            exception_id,
            reference,
            state,
            source,
            at,
        } => Ok(operations::Event::OperationalStateChanged(
            operations::OperationalSignalFact {
                process: operations::OperationalProcessRef::try_from_uuid(exception_id.as_uuid())
                    .map_err(|_| IntegrationMappingError::InvalidReference)?,
                process_code: ReferenceCode::try_new(reference.clone())
                    .map_err(|_| IntegrationMappingError::InvalidReference)?,
                state: ReferenceCode::try_new(state.as_str())
                    .map_err(|_| IntegrationMappingError::InvalidReference)?,
                observed_at: *at,
                source: source.clone(),
            },
        )),
        DomainEvent::WorkQueuePolicyPublished {
            policy_id,
            name,
            allowed_sources,
            escalation_after_minutes,
            at,
        } => Ok(operations::Event::WorkQueuePolicyPublished(
            operations::WorkQueuePolicyFact {
                policy: operations::WorkQueuePolicyRef::try_from_uuid(policy_id.as_uuid())
                    .map_err(|_| IntegrationMappingError::InvalidReference)?,
                name: DisplayName::try_new(name.clone())
                    .map_err(|_| IntegrationMappingError::InvalidReference)?,
                allowed_sources: allowed_sources.clone(),
                escalation_after_minutes: *escalation_after_minutes,
                published_at: *at,
            },
        )),
        DomainEvent::ApprovalDecided(fact) => Ok(operations::Event::ApprovalDecided(fact.clone())),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_integration_contracts::{
        AggregateKind, CurrencyCode, IntegrationEvent, MoneyAmount, customer,
    };
    use erpx_kernel::{AggregateId, CorrelationId, EventId, TenantScope};
    use uuid::Uuid;

    use super::*;

    fn sales_envelope() -> IntegrationEventEnvelope<sales::Event> {
        let currency = CurrencyCode::try_new("THB").unwrap();
        IntegrationEventEnvelope::try_new(
            EventId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            TenantScope::Company {
                holding_id: Uuid::from_u128(2),
                company_id: Uuid::from_u128(3),
            },
            AggregateKind::try_new("sales_order").unwrap(),
            AggregateId::try_from_uuid(Uuid::from_u128(4)).unwrap(),
            2,
            CorrelationId::try_from_uuid(Uuid::from_u128(5)).unwrap(),
            None,
            Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
            sales::Event::SalesOrderChanged(sales::SalesOrderFact {
                sales_order: sales::SalesOrderRef::try_from_uuid(Uuid::from_u128(4)).unwrap(),
                order_code: ReferenceCode::try_new("SO-4").unwrap(),
                customer: customer::CustomerRef::try_from_uuid(Uuid::from_u128(6)).unwrap(),
                status: sales::SalesOrderStatus::Confirmed,
                total: MoneyAmount::new(1_000, currency),
                lines: vec![sales::SalesOrderLineFact {
                    line: sales::SalesOrderLineRef::try_from_uuid(Uuid::from_u128(7)).unwrap(),
                    item_code: ReferenceCode::try_new("SKU-7").unwrap(),
                    quantity: erpx_integration_contracts::Quantity::try_new(1, 0).unwrap(),
                    line_value: MoneyAmount::new(1_000, currency),
                }],
            }),
        )
        .unwrap()
    }

    #[test]
    fn typed_sales_event_becomes_an_immutable_operations_source_message() {
        let message = PublishedSourceEvent::Sales(sales_envelope())
            .try_into_message()
            .unwrap()
            .unwrap();
        assert_eq!(
            message.source_context(),
            erpx_integration_contracts::BoundedContext::Sales
        );
        assert_eq!(message.event_type(), "sales.order_changed");
        assert!(matches!(message.fact(), operations::SourceFact::Sales(_)));
    }

    #[test]
    fn irrelevant_owner_event_is_not_coerced_into_a_source_fact() {
        let envelope = IntegrationEventEnvelope::try_new(
            EventId::try_from_uuid(Uuid::from_u128(11)).unwrap(),
            TenantScope::Company {
                holding_id: Uuid::from_u128(2),
                company_id: Uuid::from_u128(3),
            },
            AggregateKind::try_new("stock_reservation").unwrap(),
            AggregateId::try_from_uuid(Uuid::from_u128(12)).unwrap(),
            1,
            CorrelationId::try_from_uuid(Uuid::from_u128(13)).unwrap(),
            None,
            Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
            inventory::Event::InventoryReservationReleased {
                reservation: inventory::ReservationRef::try_from_uuid(Uuid::from_u128(12)).unwrap(),
            },
        )
        .unwrap();
        assert!(
            PublishedSourceEvent::Inventory(envelope)
                .try_into_message()
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn operations_owned_event_exposes_state_and_public_source_fact_only() {
        let at = Utc.with_ymd_and_hms(2026, 8, 5, 10, 0, 0).single().unwrap();
        let source = PublishedSourceEvent::Sales(sales_envelope())
            .try_into_message()
            .unwrap()
            .unwrap();
        let event = DomainEvent::ExceptionChanged {
            exception_id: crate::domain::ExceptionId::try_from_uuid(Uuid::from_u128(20)).unwrap(),
            reference: "OPS-20".into(),
            state: crate::domain::ExceptionStatus::Assigned,
            source: source.fact().clone(),
            at,
        };
        let mapped = published_events(&[event]).unwrap();
        assert!(matches!(
            mapped.as_slice(),
            [operations::Event::OperationalStateChanged(fact)] if fact.observed_at == at
        ));
        assert!(mapped[0].validate().is_ok());
    }

    #[test]
    fn pos_emitted_events_map_to_their_exact_operations_fact_variants() {
        let at = Utc.with_ymd_and_hms(2026, 8, 5, 11, 0, 0).single().unwrap();
        let currency = CurrencyCode::try_new("THB").unwrap();
        let scope = TenantScope::Company {
            holding_id: Uuid::from_u128(2),
            company_id: Uuid::from_u128(3),
        };
        let events = [
            pos::Event::SaleCompleted(pos::PosSaleFact {
                transaction: pos::PosTransactionRef::try_from_uuid(Uuid::from_u128(31)).unwrap(),
                register: pos::RegisterRef::try_from_uuid(Uuid::from_u128(32)).unwrap(),
                receipt_code: ReferenceCode::try_new("POS-31").unwrap(),
                customer: None,
                gross_amount: MoneyAmount::new(1_000, currency),
                completed_at: at,
            }),
            pos::Event::RefundApproved(pos::PosRefundFact {
                refund: pos::PosTransactionRef::try_from_uuid(Uuid::from_u128(33)).unwrap(),
                original_sale: pos::PosTransactionRef::try_from_uuid(Uuid::from_u128(31)).unwrap(),
                amount: MoneyAmount::new(100, currency),
                approved_at: at,
            }),
        ];
        let mapped = events
            .into_iter()
            .enumerate()
            .map(|(index, event)| {
                let id = Uuid::from_u128(40 + index as u128);
                let envelope = IntegrationEventEnvelope::try_new(
                    EventId::try_from_uuid(id).unwrap(),
                    scope.clone(),
                    AggregateKind::try_new("pos_transaction").unwrap(),
                    AggregateId::try_from_uuid(Uuid::from_u128(50 + index as u128)).unwrap(),
                    1,
                    CorrelationId::try_from_uuid(Uuid::from_u128(60 + index as u128)).unwrap(),
                    None,
                    at,
                    event,
                )
                .unwrap();
                PublishedSourceEvent::Pos(envelope)
                    .try_into_message()
                    .unwrap()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert!(matches!(
            mapped[0].fact(),
            operations::SourceFact::PosSale(_)
        ));
        assert!(matches!(
            mapped[1].fact(),
            operations::SourceFact::PosRefund(_)
        ));
    }

    #[test]
    fn policy_publish_maps_to_operations_owned_contract() {
        let at = Utc.with_ymd_and_hms(2026, 8, 5, 12, 0, 0).single().unwrap();
        let mapped = published_events(&[DomainEvent::WorkQueuePolicyPublished {
            policy_id: crate::domain::WorkQueuePolicyId::try_from_uuid(Uuid::from_u128(70))
                .unwrap(),
            name: "priority queue".into(),
            allowed_sources: vec![
                erpx_integration_contracts::BoundedContext::Sales,
                erpx_integration_contracts::BoundedContext::Support,
            ],
            escalation_after_minutes: 60,
            at,
        }])
        .unwrap();
        assert!(matches!(
            mapped.as_slice(),
            [operations::Event::WorkQueuePolicyPublished(fact)]
                if fact.escalation_after_minutes == 60 && fact.published_at == at
        ));
        assert!(mapped[0].validate().is_ok());
    }
}
