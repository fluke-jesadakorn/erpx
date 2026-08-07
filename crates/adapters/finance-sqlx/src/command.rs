use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_finance::{
    AccountingPeriod, Chart, ChartId, CommandUnitOfWork, FinanceAggregate, FinanceCommit, Journal,
    JournalId, JournalSource, PortError, StoredCommit, SubledgerId, SubledgerItem, WriteRepository,
    ports::{CommandFingerprint, CommandIdentity, ReferenceVersion},
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    CHART_AGGREGATE_TYPE, CHART_SNAPSHOT_EVENT, CONTEXT_NAME, EVENT_VERSION,
    JOURNAL_AGGREGATE_TYPE, JOURNAL_SNAPSHOT_EVENT, PERIOD_AGGREGATE_TYPE, PERIOD_SNAPSHOT_EVENT,
    SUBLEDGER_AGGREGATE_TYPE, SUBLEDGER_SNAPSHOT_EVENT, chart_partition, journal_partition,
    period_partition,
    projection::{
        FinanceOutboxPayload, FinanceSnapshot, account_category_name, account_status_name,
        journal_source_identity, journal_status_name, normal_balance_name, period_status_name,
        statutory_tax_kind_name, subledger_identity, subledger_status_name,
    },
    subledger_partition,
};

#[derive(Debug, Clone)]
pub struct FinanceWriteRepository {
    store: CommandStore,
}

impl FinanceWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }

    async fn load(
        &self,
        authority: &TenantScope,
        partition: &str,
        kind: &str,
        id: Uuid,
    ) -> Result<Option<FinanceAggregate>, PortError> {
        let id = AggregateId::try_from_uuid(id).map_err(|_| PortError::Internal)?;
        let snapshot = self
            .store
            .load_snapshot::<FinanceAggregate>(CONTEXT_NAME, partition, kind, id)
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        if snapshot.version != snapshot.state.version()
            || snapshot.state.aggregate_type() != kind
            || snapshot.state.id_uuid() != id.as_uuid()
            || !aggregate_valid(&snapshot.state)
        {
            return Err(PortError::Internal);
        }
        if !authority.contains(snapshot.state.scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(snapshot.state))
    }
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
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
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
            .commit_guarded(command, &guard)
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

struct FinanceGuard {
    aggregate: FinanceAggregate,
    references: Vec<ReferenceVersion>,
}

#[async_trait]
impl AtomicCommandGuard for FinanceGuard {
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
        };
        ensure_existence(context.expected_version, current)
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
        }
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
    sqlx::query("INSERT INTO erpx_finance_write.chart_guards (chart_id,scope_key,chart_code,chart_name,currency,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (chart_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,chart_code=EXCLUDED.chart_code,chart_name=EXCLUDED.chart_name,currency=EXCLUDED.currency,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at").bind(v.id().as_uuid()).bind(scope_key(v.scope())?).bind(v.code()).bind(v.name()).bind(v.currency()).bind(to_i64(v.version())?).bind(v.created_at()).bind(v.updated_at()).execute(&mut **tx).await?;
    for account in v.accounts() {
        sqlx::query("INSERT INTO erpx_finance_write.account_guards (chart_id,account_id,account_code,account_name,category,normal_balance,parent_account_id,account_status) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (chart_id,account_id) DO UPDATE SET account_code=EXCLUDED.account_code,account_name=EXCLUDED.account_name,category=EXCLUDED.category,normal_balance=EXCLUDED.normal_balance,parent_account_id=EXCLUDED.parent_account_id,account_status=EXCLUDED.account_status").bind(v.id().as_uuid()).bind(account.account_id.as_uuid()).bind(&account.code).bind(&account.name).bind(account_category_name(account.category)).bind(normal_balance_name(account.normal_balance)).bind(account.parent_account_id.map(|id| id.as_uuid())).bind(account_status_name(account.status)).execute(&mut **tx).await?;
    }
    Ok(())
}

async fn apply_period(
    tx: &mut Transaction<'_, Postgres>,
    v: &AccountingPeriod,
) -> Result<(), CqrsSqlxError> {
    sqlx::query("INSERT INTO erpx_finance_write.period_guards (period_id,scope_key,period_code,starts_on,ends_on,period_status,close_requested_by,closed_by,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT (period_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,period_code=EXCLUDED.period_code,starts_on=EXCLUDED.starts_on,ends_on=EXCLUDED.ends_on,period_status=EXCLUDED.period_status,close_requested_by=EXCLUDED.close_requested_by,closed_by=EXCLUDED.closed_by,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at").bind(v.id().as_uuid()).bind(scope_key(v.scope())?).bind(v.code()).bind(v.starts_on()).bind(v.ends_on()).bind(period_status_name(v.status())).bind(v.close_requested_by()).bind(v.closed_by()).bind(to_i64(v.version())?).bind(v.created_at()).bind(v.updated_at()).execute(&mut **tx).await?;
    Ok(())
}

