//! Normalized holding-write persistence for the authoritative Chart of
//! Accounts slice. No transport or authorization decisions live here.

use std::collections::BTreeMap;

use chrono::Utc;
use erpx_domain::contracts::{
    ActorAttribution, ActorKind, AuthorityScope, CONTRACT_VERSION, CommandContext, EventEnvelope,
};
use erpx_domain::finance::{AccountKind, ChartOfAccounts};
use erpx_domain::finance_chart::{
    AccountCategory, AccountStatus, ChartCommandFacts, FinanceAccount, FinanceChart,
    MAX_FINANCE_CHART_ACCOUNTS, MAX_FINANCE_IDEMPOTENCY_KEY_BYTES, NormalBalance,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::PersistenceError;
use crate::finance_persistence::begin_scoped;

pub const FINANCE_CHART_AGGREGATE: &str = "finance_chart_of_accounts";
pub const FINANCE_CHART_SAVED_EVENT: &str = "finance.chart_of_accounts_saved.v1";
const MAX_DATA_PROFILE_KEY_BYTES: usize = 255;
const MAX_DATABASE_VERSION: u64 = i64::MAX as u64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceChartSavedEvent {
    pub chart_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub accounts: Vec<FinanceAccount>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FinanceChartOutboxEvent {
    pub envelope: EventEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceChartWriteReceipt {
    pub chart_id: Uuid,
    pub version: u64,
    pub idempotency_key: String,
    pub event_id: Uuid,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub struct SqlxFinanceChartStore {
    write_pool: PgPool,
    data_profile_key: String,
}

impl SqlxFinanceChartStore {
    pub fn new(
        write_pool: PgPool,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let data_profile_key = data_profile_key.into();
        validate_data_profile_key(&data_profile_key)?;
        Ok(Self {
            write_pool,
            data_profile_key,
        })
    }

    pub fn data_profile_key(&self) -> &str {
        &self.data_profile_key
    }

    /// Atomically persists normalized chart state, exact idempotency binding,
    /// and a complete shared outbox envelope.
    pub async fn save_chart(
        &self,
        chart: &FinanceChart,
        context: &CommandContext,
        expected_version: Option<u64>,
    ) -> Result<FinanceChartWriteReceipt, PersistenceError> {
        validate_chart_command(chart, context, expected_version)?;
        let hash = request_hash(chart, expected_version)?;
        let mut tx = begin_scoped(&self.write_pool, &self.data_profile_key).await?;
        let mut locks = [
            format!("erpx.finance-chart:{}:{}", self.data_profile_key, chart.id),
            format!(
                "erpx.finance-chart-idempotency:{}:{}",
                self.data_profile_key, context.idempotency_key
            ),
        ];
        locks.sort_unstable();
        for lock in locks {
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(lock)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(receipt) = find_replay(
            &mut tx,
            &self.data_profile_key,
            &context.idempotency_key,
            &hash,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }

        let current = sqlx::query(
            "SELECT holding_id, company_id, version
               FROM erpx_holding.finance_chart_of_accounts
              WHERE data_profile_key=$1 AND chart_id=$2
              FOR UPDATE",
        )
        .bind(&self.data_profile_key)
        .bind(chart.id)
        .fetch_optional(&mut *tx)
        .await?;
        let actual_version = current
            .as_ref()
            .map(|row| as_u64(row.get::<i64, _>("version"), "Finance chart version"))
            .transpose()?;
        if actual_version != expected_version {
            return Err(concurrency(chart.id, expected_version, actual_version));
        }
        if let Some(row) = current.as_ref()
            && (row.get::<Uuid, _>("holding_id") != chart.holding_id
                || row.get::<Uuid, _>("company_id") != chart.company_id)
        {
            return Err(invalid("Finance chart holding/company scope is immutable"));
        }
        let required_version = expected_version
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| invalid("Finance chart version overflow"))?;
        if chart.version != required_version {
            return Err(concurrency(
                chart.id,
                Some(required_version),
                Some(chart.version),
            ));
        }

        let existing = load_account_rows(&mut tx, &self.data_profile_key, chart.id).await?;
        validate_persisted_transition(&mut tx, &self.data_profile_key, chart, &existing).await?;
        save_header(&mut tx, &self.data_profile_key, chart, expected_version).await?;
        save_accounts(&mut tx, &self.data_profile_key, chart).await?;

        let event = build_event(chart, context)?;
        append_event(&mut tx, &self.data_profile_key, &event).await?;
        let receipt = FinanceChartWriteReceipt {
            chart_id: chart.id,
            version: chart.version,
            idempotency_key: context.idempotency_key.clone(),
            event_id: event.envelope.event_id,
            replayed: false,
        };
        record_idempotency(&mut tx, &self.data_profile_key, &hash, &receipt).await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn load_chart(
        &self,
        chart_id: Uuid,
    ) -> Result<Option<FinanceChart>, PersistenceError> {
        if chart_id.is_nil() {
            return Err(invalid("Finance chart ID cannot be nil"));
        }
        let mut tx = begin_scoped(&self.write_pool, &self.data_profile_key).await?;
        let row = sqlx::query(
            "SELECT chart_id, holding_id, company_id, chart_name, version,
                    last_command_id, idempotency_key, actor_ref, correlation_id
               FROM erpx_holding.finance_chart_of_accounts
              WHERE data_profile_key=$1 AND chart_id=$2",
        )
        .bind(&self.data_profile_key)
        .bind(chart_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let version = as_u64(row.get("version"), "Finance chart version")?;
        let stale_account_count: i64 = sqlx::query_scalar(
            "SELECT count(*)
               FROM erpx_holding.finance_chart_accounts
              WHERE data_profile_key=$1 AND chart_id=$2 AND aggregate_version <> $3",
        )
        .bind(&self.data_profile_key)
        .bind(chart_id)
        .bind(as_i64(version, "Finance chart account version")?)
        .fetch_one(&mut *tx)
        .await?;
        if stale_account_count != 0 {
            return Err(invalid(format!(
                "Finance chart normalized accounts are not at authoritative chart version {version} (stale_rows={stale_account_count})"
            )));
        }
        let accounts = load_account_rows(&mut tx, &self.data_profile_key, chart_id)
            .await?
            .into_values()
            .collect();
        let chart = FinanceChart {
            id: row.get("chart_id"),
            holding_id: row.get("holding_id"),
            company_id: row.get("company_id"),
            name: row.get("chart_name"),
            accounts,
            version,
            last_command: ChartCommandFacts {
                command_id: row.get("last_command_id"),
                idempotency_key: row.get("idempotency_key"),
                actor_ref: row.get("actor_ref"),
                correlation_id: row.get("correlation_id"),
            },
        };
        chart
            .validate()
            .map_err(|error| invalid(format!("invalid persisted Finance chart: {error}")))?;
        tx.commit().await?;
        Ok(Some(chart))
    }
}

/// Mirror the legacy JSON chart writer into the normalized authoritative
/// source in the same transaction. Posting continues to read only the
/// normalized tables; the legacy payload is never used as a fallback source.
pub(crate) async fn mirror_legacy_chart_into_normalized(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    legacy: &ChartOfAccounts,
    expected_version: Option<u64>,
    version: u64,
) -> Result<(), PersistenceError> {
    let context = legacy_compatibility_context(
        data_profile_key,
        legacy.id,
        legacy.holding_id,
        legacy.company_id,
        expected_version,
        version,
    )?;
    let chart = normalized_chart_from_legacy(legacy, version, &context)?;
    let current = sqlx::query(
        "SELECT holding_id, company_id, version
           FROM erpx_holding.finance_chart_of_accounts
          WHERE data_profile_key=$1 AND chart_id=$2
          FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(legacy.id)
    .fetch_optional(&mut **tx)
    .await?;
    let actual_version = current
        .as_ref()
        .map(|row| as_u64(row.get::<i64, _>("version"), "Finance chart version"))
        .transpose()?;
    if actual_version != expected_version {
        return Err(concurrency(legacy.id, expected_version, actual_version));
    }
    if let Some(row) = current.as_ref()
        && (row.get::<Uuid, _>("holding_id") != legacy.holding_id
            || row.get::<Uuid, _>("company_id") != legacy.company_id)
    {
        return Err(invalid("Finance chart holding/company scope is immutable"));
    }

    let existing = load_account_rows(tx, data_profile_key, legacy.id).await?;
    validate_persisted_transition(tx, data_profile_key, &chart, &existing).await?;
    for (account_id, previous) in &existing {
        if let Some(current) = chart.account(*account_id)
            && previous.status != current.status
        {
            return Err(invalid(
                "legacy Finance chart cannot change normalized account lifecycle",
            ));
        }
    }
    save_header(tx, data_profile_key, &chart, expected_version).await?;
    save_accounts(tx, data_profile_key, &chart).await?;
    let event = build_event(&chart, &context)?;
    append_event(tx, data_profile_key, &event).await?;
    let receipt = FinanceChartWriteReceipt {
        chart_id: chart.id,
        version: chart.version,
        idempotency_key: context.idempotency_key.clone(),
        event_id: event.envelope.event_id,
        replayed: false,
    };
    record_idempotency(
        tx,
        data_profile_key,
        &request_hash(&chart, expected_version)?,
        &receipt,
    )
    .await?;
    Ok(())
}

fn legacy_compatibility_context(
    data_profile_key: &str,
    chart_id: Uuid,
    holding_id: Uuid,
    company_id: Uuid,
    expected_version: Option<u64>,
    version: u64,
) -> Result<CommandContext, PersistenceError> {
    let key = format!("legacy-finance-chart:{data_profile_key}:{chart_id}:{version}");
    let command_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("{key}:command").as_bytes());
    let correlation_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("{key}:correlation").as_bytes(),
    );
    let mut context = CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "legacy-finance-store".into(),
            kind: ActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        AuthorityScope::Company {
            holding_id,
            company_id,
        },
        "finance.legacy.chart.compatibility",
    )
    .map_err(|error| invalid(format!("invalid legacy Finance chart context: {error}")))?;
    // Replace the generated transport IDs with deterministic facts.
    context.command_id = command_id;
    context.correlation_id = correlation_id;
    context.audit.correlation_id = correlation_id;
    context.expected_aggregate_version = expected_version;
    context.audit.module = Some("finance".into());
    Ok(context)
}

fn normalized_chart_from_legacy(
    legacy: &ChartOfAccounts,
    version: u64,
    context: &CommandContext,
) -> Result<FinanceChart, PersistenceError> {
    let accounts = legacy
        .accounts
        .iter()
        .map(|account| {
            let mut normalized = FinanceAccount::new(
                account.id,
                &account.code,
                &account.name,
                legacy_category(account.kind),
                legacy_normal_balance(account.kind),
                account.parent_id,
            )
            .map_err(|error| invalid(format!("invalid legacy Finance account: {error}")))?;
            normalized.status = if account.active {
                AccountStatus::Active
            } else {
                AccountStatus::Inactive
            };
            Ok(normalized)
        })
        .collect::<Result<Vec<_>, PersistenceError>>()?;
    let chart = FinanceChart {
        id: legacy.id,
        holding_id: legacy.holding_id,
        company_id: legacy.company_id,
        name: legacy.name.clone(),
        accounts,
        version,
        last_command: ChartCommandFacts {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            actor_ref: context.actor.principal_ref.clone(),
            correlation_id: context.correlation_id,
        },
    };
    chart
        .validate()
        .map_err(|error| invalid(format!("invalid normalized legacy Finance chart: {error}")))?;
    Ok(chart)
}

const fn legacy_category(kind: AccountKind) -> AccountCategory {
    match kind {
        AccountKind::Asset => AccountCategory::Asset,
        AccountKind::Liability => AccountCategory::Liability,
        AccountKind::Equity => AccountCategory::Equity,
        AccountKind::Revenue => AccountCategory::Revenue,
        AccountKind::Expense => AccountCategory::Expense,
    }
}

const fn legacy_normal_balance(kind: AccountKind) -> NormalBalance {
    match kind {
        AccountKind::Asset | AccountKind::Expense => NormalBalance::Debit,
        AccountKind::Liability | AccountKind::Equity | AccountKind::Revenue => {
            NormalBalance::Credit
        }
    }
}

async fn save_header(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    chart: &FinanceChart,
    expected_version: Option<u64>,
) -> Result<(), PersistenceError> {
    let affected = if let Some(expected) = expected_version {
        sqlx::query(
            "UPDATE erpx_holding.finance_chart_of_accounts
                SET chart_name=$3, version=$4, last_command_id=$5,
                    idempotency_key=$6, actor_ref=$7, correlation_id=$8,
                    updated_at=now()
              WHERE data_profile_key=$1 AND chart_id=$2 AND version=$9",
        )
        .bind(data_profile_key)
        .bind(chart.id)
        .bind(&chart.name)
        .bind(as_i64(chart.version, "Finance chart version")?)
        .bind(chart.last_command.command_id)
        .bind(&chart.last_command.idempotency_key)
        .bind(&chart.last_command.actor_ref)
        .bind(chart.last_command.correlation_id)
        .bind(as_i64(expected, "expected Finance chart version")?)
        .execute(&mut **tx)
        .await?
        .rows_affected()
    } else {
        sqlx::query(
            "INSERT INTO erpx_holding.finance_chart_of_accounts
                (data_profile_key, chart_id, holding_id, company_id, chart_name,
                 version, last_command_id, idempotency_key, actor_ref,
                 correlation_id)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
             ON CONFLICT DO NOTHING",
        )
        .bind(data_profile_key)
        .bind(chart.id)
        .bind(chart.holding_id)
        .bind(chart.company_id)
        .bind(&chart.name)
        .bind(as_i64(chart.version, "Finance chart version")?)
        .bind(chart.last_command.command_id)
        .bind(&chart.last_command.idempotency_key)
        .bind(&chart.last_command.actor_ref)
        .bind(chart.last_command.correlation_id)
        .execute(&mut **tx)
        .await?
        .rows_affected()
    };
    if affected != 1 {
        return Err(concurrency(chart.id, expected_version, None));
    }
    Ok(())
}

async fn save_accounts(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    chart: &FinanceChart,
) -> Result<(), PersistenceError> {
    for account in &chart.accounts {
        sqlx::query(
            "INSERT INTO erpx_holding.finance_chart_accounts
                (data_profile_key, chart_id, holding_id, company_id, account_id,
                 account_code, account_name, category, normal_balance,
                 parent_account_id, status, aggregate_version)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
             ON CONFLICT (data_profile_key, chart_id, account_id) DO UPDATE SET
                 account_code=EXCLUDED.account_code,
                 account_name=EXCLUDED.account_name,
                 category=EXCLUDED.category,
                 normal_balance=EXCLUDED.normal_balance,
                 parent_account_id=EXCLUDED.parent_account_id,
                 status=EXCLUDED.status,
                 aggregate_version=EXCLUDED.aggregate_version,
                 updated_at=now()",
        )
        .bind(data_profile_key)
        .bind(chart.id)
        .bind(chart.holding_id)
        .bind(chart.company_id)
        .bind(account.id)
        .bind(&account.code)
        .bind(&account.name)
        .bind(category(account.category))
        .bind(normal_balance(account.normal_balance))
        .bind(account.parent_id)
        .bind(status(account.status))
        .bind(as_i64(chart.version, "Finance chart account version")?)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn load_account_rows(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    chart_id: Uuid,
) -> Result<BTreeMap<Uuid, FinanceAccount>, PersistenceError> {
    let rows = sqlx::query(
        "SELECT account_id, account_code, account_name, category,
                normal_balance, parent_account_id, status
           FROM erpx_holding.finance_chart_accounts
          WHERE data_profile_key=$1 AND chart_id=$2
          ORDER BY account_id",
    )
    .bind(data_profile_key)
    .bind(chart_id)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|row| {
            let account = FinanceAccount {
                id: row.get("account_id"),
                code: row.get("account_code"),
                name: row.get("account_name"),
                category: parse_category(&row.get::<String, _>("category"))?,
                normal_balance: parse_normal_balance(&row.get::<String, _>("normal_balance"))?,
                parent_id: row.get("parent_account_id"),
                status: parse_status(&row.get::<String, _>("status"))?,
            };
            account
                .validate()
                .map_err(|error| invalid(format!("invalid persisted Finance account: {error}")))?;
            Ok((account.id, account))
        })
        .collect()
}

async fn validate_persisted_transition(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    chart: &FinanceChart,
    existing: &BTreeMap<Uuid, FinanceAccount>,
) -> Result<(), PersistenceError> {
    if existing.len() > chart.accounts.len() {
        return Err(invalid("Finance chart accounts cannot be removed"));
    }
    let incoming: BTreeMap<_, _> = chart
        .accounts
        .iter()
        .map(|account| (account.id, account))
        .collect();
    for (account_id, previous) in existing {
        let current = incoming
            .get(account_id)
            .ok_or_else(|| invalid("Finance chart accounts cannot be removed"))?;
        if structural_change(previous, current)
            && account_has_postings(tx, data_profile_key, chart.company_id, *account_id).await?
        {
            return Err(invalid(format!(
                "posted/used Finance account {account_id} is structurally immutable"
            )));
        }
    }
    Ok(())
}

async fn account_has_postings(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    company_id: Uuid,
    account_id: Uuid,
) -> Result<bool, PersistenceError> {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
               FROM erpx_holding.finance_posted_journals journal
               CROSS JOIN LATERAL jsonb_array_elements(
                   COALESCE(journal.payload #> '{entry,lines}', '[]'::jsonb)
               ) AS line
              WHERE journal.data_profile_key=$1
                AND journal.company_id=$2
                AND line ->> 'account_id'=$3
         )",
    )
    .bind(data_profile_key)
    .bind(company_id)
    .bind(account_id.to_string())
    .fetch_one(&mut **tx)
    .await
    .map_err(PersistenceError::from)
}

fn structural_change(previous: &FinanceAccount, current: &FinanceAccount) -> bool {
    previous.code != current.code
        || previous.name != current.name
        || previous.category != current.category
        || previous.normal_balance != current.normal_balance
        || previous.parent_id != current.parent_id
}

fn build_event(
    chart: &FinanceChart,
    context: &CommandContext,
) -> Result<FinanceChartOutboxEvent, PersistenceError> {
    let payload = serde_json::to_value(FinanceChartSavedEvent {
        chart_id: chart.id,
        holding_id: chart.holding_id,
        company_id: chart.company_id,
        name: chart.name.clone(),
        accounts: chart.accounts.clone(),
    })?;
    Ok(FinanceChartOutboxEvent {
        envelope: EventEnvelope {
            contract_version: context.contract_version,
            event_id: Uuid::new_v4(),
            event_type: FINANCE_CHART_SAVED_EVENT.into(),
            event_version: 1,
            occurred_at: Utc::now(),
            aggregate_type: FINANCE_CHART_AGGREGATE.into(),
            aggregate_id: chart.id,
            aggregate_version: chart.version,
            active_scope: context.active_scope.clone(),
            actor: context.actor.clone(),
            correlation_id: context.correlation_id,
            causation_id: context.causation_id,
            idempotency_key: Some(context.idempotency_key.clone()),
            payload,
        },
    })
}

async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    event: &FinanceChartOutboxEvent,
) -> Result<(), PersistenceError> {
    let envelope = &event.envelope;
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key,
             aggregate_version, event_type, payload, occurred_at,
             contract_version, event_version, active_scope, actor,
             correlation_id, causation_id, idempotency_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
    )
    .bind(envelope.event_id)
    .bind(&envelope.aggregate_type)
    .bind(envelope.aggregate_id)
    .bind(data_profile_key)
    .bind(as_i64(
        envelope.aggregate_version,
        "Finance chart event aggregate version",
    )?)
    .bind(&envelope.event_type)
    .bind(&envelope.payload)
    .bind(envelope.occurred_at)
    .bind(i32::from(envelope.contract_version))
    .bind(i32::from(envelope.event_version))
    .bind(serde_json::to_value(&envelope.active_scope)?)
    .bind(serde_json::to_value(&envelope.actor)?)
    .bind(envelope.correlation_id)
    .bind(envelope.causation_id)
    .bind(&envelope.idempotency_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_replay_evidence(
    idempotency_key: &str,
    chart_id: Uuid,
    version: u64,
    source_version: u64,
    source_idempotency_key: &str,
    outbox_aggregate_type: String,
    outbox_aggregate_id: Uuid,
    outbox_version: u64,
    outbox_event_type: String,
    outbox_idempotency_key: Option<String>,
) -> Result<(), PersistenceError> {
    if chart_id.is_nil()
        || source_version != version
        || source_idempotency_key != idempotency_key
        || outbox_aggregate_type != FINANCE_CHART_AGGREGATE
        || outbox_aggregate_id != chart_id
        || outbox_version != version
        || outbox_event_type != FINANCE_CHART_SAVED_EVENT
        || outbox_idempotency_key.as_deref() != Some(idempotency_key)
    {
        return Err(invalid(
            "Finance chart replay source and outbox evidence does not match",
        ));
    }
    Ok(())
}

async fn find_replay(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    idempotency_key: &str,
    hash: &str,
) -> Result<Option<FinanceChartWriteReceipt>, PersistenceError> {
    let row = sqlx::query(
        "SELECT request_hash, chart_id, aggregate_version, event_id
           FROM erpx_holding.finance_chart_command_idempotency
          WHERE data_profile_key=$1 AND idempotency_key=$2
          FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(idempotency_key)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else { return Ok(None) };
    if row.get::<String, _>("request_hash") != hash {
        return Err(PersistenceError::IdempotencyConflict {
            key: idempotency_key.into(),
        });
    }
    let chart_id: Uuid = row.get("chart_id");
    let version = as_u64(row.get("aggregate_version"), "Finance chart replay version")?;
    let event_id: Uuid = row.get("event_id");
    let source = sqlx::query(
        "SELECT version, idempotency_key
           FROM erpx_holding.finance_chart_of_accounts
          WHERE data_profile_key=$1 AND chart_id=$2
          FOR SHARE",
    )
    .bind(data_profile_key)
    .bind(chart_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| invalid("Finance chart replay is missing authoritative source state"))?;
    let source_version = as_u64(source.get("version"), "Finance chart source version")?;
    let source_idempotency_key: String = source.get("idempotency_key");
    let outbox = sqlx::query(
        "SELECT aggregate_type, aggregate_id, aggregate_version,
                event_type, idempotency_key
           FROM erpx_holding.outbox_events
          WHERE data_profile_key=$1 AND event_id=$2
          FOR SHARE",
    )
    .bind(data_profile_key)
    .bind(event_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| invalid("Finance chart replay is missing authoritative outbox evidence"))?;
    validate_replay_evidence(
        idempotency_key,
        chart_id,
        version,
        source_version,
        &source_idempotency_key,
        outbox.get("aggregate_type"),
        outbox.get("aggregate_id"),
        as_u64(
            outbox.get("aggregate_version"),
            "Finance chart replay outbox version",
        )?,
        outbox.get("event_type"),
        outbox.get("idempotency_key"),
    )?;
    Ok(Some(FinanceChartWriteReceipt {
        chart_id,
        version,
        idempotency_key: idempotency_key.into(),
        event_id,
        replayed: true,
    }))
}

async fn record_idempotency(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    hash: &str,
    receipt: &FinanceChartWriteReceipt,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_holding.finance_chart_command_idempotency
            (data_profile_key, idempotency_key, request_hash, chart_id,
             aggregate_version, event_id)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(data_profile_key)
    .bind(&receipt.idempotency_key)
    .bind(hash)
    .bind(receipt.chart_id)
    .bind(as_i64(
        receipt.version,
        "Finance chart idempotency version",
    )?)
    .bind(receipt.event_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn validate_chart_command(
    chart: &FinanceChart,
    context: &CommandContext,
    expected_version: Option<u64>,
) -> Result<(), PersistenceError> {
    chart
        .validate()
        .map_err(|error| invalid(format!("invalid Finance chart: {error}")))?;
    context
        .validate()
        .map_err(|error| invalid(format!("invalid Finance chart command context: {error}")))?;
    if context.contract_version != CONTRACT_VERSION {
        return Err(invalid("unsupported Finance chart contract version"));
    }
    if context.command_id.is_nil() || context.correlation_id.is_nil() {
        return Err(invalid("Finance chart command identifiers cannot be nil"));
    }
    validate_bounded(
        &context.idempotency_key,
        "Finance chart idempotency key",
        MAX_FINANCE_IDEMPOTENCY_KEY_BYTES,
    )?;
    validate_bounded(&context.actor.principal_ref, "Finance chart actor", 255)?;
    let scope = AuthorityScope::Company {
        holding_id: chart.holding_id,
        company_id: chart.company_id,
    };
    if !context.active_scope.contains(&scope) {
        return Err(invalid(
            "Finance chart command scope does not contain chart company",
        ));
    }
    if context.expected_aggregate_version != expected_version {
        return Err(invalid(
            "Finance chart context expected version does not match request",
        ));
    }
    if chart.last_command.command_id != context.command_id
        || chart.last_command.idempotency_key != context.idempotency_key
        || chart.last_command.actor_ref != context.actor.principal_ref
        || chart.last_command.correlation_id != context.correlation_id
    {
        return Err(invalid(
            "Finance chart command facts do not match command context",
        ));
    }
    if chart.accounts.len() > MAX_FINANCE_CHART_ACCOUNTS {
        return Err(invalid("Finance chart account limit exceeded"));
    }
    if chart.version == 0 || chart.version > MAX_DATABASE_VERSION {
        return Err(invalid("Finance chart version must fit a positive BIGINT"));
    }
    Ok(())
}

fn request_hash(
    chart: &FinanceChart,
    expected_version: Option<u64>,
) -> Result<String, PersistenceError> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&(
        FINANCE_CHART_AGGREGATE,
        chart.id,
        chart,
        expected_version,
    ))?)))
}

fn validate_data_profile_key(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > MAX_DATA_PROFILE_KEY_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(PersistenceError::Configuration(
            "holding reference must be bounded and non-empty",
        ));
    }
    Ok(())
}

fn validate_bounded(value: &str, field: &str, max: usize) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(invalid(format!("{field} must be bounded and non-empty")));
    }
    Ok(())
}

