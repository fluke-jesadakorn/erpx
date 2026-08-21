//! Deterministic Procurement-to-Payment process-manager decisions.
//!
//! The process manager consumes only validated owner facts. Its outputs are
//! ordinary owner commands: Inventory revalidates every receipt movement and
//! Finance creates a payment proposal against a posted payable. No handoff is
//! an approval, acceptance, posting, or release decision.

use std::collections::BTreeMap;

use erpx_integration_contracts::{IntegrationEventEnvelope, finance, procurement};
use erpx_kernel::{AuthorityScope, EventId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

const RECEIPT_MOVEMENT_NAMESPACE: Uuid = Uuid::from_u128(0x4552_5058_5052_4f43_5245_4354_0000_0001);
const PAYMENT_PROPOSAL_NAMESPACE: Uuid = Uuid::from_u128(0x4552_5058_5052_4f43_5041_594d_0000_0001);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ProcureToPayHandoffKind {
    ReceiveInventory {
        movement_id: Uuid,
        stock_item_id: Uuid,
        purchase_order_line_id: Uuid,
        quantity: i64,
    },
    ProposePayment {
        payment_id: Uuid,
        payable_id: Uuid,
        amount_minor: i64,
        currency: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcureToPayHandoff {
    pub source_event_id: EventId,
    pub source_aggregate_version: u64,
    pub source_event_type: String,
    pub active_scope: AuthorityScope,
    pub correlation_id: Uuid,
    pub handoff: ProcureToPayHandoffKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcureToPayConsumeOutcome {
    Staged(Vec<ProcureToPayHandoff>),
    Ignored,
    Replay(Vec<ProcureToPayHandoff>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "owner", content = "envelope", rename_all = "snake_case")]
pub enum ProcureToPaySourceEvent {
    Procurement(IntegrationEventEnvelope<procurement::Event>),
    Finance(IntegrationEventEnvelope<finance::Event>),
}

impl ProcureToPaySourceEvent {
    fn event_id(&self) -> EventId {
        match self {
            Self::Procurement(value) => value.event_id(),
            Self::Finance(value) => value.event_id(),
        }
    }

    fn validate(&self) -> Result<(), ProcureToPayConsumerError> {
        match self {
            Self::Procurement(value) => value.validate(),
            Self::Finance(value) => value.validate(),
        }
        .map_err(|_| ProcureToPayConsumerError::InvalidEnvelope)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProcureToPayConsumerState {
    checkpoint: u64,
    processed: BTreeMap<EventId, Vec<ProcureToPayHandoff>>,
}

impl ProcureToPayConsumerState {
    pub fn consume(
        &mut self,
        global_sequence: u64,
        source: &ProcureToPaySourceEvent,
    ) -> Result<ProcureToPayConsumeOutcome, ProcureToPayConsumerError> {
        source.validate()?;
        if global_sequence == 0 {
            return Err(ProcureToPayConsumerError::InvalidSequence);
        }
        let event_id = source.event_id();
        if let Some(stored) = self.processed.get(&event_id) {
            return Ok(ProcureToPayConsumeOutcome::Replay(stored.clone()));
        }
        if global_sequence <= self.checkpoint {
            return Err(ProcureToPayConsumerError::CheckpointRegression);
        }
        let handoffs = map_handoffs(source)?;
        self.checkpoint = global_sequence;
        self.processed.insert(event_id, handoffs.clone());
        Ok(if handoffs.is_empty() {
            ProcureToPayConsumeOutcome::Ignored
        } else {
            ProcureToPayConsumeOutcome::Staged(handoffs)
        })
    }

    pub const fn checkpoint(&self) -> u64 {
        self.checkpoint
    }
}

fn map_handoffs(
    source: &ProcureToPaySourceEvent,
) -> Result<Vec<ProcureToPayHandoff>, ProcureToPayConsumerError> {
    match source {
        ProcureToPaySourceEvent::Procurement(envelope) => {
            let procurement::Event::GoodsReceiptAccepted(fact) = envelope.payload() else {
                return Ok(Vec::new());
            };
            fact.lines
                .iter()
                .map(|line| {
                    let mut seed = Vec::with_capacity(32);
                    seed.extend_from_slice(envelope.event_id().as_uuid().as_bytes());
                    seed.extend_from_slice(line.line.as_uuid().as_bytes());
                    let quantity = line.accepted_quantity.units();
                    if quantity <= 0 {
                        return Err(ProcureToPayConsumerError::InvalidOwnerFact);
                    }
                    Ok(ProcureToPayHandoff {
                        source_event_id: envelope.event_id(),
                        source_aggregate_version: envelope.aggregate_version(),
                        source_event_type: envelope.event_type().into(),
                        active_scope: envelope.active_scope().clone(),
                        correlation_id: envelope.correlation_id().as_uuid(),
                        handoff: ProcureToPayHandoffKind::ReceiveInventory {
                            movement_id: Uuid::new_v5(&RECEIPT_MOVEMENT_NAMESPACE, &seed),
                            stock_item_id: line.stock_item.as_uuid(),
                            purchase_order_line_id: line.purchase_order_line.as_uuid(),
                            quantity,
                        },
                    })
                })
                .collect()
        }
        ProcureToPaySourceEvent::Finance(envelope) => {
            let finance::Event::VendorPayablePosted(fact) = envelope.payload() else {
                return Ok(Vec::new());
            };
            if fact.payable_amount.minor_units() <= 0 {
                return Err(ProcureToPayConsumerError::InvalidOwnerFact);
            }
            Ok(vec![ProcureToPayHandoff {
                source_event_id: envelope.event_id(),
                source_aggregate_version: envelope.aggregate_version(),
                source_event_type: envelope.event_type().into(),
                active_scope: envelope.active_scope().clone(),
                correlation_id: envelope.correlation_id().as_uuid(),
                handoff: ProcureToPayHandoffKind::ProposePayment {
                    payment_id: Uuid::new_v5(
                        &PAYMENT_PROPOSAL_NAMESPACE,
                        envelope.event_id().as_uuid().as_bytes(),
                    ),
                    payable_id: fact.payable.as_uuid(),
                    amount_minor: fact.payable_amount.minor_units(),
                    currency: fact.payable_amount.currency().as_str().into(),
                },
            }])
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ProcureToPayConsumerError {
    #[error("Procure-to-Pay integration envelope is invalid")]
    InvalidEnvelope,
    #[error("Procure-to-Pay integration sequence is invalid")]
    InvalidSequence,
    #[error("Procure-to-Pay checkpoint moved backwards")]
    CheckpointRegression,
    #[error("Procure-to-Pay owner fact is invalid")]
    InvalidOwnerFact,
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_integration_contracts::{
        AggregateKind, CurrencyCode, MoneyAmount, Quantity, RecordTypeId, RecordVersionRef,
        StableRecordRef, finance, inventory, procurement,
    };
    use erpx_kernel::{AggregateId, AuthorityScope, CorrelationId, RecordId};

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn record(record_type: &str, id_value: u128, version: u64) -> RecordVersionRef {
        let record_type = RecordTypeId::try_new(record_type).unwrap();
        RecordVersionRef::try_new(
            StableRecordRef::try_new(
                RecordId::try_from_uuid(id(id_value)).unwrap(),
                record_type.descriptor().owner,
                record_type,
            )
            .unwrap(),
            version,
        )
        .unwrap()
    }

    fn receipt_event(event_id: u128) -> ProcureToPaySourceEvent {
        let payload =
            procurement::Event::GoodsReceiptAccepted(procurement::AcceptedGoodsReceiptFact {
                receipt: record("procurement.receipt", 10, 3),
                purchase_order: record("procurement.purchase-order", 11, 5),
                lines: vec![procurement::AcceptedGoodsReceiptLineFact {
                    line: procurement::GoodsReceiptLineRef::try_from_uuid(id(12)).unwrap(),
                    purchase_order_line: procurement::PurchaseOrderLineRef::try_from_uuid(id(13))
                        .unwrap(),
                    stock_item: inventory::StockItemRef::try_from_uuid(id(14)).unwrap(),
                    accepted_quantity: Quantity::try_new(7, 0).unwrap(),
                }],
                accepted_at: Utc.with_ymd_and_hms(2026, 8, 21, 9, 0, 0).unwrap(),
            });
        ProcureToPaySourceEvent::Procurement(
            IntegrationEventEnvelope::try_new(
                EventId::try_from_uuid(id(event_id)).unwrap(),
                AuthorityScope::Company {
                    holding_id: id(2),
                    company_id: id(3),
                },
                AggregateKind::try_new("goods_receipt").unwrap(),
                AggregateId::try_from_uuid(id(10)).unwrap(),
                3,
                CorrelationId::try_from_uuid(id(15)).unwrap(),
                None,
                Utc.with_ymd_and_hms(2026, 8, 21, 9, 0, 0).unwrap(),
                payload,
            )
            .unwrap(),
        )
    }

    fn payable_event(event_id: u128) -> ProcureToPaySourceEvent {
        let currency = CurrencyCode::try_new("THB").unwrap();
        let payload = finance::Event::VendorPayablePosted(finance::VendorPayablePostedFact {
            vendor_invoice: finance::VendorInvoiceRef::try_from_uuid(id(20)).unwrap(),
            payable: finance::PayableRef::try_from_uuid(id(21)).unwrap(),
            supplier: procurement::SupplierRef::try_from_uuid(id(22)).unwrap(),
            purchase_order: record("procurement.purchase-order", 23, 4),
            goods_receipt: record("procurement.receipt", 24, 2),
            payable_amount: MoneyAmount::new(12_300, currency),
            posted_at: Utc.with_ymd_and_hms(2026, 8, 21, 10, 0, 0).unwrap(),
        });
        ProcureToPaySourceEvent::Finance(
            IntegrationEventEnvelope::try_new(
                EventId::try_from_uuid(id(event_id)).unwrap(),
                AuthorityScope::Company {
                    holding_id: id(2),
                    company_id: id(3),
                },
                AggregateKind::try_new("vendor_invoice").unwrap(),
                AggregateId::try_from_uuid(id(20)).unwrap(),
                2,
                CorrelationId::try_from_uuid(id(25)).unwrap(),
                None,
                Utc.with_ymd_and_hms(2026, 8, 21, 10, 0, 0).unwrap(),
                payload,
            )
            .unwrap(),
        )
    }

    #[test]
    fn accepted_receipt_stages_one_deterministic_inventory_command_per_line() {
        let source = receipt_event(1);
        let mut state = ProcureToPayConsumerState::default();
        let ProcureToPayConsumeOutcome::Staged(first) = state.consume(10, &source).unwrap() else {
            panic!("receipt must stage inventory")
        };
        let ProcureToPayConsumeOutcome::Replay(replay) = state.consume(10, &source).unwrap() else {
            panic!("receipt replay must be stable")
        };
        assert_eq!(first, replay);
        assert!(matches!(
            &first[0].handoff,
            ProcureToPayHandoffKind::ReceiveInventory { quantity: 7, .. }
        ));
    }

    #[test]
    fn posted_payable_proposes_but_does_not_release_a_payment() {
        let source = payable_event(2);
        let mut state = ProcureToPayConsumerState::default();
        let ProcureToPayConsumeOutcome::Staged(values) = state.consume(11, &source).unwrap() else {
            panic!("payable must stage payment proposal")
        };
        assert!(matches!(
            &values[0].handoff,
            ProcureToPayHandoffKind::ProposePayment {
                amount_minor: 12_300,
                currency,
                ..
            } if currency == "THB"
        ));
    }
}