async fn apply_journal(
    tx: &mut Transaction<'_, Postgres>,
    v: &Journal,
) -> Result<(), CqrsSqlxError> {
    let (source_kind, source_reference) = journal_source_identity(v.source())
        .map_err(|_| CqrsSqlxError::GuardRejected("Finance journal source is invalid"))?;
    sqlx::query("INSERT INTO erpx_finance_write.journal_guards (journal_id,scope_key,chart_id,period_id,journal_code,entry_date,currency,source_kind,source_reference,description,debit_total_minor,credit_total_minor,journal_status,posted_at,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) ON CONFLICT (journal_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,chart_id=EXCLUDED.chart_id,period_id=EXCLUDED.period_id,journal_code=EXCLUDED.journal_code,entry_date=EXCLUDED.entry_date,currency=EXCLUDED.currency,source_kind=EXCLUDED.source_kind,source_reference=EXCLUDED.source_reference,description=EXCLUDED.description,debit_total_minor=EXCLUDED.debit_total_minor,credit_total_minor=EXCLUDED.credit_total_minor,journal_status=EXCLUDED.journal_status,posted_at=EXCLUDED.posted_at,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at").bind(v.id().as_uuid()).bind(scope_key(v.scope())?).bind(v.chart_id().as_uuid()).bind(v.period_id().as_uuid()).bind(v.journal_code()).bind(v.entry_date()).bind(v.currency()).bind(source_kind).bind(source_reference).bind(v.description()).bind(v.debit_total_minor()).bind(v.credit_total_minor()).bind(journal_status_name(v.status())).bind(v.posted_at()).bind(to_i64(v.version())?).bind(v.created_at()).bind(v.updated_at()).execute(&mut **tx).await?;
    for line in v.lines() {
        sqlx::query("INSERT INTO erpx_finance_write.journal_line_guards (journal_id,chart_id,line_id,account_id,debit_minor,credit_minor,currency,memo) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (journal_id,line_id) DO UPDATE SET chart_id=EXCLUDED.chart_id,account_id=EXCLUDED.account_id,debit_minor=EXCLUDED.debit_minor,credit_minor=EXCLUDED.credit_minor,currency=EXCLUDED.currency,memo=EXCLUDED.memo").bind(v.id().as_uuid()).bind(v.chart_id().as_uuid()).bind(line.line_id.as_uuid()).bind(line.account_id.as_uuid()).bind(line.debit_minor).bind(line.credit_minor).bind(&line.currency).bind(&line.memo).execute(&mut **tx).await?;
    }
    match v.source() {
        JournalSource::BankStatement(fact) => compare_bank_fact(tx, v.id(), fact).await?,
        JournalSource::TaxAdjustment(fact) => compare_tax_fact(tx, v.id(), fact).await?,
        _ => {}
    }
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
    sqlx::query("INSERT INTO erpx_finance_write.subledger_guards (subledger_id,scope_key,subledger_kind,party_id,document_code,source_reference,currency,original_minor,outstanding_minor,subledger_status,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT (subledger_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,subledger_kind=EXCLUDED.subledger_kind,party_id=EXCLUDED.party_id,document_code=EXCLUDED.document_code,source_reference=EXCLUDED.source_reference,currency=EXCLUDED.currency,original_minor=EXCLUDED.original_minor,outstanding_minor=EXCLUDED.outstanding_minor,subledger_status=EXCLUDED.subledger_status,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at").bind(v.id().as_uuid()).bind(scope_key(v.scope())?).bind(kind).bind(party).bind(v.document_code()).bind(v.source_reference()).bind(v.currency()).bind(v.original_minor()).bind(v.outstanding_minor()).bind(subledger_status_name(v.status())).bind(to_i64(v.version())?).bind(v.created_at()).bind(v.updated_at()).execute(&mut **tx).await?;
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
        CommandFingerprint::CreateReceivableFromSales { subledger_id, .. }
        | CommandFingerprint::CreatePayableFromProcurement { subledger_id, .. }
        | CommandFingerprint::ApplySubledgerSettlement { subledger_id, .. } => {
            (subledger_partition(*subledger_id), SUBLEDGER_AGGREGATE_TYPE)
        }
    }
}
fn aggregate_partition(v: &FinanceAggregate) -> String {
    match v {
        FinanceAggregate::Chart(x) => chart_partition(x.id()),
        FinanceAggregate::Period(x) => period_partition(x.id()),
        FinanceAggregate::Journal(x) => journal_partition(x.id()),
        FinanceAggregate::Subledger(x) => subledger_partition(x.id()),
    }
}
fn snapshot_event(v: &FinanceAggregate) -> &'static str {
    match v {
        FinanceAggregate::Chart(_) => CHART_SNAPSHOT_EVENT,
        FinanceAggregate::Period(_) => PERIOD_SNAPSHOT_EVENT,
        FinanceAggregate::Journal(_) => JOURNAL_SNAPSHOT_EVENT,
        FinanceAggregate::Subledger(_) => SUBLEDGER_SNAPSHOT_EVENT,
    }
}
fn aggregate_updated_at(v: &FinanceAggregate) -> chrono::DateTime<chrono::Utc> {
    match v {
        FinanceAggregate::Chart(x) => x.updated_at(),
        FinanceAggregate::Period(x) => x.updated_at(),
        FinanceAggregate::Journal(x) => x.updated_at(),
        FinanceAggregate::Subledger(x) => x.updated_at(),
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
        CommandFingerprint::CreateReceivableFromSales { .. } => "finance.receivable.create",
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
}