fn as_i64(value: u64, field: &str) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| invalid(format!("{field} exceeds BIGINT")))
}

fn as_u64(value: i64, field: &str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| invalid(format!("{field} is negative")))
}

fn concurrency(aggregate_id: Uuid, expected: Option<u64>, actual: Option<u64>) -> PersistenceError {
    PersistenceError::OptimisticConcurrency {
        aggregate_type: FINANCE_CHART_AGGREGATE.into(),
        aggregate_id,
        expected,
        actual,
    }
}

fn invalid(message: impl Into<String>) -> PersistenceError {
    PersistenceError::InvalidData(message.into())
}

const fn category(value: AccountCategory) -> &'static str {
    match value {
        AccountCategory::Asset => "asset",
        AccountCategory::Liability => "liability",
        AccountCategory::Equity => "equity",
        AccountCategory::Revenue => "revenue",
        AccountCategory::Expense => "expense",
    }
}

fn parse_category(value: &str) -> Result<AccountCategory, PersistenceError> {
    match value {
        "asset" => Ok(AccountCategory::Asset),
        "liability" => Ok(AccountCategory::Liability),
        "equity" => Ok(AccountCategory::Equity),
        "revenue" => Ok(AccountCategory::Revenue),
        "expense" => Ok(AccountCategory::Expense),
        _ => Err(invalid("invalid persisted Finance account category")),
    }
}

