//! Minimal read source; no post/reviewer, period-open or active-account policy.
//! Foreign-owner and secondary-book admission remains explicitly unsupported.
use crate::adapters::postgres_manual_draft_provenance::{
    CurrentManualDraftProvenance, inspect_manual_draft_provenance_with_limit,
    lock_manual_draft_provenance,
};
use crate::{application::journal::FinanceActor, public::journal_detail::*};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId, UserId};
use erpx_server_workflow::public::record_origin::{
    PgRecordOrigin, RecordOriginRequest, lock_record_origin_sources, read_record_origin_with_limit,
    verify_record_origin,
};
use futures_util::TryStreamExt;
use serde_json::Value;
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
const BYTE_LIMIT: usize = 2 * 1024 * 1024;
const ROW_LIMIT: usize = 10_000;
const HEADER: &str = "SELECT jsonb_build_object('id',j.id,'company_id',j.company_id,'chart_id',j.chart_id,'currency',j.currency,'source_type',j.source_type,'source_id',j.source_id,'created_by',j.created_by,'journal_number',j.journal_number,'is_opening',j.is_opening)::text FROM finance_journals j WHERE j.company_id=$1 AND j.id=$2 FOR SHARE OF j";
fn source_header(row: &FinanceLedgerRow) -> Result<FinanceLedgerRow, Error> {
    [
        "id",
        "company_id",
        "chart_id",
        "currency",
        "source_type",
        "source_id",
        "created_by",
        "journal_number",
        "is_opening",
    ]
    .into_iter()
    .map(|key| Ok((key.into(), row.get(key).ok_or_else(denied)?.clone())))
    .collect()
}

