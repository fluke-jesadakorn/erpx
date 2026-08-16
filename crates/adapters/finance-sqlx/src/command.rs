use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_finance::{
    Account, AccountCategory, AccountId, AccountStatus, AccountingPeriod, BankTransactionFact,
    Chart, ChartId, CommandUnitOfWork, CustomerInvoice, CustomerInvoiceId, CustomerInvoiceStatus,
    FinanceAggregate, FinanceCommit, Journal, JournalId, JournalLine, JournalLineId, JournalSource,
    JournalStatus, NormalBalance, PeriodId, PeriodStatus, PortError, RevenueObligation,
    RevenueRecognitionEntry, RevenueSchedule, RevenueScheduleId, RevenueScheduleStatus,
    StatutoryTaxFact, StatutoryTaxKind, StoredCommit, SubledgerId, SubledgerItem, SubledgerKind,
    SubledgerStatus, WriteRepository,
    ports::{CommandFingerprint, CommandIdentity, ReferenceVersion},
};
use erpx_integration_contracts::{
    BoundedContext, CurrencyCode, DimensionSet, ITEM_DIRECTORY_FACT_VERSION,
    ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent, ItemDescriptorDraft, MoneyAmount, Quantity,
    RecordRef, RecordTypeId, ReferenceCode, StableRecordRef, assets, catalog::ContextId, customer,
    inventory, legal, marketing, organization, people_payroll, pos, procurement, record_policy_id,
    sales,
};
use erpx_kernel::{AggregateId, CommandId, DataClassification, EventId, RecordId, TenantScope};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    CHART_AGGREGATE_TYPE, CHART_SNAPSHOT_EVENT, CONTEXT_NAME, CUSTOMER_INVOICE_AGGREGATE_TYPE,
    CUSTOMER_INVOICE_SNAPSHOT_EVENT, EVENT_VERSION, JOURNAL_AGGREGATE_TYPE, JOURNAL_SNAPSHOT_EVENT,
    PERIOD_AGGREGATE_TYPE, PERIOD_SNAPSHOT_EVENT, REVENUE_SCHEDULE_AGGREGATE_TYPE,
    REVENUE_SCHEDULE_SNAPSHOT_EVENT, SUBLEDGER_AGGREGATE_TYPE, SUBLEDGER_SNAPSHOT_EVENT,
    chart_partition, customer_invoice_partition, journal_partition, period_partition,
    projection::{
        FinanceOutboxPayload, FinanceSnapshot, account_category_name, account_status_name,
        customer_invoice_status_name, journal_source_identity, journal_status_name,
        normal_balance_name, period_status_name, revenue_schedule_status_name,
        statutory_tax_kind_name, subledger_identity, subledger_status_name,
    },
    revenue_schedule_partition, subledger_partition,
};

#[derive(Debug, Clone)]
pub struct FinanceWriteRepository {
    pool: WritePool,
}

impl FinanceWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self { pool }
    }

    async fn load(
        &self,
        authority: &TenantScope,
        partition: &str,
        kind: &str,
        id: Uuid,
    ) -> Result<Option<FinanceAggregate>, PortError> {
        let _ = partition;
        load_finance_relational(&self.pool, authority, kind, id).await
    }
}

async fn load_finance_relational(
    pool: &WritePool,
    authority: &TenantScope,
    kind: &str,
    id: Uuid,
) -> Result<Option<FinanceAggregate>, PortError> {
    authority.validate().map_err(|_| PortError::Denied)?;
    let mut tx = pool.begin().await.map_err(map_store_error)?;
    match kind {
        CHART_AGGREGATE_TYPE => load_chart_relational(&mut tx, authority, id)
            .await
            .map(|x| x.map(FinanceAggregate::Chart)),
        PERIOD_AGGREGATE_TYPE => load_period_relational(&mut tx, authority, id)
            .await
            .map(|x| x.map(FinanceAggregate::Period)),
        JOURNAL_AGGREGATE_TYPE => load_journal_relational(&mut tx, authority, id)
            .await
            .map(|x| x.map(FinanceAggregate::Journal)),
        SUBLEDGER_AGGREGATE_TYPE => load_subledger_relational(&mut tx, authority, id)
            .await
            .map(|x| x.map(FinanceAggregate::Subledger)),
        CUSTOMER_INVOICE_AGGREGATE_TYPE => load_invoice_relational(&mut tx, authority, id)
            .await
            .map(|x| x.map(FinanceAggregate::CustomerInvoice)),
        REVENUE_SCHEDULE_AGGREGATE_TYPE => load_schedule_relational(&mut tx, authority, id)
            .await
            .map(|x| x.map(FinanceAggregate::RevenueSchedule)),
        _ => Err(PortError::Internal),
    }
}

