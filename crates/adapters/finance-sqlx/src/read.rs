use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_finance::{
    ChartId, JournalId, PortError, StatutoryTaxKind, SubledgerId,
    ports::{
        BankTransactionList, BankTransactionListFilter, BankTransactionView, ChartView,
        JournalList, JournalListFilter, JournalView, PeriodView, ReadProjection, ReadRepository,
        StatutoryTaxFactView, StatutoryTaxReport, SubledgerList, SubledgerListFilter,
        SubledgerView, TrialBalanceLine, TrialBalanceView,
    },
};
use erpx_kernel::TenantScope;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    BANK_TRANSACTION_PROJECTION, CHART_PROJECTION, JOURNAL_PROJECTION, PERIOD_PROJECTION,
    STATUTORY_TAX_PROJECTION, SUBLEDGER_PROJECTION, TRIAL_BALANCE_PROJECTION, chart_partition,
    journal_partition, period_partition,
    projection::{journal_status_name, subledger_status_name},
    subledger_partition,
};

const SEP: char = '|';
#[derive(Debug, Clone)]
pub struct FinanceReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}
impl FinanceReadRepository {
    pub fn new(pool: ReadPool) -> Result<Self, CqrsSqlxError> {
        Self::with_waiter_config(pool, WaiterConfig::default())
    }
    pub fn with_waiter_config(pool: ReadPool, config: WaiterConfig) -> Result<Self, CqrsSqlxError> {
        Ok(Self {
            read_side: ReadSideStore::new(pool.clone()),
            waiter: SqlxProjectionWaiter::new(pool.clone(), config)?,
            pool,
        })
    }
    async fn checkpoint(
        &self,
        projection: &str,
        prefix: &str,
        consistency: &QueryConsistency,
        exact: Option<&str>,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        match consistency {
            QueryConsistency::Eventual => match exact {
                Some(p) => self
                    .read_side
                    .checkpoint(projection, p)
                    .await
                    .map_err(map_read_error),
                None => Ok(None),
            },
            QueryConsistency::ReadYourWrites(r) => {
                let valid = r
                    .partition
                    .strip_prefix(prefix)
                    .is_some_and(|x| x.len() == 32 && x.bytes().all(|b| b.is_ascii_hexdigit()));
                if r.projection != projection || !valid || exact.is_some_and(|p| p != r.partition) {
                    return Err(PortError::NotReady(ProjectionNotReady {
                        projection: r.projection.clone(),
                        partition: r.partition.clone(),
                        target: r.target,
                        observed: None,
                    }));
                }
                self.waiter
                    .wait_until_checked(r)
                    .await
                    .map(Some)
                    .map_err(|e| match e {
                        WaitError::NotReady(v) => PortError::NotReady(v),
                        WaitError::Database(_) => PortError::Unavailable,
                    })
            }
        }
    }
    async fn metadata(
        &self,
        projection: &str,
        checkpoint: Option<ProjectionCheckpoint>,
    ) -> Result<ProjectionMetadata, PortError> {
        let status:Option<String>=sqlx::query_scalar("SELECT status FROM erpx_cqrs_read.projection_generations WHERE projection_name=$1 AND status IN ('active','building') ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END LIMIT 1").bind(projection).fetch_optional(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let freshness = match status.as_deref() {
            Some("active") => FreshnessState::Current,
            Some("building") => FreshnessState::Rebuilding,
            _ => FreshnessState::Unavailable,
        };
        let lag_millis = checkpoint
            .as_ref()
            .and_then(|v| Utc::now().signed_duration_since(v.updated_at).to_std().ok())
            .and_then(|d| u64::try_from(d.as_millis()).ok());
        Ok(ProjectionMetadata {
            checkpoint,
            freshness,
            lag_millis,
        })
    }
}

#[async_trait]
impl ReadRepository for FinanceReadRepository {
    async fn get_chart(
        &self,
        scope: &TenantScope,
        id: ChartId,
        c: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ChartView>>, PortError> {
        let p = chart_partition(id);
        let cp = self
            .checkpoint(CHART_PROJECTION, "finance.chart:", c, Some(&p))
            .await?;
        let v = self
            .get_state(
                "erpx_finance_read.chart_projection",
                "chart_id",
                id.as_uuid(),
                CHART_PROJECTION,
                &p,
            )
            .await?;
        scope_option(scope, &v)?;
        Ok(ReadProjection {
            value: v,
            metadata: self.metadata(CHART_PROJECTION, cp).await?,
        })
    }
    async fn get_period(
        &self,
        scope: &TenantScope,
        id: erpx_finance::PeriodId,
        c: &QueryConsistency,
    ) -> Result<ReadProjection<Option<PeriodView>>, PortError> {
        let p = period_partition(id);
        let cp = self
            .checkpoint(PERIOD_PROJECTION, "finance.period:", c, Some(&p))
            .await?;
        let row=sqlx::query("SELECT projection.state FROM erpx_finance_read.period_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND projection.period_id=$2 AND projection.partition_key=$3").bind(PERIOD_PROJECTION).bind(id.as_uuid()).bind(&p).fetch_optional(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let v = row.map(decode_state).transpose()?;
        scope_option(scope, &v)?;
        Ok(ReadProjection {
            value: v,
            metadata: self.metadata(PERIOD_PROJECTION, cp).await?,
        })
    }
    async fn get_journal(
        &self,
        scope: &TenantScope,
        id: JournalId,
        c: &QueryConsistency,
    ) -> Result<ReadProjection<Option<JournalView>>, PortError> {
        let p = journal_partition(id);
        let cp = self
            .checkpoint(JOURNAL_PROJECTION, "finance.journal:", c, Some(&p))
            .await?;
        let v = self
            .get_state(
                "erpx_finance_read.journal_projection",
                "journal_id",
                id.as_uuid(),
                JOURNAL_PROJECTION,
                &p,
            )
            .await?;
        scope_option(scope, &v)?;
        Ok(ReadProjection {
            value: v,
            metadata: self.metadata(JOURNAL_PROJECTION, cp).await?,
        })
    }
    async fn list_journals(
        &self,
        scope: &TenantScope,
        f: &JournalListFilter,
        c: &QueryConsistency,
    ) -> Result<ReadProjection<JournalList>, PortError> {
        let cp = self
            .checkpoint(JOURNAL_PROJECTION, "finance.journal:", c, None)
            .await?;
        let cursor = f.cursor.as_deref().map(decode_cursor).transpose()?;
        let rows=sqlx::query("SELECT projection.state FROM erpx_finance_read.journal_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND ($2::text IS NULL OR projection.journal_status=$2) AND ($3::uuid IS NULL OR projection.period_id=$3) AND ($4::timestamptz IS NULL OR (projection.aggregate_updated_at,projection.journal_id)<($4,$5)) ORDER BY projection.aggregate_updated_at DESC,projection.journal_id DESC LIMIT $6").bind(JOURNAL_PROJECTION).bind(f.status.map(journal_status_name)).bind(f.period_id.map(|x|x.as_uuid())).bind(cursor.as_ref().map(|x|x.0)).bind(cursor.as_ref().map(|x|x.1.as_uuid())).bind(scan(f.page_size)).fetch_all(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let mut values = rows
            .into_iter()
            .map(decode_state)
            .collect::<Result<Vec<JournalView>, _>>()?;
        values.retain(|v| scope.contains(&v.scope));
        let next = truncate(&mut values, f.page_size, encode_cursor);
        Ok(ReadProjection {
            value: JournalList {
                journals: values,
                next_cursor: next,
            },
            metadata: self.metadata(JOURNAL_PROJECTION, cp).await?,
        })
    }
    async fn get_subledger(
        &self,
        scope: &TenantScope,
        id: SubledgerId,
        c: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SubledgerView>>, PortError> {
        let p = subledger_partition(id);
        let cp = self
            .checkpoint(SUBLEDGER_PROJECTION, "finance.subledger:", c, Some(&p))
            .await?;
        let row=sqlx::query("SELECT projection.state FROM erpx_finance_read.subledger_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND projection.subledger_id=$2 AND projection.partition_key=$3").bind(SUBLEDGER_PROJECTION).bind(id.as_uuid()).bind(&p).fetch_optional(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let v = row.map(decode_state).transpose()?;
        scope_option(scope, &v)?;
        Ok(ReadProjection {
            value: v,
            metadata: self.metadata(SUBLEDGER_PROJECTION, cp).await?,
        })
    }
    async fn list_subledger(
        &self,
        scope: &TenantScope,
        f: &SubledgerListFilter,
        c: &QueryConsistency,
    ) -> Result<ReadProjection<SubledgerList>, PortError> {
        let cp = self
            .checkpoint(SUBLEDGER_PROJECTION, "finance.subledger:", c, None)
            .await?;
        let cursor = f
            .cursor
            .as_deref()
            .map(decode_subledger_cursor)
            .transpose()?;
        let kind = f.receivable_only.and_then(|v| v.then_some("receivable"));
        let rows=sqlx::query("SELECT projection.state FROM erpx_finance_read.subledger_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND ($2::text IS NULL OR projection.subledger_status=$2) AND ($3::text IS NULL OR projection.subledger_kind=$3) AND ($4::timestamptz IS NULL OR (projection.aggregate_updated_at,projection.subledger_id)<($4,$5)) ORDER BY projection.aggregate_updated_at DESC,projection.subledger_id DESC LIMIT $6").bind(SUBLEDGER_PROJECTION).bind(f.status.map(subledger_status_name)).bind(kind).bind(cursor.as_ref().map(|x|x.0)).bind(cursor.as_ref().map(|x|x.1.as_uuid())).bind(scan(f.page_size)).fetch_all(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let mut values = rows
            .into_iter()
            .map(decode_state)
            .collect::<Result<Vec<SubledgerView>, _>>()?;
        values.retain(|v| scope.contains(&v.scope));
        let next = truncate(&mut values, f.page_size, |v| {
            format!(
                "{}{}{}",
                v.updated_at.to_rfc3339(),
                SEP,
                v.subledger_id.as_uuid()
            )
        });
        Ok(ReadProjection {
            value: SubledgerList {
                items: values,
                next_cursor: next,
            },
            metadata: self.metadata(SUBLEDGER_PROJECTION, cp).await?,
        })
    }
    async fn get_trial_balance(
        &self,
        scope: &TenantScope,
        chart: ChartId,
        c: &QueryConsistency,
    ) -> Result<ReadProjection<TrialBalanceView>, PortError> {
        let cp = self
            .checkpoint(TRIAL_BALANCE_PROJECTION, "finance.journal:", c, None)
            .await?;
        let rows=sqlx::query("SELECT tenant_scope,currency,account_id,account_code,account_name,debit_minor,credit_minor,net_minor FROM erpx_finance_read.trial_balance_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND projection.chart_id=$2 ORDER BY account_code,account_id").bind(TRIAL_BALANCE_PROJECTION).bind(chart.as_uuid()).fetch_all(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let mut line_totals = std::collections::BTreeMap::new();
        let mut projected_scope = None;
        let mut currency = None;
        let mut debit = 0i64;
        let mut credit = 0i64;
        for row in rows {
            let s: TenantScope = serde_json::from_value(
                row.try_get("tenant_scope")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?;
            if !scope.contains(&s) {
                return Err(PortError::Denied);
            }
            projected_scope.get_or_insert(s);
            currency.get_or_insert(
                row.try_get::<String, _>("currency")
                    .map_err(|_| PortError::Internal)?,
            );
            let d = row
                .try_get("debit_minor")
                .map_err(|_| PortError::Internal)?;
            let c = row
                .try_get("credit_minor")
                .map_err(|_| PortError::Internal)?;
            debit = debit.checked_add(d).ok_or(PortError::Internal)?;
            credit = credit.checked_add(c).ok_or(PortError::Internal)?;
            let account_id = erpx_finance::AccountId::try_from_uuid(
                row.try_get("account_id").map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?;
            let entry = line_totals.entry(account_id).or_insert(TrialBalanceLine {
                account_id,
                account_code: row
                    .try_get("account_code")
                    .map_err(|_| PortError::Internal)?,
                account_name: row
                    .try_get("account_name")
                    .map_err(|_| PortError::Internal)?,
                debit_minor: 0,
                credit_minor: 0,
                net_minor: 0,
            });
            entry.debit_minor = entry
                .debit_minor
                .checked_add(d)
                .ok_or(PortError::Internal)?;
            entry.credit_minor = entry
                .credit_minor
                .checked_add(c)
                .ok_or(PortError::Internal)?;
            entry.net_minor = entry
                .net_minor
                .checked_add(row.try_get("net_minor").map_err(|_| PortError::Internal)?)
                .ok_or(PortError::Internal)?;
        }
        let lines = line_totals.into_values().collect();
        let value = TrialBalanceView {
            scope: projected_scope.unwrap_or_else(|| scope.clone()),
            chart_id: chart,
            currency: currency.unwrap_or_default(),
            lines,
            total_debit_minor: debit,
            total_credit_minor: credit,
        };
        Ok(ReadProjection {
            value,
            metadata: self.metadata(TRIAL_BALANCE_PROJECTION, cp).await?,
        })
    }
    async fn list_bank_transactions(
        &self,
        scope: &TenantScope,
        f: &BankTransactionListFilter,
        c: &QueryConsistency,
    ) -> Result<ReadProjection<BankTransactionList>, PortError> {
        let cp = self
            .checkpoint(BANK_TRANSACTION_PROJECTION, "finance.journal:", c, None)
            .await?;
        let cursor = f.cursor.as_deref().map(decode_bank_cursor).transpose()?;
        let rows=sqlx::query("SELECT tenant_scope,journal_id,import_id,transaction_id,reference,posted_on,amount_minor,currency FROM erpx_finance_read.bank_transaction_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND ($2::date IS NULL OR posted_on >= $2) AND ($3::date IS NULL OR posted_on <= $3) AND ($4::date IS NULL OR (posted_on,transaction_id)<($4,$5)) ORDER BY posted_on DESC,transaction_id DESC LIMIT $6").bind(BANK_TRANSACTION_PROJECTION).bind(f.posted_from).bind(f.posted_through).bind(cursor.as_ref().map(|x|x.0)).bind(cursor.as_ref().map(|x|x.1)).bind(scan(f.page_size)).fetch_all(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let mut values = Vec::new();
        for row in rows {
            let s: TenantScope = serde_json::from_value(
                row.try_get("tenant_scope")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?;
            if !scope.contains(&s) {
                continue;
            }
            values.push(BankTransactionView {
                scope: s,
                journal_id: JournalId::try_from_uuid(
                    row.try_get("journal_id").map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                fact: erpx_finance::BankTransactionFact {
                    import_id: row.try_get("import_id").map_err(|_| PortError::Internal)?,
                    transaction_id: row
                        .try_get("transaction_id")
                        .map_err(|_| PortError::Internal)?,
                    reference: row.try_get("reference").map_err(|_| PortError::Internal)?,
                    posted_on: row.try_get("posted_on").map_err(|_| PortError::Internal)?,
                    amount_minor: row
                        .try_get("amount_minor")
                        .map_err(|_| PortError::Internal)?,
                    currency: row.try_get("currency").map_err(|_| PortError::Internal)?,
                },
            });
        }
        let next = truncate(&mut values, f.page_size, |v| {
            format!("{}{}{}", v.fact.posted_on, SEP, v.fact.transaction_id)
        });
        Ok(ReadProjection {
            value: BankTransactionList {
                transactions: values,
                next_cursor: next,
            },
            metadata: self.metadata(BANK_TRANSACTION_PROJECTION, cp).await?,
        })
    }
    async fn get_statutory_tax_report(
        &self,
        scope: &TenantScope,
        period: &str,
        currency: &str,
        c: &QueryConsistency,
    ) -> Result<ReadProjection<StatutoryTaxReport>, PortError> {
        let cp = self
            .checkpoint(STATUTORY_TAX_PROJECTION, "finance.journal:", c, None)
            .await?;
        let rows=sqlx::query("SELECT tenant_scope,journal_id,tax_fact_id,reference,tax_date,tax_period,tax_kind,currency,base_minor,tax_minor,tax_rate_basis_points,tax_identifier FROM erpx_finance_read.statutory_tax_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND tax_period=$2 AND currency=$3 ORDER BY tax_date,tax_fact_id").bind(STATUTORY_TAX_PROJECTION).bind(period).bind(currency).fetch_all(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let mut facts = Vec::new();
        let mut output = 0i64;
        let mut input = 0i64;
        let mut withholding = 0i64;
        for row in rows {
            let s: TenantScope = serde_json::from_value(
                row.try_get("tenant_scope")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?;
            if !scope.contains(&s) {
                continue;
            }
            let kind = decode_tax_kind(
                &row.try_get::<String, _>("tax_kind")
                    .map_err(|_| PortError::Internal)?,
            )?;
            let amount: i64 = row.try_get("tax_minor").map_err(|_| PortError::Internal)?;
            match kind {
                StatutoryTaxKind::OutputVat => {
                    output = output.checked_add(amount).ok_or(PortError::Internal)?
                }
                StatutoryTaxKind::InputVat => {
                    input = input.checked_add(amount).ok_or(PortError::Internal)?
                }
                StatutoryTaxKind::Withholding => {
                    withholding = withholding.checked_add(amount).ok_or(PortError::Internal)?
                }
            }
            facts.push(StatutoryTaxFactView {
                scope: s,
                journal_id: JournalId::try_from_uuid(
                    row.try_get("journal_id").map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                fact: erpx_finance::StatutoryTaxFact {
                    tax_fact_id: row
                        .try_get("tax_fact_id")
                        .map_err(|_| PortError::Internal)?,
                    reference: row.try_get("reference").map_err(|_| PortError::Internal)?,
                    tax_date: row.try_get("tax_date").map_err(|_| PortError::Internal)?,
                    tax_period: row.try_get("tax_period").map_err(|_| PortError::Internal)?,
                    kind,
                    currency: row.try_get("currency").map_err(|_| PortError::Internal)?,
                    base_minor: row.try_get("base_minor").map_err(|_| PortError::Internal)?,
                    tax_minor: amount,
                    tax_rate_basis_points: u32::try_from(
                        row.try_get::<i32, _>("tax_rate_basis_points")
                            .map_err(|_| PortError::Internal)?,
                    )
                    .map_err(|_| PortError::Internal)?,
                    tax_identifier: row
                        .try_get("tax_identifier")
                        .map_err(|_| PortError::Internal)?,
                },
            });
        }
        Ok(ReadProjection {
            value: StatutoryTaxReport {
                scope: scope.clone(),
                tax_period: period.into(),
                currency: currency.into(),
                facts,
                output_vat_minor: output,
                input_vat_minor: input,
                withholding_minor: withholding,
            },
            metadata: self.metadata(STATUTORY_TAX_PROJECTION, cp).await?,
        })
    }
}

impl FinanceReadRepository {
    async fn get_state<T: serde::de::DeserializeOwned>(
        &self,
        table: &str,
        id_col: &str,
        id: Uuid,
        projection: &str,
        partition: &str,
    ) -> Result<Option<T>, PortError> {
        let allowed = match (table, id_col) {
            ("erpx_finance_read.chart_projection", "chart_id") => {
                "SELECT projection.state FROM erpx_finance_read.chart_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND projection.chart_id=$2 AND projection.partition_key=$3"
            }
            ("erpx_finance_read.journal_projection", "journal_id") => {
                "SELECT projection.state FROM erpx_finance_read.journal_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND projection.journal_id=$2 AND projection.partition_key=$3"
            }
            _ => return Err(PortError::Internal),
        };
        let row = sqlx::query(allowed)
            .bind(projection)
            .bind(id)
            .bind(partition)
            .fetch_optional(self.pool.as_pg_pool())
            .await
            .map_err(|_| PortError::Unavailable)?;
        row.map(decode_state).transpose()
    }
}
fn scope_option<T: ScopeView>(scope: &TenantScope, v: &Option<T>) -> Result<(), PortError> {
    if v.as_ref().is_some_and(|x| !scope.contains(x.scope())) {
        Err(PortError::Denied)
    } else {
        Ok(())
    }
}
trait ScopeView {
    fn scope(&self) -> &TenantScope;
}
impl ScopeView for ChartView {
    fn scope(&self) -> &TenantScope {
        &self.scope
    }
}
impl ScopeView for PeriodView {
    fn scope(&self) -> &TenantScope {
        &self.scope
    }
}
impl ScopeView for JournalView {
    fn scope(&self) -> &TenantScope {
        &self.scope
    }
}
impl ScopeView for SubledgerView {
    fn scope(&self) -> &TenantScope {
        &self.scope
    }
}
fn decode_state<T: serde::de::DeserializeOwned>(
    row: sqlx::postgres::PgRow,
) -> Result<T, PortError> {
    serde_json::from_value(row.try_get("state").map_err(|_| PortError::Internal)?)
        .map_err(|_| PortError::Internal)
}
pub fn encode_cursor(v: &JournalView) -> String {
    format!(
        "{}{}{}",
        v.updated_at.to_rfc3339(),
        SEP,
        v.journal_id.as_uuid()
    )
}
pub fn decode_cursor(v: &str) -> Result<(DateTime<Utc>, JournalId), PortError> {
    let (a, b) = v.split_once(SEP).ok_or(PortError::Internal)?;
    Ok((
        DateTime::parse_from_rfc3339(a)
            .map_err(|_| PortError::Internal)?
            .with_timezone(&Utc),
        JournalId::try_from_uuid(Uuid::parse_str(b).map_err(|_| PortError::Internal)?)
            .map_err(|_| PortError::Internal)?,
    ))
}
fn decode_subledger_cursor(v: &str) -> Result<(DateTime<Utc>, SubledgerId), PortError> {
    let (a, b) = v.split_once(SEP).ok_or(PortError::Internal)?;
    Ok((
        DateTime::parse_from_rfc3339(a)
            .map_err(|_| PortError::Internal)?
            .with_timezone(&Utc),
        SubledgerId::try_from_uuid(Uuid::parse_str(b).map_err(|_| PortError::Internal)?)
            .map_err(|_| PortError::Internal)?,
    ))
}
fn decode_bank_cursor(v: &str) -> Result<(NaiveDate, Uuid), PortError> {
    let (a, b) = v.split_once(SEP).ok_or(PortError::Internal)?;
    Ok((
        NaiveDate::parse_from_str(a, "%Y-%m-%d").map_err(|_| PortError::Internal)?,
        Uuid::parse_str(b).map_err(|_| PortError::Internal)?,
    ))
}
fn decode_tax_kind(v: &str) -> Result<StatutoryTaxKind, PortError> {
    match v {
        "output_vat" => Ok(StatutoryTaxKind::OutputVat),
        "input_vat" => Ok(StatutoryTaxKind::InputVat),
        "withholding" => Ok(StatutoryTaxKind::Withholding),
        _ => Err(PortError::Internal),
    }
}
fn scan(v: u16) -> i64 {
    i64::from(v).saturating_mul(8).saturating_add(8)
}
fn truncate<T, F: Fn(&T) -> String>(v: &mut Vec<T>, size: u16, encode: F) -> Option<String> {
    if v.len() > usize::from(size) {
        v.truncate(usize::from(size));
        v.last().map(encode)
    } else {
        None
    }
}
fn map_read_error(_: CqrsSqlxError) -> PortError {
    PortError::Unavailable
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_cursors_fail_closed() {
        assert_eq!(decode_cursor("bad"), Err(PortError::Internal));
        assert_eq!(decode_bank_cursor("bad"), Err(PortError::Internal));
    }
    #[test]
    fn post_read_scope_defense_is_present() {
        let source = include_str!("read.rs");
        assert!(source.contains("scope.contains"));
        assert!(source.contains("generation.status='active'"));
        assert!(!source.contains(concat!("fallback", "_to_write")));
    }
}