fn denied() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "JOURNAL_NOT_FOUND",
        "Journal not found.",
    )
    .into()
}
fn limit() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "JOURNAL_DETAIL_SOURCE_LIMIT",
        "The journal exceeds this bounded read.",
    )
    .into()
}
async fn witness(c: &mut PgConnection) -> Result<(String, i32, String, String), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("JOURNAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("JOURNAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub async fn lock_journal_detail_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("JOURNAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    lock_record_origin_sources(c).await?;
    // Utility-only before IAM's first snapshot. These retained absence/source
    // fences also cover new editions/lines/book relationships, not just rows.
    sqlx::query("LOCK TABLE finance_journals,finance_journal_lines,finance_manual_draft_editions,finance_accounts,finance_charts,finance_book_postings,finance_book_rules IN SHARE MODE").execute(&mut *c).await?;
    lock_manual_draft_provenance(c, false).await?;
    Ok(())
}
pub(super) struct Budget {
    pub(super) bytes: usize,
    pub(super) rows: usize,
}
pub(super) async fn rows(
    c: &mut PgConnection,
    sql: &str,
    company: &str,
    id: &str,
    budget: &mut Budget,
) -> Result<Vec<FinanceLedgerRow>, Error> {
    let guarded = format!(
        "WITH requested AS MATERIALIZED ({sql}), measured AS(SELECT value,row_number()OVER() ordinal,octet_length(value)::bigint bytes FROM requested bounded(value)),admitted AS(SELECT value,ordinal,bytes,sum(bytes)OVER(ORDER BY ordinal)total FROM measured)SELECT CASE WHEN bytes>262144 OR total>$3 THEN NULL ELSE value END FROM admitted ORDER BY ordinal"
    );
    let mut stream = sqlx::query_scalar::<_, Option<String>>(sqlx::AssertSqlSafe(guarded))
        .bind(company)
        .bind(id)
        .bind(i64::try_from(BYTE_LIMIT.saturating_sub(budget.bytes)).map_err(|_| limit())?)
        .fetch(c);
    let mut result = Vec::new();
    while let Some(raw) = stream.try_next().await? {
        let raw = raw.ok_or_else(limit)?;
        budget.bytes = budget.bytes.checked_add(raw.len()).ok_or_else(limit)?;
        budget.rows += 1;
        if budget.bytes > BYTE_LIMIT || budget.rows > ROW_LIMIT {
            return Err(limit());
        }
        let value: std::collections::BTreeMap<String, Value> =
            serde_json::from_str(&raw).map_err(|_| denied())?;
        let mut row = FinanceLedgerRow::new();
        for (k, v) in value {
            row.insert(
                k,
                match v {
                    Value::String(s) => FinanceLedgerValue::Text(s),
                    Value::Null => FinanceLedgerValue::Null,
                    Value::Number(n) => FinanceLedgerValue::Integer(FinanceReadInteger::new(
                        n.as_i64().ok_or_else(limit)?,
                    )?),
                    _ => return Err(limit()),
                },
            );
        }
        result.push(row);
    }
    Ok(result)
}
fn text<'a>(row: &'a FinanceLedgerRow, key: &str) -> Result<&'a str, Error> {
    match row.get(key) {
        Some(FinanceLedgerValue::Text(v)) => Ok(v),
        _ => Err(denied()),
    }
}
fn integer(row: &FinanceLedgerRow, key: &str) -> Result<i64, Error> {
    match row.get(key) {
        Some(FinanceLedgerValue::Integer(v)) => Ok(v.get()),
        _ => Err(denied()),
    }
}
async fn payload(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
) -> Result<FinanceJournalDetail, Error> {
    let mut budget = Budget { bytes: 0, rows: 0 };
    let mut headers=rows(c,"SELECT to_jsonb(j)::text FROM finance_journals j JOIN finance_charts h ON h.id=j.chart_id AND h.company_id=j.company_id WHERE j.company_id=$1 AND j.id=$2 FOR SHARE OF j,h",actor.company_id.as_str(),id,&mut budget).await?;
    if headers.len() != 1 {
        return Err(denied());
    }
    let journal = headers.remove(0);
    let lines=rows(c,"SELECT (to_jsonb(l)||jsonb_build_object('code',a.code,'name',a.name))::text FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id JOIN finance_accounts a ON a.id=l.account_id AND a.company_id=j.company_id WHERE j.company_id=$1 AND j.id=$2 ORDER BY l.ordinal LIMIT 10001 FOR SHARE OF l,a,j",actor.company_id.as_str(),id,&mut budget).await?;
    let history=rows(c,"SELECT to_jsonb(h)::text FROM finance_manual_draft_editions h WHERE h.company_id=$1 AND h.journal_id=$2 ORDER BY h.journal_version LIMIT 10001 FOR SHARE OF h",actor.company_id.as_str(),id,&mut budget).await?;
    Ok(FinanceJournalDetail {
        journal,
        lines,
        history,
    })
}
pub struct CurrentJournalDetail {
    actor: FinanceActor,
    id: String,
    transaction: (String, i32, String, String),
    detail: FinanceJournalDetail,
    origins: Vec<PgRecordOrigin>,
    manual_provenance: Vec<CurrentManualDraftProvenance>,
    ancestors: Vec<FinanceLedgerRow>,
}
impl CurrentJournalDetail {
    pub fn detail(&self) -> &FinanceJournalDetail {
        &self.detail
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if witness(c).await? != self.transaction {
            return Err(denied());
        }
        for origin in &self.origins {
            verify_record_origin(c, origin).await?;
        }
        for origin in &self.manual_provenance {
            origin.require_current(c).await?;
        }
        let mut budget = Budget { bytes: 0, rows: 0 };
        for ancestor in &self.ancestors {
            let fresh = rows(
                c,
                HEADER,
                self.actor.company_id.as_str(),
                text(ancestor, "id")?,
                &mut budget,
            )
            .await?;
            if fresh.as_slice() != std::slice::from_ref(ancestor) {
                return Err(denied());
            }
        }
        if payload(c, &self.actor, &self.id).await? != self.detail {
            return Err(denied());
        }
        Ok(())
    }
}
pub async fn read_journal_detail(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &RecordId,
) -> Result<CurrentJournalDetail, Error> {
    let transaction = witness(c).await?;
    let detail = payload(c, actor, id.as_str()).await?;
    let mut ancestors = Vec::new();
    let mut current = source_header(&detail.journal)?;
    let mut ancestry_budget = Budget { bytes: 0, rows: 0 };
    let mut seen = std::collections::BTreeSet::new();
    let mut origins = Vec::new();
    let mut manual_provenance = Vec::new();
    let mut origin_bytes = 0usize;
    for depth in 0..=100 {
        let current_id = text(&current, "id")?.to_owned();
        if !seen.insert(current_id.clone()) {
            return Err(denied());
        }
        let secondary: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM finance_book_postings WHERE secondary_journal_id=$1) OR EXISTS(SELECT 1 FROM finance_book_rules WHERE company_id=$2 AND secondary_chart_id=$3 AND status='approved')",
        )
        .bind(&current_id).bind(actor.company_id.as_str()).bind(text(&current,"chart_id")?)
        .fetch_one(&mut *c)
        .await?;
        if secondary {
            return Err(denied());
        }
        let kind = text(&current, "source_type")?;
        let reversal = kind == "reversal";
        let root = !reversal;
        if root
            && !((kind == "manual"
                && current.get("source_id") == Some(&FinanceLedgerValue::Null)
                && integer(&current, "is_opening")? == 0)
                || (depth == 0
                    && kind == "opening_import"
                    && text(&current, "source_id")? == text(&current, "chart_id")?
                    && integer(&current, "is_opening")? == 1))
        {
            return Err(denied());
        }
        let key = current_id
            .strip_prefix(if root { "journal-" } else { "reversal-" })
            .filter(|v| !v.is_empty())
            .ok_or_else(denied)?;
        let parent = if root {
            current_id.as_str()
        } else {
            text(&current, "source_id")?
        };
        if root && kind == "manual" {
            let proof = inspect_manual_draft_provenance_with_limit(
                c,
                &actor.company_id,
                &current_id,
                (4 * 1024 * 1024_usize).saturating_sub(origin_bytes),
            )
            .await?;
            origin_bytes = origin_bytes
                .checked_add(proof.retained_text_bytes())
                .filter(|n| *n <= 4 * 1024 * 1024)
                .ok_or_else(limit)?;
            let _ = origin_bytes;
            manual_provenance.push(proof);
            ancestors.push(current.clone());
            break;
        }
        let receipt_limit = (4 * 1024 * 1024_usize)
            .saturating_sub(origin_bytes)
            .saturating_sub(
                key.len()
                    + parent.len()
                    + actor.company_id.as_str().len()
                    + text(&current, "created_by")?.len()
                    + 1024,
            );
        let origin = read_record_origin_with_limit(
            c,
            RecordOriginRequest {
                command_key: key.into(),
                operation: if root {
                    "finance.journal.create"
                } else {
                    "finance.journal.reverse"
                }
                .into(),
                record_id: parent.into(),
                creator_user_id: UserId::new(text(&current, "created_by")?)?,
                company_id: actor.company_id.clone(),
            },
            receipt_limit,
        )
        .await?
        .ok_or_else(denied)?;
        origin_bytes = origin_bytes
            .checked_add(origin.retained_text_bytes())
            .ok_or_else(limit)?;
        if origin_bytes > 4 * 1024 * 1024 || origin.result().as_str().len() > 16384 {
            return Err(limit());
        }
        let result: Value = serde_json::from_str(origin.result().as_str()).map_err(|_| denied())?;
        if root {
            if result["id"] != current_id
                || result["number"] != format!("J-{key}")
                || result["status"] != "draft"
                || result["version"] != 1
                || text(&current, "journal_number")? != format!("J-{key}")
            {
                return Err(denied());
            }
        } else if result["reversalJournalId"] != current_id
            || result["reversedJournalId"] != parent
            || text(&current, "journal_number")? != format!("REV-{key}")
        {
            return Err(denied());
        }
        origins.push(origin);
        ancestors.push(current.clone());
        if root {
            break;
        }
        if depth == 100 {
            return Err(limit());
        }
        let mut headers = rows(
            c,
            HEADER,
            actor.company_id.as_str(),
            parent,
            &mut ancestry_budget,
        )
        .await?;
        current = headers.pop().ok_or_else(denied)?;
        if text(&current, "chart_id")? != text(&detail.journal, "chart_id")?
            || text(&current, "currency")? != text(&detail.journal, "currency")?
        {
            return Err(denied());
        }
    }
    // All ancestry stays Finance-owned and is rechecked by retained table/tuple
    // sources; no foreign label can select a capability or permission.
    Ok(CurrentJournalDetail {
        actor: actor.clone(),
        id: id.as_str().into(),
        transaction,
        detail,
        origins,
        manual_provenance,
        ancestors,
    })
}

