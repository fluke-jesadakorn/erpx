use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use erpx_integration_contracts::{RecordVersionRef, catalog::ContextId};
use erpx_kernel::{AuthorityScope, CommandId, HumanDecisionProof};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    MAX_LINES, ProcurementDomainError, PurchaseOrderLineId, validate_code, validate_currency,
    validate_exact_source, validate_record_scope,
};

macro_rules! workflow_id {
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

workflow_id!(PurchaseRequisitionId, "purchase_requisition_id");
workflow_id!(PurchaseRequisitionLineId, "purchase_requisition_line_id");
workflow_id!(GoodsReceiptId, "goods_receipt_id");
workflow_id!(GoodsReceiptLineId, "goods_receipt_line_id");
workflow_id!(InventoryStockItemId, "inventory_stock_item_id");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseRequisitionLine {
    pub line_id: PurchaseRequisitionLineId,
    pub item_code: String,
    pub requested_quantity: i64,
    pub estimated_unit_price_minor: i64,
    pub currency: String,
    pub destination_code: String,
}

impl PurchaseRequisitionLine {
    pub fn validate(&self) -> Result<(), ProcurementDomainError> {
        validate_code(&self.item_code, "item_code")?;
        validate_code(&self.destination_code, "destination_code")?;
        validate_currency(&self.currency)?;
        if self.requested_quantity <= 0 || self.estimated_unit_price_minor <= 0 {
            return Err(ProcurementDomainError::InvalidAmount);
        }
        self.requested_quantity
            .checked_mul(self.estimated_unit_price_minor)
            .ok_or(ProcurementDomainError::ArithmeticOverflow)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseRequisitionStatus {
    Draft,
    Submitted,
    Approved,
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedRequisitionState {
    pub requisition_id: PurchaseRequisitionId,
    pub approved_version: u64,
    pub scope: AuthorityScope,
    pub requisition_code: String,
    pub requesting_department_code: String,
    pub budget_code: Option<String>,
    pub lines: Vec<PurchaseRequisitionLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PurchaseRequisitionDomainEvent {
    Created,
    Submitted,
    Approved {
        state: ApprovedRequisitionState,
        at: DateTime<Utc>,
    },
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseRequisition {
    id: PurchaseRequisitionId,
    version: u64,
    scope: AuthorityScope,
    requisition_code: String,
    requesting_department_code: String,
    budget_code: Option<String>,
    currency: String,
    lines: Vec<PurchaseRequisitionLine>,
    status: PurchaseRequisitionStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<PurchaseRequisitionDomainEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurchaseRequisitionState {
    pub id: PurchaseRequisitionId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub requisition_code: String,
    pub requesting_department_code: String,
    pub budget_code: Option<String>,
    pub currency: String,
    pub lines: Vec<PurchaseRequisitionLine>,
    pub status: PurchaseRequisitionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PurchaseRequisition {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: PurchaseRequisitionId,
        scope: AuthorityScope,
        requisition_code: String,
        requesting_department_code: String,
        budget_code: String,
        currency: String,
        lines: Vec<PurchaseRequisitionLine>,
        at: DateTime<Utc>,
    ) -> Result<Self, ProcurementDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            requisition_code,
            requesting_department_code,
            budget_code: Some(budget_code),
            currency,
            lines,
            status: PurchaseRequisitionStatus::Draft,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![PurchaseRequisitionDomainEvent::Created],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn try_rehydrate(state: PurchaseRequisitionState) -> Result<Self, ProcurementDomainError> {
        let value = Self {
            id: state.id,
            version: state.version,
            scope: state.scope,
            requisition_code: state.requisition_code,
            requesting_department_code: state.requesting_department_code,
            budget_code: state.budget_code,
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

    pub fn validate(&self) -> Result<(), ProcurementDomainError> {
        validate_record_scope(&self.scope)?;
        validate_code(&self.requisition_code, "requisition_code")?;
        validate_code(
            &self.requesting_department_code,
            "requesting_department_code",
        )?;
        if let Some(budget_code) = &self.budget_code {
            validate_code(budget_code, "budget_code")?;
        }
        validate_currency(&self.currency)?;
        validate_requisition_lines(&self.lines, &self.currency)?;
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(ProcurementDomainError::InvalidTime);
        }
        Ok(())
    }

    pub fn submit(
        &mut self,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<(), ProcurementDomainError> {
        if self.budget_code.is_none() {
            return Err(ProcurementDomainError::MissingBudgetReference);
        }
        self.transition(
            expected_version,
            PurchaseRequisitionStatus::Draft,
            PurchaseRequisitionStatus::Submitted,
            at,
        )?;
        self.uncommitted_events
            .push(PurchaseRequisitionDomainEvent::Submitted);
        Ok(())
    }

    pub fn approve(
        &mut self,
        expected_version: u64,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        at: DateTime<Utc>,
    ) -> Result<(), ProcurementDomainError> {
        self.validate_decision(decision, command_id, "approve", at)?;
        self.transition(
            expected_version,
            PurchaseRequisitionStatus::Submitted,
            PurchaseRequisitionStatus::Approved,
            at,
        )?;
        self.uncommitted_events
            .push(PurchaseRequisitionDomainEvent::Approved {
                state: ApprovedRequisitionState {
                    requisition_id: self.id,
                    approved_version: self.version,
                    scope: self.scope.clone(),
                    requisition_code: self.requisition_code.clone(),
                    requesting_department_code: self.requesting_department_code.clone(),
                    budget_code: self.budget_code.clone(),
                    lines: self.lines.clone(),
                },
                at,
            });
        Ok(())
    }

    pub fn reject(
        &mut self,
        expected_version: u64,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        at: DateTime<Utc>,
    ) -> Result<(), ProcurementDomainError> {
        self.validate_decision(decision, command_id, "reject", at)?;
        self.transition(
            expected_version,
            PurchaseRequisitionStatus::Submitted,
            PurchaseRequisitionStatus::Rejected,
            at,
        )?;
        self.uncommitted_events
            .push(PurchaseRequisitionDomainEvent::Rejected);
        Ok(())
    }

    pub fn cancel(
        &mut self,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<(), ProcurementDomainError> {
        if !matches!(
            self.status,
            PurchaseRequisitionStatus::Draft | PurchaseRequisitionStatus::Submitted
        ) {
            return Err(ProcurementDomainError::InvalidTransition);
        }
        self.advance(expected_version, at)?;
        self.status = PurchaseRequisitionStatus::Cancelled;
        self.uncommitted_events
            .push(PurchaseRequisitionDomainEvent::Cancelled);
        Ok(())
    }

    fn transition(
        &mut self,
        expected_version: u64,
        current: PurchaseRequisitionStatus,
        next: PurchaseRequisitionStatus,
        at: DateTime<Utc>,
    ) -> Result<(), ProcurementDomainError> {
        if self.status != current {
            return Err(ProcurementDomainError::InvalidTransition);
        }
        self.advance(expected_version, at)?;
        self.status = next;
        Ok(())
    }

    fn advance(
        &mut self,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<(), ProcurementDomainError> {
        if expected_version != self.version {
            return Err(ProcurementDomainError::VersionConflict {
                expected: expected_version,
                actual: self.version,
            });
        }
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

    fn validate_decision(
        &self,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        action: &'static str,
        at: DateTime<Utc>,
    ) -> Result<(), ProcurementDomainError> {
        if decision.decision_ref() != requisition_decision_ref(command_id, self.id, action)
            || decision.decided_at() != at
            || !decision.authority_scope().contains(&self.scope)
        {
            return Err(ProcurementDomainError::HumanDecisionRequired);
        }
        Ok(())
    }

    pub const fn id(&self) -> PurchaseRequisitionId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn requisition_code(&self) -> &str {
        &self.requisition_code
    }
    pub fn requesting_department_code(&self) -> &str {
        &self.requesting_department_code
    }
    pub fn budget_code(&self) -> Option<&str> {
        self.budget_code.as_deref()
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub fn lines(&self) -> &[PurchaseRequisitionLine] {
        &self.lines
    }
    pub const fn status(&self) -> PurchaseRequisitionStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<PurchaseRequisitionDomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoodsReceiptLine {
    pub line_id: GoodsReceiptLineId,
    pub purchase_order_line_id: PurchaseOrderLineId,
    pub inventory_stock_item_id: InventoryStockItemId,
    pub received_quantity: i64,
}

impl GoodsReceiptLine {
    pub fn validate(&self) -> Result<(), ProcurementDomainError> {
        if self.received_quantity <= 0 {
            return Err(ProcurementDomainError::InvalidAmount);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoodsReceiptStatus {
    PendingAcceptance,
    Accepted,
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptLineAllowance {
    pub purchase_order_line_id: PurchaseOrderLineId,
    pub ordered_quantity: i64,
    pub already_accepted_quantity: i64,
}

impl ReceiptLineAllowance {
    pub fn remaining_quantity(&self) -> Result<i64, ProcurementDomainError> {
        if self.ordered_quantity <= 0
            || self.already_accepted_quantity < 0
            || self.already_accepted_quantity > self.ordered_quantity
        {
            return Err(ProcurementDomainError::InvalidAmount);
        }
        Ok(self.ordered_quantity - self.already_accepted_quantity)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedGoodsReceiptLineState {
    pub goods_receipt_line_id: GoodsReceiptLineId,
    pub purchase_order_line_id: PurchaseOrderLineId,
    pub inventory_stock_item_id: InventoryStockItemId,
    pub accepted_quantity: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedGoodsReceiptState {
    pub goods_receipt_id: GoodsReceiptId,
    pub accepted_version: u64,
    pub scope: AuthorityScope,
    pub receipt_code: String,
    pub source_purchase_order: RecordVersionRef,
    pub lines: Vec<AcceptedGoodsReceiptLineState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoodsReceiptDomainEvent {
    Created,
    Accepted {
        state: AcceptedGoodsReceiptState,
        at: DateTime<Utc>,
    },
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoodsReceipt {
    id: GoodsReceiptId,
    version: u64,
    scope: AuthorityScope,
    receipt_code: String,
    source_purchase_order: RecordVersionRef,
    lines: Vec<GoodsReceiptLine>,
    status: GoodsReceiptStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<GoodsReceiptDomainEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoodsReceiptState {
    pub id: GoodsReceiptId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub receipt_code: String,
    pub source_purchase_order: RecordVersionRef,
    pub lines: Vec<GoodsReceiptLine>,
    pub status: GoodsReceiptStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl GoodsReceipt {
    pub fn create(
        id: GoodsReceiptId,
        scope: AuthorityScope,
        receipt_code: String,
        source_purchase_order: RecordVersionRef,
        lines: Vec<GoodsReceiptLine>,
        allowances: &[ReceiptLineAllowance],
        at: DateTime<Utc>,
    ) -> Result<Self, ProcurementDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            receipt_code,
            source_purchase_order,
            lines,
            status: GoodsReceiptStatus::PendingAcceptance,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![GoodsReceiptDomainEvent::Created],
        };
        value.validate()?;
        value.validate_allowances(allowances)?;
        Ok(value)
    }

    pub fn try_rehydrate(state: GoodsReceiptState) -> Result<Self, ProcurementDomainError> {
        let value = Self {
            id: state.id,
            version: state.version,
            scope: state.scope,
            receipt_code: state.receipt_code,
            source_purchase_order: state.source_purchase_order,
            lines: state.lines,
            status: state.status,
            created_at: state.created_at,
            updated_at: state.updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ProcurementDomainError> {
        validate_record_scope(&self.scope)?;
        validate_code(&self.receipt_code, "receipt_code")?;
        validate_exact_source(
            &self.source_purchase_order,
            ContextId::Procurement,
            "procurement.purchase-order",
        )?;
        if self.lines.is_empty() || self.lines.len() > MAX_LINES {
            return Err(ProcurementDomainError::InvalidLineCount);
        }
        let mut ids = BTreeSet::new();
        let mut purchase_order_lines = BTreeSet::new();
        for line in &self.lines {
            line.validate()?;
            if !ids.insert(line.line_id)
                || !purchase_order_lines.insert(line.purchase_order_line_id)
            {
                return Err(ProcurementDomainError::DuplicateLine);
            }
        }
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(ProcurementDomainError::InvalidTime);
        }
        Ok(())
    }

    pub fn validate_allowances(
        &self,
        allowances: &[ReceiptLineAllowance],
    ) -> Result<(), ProcurementDomainError> {
        for line in &self.lines {
            let allowance = allowances
                .iter()
                .find(|allowance| allowance.purchase_order_line_id == line.purchase_order_line_id)
                .ok_or(ProcurementDomainError::InvalidSourceReference)?;
            if line.received_quantity > allowance.remaining_quantity()? {
                return Err(ProcurementDomainError::ReceiptQuantityExceeded {
                    purchase_order_line_id: line.purchase_order_line_id.as_uuid(),
                    remaining: allowance.remaining_quantity()?,
                    requested: line.received_quantity,
                });
            }
        }
        Ok(())
    }

    pub fn accept(
        &mut self,
        expected_version: u64,
        current_allowances: &[ReceiptLineAllowance],
        decision: &HumanDecisionProof,
        command_id: CommandId,
        at: DateTime<Utc>,
    ) -> Result<(), ProcurementDomainError> {
        self.validate_decision(decision, command_id, "accept", at)?;
        self.validate_allowances(current_allowances)?;
        self.transition(expected_version, GoodsReceiptStatus::Accepted, at)?;
        self.uncommitted_events
            .push(GoodsReceiptDomainEvent::Accepted {
                state: AcceptedGoodsReceiptState {
                    goods_receipt_id: self.id,
                    accepted_version: self.version,
                    scope: self.scope.clone(),
                    receipt_code: self.receipt_code.clone(),
                    source_purchase_order: self.source_purchase_order.clone(),
                    lines: self
                        .lines
                        .iter()
                        .map(|line| AcceptedGoodsReceiptLineState {
                            goods_receipt_line_id: line.line_id,
                            purchase_order_line_id: line.purchase_order_line_id,
                            inventory_stock_item_id: line.inventory_stock_item_id,
                            accepted_quantity: line.received_quantity,
                        })
                        .collect(),
                },
                at,
            });
        Ok(())
    }

    pub fn reject(
        &mut self,
        expected_version: u64,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        at: DateTime<Utc>,
    ) -> Result<(), ProcurementDomainError> {
        self.validate_decision(decision, command_id, "reject", at)?;
        self.transition(expected_version, GoodsReceiptStatus::Rejected, at)?;
        self.uncommitted_events
            .push(GoodsReceiptDomainEvent::Rejected);
        Ok(())
    }

    pub fn cancel(
        &mut self,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<(), ProcurementDomainError> {
        self.transition(expected_version, GoodsReceiptStatus::Cancelled, at)?;
        self.uncommitted_events
            .push(GoodsReceiptDomainEvent::Cancelled);
        Ok(())
    }

    fn transition(
        &mut self,
        expected_version: u64,
        next: GoodsReceiptStatus,
        at: DateTime<Utc>,
    ) -> Result<(), ProcurementDomainError> {
        if self.status != GoodsReceiptStatus::PendingAcceptance {
            return Err(ProcurementDomainError::InvalidTransition);
        }
        if expected_version != self.version {
            return Err(ProcurementDomainError::VersionConflict {
                expected: expected_version,
                actual: self.version,
            });
        }
        if at < self.updated_at {
            return Err(ProcurementDomainError::InvalidTime);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(ProcurementDomainError::VersionOverflow)?;
        self.status = next;
        self.updated_at = at;
        Ok(())
    }

    fn validate_decision(
        &self,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        action: &'static str,
        at: DateTime<Utc>,
    ) -> Result<(), ProcurementDomainError> {
        if decision.decision_ref() != goods_receipt_decision_ref(command_id, self.id, action)
            || decision.decided_at() != at
            || !decision.authority_scope().contains(&self.scope)
        {
            return Err(ProcurementDomainError::HumanDecisionRequired);
        }
        Ok(())
    }

    pub const fn id(&self) -> GoodsReceiptId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn receipt_code(&self) -> &str {
        &self.receipt_code
    }
    pub const fn source_purchase_order(&self) -> &RecordVersionRef {
        &self.source_purchase_order
    }
    pub fn lines(&self) -> &[GoodsReceiptLine] {
        &self.lines
    }
    pub const fn status(&self) -> GoodsReceiptStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<GoodsReceiptDomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

fn validate_requisition_lines(
    lines: &[PurchaseRequisitionLine],
    currency: &str,
) -> Result<(), ProcurementDomainError> {
    if lines.is_empty() || lines.len() > MAX_LINES {
        return Err(ProcurementDomainError::InvalidLineCount);
    }
    let mut ids = BTreeSet::new();
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

pub(crate) fn requisition_decision_ref(
    command_id: CommandId,
    requisition_id: PurchaseRequisitionId,
    action: &'static str,
) -> String {
    format!(
        "procurement.requisition.{action}:{}:{command_id}",
        requisition_id.as_uuid()
    )
}

pub(crate) fn goods_receipt_decision_ref(
    command_id: CommandId,
    goods_receipt_id: GoodsReceiptId,
    action: &'static str,
) -> String {
    format!(
        "procurement.goods-receipt.{action}:{}:{command_id}",
        goods_receipt_id.as_uuid()
    )
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use erpx_integration_contracts::{RecordTypeId, StableRecordRef};
    use erpx_kernel::{ActorAttribution, ActorKind, RecordId, SessionId};

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 21, hour, 0, 0)
            .single()
            .unwrap()
    }

    fn scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: id(1),
            company_id: id(2),
        }
    }

    fn versioned(record_id: u128, record_type: &str, version: u64) -> RecordVersionRef {
        RecordVersionRef::try_new(
            StableRecordRef::try_new(
                RecordId::try_from_uuid(id(record_id)).unwrap(),
                ContextId::Procurement,
                RecordTypeId::try_new(record_type).unwrap(),
            )
            .unwrap(),
            version,
        )
        .unwrap()
    }

    fn decision(reference: String, at: DateTime<Utc>) -> HumanDecisionProof {
        HumanDecisionProof::try_new(
            ActorAttribution {
                principal_ref: "procurement-approver".into(),
                kind: ActorKind::HoldingUser,
                session_id: Some(SessionId::try_from_uuid(id(9)).unwrap()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope(),
            reference,
            at,
        )
        .unwrap()
    }

    fn requisition_line() -> PurchaseRequisitionLine {
        PurchaseRequisitionLine {
            line_id: PurchaseRequisitionLineId::try_from_uuid(id(11)).unwrap(),
            item_code: "ITEM-1".into(),
            requested_quantity: 5,
            estimated_unit_price_minor: 100,
            currency: "THB".into(),
            destination_code: "WH-1".into(),
        }
    }

    #[test]
    fn requisition_approval_is_exact_versioned_and_human_only() {
        let mut requisition = PurchaseRequisition::create(
            PurchaseRequisitionId::try_from_uuid(id(10)).unwrap(),
            scope(),
            "PR-1".into(),
            "FIN".into(),
            "BUDGET-1".into(),
            "THB".into(),
            vec![requisition_line()],
            at(8),
        )
        .unwrap();
        requisition.submit(1, at(9)).unwrap();
        let command_id = CommandId::try_from_uuid(id(20)).unwrap();
        let proof = decision(
            requisition_decision_ref(command_id, requisition.id(), "approve"),
            at(10),
        );
        assert_eq!(
            requisition.approve(1, &proof, command_id, at(10)),
            Err(ProcurementDomainError::VersionConflict {
                expected: 1,
                actual: 2,
            })
        );
        requisition.approve(2, &proof, command_id, at(10)).unwrap();
        assert_eq!(requisition.status(), PurchaseRequisitionStatus::Approved);

        assert!(
            HumanDecisionProof::try_new(
                ActorAttribution {
                    principal_ref: "agent".into(),
                    kind: ActorKind::Agent,
                    session_id: None,
                    represented_principal_ref: None,
                    support_capability_id: None,
                },
                scope(),
                "anything",
                at(10),
            )
            .is_err()
        );
    }

    #[test]
    fn receipt_rejects_over_receipt_and_acceptance_rechecks_current_allowance() {
        let allowances = vec![ReceiptLineAllowance {
            purchase_order_line_id: PurchaseOrderLineId::try_from_uuid(id(31)).unwrap(),
            ordered_quantity: 5,
            already_accepted_quantity: 2,
        }];
        let receipt_line = GoodsReceiptLine {
            line_id: GoodsReceiptLineId::try_from_uuid(id(32)).unwrap(),
            purchase_order_line_id: PurchaseOrderLineId::try_from_uuid(id(31)).unwrap(),
            inventory_stock_item_id: InventoryStockItemId::try_from_uuid(id(33)).unwrap(),
            received_quantity: 3,
        };
        let mut receipt = GoodsReceipt::create(
            GoodsReceiptId::try_from_uuid(id(30)).unwrap(),
            scope(),
            "GR-1".into(),
            versioned(40, "procurement.purchase-order", 2),
            vec![receipt_line.clone()],
            &allowances,
            at(9),
        )
        .unwrap();
        let reduced = vec![ReceiptLineAllowance {
            purchase_order_line_id: PurchaseOrderLineId::try_from_uuid(id(31)).unwrap(),
            ordered_quantity: 5,
            already_accepted_quantity: 3,
        }];
        let command_id = CommandId::try_from_uuid(id(41)).unwrap();
        let proof = decision(
            goods_receipt_decision_ref(command_id, receipt.id(), "accept"),
            at(10),
        );
        assert!(matches!(
            receipt.accept(1, &reduced, &proof, command_id, at(10)),
            Err(ProcurementDomainError::ReceiptQuantityExceeded { .. })
        ));
        receipt
            .accept(1, &allowances, &proof, command_id, at(10))
            .unwrap();
        assert_eq!(receipt.status(), GoodsReceiptStatus::Accepted);

        let too_many = GoodsReceipt::create(
            GoodsReceiptId::try_from_uuid(id(50)).unwrap(),
            scope(),
            "GR-2".into(),
            versioned(40, "procurement.purchase-order", 2),
            vec![GoodsReceiptLine {
                received_quantity: 4,
                ..receipt_line
            }],
            &allowances,
            at(9),
        );
        assert!(matches!(
            too_many,
            Err(ProcurementDomainError::ReceiptQuantityExceeded { .. })
        ));
    }

    #[test]
    fn legacy_requisition_without_budget_restores_but_cannot_submit() {
        let mut requisition = PurchaseRequisition::try_rehydrate(PurchaseRequisitionState {
            id: PurchaseRequisitionId::try_from_uuid(id(70)).unwrap(),
            version: 1,
            scope: scope(),
            requisition_code: "PR-LEGACY".into(),
            requesting_department_code: "FIN".into(),
            budget_code: None,
            currency: "THB".into(),
            lines: vec![requisition_line()],
            status: PurchaseRequisitionStatus::Draft,
            created_at: at(8),
            updated_at: at(8),
        })
        .unwrap();
        assert_eq!(requisition.budget_code(), None);
        assert_eq!(
            requisition.submit(1, at(9)),
            Err(ProcurementDomainError::MissingBudgetReference)
        );
    }
}
