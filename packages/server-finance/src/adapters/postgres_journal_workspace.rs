//! Current references for the original Journal record. All SQL belongs to
//! Finance; current Identity/People/Commercial and release remain external.
use super::postgres_journal_detail::{Budget, rows};
use crate::{
    application::journal::FinanceActor,
    public::{
        journal_detail::{
            CurrentJournalDetail, FinanceJournalDetail, FinanceLedgerRow, FinanceLedgerValue,
            lock_journal_detail_sources, read_journal_detail,
        },
        journal_workspace::FinanceJournalRecordWorkspace,
    },
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::Serialize;
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
const REPLY_LIMIT: usize = 2 * 1024 * 1024;
fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "JOURNAL_WORKSPACE_SOURCE_LIMIT",
        "The original record workspace exceeds this bounded read.",
    )
    .into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "JOURNAL_WORKSPACE_SOURCE_CHANGED",
        "The original journal reference facts changed in this read.",
    )
    .into()
}
fn chart_missing() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "CHART_NOT_FOUND",
        "The record's book is unavailable.",
    )
    .into()
}
/// Reuses the validated detail source utility, with additional reference/absence
/// fences. Read-only data facts do not require account-active/period-open state.
pub async fn lock_journal_workspace_sources(c: &mut PgConnection) -> Result<(), Error> {
    lock_journal_detail_sources(c).await?;
    sqlx::query("LOCK TABLE finance_periods,finance_dimension_definitions,finance_dimension_values IN SHARE MODE").execute(c).await?;
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct JournalWorkspaceReferences {
    pub chart: FinanceLedgerRow,
    pub accounts: Vec<FinanceLedgerRow>,
    pub periods: Vec<FinanceLedgerRow>,
    pub dimensions: Vec<FinanceLedgerRow>,
    #[serde(rename = "dimensionValues")]
    pub dimension_values: Vec<FinanceLedgerRow>,
}
// B0 orders values by code without an ID tie-breaker. Preserve that wire order,
// but compare source facts by identity so a legal tie order is not a mutation.
fn indexed(
    rows: &[FinanceLedgerRow],
) -> Option<std::collections::BTreeMap<&str, &FinanceLedgerRow>> {
    let mut output = std::collections::BTreeMap::new();
    for row in rows {
        let Some(FinanceLedgerValue::Text(id)) = row.get("id") else {
            return None;
        };
        if output.insert(id.as_str(), row).is_some() {
            return None;
        }
    }
    Some(output)
}
fn same_rows(a: &[FinanceLedgerRow], b: &[FinanceLedgerRow]) -> bool {
    match (indexed(a), indexed(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}
impl JournalWorkspaceReferences {
    fn same_source(&self, other: &Self) -> bool {
        self.chart == other.chart
            && same_rows(&self.accounts, &other.accounts)
            && same_rows(&self.periods, &other.periods)
            && same_rows(&self.dimensions, &other.dimensions)
            && same_rows(&self.dimension_values, &other.dimension_values)
    }
}
struct Count<'a>(&'a mut usize);
impl std::io::Write for Count<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        *self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|n| *n <= REPLY_LIMIT)
            .ok_or_else(|| std::io::Error::other("bounded journal workspace"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn text<'a>(row: &'a FinanceLedgerRow, key: &str) -> Result<&'a str, Error> {
    match row.get(key) {
        Some(FinanceLedgerValue::Text(value)) => Ok(value),
        _ => Err(changed()),
    }
}
async fn identity(c: &mut PgConnection) -> Result<(String, i32, String, String), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("JOURNAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn references(
    c: &mut PgConnection,
    actor: &FinanceActor,
    detail: &FinanceJournalDetail,
) -> Result<JournalWorkspaceReferences, Error> {
    let chart = text(&detail.journal, "chart_id")?;
    let company = actor.company_id.as_str();
    let mut bytes = 0;
    serde_json::to_writer(Count(&mut bytes), detail).map_err(|_| unavailable())?;
    let mut budget = Budget {
        bytes,
        rows: 1 + detail.lines.len() + detail.history.len(),
    };
    let mut chart_rows=rows(c,"SELECT to_jsonb(h)::text FROM finance_charts h WHERE h.company_id=$1 AND h.id=$2 FOR SHARE OF h",company,chart,&mut budget).await?;
    if chart_rows.len() != 1 {
        return Err(chart_missing());
    }
    let accounts=rows(c,"SELECT to_jsonb(a)::text FROM finance_accounts a WHERE a.company_id=$1 AND a.chart_id=$2 ORDER BY a.code,a.id LIMIT 10001 FOR SHARE OF a",company,chart,&mut budget).await?;
    let periods=rows(c,"SELECT to_jsonb(p)::text FROM finance_periods p WHERE p.company_id=$1 AND $2::text IS NOT NULL ORDER BY p.start_date DESC,p.id LIMIT 10001 FOR SHARE OF p",company,chart,&mut budget).await?;
    let dimensions=rows(c,"SELECT jsonb_build_object('id',d.id,'code',d.code,'name',d.name,'required',d.required)::text FROM finance_dimension_definitions d WHERE d.company_id=$1 AND d.status='active'AND $2::text IS NOT NULL ORDER BY d.code LIMIT 10001 FOR SHARE OF d",company,chart,&mut budget).await?;
    let dimension_values=rows(c,"SELECT jsonb_build_object('id',v.id,'dimension_id',v.dimension_id,'code',v.code,'name',v.name)::text FROM finance_dimension_values v WHERE v.company_id=$1 AND v.status='active'AND $2::text IS NOT NULL ORDER BY v.code LIMIT 10001 FOR SHARE OF v",company,chart,&mut budget).await?;
    Ok(JournalWorkspaceReferences {
        chart: chart_rows.remove(0),
        accounts,
        periods,
        dimensions,
        dimension_values,
    })
}
/// Opaque current data witness, not a permission/reviewer/production grant.
/// People display names and the rendered context are added by the actual service
/// while retaining this proof and its normal Control/Holding authorization unit.
pub struct CurrentJournalRecordWorkspace {
    actor: FinanceActor,
    source: CurrentJournalDetail,
    data: FinanceJournalRecordWorkspace,
    identity: (String, i32, String, String),
    references: JournalWorkspaceReferences,
}
impl CurrentJournalRecordWorkspace {
    pub fn data(&self) -> &FinanceJournalRecordWorkspace {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if identity(c).await? != self.identity {
            return Err(changed());
        }
        self.source.require_current(c).await?;
        if !self
            .references
            .same_source(&references(c, &self.actor, self.source.detail()).await?)
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_journal_record_workspace(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &RecordId,
) -> Result<CurrentJournalRecordWorkspace, Error> {
    let source = read_journal_detail(c, actor, id).await?;
    let identity = identity(c).await?;
    let references = references(c, actor, source.detail()).await?;
    let detail = source.detail();
    let data = FinanceJournalRecordWorkspace {
        journal: detail.journal.clone(),
        lines: detail.lines.clone(),
        history: detail.history.clone(),
        chart: references.chart.clone(),
        accounts: references.accounts.clone(),
        periods: references.periods.clone(),
        dimensions: references.dimensions.clone(),
        dimension_values: references.dimension_values.clone(),
    };
    Ok(CurrentJournalRecordWorkspace {
        actor: actor.clone(),
        source,
        data,
        identity,
        references,
    })
}

/// Workspace projection for an inspected direct Service target. Composition
/// retains the separate cross-owner lineage and current authorization.
pub struct CurrentFieldServiceJournalWorkspace {
    actor: FinanceActor,
    source: crate::public::journal_detail::CurrentFieldServiceJournalDetail,
    data: FinanceJournalRecordWorkspace,
    identity: (String, i32, String, String),
    references: JournalWorkspaceReferences,
}
impl CurrentFieldServiceJournalWorkspace {
    pub fn data(&self) -> &FinanceJournalRecordWorkspace {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if identity(c).await? != self.identity {
            return Err(changed());
        }
        self.source.require_current(c).await?;
        if !self
            .references
            .same_source(&references(c, &self.actor, self.source.detail()).await?)
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_field_service_journal_workspace(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &RecordId,
    target: &crate::public::journal_review::CurrentFieldServicePartsJournalTarget,
) -> Result<CurrentFieldServiceJournalWorkspace, Error> {
    let source =
        crate::public::journal_detail::read_field_service_journal_detail(c, actor, id, target)
            .await?;
    let identity = identity(c).await?;
    let references = references(c, actor, source.detail()).await?;
    let detail = source.detail();
    let data = FinanceJournalRecordWorkspace {
        journal: detail.journal.clone(),
        lines: detail.lines.clone(),
        history: detail.history.clone(),
        chart: references.chart.clone(),
        accounts: references.accounts.clone(),
        periods: references.periods.clone(),
        dimensions: references.dimensions.clone(),
        dimension_values: references.dimension_values.clone(),
    };
    Ok(CurrentFieldServiceJournalWorkspace {
        actor: actor.clone(),
        source,
        data,
        identity,
        references,
    })
}
