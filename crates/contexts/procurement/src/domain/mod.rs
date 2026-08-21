mod records;
pub use records::{
    ProcurementRecord, ProcurementRecordError, ProcurementRecordKind, ProcurementRecordStatus,
};

use chrono::{DateTime, Utc};
use erpx_kernel::{AuthorityScope, CommandId, HumanDecisionProof};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_LINES: usize = 10_000;
pub const MAX_CODE_BYTES: usize = 128;
pub const MAX_TEXT_BYTES: usize = 255;

macro_rules! domain_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, ProcurementDomainError> {
                if value.is_nil() {
                    return Err(ProcurementDomainError::NilIdentifier { field: $field });
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = ProcurementDomainError;

            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                Self::try_from_uuid(value)
            }
        }
    };
}

domain_id!(PurchaseOrderId, "purchase_order_id");
domain_id!(PurchaseOrderLineId, "purchase_order_line_id");
domain_id!(SupplierId, "supplier_id");

pub(crate) fn purchase_order_release_decision_ref(
    command_id: CommandId,
    purchase_order_id: PurchaseOrderId,
) -> String {
    format!(
        "procurement.purchase_order.release:{}:{}",
        purchase_order_id.as_uuid(),
        command_id
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProcurementDomainError {
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("Procurement records require an explicit company or branch scope")]
    InvalidScope,
    #[error("{field} is required or exceeds its safe length")]
    InvalidText { field: &'static str },
    #[error("purchase order requires between one and 10,000 lines")]
    InvalidLineCount,
    #[error("purchase order line identifiers must be unique")]
    DuplicateLine,
    #[error("purchase order quantities and unit prices must be positive")]
    InvalidAmount,
    #[error("all purchase order lines must use the order currency")]
    CurrencyMismatch,
    #[error("currency must be a three-letter uppercase ASCII code")]
    InvalidCurrency,
    #[error("purchase order arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("purchase order lifecycle transition is not allowed")]
    InvalidTransition,
    #[error("releasing a purchase order requires an authenticated human decision")]
    HumanDecisionRequired,
    #[error("purchase order timestamp cannot move backwards")]
    InvalidTime,
    #[error("aggregate version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseOrderLine {
    pub line_id: PurchaseOrderLineId,
    pub item_code: String,
    pub ordered_quantity: i64,
    pub unit_price_minor: i64,
    pub currency: String,
    pub destination_code: String,
}

impl PurchaseOrderLine {
    pub fn validate(&self) -> Result<(), ProcurementDomainError> {
        validate_code(&self.item_code, "item_code")?;
        validate_code(&self.destination_code, "destination_code")?;
        validate_currency(&self.currency)?;
        if self.ordered_quantity <= 0 || self.unit_price_minor <= 0 {
            return Err(ProcurementDomainError::InvalidAmount);
        }
        self.line_value_minor()?;
        Ok(())
    }

    pub fn line_value_minor(&self) -> Result<i64, ProcurementDomainError> {
        self.ordered_quantity
            .checked_mul(self.unit_price_minor)
            .ok_or(ProcurementDomainError::ArithmeticOverflow)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseOrderStatus {
    Draft,
    Released,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReleasedPurchaseOrderState {
    pub purchase_order_id: PurchaseOrderId,
    pub supplier_id: SupplierId,
    pub order_code: String,
    pub lines: Vec<PurchaseOrderLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    Created,
    Released {
        state: ReleasedPurchaseOrderState,
        at: DateTime<Utc>,
    },
    Cancelled {
        purchase_order_id: PurchaseOrderId,
        at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseOrder {
    id: PurchaseOrderId,
    version: u64,
    scope: AuthorityScope,
    order_code: String,
    supplier_id: SupplierId,
    currency: String,
    lines: Vec<PurchaseOrderLine>,
    status: PurchaseOrderStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurchaseOrderState {
    pub id: PurchaseOrderId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub order_code: String,
    pub supplier_id: SupplierId,
    pub currency: String,
    pub lines: Vec<PurchaseOrderLine>,
    pub status: PurchaseOrderStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PurchaseOrder {
    pub fn try_rehydrate(state: PurchaseOrderState) -> Result<Self, ProcurementDomainError> {
        let value = Self {
            id: state.id,
            version: state.version,
            scope: state.scope,
            order_code: state.order_code,
            supplier_id: state.supplier_id,
            currency: state.currency,
            lines: state.lines,
            status: state.status,
            created_at: state.created_at,
            updated_at: state.updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: PurchaseOrderId,
        scope: AuthorityScope,
        order_code: String,
        supplier_id: SupplierId,
        currency: String,
        lines: Vec<PurchaseOrderLine>,
        at: DateTime<Utc>,
    ) -> Result<Self, ProcurementDomainError> {
        validate_record_scope(&scope)?;
        validate_code(&order_code, "order_code")?;
        validate_currency(&currency)?;
        validate_lines(&lines, &currency)?;
        let value = Self {
            id,
            version: 1,
            scope,
            order_code,
            supplier_id,
            currency,
            lines,
            status: PurchaseOrderStatus::Draft,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::Created],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ProcurementDomainError> {
        validate_record_scope(&self.scope)?;
        validate_code(&self.order_code, "order_code")?;
        validate_currency(&self.currency)?;
        validate_lines(&self.lines, &self.currency)?;
        self.total_minor()?;
        if self.version == 0 {
            return Err(ProcurementDomainError::VersionOverflow);
        }
        if self.updated_at < self.created_at {
            return Err(ProcurementDomainError::InvalidTime);
        }
        Ok(())
    }

    pub fn release(
        &mut self,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        at: DateTime<Utc>,
    ) -> Result<(), ProcurementDomainError> {
        if decision.decision_ref() != purchase_order_release_decision_ref(command_id, self.id)
            || decision.decided_at() != at
            || !decision.authority_scope().contains(&self.scope)
        {
            return Err(ProcurementDomainError::HumanDecisionRequired);
        }
        if self.status != PurchaseOrderStatus::Draft {
            return Err(ProcurementDomainError::InvalidTransition);
        }
        self.advance(at)?;
        self.status = PurchaseOrderStatus::Released;
        self.uncommitted_events.push(DomainEvent::Released {
            state: ReleasedPurchaseOrderState {
                purchase_order_id: self.id,
                supplier_id: self.supplier_id,
                order_code: self.order_code.clone(),
                lines: self.lines.clone(),
            },
            at,
        });
        Ok(())
    }

    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), ProcurementDomainError> {
        if self.status == PurchaseOrderStatus::Cancelled {
            return Err(ProcurementDomainError::InvalidTransition);
        }
        self.advance(at)?;
        self.status = PurchaseOrderStatus::Cancelled;
        self.uncommitted_events.push(DomainEvent::Cancelled {
            purchase_order_id: self.id,
            at,
        });
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), ProcurementDomainError> {
        if at < self.updated_at {
            return Err(ProcurementDomainError::InvalidTime);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(ProcurementDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> PurchaseOrderId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn order_code(&self) -> &str {
        &self.order_code
    }
    pub const fn supplier_id(&self) -> SupplierId {
        self.supplier_id
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub fn lines(&self) -> &[PurchaseOrderLine] {
        &self.lines
    }
    pub fn total_minor(&self) -> Result<i64, ProcurementDomainError> {
        self.lines.iter().try_fold(0_i64, |total, line| {
            total
                .checked_add(line.line_value_minor()?)
                .ok_or(ProcurementDomainError::ArithmeticOverflow)
        })
    }
    pub const fn status(&self) -> PurchaseOrderStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

fn validate_record_scope(scope: &AuthorityScope) -> Result<(), ProcurementDomainError> {
    scope
        .validate()
        .map_err(|_| ProcurementDomainError::InvalidScope)?;
    if matches!(
        scope,
        AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(ProcurementDomainError::InvalidScope)
    }
}

fn validate_lines(
    lines: &[PurchaseOrderLine],
    currency: &str,
) -> Result<(), ProcurementDomainError> {
    if lines.is_empty() || lines.len() > MAX_LINES {
        return Err(ProcurementDomainError::InvalidLineCount);
    }
    let mut ids = std::collections::BTreeSet::new();
    for line in lines {
        line.validate()?;
        if !ids.insert(line.line_id) {
            return Err(ProcurementDomainError::DuplicateLine);
        }
        if line.currency != currency {
            return Err(ProcurementDomainError::CurrencyMismatch);
        }
    }
    Ok(())
}

fn validate_code(value: &str, field: &'static str) -> Result<(), ProcurementDomainError> {
    if value.trim().is_empty()
        || value.len() > MAX_CODE_BYTES
        || value.chars().any(char::is_control)
    {
        Err(ProcurementDomainError::InvalidText { field })
    } else {
        Ok(())
    }
}

fn validate_currency(value: &str) -> Result<(), ProcurementDomainError> {
    if value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase()) {
        Ok(())
    } else {
        Err(ProcurementDomainError::InvalidCurrency)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_kernel::{ActorAttribution, ActorKind, SessionId};

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: id(1),
            company_id: id(2),
        }
    }

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 5, hour, 0, 0)
            .single()
            .unwrap()
    }

    fn command_id(value: u128) -> CommandId {
        CommandId::try_from_uuid(id(value)).unwrap()
    }

    fn human_decision(
        command_id: CommandId,
        purchase_order_id: PurchaseOrderId,
        decided_at: DateTime<Utc>,
    ) -> HumanDecisionProof {
        HumanDecisionProof::try_new(
            ActorAttribution {
                principal_ref: "procurement-controller".into(),
                kind: ActorKind::HoldingUser,
                session_id: Some(SessionId::try_from_uuid(id(3)).unwrap()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope(),
            purchase_order_release_decision_ref(command_id, purchase_order_id),
            decided_at,
        )
        .unwrap()
    }

    fn line(line_id: u128) -> PurchaseOrderLine {
        PurchaseOrderLine {
            line_id: PurchaseOrderLineId::try_from_uuid(id(line_id)).unwrap(),
            item_code: "ITEM-1".into(),
            ordered_quantity: 5,
            unit_price_minor: 120,
            currency: "THB".into(),
            destination_code: "WH-BKK".into(),
        }
    }

    fn order_with_id(value: u128) -> PurchaseOrder {
        PurchaseOrder::create(
            PurchaseOrderId::try_from_uuid(id(value)).unwrap(),
            scope(),
            format!("PO-{value}"),
            SupplierId::try_from_uuid(id(11)).unwrap(),
            "THB".into(),
            vec![line(12)],
            at(9),
        )
        .unwrap()
    }

    fn order() -> PurchaseOrder {
        order_with_id(10)
    }

    #[test]
    fn order_requires_exact_company_or_branch_scope() {
        assert_eq!(
            PurchaseOrder::create(
                PurchaseOrderId::try_from_uuid(id(10)).unwrap(),
                AuthorityScope::Holding { holding_id: id(1) },
                "PO-1".into(),
                SupplierId::try_from_uuid(id(11)).unwrap(),
                "THB".into(),
                vec![line(12)],
                at(9),
            ),
            Err(ProcurementDomainError::InvalidScope)
        );
    }

    #[test]
    fn duplicate_lines_and_currency_mismatch_are_rejected() {
        let duplicate = vec![line(12), line(12)];
        assert!(matches!(
            PurchaseOrder::create(
                PurchaseOrderId::try_from_uuid(id(10)).unwrap(),
                scope(),
                "PO-1".into(),
                SupplierId::try_from_uuid(id(11)).unwrap(),
                "THB".into(),
                duplicate,
                at(9),
            ),
            Err(ProcurementDomainError::DuplicateLine)
        ));
        let mut mismatched = line(13);
        mismatched.currency = "USD".into();
        assert!(matches!(
            PurchaseOrder::create(
                PurchaseOrderId::try_from_uuid(id(10)).unwrap(),
                scope(),
                "PO-1".into(),
                SupplierId::try_from_uuid(id(11)).unwrap(),
                "THB".into(),
                vec![mismatched],
                at(9),
            ),
            Err(ProcurementDomainError::CurrencyMismatch)
        ));
    }

    #[test]
    fn release_is_one_way_and_cancel_is_terminal() {
        let mut order = order();
        order.take_uncommitted_events();
        let first_command = command_id(90);
        order
            .release(
                &human_decision(first_command, order.id(), at(10)),
                first_command,
                at(10),
            )
            .unwrap();
        assert_eq!(order.status(), PurchaseOrderStatus::Released);
        assert_eq!(order.version(), 2);
        let second_command = command_id(91);
        assert_eq!(
            order.release(
                &human_decision(second_command, order.id(), at(11)),
                second_command,
                at(11),
            ),
            Err(ProcurementDomainError::InvalidTransition)
        );
        order.cancel(at(11)).unwrap();
        assert_eq!(
            order.cancel(at(12)),
            Err(ProcurementDomainError::InvalidTransition)
        );
    }

    #[test]
    fn timestamp_cannot_move_backwards() {
        let mut order = order();
        let command_id = command_id(92);
        assert_eq!(
            order.release(
                &human_decision(command_id, order.id(), at(8)),
                command_id,
                at(8),
            ),
            Err(ProcurementDomainError::InvalidTime)
        );
        assert_eq!(order.version(), 1);
    }

    #[test]
    fn release_decision_cannot_be_reused_for_another_command_or_purchase_order() {
        let approved_command = command_id(93);
        let approved_order_id = PurchaseOrderId::try_from_uuid(id(10)).unwrap();
        let decision = human_decision(approved_command, approved_order_id, at(10));

        let mut wrong_command = order();
        assert_eq!(
            wrong_command.release(&decision, command_id(94), at(10)),
            Err(ProcurementDomainError::HumanDecisionRequired)
        );

        let mut wrong_order = order_with_id(13);
        assert_eq!(
            wrong_order.release(&decision, approved_command, at(10)),
            Err(ProcurementDomainError::HumanDecisionRequired)
        );
    }

    #[test]
    fn aggregate_total_overflow_is_rejected_before_release() {
        let mut first = line(12);
        first.ordered_quantity = 1;
        first.unit_price_minor = i64::MAX;
        let mut second = line(13);
        second.ordered_quantity = 1;
        second.unit_price_minor = 1;
        assert!(matches!(
            PurchaseOrder::create(
                PurchaseOrderId::try_from_uuid(id(10)).unwrap(),
                scope(),
                "PO-OVERFLOW".into(),
                SupplierId::try_from_uuid(id(11)).unwrap(),
                "THB".into(),
                vec![first, second],
                at(9),
            ),
            Err(ProcurementDomainError::ArithmeticOverflow)
        ));
    }
}
