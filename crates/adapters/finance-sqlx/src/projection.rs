use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_finance::{
    AccountCategory, AccountStatus, CustomerInvoiceStatus, FinanceAggregate, JournalSource,
    JournalStatus, NormalBalance, PeriodStatus, RevenueScheduleStatus, StatutoryTaxKind,
    SubledgerKind, SubledgerStatus,
    ports::{
        ChartView, CustomerInvoiceView, JournalView, PeriodView, RevenueScheduleView, SubledgerView,
    },
};
use erpx_integration_contracts::{ItemDescriptorDraft, finance};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    BANK_TRANSACTION_PROJECTION, CHART_AGGREGATE_TYPE, CHART_PROJECTION, CHART_SNAPSHOT_EVENT,
    CONTEXT_NAME, CUSTOMER_INVOICE_AGGREGATE_TYPE, CUSTOMER_INVOICE_PROJECTION,
    CUSTOMER_INVOICE_SNAPSHOT_EVENT, EVENT_VERSION, JOURNAL_AGGREGATE_TYPE, JOURNAL_PROJECTION,
    JOURNAL_SNAPSHOT_EVENT, PERIOD_AGGREGATE_TYPE, PERIOD_PROJECTION, PERIOD_SNAPSHOT_EVENT,
    REVENUE_SCHEDULE_AGGREGATE_TYPE, REVENUE_SCHEDULE_PROJECTION, REVENUE_SCHEDULE_SNAPSHOT_EVENT,
    STATUTORY_TAX_PROJECTION, SUBLEDGER_AGGREGATE_TYPE, SUBLEDGER_PROJECTION,
    SUBLEDGER_SNAPSHOT_EVENT, TRIAL_BALANCE_PROJECTION, chart_partition,
    customer_invoice_partition, journal_partition, period_partition, revenue_schedule_partition,
    subledger_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum FinanceSnapshot {
    Chart(ChartView),
    Period(PeriodView),
    Journal(JournalView),
    Subledger(SubledgerView),
    CustomerInvoice(CustomerInvoiceView),
    RevenueSchedule(RevenueScheduleView),
}
impl From<&FinanceAggregate> for FinanceSnapshot {
    fn from(v: &FinanceAggregate) -> Self {
        match v {
            FinanceAggregate::Chart(x) => Self::Chart(x.into()),
            FinanceAggregate::Period(x) => Self::Period(x.into()),
            FinanceAggregate::Journal(x) => Self::Journal(x.into()),
            FinanceAggregate::Subledger(x) => Self::Subledger(x.into()),
            FinanceAggregate::CustomerInvoice(x) => Self::CustomerInvoice(x.into()),
            FinanceAggregate::RevenueSchedule(x) => Self::RevenueSchedule(x.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
// This public adapter payload is serialized directly into the outbox. Boxing
// its snapshot variant would change the construction API without reducing the
// serialized record, so keep the stable boundary explicit.
#[allow(clippy::large_enum_variant)]
pub enum FinanceOutboxPayload {
    Integration(finance::Event),
    Snapshot(FinanceSnapshot),
    Directory(ItemDescriptorDraft),
}

macro_rules! projector {
    ($name:ident,$projection:expr,$event:expr,$apply:ident,$reset:ident) => {
        #[derive(Debug, Clone)]
        pub struct $name {
            event_types: Vec<String>,
        }
        impl $name {
            pub fn new() -> Self {
                Self {
                    event_types: vec![$event.into()],
                }
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        #[async_trait]
        impl Projector for $name {
            fn name(&self) -> &str {
                $projection
            }
            fn event_types(&self) -> &[String] {
                &self.event_types
            }
            async fn apply(
                &self,
                tx: &mut Transaction<'_, Postgres>,
                ctx: &ProjectorContext<'_>,
                event: &ProjectionEvent,
            ) -> Result<(), ProjectionFailure> {
                $apply(tx, ctx, event).await
            }
            async fn reset(
                &self,
                tx: &mut Transaction<'_, Postgres>,
                ctx: &ProjectorContext<'_>,
            ) -> Result<(), ProjectionFailure> {
                $reset(tx, ctx).await
            }
        }
    };
}

projector!(
    FinanceChartProjector,
    CHART_PROJECTION,
    CHART_SNAPSHOT_EVENT,
    apply_chart,
    reset_chart
);
projector!(
    FinanceRevenueScheduleProjector,
    REVENUE_SCHEDULE_PROJECTION,
    REVENUE_SCHEDULE_SNAPSHOT_EVENT,
    apply_revenue_schedule,
    reset_revenue_schedule
);
projector!(
    FinancePeriodProjector,
    PERIOD_PROJECTION,
    PERIOD_SNAPSHOT_EVENT,
    apply_period,
    reset_period
);
projector!(
    FinanceJournalProjector,
    JOURNAL_PROJECTION,
    JOURNAL_SNAPSHOT_EVENT,
    apply_journal,
    reset_journal
);
projector!(
    FinanceSubledgerProjector,
    SUBLEDGER_PROJECTION,
    SUBLEDGER_SNAPSHOT_EVENT,
    apply_subledger,
    reset_subledger
);
projector!(
    FinanceCustomerInvoiceProjector,
    CUSTOMER_INVOICE_PROJECTION,
    CUSTOMER_INVOICE_SNAPSHOT_EVENT,
    apply_customer_invoice,
    reset_customer_invoice
);
projector!(
    FinanceTrialBalanceProjector,
    TRIAL_BALANCE_PROJECTION,
    JOURNAL_SNAPSHOT_EVENT,
    apply_trial_balance,
    reset_trial_balance
);
projector!(
    FinanceBankTransactionProjector,
    BANK_TRANSACTION_PROJECTION,
    JOURNAL_SNAPSHOT_EVENT,
    apply_bank,
    reset_bank
);
projector!(
    FinanceStatutoryTaxProjector,
    STATUTORY_TAX_PROJECTION,
    JOURNAL_SNAPSHOT_EVENT,
    apply_tax,
    reset_tax
);

fn snapshot(
    ctx: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    projection: &str,
    kind: &str,
    event_type: &str,
) -> Result<FinanceSnapshot, ProjectionFailure> {
    if ctx.projection != projection
        || event.context != CONTEXT_NAME
        || event.aggregate_type != kind
        || event.event_type != event_type
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "Finance projector received an incompatible event",
        ));
    }
    let payload: FinanceOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    let FinanceOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "Finance snapshot event carried an integration payload",
        ));
    };
    let matches = match &snapshot {
        FinanceSnapshot::Chart(v) => {
            kind == CHART_AGGREGATE_TYPE
                && v.version == event.aggregate_version
                && v.chart_id.as_uuid() == event.aggregate_id.as_uuid()
                && v.scope == event.tenant_scope
                && event.partition == chart_partition(v.chart_id)
        }
        FinanceSnapshot::Period(v) => {
            kind == PERIOD_AGGREGATE_TYPE
                && v.version == event.aggregate_version
                && v.period_id.as_uuid() == event.aggregate_id.as_uuid()
                && v.scope == event.tenant_scope
                && event.partition == period_partition(v.period_id)
        }
        FinanceSnapshot::Journal(v) => {
            kind == JOURNAL_AGGREGATE_TYPE
                && v.version == event.aggregate_version
                && v.journal_id.as_uuid() == event.aggregate_id.as_uuid()
                && v.scope == event.tenant_scope
                && event.partition == journal_partition(v.journal_id)
        }
        FinanceSnapshot::Subledger(v) => {
            kind == SUBLEDGER_AGGREGATE_TYPE
                && v.version == event.aggregate_version
                && v.subledger_id.as_uuid() == event.aggregate_id.as_uuid()
                && v.scope == event.tenant_scope
                && event.partition == subledger_partition(v.subledger_id)
        }
        FinanceSnapshot::CustomerInvoice(v) => {
            kind == CUSTOMER_INVOICE_AGGREGATE_TYPE
                && v.version == event.aggregate_version
                && v.invoice_id.as_uuid() == event.aggregate_id.as_uuid()
                && v.scope == event.tenant_scope
                && event.partition == customer_invoice_partition(v.invoice_id)
        }
        FinanceSnapshot::RevenueSchedule(v) => {
            kind == REVENUE_SCHEDULE_AGGREGATE_TYPE
                && v.version == event.aggregate_version
                && v.schedule_id.as_uuid() == event.aggregate_id.as_uuid()
                && v.scope == event.tenant_scope
                && event.partition == revenue_schedule_partition(v.schedule_id)
        }
    };
    if !matches {
        return Err(ProjectionFailure::new("Finance snapshot metadata mismatch"));
    }
    Ok(snapshot)
}

