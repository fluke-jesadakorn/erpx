use erpx_integration_contracts::{CurrencyCode, MoneyAmount, Quantity, ReferenceCode, sales};

use crate::domain::{
    BillingEligibilityFactState, DomainEvent, FulfillmentAcceptedFactState,
    RevenueArrangementFactState, SalesOrderFactState, SalesOrderStatus,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    InvalidContractValue,
}

pub(crate) fn published_events(
    events: &[DomainEvent],
) -> Result<Vec<sales::Event>, IntegrationMappingError> {
    events.iter().filter_map(map_event).collect()
}

fn map_event(event: &DomainEvent) -> Option<Result<sales::Event, IntegrationMappingError>> {
    match event {
        DomainEvent::QuoteCreated | DomainEvent::QuoteChanged => None,
        DomainEvent::OrderChanged { state } => {
            Some(fact(state).map(sales::Event::SalesOrderChanged))
        }
        DomainEvent::RevenueArrangementActivated { state } => {
            Some(arrangement_fact(state).map(sales::Event::RevenueArrangementActivated))
        }
        DomainEvent::FulfillmentAccepted { state } => {
            Some(fulfillment_fact(state).map(sales::Event::FulfillmentAccepted))
        }
        DomainEvent::BillingEligible { state } => {
            Some(billing_fact(state).map(sales::Event::BillingEligible))
        }
    }
}

fn arrangement_fact(
    state: &RevenueArrangementFactState,
) -> Result<sales::RevenueArrangementFact, IntegrationMappingError> {
    let currency = CurrencyCode::try_new(&state.currency)
        .map_err(|_| IntegrationMappingError::InvalidContractValue)?;
    let obligations = state
        .obligations
        .iter()
        .map(|value| {
            Ok(sales::PerformanceObligationFact {
                obligation: sales::PerformanceObligationRef::try_from_uuid(
                    value.obligation_id().as_uuid(),
                )
                .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                line: sales::SalesOrderLineRef::try_from_uuid(value.line_id().as_uuid())
                    .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                model: value.model(),
                allocated_amount: MoneyAmount::new(value.allocated_minor(), currency),
                service_starts_on: value.service_starts_on(),
                service_ends_on: value.service_ends_on(),
            })
        })
        .collect::<Result<Vec<_>, IntegrationMappingError>>()?;
    Ok(sales::RevenueArrangementFact {
        arrangement: sales::RevenueArrangementRef::try_from_uuid(state.arrangement_id.as_uuid())
            .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
        sales_order: sales::SalesOrderRef::try_from_uuid(state.sales_order_id.as_uuid())
            .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
        customer: state.customer,
        legal_contract: state.legal_contract,
        order_code: ReferenceCode::try_new(state.order_code.clone())
            .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
        dimensions: state.dimensions.clone(),
        obligations,
    })
}

fn fulfillment_fact(
    state: &FulfillmentAcceptedFactState,
) -> Result<sales::FulfillmentAcceptedFact, IntegrationMappingError> {
    let currency = CurrencyCode::try_new(&state.currency)
        .map_err(|_| IntegrationMappingError::InvalidContractValue)?;
    Ok(sales::FulfillmentAcceptedFact {
        fulfillment: sales::FulfillmentRef::try_from_uuid(state.fulfillment_id.as_uuid())
            .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
        arrangement: sales::RevenueArrangementRef::try_from_uuid(state.arrangement_id.as_uuid())
            .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
        sales_order: sales::SalesOrderRef::try_from_uuid(state.sales_order_id.as_uuid())
            .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
        obligation: sales::PerformanceObligationRef::try_from_uuid(state.obligation_id.as_uuid())
            .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
        kind: state.kind,
        accepted_amount: MoneyAmount::new(state.accepted_minor, currency),
        accepted_at: state.accepted_at,
        dimensions: state.dimensions.clone(),
    })
}

fn billing_fact(
    state: &BillingEligibilityFactState,
) -> Result<sales::BillingEligibilityFact, IntegrationMappingError> {
    let currency = CurrencyCode::try_new(&state.currency)
        .map_err(|_| IntegrationMappingError::InvalidContractValue)?;
    Ok(sales::BillingEligibilityFact {
        billing_event: sales::BillingEventRef::try_from_uuid(state.billing_event_id.as_uuid())
            .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
        fulfillment: sales::FulfillmentRef::try_from_uuid(state.fulfillment_id.as_uuid())
            .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
        arrangement: sales::RevenueArrangementRef::try_from_uuid(state.arrangement_id.as_uuid())
            .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
        sales_order: sales::SalesOrderRef::try_from_uuid(state.sales_order_id.as_uuid())
            .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
        customer: state.customer,
        order_code: ReferenceCode::try_new(state.order_code.clone())
            .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
        billable_amount: MoneyAmount::new(state.billable_minor, currency),
        tax_amount: MoneyAmount::new(state.tax_minor, currency),
        eligible_on: state.eligible_on,
        dimensions: state.dimensions.clone(),
    })
}

