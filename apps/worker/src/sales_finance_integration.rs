//! Idempotent Sales fact handoff into Finance-owned work.
//!
//! This process manager produces deterministic commands only. It never writes
//! a Sales table or a Finance aggregate directly; a Finance command handler
//! owns validation and persistence of the resulting plan/packet/schedule.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use erpx_integration_contracts::{DimensionSet, IntegrationEventEnvelope, sales};
use erpx_kernel::EventId;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

const FINANCE_HANDOFF_NAMESPACE: Uuid = Uuid::from_u128(0x4552_5058_5341_4c45_5346_494e_0000_0001);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SalesFinanceHandoffKind {
    CreateRevenueSchedule {
        schedule_id: Uuid,
        arrangement_id: Uuid,
        sales_order_id: Uuid,
        customer_id: Uuid,
        dimensions: DimensionSet,
        obligations: Vec<sales::PerformanceObligationFact>,
    },
    PreparePostingPacket {
        packet_id: Uuid,
        billing_event_id: Uuid,
        fulfillment_id: Uuid,
        arrangement_id: Uuid,
        sales_order_id: Uuid,
        customer_id: Uuid,
        order_code: String,
        net_minor: i64,
        tax_minor: i64,
        currency: String,
        eligible_on: NaiveDate,
        dimensions: DimensionSet,
    },
    RecognizeRevenue {
        recognition_id: Uuid,
        fulfillment_id: Uuid,
        arrangement_id: Uuid,
        sales_order_id: Uuid,
        obligation_id: Uuid,
        accepted_minor: i64,
        currency: String,
        accepted_at: DateTime<Utc>,
        dimensions: DimensionSet,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesFinanceHandoff {
    pub source_event_id: EventId,
    pub source_aggregate_version: u64,
    pub source_event_type: String,
    pub correlation_ref: String,
    pub handoff: SalesFinanceHandoffKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SalesFinanceConsumeOutcome {
    Staged(SalesFinanceHandoff),
    Ignored,
    Replay(SalesFinanceHandoff),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesFinanceFailure {
    pub source_event_id: EventId,
    pub failure_code: String,
    pub retryable: bool,
    pub attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SalesFinanceConsumerState {
    checkpoint: u64,
    staged: BTreeMap<EventId, SalesFinanceHandoff>,
    failures: BTreeMap<EventId, SalesFinanceFailure>,
}

impl SalesFinanceConsumerState {
    pub fn consume(
        &mut self,
        global_sequence: u64,
        envelope: &IntegrationEventEnvelope<sales::Event>,
    ) -> Result<SalesFinanceConsumeOutcome, SalesFinanceConsumerError> {
        envelope
            .validate()
            .map_err(|_| SalesFinanceConsumerError::InvalidEnvelope)?;
        if global_sequence == 0 {
            return Err(SalesFinanceConsumerError::InvalidSequence);
        }
        if let Some(staged) = self.staged.get(&envelope.event_id()) {
            return Ok(SalesFinanceConsumeOutcome::Replay(staged.clone()));
        }
        if global_sequence <= self.checkpoint {
            return Err(SalesFinanceConsumerError::CheckpointRegression);
        }
        let handoff = map_handoff(envelope)?;
        self.checkpoint = global_sequence;
        self.failures.remove(&envelope.event_id());
        let Some(handoff) = handoff else {
            return Ok(SalesFinanceConsumeOutcome::Ignored);
        };
        self.staged.insert(envelope.event_id(), handoff.clone());
        Ok(SalesFinanceConsumeOutcome::Staged(handoff))
    }

    pub fn record_failure(
        &mut self,
        source_event_id: EventId,
        failure_code: String,
        retryable: bool,
    ) -> Result<(), SalesFinanceConsumerError> {
        if failure_code.trim().is_empty()
            || failure_code.len() > 128
            || failure_code.chars().any(char::is_control)
        {
            return Err(SalesFinanceConsumerError::InvalidFailure);
        }
        let failure = self
            .failures
            .entry(source_event_id)
            .or_insert(SalesFinanceFailure {
                source_event_id,
                failure_code: failure_code.clone(),
                retryable,
                attempts: 0,
            });
        failure.failure_code = failure_code;
        failure.retryable = retryable;
        failure.attempts = failure
            .attempts
            .checked_add(1)
            .ok_or(SalesFinanceConsumerError::AttemptOverflow)?;
        Ok(())
    }

    pub const fn checkpoint(&self) -> u64 {
        self.checkpoint
    }

    pub fn staged(&self) -> impl Iterator<Item = &SalesFinanceHandoff> {
        self.staged.values()
    }

    pub fn retryable_failures(&self) -> impl Iterator<Item = &SalesFinanceFailure> {
        self.failures.values().filter(|failure| failure.retryable)
    }
}

fn map_handoff(
    envelope: &IntegrationEventEnvelope<sales::Event>,
) -> Result<Option<SalesFinanceHandoff>, SalesFinanceConsumerError> {
    let source_event_id = envelope.event_id();
    let deterministic_id = Uuid::new_v5(
        &FINANCE_HANDOFF_NAMESPACE,
        source_event_id.as_uuid().as_bytes(),
    );
    let handoff = match envelope.payload() {
        sales::Event::QuoteApprovalRequested(_) | sales::Event::SalesOrderChanged(_) => {
            return Ok(None);
        }
        sales::Event::RevenueArrangementActivated(fact) => {
            SalesFinanceHandoffKind::CreateRevenueSchedule {
                schedule_id: deterministic_id,
                arrangement_id: fact.arrangement.as_uuid(),
                sales_order_id: fact.sales_order.as_uuid(),
                customer_id: fact.customer.as_uuid(),
                dimensions: fact.dimensions.clone(),
                obligations: fact.obligations.clone(),
            }
        }
        sales::Event::BillingEligible(fact) => SalesFinanceHandoffKind::PreparePostingPacket {
            packet_id: deterministic_id,
            billing_event_id: fact.billing_event.as_uuid(),
            fulfillment_id: fact.fulfillment.as_uuid(),
            arrangement_id: fact.arrangement.as_uuid(),
            sales_order_id: fact.sales_order.as_uuid(),
            customer_id: fact.customer.as_uuid(),
            order_code: fact.order_code.as_str().into(),
            net_minor: fact.billable_amount.minor_units(),
            tax_minor: fact.tax_amount.minor_units(),
            currency: fact.billable_amount.currency().as_str().into(),
            eligible_on: fact.eligible_on,
            dimensions: fact.dimensions.clone(),
        },
        sales::Event::FulfillmentAccepted(fact) => SalesFinanceHandoffKind::RecognizeRevenue {
            recognition_id: deterministic_id,
            fulfillment_id: fact.fulfillment.as_uuid(),
            arrangement_id: fact.arrangement.as_uuid(),
            sales_order_id: fact.sales_order.as_uuid(),
            obligation_id: fact.obligation.as_uuid(),
            accepted_minor: fact.accepted_amount.minor_units(),
            currency: fact.accepted_amount.currency().as_str().into(),
            accepted_at: fact.accepted_at,
            dimensions: fact.dimensions.clone(),
        },
    };
    Ok(Some(SalesFinanceHandoff {
        source_event_id,
        source_aggregate_version: envelope.aggregate_version(),
        source_event_type: envelope.event_type().into(),
        correlation_ref: envelope.correlation_id().as_uuid().to_string(),
        handoff,
    }))
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SalesFinanceConsumerError {
    #[error("Sales integration envelope is invalid")]
    InvalidEnvelope,
    #[error("Sales integration global sequence is invalid")]
    InvalidSequence,
    #[error("Sales-to-Finance checkpoint moved backwards")]
    CheckpointRegression,
    #[error("Sales-to-Finance failure code is invalid")]
    InvalidFailure,
    #[error("Sales-to-Finance retry attempts overflowed")]
    AttemptOverflow,
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_integration_contracts::{
        AggregateKind, CurrencyCode, DimensionSet, MoneyAmount, ReferenceCode, customer,
        organization,
    };
    use erpx_kernel::{AggregateId, CorrelationId, TenantScope};

    use super::*;

    fn uuid(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn billing_envelope(event_id: u128) -> IntegrationEventEnvelope<sales::Event> {
        let currency = CurrencyCode::try_new("THB").unwrap();
        IntegrationEventEnvelope::try_new(
            EventId::try_from_uuid(uuid(event_id)).unwrap(),
            TenantScope::Company {
                holding_id: uuid(2),
                company_id: uuid(3),
            },
            AggregateKind::try_new("revenue_arrangement").unwrap(),
            AggregateId::try_from_uuid(uuid(4)).unwrap(),
            7,
            CorrelationId::try_from_uuid(uuid(5)).unwrap(),
            None,
            Utc.with_ymd_and_hms(2026, 8, 18, 10, 0, 0).unwrap(),
            sales::Event::BillingEligible(sales::BillingEligibilityFact {
                billing_event: sales::BillingEventRef::try_from_uuid(uuid(10)).unwrap(),
                fulfillment: sales::FulfillmentRef::try_from_uuid(uuid(11)).unwrap(),
                arrangement: sales::RevenueArrangementRef::try_from_uuid(uuid(12)).unwrap(),
                sales_order: sales::SalesOrderRef::try_from_uuid(uuid(13)).unwrap(),
                customer: customer::CustomerRef::try_from_uuid(uuid(14)).unwrap(),
                order_code: ReferenceCode::try_new("SO-001").unwrap(),
                billable_amount: MoneyAmount::new(100_000, currency),
                tax_amount: MoneyAmount::new(7_000, currency),
                eligible_on: chrono::NaiveDate::from_ymd_opt(2026, 8, 18).unwrap(),
                dimensions: DimensionSet::for_legal_entity(
                    organization::CompanyRef::try_from_uuid(uuid(3)).unwrap(),
                ),
            }),
        )
        .unwrap()
    }

    #[test]
    fn duplicate_event_replays_the_same_packet_without_staging_another() {
        let envelope = billing_envelope(1);
        let mut consumer = SalesFinanceConsumerState::default();
        let first = consumer.consume(20, &envelope).unwrap();
        let replay = consumer.consume(20, &envelope).unwrap();
        let (SalesFinanceConsumeOutcome::Staged(first), SalesFinanceConsumeOutcome::Replay(replay)) =
            (first, replay)
        else {
            panic!("expected staged then replay")
        };
        assert_eq!(first, replay);
        assert_eq!(consumer.staged().count(), 1);
        assert_eq!(consumer.checkpoint(), 20);
    }

    #[test]
    fn unknown_old_event_cannot_cross_the_checkpoint() {
        let mut consumer = SalesFinanceConsumerState::default();
        consumer.consume(20, &billing_envelope(1)).unwrap();
        assert_eq!(
            consumer.consume(19, &billing_envelope(2)),
            Err(SalesFinanceConsumerError::CheckpointRegression)
        );
    }

    #[test]
    fn retryable_failures_are_visible_without_changing_the_checkpoint() {
        let mut consumer = SalesFinanceConsumerState::default();
        let event_id = EventId::try_from_uuid(uuid(1)).unwrap();
        consumer
            .record_failure(event_id, "projection_lag".into(), true)
            .unwrap();
        consumer
            .record_failure(event_id, "projection_lag".into(), true)
            .unwrap();
        let failure = consumer.retryable_failures().next().unwrap();
        assert_eq!(failure.attempts, 2);
        assert_eq!(consumer.checkpoint(), 0);
    }
}
