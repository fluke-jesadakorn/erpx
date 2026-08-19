use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{ItemDescriptorDraft, pos};
use erpx_kernel::AuthorityScope;
use erpx_pos::{PaymentMethod, RefundStatus, RefundView, SaleStatus, SaleView};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    CONTEXT_NAME, EVENT_VERSION, REFUND_AGGREGATE_TYPE, REFUND_PROJECTION_NAME,
    REFUND_SNAPSHOT_EVENT_TYPE, SALE_AGGREGATE_TYPE, SALE_PROJECTION_NAME,
    SALE_SNAPSHOT_EVENT_TYPE, refund_partition, sale_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosSaleSnapshot {
    pub sale: SaleView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosRefundSnapshot {
    pub refund: RefundView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PosOutboxPayload {
    Integration(pos::Event),
    SaleSnapshot(PosSaleSnapshot),
    RefundSnapshot(PosRefundSnapshot),
    Directory(ItemDescriptorDraft),
}

#[derive(Debug, Clone)]
pub struct PosSaleProjector {
    event_types: Vec<String>,
}

impl PosSaleProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![SALE_SNAPSHOT_EVENT_TYPE.into()],
        }
    }
}

impl Default for PosSaleProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for PosSaleProjector {
    fn name(&self) -> &str {
        SALE_PROJECTION_NAME
    }

