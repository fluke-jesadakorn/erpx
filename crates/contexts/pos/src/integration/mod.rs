use erpx_integration_contracts::{CurrencyCode, MoneyAmount, ReferenceCode, customer, pos};

use crate::domain::{CompletedSaleState, DomainEvent, PostedRefundState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntegrationMappingError {
    InvalidContractValue,
}

pub(crate) fn published_events(
    events: &[DomainEvent],
) -> Result<Vec<pos::Event>, IntegrationMappingError> {
    events.iter().filter_map(map_event).collect()
}

fn map_event(event: &DomainEvent) -> Option<Result<pos::Event, IntegrationMappingError>> {
    match event {
        DomainEvent::SaleCreated | DomainEvent::RefundRequested => None,
        DomainEvent::SaleCompleted(state) => Some(
            sale_fact(state)
                .map(pos::Event::SaleCompleted)
                .map_err(|_| IntegrationMappingError::InvalidContractValue),
        ),
        DomainEvent::RefundPosted(state) => Some(
            refund_fact(state)
                .map(pos::Event::RefundApproved)
                .map_err(|_| IntegrationMappingError::InvalidContractValue),
        ),
    }
}

fn sale_fact(state: &CompletedSaleState) -> Result<pos::PosSaleFact, ()> {
    let currency = CurrencyCode::try_new(&state.currency).map_err(|_| ())?;
    Ok(pos::PosSaleFact {
        transaction: pos::PosTransactionRef::try_from_uuid(state.sale_id.as_uuid())
            .map_err(|_| ())?,
        register: pos::RegisterRef::try_from_uuid(state.register_id.as_uuid()).map_err(|_| ())?,
        receipt_code: ReferenceCode::try_new(state.receipt_code.clone()).map_err(|_| ())?,
        customer: state
            .customer_id
            .map(|id| customer::CustomerRef::try_from_uuid(id.as_uuid()))
            .transpose()
            .map_err(|_| ())?,
        gross_amount: MoneyAmount::new(state.gross_amount_minor, currency),
        completed_at: state.completed_at,
    })
}

fn refund_fact(state: &PostedRefundState) -> Result<pos::PosRefundFact, ()> {
    let currency = CurrencyCode::try_new(&state.currency).map_err(|_| ())?;
    Ok(pos::PosRefundFact {
        refund: pos::PosTransactionRef::try_from_uuid(state.refund_id.as_uuid()).map_err(|_| ())?,
        original_sale: pos::PosTransactionRef::try_from_uuid(state.sale_id.as_uuid())
            .map_err(|_| ())?,
        amount: MoneyAmount::new(state.amount_minor, currency),
        // Compatibility: the retained v1 contract field is named
        // `approved_at`; publication is delayed until posting, so it carries
        // the effective posted time.
        approved_at: state.posted_at,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_integration_contracts::IntegrationEvent;
    use uuid::Uuid;

    use super::*;
    use crate::domain::{CustomerId, RefundId, RegisterId, SaleId};

    fn at() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
    }

    #[test]
    fn only_finance_relevant_completion_and_approval_facts_are_published() {
        let events = published_events(&[
            DomainEvent::SaleCreated,
            DomainEvent::SaleCompleted(CompletedSaleState {
                sale_id: SaleId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
                register_id: RegisterId::try_from_uuid(Uuid::from_u128(2)).unwrap(),
                receipt_code: "R-1".into(),
                customer_id: Some(CustomerId::try_from_uuid(Uuid::from_u128(3)).unwrap()),
                currency: "THB".into(),
                gross_amount_minor: 214,
                completed_at: at(),
            }),
            DomainEvent::RefundRequested,
            DomainEvent::RefundPosted(PostedRefundState {
                refund_id: RefundId::try_from_uuid(Uuid::from_u128(4)).unwrap(),
                sale_id: SaleId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
                currency: "THB".into(),
                amount_minor: 107,
                posted_at: at(),
            }),
        ])
        .unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type(), "pos.sale_completed");
        assert_eq!(events[1].event_type(), "pos.refund_approved");
        assert!(events.iter().all(|event| event.validate().is_ok()));
    }
}
