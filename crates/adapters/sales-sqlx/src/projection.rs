use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{ItemDescriptorDraft, sales};
use erpx_sales::{
    QuoteStatus, QuoteView, RevenueArrangementStatus, RevenueArrangementView, SalesAggregate,
    SalesOrderStatus, SalesOrderView,
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    ARRANGEMENT_AGGREGATE_TYPE, ARRANGEMENT_PROJECTION, ARRANGEMENT_SNAPSHOT_EVENT, CONTEXT_NAME,
    EVENT_VERSION, ORDER_AGGREGATE_TYPE, ORDER_PROJECTION, ORDER_SNAPSHOT_EVENT,
    QUOTE_AGGREGATE_TYPE, QUOTE_PROJECTION, QUOTE_SNAPSHOT_EVENT, arrangement_partition,
    command::scope_key, order_partition, quote_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum SalesSnapshot {
    Quote(QuoteView),
    SalesOrder(SalesOrderView),
    RevenueArrangement(Box<RevenueArrangementView>),
}

impl From<&SalesAggregate> for SalesSnapshot {
    fn from(value: &SalesAggregate) -> Self {
        match value {
            SalesAggregate::Quote(value) => Self::Quote(value.into()),
            SalesAggregate::SalesOrder(value) => Self::SalesOrder(value.into()),
            SalesAggregate::RevenueArrangement(value) => {
                Self::RevenueArrangement(Box::new(value.as_ref().into()))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SalesOutboxPayload {
    Integration(sales::Event),
    Snapshot(SalesSnapshot),
    Directory(ItemDescriptorDraft),
}

#[derive(Debug, Clone)]
pub struct SalesQuoteProjector {
    event_types: Vec<String>,
}

impl SalesQuoteProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![QUOTE_SNAPSHOT_EVENT.into()],
        }
    }
}

impl Default for SalesQuoteProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for SalesQuoteProjector {
    fn name(&self) -> &str {
        QUOTE_PROJECTION
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
        let snapshot = validate_event(
            context,
            event,
            QUOTE_PROJECTION,
            QUOTE_AGGREGATE_TYPE,
            QUOTE_SNAPSHOT_EVENT,
        )?;
        let SalesSnapshot::Quote(view) = snapshot else {
            return Err(ProjectionFailure::new(
                "Sales quote projector received an order snapshot",
            ));
        };
        let state = serde_json::to_value(&view).map_err(failure)?;
        let lines = serde_json::to_value(&view.lines).map_err(failure)?;
        let result = sqlx::query(
            "INSERT INTO erpx_sales_read.quote_projection
              (projection_name,generation,partition_key,quote_id,source_aggregate_version,
               source_global_sequence,tenant_scope,scope_key,opportunity_id,customer_id,
               quote_code,currency,total_minor,valid_until,quote_status,lines,state,
               aggregate_created_at,aggregate_updated_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,now())
             ON CONFLICT (tenant_id,projection_name,generation,partition_key,quote_id) DO UPDATE SET
               source_aggregate_version=EXCLUDED.source_aggregate_version,
               source_global_sequence=EXCLUDED.source_global_sequence,
               tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,
               opportunity_id=EXCLUDED.opportunity_id,customer_id=EXCLUDED.customer_id,
               quote_code=EXCLUDED.quote_code,currency=EXCLUDED.currency,
               total_minor=EXCLUDED.total_minor,valid_until=EXCLUDED.valid_until,
               quote_status=EXCLUDED.quote_status,lines=EXCLUDED.lines,state=EXCLUDED.state,
               aggregate_created_at=EXCLUDED.aggregate_created_at,
               aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
             WHERE erpx_sales_read.quote_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(QUOTE_PROJECTION)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(view.quote_id.as_uuid())
        .bind(to_i64(view.version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(serde_json::to_value(&view.scope).map_err(failure)?)
        .bind(scope_key(&view.scope).map_err(failure)?)
        .bind(view.opportunity.as_uuid())
        .bind(view.customer.as_uuid())
        .bind(&view.quote_code)
        .bind(&view.currency)
        .bind(view.total_minor)
        .bind(view.valid_until)
        .bind(quote_status_name(view.status))
        .bind(lines)
        .bind(state)
        .bind(view.created_at)
        .bind(view.updated_at)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        if result.rows_affected() != 1 {
            return Err(ProjectionFailure::new(
                "Sales quote snapshot is duplicate or out of order",
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
            "DELETE FROM erpx_sales_read.quote_projection
             WHERE projection_name=$1 AND generation=$2",
        )
        .bind(QUOTE_PROJECTION)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SalesOrderProjector {
    event_types: Vec<String>,
}

impl SalesOrderProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![ORDER_SNAPSHOT_EVENT.into()],
        }
    }
}

impl Default for SalesOrderProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for SalesOrderProjector {
    fn name(&self) -> &str {
        ORDER_PROJECTION
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
        let snapshot = validate_event(
            context,
            event,
            ORDER_PROJECTION,
            ORDER_AGGREGATE_TYPE,
            ORDER_SNAPSHOT_EVENT,
        )?;
        let SalesSnapshot::SalesOrder(view) = snapshot else {
            return Err(ProjectionFailure::new(
                "Sales order projector received a quote snapshot",
            ));
        };
        let state = serde_json::to_value(&view).map_err(failure)?;
        let lines = serde_json::to_value(&view.lines).map_err(failure)?;
        let result = sqlx::query(
            "INSERT INTO erpx_sales_read.sales_order_projection
              (projection_name,generation,partition_key,sales_order_id,source_aggregate_version,
               source_global_sequence,tenant_scope,scope_key,quote_id,opportunity_id,customer_id,
               order_code,currency,total_minor,order_status,lines,state,
               aggregate_created_at,aggregate_updated_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,now())
             ON CONFLICT (tenant_id,projection_name,generation,partition_key,sales_order_id) DO UPDATE SET
               source_aggregate_version=EXCLUDED.source_aggregate_version,
               source_global_sequence=EXCLUDED.source_global_sequence,
               tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,
               quote_id=EXCLUDED.quote_id,opportunity_id=EXCLUDED.opportunity_id,
               customer_id=EXCLUDED.customer_id,order_code=EXCLUDED.order_code,
               currency=EXCLUDED.currency,total_minor=EXCLUDED.total_minor,
               order_status=EXCLUDED.order_status,lines=EXCLUDED.lines,state=EXCLUDED.state,
               aggregate_created_at=EXCLUDED.aggregate_created_at,
               aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
             WHERE erpx_sales_read.sales_order_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(ORDER_PROJECTION)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(view.sales_order_id.as_uuid())
        .bind(to_i64(view.version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(serde_json::to_value(&view.scope).map_err(failure)?)
        .bind(scope_key(&view.scope).map_err(failure)?)
        .bind(view.quote_id.as_uuid())
        .bind(view.opportunity.as_uuid())
        .bind(view.customer.as_uuid())
        .bind(&view.order_code)
        .bind(&view.currency)
        .bind(view.total_minor)
        .bind(order_status_name(view.status))
        .bind(lines)
        .bind(state)
        .bind(view.created_at)
        .bind(view.updated_at)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        if result.rows_affected() != 1 {
            return Err(ProjectionFailure::new(
                "Sales order snapshot is duplicate or out of order",
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
            "DELETE FROM erpx_sales_read.sales_order_projection
             WHERE projection_name=$1 AND generation=$2",
        )
        .bind(ORDER_PROJECTION)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SalesRevenueArrangementProjector {
    event_types: Vec<String>,
}

impl SalesRevenueArrangementProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![ARRANGEMENT_SNAPSHOT_EVENT.into()],
        }
    }
}

impl Default for SalesRevenueArrangementProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for SalesRevenueArrangementProjector {
    fn name(&self) -> &str {
        ARRANGEMENT_PROJECTION
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
        let snapshot = validate_event(
            context,
            event,
            ARRANGEMENT_PROJECTION,
            ARRANGEMENT_AGGREGATE_TYPE,
            ARRANGEMENT_SNAPSHOT_EVENT,
        )?;
        let SalesSnapshot::RevenueArrangement(view) = snapshot else {
            return Err(ProjectionFailure::new(
                "Sales revenue-arrangement projector received another snapshot",
            ));
        };
        let state = serde_json::to_value(&view).map_err(failure)?;
        let dimensions = serde_json::to_value(&view.dimensions).map_err(failure)?;
        let obligations = serde_json::to_value(&view.obligations).map_err(failure)?;
        let fulfillments = serde_json::to_value(&view.fulfillments).map_err(failure)?;
        let result = sqlx::query(
            "INSERT INTO erpx_sales_read.revenue_arrangement_projection
              (projection_name,generation,partition_key,arrangement_id,
               source_aggregate_version,source_global_sequence,tenant_scope,scope_key,
               sales_order_id,customer_id,legal_contract_id,order_code,currency,
               arrangement_status,dimensions,obligations,fulfillments,state,
               aggregate_created_at,aggregate_updated_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,now())
             ON CONFLICT (tenant_id,projection_name,generation,partition_key,arrangement_id) DO UPDATE SET
               source_aggregate_version=EXCLUDED.source_aggregate_version,
               source_global_sequence=EXCLUDED.source_global_sequence,
               tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,
               sales_order_id=EXCLUDED.sales_order_id,customer_id=EXCLUDED.customer_id,
               legal_contract_id=EXCLUDED.legal_contract_id,order_code=EXCLUDED.order_code,
               currency=EXCLUDED.currency,arrangement_status=EXCLUDED.arrangement_status,
               dimensions=EXCLUDED.dimensions,obligations=EXCLUDED.obligations,
               fulfillments=EXCLUDED.fulfillments,state=EXCLUDED.state,
               aggregate_created_at=EXCLUDED.aggregate_created_at,
               aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
             WHERE erpx_sales_read.revenue_arrangement_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(ARRANGEMENT_PROJECTION)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(view.arrangement_id.as_uuid())
        .bind(to_i64(view.version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(serde_json::to_value(&view.scope).map_err(failure)?)
        .bind(scope_key(&view.scope).map_err(failure)?)
        .bind(view.sales_order_id.as_uuid())
        .bind(view.customer.as_uuid())
        .bind(view.legal_contract.map(|reference| reference.as_uuid()))
        .bind(&view.order_code)
        .bind(&view.currency)
        .bind(arrangement_status_name(view.status))
        .bind(dimensions)
        .bind(obligations)
        .bind(fulfillments)
        .bind(state)
        .bind(view.created_at)
        .bind(view.updated_at)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        if result.rows_affected() != 1 {
            return Err(ProjectionFailure::new(
                "Sales revenue-arrangement snapshot is duplicate or out of order",
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
            "DELETE FROM erpx_sales_read.revenue_arrangement_projection
             WHERE projection_name=$1 AND generation=$2",
        )
        .bind(ARRANGEMENT_PROJECTION)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        Ok(())
    }
}

fn validate_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    projection: &str,
    aggregate_type: &str,
    event_type: &str,
) -> Result<SalesSnapshot, ProjectionFailure> {
    if context.projection != projection
        || event.context != CONTEXT_NAME
        || event.aggregate_type != aggregate_type
        || event.event_type != event_type
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "Sales projector received an incompatible event",
        ));
    }
    let payload: SalesOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    let SalesOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "Sales snapshot event carried an integration payload",
        ));
    };
    let matches = match &snapshot {
        SalesSnapshot::Quote(value) => {
            aggregate_type == QUOTE_AGGREGATE_TYPE
                && value.version == event.aggregate_version
                && value.quote_id.as_uuid() == event.aggregate_id.as_uuid()
                && value.scope == event.tenant_scope
                && event.partition == quote_partition(value.quote_id)
        }
        SalesSnapshot::SalesOrder(value) => {
            aggregate_type == ORDER_AGGREGATE_TYPE
                && value.version == event.aggregate_version
                && value.sales_order_id.as_uuid() == event.aggregate_id.as_uuid()
                && value.scope == event.tenant_scope
                && event.partition == order_partition(value.sales_order_id)
        }
        SalesSnapshot::RevenueArrangement(value) => {
            aggregate_type == ARRANGEMENT_AGGREGATE_TYPE
                && value.version == event.aggregate_version
                && value.arrangement_id.as_uuid() == event.aggregate_id.as_uuid()
                && value.scope == event.tenant_scope
                && event.partition == arrangement_partition(value.arrangement_id)
        }
    };
    if !matches {
        return Err(ProjectionFailure::new("Sales snapshot metadata mismatch"));
    }
    Ok(snapshot)
}

pub(crate) const fn quote_status_name(value: QuoteStatus) -> &'static str {
    match value {
        QuoteStatus::Draft => "draft",
        QuoteStatus::Sent => "sent",
        QuoteStatus::Accepted => "accepted",
        QuoteStatus::Rejected => "rejected",
        QuoteStatus::Expired => "expired",
    }
}

pub(crate) const fn order_status_name(value: SalesOrderStatus) -> &'static str {
    match value {
        SalesOrderStatus::Draft => "draft",
        SalesOrderStatus::Confirmed => "confirmed",
        SalesOrderStatus::PartiallyFulfilled => "partially_fulfilled",
        SalesOrderStatus::Fulfilled => "fulfilled",
        SalesOrderStatus::Cancelled => "cancelled",
    }
}

pub(crate) const fn arrangement_status_name(value: RevenueArrangementStatus) -> &'static str {
    match value {
        RevenueArrangementStatus::Active => "active",
        RevenueArrangementStatus::Completed => "completed",
        RevenueArrangementStatus::Cancelled => "cancelled",
    }
}

fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(|_| ProjectionFailure::new("Sales numeric value is too large"))
}

fn failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projectors_are_snapshot_specific_and_rebuildable() {
        let quote = SalesQuoteProjector::new();
        let order = SalesOrderProjector::new();
        let arrangement = SalesRevenueArrangementProjector::new();
        assert_eq!(quote.name(), QUOTE_PROJECTION);
        assert_eq!(quote.event_types(), &[QUOTE_SNAPSHOT_EVENT.to_owned()]);
        assert_eq!(order.name(), ORDER_PROJECTION);
        assert_eq!(order.event_types(), &[ORDER_SNAPSHOT_EVENT.to_owned()]);
        assert_eq!(arrangement.name(), ARRANGEMENT_PROJECTION);
        assert_eq!(
            arrangement.event_types(),
            &[ARRANGEMENT_SNAPSHOT_EVENT.to_owned()]
        );
    }

    #[test]
    fn status_and_version_guards_are_stable() {
        assert_eq!(quote_status_name(QuoteStatus::Accepted), "accepted");
        assert_eq!(
            order_status_name(SalesOrderStatus::PartiallyFulfilled),
            "partially_fulfilled"
        );
        let source = include_str!("projection.rs");
        assert!(source.contains(
            "source_aggregate_version\n                   < EXCLUDED.source_aggregate_version"
        ));
        assert!(source.contains("DELETE FROM erpx_sales_read.quote_projection"));
        assert!(source.contains("DELETE FROM erpx_sales_read.sales_order_projection"));
        assert!(source.contains("DELETE FROM erpx_sales_read.revenue_arrangement_projection"));
    }
}