    fn event_types(&self) -> &[String] {
        &self.event_types
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let view = validate_sale_event(context, event)?;
        let state = serde_json::to_value(&view).map_err(failure)?;
        let lines = serde_json::to_value(&view.lines).map_err(failure)?;
        let payments = serde_json::to_value(&view.payments).map_err(failure)?;
        let result = sqlx::query(
            "INSERT INTO erpx_pos_read.sale_projection
                (projection_name,generation,partition_key,sale_id,
                 source_aggregate_version,source_global_sequence,active_scope,scope_key,
                 receipt_code,register_id,customer_id,currency,subtotal_minor,tax_minor,
                 total_minor,paid_minor,sale_status,lines,payments,state,
                 aggregate_created_at,aggregate_updated_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,now())
             ON CONFLICT (holding_id,projection_name,generation,partition_key,sale_id)
             DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
                source_global_sequence=EXCLUDED.source_global_sequence,
                active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,
                receipt_code=EXCLUDED.receipt_code,register_id=EXCLUDED.register_id,
                customer_id=EXCLUDED.customer_id,currency=EXCLUDED.currency,
                subtotal_minor=EXCLUDED.subtotal_minor,tax_minor=EXCLUDED.tax_minor,
                total_minor=EXCLUDED.total_minor,paid_minor=EXCLUDED.paid_minor,
                sale_status=EXCLUDED.sale_status,lines=EXCLUDED.lines,payments=EXCLUDED.payments,
                state=EXCLUDED.state,aggregate_created_at=EXCLUDED.aggregate_created_at,
                aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
             WHERE erpx_pos_read.sale_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(SALE_PROJECTION_NAME)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(view.sale_id.as_uuid())
        .bind(to_i64(view.version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(serde_json::to_value(&view.scope).map_err(failure)?)
        .bind(scope_key(&view.scope)?)
        .bind(&view.receipt_code)
        .bind(view.register_id.as_uuid())
        .bind(view.customer_id.map(|value| value.as_uuid()))
        .bind(&view.currency)
        .bind(view.subtotal_minor)
        .bind(view.tax_minor)
        .bind(view.total_minor)
        .bind(view.paid_minor)
        .bind(sale_status_name(view.status))
        .bind(lines)
        .bind(payments)
        .bind(state)
        .bind(view.created_at)
        .bind(view.updated_at)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        if result.rows_affected() != 1 {
            return Err(ProjectionFailure::new(
                "POS sale snapshot is duplicate or out of order",
            ));
        }
        Ok(())
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query(
            "DELETE FROM erpx_pos_read.sale_projection
             WHERE projection_name=$1 AND generation=$2",
        )
        .bind(SALE_PROJECTION_NAME)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PosRefundProjector {
    event_types: Vec<String>,
}

impl PosRefundProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![REFUND_SNAPSHOT_EVENT_TYPE.into()],
        }
    }
}

impl Default for PosRefundProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for PosRefundProjector {
    fn name(&self) -> &str {
        REFUND_PROJECTION_NAME
    }

    fn event_types(&self) -> &[String] {
        &self.event_types
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let view = validate_refund_event(context, event)?;
        let state = serde_json::to_value(&view).map_err(failure)?;
        let lines = serde_json::to_value(&view.lines).map_err(failure)?;
        let result = sqlx::query(
            "INSERT INTO erpx_pos_read.refund_projection
                (projection_name,generation,partition_key,refund_id,
                 source_aggregate_version,source_global_sequence,active_scope,scope_key,
                 sale_id,currency,total_amount_minor,refund_status,requester_principal_ref,
                 reviewer_principal_ref,lines,state,aggregate_requested_at,
                 aggregate_updated_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,now())
             ON CONFLICT (holding_id,projection_name,generation,partition_key,refund_id)
             DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
                source_global_sequence=EXCLUDED.source_global_sequence,
                active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,
                sale_id=EXCLUDED.sale_id,currency=EXCLUDED.currency,
                total_amount_minor=EXCLUDED.total_amount_minor,
                refund_status=EXCLUDED.refund_status,
                requester_principal_ref=EXCLUDED.requester_principal_ref,
                reviewer_principal_ref=EXCLUDED.reviewer_principal_ref,
                lines=EXCLUDED.lines,state=EXCLUDED.state,
                aggregate_requested_at=EXCLUDED.aggregate_requested_at,
                aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
             WHERE erpx_pos_read.refund_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(REFUND_PROJECTION_NAME)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(view.refund_id.as_uuid())
        .bind(to_i64(view.version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(serde_json::to_value(&view.scope).map_err(failure)?)
        .bind(scope_key(&view.scope)?)
        .bind(view.sale_id.as_uuid())
        .bind(&view.currency)
        .bind(view.total_amount_minor)
        .bind(refund_status_name(view.status))
        .bind(&view.requester_principal_ref)
        .bind(&view.reviewer_principal_ref)
        .bind(lines)
        .bind(state)
        .bind(view.requested_at)
        .bind(view.updated_at)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        if result.rows_affected() != 1 {
            return Err(ProjectionFailure::new(
                "POS refund snapshot is duplicate or out of order",
            ));
        }
        Ok(())
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query(
            "DELETE FROM erpx_pos_read.refund_projection
             WHERE projection_name=$1 AND generation=$2",
        )
        .bind(REFUND_PROJECTION_NAME)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        Ok(())
    }
}

fn validate_sale_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<SaleView, ProjectionFailure> {
    if context.projection != SALE_PROJECTION_NAME
        || event.context != CONTEXT_NAME
        || event.aggregate_type != SALE_AGGREGATE_TYPE
        || event.event_type != SALE_SNAPSHOT_EVENT_TYPE
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "POS sale projector received an incompatible event contract",
        ));
    }
    let payload: PosOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    let PosOutboxPayload::SaleSnapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "POS sale snapshot event carried a different payload",
        ));
    };
    let view = snapshot.sale;
    if view.version != event.aggregate_version
        || view.sale_id.as_uuid() != event.aggregate_id.as_uuid()
        || view.scope != event.active_scope
        || event.partition != sale_partition(view.sale_id)
        || scope_key(&view.scope).is_err()
    {
        return Err(ProjectionFailure::new(
            "POS sale snapshot metadata does not match its envelope",
        ));
    }
    Ok(view)
}