async fn apply_chart(
    tx: &mut Transaction<'_, Postgres>,
    ctx: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<(), ProjectionFailure> {
    let FinanceSnapshot::Chart(v) = snapshot(
        ctx,
        event,
        CHART_PROJECTION,
        CHART_AGGREGATE_TYPE,
        CHART_SNAPSHOT_EVENT,
    )?
    else {
        return Err(ProjectionFailure::new("Finance chart snapshot mismatch"));
    };
    let state = serde_json::to_value(&v).map_err(failure)?;
    let accounts = serde_json::to_value(&v.accounts).map_err(failure)?;
    changed(sqlx::query("INSERT INTO erpx_finance_read.chart_projection (projection_name,generation,partition_key,chart_id,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,chart_code,chart_name,currency,accounts,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,now()) ON CONFLICT (tenant_id,projection_name,generation,partition_key,chart_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,chart_code=EXCLUDED.chart_code,chart_name=EXCLUDED.chart_name,currency=EXCLUDED.currency,accounts=EXCLUDED.accounts,state=EXCLUDED.state,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_finance_read.chart_projection.source_aggregate_version<EXCLUDED.source_aggregate_version").bind(CHART_PROJECTION).bind(ctx.generation).bind(&event.partition).bind(v.chart_id.as_uuid()).bind(i64v(v.version)?).bind(i64v(event.global_sequence)?).bind(serde_json::to_value(&v.scope).map_err(failure)?).bind(scope_key(&v.scope)?).bind(&v.code).bind(&v.name).bind(&v.currency).bind(accounts).bind(state).bind(v.created_at).bind(v.updated_at).execute(&mut **tx).await.map_err(failure)?,"Finance chart snapshot is duplicate or out of order")
}

async fn apply_period(
    tx: &mut Transaction<'_, Postgres>,
    ctx: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<(), ProjectionFailure> {
    let FinanceSnapshot::Period(v) = snapshot(
        ctx,
        event,
        PERIOD_PROJECTION,
        PERIOD_AGGREGATE_TYPE,
        PERIOD_SNAPSHOT_EVENT,
    )?
    else {
        return Err(ProjectionFailure::new("Finance period snapshot mismatch"));
    };
    let state = serde_json::to_value(&v).map_err(failure)?;
    changed(sqlx::query("INSERT INTO erpx_finance_read.period_projection (projection_name,generation,partition_key,period_id,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,period_code,starts_on,ends_on,period_status,close_requested_by,closed_by,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,now()) ON CONFLICT (tenant_id,projection_name,generation,partition_key,period_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,period_code=EXCLUDED.period_code,starts_on=EXCLUDED.starts_on,ends_on=EXCLUDED.ends_on,period_status=EXCLUDED.period_status,close_requested_by=EXCLUDED.close_requested_by,closed_by=EXCLUDED.closed_by,state=EXCLUDED.state,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_finance_read.period_projection.source_aggregate_version<EXCLUDED.source_aggregate_version").bind(PERIOD_PROJECTION).bind(ctx.generation).bind(&event.partition).bind(v.period_id.as_uuid()).bind(i64v(v.version)?).bind(i64v(event.global_sequence)?).bind(serde_json::to_value(&v.scope).map_err(failure)?).bind(scope_key(&v.scope)?).bind(&v.code).bind(v.starts_on).bind(v.ends_on).bind(period_status_name(v.status)).bind(&v.close_requested_by).bind(&v.closed_by).bind(state).bind(v.created_at).bind(v.updated_at).execute(&mut **tx).await.map_err(failure)?,"Finance period snapshot is duplicate or out of order")
}

async fn apply_journal(
    tx: &mut Transaction<'_, Postgres>,
    ctx: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<(), ProjectionFailure> {
    let FinanceSnapshot::Journal(v) = snapshot(
        ctx,
        event,
        JOURNAL_PROJECTION,
        JOURNAL_AGGREGATE_TYPE,
        JOURNAL_SNAPSHOT_EVENT,
    )?
    else {
        return Err(ProjectionFailure::new("Finance journal snapshot mismatch"));
    };
    let (kind, _) = journal_source_identity(&v.source).map_err(failure)?;
    let state = serde_json::to_value(&v).map_err(failure)?;
    changed(sqlx::query("INSERT INTO erpx_finance_read.journal_projection (projection_name,generation,partition_key,journal_id,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,chart_id,period_id,journal_code,entry_date,currency,source_kind,source,description,lines,debit_total_minor,credit_total_minor,journal_status,posted_at,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,now()) ON CONFLICT (tenant_id,projection_name,generation,partition_key,journal_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,chart_id=EXCLUDED.chart_id,period_id=EXCLUDED.period_id,journal_code=EXCLUDED.journal_code,entry_date=EXCLUDED.entry_date,currency=EXCLUDED.currency,source_kind=EXCLUDED.source_kind,source=EXCLUDED.source,description=EXCLUDED.description,lines=EXCLUDED.lines,debit_total_minor=EXCLUDED.debit_total_minor,credit_total_minor=EXCLUDED.credit_total_minor,journal_status=EXCLUDED.journal_status,posted_at=EXCLUDED.posted_at,state=EXCLUDED.state,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_finance_read.journal_projection.source_aggregate_version<EXCLUDED.source_aggregate_version").bind(JOURNAL_PROJECTION).bind(ctx.generation).bind(&event.partition).bind(v.journal_id.as_uuid()).bind(i64v(v.version)?).bind(i64v(event.global_sequence)?).bind(serde_json::to_value(&v.scope).map_err(failure)?).bind(scope_key(&v.scope)?).bind(v.chart_id.as_uuid()).bind(v.period_id.as_uuid()).bind(&v.journal_code).bind(v.entry_date).bind(&v.currency).bind(kind).bind(serde_json::to_value(&v.source).map_err(failure)?).bind(&v.description).bind(serde_json::to_value(&v.lines).map_err(failure)?).bind(v.debit_total_minor).bind(v.credit_total_minor).bind(journal_status_name(v.status)).bind(v.posted_at).bind(state).bind(v.created_at).bind(v.updated_at).execute(&mut **tx).await.map_err(failure)?,"Finance journal snapshot is duplicate or out of order")
}

async fn apply_subledger(
    tx: &mut Transaction<'_, Postgres>,
    ctx: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<(), ProjectionFailure> {
    let FinanceSnapshot::Subledger(v) = snapshot(
        ctx,
        event,
        SUBLEDGER_PROJECTION,
        SUBLEDGER_AGGREGATE_TYPE,
        SUBLEDGER_SNAPSHOT_EVENT,
    )?
    else {
        return Err(ProjectionFailure::new(
            "Finance subledger snapshot mismatch",
        ));
    };
    let (kind, party) = subledger_identity(&v.kind);
    let state = serde_json::to_value(&v).map_err(failure)?;
    changed(sqlx::query("INSERT INTO erpx_finance_read.subledger_projection (projection_name,generation,partition_key,subledger_id,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,subledger_kind,party_id,document_code,source_reference,currency,original_minor,outstanding_minor,subledger_status,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,now()) ON CONFLICT (tenant_id,projection_name,generation,partition_key,subledger_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,subledger_kind=EXCLUDED.subledger_kind,party_id=EXCLUDED.party_id,document_code=EXCLUDED.document_code,source_reference=EXCLUDED.source_reference,currency=EXCLUDED.currency,original_minor=EXCLUDED.original_minor,outstanding_minor=EXCLUDED.outstanding_minor,subledger_status=EXCLUDED.subledger_status,state=EXCLUDED.state,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_finance_read.subledger_projection.source_aggregate_version<EXCLUDED.source_aggregate_version").bind(SUBLEDGER_PROJECTION).bind(ctx.generation).bind(&event.partition).bind(v.subledger_id.as_uuid()).bind(i64v(v.version)?).bind(i64v(event.global_sequence)?).bind(serde_json::to_value(&v.scope).map_err(failure)?).bind(scope_key(&v.scope)?).bind(kind).bind(party).bind(&v.document_code).bind(&v.source_reference).bind(&v.currency).bind(v.original_minor).bind(v.outstanding_minor).bind(subledger_status_name(v.status)).bind(state).bind(v.created_at).bind(v.updated_at).execute(&mut **tx).await.map_err(failure)?,"Finance subledger snapshot is duplicate or out of order")
}

async fn apply_customer_invoice(
    tx: &mut Transaction<'_, Postgres>,
    ctx: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<(), ProjectionFailure> {
    let FinanceSnapshot::CustomerInvoice(v) = snapshot(
        ctx,
        event,
        CUSTOMER_INVOICE_PROJECTION,
        CUSTOMER_INVOICE_AGGREGATE_TYPE,
        CUSTOMER_INVOICE_SNAPSHOT_EVENT,
    )?
    else {
        return Err(ProjectionFailure::new(
            "Finance customer invoice snapshot mismatch",
        ));
    };
    let state = serde_json::to_value(&v).map_err(failure)?;
    changed(
        sqlx::query(
            "INSERT INTO erpx_finance_read.customer_invoice_projection
              (projection_name,generation,partition_key,invoice_id,source_aggregate_version,
               source_global_sequence,tenant_scope,scope_key,invoice_code,customer_id,
               billing_event_id,currency,receivable_minor,due_on,invoice_status,state,
               aggregate_created_at,aggregate_updated_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,now())
             ON CONFLICT (tenant_id,projection_name,generation,partition_key,invoice_id) DO UPDATE SET
               source_aggregate_version=EXCLUDED.source_aggregate_version,
               source_global_sequence=EXCLUDED.source_global_sequence,
               tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,
               invoice_code=EXCLUDED.invoice_code,customer_id=EXCLUDED.customer_id,
               billing_event_id=EXCLUDED.billing_event_id,currency=EXCLUDED.currency,
               receivable_minor=EXCLUDED.receivable_minor,due_on=EXCLUDED.due_on,
               invoice_status=EXCLUDED.invoice_status,state=EXCLUDED.state,
               aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
             WHERE erpx_finance_read.customer_invoice_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(CUSTOMER_INVOICE_PROJECTION)
        .bind(ctx.generation)
        .bind(&event.partition)
        .bind(v.invoice_id.as_uuid())
        .bind(i64v(v.version)?)
        .bind(i64v(event.global_sequence)?)
        .bind(serde_json::to_value(&v.scope).map_err(failure)?)
        .bind(scope_key(&v.scope)?)
        .bind(&v.invoice_code)
        .bind(v.billing.customer.as_uuid())
        .bind(v.billing.billing_event.as_uuid())
        .bind(v.billing.billable_amount.currency().as_str())
        .bind(v.receivable_minor)
        .bind(v.due_on)
        .bind(customer_invoice_status_name(v.status))
        .bind(state)
        .bind(v.created_at)
        .bind(v.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(failure)?,
        "Finance customer invoice snapshot is duplicate or out of order",
    )
}

async fn apply_revenue_schedule(
    tx: &mut Transaction<'_, Postgres>,
    ctx: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<(), ProjectionFailure> {
    let FinanceSnapshot::RevenueSchedule(v) = snapshot(
        ctx,
        event,
        REVENUE_SCHEDULE_PROJECTION,
        REVENUE_SCHEDULE_AGGREGATE_TYPE,
        REVENUE_SCHEDULE_SNAPSHOT_EVENT,
    )?
    else {
        return Err(ProjectionFailure::new(
            "Finance revenue schedule snapshot mismatch",
        ));
    };
    let allocated_minor = v.obligations.iter().try_fold(0_i64, |total, value| {
        total
            .checked_add(value.allocated_minor())
            .ok_or_else(|| ProjectionFailure::new("Finance revenue allocation overflow"))
    })?;
    let state = serde_json::to_value(&v).map_err(failure)?;
    changed(
        sqlx::query(
            "INSERT INTO erpx_finance_read.revenue_schedule_projection
              (projection_name,generation,partition_key,schedule_id,source_aggregate_version,
               source_global_sequence,tenant_scope,scope_key,arrangement_id,sales_order_id,
               customer_id,currency,allocated_minor,recognized_minor,schedule_status,state,
               aggregate_created_at,aggregate_updated_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,now())
             ON CONFLICT (tenant_id,projection_name,generation,partition_key,schedule_id) DO UPDATE SET
               source_aggregate_version=EXCLUDED.source_aggregate_version,
               source_global_sequence=EXCLUDED.source_global_sequence,
               tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,
               arrangement_id=EXCLUDED.arrangement_id,sales_order_id=EXCLUDED.sales_order_id,
               customer_id=EXCLUDED.customer_id,currency=EXCLUDED.currency,
               allocated_minor=EXCLUDED.allocated_minor,
               recognized_minor=EXCLUDED.recognized_minor,
               schedule_status=EXCLUDED.schedule_status,state=EXCLUDED.state,
               aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
             WHERE erpx_finance_read.revenue_schedule_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(REVENUE_SCHEDULE_PROJECTION)
        .bind(ctx.generation)
        .bind(&event.partition)
        .bind(v.schedule_id.as_uuid())
        .bind(i64v(v.version)?)
        .bind(i64v(event.global_sequence)?)
        .bind(serde_json::to_value(&v.scope).map_err(failure)?)
        .bind(scope_key(&v.scope)?)
        .bind(v.arrangement.arrangement.as_uuid())
        .bind(v.arrangement.sales_order.as_uuid())
        .bind(v.arrangement.customer.as_uuid())
        .bind(&v.currency)
        .bind(allocated_minor)
        .bind(v.recognized_total_minor)
        .bind(revenue_schedule_status_name(v.status))
        .bind(state)
        .bind(v.created_at)
        .bind(v.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(failure)?,
        "Finance revenue schedule snapshot is duplicate or out of order",
    )
}

async fn apply_trial_balance(
    tx: &mut Transaction<'_, Postgres>,
    ctx: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<(), ProjectionFailure> {
    let FinanceSnapshot::Journal(v) = snapshot(
        ctx,
        event,
        TRIAL_BALANCE_PROJECTION,
        JOURNAL_AGGREGATE_TYPE,
        JOURNAL_SNAPSHOT_EVENT,
    )?
    else {
        return Err(ProjectionFailure::new(
            "Finance trial balance snapshot mismatch",
        ));
    };
    if v.status != JournalStatus::Posted {
        return Ok(());
    }
    let mut totals = std::collections::BTreeMap::new();
    for line in &v.lines {
        let entry = totals.entry(line.account_id).or_insert((0_i64, 0_i64));
        entry.0 = entry
            .0
            .checked_add(line.debit_minor)
            .ok_or_else(|| ProjectionFailure::new("Finance balance overflow"))?;
        entry.1 = entry
            .1
            .checked_add(line.credit_minor)
            .ok_or_else(|| ProjectionFailure::new("Finance balance overflow"))?;
    }
    for (account_id, (debit, credit)) in totals {
        let net = debit
            .checked_sub(credit)
            .ok_or_else(|| ProjectionFailure::new("Finance balance overflow"))?;
        sqlx::query("INSERT INTO erpx_finance_read.trial_balance_projection (projection_name,generation,partition_key,chart_id,account_id,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,currency,account_code,account_name,debit_minor,credit_minor,net_minor,projected_at) SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,account.code,account.name,$11,$12,$13,now() FROM erpx_finance_read.chart_projection chart CROSS JOIN LATERAL jsonb_to_recordset(chart.accounts) AS account(account_id uuid,code text,name text) WHERE chart.projection_name=$14 AND chart.generation=$2 AND chart.chart_id=$4 AND account.account_id=$5 ON CONFLICT (tenant_id,projection_name,generation,partition_key,chart_id,account_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,debit_minor=EXCLUDED.debit_minor,credit_minor=EXCLUDED.credit_minor,net_minor=EXCLUDED.net_minor,projected_at=now() WHERE erpx_finance_read.trial_balance_projection.source_global_sequence<EXCLUDED.source_global_sequence").bind(TRIAL_BALANCE_PROJECTION).bind(ctx.generation).bind(&event.partition).bind(v.chart_id.as_uuid()).bind(account_id.as_uuid()).bind(i64v(v.version)?).bind(i64v(event.global_sequence)?).bind(serde_json::to_value(&v.scope).map_err(failure)?).bind(scope_key(&v.scope)?).bind(&v.currency).bind(debit).bind(credit).bind(net).bind(CHART_PROJECTION).execute(&mut **tx).await.map_err(failure)?;
    }
    Ok(())
}

async fn apply_bank(
    tx: &mut Transaction<'_, Postgres>,
    ctx: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<(), ProjectionFailure> {
    let FinanceSnapshot::Journal(v) = snapshot(
        ctx,
        event,
        BANK_TRANSACTION_PROJECTION,
        JOURNAL_AGGREGATE_TYPE,
        JOURNAL_SNAPSHOT_EVENT,
    )?
    else {
        return Err(ProjectionFailure::new("Finance bank snapshot mismatch"));
    };
    let JournalSource::BankStatement(f) = v.source else {
        return Ok(());
    };
    changed(sqlx::query("INSERT INTO erpx_finance_read.bank_transaction_projection (projection_name,generation,partition_key,transaction_id,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,journal_id,import_id,reference,posted_on,amount_minor,currency,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,now()) ON CONFLICT (tenant_id,projection_name,generation,partition_key,transaction_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,projected_at=now() WHERE erpx_finance_read.bank_transaction_projection.source_aggregate_version<EXCLUDED.source_aggregate_version").bind(BANK_TRANSACTION_PROJECTION).bind(ctx.generation).bind(&event.partition).bind(f.transaction_id).bind(i64v(v.version)?).bind(i64v(event.global_sequence)?).bind(serde_json::to_value(&v.scope).map_err(failure)?).bind(scope_key(&v.scope)?).bind(v.journal_id.as_uuid()).bind(f.import_id).bind(f.reference).bind(f.posted_on).bind(f.amount_minor).bind(f.currency).execute(&mut **tx).await.map_err(failure)?,"Finance bank snapshot is duplicate or out of order")
}

async fn apply_tax(
    tx: &mut Transaction<'_, Postgres>,
    ctx: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<(), ProjectionFailure> {
    let FinanceSnapshot::Journal(v) = snapshot(
        ctx,
        event,
        STATUTORY_TAX_PROJECTION,
        JOURNAL_AGGREGATE_TYPE,
        JOURNAL_SNAPSHOT_EVENT,
    )?
    else {
        return Err(ProjectionFailure::new("Finance tax snapshot mismatch"));
    };
    let JournalSource::TaxAdjustment(f) = v.source else {
        return Ok(());
    };
    changed(sqlx::query("INSERT INTO erpx_finance_read.statutory_tax_projection (projection_name,generation,partition_key,tax_fact_id,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,journal_id,reference,tax_date,tax_period,tax_kind,currency,base_minor,tax_minor,tax_rate_basis_points,tax_identifier,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,now()) ON CONFLICT (tenant_id,projection_name,generation,partition_key,tax_fact_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,projected_at=now() WHERE erpx_finance_read.statutory_tax_projection.source_aggregate_version<EXCLUDED.source_aggregate_version").bind(STATUTORY_TAX_PROJECTION).bind(ctx.generation).bind(&event.partition).bind(f.tax_fact_id).bind(i64v(v.version)?).bind(i64v(event.global_sequence)?).bind(serde_json::to_value(&v.scope).map_err(failure)?).bind(scope_key(&v.scope)?).bind(v.journal_id.as_uuid()).bind(f.reference).bind(f.tax_date).bind(f.tax_period).bind(statutory_tax_kind_name(f.kind)).bind(f.currency).bind(f.base_minor).bind(f.tax_minor).bind(i32::try_from(f.tax_rate_basis_points).map_err(|_|ProjectionFailure::new("Finance tax rate too large"))?).bind(f.tax_identifier).execute(&mut **tx).await.map_err(failure)?,"Finance tax snapshot is duplicate or out of order")
}

macro_rules! reset {
    ($fn:ident,$table:literal,$projection:expr) => {
        async fn $fn(
            tx: &mut Transaction<'_, Postgres>,
            ctx: &ProjectorContext<'_>,
        ) -> Result<(), ProjectionFailure> {
            sqlx::query(concat!(
                "DELETE FROM ",
                $table,
                " WHERE projection_name=$1 AND generation=$2"
            ))
            .bind($projection)
            .bind(ctx.generation)
            .execute(&mut **tx)
            .await
            .map_err(failure)?;
            Ok(())
        }
    };
}
reset!(
    reset_chart,
    "erpx_finance_read.chart_projection",
    CHART_PROJECTION
);
reset!(
    reset_period,
    "erpx_finance_read.period_projection",
    PERIOD_PROJECTION
);
reset!(
    reset_journal,
    "erpx_finance_read.journal_projection",
    JOURNAL_PROJECTION
);
reset!(
    reset_subledger,
    "erpx_finance_read.subledger_projection",
    SUBLEDGER_PROJECTION
);
reset!(
    reset_customer_invoice,
    "erpx_finance_read.customer_invoice_projection",
    CUSTOMER_INVOICE_PROJECTION
);
reset!(
    reset_revenue_schedule,
    "erpx_finance_read.revenue_schedule_projection",
    REVENUE_SCHEDULE_PROJECTION
);
reset!(
    reset_trial_balance,
    "erpx_finance_read.trial_balance_projection",
    TRIAL_BALANCE_PROJECTION
);
reset!(
    reset_bank,
    "erpx_finance_read.bank_transaction_projection",
    BANK_TRANSACTION_PROJECTION
);
reset!(
    reset_tax,
    "erpx_finance_read.statutory_tax_projection",
    STATUTORY_TAX_PROJECTION
);

fn changed(r: sqlx::postgres::PgQueryResult, m: &'static str) -> Result<(), ProjectionFailure> {
    if r.rows_affected() != 1 {
        Err(ProjectionFailure::new(m))
    } else {
        Ok(())
    }
}
pub(crate) const fn account_category_name(v: AccountCategory) -> &'static str {
    match v {
        AccountCategory::Asset => "asset",
        AccountCategory::Liability => "liability",
        AccountCategory::Equity => "equity",
        AccountCategory::Revenue => "revenue",
        AccountCategory::Expense => "expense",
    }
}
pub(crate) const fn normal_balance_name(v: NormalBalance) -> &'static str {
    match v {
        NormalBalance::Debit => "debit",
        NormalBalance::Credit => "credit",
    }
}
pub(crate) const fn account_status_name(v: AccountStatus) -> &'static str {
    match v {
        AccountStatus::Active => "active",
        AccountStatus::Inactive => "inactive",
    }
}
pub(crate) const fn period_status_name(v: PeriodStatus) -> &'static str {
    match v {
        PeriodStatus::Open => "open",
        PeriodStatus::Closing => "closing",
        PeriodStatus::Closed => "closed",
    }
}
pub(crate) const fn journal_status_name(v: JournalStatus) -> &'static str {
    match v {
        JournalStatus::Draft => "draft",
        JournalStatus::Posted => "posted",
    }
}
pub(crate) const fn subledger_status_name(v: SubledgerStatus) -> &'static str {
    match v {
        SubledgerStatus::Open => "open",
        SubledgerStatus::PartiallySettled => "partially_settled",
        SubledgerStatus::Settled => "settled",
    }
}
pub(crate) const fn customer_invoice_status_name(v: CustomerInvoiceStatus) -> &'static str {
    match v {
        CustomerInvoiceStatus::Draft => "draft",
        CustomerInvoiceStatus::Posted => "posted",
        CustomerInvoiceStatus::Cancelled => "cancelled",
    }
}
pub(crate) const fn revenue_schedule_status_name(v: RevenueScheduleStatus) -> &'static str {
    match v {
        RevenueScheduleStatus::Active => "active",
        RevenueScheduleStatus::Completed => "completed",
    }
}
pub(crate) const fn statutory_tax_kind_name(v: StatutoryTaxKind) -> &'static str {
    match v {
        StatutoryTaxKind::OutputVat => "output_vat",
        StatutoryTaxKind::InputVat => "input_vat",
        StatutoryTaxKind::Withholding => "withholding",
    }
}
pub(crate) fn subledger_identity(v: &SubledgerKind) -> (&'static str, Uuid) {
    match v {
        SubledgerKind::Receivable(x) => ("receivable", x.as_uuid()),
        SubledgerKind::Payable(x) => ("payable", x.as_uuid()),
    }
}
pub(crate) fn journal_source_identity(
    v: &JournalSource,
) -> Result<(&'static str, String), ProjectionFailure> {
    Ok(match v {
        JournalSource::SalesOrder(x) => ("sales_order", x.order_code.as_str().into()),
        JournalSource::InventoryMovement(x) => {
            ("inventory_movement", x.movement.as_uuid().to_string())
        }
        JournalSource::Payroll(x) => ("payroll", x.period_code.as_str().into()),
        JournalSource::PosSale(x) => ("pos_sale", x.receipt_code.as_str().into()),
        JournalSource::PosRefund(x) => ("pos_refund", x.refund.as_uuid().to_string()),
        JournalSource::BankStatement(x) => ("bank_statement", x.reference.clone()),
        JournalSource::TaxAdjustment(x) => ("tax_adjustment", x.reference.clone()),
        JournalSource::Manual { reference } => ("manual", reference.clone()),
    })
}
fn scope_key(s: &erpx_kernel::TenantScope) -> Result<String, ProjectionFailure> {
    crate::command::scope_key(s).map_err(failure)
}
fn i64v(v: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(v).map_err(|_| ProjectionFailure::new("Finance numeric value is too large"))
}
fn failure(e: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nine_projectors_are_collision_free() {
        let chart = FinanceChartProjector::new();
        let period = FinancePeriodProjector::new();
        let journal = FinanceJournalProjector::new();
        let subledger = FinanceSubledgerProjector::new();
        let customer_invoice = FinanceCustomerInvoiceProjector::new();
        let revenue_schedule = FinanceRevenueScheduleProjector::new();
        let trial = FinanceTrialBalanceProjector::new();
        let bank = FinanceBankTransactionProjector::new();
        let tax = FinanceStatutoryTaxProjector::new();
        let values: [&dyn Projector; 9] = [
            &chart,
            &period,
            &journal,
            &subledger,
            &customer_invoice,
            &revenue_schedule,
            &trial,
            &bank,
            &tax,
        ];
        let names = values
            .iter()
            .map(|v| v.name())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(names.len(), 9);
        assert!(
            include_str!("projection.rs")
                .contains("source_aggregate_version<EXCLUDED.source_aggregate_version")
        );
    }
}