fn fact(state: &SalesOrderFactState) -> Result<sales::SalesOrderFact, IntegrationMappingError> {
    let currency = CurrencyCode::try_new(&state.currency)
        .map_err(|_| IntegrationMappingError::InvalidContractValue)?;
    let lines = state
        .lines
        .iter()
        .map(|line| {
            Ok(sales::SalesOrderLineFact {
                line: sales::SalesOrderLineRef::try_from_uuid(line.line_id.as_uuid())
                    .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                item_code: ReferenceCode::try_new(line.item_code.clone())
                    .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                quantity: Quantity::try_new(line.quantity, 0)
                    .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                line_value: MoneyAmount::new(
                    line.line_value_minor()
                        .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
                    currency,
                ),
            })
        })
        .collect::<Result<Vec<_>, IntegrationMappingError>>()?;
    let status = match state.status {
        SalesOrderStatus::Confirmed => sales::SalesOrderStatus::Confirmed,
        SalesOrderStatus::PartiallyFulfilled => sales::SalesOrderStatus::PartiallyFulfilled,
        SalesOrderStatus::Fulfilled => sales::SalesOrderStatus::Fulfilled,
        SalesOrderStatus::Cancelled => sales::SalesOrderStatus::Cancelled,
        SalesOrderStatus::Draft => return Err(IntegrationMappingError::InvalidContractValue),
    };
    Ok(sales::SalesOrderFact {
        sales_order: sales::SalesOrderRef::try_from_uuid(state.sales_order_id.as_uuid())
            .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
        order_code: ReferenceCode::try_new(state.order_code.clone())
            .map_err(|_| IntegrationMappingError::InvalidContractValue)?,
        customer: state.customer,
        status,
        total: MoneyAmount::new(state.total_minor, currency),
        lines,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_integration_contracts::{CurrencyCode, IntegrationEvent, MoneyAmount, crm, customer};
    use erpx_kernel::{
        ActorAttribution, ActorKind, AuthorityScope, CommandId, HumanDecisionProof, SessionId,
    };
    use uuid::Uuid;

    use super::*;
    use crate::domain::{
        Quote, QuoteId, SalesOrder, SalesOrderId, SalesOrderLine, SalesOrderLineId,
    };

    #[test]
    fn quote_is_private_and_confirmed_order_publishes_typed_fact() {
        let at = Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap();
        let scope = AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        let mut quote = Quote::create_from_opportunity(
            QuoteId::try_from_uuid(Uuid::from_u128(3)).unwrap(),
            scope.clone(),
            crm::OpportunityFact {
                opportunity: crm::OpportunityRef::try_from_uuid(Uuid::from_u128(4)).unwrap(),
                customer: customer::CustomerRef::try_from_uuid(Uuid::from_u128(5)).unwrap(),
                stage: crm::OpportunityStage::Won,
                expected_value: MoneyAmount::new(200, CurrencyCode::try_new("THB").unwrap()),
            },
            "QT-3".into(),
            "THB".into(),
            vec![SalesOrderLine {
                line_id: SalesOrderLineId::try_from_uuid(Uuid::from_u128(6)).unwrap(),
                item_code: "ITEM-6".into(),
                description: "Item".into(),
                quantity: 2,
                unit_price_minor: 100,
                tax_minor: 0,
                currency: "THB".into(),
            }],
            at + chrono::Duration::days(1),
            at,
        )
        .unwrap();
        assert!(
            published_events(&quote.take_uncommitted_events())
                .unwrap()
                .is_empty()
        );
        quote.send(at).unwrap();
        let quote_command_id = CommandId::try_from_uuid(Uuid::from_u128(90)).unwrap();
        let quote_decision = HumanDecisionProof::try_new(
            ActorAttribution {
                principal_ref: "sales-reviewer".into(),
                kind: ActorKind::HoldingUser,
                session_id: Some(SessionId::try_from_uuid(Uuid::from_u128(99)).unwrap()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope.clone(),
            quote_command_id.to_string(),
            at,
        )
        .unwrap();
        quote.accept(&quote_decision, quote_command_id, at).unwrap();
        let mut order = SalesOrder::create_from_quote(
            SalesOrderId::try_from_uuid(Uuid::from_u128(7)).unwrap(),
            &quote,
            "SO-7".into(),
            at,
        )
        .unwrap();
        let order_command_id = CommandId::try_from_uuid(Uuid::from_u128(91)).unwrap();
        let order_decision = HumanDecisionProof::try_new(
            quote_decision.actor().clone(),
            scope,
            order_command_id.to_string(),
            at,
        )
        .unwrap();
        order
            .confirm(&order_decision, order_command_id, at)
            .unwrap();
        let events = published_events(&order.take_uncommitted_events()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type(), "sales.order_changed");
        let sales::Event::SalesOrderChanged(fact) = &events[0] else {
            panic!("expected sales order event");
        };
        assert_eq!(fact.sales_order.as_uuid(), Uuid::from_u128(7));
        assert_eq!(fact.total.minor_units(), 200);
        assert_eq!(fact.lines.len(), 1);
    }
}