fn validate_refund_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<RefundView, ProjectionFailure> {
    if context.projection != REFUND_PROJECTION_NAME
        || event.context != CONTEXT_NAME
        || event.aggregate_type != REFUND_AGGREGATE_TYPE
        || event.event_type != REFUND_SNAPSHOT_EVENT_TYPE
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "POS refund projector received an incompatible event contract",
        ));
    }
    let payload: PosOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    let PosOutboxPayload::RefundSnapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "POS refund snapshot event carried a different payload",
        ));
    };
    let view = snapshot.refund;
    if view.version != event.aggregate_version
        || view.refund_id.as_uuid() != event.aggregate_id.as_uuid()
        || view.scope != event.active_scope
        || event.partition != refund_partition(view.refund_id)
        || scope_key(&view.scope).is_err()
    {
        return Err(ProjectionFailure::new(
            "POS refund snapshot metadata does not match its envelope",
        ));
    }
    Ok(view)
}

pub(crate) fn scope_key(scope: &AuthorityScope) -> Result<String, ProjectionFailure> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("holding:{holding_id}/company:{company_id}")),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(format!(
            "holding:{holding_id}/company:{company_id}/branch:{branch_id}"
        )),
        _ => Err(ProjectionFailure::new(
            "POS snapshot uses a non-exact scope",
        )),
    }
}

pub(crate) const fn sale_status_name(status: SaleStatus) -> &'static str {
    match status {
        SaleStatus::Draft => "draft",
        SaleStatus::Open => "open",
        SaleStatus::Held => "held",
        SaleStatus::Completed => "completed",
        SaleStatus::Voided => "voided",
    }
}

pub(crate) const fn refund_status_name(status: RefundStatus) -> &'static str {
    match status {
        RefundStatus::Requested => "requested",
        RefundStatus::Approved => "approved",
        RefundStatus::Rejected => "rejected",
        RefundStatus::Posted => "posted",
        RefundStatus::Cancelled => "cancelled",
    }
}

pub(crate) const fn payment_method_name(method: PaymentMethod) -> &'static str {
    match method {
        PaymentMethod::Cash => "cash",
        PaymentMethod::Card => "card",
        PaymentMethod::BankTransfer => "bank_transfer",
        PaymentMethod::MobileWallet => "mobile_wallet",
        PaymentMethod::Voucher => "voucher",
        PaymentMethod::Other => "other",
    }
}

fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(|_| ProjectionFailure::new("POS numeric value is too large"))
}

fn failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use erpx_cqrs_sqlx::Projector;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn projectors_consume_only_their_private_snapshot_events() {
        let sales = PosSaleProjector::new();
        let refunds = PosRefundProjector::new();
        assert_eq!(sales.name(), SALE_PROJECTION_NAME);
        assert_eq!(sales.event_types(), &[SALE_SNAPSHOT_EVENT_TYPE.to_owned()]);
        assert_eq!(refunds.name(), REFUND_PROJECTION_NAME);
        assert_eq!(
            refunds.event_types(),
            &[REFUND_SNAPSHOT_EVENT_TYPE.to_owned()]
        );
    }

    #[test]
    fn status_method_and_scope_names_match_frozen_ddl() {
        assert_eq!(sale_status_name(SaleStatus::Held), "held");
        assert_eq!(refund_status_name(RefundStatus::Posted), "posted");
        assert_eq!(
            payment_method_name(PaymentMethod::MobileWallet),
            "mobile_wallet"
        );
        let scope = AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        assert_eq!(
            scope_key(&scope).unwrap(),
            format!(
                "holding:{}/company:{}",
                Uuid::from_u128(1),
                Uuid::from_u128(2)
            )
        );
    }

    #[test]
    fn projector_sql_fences_duplicate_and_out_of_order_versions() {
        let source = include_str!("projection.rs");
        assert!(source.contains("source_aggregate_version\n                   < EXCLUDED"));
        assert!(source.contains("duplicate or out of order"));
    }
}
