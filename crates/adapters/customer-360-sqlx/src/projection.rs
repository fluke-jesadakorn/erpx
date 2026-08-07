use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{commercial, crm, customer, sales, support};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    CONTROL_CUSTOMERS_PROJECTION_NAME, CONTROL_PROFILE_EVENT, EVENT_VERSION, OPPORTUNITY_EVENT,
    SALES_ORDER_EVENT, SUBSCRIPTION_EVENT, SUPPORT_CASE_EVENT, TENANT_ACTIVITY_PROJECTION_NAME,
    control_partition,
};

#[derive(Debug, Clone)]
pub struct ControlCustomerProjector {
    events: Vec<String>,
}

impl ControlCustomerProjector {
    pub fn new() -> Self {
        Self {
            events: vec![CONTROL_PROFILE_EVENT.into()],
        }
    }
}

impl Default for ControlCustomerProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct TenantActivityProjector {
    events: Vec<String>,
}

impl TenantActivityProjector {
    pub fn new() -> Self {
        Self {
            events: vec![
                SUBSCRIPTION_EVENT.into(),
                OPPORTUNITY_EVENT.into(),
                SALES_ORDER_EVENT.into(),
                SUPPORT_CASE_EVENT.into(),
            ],
        }
    }
}

impl Default for TenantActivityProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for ControlCustomerProjector {
    fn name(&self) -> &str {
        CONTROL_CUSTOMERS_PROJECTION_NAME
    }