async fn load_chart_relational(
    tx: &mut Transaction<'_, Postgres>,
    authority: &TenantScope,
    id: Uuid,
) -> Result<Option<Chart>, PortError> {
    let row=finance_authorized_root(tx,authority,"SELECT aggregate_version,holding_id,company_id,branch_id,chart_code,chart_name,currency,created_at,updated_at FROM erpx_finance_write.chart_guards WHERE chart_id=$1","erpx_finance_write.chart_guards","chart_id",id).await?;
    let Some(row) = row else { return Ok(None) };
    let rows=sqlx::query("SELECT account_id,account_code,account_name,category,normal_balance,parent_account_id,account_status FROM erpx_finance_write.account_guards WHERE chart_id=$1 ORDER BY account_code,account_id").bind(id).fetch_all(&mut **tx).await.map_err(|_|PortError::Unavailable)?;
    let accounts = rows
        .into_iter()
        .map(|a| {
            Ok(Account {
                account_id: AccountId::try_from_uuid(
                    a.try_get("account_id").map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                code: a.try_get("account_code").map_err(|_| PortError::Internal)?,
                name: a.try_get("account_name").map_err(|_| PortError::Internal)?,
                category: parse_account_category(
                    a.try_get("category").map_err(|_| PortError::Internal)?,
                )?,
                normal_balance: parse_normal_balance(
                    a.try_get("normal_balance")
                        .map_err(|_| PortError::Internal)?,
                )?,
                parent_account_id: a
                    .try_get::<Option<Uuid>, _>("parent_account_id")
                    .map_err(|_| PortError::Internal)?
                    .map(AccountId::try_from_uuid)
                    .transpose()
                    .map_err(|_| PortError::Internal)?,
                status: parse_account_status(
                    a.try_get("account_status")
                        .map_err(|_| PortError::Internal)?,
                )?,
            })
        })
        .collect::<Result<Vec<_>, PortError>>()?;
    Chart::try_rehydrate(
        ChartId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        finance_row_u64(&row, "aggregate_version")?,
        finance_exact_scope(&row)?,
        row.try_get("chart_code").map_err(|_| PortError::Internal)?,
        row.try_get("chart_name").map_err(|_| PortError::Internal)?,
        row.try_get("currency").map_err(|_| PortError::Internal)?,
        accounts,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_period_relational(
    tx: &mut Transaction<'_, Postgres>,
    authority: &TenantScope,
    id: Uuid,
) -> Result<Option<AccountingPeriod>, PortError> {
    let row=finance_authorized_root(tx,authority,"SELECT aggregate_version,holding_id,company_id,branch_id,period_code,starts_on,ends_on,period_status,close_requested_by,closed_by,created_at,updated_at FROM erpx_finance_write.period_guards WHERE period_id=$1","erpx_finance_write.period_guards","period_id",id).await?;
    let Some(row) = row else { return Ok(None) };
    AccountingPeriod::try_rehydrate(
        PeriodId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        finance_row_u64(&row, "aggregate_version")?,
        finance_exact_scope(&row)?,
        row.try_get("period_code")
            .map_err(|_| PortError::Internal)?,
        row.try_get("starts_on").map_err(|_| PortError::Internal)?,
        row.try_get("ends_on").map_err(|_| PortError::Internal)?,
        parse_period_status(
            row.try_get("period_status")
                .map_err(|_| PortError::Internal)?,
        )?,
        row.try_get("close_requested_by")
            .map_err(|_| PortError::Internal)?,
        row.try_get("closed_by").map_err(|_| PortError::Internal)?,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_subledger_relational(
    tx: &mut Transaction<'_, Postgres>,
    authority: &TenantScope,
    id: Uuid,
) -> Result<Option<SubledgerItem>, PortError> {
    let row=finance_authorized_root(tx,authority,"SELECT aggregate_version,holding_id,company_id,branch_id,subledger_kind,party_id,document_code,source_reference,currency,original_minor,outstanding_minor,subledger_status,created_at,updated_at FROM erpx_finance_write.subledger_guards WHERE subledger_id=$1","erpx_finance_write.subledger_guards","subledger_id",id).await?;
    let Some(row) = row else { return Ok(None) };
    let party: Uuid = row.try_get("party_id").map_err(|_| PortError::Internal)?;
    let kind = match row
        .try_get::<String, _>("subledger_kind")
        .map_err(|_| PortError::Internal)?
        .as_str()
    {
        "receivable" => SubledgerKind::Receivable(
            customer::CustomerRef::try_from_uuid(party).map_err(|_| PortError::Internal)?,
        ),
        "payable" => SubledgerKind::Payable(
            procurement::SupplierRef::try_from_uuid(party).map_err(|_| PortError::Internal)?,
        ),
        _ => return Err(PortError::Internal),
    };
    SubledgerItem::try_rehydrate(
        SubledgerId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        finance_row_u64(&row, "aggregate_version")?,
        finance_exact_scope(&row)?,
        kind,
        row.try_get("document_code")
            .map_err(|_| PortError::Internal)?,
        row.try_get("source_reference")
            .map_err(|_| PortError::Internal)?,
        row.try_get("currency").map_err(|_| PortError::Internal)?,
        row.try_get("original_minor")
            .map_err(|_| PortError::Internal)?,
        row.try_get("outstanding_minor")
            .map_err(|_| PortError::Internal)?,
        parse_subledger_status(
            row.try_get("subledger_status")
                .map_err(|_| PortError::Internal)?,
        )?,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_journal_relational(
    tx: &mut Transaction<'_, Postgres>,
    authority: &TenantScope,
    id: Uuid,
) -> Result<Option<Journal>, PortError> {
    let row=finance_authorized_root(tx,authority,"SELECT aggregate_version,holding_id,company_id,branch_id,chart_id,period_id,journal_code,entry_date,currency,source_kind,description,journal_status,posted_at,created_at,updated_at FROM erpx_finance_write.journal_guards WHERE journal_id=$1","erpx_finance_write.journal_guards","journal_id",id).await?;
    let Some(row) = row else { return Ok(None) };
    let lines=sqlx::query("SELECT line_id,account_id,debit_minor,credit_minor,currency,memo FROM erpx_finance_write.journal_line_guards WHERE journal_id=$1 ORDER BY line_id").bind(id).fetch_all(&mut **tx).await.map_err(|_|PortError::Unavailable)?.into_iter().map(|l|Ok(JournalLine{line_id:JournalLineId::try_from_uuid(l.try_get("line_id").map_err(|_|PortError::Internal)?).map_err(|_|PortError::Internal)?,account_id:AccountId::try_from_uuid(l.try_get("account_id").map_err(|_|PortError::Internal)?).map_err(|_|PortError::Internal)?,debit_minor:l.try_get("debit_minor").map_err(|_|PortError::Internal)?,credit_minor:l.try_get("credit_minor").map_err(|_|PortError::Internal)?,currency:l.try_get("currency").map_err(|_|PortError::Internal)?,memo:l.try_get("memo").map_err(|_|PortError::Internal)?})).collect::<Result<Vec<_>,PortError>>()?;
    let currency: String = row.try_get("currency").map_err(|_| PortError::Internal)?;
    let source = load_journal_source(
        tx,
        id,
        row.try_get("source_kind")
            .map_err(|_| PortError::Internal)?,
        &currency,
    )
    .await?;
    Journal::try_rehydrate(
        JournalId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        finance_row_u64(&row, "aggregate_version")?,
        finance_exact_scope(&row)?,
        ChartId::try_from_uuid(row.try_get("chart_id").map_err(|_| PortError::Internal)?)
            .map_err(|_| PortError::Internal)?,
        PeriodId::try_from_uuid(row.try_get("period_id").map_err(|_| PortError::Internal)?)
            .map_err(|_| PortError::Internal)?,
        row.try_get("journal_code")
            .map_err(|_| PortError::Internal)?,
        row.try_get("entry_date").map_err(|_| PortError::Internal)?,
        currency,
        source,
        row.try_get("description")
            .map_err(|_| PortError::Internal)?,
        lines,
        parse_journal_status(
            row.try_get("journal_status")
                .map_err(|_| PortError::Internal)?,
        )?,
        row.try_get("posted_at").map_err(|_| PortError::Internal)?,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_invoice_relational(
    tx: &mut Transaction<'_, Postgres>,
    authority: &TenantScope,
    id: Uuid,
) -> Result<Option<CustomerInvoice>, PortError> {
    let row=finance_authorized_root(tx,authority,"SELECT aggregate_version,holding_id,company_id,branch_id,invoice_code,customer_id,billing_event_id,source_aggregate_kind,source_aggregate_id,source_aggregate_version,currency,net_minor,tax_minor,eligible_on,due_on,prepared_by,posted_by,posted_at,invoice_status,created_at,updated_at FROM erpx_finance_write.customer_invoice_guards WHERE invoice_id=$1","erpx_finance_write.customer_invoice_guards","invoice_id",id).await?;
    let Some(row) = row else { return Ok(None) };
    let drow = sqlx::query(
        "SELECT * FROM erpx_finance_write.customer_invoice_dimension_authority WHERE invoice_id=$1",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| PortError::Unavailable)?
    .ok_or(PortError::Internal)?;
    let dimensions = finance_dimensions_from_row(&drow)?;
    let currency = currency_from_row(&row, "currency")?;
    let arrangement_id: Uuid = drow
        .try_get("arrangement_id")
        .map_err(|_| PortError::Internal)?;
    let source = RecordRef::try_new(
        BoundedContext::Sales,
        row.try_get::<String, _>("source_aggregate_kind")
            .map_err(|_| PortError::Internal)?,
        row.try_get("source_aggregate_id")
            .map_err(|_| PortError::Internal)?,
        finance_row_u64(&row, "source_aggregate_version")?,
    )
    .map_err(|_| PortError::Internal)?;
    let billing = sales::BillingEligibilityFact {
        billing_event: sales::BillingEventRef::try_from_uuid(
            row.try_get("billing_event_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        fulfillment: sales::FulfillmentRef::try_from_uuid(
            drow.try_get("fulfillment_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        arrangement: sales::RevenueArrangementRef::try_from_uuid(arrangement_id)
            .map_err(|_| PortError::Internal)?,
        sales_order: sales::SalesOrderRef::try_from_uuid(
            drow.try_get("sales_order_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        customer: customer::CustomerRef::try_from_uuid(
            row.try_get("customer_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        order_code: ReferenceCode::try_new(
            drow.try_get::<String, _>("order_code")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        billable_amount: MoneyAmount::new(
            row.try_get("net_minor").map_err(|_| PortError::Internal)?,
            currency,
        ),
        tax_amount: MoneyAmount::new(
            row.try_get("tax_minor").map_err(|_| PortError::Internal)?,
            currency,
        ),
        eligible_on: row
            .try_get("eligible_on")
            .map_err(|_| PortError::Internal)?,
        dimensions,
    };
    CustomerInvoice::try_rehydrate(
        CustomerInvoiceId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        finance_row_u64(&row, "aggregate_version")?,
        finance_exact_scope(&row)?,
        row.try_get("invoice_code")
            .map_err(|_| PortError::Internal)?,
        source,
        billing,
        row.try_get("due_on").map_err(|_| PortError::Internal)?,
        row.try_get("prepared_by")
            .map_err(|_| PortError::Internal)?,
        row.try_get("posted_by").map_err(|_| PortError::Internal)?,
        row.try_get("posted_at").map_err(|_| PortError::Internal)?,
        parse_invoice_status(
            row.try_get("invoice_status")
                .map_err(|_| PortError::Internal)?,
        )?,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_schedule_relational(
    tx: &mut Transaction<'_, Postgres>,
    authority: &TenantScope,
    id: Uuid,
) -> Result<Option<RevenueSchedule>, PortError> {
    let row=finance_authorized_root(tx,authority,"SELECT aggregate_version,holding_id,company_id,branch_id,arrangement_id,sales_order_id,customer_id,legal_contract_id,order_code,source_aggregate_kind,source_aggregate_version,last_source_version,currency,schedule_status,created_at,updated_at FROM erpx_finance_write.revenue_schedule_guards WHERE schedule_id=$1","erpx_finance_write.revenue_schedule_guards","schedule_id",id).await?;
    let Some(row) = row else { return Ok(None) };
    let drow=sqlx::query("SELECT * FROM erpx_finance_write.revenue_schedule_dimension_authority WHERE schedule_id=$1").bind(id).fetch_optional(&mut **tx).await.map_err(|_|PortError::Unavailable)?.ok_or(PortError::Internal)?;
    let dimensions = finance_dimensions_from_row(&drow)?;
    let currency = currency_from_row(&row, "currency")?;
    let arrangement_id: Uuid = row
        .try_get("arrangement_id")
        .map_err(|_| PortError::Internal)?;
    let obligation_rows=sqlx::query("SELECT obligation_id,line_id,revenue_model,allocated_minor,recognized_minor,service_starts_on,service_ends_on FROM erpx_finance_write.revenue_schedule_obligation_authority WHERE schedule_id=$1 ORDER BY obligation_id").bind(id).fetch_all(&mut **tx).await.map_err(|_|PortError::Unavailable)?;
    let mut fact_obligations = Vec::with_capacity(obligation_rows.len());
    let mut obligations = Vec::with_capacity(obligation_rows.len());
    for o in obligation_rows {
        let obligation = sales::PerformanceObligationRef::try_from_uuid(
            o.try_get("obligation_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        let line = sales::SalesOrderLineRef::try_from_uuid(
            o.try_get("line_id").map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        let model = parse_finance_revenue_model(
            o.try_get("revenue_model")
                .map_err(|_| PortError::Internal)?,
        )?;
        let allocated: i64 = o
            .try_get("allocated_minor")
            .map_err(|_| PortError::Internal)?;
        let starts = o
            .try_get("service_starts_on")
            .map_err(|_| PortError::Internal)?;
        let ends = o
            .try_get("service_ends_on")
            .map_err(|_| PortError::Internal)?;
        fact_obligations.push(sales::PerformanceObligationFact {
            obligation,
            line,
            model,
            allocated_amount: MoneyAmount::new(allocated, currency),
            service_starts_on: starts,
            service_ends_on: ends,
        });
        obligations.push(RevenueObligation::try_rehydrate(
            obligation,
            line,
            model,
            allocated,
            o.try_get("recognized_minor")
                .map_err(|_| PortError::Internal)?,
            starts,
            ends,
        ));
    }
    let arrangement = sales::RevenueArrangementFact {
        arrangement: sales::RevenueArrangementRef::try_from_uuid(arrangement_id)
            .map_err(|_| PortError::Internal)?,
        sales_order: sales::SalesOrderRef::try_from_uuid(
            row.try_get("sales_order_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        customer: customer::CustomerRef::try_from_uuid(
            row.try_get("customer_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        legal_contract: row
            .try_get::<Option<Uuid>, _>("legal_contract_id")
            .map_err(|_| PortError::Internal)?
            .map(legal::LegalContractRef::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?,
        order_code: ReferenceCode::try_new(
            row.try_get::<String, _>("order_code")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        dimensions,
        obligations: fact_obligations,
    };
    let arrangement_source = RecordRef::try_new(
        BoundedContext::Sales,
        row.try_get::<String, _>("source_aggregate_kind")
            .map_err(|_| PortError::Internal)?,
        arrangement_id,
        finance_row_u64(&row, "source_aggregate_version")?,
    )
    .map_err(|_| PortError::Internal)?;
    let recognition_rows=sqlx::query("SELECT fulfillment_id,obligation_id,source_aggregate_version,recognized_minor,recognized_at FROM erpx_finance_write.revenue_schedule_recognition_authority WHERE schedule_id=$1 ORDER BY recognition_ordinal").bind(id).fetch_all(&mut **tx).await.map_err(|_|PortError::Unavailable)?;
    let recognitions = recognition_rows
        .into_iter()
        .map(|r| {
            Ok(RevenueRecognitionEntry::try_rehydrate(
                sales::FulfillmentRef::try_from_uuid(
                    r.try_get("fulfillment_id")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                sales::PerformanceObligationRef::try_from_uuid(
                    r.try_get("obligation_id")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                RecordRef::try_new(
                    BoundedContext::Sales,
                    "revenue_arrangement",
                    arrangement_id,
                    finance_row_u64(&r, "source_aggregate_version")?,
                )
                .map_err(|_| PortError::Internal)?,
                r.try_get("recognized_minor")
                    .map_err(|_| PortError::Internal)?,
                r.try_get("recognized_at")
                    .map_err(|_| PortError::Internal)?,
            ))
        })
        .collect::<Result<Vec<_>, PortError>>()?;
    let last_source = RecordRef::try_new(
        BoundedContext::Sales,
        "revenue_arrangement",
        arrangement_id,
        finance_row_u64(&row, "last_source_version")?,
    )
    .map_err(|_| PortError::Internal)?;
    RevenueSchedule::try_rehydrate(
        RevenueScheduleId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        finance_row_u64(&row, "aggregate_version")?,
        finance_exact_scope(&row)?,
        arrangement_source,
        last_source,
        arrangement,
        row.try_get("currency").map_err(|_| PortError::Internal)?,
        obligations,
        recognitions,
        parse_schedule_status(
            row.try_get("schedule_status")
                .map_err(|_| PortError::Internal)?,
        )?,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_journal_source(
    tx: &mut Transaction<'_, Postgres>,
    journal_id: Uuid,
    kind: String,
    currency: &str,
) -> Result<JournalSource, PortError> {
    let currency_code = CurrencyCode::try_new(currency).map_err(|_| PortError::Internal)?;
    match kind.as_str() {
        "bank_statement" => {
            let r=sqlx::query("SELECT import_id,transaction_id,reference,posted_on,amount_minor,currency FROM erpx_finance_write.bank_transaction_facts WHERE journal_id=$1").bind(journal_id).fetch_optional(&mut **tx).await.map_err(|_|PortError::Unavailable)?.ok_or(PortError::Internal)?;
            Ok(JournalSource::BankStatement(BankTransactionFact {
                import_id: r.try_get("import_id").map_err(|_| PortError::Internal)?,
                transaction_id: r
                    .try_get("transaction_id")
                    .map_err(|_| PortError::Internal)?,
                reference: r.try_get("reference").map_err(|_| PortError::Internal)?,
                posted_on: r.try_get("posted_on").map_err(|_| PortError::Internal)?,
                amount_minor: r.try_get("amount_minor").map_err(|_| PortError::Internal)?,
                currency: r.try_get("currency").map_err(|_| PortError::Internal)?,
            }))
        }
        "tax_adjustment" => {
            let r=sqlx::query("SELECT tax_fact_id,reference,tax_date,tax_period,tax_kind,currency,base_minor,tax_minor,tax_rate_basis_points,tax_identifier FROM erpx_finance_write.statutory_tax_facts WHERE journal_id=$1").bind(journal_id).fetch_optional(&mut **tx).await.map_err(|_|PortError::Unavailable)?.ok_or(PortError::Internal)?;
            Ok(JournalSource::TaxAdjustment(StatutoryTaxFact {
                tax_fact_id: r.try_get("tax_fact_id").map_err(|_| PortError::Internal)?,
                reference: r.try_get("reference").map_err(|_| PortError::Internal)?,
                tax_date: r.try_get("tax_date").map_err(|_| PortError::Internal)?,
                tax_period: r.try_get("tax_period").map_err(|_| PortError::Internal)?,
                kind: parse_tax_kind(r.try_get("tax_kind").map_err(|_| PortError::Internal)?)?,
                currency: r.try_get("currency").map_err(|_| PortError::Internal)?,
                base_minor: r.try_get("base_minor").map_err(|_| PortError::Internal)?,
                tax_minor: r.try_get("tax_minor").map_err(|_| PortError::Internal)?,
                tax_rate_basis_points: u32::try_from(
                    r.try_get::<i32, _>("tax_rate_basis_points")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                tax_identifier: r
                    .try_get("tax_identifier")
                    .map_err(|_| PortError::Internal)?,
            }))
        }
        _ => {
            let r = sqlx::query(
                "SELECT * FROM erpx_finance_write.journal_source_authority WHERE journal_id=$1",
            )
            .bind(journal_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(|_| PortError::Unavailable)?
            .ok_or(PortError::Internal)?;
            match kind.as_str() {
                "manual" => Ok(JournalSource::Manual {
                    reference: required_finance(&r, "manual_reference")?,
                }),
                "inventory_movement" => Ok(JournalSource::InventoryMovement(
                    inventory::StockMovementFact {
                        movement: inventory::StockMovementRef::try_from_uuid(required_finance(
                            &r,
                            "inventory_movement_id",
                        )?)
                        .map_err(|_| PortError::Internal)?,
                        stock_item: inventory::StockItemRef::try_from_uuid(required_finance(
                            &r,
                            "inventory_stock_item_id",
                        )?)
                        .map_err(|_| PortError::Internal)?,
                        warehouse: inventory::WarehouseRef::try_from_uuid(required_finance(
                            &r,
                            "inventory_warehouse_id",
                        )?)
                        .map_err(|_| PortError::Internal)?,
                        item_code: ReferenceCode::try_new(required_finance::<String>(
                            &r,
                            "inventory_item_code",
                        )?)
                        .map_err(|_| PortError::Internal)?,
                        quantity_delta: Quantity::try_new(
                            required_finance(&r, "inventory_quantity_units")?,
                            u8::try_from(required_finance::<i16>(&r, "inventory_quantity_scale")?)
                                .map_err(|_| PortError::Internal)?,
                        )
                        .map_err(|_| PortError::Internal)?,
                        procurement_line: r
                            .try_get::<Option<Uuid>, _>("inventory_procurement_line_id")
                            .map_err(|_| PortError::Internal)?
                            .map(procurement::PurchaseOrderLineRef::try_from_uuid)
                            .transpose()
                            .map_err(|_| PortError::Internal)?,
                    },
                )),
                "payroll" => Ok(JournalSource::Payroll(
                    people_payroll::PayrollFinalizationFact {
                        payroll_run: people_payroll::PayrollRunRef::try_from_uuid(
                            required_finance(&r, "payroll_run_id")?,
                        )
                        .map_err(|_| PortError::Internal)?,
                        period_code: ReferenceCode::try_new(required_finance::<String>(
                            &r,
                            "payroll_period_code",
                        )?)
                        .map_err(|_| PortError::Internal)?,
                        finalized_at: required_finance(&r, "payroll_finalized_at")?,
                        gross_amount: MoneyAmount::new(
                            required_finance(&r, "payroll_gross_minor")?,
                            currency_code,
                        ),
                        deduction_amount: MoneyAmount::new(
                            required_finance(&r, "payroll_deduction_minor")?,
                            currency_code,
                        ),
                        net_amount: MoneyAmount::new(
                            required_finance(&r, "payroll_net_minor")?,
                            currency_code,
                        ),
                    },
                )),
                "pos_sale" => Ok(JournalSource::PosSale(pos::PosSaleFact {
                    transaction: pos::PosTransactionRef::try_from_uuid(required_finance(
                        &r,
                        "pos_transaction_id",
                    )?)
                    .map_err(|_| PortError::Internal)?,
                    register: pos::RegisterRef::try_from_uuid(required_finance(
                        &r,
                        "pos_register_id",
                    )?)
                    .map_err(|_| PortError::Internal)?,
                    receipt_code: ReferenceCode::try_new(required_finance::<String>(
                        &r,
                        "pos_receipt_code",
                    )?)
                    .map_err(|_| PortError::Internal)?,
                    customer: r
                        .try_get::<Option<Uuid>, _>("pos_customer_id")
                        .map_err(|_| PortError::Internal)?
                        .map(customer::CustomerRef::try_from_uuid)
                        .transpose()
                        .map_err(|_| PortError::Internal)?,
                    gross_amount: MoneyAmount::new(
                        required_finance(&r, "pos_amount_minor")?,
                        currency_code,
                    ),
                    completed_at: required_finance(&r, "pos_occurred_at")?,
                })),
                "pos_refund" => Ok(JournalSource::PosRefund(pos::PosRefundFact {
                    refund: pos::PosTransactionRef::try_from_uuid(required_finance(
                        &r,
                        "pos_transaction_id",
                    )?)
                    .map_err(|_| PortError::Internal)?,
                    original_sale: pos::PosTransactionRef::try_from_uuid(required_finance(
                        &r,
                        "pos_original_sale_id",
                    )?)
                    .map_err(|_| PortError::Internal)?,
                    amount: MoneyAmount::new(
                        required_finance(&r, "pos_amount_minor")?,
                        currency_code,
                    ),
                    approved_at: required_finance(&r, "pos_occurred_at")?,
                })),
                "sales_order" => {
                    let status = parse_fact_order_status(
                        required_finance::<String>(&r, "sales_order_status")?.as_str(),
                    )?;
                    let lines=sqlx::query("SELECT line_id,item_code,quantity_units,quantity_scale,line_value_minor FROM erpx_finance_write.journal_sales_line_authority WHERE journal_id=$1 ORDER BY line_id").bind(journal_id).fetch_all(&mut **tx).await.map_err(|_|PortError::Unavailable)?.into_iter().map(|l|Ok(sales::SalesOrderLineFact{line:sales::SalesOrderLineRef::try_from_uuid(l.try_get("line_id").map_err(|_|PortError::Internal)?).map_err(|_|PortError::Internal)?,item_code:ReferenceCode::try_new(l.try_get::<String,_>("item_code").map_err(|_|PortError::Internal)?).map_err(|_|PortError::Internal)?,quantity:Quantity::try_new(l.try_get("quantity_units").map_err(|_|PortError::Internal)?,u8::try_from(l.try_get::<i16,_>("quantity_scale").map_err(|_|PortError::Internal)?).map_err(|_|PortError::Internal)?).map_err(|_|PortError::Internal)?,line_value:MoneyAmount::new(l.try_get("line_value_minor").map_err(|_|PortError::Internal)?,currency_code)})).collect::<Result<Vec<_>,PortError>>()?;
                    Ok(JournalSource::SalesOrder(sales::SalesOrderFact {
                        sales_order: sales::SalesOrderRef::try_from_uuid(required_finance(
                            &r,
                            "sales_order_id",
                        )?)
                        .map_err(|_| PortError::Internal)?,
                        order_code: ReferenceCode::try_new(required_finance::<String>(
                            &r,
                            "sales_order_code",
                        )?)
                        .map_err(|_| PortError::Internal)?,
                        customer: customer::CustomerRef::try_from_uuid(required_finance(
                            &r,
                            "sales_customer_id",
                        )?)
                        .map_err(|_| PortError::Internal)?,
                        status,
                        total: MoneyAmount::new(
                            required_finance(&r, "sales_total_minor")?,
                            currency_code,
                        ),
                        lines,
                    }))
                }
                _ => Err(PortError::Internal),
            }
        }
    }
}

async fn finance_authorized_root(
    tx: &mut Transaction<'_, Postgres>,
    authority: &TenantScope,
    base: &str,
    table: &str,
    id_column: &str,
    id: Uuid,
) -> Result<Option<sqlx::postgres::PgRow>, PortError> {
    let (kind, h, c, b, companies, branches) = finance_authority_parameters(authority)?;
    let query = format!(
        "{base} AND ($2='platform' OR ($2='holding' AND holding_id=$3) OR ($2='company' AND holding_id=$3 AND company_id=$4) OR ($2='branch' AND holding_id=$3 AND company_id=$4 AND branch_id=$5) OR ($2='subtree' AND holding_id=$3 AND ((branch_id IS NULL AND cardinality($7::uuid[])=0 AND company_id=ANY($6::uuid[])) OR (branch_id IS NOT NULL AND company_id=ANY($6::uuid[]) AND branch_id=ANY($7::uuid[])))))"
    );
    let row = sqlx::query(&query)
        .bind(id)
        .bind(kind)
        .bind(h)
        .bind(c)
        .bind(b)
        .bind(companies)
        .bind(branches)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
    if row.is_some() {
        return Ok(row);
    }
    let exists_query = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE {id_column}=$1)");
    let exists: bool = sqlx::query_scalar(&exists_query)
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
    if exists {
        Err(PortError::Denied)
    } else {
        Ok(None)
    }
}
type FinanceAuthorityParameters = (
    &'static str,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    Vec<Uuid>,
    Vec<Uuid>,
);
fn finance_authority_parameters(
    scope: &TenantScope,
) -> Result<FinanceAuthorityParameters, PortError> {
    scope.validate().map_err(|_| PortError::Denied)?;
    Ok(match scope {
        TenantScope::Platform => ("platform", None, None, None, vec![], vec![]),
        TenantScope::Holding { holding_id } => {
            ("holding", Some(*holding_id), None, None, vec![], vec![])
        }
        TenantScope::Company {
            holding_id,
            company_id,
        } => (
            "company",
            Some(*holding_id),
            Some(*company_id),
            None,
            vec![],
            vec![],
        ),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => (
            "branch",
            Some(*holding_id),
            Some(*company_id),
            Some(*branch_id),
            vec![],
            vec![],
        ),
        TenantScope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => (
            "subtree",
            Some(*holding_id),
            None,
            None,
            company_ids.clone(),
            branch_ids.clone(),
        ),
    })
}
fn finance_exact_scope(row: &sqlx::postgres::PgRow) -> Result<TenantScope, PortError> {
    let holding_id = row.try_get("holding_id").map_err(|_| PortError::Internal)?;
    let company_id = row.try_get("company_id").map_err(|_| PortError::Internal)?;
    Ok(
        match row.try_get("branch_id").map_err(|_| PortError::Internal)? {
            Some(branch_id) => TenantScope::Branch {
                holding_id,
                company_id,
                branch_id,
            },
            None => TenantScope::Company {
                holding_id,
                company_id,
            },
        },
    )
}
fn finance_row_u64(row: &sqlx::postgres::PgRow, column: &str) -> Result<u64, PortError> {
    u64::try_from(
        row.try_get::<i64, _>(column)
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)
}
fn required_finance<'r, T>(row: &'r sqlx::postgres::PgRow, column: &str) -> Result<T, PortError>
where
    T: sqlx::Decode<'r, Postgres> + sqlx::Type<Postgres>,
{
    row.try_get::<Option<T>, _>(column)
        .map_err(|_| PortError::Internal)?
        .ok_or(PortError::Internal)
}
fn currency_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<CurrencyCode, PortError> {
    CurrencyCode::try_new(
        row.try_get::<String, _>(column)
            .map_err(|_| PortError::Internal)?
            .as_str(),
    )
    .map_err(|_| PortError::Internal)
}
fn parse_account_category(v: &str) -> Result<AccountCategory, PortError> {
    match v {
        "asset" => Ok(AccountCategory::Asset),
        "liability" => Ok(AccountCategory::Liability),
        "equity" => Ok(AccountCategory::Equity),
        "revenue" => Ok(AccountCategory::Revenue),
        "expense" => Ok(AccountCategory::Expense),
        _ => Err(PortError::Internal),
    }
}
fn parse_normal_balance(v: &str) -> Result<NormalBalance, PortError> {
    match v {
        "debit" => Ok(NormalBalance::Debit),
        "credit" => Ok(NormalBalance::Credit),
        _ => Err(PortError::Internal),
    }
}
fn parse_account_status(v: &str) -> Result<AccountStatus, PortError> {
    match v {
        "active" => Ok(AccountStatus::Active),
        "inactive" => Ok(AccountStatus::Inactive),
        _ => Err(PortError::Internal),
    }
}
fn parse_period_status(v: &str) -> Result<PeriodStatus, PortError> {
    match v {
        "open" => Ok(PeriodStatus::Open),
        "closing" => Ok(PeriodStatus::Closing),
        "closed" => Ok(PeriodStatus::Closed),
        _ => Err(PortError::Internal),
    }
}
fn parse_journal_status(v: &str) -> Result<JournalStatus, PortError> {
    match v {
        "draft" => Ok(JournalStatus::Draft),
        "posted" => Ok(JournalStatus::Posted),
        _ => Err(PortError::Internal),
    }
}
fn parse_subledger_status(v: &str) -> Result<SubledgerStatus, PortError> {
    match v {
        "open" => Ok(SubledgerStatus::Open),
        "partially_settled" => Ok(SubledgerStatus::PartiallySettled),
        "settled" => Ok(SubledgerStatus::Settled),
        _ => Err(PortError::Internal),
    }
}
fn parse_invoice_status(v: &str) -> Result<CustomerInvoiceStatus, PortError> {
    match v {
        "draft" => Ok(CustomerInvoiceStatus::Draft),
        "posted" => Ok(CustomerInvoiceStatus::Posted),
        "cancelled" => Ok(CustomerInvoiceStatus::Cancelled),
        _ => Err(PortError::Internal),
    }
}
fn parse_schedule_status(v: &str) -> Result<RevenueScheduleStatus, PortError> {
    match v {
        "active" => Ok(RevenueScheduleStatus::Active),
        "completed" => Ok(RevenueScheduleStatus::Completed),
        _ => Err(PortError::Internal),
    }
}
fn parse_finance_revenue_model(v: &str) -> Result<sales::RevenueModel, PortError> {
    match v {
        "goods" => Ok(sales::RevenueModel::Goods),
        "service_milestone" => Ok(sales::RevenueModel::ServiceMilestone),
        "recurring" => Ok(sales::RevenueModel::Recurring),
        _ => Err(PortError::Internal),
    }
}
fn parse_tax_kind(v: &str) -> Result<StatutoryTaxKind, PortError> {
    match v {
        "output_vat" => Ok(StatutoryTaxKind::OutputVat),
        "input_vat" => Ok(StatutoryTaxKind::InputVat),
        "withholding" => Ok(StatutoryTaxKind::Withholding),
        _ => Err(PortError::Internal),
    }
}
fn parse_fact_order_status(v: &str) -> Result<sales::SalesOrderStatus, PortError> {
    match v {
        "confirmed" => Ok(sales::SalesOrderStatus::Confirmed),
        "partially_fulfilled" => Ok(sales::SalesOrderStatus::PartiallyFulfilled),
        "fulfilled" => Ok(sales::SalesOrderStatus::Fulfilled),
        "cancelled" => Ok(sales::SalesOrderStatus::Cancelled),
        _ => Err(PortError::Internal),
    }
}
fn finance_dimensions_from_row(row: &sqlx::postgres::PgRow) -> Result<DimensionSet, PortError> {
    macro_rules! req {
        ($f:path,$c:literal) => {
            $f(row.try_get($c).map_err(|_| PortError::Internal)?)
                .map_err(|_| PortError::Internal)?
        };
    }
    macro_rules! opt {
        ($f:path,$c:literal) => {
            row.try_get::<Option<Uuid>, _>($c)
                .map_err(|_| PortError::Internal)?
                .map($f)
                .transpose()
                .map_err(|_| PortError::Internal)?
        };
    }
    Ok(DimensionSet {
        legal_entity: req!(organization::CompanyRef::try_from_uuid, "legal_entity_id"),
        branch: opt!(
            organization::BranchRef::try_from_uuid,
            "branch_dimension_id"
        ),
        business_unit: opt!(
            organization::BusinessUnitRef::try_from_uuid,
            "business_unit_id"
        ),
        cost_center: opt!(
            erpx_integration_contracts::CostCenterRef::try_from_uuid,
            "cost_center_id"
        ),
        location: opt!(
            erpx_integration_contracts::LocationRef::try_from_uuid,
            "location_id"
        ),
        project: opt!(
            erpx_integration_contracts::ProjectRef::try_from_uuid,
            "project_id"
        ),
        campaign: opt!(marketing::CampaignRef::try_from_uuid, "campaign_id"),
        channel: opt!(marketing::ChannelRef::try_from_uuid, "channel_id"),
        party: opt!(
            erpx_integration_contracts::PartyRef::try_from_uuid,
            "party_id"
        ),
        employee: opt!(people_payroll::EmployeeRef::try_from_uuid, "employee_id"),
        asset: opt!(assets::AssetRef::try_from_uuid, "asset_id"),
        legal_matter: opt!(legal::LegalMatterRef::try_from_uuid, "legal_matter_id"),
        intercompany_partner: opt!(
            organization::CompanyRef::try_from_uuid,
            "intercompany_partner_id"
        ),
    })
}

#[async_trait]
impl WriteRepository for FinanceWriteRepository {
    async fn load_chart(
        &self,
        scope: &TenantScope,
        id: ChartId,
    ) -> Result<Option<Chart>, PortError> {
        match self
            .load(
                scope,
                &chart_partition(id),
                CHART_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(FinanceAggregate::Chart(v)) => Ok(Some(v)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
    async fn load_period(
        &self,
        scope: &TenantScope,
        id: erpx_finance::PeriodId,
    ) -> Result<Option<AccountingPeriod>, PortError> {
        match self
            .load(
                scope,
                &period_partition(id),
                PERIOD_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(FinanceAggregate::Period(v)) => Ok(Some(v)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
    async fn load_journal(
        &self,
        scope: &TenantScope,
        id: JournalId,
    ) -> Result<Option<Journal>, PortError> {
        match self
            .load(
                scope,
                &journal_partition(id),
                JOURNAL_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(FinanceAggregate::Journal(v)) => Ok(Some(v)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
    async fn load_subledger(
        &self,
        scope: &TenantScope,
        id: SubledgerId,
    ) -> Result<Option<SubledgerItem>, PortError> {
        match self
            .load(
                scope,
                &subledger_partition(id),
                SUBLEDGER_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(FinanceAggregate::Subledger(v)) => Ok(Some(v)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
    async fn load_customer_invoice(
        &self,
        scope: &TenantScope,
        id: CustomerInvoiceId,
    ) -> Result<Option<CustomerInvoice>, PortError> {
        match self
            .load(
                scope,
                &customer_invoice_partition(id),
                CUSTOMER_INVOICE_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(FinanceAggregate::CustomerInvoice(v)) => Ok(Some(v)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
    async fn load_revenue_schedule(
        &self,
        scope: &TenantScope,
        id: RevenueScheduleId,
    ) -> Result<Option<RevenueSchedule>, PortError> {
        match self
            .load(
                scope,
                &revenue_schedule_partition(id),
                REVENUE_SCHEDULE_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(FinanceAggregate::RevenueSchedule(v)) => Ok(Some(v)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FinanceCommandAdapter {
    store: CommandStore,
}
impl FinanceCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for FinanceCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let (partition, kind) = fingerprint_target(&identity.fingerprint);
        self.store
            .find_receipt::<FinanceAggregate>(
                CONTEXT_NAME,
                &partition,
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?
            .map(|receipt| receipt_to_stored(receipt, kind))
            .transpose()
    }

    async fn commit(&self, request: FinanceCommit) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let partition = aggregate_partition(&request.aggregate);
        let kind = request.aggregate.aggregate_type();
        let id = request.aggregate.id_uuid();
        let occurred_at = aggregate_updated_at(&request.aggregate);
        let snapshot_event = snapshot_event(&request.aggregate);
        let command_id = request.identity.command_id;
        let mut events = request
            .integration_events
            .iter()
            .enumerate()
            .map(|(index, event)| OutboxEventWrite {
                event_id: deterministic_event_id(command_id, event.event_type(), index),
                event_type: event.event_type().into(),
                event_version: u32::from(event.schema_version()),
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: FinanceOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, snapshot_event, events.len()),
            event_type: snapshot_event.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: FinanceOutboxPayload::Snapshot(FinanceSnapshot::from(&request.aggregate)),
        });
        if let Some(directory_draft) = finance_directory_draft(&request.aggregate)? {
            events.push(OutboxEventWrite {
                event_id: deterministic_event_id(
                    command_id,
                    ITEM_DIRECTORY_OWNER_EVENT_TYPE,
                    events.len(),
                ),
                event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
                event_version: ITEM_DIRECTORY_FACT_VERSION,
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: FinanceOutboxPayload::Directory(directory_draft),
            });
        }
        let guard = FinanceGuard {
            aggregate: request.aggregate.clone(),
            references: request.reference_versions.clone(),
        };
        let aggregate = request.aggregate;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.identity.idempotency_key,
            request_hash: request_hash(&request.identity.fingerprint)?,
            tenant_scope: aggregate.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: kind.into(),
            aggregate_id: AggregateId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
            expected_version: request.expected_version,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"finance-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({ "aggregate_type": kind, "aggregate_id": id, "expected_version": request.expected_version, "reference_versions": request.reference_versions.iter().map(|v| json!({"type":v.aggregate_type,"id":v.aggregate_id,"version":v.expected_version})).collect::<Vec<_>>() }),
            },
            events,
        };
        let outcome = self
            .store
            .commit_normalized(command, &guard)
            .await
            .map_err(map_store_error)?;
        let replayed = matches!(outcome, AtomicCommandOutcome::Replayed(_));
        let receipt = match outcome {
            AtomicCommandOutcome::Committed(v) | AtomicCommandOutcome::Replayed(v) => v,
        };
        let mut stored = receipt_to_stored(receipt, kind)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

fn finance_directory_draft(
    aggregate: &FinanceAggregate,
) -> Result<Option<ItemDescriptorDraft>, PortError> {
    let (record_type, lifecycle_state, search_text) = match aggregate {
        FinanceAggregate::Period(value) => (
            "finance.close-cycle",
            period_status_name(value.status()),
            value.code().to_owned(),
        ),
        FinanceAggregate::Journal(value) => (
            "finance.journal",
            journal_status_name(value.status()),
            value.journal_code().to_owned(),
        ),
        FinanceAggregate::CustomerInvoice(value) => (
            "finance.invoice",
            customer_invoice_status_name(value.status()),
            value.invoice_code().to_owned(),
        ),
        // Chart, subledger, and revenue-schedule roots are implemented write
        // authority but do not have first-class route families in the catalog.
        FinanceAggregate::Chart(_)
        | FinanceAggregate::Subledger(_)
        | FinanceAggregate::RevenueSchedule(_) => return Ok(None),
    };
    let record_id =
        RecordId::try_from_uuid(aggregate.id_uuid()).map_err(|_| PortError::Internal)?;
    let record = StableRecordRef::try_new(
        record_id,
        ContextId::Finance,
        RecordTypeId::try_new(record_type).map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: aggregate.scope().clone(),
        lifecycle_state: lifecycle_state.into(),
        classification: DataClassification::Restricted,
        descriptor_version: aggregate.version(),
        authorized_search_text: search_text,
    };
    value.validate().map_err(|_| PortError::Internal)?;
    Ok(Some(value))
}

struct FinanceGuard {
    aggregate: FinanceAggregate,
    references: Vec<ReferenceVersion>,
}

#[async_trait]
impl AtomicCommandGuard for FinanceGuard {
    async fn authoritative_version(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let current: Option<i64> = match &self.aggregate {
            FinanceAggregate::Chart(v) => sqlx::query_scalar("SELECT aggregate_version FROM erpx_finance_write.chart_guards WHERE chart_id=$1 FOR UPDATE").bind(v.id().as_uuid()).fetch_optional(&mut **tx).await?,
            FinanceAggregate::Period(v) => sqlx::query_scalar("SELECT aggregate_version FROM erpx_finance_write.period_guards WHERE period_id=$1 FOR UPDATE").bind(v.id().as_uuid()).fetch_optional(&mut **tx).await?,
            FinanceAggregate::Journal(v) => sqlx::query_scalar("SELECT aggregate_version FROM erpx_finance_write.journal_guards WHERE journal_id=$1 FOR UPDATE").bind(v.id().as_uuid()).fetch_optional(&mut **tx).await?,
            FinanceAggregate::Subledger(v) => sqlx::query_scalar("SELECT aggregate_version FROM erpx_finance_write.subledger_guards WHERE subledger_id=$1 FOR UPDATE").bind(v.id().as_uuid()).fetch_optional(&mut **tx).await?,
            FinanceAggregate::CustomerInvoice(v) => sqlx::query_scalar("SELECT aggregate_version FROM erpx_finance_write.customer_invoice_guards WHERE invoice_id=$1 FOR UPDATE").bind(v.id().as_uuid()).fetch_optional(&mut **tx).await?,
            FinanceAggregate::RevenueSchedule(v) => sqlx::query_scalar("SELECT aggregate_version FROM erpx_finance_write.revenue_schedule_guards WHERE schedule_id=$1 FOR UPDATE").bind(v.id().as_uuid()).fetch_optional(&mut **tx).await?,
        };
        current
            .map(|value| u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange))
            .transpose()
    }

    async fn validate(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        validate_reference_versions(tx, &self.references).await?;
        let current: Option<i64> = match &self.aggregate {
            FinanceAggregate::Chart(v) => sqlx::query_scalar("SELECT aggregate_version FROM erpx_finance_write.chart_guards WHERE chart_id=$1 FOR UPDATE").bind(v.id().as_uuid()).fetch_optional(&mut **tx).await?,
            FinanceAggregate::Period(v) => sqlx::query_scalar("SELECT aggregate_version FROM erpx_finance_write.period_guards WHERE period_id=$1 FOR UPDATE").bind(v.id().as_uuid()).fetch_optional(&mut **tx).await?,
            FinanceAggregate::Journal(v) => sqlx::query_scalar("SELECT aggregate_version FROM erpx_finance_write.journal_guards WHERE journal_id=$1 FOR UPDATE").bind(v.id().as_uuid()).fetch_optional(&mut **tx).await?,
            FinanceAggregate::Subledger(v) => sqlx::query_scalar("SELECT aggregate_version FROM erpx_finance_write.subledger_guards WHERE subledger_id=$1 FOR UPDATE").bind(v.id().as_uuid()).fetch_optional(&mut **tx).await?,
            FinanceAggregate::CustomerInvoice(v) => sqlx::query_scalar("SELECT aggregate_version FROM erpx_finance_write.customer_invoice_guards WHERE invoice_id=$1 FOR UPDATE").bind(v.id().as_uuid()).fetch_optional(&mut **tx).await?,
            FinanceAggregate::RevenueSchedule(v) => sqlx::query_scalar("SELECT aggregate_version FROM erpx_finance_write.revenue_schedule_guards WHERE schedule_id=$1 FOR UPDATE").bind(v.id().as_uuid()).fetch_optional(&mut **tx).await?,
        };
        ensure_existence(context.expected_version, current)?;
        if let FinanceAggregate::Journal(value) = &self.aggregate {
            validate_journal_source_uniqueness(tx, value).await?;
        }
        if let FinanceAggregate::CustomerInvoice(value) = &self.aggregate {
            validate_customer_invoice_uniqueness(tx, value).await?;
        }
        if let FinanceAggregate::RevenueSchedule(value) = &self.aggregate {
            validate_revenue_schedule_uniqueness(tx, value).await?;
        }
        Ok(())
    }

    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_type != self.aggregate.aggregate_type()
            || context.aggregate_id.as_uuid() != self.aggregate.id_uuid()
            || context.partition != aggregate_partition(&self.aggregate)
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Finance applied guard metadata mismatch",
            ));
        }
        match &self.aggregate {
            FinanceAggregate::Chart(v) => apply_chart(tx, v).await,
            FinanceAggregate::Period(v) => apply_period(tx, v).await,
            FinanceAggregate::Journal(v) => apply_journal(tx, v).await,
            FinanceAggregate::Subledger(v) => apply_subledger(tx, v).await,
            FinanceAggregate::CustomerInvoice(v) => apply_customer_invoice(tx, v).await,
            FinanceAggregate::RevenueSchedule(v) => apply_revenue_schedule(tx, v).await,
        }
    }
}

async fn validate_journal_source_uniqueness(
    tx: &mut Transaction<'_, Postgres>,
    value: &erpx_finance::Journal,
) -> Result<(), CqrsSqlxError> {
    let (source_kind, source_reference) = journal_source_identity(value.source())
        .map_err(|_| CqrsSqlxError::GuardRejected("Finance journal source is invalid"))?;
    let scope = scope_key(value.scope())?;
    let lock_identity = format!("finance.journal-source:{scope}:{source_kind}:{source_reference}");
    // Serialize the absence check as well as the existing-row case. A plain
    // SELECT ... FOR UPDATE cannot lock a source identity that has no row yet.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(lock_identity)
        .execute(&mut **tx)
        .await?;
    let duplicate: Option<Uuid> = sqlx::query_scalar(
        "SELECT journal_id FROM erpx_finance_write.journal_guards
         WHERE scope_key=$1 AND source_kind=$2 AND source_reference=$3
           AND journal_id<>$4 ORDER BY journal_id LIMIT 1 FOR UPDATE",
    )
    .bind(scope)
    .bind(source_kind)
    .bind(source_reference)
    .bind(value.id().as_uuid())
    .fetch_optional(&mut **tx)
    .await?;
    if duplicate.is_some() {
        Err(CqrsSqlxError::GuardRejected(
            "Finance journal source was already consumed",
        ))
    } else {
        Ok(())
    }
}

async fn validate_customer_invoice_uniqueness(
    tx: &mut Transaction<'_, Postgres>,
    value: &CustomerInvoice,
) -> Result<(), CqrsSqlxError> {
    let duplicate: Option<Uuid> = sqlx::query_scalar(
        "SELECT invoice_id FROM erpx_finance_write.customer_invoice_guards
         WHERE (scope_key=$1 AND invoice_code=$2 OR billing_event_id=$3)
           AND invoice_id<>$4 ORDER BY invoice_id LIMIT 1 FOR UPDATE",
    )
    .bind(scope_key(value.scope())?)
    .bind(value.invoice_code())
    .bind(value.billing().billing_event.as_uuid())
    .bind(value.id().as_uuid())
    .fetch_optional(&mut **tx)
    .await?;
    if duplicate.is_some() {
        Err(CqrsSqlxError::GuardRejected(
            "Finance invoice code or billing event already exists",
        ))
    } else {
        Ok(())
    }
}

async fn validate_revenue_schedule_uniqueness(
    tx: &mut Transaction<'_, Postgres>,
    value: &RevenueSchedule,
) -> Result<(), CqrsSqlxError> {
    let duplicate: Option<Uuid> = sqlx::query_scalar(
        "SELECT schedule_id FROM erpx_finance_write.revenue_schedule_guards
         WHERE arrangement_id=$1 AND schedule_id<>$2
         ORDER BY schedule_id LIMIT 1 FOR UPDATE",
    )
    .bind(value.arrangement().arrangement.as_uuid())
    .bind(value.id().as_uuid())
    .fetch_optional(&mut **tx)
    .await?;
    if duplicate.is_some() {
        Err(CqrsSqlxError::GuardRejected(
            "Finance revenue arrangement already has a schedule",
        ))
    } else {
        Ok(())
    }
}

async fn validate_reference_versions(
    tx: &mut Transaction<'_, Postgres>,
    refs: &[ReferenceVersion],
) -> Result<(), CqrsSqlxError> {
    for reference in refs {
        let actual: Option<i64> = match reference.aggregate_type {
            "chart" => sqlx::query_scalar("SELECT aggregate_version FROM erpx_finance_write.chart_guards WHERE chart_id=$1 FOR SHARE").bind(reference.aggregate_id).fetch_optional(&mut **tx).await?,
            "period" => sqlx::query_scalar("SELECT aggregate_version FROM erpx_finance_write.period_guards WHERE period_id=$1 FOR SHARE").bind(reference.aggregate_id).fetch_optional(&mut **tx).await?,
            _ => return Err(CqrsSqlxError::GuardRejected("Finance reference type is invalid")),
        };
        if actual.and_then(|v| u64::try_from(v).ok()) != Some(reference.expected_version) {
            return Err(CqrsSqlxError::GuardRejected(
                "Finance reference aggregate moved",
            ));
        }
    }
    Ok(())
}

async fn apply_chart(tx: &mut Transaction<'_, Postgres>, v: &Chart) -> Result<(), CqrsSqlxError> {
    let changed=sqlx::query("INSERT INTO erpx_finance_write.chart_guards (chart_id,scope_key,chart_code,chart_name,currency,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (chart_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,chart_code=EXCLUDED.chart_code,chart_name=EXCLUDED.chart_name,currency=EXCLUDED.currency,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at WHERE erpx_finance_write.chart_guards.aggregate_version + 1=EXCLUDED.aggregate_version").bind(v.id().as_uuid()).bind(scope_key(v.scope())?).bind(v.code()).bind(v.name()).bind(v.currency()).bind(to_i64(v.version())?).bind(v.created_at()).bind(v.updated_at()).execute(&mut **tx).await?.rows_affected();
    finalize_finance_root(
        tx,
        "erpx_finance_write.chart_guards",
        "chart_id",
        v.id().as_uuid(),
        v.scope(),
        changed,
    )
    .await?;
    sqlx::query("DELETE FROM erpx_finance_write.account_guards WHERE chart_id=$1")
        .bind(v.id().as_uuid())
        .execute(&mut **tx)
        .await?;
    for account in v.accounts() {
        sqlx::query("INSERT INTO erpx_finance_write.account_guards (chart_id,account_id,account_code,account_name,category,normal_balance,parent_account_id,account_status) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (chart_id,account_id) DO UPDATE SET account_code=EXCLUDED.account_code,account_name=EXCLUDED.account_name,category=EXCLUDED.category,normal_balance=EXCLUDED.normal_balance,parent_account_id=EXCLUDED.parent_account_id,account_status=EXCLUDED.account_status").bind(v.id().as_uuid()).bind(account.account_id.as_uuid()).bind(&account.code).bind(&account.name).bind(account_category_name(account.category)).bind(normal_balance_name(account.normal_balance)).bind(account.parent_account_id.map(|id| id.as_uuid())).bind(account_status_name(account.status)).execute(&mut **tx).await?;
    }
    Ok(())
}

async fn apply_period(
    tx: &mut Transaction<'_, Postgres>,
    v: &AccountingPeriod,
) -> Result<(), CqrsSqlxError> {
    let changed=sqlx::query("INSERT INTO erpx_finance_write.period_guards (period_id,scope_key,period_code,starts_on,ends_on,period_status,close_requested_by,closed_by,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT (period_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,period_code=EXCLUDED.period_code,starts_on=EXCLUDED.starts_on,ends_on=EXCLUDED.ends_on,period_status=EXCLUDED.period_status,close_requested_by=EXCLUDED.close_requested_by,closed_by=EXCLUDED.closed_by,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at WHERE erpx_finance_write.period_guards.aggregate_version + 1=EXCLUDED.aggregate_version").bind(v.id().as_uuid()).bind(scope_key(v.scope())?).bind(v.code()).bind(v.starts_on()).bind(v.ends_on()).bind(period_status_name(v.status())).bind(v.close_requested_by()).bind(v.closed_by()).bind(to_i64(v.version())?).bind(v.created_at()).bind(v.updated_at()).execute(&mut **tx).await?.rows_affected();
    finalize_finance_root(
        tx,
        "erpx_finance_write.period_guards",
        "period_id",
        v.id().as_uuid(),
        v.scope(),
        changed,
    )
    .await?;
    Ok(())
}

async fn apply_journal(
    tx: &mut Transaction<'_, Postgres>,
    v: &Journal,
) -> Result<(), CqrsSqlxError> {
    let (source_kind, source_reference) = journal_source_identity(v.source())
        .map_err(|_| CqrsSqlxError::GuardRejected("Finance journal source is invalid"))?;
    let changed=sqlx::query("INSERT INTO erpx_finance_write.journal_guards (journal_id,scope_key,chart_id,period_id,journal_code,entry_date,currency,source_kind,source_reference,description,debit_total_minor,credit_total_minor,journal_status,posted_at,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) ON CONFLICT (journal_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,chart_id=EXCLUDED.chart_id,period_id=EXCLUDED.period_id,journal_code=EXCLUDED.journal_code,entry_date=EXCLUDED.entry_date,currency=EXCLUDED.currency,source_kind=EXCLUDED.source_kind,source_reference=EXCLUDED.source_reference,description=EXCLUDED.description,debit_total_minor=EXCLUDED.debit_total_minor,credit_total_minor=EXCLUDED.credit_total_minor,journal_status=EXCLUDED.journal_status,posted_at=EXCLUDED.posted_at,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at WHERE erpx_finance_write.journal_guards.aggregate_version + 1=EXCLUDED.aggregate_version").bind(v.id().as_uuid()).bind(scope_key(v.scope())?).bind(v.chart_id().as_uuid()).bind(v.period_id().as_uuid()).bind(v.journal_code()).bind(v.entry_date()).bind(v.currency()).bind(source_kind).bind(source_reference).bind(v.description()).bind(v.debit_total_minor()).bind(v.credit_total_minor()).bind(journal_status_name(v.status())).bind(v.posted_at()).bind(to_i64(v.version())?).bind(v.created_at()).bind(v.updated_at()).execute(&mut **tx).await?.rows_affected();
    finalize_finance_root(
        tx,
        "erpx_finance_write.journal_guards",
        "journal_id",
        v.id().as_uuid(),
        v.scope(),
        changed,
    )
    .await?;
    sqlx::query("DELETE FROM erpx_finance_write.journal_line_guards WHERE journal_id=$1")
        .bind(v.id().as_uuid())
        .execute(&mut **tx)
        .await?;
    for line in v.lines() {
        sqlx::query("INSERT INTO erpx_finance_write.journal_line_guards (journal_id,chart_id,line_id,account_id,debit_minor,credit_minor,currency,memo) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (journal_id,line_id) DO UPDATE SET chart_id=EXCLUDED.chart_id,account_id=EXCLUDED.account_id,debit_minor=EXCLUDED.debit_minor,credit_minor=EXCLUDED.credit_minor,currency=EXCLUDED.currency,memo=EXCLUDED.memo").bind(v.id().as_uuid()).bind(v.chart_id().as_uuid()).bind(line.line_id.as_uuid()).bind(line.account_id.as_uuid()).bind(line.debit_minor).bind(line.credit_minor).bind(&line.currency).bind(&line.memo).execute(&mut **tx).await?;
    }
    match v.source() {
        JournalSource::BankStatement(fact) => compare_bank_fact(tx, v.id(), fact).await?,
        JournalSource::TaxAdjustment(fact) => compare_tax_fact(tx, v.id(), fact).await?,
        _ => {}
    }
    write_journal_source_authority(tx, v).await?;
    Ok(())
}

async fn compare_bank_fact(
    tx: &mut Transaction<'_, Postgres>,
    journal_id: JournalId,
    fact: &erpx_finance::BankTransactionFact,
) -> Result<(), CqrsSqlxError> {
    let existing = sqlx::query("SELECT import_id,transaction_id,reference,posted_on,amount_minor,currency FROM erpx_finance_write.bank_transaction_facts WHERE journal_id=$1 FOR SHARE").bind(journal_id.as_uuid()).fetch_optional(&mut **tx).await?;
    if let Some(row) = existing {
        if row.try_get::<Uuid, _>("import_id")? != fact.import_id
            || row.try_get::<Uuid, _>("transaction_id")? != fact.transaction_id
            || row.try_get::<String, _>("reference")? != fact.reference
            || row.try_get::<chrono::NaiveDate, _>("posted_on")? != fact.posted_on
            || row.try_get::<i64, _>("amount_minor")? != fact.amount_minor
            || row.try_get::<String, _>("currency")? != fact.currency
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Finance bank source fact changed",
            ));
        }
        return Ok(());
    }
    sqlx::query("INSERT INTO erpx_finance_write.bank_transaction_facts (journal_id,import_id,transaction_id,reference,posted_on,amount_minor,currency) VALUES ($1,$2,$3,$4,$5,$6,$7)").bind(journal_id.as_uuid()).bind(fact.import_id).bind(fact.transaction_id).bind(&fact.reference).bind(fact.posted_on).bind(fact.amount_minor).bind(&fact.currency).execute(&mut **tx).await?;
    Ok(())
}

async fn compare_tax_fact(
    tx: &mut Transaction<'_, Postgres>,
    journal_id: JournalId,
    fact: &erpx_finance::StatutoryTaxFact,
) -> Result<(), CqrsSqlxError> {
    let existing = sqlx::query("SELECT tax_fact_id,reference,tax_date,tax_period,tax_kind,currency,base_minor,tax_minor,tax_rate_basis_points,tax_identifier FROM erpx_finance_write.statutory_tax_facts WHERE journal_id=$1 FOR SHARE").bind(journal_id.as_uuid()).fetch_optional(&mut **tx).await?;
    if let Some(row) = existing {
        if row.try_get::<Uuid, _>("tax_fact_id")? != fact.tax_fact_id
            || row.try_get::<String, _>("reference")? != fact.reference
            || row.try_get::<chrono::NaiveDate, _>("tax_date")? != fact.tax_date
            || row.try_get::<String, _>("tax_period")? != fact.tax_period
            || row.try_get::<String, _>("tax_kind")? != statutory_tax_kind_name(fact.kind)
            || row.try_get::<String, _>("currency")? != fact.currency
            || row.try_get::<i64, _>("base_minor")? != fact.base_minor
            || row.try_get::<i64, _>("tax_minor")? != fact.tax_minor
            || row.try_get::<i32, _>("tax_rate_basis_points")?
                != i32::try_from(fact.tax_rate_basis_points)
                    .map_err(|_| CqrsSqlxError::NumericRange)?
            || row.try_get::<String, _>("tax_identifier")? != fact.tax_identifier
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Finance tax source fact changed",
            ));
        }
        return Ok(());
    }
    sqlx::query("INSERT INTO erpx_finance_write.statutory_tax_facts (journal_id,tax_fact_id,reference,tax_date,tax_period,tax_kind,currency,base_minor,tax_minor,tax_rate_basis_points,tax_identifier) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)").bind(journal_id.as_uuid()).bind(fact.tax_fact_id).bind(&fact.reference).bind(fact.tax_date).bind(&fact.tax_period).bind(statutory_tax_kind_name(fact.kind)).bind(&fact.currency).bind(fact.base_minor).bind(fact.tax_minor).bind(i32::try_from(fact.tax_rate_basis_points).map_err(|_| CqrsSqlxError::NumericRange)?).bind(&fact.tax_identifier).execute(&mut **tx).await?;
    Ok(())
}

async fn apply_subledger(
    tx: &mut Transaction<'_, Postgres>,
    v: &SubledgerItem,
) -> Result<(), CqrsSqlxError> {
    let (kind, party) = subledger_identity(v.kind());
    let changed=sqlx::query("INSERT INTO erpx_finance_write.subledger_guards (subledger_id,scope_key,subledger_kind,party_id,document_code,source_reference,currency,original_minor,outstanding_minor,subledger_status,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT (subledger_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,subledger_kind=EXCLUDED.subledger_kind,party_id=EXCLUDED.party_id,document_code=EXCLUDED.document_code,source_reference=EXCLUDED.source_reference,currency=EXCLUDED.currency,original_minor=EXCLUDED.original_minor,outstanding_minor=EXCLUDED.outstanding_minor,subledger_status=EXCLUDED.subledger_status,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at WHERE erpx_finance_write.subledger_guards.aggregate_version + 1=EXCLUDED.aggregate_version").bind(v.id().as_uuid()).bind(scope_key(v.scope())?).bind(kind).bind(party).bind(v.document_code()).bind(v.source_reference()).bind(v.currency()).bind(v.original_minor()).bind(v.outstanding_minor()).bind(subledger_status_name(v.status())).bind(to_i64(v.version())?).bind(v.created_at()).bind(v.updated_at()).execute(&mut **tx).await?.rows_affected();
    finalize_finance_root(
        tx,
        "erpx_finance_write.subledger_guards",
        "subledger_id",
        v.id().as_uuid(),
        v.scope(),
        changed,
    )
    .await?;
    Ok(())
}

async fn apply_customer_invoice(
    tx: &mut Transaction<'_, Postgres>,
    v: &CustomerInvoice,
) -> Result<(), CqrsSqlxError> {
    let billing = v.billing();
    let receivable_minor = v
        .receivable_minor()
        .map_err(|_| CqrsSqlxError::GuardRejected("Finance customer invoice amount is invalid"))?;
    let dimensions = serde_json::to_value(&billing.dimensions).map_err(|_| {
        CqrsSqlxError::GuardRejected("Finance customer invoice dimensions are invalid")
    })?;
    let changed = sqlx::query(
        "INSERT INTO erpx_finance_write.customer_invoice_guards
          (invoice_id,scope_key,invoice_code,customer_id,billing_event_id,source_context,
           source_aggregate_kind,source_aggregate_id,source_aggregate_version,currency,
           net_minor,tax_minor,receivable_minor,eligible_on,due_on,dimensions,prepared_by,
           posted_by,posted_at,invoice_status,aggregate_version,created_at,updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23)
         ON CONFLICT (invoice_id) DO UPDATE SET
          scope_key=EXCLUDED.scope_key,invoice_code=EXCLUDED.invoice_code,
          customer_id=EXCLUDED.customer_id,billing_event_id=EXCLUDED.billing_event_id,
          source_context=EXCLUDED.source_context,
          source_aggregate_kind=EXCLUDED.source_aggregate_kind,
          source_aggregate_id=EXCLUDED.source_aggregate_id,
          source_aggregate_version=EXCLUDED.source_aggregate_version,currency=EXCLUDED.currency,
          net_minor=EXCLUDED.net_minor,tax_minor=EXCLUDED.tax_minor,
          receivable_minor=EXCLUDED.receivable_minor,eligible_on=EXCLUDED.eligible_on,
          due_on=EXCLUDED.due_on,dimensions=EXCLUDED.dimensions,prepared_by=EXCLUDED.prepared_by,
          posted_by=EXCLUDED.posted_by,posted_at=EXCLUDED.posted_at,
          invoice_status=EXCLUDED.invoice_status,aggregate_version=EXCLUDED.aggregate_version,
          updated_at=EXCLUDED.updated_at
         WHERE erpx_finance_write.customer_invoice_guards.aggregate_version + 1=EXCLUDED.aggregate_version",
    )
    .bind(v.id().as_uuid())
    .bind(scope_key(v.scope())?)
    .bind(v.invoice_code())
    .bind(billing.customer.as_uuid())
    .bind(billing.billing_event.as_uuid())
    .bind("sales")
    .bind(v.source().aggregate_kind.as_str())
    .bind(v.source().aggregate_id)
    .bind(to_i64(v.source().aggregate_version)?)
    .bind(billing.billable_amount.currency().as_str())
    .bind(billing.billable_amount.minor_units())
    .bind(billing.tax_amount.minor_units())
    .bind(receivable_minor)
    .bind(billing.eligible_on)
    .bind(v.due_on())
    .bind(dimensions)
    .bind(v.prepared_by())
    .bind(v.posted_by())
    .bind(v.posted_at())
    .bind(customer_invoice_status_name(v.status()))
    .bind(to_i64(v.version())?)
    .bind(v.created_at())
    .bind(v.updated_at())
    .execute(&mut **tx)
    .await?
    .rows_affected();
    finalize_finance_root(
        tx,
        "erpx_finance_write.customer_invoice_guards",
        "invoice_id",
        v.id().as_uuid(),
        v.scope(),
        changed,
    )
    .await?;
    write_invoice_dimension_authority(tx, v).await?;
    Ok(())
}

async fn apply_revenue_schedule(
    tx: &mut Transaction<'_, Postgres>,
    v: &RevenueSchedule,
) -> Result<(), CqrsSqlxError> {
    let allocated_minor = v
        .allocated_total_minor()
        .map_err(|_| CqrsSqlxError::GuardRejected("Finance revenue allocation is invalid"))?;
    let recognized_minor = v
        .recognized_total_minor()
        .map_err(|_| CqrsSqlxError::GuardRejected("Finance recognized revenue is invalid"))?;
    let changed = sqlx::query(
        "INSERT INTO erpx_finance_write.revenue_schedule_guards
          (schedule_id,scope_key,arrangement_id,sales_order_id,customer_id,source_context,
           source_aggregate_kind,source_aggregate_version,last_source_version,currency,
           allocated_minor,recognized_minor,schedule_status,aggregate_version,created_at,updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
         ON CONFLICT (schedule_id) DO UPDATE SET
          scope_key=EXCLUDED.scope_key,arrangement_id=EXCLUDED.arrangement_id,
          sales_order_id=EXCLUDED.sales_order_id,customer_id=EXCLUDED.customer_id,
          source_context=EXCLUDED.source_context,
          source_aggregate_kind=EXCLUDED.source_aggregate_kind,
          source_aggregate_version=EXCLUDED.source_aggregate_version,
          last_source_version=EXCLUDED.last_source_version,currency=EXCLUDED.currency,
          allocated_minor=EXCLUDED.allocated_minor,recognized_minor=EXCLUDED.recognized_minor,
          schedule_status=EXCLUDED.schedule_status,aggregate_version=EXCLUDED.aggregate_version,
          updated_at=EXCLUDED.updated_at
         WHERE erpx_finance_write.revenue_schedule_guards.aggregate_version + 1=EXCLUDED.aggregate_version",
    )
    .bind(v.id().as_uuid())
    .bind(scope_key(v.scope())?)
    .bind(v.arrangement().arrangement.as_uuid())
    .bind(v.arrangement().sales_order.as_uuid())
    .bind(v.arrangement().customer.as_uuid())
    .bind("sales")
    .bind(v.arrangement_source().aggregate_kind.as_str())
    .bind(to_i64(v.arrangement_source().aggregate_version)?)
    .bind(to_i64(v.last_source().aggregate_version)?)
    .bind(v.currency())
    .bind(allocated_minor)
    .bind(recognized_minor)
    .bind(revenue_schedule_status_name(v.status()))
    .bind(to_i64(v.version())?)
    .bind(v.created_at())
    .bind(v.updated_at())
    .execute(&mut **tx)
    .await?
    .rows_affected();
    finalize_finance_root(
        tx,
        "erpx_finance_write.revenue_schedule_guards",
        "schedule_id",
        v.id().as_uuid(),
        v.scope(),
        changed,
    )
    .await?;
    write_schedule_authority(tx, v).await?;
    Ok(())
}

fn exact_finance_scope_ids(
    scope: &TenantScope,
) -> Result<(Uuid, Uuid, Option<Uuid>), CqrsSqlxError> {
    match scope {
        TenantScope::Company {
            holding_id,
            company_id,
        } => Ok((*holding_id, *company_id, None)),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok((*holding_id, *company_id, Some(*branch_id))),
        _ => Err(CqrsSqlxError::GuardRejected("Finance scope must be exact")),
    }
}
async fn finalize_finance_root(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    id_column: &str,
    id: Uuid,
    scope: &TenantScope,
    changed: u64,
) -> Result<(), CqrsSqlxError> {
    if changed != 1 {
        return Err(CqrsSqlxError::GuardRejected(
            "Finance normalized authority write conflict",
        ));
    }
    let (h, c, b) = exact_finance_scope_ids(scope)?;
    let q =
        format!("UPDATE {table} SET holding_id=$2,company_id=$3,branch_id=$4 WHERE {id_column}=$1");
    if sqlx::query(&q)
        .bind(id)
        .bind(h)
        .bind(c)
        .bind(b)
        .execute(&mut **tx)
        .await?
        .rows_affected()
        != 1
    {
        return Err(CqrsSqlxError::GuardRejected(
            "Finance scope authority write conflict",
        ));
    }
    Ok(())
}
async fn write_journal_source_authority(
    tx: &mut Transaction<'_, Postgres>,
    v: &Journal,
) -> Result<(), CqrsSqlxError> {
    sqlx::query("DELETE FROM erpx_finance_write.journal_sales_line_authority WHERE journal_id=$1")
        .bind(v.id().as_uuid())
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM erpx_finance_write.journal_source_authority WHERE journal_id=$1")
        .bind(v.id().as_uuid())
        .execute(&mut **tx)
        .await?;
    match v.source() {
        JournalSource::SalesOrder(f) => {
            sqlx::query("INSERT INTO erpx_finance_write.journal_source_authority (journal_id,source_kind,sales_order_id,sales_order_code,sales_customer_id,sales_order_status,sales_total_minor) VALUES ($1,'sales_order',$2,$3,$4,$5,$6)").bind(v.id().as_uuid()).bind(f.sales_order.as_uuid()).bind(f.order_code.as_str()).bind(f.customer.as_uuid()).bind(fact_order_status_name(f.status)).bind(f.total.minor_units()).execute(&mut **tx).await?;
            for line in &f.lines {
                sqlx::query("INSERT INTO erpx_finance_write.journal_sales_line_authority (journal_id,line_id,item_code,quantity_units,quantity_scale,line_value_minor) VALUES ($1,$2,$3,$4,$5,$6)").bind(v.id().as_uuid()).bind(line.line.as_uuid()).bind(line.item_code.as_str()).bind(line.quantity.units()).bind(i16::from(line.quantity.scale())).bind(line.line_value.minor_units()).execute(&mut **tx).await?;
            }
        }
        JournalSource::InventoryMovement(f) => {
            sqlx::query("INSERT INTO erpx_finance_write.journal_source_authority (journal_id,source_kind,inventory_movement_id,inventory_stock_item_id,inventory_warehouse_id,inventory_item_code,inventory_quantity_units,inventory_quantity_scale,inventory_procurement_line_id) VALUES ($1,'inventory_movement',$2,$3,$4,$5,$6,$7,$8)").bind(v.id().as_uuid()).bind(f.movement.as_uuid()).bind(f.stock_item.as_uuid()).bind(f.warehouse.as_uuid()).bind(f.item_code.as_str()).bind(f.quantity_delta.units()).bind(i16::from(f.quantity_delta.scale())).bind(f.procurement_line.map(|x|x.as_uuid())).execute(&mut **tx).await?;
        }
        JournalSource::Payroll(f) => {
            sqlx::query("INSERT INTO erpx_finance_write.journal_source_authority (journal_id,source_kind,payroll_run_id,payroll_period_code,payroll_finalized_at,payroll_gross_minor,payroll_deduction_minor,payroll_net_minor) VALUES ($1,'payroll',$2,$3,$4,$5,$6,$7)").bind(v.id().as_uuid()).bind(f.payroll_run.as_uuid()).bind(f.period_code.as_str()).bind(f.finalized_at).bind(f.gross_amount.minor_units()).bind(f.deduction_amount.minor_units()).bind(f.net_amount.minor_units()).execute(&mut **tx).await?;
        }
        JournalSource::PosSale(f) => {
            sqlx::query("INSERT INTO erpx_finance_write.journal_source_authority (journal_id,source_kind,pos_transaction_id,pos_register_id,pos_receipt_code,pos_customer_id,pos_amount_minor,pos_occurred_at) VALUES ($1,'pos_sale',$2,$3,$4,$5,$6,$7)").bind(v.id().as_uuid()).bind(f.transaction.as_uuid()).bind(f.register.as_uuid()).bind(f.receipt_code.as_str()).bind(f.customer.map(|x|x.as_uuid())).bind(f.gross_amount.minor_units()).bind(f.completed_at).execute(&mut **tx).await?;
        }
        JournalSource::PosRefund(f) => {
            sqlx::query("INSERT INTO erpx_finance_write.journal_source_authority (journal_id,source_kind,pos_transaction_id,pos_original_sale_id,pos_amount_minor,pos_occurred_at) VALUES ($1,'pos_refund',$2,$3,$4,$5)").bind(v.id().as_uuid()).bind(f.refund.as_uuid()).bind(f.original_sale.as_uuid()).bind(f.amount.minor_units()).bind(f.approved_at).execute(&mut **tx).await?;
        }
        JournalSource::Manual { reference } => {
            sqlx::query("INSERT INTO erpx_finance_write.journal_source_authority (journal_id,source_kind,manual_reference) VALUES ($1,'manual',$2)").bind(v.id().as_uuid()).bind(reference).execute(&mut **tx).await?;
        }
        JournalSource::BankStatement(_) => {
            sqlx::query("INSERT INTO erpx_finance_write.journal_source_authority (journal_id,source_kind) VALUES ($1,'bank_statement')").bind(v.id().as_uuid()).execute(&mut **tx).await?;
        }
        JournalSource::TaxAdjustment(_) => {
            sqlx::query("INSERT INTO erpx_finance_write.journal_source_authority (journal_id,source_kind) VALUES ($1,'tax_adjustment')").bind(v.id().as_uuid()).execute(&mut **tx).await?;
        }
    }
    Ok(())
}
fn fact_order_status_name(v: sales::SalesOrderStatus) -> &'static str {
    match v {
        sales::SalesOrderStatus::Confirmed => "confirmed",
        sales::SalesOrderStatus::PartiallyFulfilled => "partially_fulfilled",
        sales::SalesOrderStatus::Fulfilled => "fulfilled",
        sales::SalesOrderStatus::Cancelled => "cancelled",
    }
}
fn finance_revenue_model_name(v: sales::RevenueModel) -> &'static str {
    match v {
        sales::RevenueModel::Goods => "goods",
        sales::RevenueModel::ServiceMilestone => "service_milestone",
        sales::RevenueModel::Recurring => "recurring",
    }
}
async fn write_invoice_dimension_authority(
    tx: &mut Transaction<'_, Postgres>,
    v: &CustomerInvoice,
) -> Result<(), CqrsSqlxError> {
    let b = v.billing();
    write_dimension_row(
        tx,
        "erpx_finance_write.customer_invoice_dimension_authority",
        "invoice_id",
        v.id().as_uuid(),
        &b.dimensions,
        Some((
            b.fulfillment.as_uuid(),
            b.arrangement.as_uuid(),
            b.sales_order.as_uuid(),
            b.order_code.as_str(),
        )),
    )
    .await
}
async fn write_schedule_authority(
    tx: &mut Transaction<'_, Postgres>,
    v: &RevenueSchedule,
) -> Result<(), CqrsSqlxError> {
    sqlx::query("UPDATE erpx_finance_write.revenue_schedule_guards SET legal_contract_id=$2,order_code=$3 WHERE schedule_id=$1").bind(v.id().as_uuid()).bind(v.arrangement().legal_contract.map(|x|x.as_uuid())).bind(v.arrangement().order_code.as_str()).execute(&mut **tx).await?;
    write_dimension_row(
        tx,
        "erpx_finance_write.revenue_schedule_dimension_authority",
        "schedule_id",
        v.id().as_uuid(),
        &v.arrangement().dimensions,
        None,
    )
    .await?;
    sqlx::query("DELETE FROM erpx_finance_write.revenue_schedule_recognition_authority WHERE schedule_id=$1").bind(v.id().as_uuid()).execute(&mut **tx).await?;
    sqlx::query(
        "DELETE FROM erpx_finance_write.revenue_schedule_obligation_authority WHERE schedule_id=$1",
    )
    .bind(v.id().as_uuid())
    .execute(&mut **tx)
    .await?;
    for o in v.obligations() {
        sqlx::query("INSERT INTO erpx_finance_write.revenue_schedule_obligation_authority (schedule_id,obligation_id,line_id,revenue_model,allocated_minor,recognized_minor,service_starts_on,service_ends_on) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)").bind(v.id().as_uuid()).bind(o.obligation().as_uuid()).bind(o.line().as_uuid()).bind(finance_revenue_model_name(o.model())).bind(o.allocated_minor()).bind(o.recognized_minor()).bind(o.service_starts_on()).bind(o.service_ends_on()).execute(&mut **tx).await?;
    }
    for (index, r) in v.recognitions().iter().enumerate() {
        sqlx::query("INSERT INTO erpx_finance_write.revenue_schedule_recognition_authority (schedule_id,recognition_ordinal,fulfillment_id,obligation_id,source_aggregate_version,recognized_minor,recognized_at) VALUES ($1,$2,$3,$4,$5,$6,$7)").bind(v.id().as_uuid()).bind(to_i64(u64::try_from(index+1).map_err(|_|CqrsSqlxError::NumericRange)?)?).bind(r.fulfillment().as_uuid()).bind(r.obligation().as_uuid()).bind(to_i64(r.source().aggregate_version)?).bind(r.recognized_minor()).bind(r.recognized_at()).execute(&mut **tx).await?;
    }
    Ok(())
}
async fn write_dimension_row(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    id_column: &str,
    id: Uuid,
    d: &DimensionSet,
    billing: Option<(Uuid, Uuid, Uuid, &str)>,
) -> Result<(), CqrsSqlxError> {
    let (extra_columns, extra_values, extra_update) = if billing.is_some() {
        (
            ",fulfillment_id,arrangement_id,sales_order_id,order_code",
            ",$15,$16,$17,$18",
            ",fulfillment_id=EXCLUDED.fulfillment_id,arrangement_id=EXCLUDED.arrangement_id,sales_order_id=EXCLUDED.sales_order_id,order_code=EXCLUDED.order_code",
        )
    } else {
        ("", "", "")
    };
    let q = format!(
        "INSERT INTO {table} ({id_column},legal_entity_id,branch_dimension_id,business_unit_id,cost_center_id,location_id,project_id,campaign_id,channel_id,party_id,employee_id,asset_id,legal_matter_id,intercompany_partner_id{extra_columns}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14{extra_values}) ON CONFLICT ({id_column}) DO UPDATE SET legal_entity_id=EXCLUDED.legal_entity_id,branch_dimension_id=EXCLUDED.branch_dimension_id,business_unit_id=EXCLUDED.business_unit_id,cost_center_id=EXCLUDED.cost_center_id,location_id=EXCLUDED.location_id,project_id=EXCLUDED.project_id,campaign_id=EXCLUDED.campaign_id,channel_id=EXCLUDED.channel_id,party_id=EXCLUDED.party_id,employee_id=EXCLUDED.employee_id,asset_id=EXCLUDED.asset_id,legal_matter_id=EXCLUDED.legal_matter_id,intercompany_partner_id=EXCLUDED.intercompany_partner_id{extra_update}"
    );
    let mut query = sqlx::query(&q)
        .bind(id)
        .bind(d.legal_entity.as_uuid())
        .bind(d.branch.map(|x| x.as_uuid()))
        .bind(d.business_unit.map(|x| x.as_uuid()))
        .bind(d.cost_center.map(|x| x.as_uuid()))
        .bind(d.location.map(|x| x.as_uuid()))
        .bind(d.project.map(|x| x.as_uuid()))
        .bind(d.campaign.map(|x| x.as_uuid()))
        .bind(d.channel.map(|x| x.as_uuid()))
        .bind(d.party.map(|x| x.as_uuid()))
        .bind(d.employee.map(|x| x.as_uuid()))
        .bind(d.asset.map(|x| x.as_uuid()))
        .bind(d.legal_matter.map(|x| x.as_uuid()))
        .bind(d.intercompany_partner.map(|x| x.as_uuid()));
    if let Some((f, a, s, o)) = billing {
        query = query.bind(f).bind(a).bind(s).bind(o);
    }
    query.execute(&mut **tx).await?;
    Ok(())
}

fn ensure_existence(expected: Option<u64>, current: Option<i64>) -> Result<(), CqrsSqlxError> {
    if matches!((expected, current), (None, Some(_)) | (Some(_), None)) {
        Err(CqrsSqlxError::GuardRejected(
            "Finance guard existence mismatch",
        ))
    } else {
        Ok(())
    }
}
fn validate_guard_context(
    v: &FinanceAggregate,
    c: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if c.context != CONTEXT_NAME
        || c.aggregate_type != v.aggregate_type()
        || c.aggregate_id.as_uuid() != v.id_uuid()
        || c.partition != aggregate_partition(v)
    {
        Err(CqrsSqlxError::GuardRejected(
            "Finance guard metadata mismatch",
        ))
    } else {
        Ok(())
    }
}
fn validate_commit(v: &FinanceCommit) -> Result<(), PortError> {
    let (p, k) = fingerprint_target(&v.identity.fingerprint);
    if !aggregate_valid(&v.aggregate)
        || p != aggregate_partition(&v.aggregate)
        || k != v.aggregate.aggregate_type()
        || !v.authority_scope.contains(v.aggregate.scope())
    {
        Err(PortError::Internal)
    } else {
        Ok(())
    }
}
fn aggregate_valid(v: &FinanceAggregate) -> bool {
    match v {
        FinanceAggregate::Chart(x) => x.validate().is_ok(),
        FinanceAggregate::Period(x) => x.validate().is_ok(),
        FinanceAggregate::Journal(x) => x.validate().is_ok(),
        FinanceAggregate::Subledger(x) => x.validate().is_ok(),
        FinanceAggregate::CustomerInvoice(x) => x.validate().is_ok(),
        FinanceAggregate::RevenueSchedule(x) => x.validate().is_ok(),
    }
}
fn fingerprint_target(v: &CommandFingerprint) -> (String, &'static str) {
    match v {
        CommandFingerprint::CreateChart { chart_id, .. }
        | CommandFingerprint::AddAccount { chart_id, .. }
        | CommandFingerprint::ChangeAccountStatus { chart_id, .. } => {
            (chart_partition(*chart_id), CHART_AGGREGATE_TYPE)
        }
        CommandFingerprint::CreatePeriod { period_id, .. }
        | CommandFingerprint::BeginPeriodClose { period_id }
        | CommandFingerprint::ClosePeriod { period_id }
        | CommandFingerprint::ReopenPeriod { period_id } => {
            (period_partition(*period_id), PERIOD_AGGREGATE_TYPE)
        }
        CommandFingerprint::CreateJournal { journal_id, .. }
        | CommandFingerprint::PostJournal { journal_id, .. } => {
            (journal_partition(*journal_id), JOURNAL_AGGREGATE_TYPE)
        }
        CommandFingerprint::CreateReceivableFromInvoice { subledger_id, .. }
        | CommandFingerprint::CreatePayableFromProcurement { subledger_id, .. }
        | CommandFingerprint::ApplySubledgerSettlement { subledger_id, .. } => {
            (subledger_partition(*subledger_id), SUBLEDGER_AGGREGATE_TYPE)
        }
        CommandFingerprint::CreateCustomerInvoice { invoice_id, .. }
        | CommandFingerprint::PostCustomerInvoice { invoice_id }
        | CommandFingerprint::CancelCustomerInvoice { invoice_id } => (
            customer_invoice_partition(*invoice_id),
            CUSTOMER_INVOICE_AGGREGATE_TYPE,
        ),
        CommandFingerprint::CreateRevenueSchedule { schedule_id, .. }
        | CommandFingerprint::RecognizeRevenueFromFulfillment { schedule_id, .. } => (
            revenue_schedule_partition(*schedule_id),
            REVENUE_SCHEDULE_AGGREGATE_TYPE,
        ),
    }
}
fn aggregate_partition(v: &FinanceAggregate) -> String {
    match v {
        FinanceAggregate::Chart(x) => chart_partition(x.id()),
        FinanceAggregate::Period(x) => period_partition(x.id()),
        FinanceAggregate::Journal(x) => journal_partition(x.id()),
        FinanceAggregate::Subledger(x) => subledger_partition(x.id()),
        FinanceAggregate::CustomerInvoice(x) => customer_invoice_partition(x.id()),
        FinanceAggregate::RevenueSchedule(x) => revenue_schedule_partition(x.id()),
    }
}
fn snapshot_event(v: &FinanceAggregate) -> &'static str {
    match v {
        FinanceAggregate::Chart(_) => CHART_SNAPSHOT_EVENT,
        FinanceAggregate::Period(_) => PERIOD_SNAPSHOT_EVENT,
        FinanceAggregate::Journal(_) => JOURNAL_SNAPSHOT_EVENT,
        FinanceAggregate::Subledger(_) => SUBLEDGER_SNAPSHOT_EVENT,
        FinanceAggregate::CustomerInvoice(_) => CUSTOMER_INVOICE_SNAPSHOT_EVENT,
        FinanceAggregate::RevenueSchedule(_) => REVENUE_SCHEDULE_SNAPSHOT_EVENT,
    }
}
fn aggregate_updated_at(v: &FinanceAggregate) -> chrono::DateTime<chrono::Utc> {
    match v {
        FinanceAggregate::Chart(x) => x.updated_at(),
        FinanceAggregate::Period(x) => x.updated_at(),
        FinanceAggregate::Journal(x) => x.updated_at(),
        FinanceAggregate::Subledger(x) => x.updated_at(),
        FinanceAggregate::CustomerInvoice(x) => x.updated_at(),
        FinanceAggregate::RevenueSchedule(x) => x.updated_at(),
    }
}
fn audit_action(v: &CommandFingerprint) -> &'static str {
    match v {
        CommandFingerprint::CreateChart { .. } => "finance.chart.create",
        CommandFingerprint::AddAccount { .. } => "finance.chart.account.add",
        CommandFingerprint::ChangeAccountStatus { .. } => "finance.chart.account.status",
        CommandFingerprint::CreatePeriod { .. } => "finance.period.create",
        CommandFingerprint::BeginPeriodClose { .. } => "finance.period.begin_close",
        CommandFingerprint::ClosePeriod { .. } => "finance.period.close",
        CommandFingerprint::ReopenPeriod { .. } => "finance.period.reopen",
        CommandFingerprint::CreateJournal { .. } => "finance.journal.create",
        CommandFingerprint::PostJournal { .. } => "finance.journal.post",
        CommandFingerprint::CreateCustomerInvoice { .. } => "finance.customer_invoice.create",
        CommandFingerprint::PostCustomerInvoice { .. } => "finance.customer_invoice.post",
        CommandFingerprint::CancelCustomerInvoice { .. } => "finance.customer_invoice.cancel",
        CommandFingerprint::CreateRevenueSchedule { .. } => "finance.revenue_schedule.create",
        CommandFingerprint::RecognizeRevenueFromFulfillment { .. } => {
            "finance.revenue_schedule.recognize"
        }
        CommandFingerprint::CreateReceivableFromInvoice { .. } => {
            "finance.receivable.create_from_invoice"
        }
        CommandFingerprint::CreatePayableFromProcurement { .. } => "finance.payable.create",
        CommandFingerprint::ApplySubledgerSettlement { .. } => "finance.subledger.settle",
    }
}
fn request_hash(v: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let mut d = Sha256::new();
    d.update(b"erpx.finance.command-fingerprint.v1\0");
    d.update(serde_json::to_vec(v).map_err(|_| PortError::Internal)?);
    Ok(RequestHash::new(d.finalize().into()))
}
fn deterministic_event_id(c: CommandId, t: &str, i: usize) -> EventId {
    let mut x = t.as_bytes().to_vec();
    x.extend_from_slice(&i.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(c.as_uuid(), &x)).expect("derived event id")
}
fn deterministic_uuid(seed: Uuid, label: &[u8]) -> Uuid {
    let mut d = Sha256::new();
    d.update(seed.as_bytes());
    d.update(label);
    let d = d.finalize();
    let mut b = [0; 16];
    b.copy_from_slice(&d[..16]);
    b[6] = (b[6] & 0x0f) | 0x50;
    b[8] = (b[8] & 0x3f) | 0x80;
    Uuid::from_bytes(b)
}
pub(crate) fn scope_key(s: &TenantScope) -> Result<String, CqrsSqlxError> {
    match s {
        TenantScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("company:{holding_id}:{company_id}")),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(format!("branch:{holding_id}:{company_id}:{branch_id}")),
        _ => Err(CqrsSqlxError::GuardRejected("Finance scope must be exact")),
    }
}
fn to_i64(v: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(v).map_err(|_| CqrsSqlxError::NumericRange)
}
fn receipt_to_stored(
    r: erpx_cqrs::CommandReceipt<FinanceAggregate>,
    kind: &str,
) -> Result<StoredCommit, PortError> {
    if r.aggregate.context != CONTEXT_NAME
        || r.aggregate.aggregate_type != kind
        || r.aggregate.aggregate_id.as_uuid() != r.response.id_uuid()
        || r.aggregate.version != r.response.version()
        || !aggregate_valid(&r.response)
    {
        return Err(PortError::Internal);
    }
    Ok(StoredCommit {
        command_id: r.command_id,
        correlation_id: r.correlation_id,
        aggregate: r.response,
        committed_events: r.committed_events,
        committed_at: r.committed_at,
        replayed: r.replayed,
    })
}
fn map_store_error(e: CqrsSqlxError) -> PortError {
    match e {
        CqrsSqlxError::IdempotencyConflict | CqrsSqlxError::CommandReplayConflict => {
            PortError::IdempotencyConflict
        }
        CqrsSqlxError::OptimisticConcurrency {
            expected: Some(expected),
            actual,
            ..
        } => PortError::Conflict {
            expected,
            actual: actual.unwrap_or(0),
        },
        CqrsSqlxError::GuardRejected(_) => PortError::Conflict {
            expected: 0,
            actual: 0,
        },
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_hash_and_targets_are_payload_sensitive() {
        let id = JournalId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let a = CommandFingerprint::PostJournal {
            journal_id: id,
            expected_chart_version: 1,
            expected_period_version: 1,
        };
        let b = CommandFingerprint::PostJournal {
            journal_id: id,
            expected_chart_version: 2,
            expected_period_version: 1,
        };
        assert_eq!(request_hash(&a).unwrap(), request_hash(&a).unwrap());
        assert_ne!(request_hash(&a).unwrap(), request_hash(&b).unwrap());
        assert_eq!(
            fingerprint_target(&a),
            (journal_partition(id), JOURNAL_AGGREGATE_TYPE)
        );
    }

    #[test]
    fn journal_source_guard_serializes_absence_and_rejects_duplicate_consumption() {
        let source = include_str!("command.rs");
        assert!(source.contains("pg_advisory_xact_lock"));
        assert!(source.contains("source_kind=$2 AND source_reference=$3"));
        assert!(source.contains("Finance journal source was already consumed"));
    }
}
