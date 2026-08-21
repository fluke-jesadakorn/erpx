use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use erpx_integration_contracts::RecordVersionRef;
use erpx_kernel::{AuthorityScope, Clock, HumanDecisionProof};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{
        GoodsReceipt, GoodsReceiptId, GoodsReceiptLine, ProcurementDomainError,
        PurchaseOrderStatus, PurchaseRequisition, PurchaseRequisitionId, PurchaseRequisitionLine,
        goods_receipt_decision_ref, requisition_decision_ref,
    },
    ports::{
        InventoryStockItemSource, PortError, ProcurementSourceGuard, ProcurementWorkflowAction,
        ProcurementWorkflowAggregate, ProcurementWorkflowAuthorizationPort,
        ProcurementWorkflowAuthorizationRequest, ProcurementWorkflowCommandContext,
        ProcurementWorkflowCommandFingerprint, ProcurementWorkflowCommandIdentity,
        ProcurementWorkflowCommit, ProcurementWorkflowDomainEvent, ProcurementWorkflowRepository,
        ProcurementWorkflowUnitOfWork, PurchaseOrderReceiptSource, StoredProcurementWorkflowCommit,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePurchaseRequisition {
    pub requisition_id: PurchaseRequisitionId,
    pub scope: AuthorityScope,
    pub requisition_code: String,
    pub requesting_department_code: String,
    pub budget_code: String,
    pub currency: String,
    pub lines: Vec<PurchaseRequisitionLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitPurchaseRequisition {
    pub requisition_id: PurchaseRequisitionId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovePurchaseRequisition {
    pub requisition_id: PurchaseRequisitionId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectPurchaseRequisition {
    pub requisition_id: PurchaseRequisitionId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelPurchaseRequisition {
    pub requisition_id: PurchaseRequisitionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateGoodsReceipt {
    pub goods_receipt_id: GoodsReceiptId,
    pub scope: AuthorityScope,
    pub receipt_code: String,
    pub source_purchase_order: RecordVersionRef,
    pub lines: Vec<GoodsReceiptLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptGoodsReceipt {
    pub goods_receipt_id: GoodsReceiptId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectGoodsReceipt {
    pub goods_receipt_id: GoodsReceiptId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelGoodsReceipt {
    pub goods_receipt_id: GoodsReceiptId,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProcurementWorkflowApplicationError {
    #[error(transparent)]
    Domain(#[from] ProcurementDomainError),
    #[error("create commands cannot declare an aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutations require an exact expected aggregate version")]
    MissingExpectedVersion,
    #[error("command authority does not contain the target scope")]
    ScopeOutsideAuthority,
    #[error("business sources must have the exact version and same company or branch scope")]
    InvalidSource,
    #[error("a directly authenticated human decision is required")]
    HumanDecisionRequired,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct ProcurementWorkflowCommandService<R, U, A, C> {
    repository: R,
    unit_of_work: U,
    authorization: A,
    clock: C,
}

impl<R, U, A, C> ProcurementWorkflowCommandService<R, U, A, C> {
    pub const fn new(repository: R, unit_of_work: U, authorization: A, clock: C) -> Self {
        Self {
            repository,
            unit_of_work,
            authorization,
            clock,
        }
    }
}

impl<R, U, A, C> ProcurementWorkflowCommandService<R, U, A, C>
where
    R: ProcurementWorkflowRepository,
    U: ProcurementWorkflowUnitOfWork,
    A: ProcurementWorkflowAuthorizationPort,
    C: Clock,
{
    pub async fn create_requisition(
        &self,
        context: ProcurementWorkflowCommandContext,
        command: CreatePurchaseRequisition,
    ) -> Result<StoredProcurementWorkflowCommit, ProcurementWorkflowApplicationError> {
        let identity = workflow_identity(
            &context,
            ProcurementWorkflowCommandFingerprint::CreateRequisition {
                requisition_id: command.requisition_id,
                scope: command.scope.clone(),
                requisition_code: command.requisition_code.clone(),
                requesting_department_code: command.requesting_department_code.clone(),
                budget_code: command.budget_code.clone(),
                currency: command.currency.clone(),
                lines: command.lines.clone(),
            },
        );
        if let Some(stored) = self
            .replay(
                &context,
                &identity,
                ProcurementWorkflowAction::CreateRequisition,
            )
            .await?
        {
            return Ok(stored);
        }
        require_create(&context)?;
        self.authorize(
            &context,
            &command.scope,
            ProcurementWorkflowAction::CreateRequisition,
        )
        .await?;
        let mut aggregate = PurchaseRequisition::create(
            command.requisition_id,
            command.scope,
            command.requisition_code,
            command.requesting_department_code,
            command.budget_code,
            command.currency,
            command.lines,
            self.clock.now(),
        )?;
        let events = aggregate
            .take_uncommitted_events()
            .into_iter()
            .map(ProcurementWorkflowDomainEvent::Requisition)
            .collect();
        self.commit(
            context,
            identity,
            None,
            ProcurementWorkflowAggregate::Requisition(aggregate),
            Vec::new(),
            events,
        )
        .await
    }

    pub async fn submit_requisition(
        &self,
        context: ProcurementWorkflowCommandContext,
        command: SubmitPurchaseRequisition,
    ) -> Result<StoredProcurementWorkflowCommit, ProcurementWorkflowApplicationError> {
        self.mutate_requisition(
            context,
            command.requisition_id,
            ProcurementWorkflowAction::SubmitRequisition,
            RequisitionMutation::Submit,
        )
        .await
    }

    pub async fn approve_requisition(
        &self,
        context: ProcurementWorkflowCommandContext,
        command: ApprovePurchaseRequisition,
    ) -> Result<StoredProcurementWorkflowCommit, ProcurementWorkflowApplicationError> {
        self.mutate_requisition(
            context,
            command.requisition_id,
            ProcurementWorkflowAction::ApproveRequisition,
            RequisitionMutation::Approve,
        )
        .await
    }

    pub async fn reject_requisition(
        &self,
        context: ProcurementWorkflowCommandContext,
        command: RejectPurchaseRequisition,
    ) -> Result<StoredProcurementWorkflowCommit, ProcurementWorkflowApplicationError> {
        self.mutate_requisition(
            context,
            command.requisition_id,
            ProcurementWorkflowAction::RejectRequisition,
            RequisitionMutation::Reject,
        )
        .await
    }

    pub async fn cancel_requisition(
        &self,
        context: ProcurementWorkflowCommandContext,
        command: CancelPurchaseRequisition,
    ) -> Result<StoredProcurementWorkflowCommit, ProcurementWorkflowApplicationError> {
        self.mutate_requisition(
            context,
            command.requisition_id,
            ProcurementWorkflowAction::CancelRequisition,
            RequisitionMutation::Cancel,
        )
        .await
    }

    pub async fn create_goods_receipt(
        &self,
        context: ProcurementWorkflowCommandContext,
        command: CreateGoodsReceipt,
    ) -> Result<StoredProcurementWorkflowCommit, ProcurementWorkflowApplicationError> {
        let identity = workflow_identity(
            &context,
            ProcurementWorkflowCommandFingerprint::CreateGoodsReceipt {
                goods_receipt_id: command.goods_receipt_id,
                scope: command.scope.clone(),
                receipt_code: command.receipt_code.clone(),
                source_purchase_order: command.source_purchase_order.clone(),
                lines: command.lines.clone(),
            },
        );
        if let Some(stored) = self
            .replay(
                &context,
                &identity,
                ProcurementWorkflowAction::CreateGoodsReceipt,
            )
            .await?
        {
            return Ok(stored);
        }
        require_create(&context)?;
        self.authorize(
            &context,
            &command.scope,
            ProcurementWorkflowAction::CreateGoodsReceipt,
        )
        .await?;
        let source = self
            .repository
            .load_purchase_order_receipt_source(
                &context.authority_scope,
                &command.source_purchase_order,
            )
            .await?
            .ok_or(ProcurementWorkflowApplicationError::InvalidSource)?;
        validate_receipt_source(&command.source_purchase_order, &command.scope, &source)?;
        let stock_items = self
            .resolve_stock_items(&context, &command.scope, &command.lines)
            .await?;
        let mut aggregate = GoodsReceipt::create(
            command.goods_receipt_id,
            command.scope,
            command.receipt_code,
            command.source_purchase_order.clone(),
            command.lines,
            &source.allowances,
            self.clock.now(),
        )?;
        let events = aggregate
            .take_uncommitted_events()
            .into_iter()
            .map(ProcurementWorkflowDomainEvent::GoodsReceipt)
            .collect();
        let guard = ProcurementSourceGuard::ReceiptQuantities {
            purchase_order: command.source_purchase_order,
            allowances: source.allowances,
            inventory_stock_items: stock_items,
        };
        self.commit(
            context,
            identity,
            None,
            ProcurementWorkflowAggregate::GoodsReceipt(aggregate),
            vec![guard],
            events,
        )
        .await
    }

    pub async fn accept_goods_receipt(
        &self,
        context: ProcurementWorkflowCommandContext,
        command: AcceptGoodsReceipt,
    ) -> Result<StoredProcurementWorkflowCommit, ProcurementWorkflowApplicationError> {
        self.mutate_goods_receipt(
            context,
            command.goods_receipt_id,
            ProcurementWorkflowAction::AcceptGoodsReceipt,
            GoodsReceiptMutation::Accept,
        )
        .await
    }

    pub async fn reject_goods_receipt(
        &self,
        context: ProcurementWorkflowCommandContext,
        command: RejectGoodsReceipt,
    ) -> Result<StoredProcurementWorkflowCommit, ProcurementWorkflowApplicationError> {
        self.mutate_goods_receipt(
            context,
            command.goods_receipt_id,
            ProcurementWorkflowAction::RejectGoodsReceipt,
            GoodsReceiptMutation::Reject,
        )
        .await
    }

    pub async fn cancel_goods_receipt(
        &self,
        context: ProcurementWorkflowCommandContext,
        command: CancelGoodsReceipt,
    ) -> Result<StoredProcurementWorkflowCommit, ProcurementWorkflowApplicationError> {
        self.mutate_goods_receipt(
            context,
            command.goods_receipt_id,
            ProcurementWorkflowAction::CancelGoodsReceipt,
            GoodsReceiptMutation::Cancel,
        )
        .await
    }

    async fn mutate_requisition(
        &self,
        context: ProcurementWorkflowCommandContext,
        requisition_id: PurchaseRequisitionId,
        action: ProcurementWorkflowAction,
        mutation: RequisitionMutation,
    ) -> Result<StoredProcurementWorkflowCommit, ProcurementWorkflowApplicationError> {
        let identity = workflow_identity(&context, mutation.fingerprint(requisition_id));
        if let Some(stored) = self.replay(&context, &identity, action).await? {
            return Ok(stored);
        }
        let expected = require_expected(&context)?;
        let mut aggregate = self
            .repository
            .load_requisition(&context.authority_scope, requisition_id)
            .await?
            .ok_or(PortError::NotFound)?;
        self.authorize(&context, aggregate.scope(), action).await?;
        let at = self.clock.now();
        match mutation {
            RequisitionMutation::Submit => aggregate.submit(expected, at)?,
            RequisitionMutation::Approve | RequisitionMutation::Reject => {
                let action_name = mutation.decision_action();
                let proof = human_decision(
                    &context,
                    requisition_decision_ref(context.command_id, requisition_id, action_name),
                    at,
                )?;
                if mutation == RequisitionMutation::Approve {
                    aggregate.approve(expected, &proof, context.command_id, at)?;
                } else {
                    aggregate.reject(expected, &proof, context.command_id, at)?;
                }
            }
            RequisitionMutation::Cancel => aggregate.cancel(expected, at)?,
        }
        let events = aggregate
            .take_uncommitted_events()
            .into_iter()
            .map(ProcurementWorkflowDomainEvent::Requisition)
            .collect();
        self.commit(
            context,
            identity,
            Some(expected),
            ProcurementWorkflowAggregate::Requisition(aggregate),
            Vec::new(),
            events,
        )
        .await
    }

    async fn mutate_goods_receipt(
        &self,
        context: ProcurementWorkflowCommandContext,
        goods_receipt_id: GoodsReceiptId,
        action: ProcurementWorkflowAction,
        mutation: GoodsReceiptMutation,
    ) -> Result<StoredProcurementWorkflowCommit, ProcurementWorkflowApplicationError> {
        let identity = workflow_identity(&context, mutation.fingerprint(goods_receipt_id));
        if let Some(stored) = self.replay(&context, &identity, action).await? {
            return Ok(stored);
        }
        let expected = require_expected(&context)?;
        let mut aggregate = self
            .repository
            .load_goods_receipt(&context.authority_scope, goods_receipt_id)
            .await?
            .ok_or(PortError::NotFound)?;
        self.authorize(&context, aggregate.scope(), action).await?;
        let source = self
            .repository
            .load_purchase_order_receipt_source(
                &context.authority_scope,
                aggregate.source_purchase_order(),
            )
            .await?
            .ok_or(ProcurementWorkflowApplicationError::InvalidSource)?;
        validate_receipt_source(
            aggregate.source_purchase_order(),
            aggregate.scope(),
            &source,
        )?;
        let stock_items = self
            .resolve_stock_items(&context, aggregate.scope(), aggregate.lines())
            .await?;
        let at = self.clock.now();
        match mutation {
            GoodsReceiptMutation::Accept | GoodsReceiptMutation::Reject => {
                let action_name = mutation.decision_action();
                let proof = human_decision(
                    &context,
                    goods_receipt_decision_ref(context.command_id, goods_receipt_id, action_name),
                    at,
                )?;
                if mutation == GoodsReceiptMutation::Accept {
                    aggregate.accept(
                        expected,
                        &source.allowances,
                        &proof,
                        context.command_id,
                        at,
                    )?;
                } else {
                    aggregate.reject(expected, &proof, context.command_id, at)?;
                }
            }
            GoodsReceiptMutation::Cancel => aggregate.cancel(expected, at)?,
        }
        let source_ref = aggregate.source_purchase_order().clone();
        let events = aggregate
            .take_uncommitted_events()
            .into_iter()
            .map(ProcurementWorkflowDomainEvent::GoodsReceipt)
            .collect();
        let guard = ProcurementSourceGuard::ReceiptQuantities {
            purchase_order: source_ref,
            allowances: source.allowances,
            inventory_stock_items: stock_items,
        };
        self.commit(
            context,
            identity,
            Some(expected),
            ProcurementWorkflowAggregate::GoodsReceipt(aggregate),
            vec![guard],
            events,
        )
        .await
    }

    async fn resolve_stock_items(
        &self,
        context: &ProcurementWorkflowCommandContext,
        target_scope: &AuthorityScope,
        lines: &[GoodsReceiptLine],
    ) -> Result<Vec<InventoryStockItemSource>, ProcurementWorkflowApplicationError> {
        let requested = lines
            .iter()
            .map(|line| line.inventory_stock_item_id)
            .collect::<BTreeSet<_>>();
        let requested_ids = requested.iter().copied().collect::<Vec<_>>();
        let resolved = self
            .repository
            .resolve_inventory_stock_items(&context.authority_scope, &requested_ids)
            .await?;
        let actual = resolved
            .iter()
            .map(|stock_item| stock_item.stock_item_id)
            .collect::<BTreeSet<_>>();
        if requested != actual
            || resolved
                .iter()
                .any(|stock_item| &stock_item.scope != target_scope)
        {
            return Err(ProcurementWorkflowApplicationError::InvalidSource);
        }
        Ok(resolved)
    }

    async fn replay(
        &self,
        context: &ProcurementWorkflowCommandContext,
        identity: &ProcurementWorkflowCommandIdentity,
        action: ProcurementWorkflowAction,
    ) -> Result<Option<StoredProcurementWorkflowCommit>, ProcurementWorkflowApplicationError> {
        let Some(mut stored) = self.unit_of_work.find_workflow_receipt(identity).await? else {
            return Ok(None);
        };
        self.authorize(context, stored.aggregate.scope(), action)
            .await?;
        stored.replayed = true;
        Ok(Some(stored))
    }

    async fn authorize(
        &self,
        context: &ProcurementWorkflowCommandContext,
        target_scope: &AuthorityScope,
        action: ProcurementWorkflowAction,
    ) -> Result<(), ProcurementWorkflowApplicationError> {
        if !context.authority_scope.contains(target_scope) {
            return Err(ProcurementWorkflowApplicationError::ScopeOutsideAuthority);
        }
        self.authorization
            .authorize_procurement_workflow(ProcurementWorkflowAuthorizationRequest {
                actor: &context.actor,
                authority_scope: &context.authority_scope,
                target_scope,
                action,
            })
            .await?;
        Ok(())
    }

    async fn commit(
        &self,
        context: ProcurementWorkflowCommandContext,
        identity: ProcurementWorkflowCommandIdentity,
        expected_version: Option<u64>,
        aggregate: ProcurementWorkflowAggregate,
        source_guards: Vec<ProcurementSourceGuard>,
        domain_events: Vec<ProcurementWorkflowDomainEvent>,
    ) -> Result<StoredProcurementWorkflowCommit, ProcurementWorkflowApplicationError> {
        Ok(self
            .unit_of_work
            .commit_workflow(ProcurementWorkflowCommit {
                identity,
                correlation_id: context.correlation_id,
                actor: context.actor,
                authority_scope: context.authority_scope,
                audit: context.audit,
                expected_version,
                aggregate,
                source_guards,
                domain_events,
            })
            .await?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequisitionMutation {
    Submit,
    Approve,
    Reject,
    Cancel,
}

impl RequisitionMutation {
    fn fingerprint(
        self,
        requisition_id: PurchaseRequisitionId,
    ) -> ProcurementWorkflowCommandFingerprint {
        match self {
            Self::Submit => {
                ProcurementWorkflowCommandFingerprint::SubmitRequisition { requisition_id }
            }
            Self::Approve => {
                ProcurementWorkflowCommandFingerprint::ApproveRequisition { requisition_id }
            }
            Self::Reject => {
                ProcurementWorkflowCommandFingerprint::RejectRequisition { requisition_id }
            }
            Self::Cancel => {
                ProcurementWorkflowCommandFingerprint::CancelRequisition { requisition_id }
            }
        }
    }

    const fn decision_action(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Submit | Self::Cancel => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GoodsReceiptMutation {
    Accept,
    Reject,
    Cancel,
}

impl GoodsReceiptMutation {
    fn fingerprint(
        self,
        goods_receipt_id: GoodsReceiptId,
    ) -> ProcurementWorkflowCommandFingerprint {
        match self {
            Self::Accept => {
                ProcurementWorkflowCommandFingerprint::AcceptGoodsReceipt { goods_receipt_id }
            }
            Self::Reject => {
                ProcurementWorkflowCommandFingerprint::RejectGoodsReceipt { goods_receipt_id }
            }
            Self::Cancel => {
                ProcurementWorkflowCommandFingerprint::CancelGoodsReceipt { goods_receipt_id }
            }
        }
    }

    const fn decision_action(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Reject => "reject",
            Self::Cancel => "",
        }
    }
}

fn workflow_identity(
    context: &ProcurementWorkflowCommandContext,
    fingerprint: ProcurementWorkflowCommandFingerprint,
) -> ProcurementWorkflowCommandIdentity {
    ProcurementWorkflowCommandIdentity {
        command_id: context.command_id,
        idempotency_key: context.idempotency_key.clone(),
        fingerprint,
    }
}

fn require_create(
    context: &ProcurementWorkflowCommandContext,
) -> Result<(), ProcurementWorkflowApplicationError> {
    if context.expected_version.is_some() {
        Err(ProcurementWorkflowApplicationError::UnexpectedCreateVersion)
    } else {
        Ok(())
    }
}

fn require_expected(
    context: &ProcurementWorkflowCommandContext,
) -> Result<u64, ProcurementWorkflowApplicationError> {
    context
        .expected_version
        .ok_or(ProcurementWorkflowApplicationError::MissingExpectedVersion)
}

fn human_decision(
    context: &ProcurementWorkflowCommandContext,
    decision_ref: String,
    at: DateTime<Utc>,
) -> Result<HumanDecisionProof, ProcurementWorkflowApplicationError> {
    HumanDecisionProof::try_new(
        context.actor.clone(),
        context.authority_scope.clone(),
        decision_ref,
        at,
    )
    .map_err(|_| ProcurementWorkflowApplicationError::HumanDecisionRequired)
}

fn validate_receipt_source(
    source_ref: &RecordVersionRef,
    target_scope: &AuthorityScope,
    source: &PurchaseOrderReceiptSource,
) -> Result<(), ProcurementWorkflowApplicationError> {
    if source.order.id().as_uuid() != source_ref.record().id().as_uuid()
        || source.order.version() != source_ref.version()
        || source.order.status() != PurchaseOrderStatus::Released
        || source.order.scope() != target_scope
    {
        return Err(ProcurementWorkflowApplicationError::InvalidSource);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use chrono::TimeZone;
    use erpx_cqrs::IdempotencyKey;
    use erpx_integration_contracts::{RecordTypeId, StableRecordRef, catalog::ContextId};
    use erpx_kernel::{
        ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, FixedClock, RecordId,
        SessionId,
    };
    use uuid::Uuid;

    use super::*;
    use crate::{
        GoodsReceiptLineId, InventoryStockItemId, PurchaseOrder, PurchaseOrderId,
        PurchaseOrderLine, PurchaseOrderLineId, ReceiptLineAllowance, SupplierId,
        ports::{ProcurementWorkflowCommit, PurchaseOrderReceiptSource},
    };

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 21, 9, 0, 0).single().unwrap()
    }

    fn scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: id(1),
            company_id: id(2),
        }
    }

    fn actor(kind: ActorKind) -> ActorAttribution {
        ActorAttribution {
            principal_ref: "procurement-user".into(),
            kind,
            session_id: (kind == ActorKind::HoldingUser)
                .then(|| SessionId::try_from_uuid(id(3)).unwrap()),
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }

    fn context(
        sequence: u128,
        kind: ActorKind,
        expected_version: Option<u64>,
    ) -> ProcurementWorkflowCommandContext {
        ProcurementWorkflowCommandContext {
            command_id: CommandId::try_from_uuid(id(100 + sequence)).unwrap(),
            idempotency_key: IdempotencyKey::try_new(format!("workflow-{sequence}")).unwrap(),
            correlation_id: CorrelationId::try_from_uuid(id(200 + sequence)).unwrap(),
            actor: actor(kind),
            authority_scope: scope(),
            audit: AuditContext {
                purpose: "procurement.workflow.test".into(),
                customer_visible: false,
                module: Some("procurement".into()),
            },
            expected_version,
        }
    }

    fn ref_version(record_id: u128, record_type: &str, version: u64) -> RecordVersionRef {
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

    fn source_order() -> PurchaseOrderReceiptSource {
        let requisition = ref_version(50, "procurement.requisition", 3);
        let order = PurchaseOrder::try_rehydrate(crate::PurchaseOrderState {
            id: PurchaseOrderId::try_from_uuid(id(40)).unwrap(),
            version: 2,
            scope: scope(),
            order_code: "PO-1".into(),
            supplier_id: SupplierId::try_from_uuid(id(41)).unwrap(),
            currency: "THB".into(),
            lines: vec![PurchaseOrderLine {
                line_id: PurchaseOrderLineId::try_from_uuid(id(42)).unwrap(),
                item_code: "ITEM-1".into(),
                ordered_quantity: 5,
                unit_price_minor: 100,
                currency: "THB".into(),
                destination_code: "WH-1".into(),
            }],
            source_requisition: Some(requisition),
            contract: None,
            status: PurchaseOrderStatus::Released,
            created_at: at(),
            updated_at: at(),
        })
        .unwrap();
        PurchaseOrderReceiptSource {
            order,
            allowances: vec![ReceiptLineAllowance {
                purchase_order_line_id: PurchaseOrderLineId::try_from_uuid(id(42)).unwrap(),
                ordered_quantity: 5,
                already_accepted_quantity: 2,
            }],
        }
    }

    #[derive(Clone)]
    struct Repo {
        receipt: Option<GoodsReceipt>,
    }

    #[async_trait]
    impl ProcurementWorkflowRepository for Repo {
        async fn load_requisition(
            &self,
            _: &AuthorityScope,
            _: PurchaseRequisitionId,
        ) -> Result<Option<PurchaseRequisition>, PortError> {
            Ok(None)
        }
        async fn load_goods_receipt(
            &self,
            _: &AuthorityScope,
            _: GoodsReceiptId,
        ) -> Result<Option<GoodsReceipt>, PortError> {
            Ok(self.receipt.clone())
        }
        async fn load_purchase_order_receipt_source(
            &self,
            _: &AuthorityScope,
            _: &RecordVersionRef,
        ) -> Result<Option<PurchaseOrderReceiptSource>, PortError> {
            Ok(Some(source_order()))
        }
        async fn resolve_inventory_stock_items(
            &self,
            _: &AuthorityScope,
            ids: &[InventoryStockItemId],
        ) -> Result<Vec<InventoryStockItemSource>, PortError> {
            Ok(ids
                .iter()
                .map(|id| InventoryStockItemSource {
                    stock_item_id: *id,
                    scope: scope(),
                })
                .collect())
        }
    }

    #[derive(Clone, Default)]
    struct Uow {
        stored: Arc<
            Mutex<
                Option<(
                    ProcurementWorkflowCommandIdentity,
                    StoredProcurementWorkflowCommit,
                )>,
            >,
        >,
    }

    #[async_trait]
    impl ProcurementWorkflowUnitOfWork for Uow {
        async fn find_workflow_receipt(
            &self,
            identity: &ProcurementWorkflowCommandIdentity,
        ) -> Result<Option<StoredProcurementWorkflowCommit>, PortError> {
            let stored = self.stored.lock().unwrap();
            match stored.as_ref() {
                Some((stored_identity, value)) if stored_identity == identity => {
                    Ok(Some(value.clone()))
                }
                Some(_) => Err(PortError::IdempotencyConflict),
                None => Ok(None),
            }
        }
        async fn commit_workflow(
            &self,
            commit: ProcurementWorkflowCommit,
        ) -> Result<StoredProcurementWorkflowCommit, PortError> {
            let stored = StoredProcurementWorkflowCommit {
                command_id: commit.identity.command_id,
                correlation_id: commit.correlation_id,
                aggregate: commit.aggregate,
                committed_events: Vec::new(),
                committed_at: at(),
                replayed: false,
            };
            *self.stored.lock().unwrap() = Some((commit.identity, stored.clone()));
            Ok(stored)
        }
    }

    #[derive(Clone)]
    struct Auth;
    #[async_trait]
    impl ProcurementWorkflowAuthorizationPort for Auth {
        async fn authorize_procurement_workflow(
            &self,
            _: ProcurementWorkflowAuthorizationRequest<'_>,
        ) -> Result<(), PortError> {
            Ok(())
        }
    }

    fn receipt_command() -> CreateGoodsReceipt {
        CreateGoodsReceipt {
            goods_receipt_id: GoodsReceiptId::try_from_uuid(id(60)).unwrap(),
            scope: scope(),
            receipt_code: "GR-1".into(),
            source_purchase_order: ref_version(40, "procurement.purchase-order", 2),
            lines: vec![GoodsReceiptLine {
                line_id: GoodsReceiptLineId::try_from_uuid(id(61)).unwrap(),
                purchase_order_line_id: PurchaseOrderLineId::try_from_uuid(id(42)).unwrap(),
                inventory_stock_item_id: InventoryStockItemId::try_from_uuid(id(62)).unwrap(),
                received_quantity: 3,
            }],
        }
    }

    #[tokio::test]
    async fn goods_receipt_create_is_idempotent_and_carries_atomic_source_guards() {
        let uow = Uow::default();
        let service = ProcurementWorkflowCommandService::new(
            Repo { receipt: None },
            uow,
            Auth,
            FixedClock::new(at()),
        );
        let context = context(1, ActorKind::HoldingUser, None);
        let first = service
            .create_goods_receipt(context.clone(), receipt_command())
            .await
            .unwrap();
        assert!(!first.replayed);
        let replay = service
            .create_goods_receipt(context, receipt_command())
            .await
            .unwrap();
        assert!(replay.replayed);
    }

    #[tokio::test]
    async fn receipt_acceptance_rejects_agent_before_domain_transition() {
        let source = source_order();
        let receipt = GoodsReceipt::create(
            receipt_command().goods_receipt_id,
            scope(),
            "GR-1".into(),
            ref_version(40, "procurement.purchase-order", 2),
            receipt_command().lines,
            &source.allowances,
            at(),
        )
        .unwrap();
        let service = ProcurementWorkflowCommandService::new(
            Repo {
                receipt: Some(receipt),
            },
            Uow::default(),
            Auth,
            FixedClock::new(at()),
        );
        let error = service
            .accept_goods_receipt(
                context(2, ActorKind::Agent, Some(1)),
                AcceptGoodsReceipt {
                    goods_receipt_id: GoodsReceiptId::try_from_uuid(id(60)).unwrap(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(
            error,
            ProcurementWorkflowApplicationError::HumanDecisionRequired
        );
    }
}