    fn event_types(&self) -> &[String] {
        &self.events
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let fact = control_fact(context, event)?;
        sqlx::query(
            "INSERT INTO erpx_customer360_read.customer_profile_projection
                (projection_name,generation,partition_key,customer_id,customer_account_id,
                 customer_code,display_name,owning_company_id,customer_status,source_event_id,
                 source_aggregate_version,source_global_sequence,occurred_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,now())
             ON CONFLICT (projection_name,generation,partition_key,customer_id)
             DO UPDATE SET customer_account_id=EXCLUDED.customer_account_id,
                           customer_code=EXCLUDED.customer_code,
                           display_name=EXCLUDED.display_name,
                           owning_company_id=EXCLUDED.owning_company_id,
                           customer_status=EXCLUDED.customer_status,
                           source_event_id=EXCLUDED.source_event_id,
                           source_aggregate_version=EXCLUDED.source_aggregate_version,
                           source_global_sequence=EXCLUDED.source_global_sequence,
                           occurred_at=EXCLUDED.occurred_at,
                           projected_at=now()
             WHERE erpx_customer360_read.customer_profile_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(CONTROL_CUSTOMERS_PROJECTION_NAME)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(fact.customer.as_uuid())
        .bind(fact.account.as_uuid())
        .bind(fact.customer_code.as_str())
        .bind(fact.display_name.as_str())
        .bind(fact.owning_company.as_uuid())
        .bind(customer_status_name(fact.status))
        .bind(event.event_id.as_uuid())
        .bind(to_i64(event.aggregate_version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(event.occurred_at)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        Ok(())
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset_table(
            transaction,
            context,
            "erpx_customer360_read.customer_profile_projection",
            CONTROL_CUSTOMERS_PROJECTION_NAME,
        )
        .await
    }
}

#[async_trait]
impl Projector for TenantActivityProjector {
    fn name(&self) -> &str {
        TENANT_ACTIVITY_PROJECTION_NAME
    }

    fn event_types(&self) -> &[String] {
        &self.events
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        if context.projection != TENANT_ACTIVITY_PROJECTION_NAME
            || event.event_version != EVENT_VERSION
            || event.aggregate_version == 0
            || event.global_sequence == 0
        {
            return Err(ProjectionFailure::new(
                "Customer 360 tenant projector received incompatible metadata",
            ));
        }
        let qualified = QualifiedSource::parse(&event.partition)?;
        let tenant_scope = serde_json::to_value(&event.tenant_scope).map_err(failure)?;
        let scope_key = scope_key(&event.tenant_scope)?;
        match event.event_type.as_str() {
            SUBSCRIPTION_EVENT => {
                let fact = subscription_fact(event)?;
                sqlx::query("INSERT INTO erpx_customer360_read.subscription_projection (projection_name,generation,source_key,partition_key,tenant_id,tenant_scope,scope_key,customer_id,subscription_id,plan_id,subscription_status,recurring_amount_minor,currency,effective_at,source_event_id,source_aggregate_version,source_global_sequence,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,now()) ON CONFLICT (projection_name,generation,source_key,subscription_id) DO UPDATE SET partition_key=EXCLUDED.partition_key,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,customer_id=EXCLUDED.customer_id,plan_id=EXCLUDED.plan_id,subscription_status=EXCLUDED.subscription_status,recurring_amount_minor=EXCLUDED.recurring_amount_minor,currency=EXCLUDED.currency,effective_at=EXCLUDED.effective_at,source_event_id=EXCLUDED.source_event_id,source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,projected_at=now() WHERE erpx_customer360_read.subscription_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
                    .bind(TENANT_ACTIVITY_PROJECTION_NAME).bind(context.generation).bind(&qualified.source_key).bind(&qualified.partition).bind(qualified.tenant_id).bind(&tenant_scope).bind(&scope_key).bind(fact.customer.as_uuid()).bind(fact.subscription.as_uuid()).bind(fact.plan.as_uuid()).bind(subscription_status_name(fact.status)).bind(fact.recurring_amount.minor_units()).bind(fact.recurring_amount.currency().as_str()).bind(fact.effective_at).bind(event.event_id.as_uuid()).bind(to_i64(event.aggregate_version)?).bind(to_i64(event.global_sequence)?).execute(&mut **transaction).await.map_err(failure)?;
            }
            OPPORTUNITY_EVENT => {
                let fact = opportunity_fact(event)?;
                sqlx::query("INSERT INTO erpx_customer360_read.opportunity_projection (projection_name,generation,source_key,partition_key,tenant_id,tenant_scope,scope_key,customer_id,opportunity_id,opportunity_stage,expected_value_minor,currency,source_event_id,source_aggregate_version,source_global_sequence,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,now()) ON CONFLICT (projection_name,generation,source_key,opportunity_id) DO UPDATE SET partition_key=EXCLUDED.partition_key,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,customer_id=EXCLUDED.customer_id,opportunity_stage=EXCLUDED.opportunity_stage,expected_value_minor=EXCLUDED.expected_value_minor,currency=EXCLUDED.currency,source_event_id=EXCLUDED.source_event_id,source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,projected_at=now() WHERE erpx_customer360_read.opportunity_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
                    .bind(TENANT_ACTIVITY_PROJECTION_NAME).bind(context.generation).bind(&qualified.source_key).bind(&qualified.partition).bind(qualified.tenant_id).bind(&tenant_scope).bind(&scope_key).bind(fact.customer.as_uuid()).bind(fact.opportunity.as_uuid()).bind(opportunity_stage_name(fact.stage)).bind(fact.expected_value.minor_units()).bind(fact.expected_value.currency().as_str()).bind(event.event_id.as_uuid()).bind(to_i64(event.aggregate_version)?).bind(to_i64(event.global_sequence)?).execute(&mut **transaction).await.map_err(failure)?;
            }
            SALES_ORDER_EVENT => {
                let fact = sales_order_fact(event)?;
                let lines = serde_json::to_value(&fact.lines).map_err(failure)?;
                sqlx::query("INSERT INTO erpx_customer360_read.sales_order_projection (projection_name,generation,source_key,partition_key,tenant_id,tenant_scope,scope_key,customer_id,sales_order_id,order_code,order_status,total_minor,currency,lines,source_event_id,source_aggregate_version,source_global_sequence,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,now()) ON CONFLICT (projection_name,generation,source_key,sales_order_id) DO UPDATE SET partition_key=EXCLUDED.partition_key,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,customer_id=EXCLUDED.customer_id,order_code=EXCLUDED.order_code,order_status=EXCLUDED.order_status,total_minor=EXCLUDED.total_minor,currency=EXCLUDED.currency,lines=EXCLUDED.lines,source_event_id=EXCLUDED.source_event_id,source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,projected_at=now() WHERE erpx_customer360_read.sales_order_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
                    .bind(TENANT_ACTIVITY_PROJECTION_NAME).bind(context.generation).bind(&qualified.source_key).bind(&qualified.partition).bind(qualified.tenant_id).bind(&tenant_scope).bind(&scope_key).bind(fact.customer.as_uuid()).bind(fact.sales_order.as_uuid()).bind(fact.order_code.as_str()).bind(sales_order_status_name(fact.status)).bind(fact.total.minor_units()).bind(fact.total.currency().as_str()).bind(lines).bind(event.event_id.as_uuid()).bind(to_i64(event.aggregate_version)?).bind(to_i64(event.global_sequence)?).execute(&mut **transaction).await.map_err(failure)?;
            }
            SUPPORT_CASE_EVENT => {
                let fact = support_case_fact(event)?;
                sqlx::query("INSERT INTO erpx_customer360_read.support_case_projection (projection_name,generation,source_key,partition_key,tenant_id,tenant_scope,scope_key,customer_id,support_case_id,case_code,case_status,category,aggregate_updated_at,source_event_id,source_aggregate_version,source_global_sequence,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,now()) ON CONFLICT (projection_name,generation,source_key,support_case_id) DO UPDATE SET partition_key=EXCLUDED.partition_key,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,customer_id=EXCLUDED.customer_id,case_code=EXCLUDED.case_code,case_status=EXCLUDED.case_status,category=EXCLUDED.category,aggregate_updated_at=EXCLUDED.aggregate_updated_at,source_event_id=EXCLUDED.source_event_id,source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,projected_at=now() WHERE erpx_customer360_read.support_case_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
                    .bind(TENANT_ACTIVITY_PROJECTION_NAME).bind(context.generation).bind(&qualified.source_key).bind(&qualified.partition).bind(qualified.tenant_id).bind(&tenant_scope).bind(&scope_key).bind(fact.customer.as_uuid()).bind(fact.support_case.as_uuid()).bind(fact.case_code.as_str()).bind(support_case_status_name(fact.status)).bind(fact.category.as_str()).bind(fact.updated_at).bind(event.event_id.as_uuid()).bind(to_i64(event.aggregate_version)?).bind(to_i64(event.global_sequence)?).execute(&mut **transaction).await.map_err(failure)?;
            }
            _ => {
                return Err(ProjectionFailure::new(
                    "Customer 360 tenant projector received an unsubscribed event",
                ));
            }
        }
        Ok(())
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        for table in [
            "erpx_customer360_read.support_case_projection",
            "erpx_customer360_read.sales_order_projection",
            "erpx_customer360_read.opportunity_projection",
            "erpx_customer360_read.subscription_projection",
        ] {
            reset_table(transaction, context, table, TENANT_ACTIVITY_PROJECTION_NAME).await?;
        }
        Ok(())
    }
}

fn control_fact(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<customer::CustomerProfileFact, ProjectionFailure> {
    if context.projection != CONTROL_CUSTOMERS_PROJECTION_NAME
        || event.context != "customer_authority"
        || event.aggregate_type != "customer_account"
        || event.event_type != CONTROL_PROFILE_EVENT
        || event.event_version != EVENT_VERSION
        || event.aggregate_version == 0
        || event.global_sequence == 0
    {
        return Err(ProjectionFailure::new(
            "Customer 360 control projector received an incompatible event",
        ));
    }
    let customer::Event::CustomerProfileChanged(fact) =
        serde_json::from_value(event.payload.clone()).map_err(failure)?
    else {
        return Err(ProjectionFailure::new(
            "customer profile event carried another Customer payload",
        ));
    };
    if fact.customer.as_uuid() != event.aggregate_id.as_uuid()
        || event.partition != control_partition(fact.customer)
    {
        return Err(ProjectionFailure::new(
            "customer profile fact does not match its outbox envelope",
        ));
    }
    Ok(fact)
}

fn subscription_fact(
    event: &ProjectionEvent,
) -> Result<commercial::SubscriptionFact, ProjectionFailure> {
    validate_owner(event, "commercial", "subscription", SUBSCRIPTION_EVENT)?;
    let commercial::Event::SubscriptionChanged(fact) =
        serde_json::from_value(event.payload.clone()).map_err(failure)?
    else {
        return Err(ProjectionFailure::new(
            "subscription event carried another Commercial payload",
        ));
    };
    validate_aggregate(event, fact.subscription.as_uuid())?;
    Ok(fact)
}
fn opportunity_fact(event: &ProjectionEvent) -> Result<crm::OpportunityFact, ProjectionFailure> {
    validate_owner(event, "crm", "opportunity", OPPORTUNITY_EVENT)?;
    let crm::Event::OpportunityChanged(fact) =
        serde_json::from_value(event.payload.clone()).map_err(failure)?
    else {
        return Err(ProjectionFailure::new(
            "opportunity event carried another CRM payload",
        ));
    };
    validate_aggregate(event, fact.opportunity.as_uuid())?;
    Ok(fact)
}
fn sales_order_fact(event: &ProjectionEvent) -> Result<sales::SalesOrderFact, ProjectionFailure> {
    validate_owner(event, "sales", "sales_order", SALES_ORDER_EVENT)?;
    let sales::Event::SalesOrderChanged(fact) =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    validate_aggregate(event, fact.sales_order.as_uuid())?;
    if fact.lines.is_empty() {
        return Err(ProjectionFailure::new("sales order fact omitted lines"));
    }
    Ok(fact)
}
fn support_case_fact(
    event: &ProjectionEvent,
) -> Result<support::SupportCaseFact, ProjectionFailure> {
    validate_owner(event, "support", "support_ticket", SUPPORT_CASE_EVENT)?;
    let support::Event::SupportCaseChanged(fact) =
        serde_json::from_value(event.payload.clone()).map_err(failure)?
    else {
        return Err(ProjectionFailure::new(
            "support case event carried another Support payload",
        ));
    };
    validate_aggregate(event, fact.support_case.as_uuid())?;
    Ok(fact)
}

fn validate_owner(
    event: &ProjectionEvent,
    context: &str,
    aggregate: &str,
    event_type: &str,
) -> Result<(), ProjectionFailure> {
    if event.context != context
        || event.aggregate_type != aggregate
        || event.event_type != event_type
    {
        Err(ProjectionFailure::new(
            "tenant activity owner metadata mismatch",
        ))
    } else {
        Ok(())
    }
}
fn validate_aggregate(event: &ProjectionEvent, fact_id: Uuid) -> Result<(), ProjectionFailure> {
    if event.aggregate_id.as_uuid() == fact_id {
        Ok(())
    } else {
        Err(ProjectionFailure::new(
            "tenant activity fact aggregate mismatch",
        ))
    }
}

struct QualifiedSource {
    source_key: String,
    partition: String,
    tenant_id: Uuid,
}
impl QualifiedSource {
    fn parse(value: &str) -> Result<Self, ProjectionFailure> {
        let (source_key, partition) = value.split_once('/').ok_or_else(|| {
            ProjectionFailure::new("tenant activity partition is not source-qualified")
        })?;
        let tenant = source_key
            .strip_prefix("tenant:")
            .ok_or_else(|| ProjectionFailure::new("tenant activity source key is invalid"))?;
        let tenant_id = Uuid::parse_str(tenant)
            .map_err(|_| ProjectionFailure::new("tenant activity tenant id is invalid"))?;
        if tenant_id.is_nil() || partition.trim().is_empty() {
            return Err(ProjectionFailure::new(
                "tenant activity source route is invalid",
            ));
        }
        Ok(Self {
            source_key: source_key.into(),
            partition: partition.into(),
            tenant_id,
        })
    }
}

async fn reset_table(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    table: &str,
    expected: &str,
) -> Result<(), ProjectionFailure> {
    if context.projection != expected {
        return Err(ProjectionFailure::new("projection reset context mismatch"));
    }
    let query = format!("DELETE FROM {table} WHERE projection_name=$1 AND generation=$2");
    sqlx::query(&query)
        .bind(expected)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
    Ok(())
}

fn scope_key(scope: &erpx_kernel::TenantScope) -> Result<String, ProjectionFailure> {
    use erpx_kernel::TenantScope;
    scope
        .validate()
        .map_err(|_| ProjectionFailure::new("tenant activity scope is invalid"))?;
    match scope {
        TenantScope::Platform => Err(ProjectionFailure::new(
            "tenant activity event scope must identify a tenant",
        )),
        TenantScope::Holding { holding_id } => Ok(format!("h:{holding_id}")),
        TenantScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}")),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}/b:{branch_id}")),
        TenantScope::Subtree { .. } => Err(ProjectionFailure::new(
            "tenant activity event scope must be exact",
        )),
    }
}

fn customer_status_name(value: customer::CustomerStatus) -> &'static str {
    match value {
        customer::CustomerStatus::Prospect => "prospect",
        customer::CustomerStatus::Active => "active",
        customer::CustomerStatus::Dormant => "dormant",
        customer::CustomerStatus::Blocked => "blocked",
        customer::CustomerStatus::Closed => "closed",
    }
}
fn subscription_status_name(value: commercial::SubscriptionStatus) -> &'static str {
    match value {
        commercial::SubscriptionStatus::Trial => "trial",
        commercial::SubscriptionStatus::Active => "active",
        commercial::SubscriptionStatus::PastDue => "past_due",
        commercial::SubscriptionStatus::Suspended => "suspended",
        commercial::SubscriptionStatus::Ended => "ended",
    }
}
fn opportunity_stage_name(value: crm::OpportunityStage) -> &'static str {
    match value {
        crm::OpportunityStage::Qualified => "qualified",
        crm::OpportunityStage::Discovery => "discovery",
        crm::OpportunityStage::Proposal => "proposal",
        crm::OpportunityStage::Negotiation => "negotiation",
        crm::OpportunityStage::Won => "won",
        crm::OpportunityStage::Lost => "lost",
    }
}
fn sales_order_status_name(value: sales::SalesOrderStatus) -> &'static str {
    match value {
        sales::SalesOrderStatus::Confirmed => "confirmed",
        sales::SalesOrderStatus::PartiallyFulfilled => "partially_fulfilled",
        sales::SalesOrderStatus::Fulfilled => "fulfilled",
        sales::SalesOrderStatus::Cancelled => "cancelled",
    }
}
fn support_case_status_name(value: support::SupportCaseStatus) -> &'static str {
    match value {
        support::SupportCaseStatus::Open => "open",
        support::SupportCaseStatus::PendingCustomer => "pending_customer",
        support::SupportCaseStatus::PendingInternal => "pending_internal",
        support::SupportCaseStatus::Resolved => "resolved",
        support::SupportCaseStatus::Closed => "closed",
    }
}
fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value)
        .map_err(|_| ProjectionFailure::new("projection number exceeds PostgreSQL range"))
}
fn failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registrations_are_exact_and_public_fact_only() {
        assert_eq!(
            ControlCustomerProjector::new().event_types(),
            &[CONTROL_PROFILE_EVENT.to_owned()]
        );
        assert_eq!(
            TenantActivityProjector::new().event_types(),
            &[
                SUBSCRIPTION_EVENT.to_owned(),
                OPPORTUNITY_EVENT.to_owned(),
                SALES_ORDER_EVENT.to_owned(),
                SUPPORT_CASE_EVENT.to_owned()
            ]
        );
    }
    #[test]
    fn qualified_sources_reject_cross_database_cursor_ambiguity() {
        assert!(QualifiedSource::parse("commercial.subscription:x").is_err());
        assert!(QualifiedSource::parse("tenant:not-a-uuid/commercial.subscription:x").is_err());
        let value = QualifiedSource::parse(
            "tenant:00000000-0000-0000-0000-000000000001/commercial.subscription:x",
        )
        .unwrap();
        assert_eq!(
            value.source_key,
            "tenant:00000000-0000-0000-0000-000000000001"
        );
    }
}