/// Bounded Finance-owned payload for an already inspected direct Service target.
/// Cross-owner provenance and authorization remain the composition's responsibility.
pub struct CurrentFieldServiceJournalDetail {
    actor: FinanceActor,
    id: String,
    transaction: (String, i32, String, String),
    detail: FinanceJournalDetail,
}
impl CurrentFieldServiceJournalDetail {
    pub fn detail(&self) -> &FinanceJournalDetail {
        &self.detail
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if witness(c).await? != self.transaction
            || payload(c, &self.actor, &self.id).await? != self.detail
        {
            return Err(denied());
        }
        Ok(())
    }
}
pub async fn read_field_service_journal_detail(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &RecordId,
    target: &crate::public::journal_review::CurrentFieldServicePartsJournalTarget,
) -> Result<CurrentFieldServiceJournalDetail, Error> {
    let facts = target.facts();
    if facts.holding_id != actor.holding_id
        || facts.company_id != actor.company_id
        || facts.journal_id != id.as_str()
    {
        return Err(denied());
    }
    target.require_current(c).await?;
    Ok(CurrentFieldServiceJournalDetail {
        actor: actor.clone(),
        id: id.as_str().into(),
        transaction: witness(c).await?,
        detail: payload(c, actor, id.as_str()).await?,
    })
}