const fn normal_balance(value: NormalBalance) -> &'static str {
    match value {
        NormalBalance::Debit => "debit",
        NormalBalance::Credit => "credit",
    }
}

fn parse_normal_balance(value: &str) -> Result<NormalBalance, PersistenceError> {
    match value {
        "debit" => Ok(NormalBalance::Debit),
        "credit" => Ok(NormalBalance::Credit),
        _ => Err(invalid("invalid persisted Finance normal balance")),
    }
}

const fn status(value: AccountStatus) -> &'static str {
    match value {
        AccountStatus::Active => "active",
        AccountStatus::Inactive => "inactive",
    }
}

fn parse_status(value: &str) -> Result<AccountStatus, PersistenceError> {
    match value {
        "active" => Ok(AccountStatus::Active),
        "inactive" => Ok(AccountStatus::Inactive),
        _ => Err(invalid("invalid persisted Finance account status")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_database_vocabulary_and_hash_are_stable() {
        assert_eq!(category(AccountCategory::Asset), "asset");
        assert_eq!(normal_balance(NormalBalance::Credit), "credit");
        assert_eq!(status(AccountStatus::Inactive), "inactive");
        assert_eq!(
            parse_category("unknown")
                .expect_err("closed category")
                .to_string(),
            "invalid persisted value: invalid persisted Finance account category"
        );
    }

    #[tokio::test]
    async fn data_profile_reference_is_bounded() {
        let pool = PgPool::connect_lazy("postgres://unused").expect("lazy pool");
        assert!(SqlxFinanceChartStore::new(pool.clone(), " ").is_err());
        assert!(SqlxFinanceChartStore::new(pool, "holding-finance").is_ok());
    }

    #[test]
    fn replay_requires_matching_source_and_outbox_evidence() {
        let chart_id = Uuid::from_u128(1);
        let evidence = |source_version: u64,
                        outbox_aggregate_type: &str,
                        outbox_version: u64,
                        outbox_idempotency_key: Option<&str>| {
            validate_replay_evidence(
                "chart-command-1",
                chart_id,
                2,
                source_version,
                "chart-command-1",
                outbox_aggregate_type.into(),
                chart_id,
                outbox_version,
                FINANCE_CHART_SAVED_EVENT.into(),
                outbox_idempotency_key.map(str::to_owned),
            )
        };

        assert!(evidence(2, FINANCE_CHART_AGGREGATE, 2, Some("chart-command-1")).is_ok());
        assert!(matches!(
            evidence(1, FINANCE_CHART_AGGREGATE, 2, Some("chart-command-1")),
            Err(PersistenceError::InvalidData(message))
                if message.contains("source and outbox evidence")
        ));
        assert!(matches!(
            evidence(2, FINANCE_CHART_AGGREGATE, 3, Some("chart-command-1")),
            Err(PersistenceError::InvalidData(message))
                if message.contains("source and outbox evidence")
        ));
    }
}
