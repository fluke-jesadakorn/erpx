use super::*;
use crate::domain::journal_reversal_money::{JournalMoneyAncestor, RetainedJournalMoney};
use crate::domain::journal_review::JournalLedgerLine;
use futures_util::TryStreamExt;

const ROW_LIMIT: usize = 20_000;
const BYTE_LIMIT: usize = 32 * 1024 * 1024;
#[derive(Default)]
pub(super) struct ImageBudget {
    bytes: usize,
    rows: usize,
}
pub(super) async fn lock_sources(c: &mut PgConnection) -> Result<(), Error> {
    // These are Finance-owned tables only. Workflow owns its receipt/key fence;
    // the mandatory external port owns all Identity/Commercial/foreign locks.
    sqlx::query("LOCK TABLE finance_journals,finance_journal_lines,money_transaction_snapshots,finance_invoices,finance_allocations,finance_book_postings,finance_financial_events IN SHARE ROW EXCLUSIVE MODE").execute(&mut *c).await?;
    sqlx::query("LOCK TABLE finance_periods,finance_charts,finance_accounts,finance_book_rules,finance_book_account_mappings,finance_dimension_definitions,finance_dimension_values,company_fx_rates,company_currency_scales IN SHARE MODE").execute(c).await?;
    Ok(())
}
pub(super) async fn database_now(c: &mut PgConnection) -> Result<i64, Error> {
    Ok(
        sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint")
            .fetch_one(c)
            .await?,
    )
}
pub(super) fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, Error> {
    v.get(key).and_then(Value::as_str).ok_or_else(changed)
}
pub(super) fn integer(v: &Value, key: &str) -> Result<i64, Error> {
    v.get(key).and_then(Value::as_i64).ok_or_else(changed)
}
pub(super) fn optional_text(v: &Value, key: &str) -> Result<Option<String>, Error> {
    match v.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        _ => Err(changed()),
    }
}
pub(super) fn configuration<'a>(image: &'a Image, name: &str) -> Result<&'a [Value], Error> {
    image
        .configurations
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_slice())
        .ok_or_else(changed)
}
fn json(raw: String) -> Result<Value, Error> {
    serde_json::from_str(&raw).map_err(|_| changed())
}

// All SQL supplied here is a closed literal/static fragment in this module.
// Identifiers are never input; every Company/source value is bound separately.
pub(super) async fn rows(
    c: &mut PgConnection,
    sql: &str,
    company: &CompanyId,
    ids: &[String],
    filters: &[String],
    budget: &mut ImageBudget,
) -> Result<Vec<Value>, Error> {
    // PostgreSQL returns bounded NULL markers for an oversized row and every
    // later row once the remaining image budget is exhausted. This bounds the
    // wire payload as well as client collection/cursor cleanup. Server query
    // execution still uses PostgreSQL's own work_mem/statement resource policy.
    let remaining = i64::try_from(BYTE_LIMIT.saturating_sub(budget.bytes)).map_err(|_| limit())?;
    let guarded = format!(
        "WITH requested AS MATERIALIZED ({sql}), measured AS (SELECT value,row_number()OVER() ordinal,octet_length(value)::bigint bytes FROM requested bounded(value)), admitted AS (SELECT value,ordinal,bytes,sum(bytes)OVER(ORDER BY ordinal) total FROM measured) SELECT CASE WHEN bytes>$4 OR total>$4 THEN NULL ELSE value END FROM admitted WHERE $3::text[] IS NOT NULL ORDER BY ordinal"
    );
    let mut stream = sqlx::query_scalar::<_, Option<String>>(sqlx::AssertSqlSafe(guarded))
        .bind(company.as_str())
        .bind(ids)
        .bind(filters)
        .bind(remaining)
        .fetch(c);
    let mut values = Vec::new();
    while let Some(raw) = stream.try_next().await? {
        let raw = raw.ok_or_else(limit)?;
        budget.rows = budget.rows.checked_add(1).ok_or_else(limit)?;
        budget.bytes = budget.bytes.checked_add(raw.len()).ok_or_else(limit)?;
        if budget.rows > ROW_LIMIT || budget.bytes > BYTE_LIMIT {
            return Err(limit());
        }
        values.push(json(raw)?);
    }
    Ok(values)
}
fn limit() -> Error {
    JournalError {
        kind: FailureKind::Unavailable,
        code: "JOURNAL_REVIEW_SOURCE_LIMIT",
        message: "The protected Finance source exceeds this bounded review unit.".into(),
    }
    .into()
}
async fn chain(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    budget: &mut ImageBudget,
) -> Result<Vec<Value>, Error> {
    rows(c,"WITH RECURSIVE ancestry AS (SELECT j.id,j.company_id,j.source_type,j.source_id,j.currency,j.entry_date,ARRAY[j.id] path,0 depth,false cycle FROM finance_journals j WHERE j.id=($2::text[])[1] AND j.company_id=$1 UNION ALL SELECT p.id,p.company_id,p.source_type,p.source_id,p.currency,p.entry_date,a.path||p.id,a.depth+1,p.id=ANY(a.path) FROM ancestry a JOIN finance_journals p ON p.id=a.source_id AND p.company_id=$1 WHERE a.source_type='reversal' AND NOT a.cycle AND a.depth<100) SELECT jsonb_build_object('id',a.id,'companyId',a.company_id,'sourceType',a.source_type,'sourceId',a.source_id,'currency',a.currency,'entryDate',a.entry_date,'depth',a.depth,'cycle',a.cycle,'total',COALESCE((SELECT SUM(l.debit_minor)FROM finance_journal_lines l WHERE l.journal_id=a.id),0)::text)::text FROM ancestry a ORDER BY depth",company,&[id.to_owned()],&[],budget).await
}
pub(super) async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    ids: &[String],
    command: &JournalReviewCommand,
) -> Result<Image, Error> {
    let anchor = ids.first().ok_or_else(changed)?;
    let mut budget = ImageBudget::default();
    let chain = chain(c, company, anchor, &mut budget).await?;
    if chain.is_empty() {
        return Err(changed());
    }
    capture_remaining(c, company, ids, command, chain, &mut budget).await
}
async fn capture_remaining(
    c: &mut PgConnection,
    company: &CompanyId,
    ids: &[String],
    command: &JournalReviewCommand,
    chain: Vec<Value>,
    budget: &mut ImageBudget,
) -> Result<Image, Error> {
    const RELATED: &str = "SELECT j.id FROM finance_journals j WHERE j.company_id=$1 AND (j.id=ANY($2) OR j.id IN(SELECT b.secondary_journal_id FROM finance_book_postings b WHERE b.company_id=$1 AND b.primary_journal_id=ANY($2)) OR (j.source_type='book_projection' AND j.source_id=ANY($2)))";
    let journals=rows(c,&format!("SELECT to_jsonb(j)::text FROM finance_journals j WHERE j.id IN({RELATED}) ORDER BY j.id LIMIT 20001"),company,ids,&[],budget).await?;
    let lines=rows(c,&format!("SELECT to_jsonb(l)::text FROM finance_journal_lines l WHERE l.journal_id IN({RELATED}) ORDER BY l.journal_id,l.ordinal,l.id LIMIT 20001"),company,ids,&[],budget).await?;
    let money=rows(c,&format!("SELECT (to_jsonb(m)||jsonb_build_object('amount_units',m.amount_units::text,'usd_units',m.usd_units::text,'carried_allocation_offset_units',m.carried_allocation_offset_units::text))::text FROM money_transaction_snapshots m WHERE m.company_id=$1 AND m.source_type='finance_journal' AND m.source_id IN({RELATED}) ORDER BY m.source_id,m.amount_kind LIMIT 20001"),company,ids,&[],budget).await?;
    let invoices=rows(c,"SELECT to_jsonb(i)::text FROM finance_invoices i WHERE i.company_id=$1 AND i.source_type='opening_import' AND i.source_id=ANY($2) ORDER BY i.id LIMIT 20001",company,ids,&[],budget).await?;
    let allocations=rows(c,"SELECT to_jsonb(a)::text FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id WHERE i.company_id=$1 AND i.source_type='opening_import' AND i.source_id=ANY($2) ORDER BY a.id LIMIT 20001",company,ids,&[],budget).await?;
    let books=rows(c,"SELECT to_jsonb(b)::text FROM finance_book_postings b WHERE b.company_id=$1 AND b.primary_journal_id=ANY($2) ORDER BY b.primary_journal_id,b.rule_id LIMIT 20001",company,ids,&[],budget).await?;
    let events=rows(c,&format!("SELECT to_jsonb(e)::text FROM finance_financial_events e WHERE e.company_id=$1 AND e.journal_id IN({RELATED}) ORDER BY e.id LIMIT 20001"),company,ids,&[],budget).await?;
    let configurations = super::closure::configurations(
        c,
        company,
        super::closure::Sources {
            ids,
            command,
            chain: &chain,
            journals: &journals,
            lines: &lines,
            money: &money,
            books: &books,
        },
        budget,
    )
    .await?;
    let image = Image {
        chain,
        journals,
        lines,
        money,
        invoices,
        allocations,
        books,
        events,
        configurations,
    };
    Ok(image)
}
fn record(v: &Value) -> Result<ReviewJournal, Error> {
    Ok(ReviewJournal {
        id: text(v, "id")?.into(),
        company_id: text(v, "company_id")?.into(),
        chart_id: text(v, "chart_id")?.into(),
        period_id: text(v, "period_id")?.into(),
        journal_number: text(v, "journal_number")?.into(),
        entry_date: text(v, "entry_date")?.into(),
        currency: text(v, "currency")?.into(),
        status: text(v, "status")?.into(),
        version: integer(v, "version")?,
        created_by: text(v, "created_by")?.into(),
        source_type: text(v, "source_type")?.into(),
        source_id: optional_text(v, "source_id")?,
    })
}
pub(super) async fn load(
    c: &mut PgConnection,
    actor: &FinanceActor,
    command: &JournalReviewCommand,
    posted_only: bool,
) -> Result<Option<PgJournalReviewSource>, Error> {
    if posted_only {
        let posted:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_journals WHERE id=$1 AND company_id=$2 AND status='posted')").bind(command.journal_id.as_str()).bind(actor.company_id.as_str()).fetch_one(&mut *c).await?;
        if !posted {
            return Ok(None);
        }
    }
    let mut ids = vec![command.journal_id.as_str().into()];
    let mut budget = ImageBudget::default();
    let ancestry = chain(
        c,
        &actor.company_id,
        command.journal_id.as_str(),
        &mut budget,
    )
    .await?;
    if ancestry.is_empty() {
        return Ok(None);
    }
    for row in &ancestry {
        let id = text(row, "id")?.to_owned();
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    if matches!(command.input, JournalReviewInput::Reverse { .. }) {
        ids.push(format!("reversal-{}", command.key.as_str()));
    }
    let before =
        capture_remaining(c, &actor.company_id, &ids, command, ancestry, &mut budget).await?;
    let Some(header) = before
        .journals
        .iter()
        .find(|j| j["id"] == command.journal_id.as_str())
    else {
        return Ok(None);
    };
    let journal = record(header)?;
    if posted_only && journal.status != "posted" {
        return Ok(None);
    }
    Ok(Some(PgJournalReviewSource {
        witness: current_transaction_witness(c).await?,
        actor: actor.clone(),
        command: command.clone(),
        journal,
        primary_ids: ids,
        before,
        database_at: database_now(c).await?,
    }))
}
pub(super) fn ledger(source: &PgJournalReviewSource) -> Result<Vec<JournalLedgerLine>, Error> {
    source
        .before
        .lines
        .iter()
        .filter(|l| l["journal_id"] == source.journal.id)
        .map(|l| {
            Ok(JournalLedgerLine {
                id: text(l, "id")?.into(),
                journal_id: text(l, "journal_id")?.into(),
                account_id: text(l, "account_id")?.into(),
                description: text(l, "description")?.into(),
                debit_minor: integer(l, "debit_minor")?,
                credit_minor: integer(l, "credit_minor")?,
                base_debit_minor: integer(l, "base_debit_minor")?,
                base_credit_minor: integer(l, "base_credit_minor")?,
                dimension_json: text(l, "dimension_json")?.into(),
                ordinal: integer(l, "ordinal")?,
            })
        })
        .collect()
}
fn period(image: &Image, id: &str, date: &str) -> Result<bool, Error> {
    let periods = configuration(image, "periods")?;
    let open = periods.iter().any(|p| {
        p["id"] == id
            && p["state"] == "open"
            && p["start_date"].as_str().is_some_and(|d| d <= date)
            && p["end_date"].as_str().is_some_and(|d| d >= date)
    });
    let closed = periods.iter().any(|p| {
        matches!(p["state"].as_str(), Some("closed" | "closing"))
            && p["start_date"].as_str().is_some_and(|d| d <= date)
            && p["end_date"].as_str().is_some_and(|d| d >= date)
    });
    Ok(open && !closed)
}
pub(super) fn posting_facts(source: &PgJournalReviewSource) -> Result<JournalPostingFacts, Error> {
    let lines = ledger(source)?;
    let accounts = configuration(&source.before, "accounts")?;
    let charts = configuration(&source.before, "charts")?;
    let base = charts
        .iter()
        .find(|c| c["id"] == source.journal.chart_id)
        .and_then(|c| c["base_currency"].as_str());
    let active_accounts = lines.iter().all(|l| {
        accounts.iter().any(|a| {
            a["id"] == l.account_id
                && a["chart_id"] == source.journal.chart_id
                && a["status"] == "active"
                && (a["currency"].is_null()
                    || a["currency"] == source.journal.currency
                    || a["currency"].as_str() == base)
        })
    });
    let opening_subledgers_ready = source
        .before
        .invoices
        .iter()
        .filter(|i| i["source_id"] == source.journal.id)
        .all(|i| {
            i["status"] == "draft"
                && (i["journal_id"].is_null() || i["journal_id"] == source.journal.id)
        });
    Ok(JournalPostingFacts {
        period_accepts_date: period(
            &source.before,
            &source.journal.period_id,
            &source.journal.entry_date,
        )?,
        active_accounts,
        opening_subledgers_ready,
        lines,
    })
}
pub(super) fn reversal_facts(
    source: &PgJournalReviewSource,
) -> Result<JournalReversalSourceFacts, Error> {
    let JournalReviewInput::Reverse {
        period_id,
        entry_date,
        ..
    } = &source.command.input
    else {
        return Err(changed());
    };
    let opening_ids: Vec<_> = source
        .before
        .invoices
        .iter()
        .filter(|i| i["source_id"] == source.journal.id)
        .filter_map(|i| i["id"].as_str())
        .collect();
    let ancestry: Vec<JournalMoneyAncestor> = source
        .before
        .chain
        .iter()
        .cloned()
        .map(serde_json::from_value)
        .collect::<Result<_, _>>()
        .map_err(|_| changed())?;
    let money = source
        .before
        .money
        .iter()
        .map(|m| {
            Ok(RetainedJournalMoney {
                company_id: text(m, "company_id")?.into(),
                source_id: text(m, "source_id")?.into(),
                amount_kind: text(m, "amount_kind")?.into(),
                transaction_date: text(m, "transaction_date")?.into(),
                currency: text(m, "currency")?.into(),
                currency_scale: u8::try_from(integer(m, "currency_scale")?)
                    .map_err(|_| changed())?,
                amount_units: text(m, "amount_units")?.into(),
                usd_units: text(m, "usd_units")?.into(),
                fx_rate_id: optional_text(m, "fx_rate_id")?,
                used_latest_fallback: m["used_latest_fallback"].as_bool().ok_or_else(changed)?,
                original_source_type: optional_text(m, "original_source_type")?,
                original_source_id: optional_text(m, "original_source_id")?,
                original_amount_kind: optional_text(m, "original_amount_kind")?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(JournalReversalSourceFacts {
        decision: crate::domain::journal_review::JournalReversalFacts {
            opening_allocated: source
                .before
                .allocations
                .iter()
                .any(|a| opening_ids.contains(&a["invoice_id"].as_str().unwrap_or(""))),
            source_opening: source.journal.source_id.as_ref().is_some_and(|id| {
                source
                    .before
                    .journals
                    .iter()
                    .any(|j| j["id"] == *id && j["source_type"] == "opening_import")
            }),
            root_source_type: ancestry.last().map(|j| j.source_type.clone()),
        },
        period_accepts_date: period(&source.before, period_id, entry_date)?,
        lines: ledger(source)?,
        ancestry,
        money,
    })
}
pub(super) async fn apply(
    c: &mut PgConnection,
    actor: &CommandActor,
    source: &PgJournalReviewSource,
    plan: &JournalReviewPlan,
) -> Result<(), Error> {
    match plan.action() {
        JournalReviewAction::Post {
            expected_version, ..
        } => {
            let count=sqlx::query("UPDATE finance_journals SET status='posted',version=version+1,posted_by=$1,posted_at=$2,updated_at=$2 WHERE id=$3 AND company_id=$4 AND version=$5 AND status IN('draft','in_review')").bind(actor.user_id.as_str()).bind(plan.now().get()).bind(&source.journal.id).bind(actor.company_id.as_str()).bind(expected_version).execute(&mut *c).await?.rows_affected();
            if count != 1 {
                return Err(changed());
            }
        }
        JournalReviewAction::Reverse {
            period_id,
            entry_date,
            memo,
            reversal_id,
            number,
            lines,
            ..
        } => {
            let count=sqlx::query("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT $1,company_id,chart_id,$2,$3,$4,currency,fx_rate_micros,'reversal',id,$5,'posted',$6,$6,$7,$7,$7 FROM finance_journals WHERE id=$8 AND company_id=$9 AND version=$10 AND status='posted'").bind(reversal_id).bind(period_id).bind(number).bind(entry_date).bind(memo).bind(actor.user_id.as_str()).bind(plan.now().get()).bind(&source.journal.id).bind(actor.company_id.as_str()).bind(source.journal.version).execute(&mut *c).await?.rows_affected();
            if count != 1 {
                return Err(changed());
            }
            let count=sqlx::query("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT $1||'-'||ordinal,$1,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=$2").bind(reversal_id).bind(&source.journal.id).execute(&mut *c).await?.rows_affected();
            if count != lines.len() as u64 {
                return Err(changed());
            }
            let count=sqlx::query("UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=$1,reversed_at=$2,updated_at=$2 WHERE id=$3 AND company_id=$4 AND version=$5 AND status='posted'").bind(actor.user_id.as_str()).bind(plan.now().get()).bind(&source.journal.id).bind(actor.company_id.as_str()).bind(source.journal.version).execute(c).await?.rows_affected();
            if count != 1 {
                return Err(changed());
            }
        }
    }
    Ok(())
}
pub(super) async fn apply_opening(
    c: &mut PgConnection,
    actor: &CommandActor,
    source: &PgJournalReviewSource,
    plan: &JournalReviewPlan,
) -> Result<(), Error> {
    if !plan.opening() {
        return Ok(());
    }
    match plan.action() {
        JournalReviewAction::Post { .. } => {
            sqlx::query("UPDATE finance_invoices SET status='posted',approved_by=$1,posted_at=$2,version=version+1,updated_at=$2 WHERE company_id=$3 AND source_type='opening_import' AND source_id=$4 AND status='draft'").bind(actor.user_id.as_str()).bind(plan.now().get()).bind(actor.company_id.as_str()).bind(&source.journal.id).execute(c).await?;
        }
        JournalReviewAction::Reverse { .. } => {
            sqlx::query("UPDATE finance_invoices SET status='voided',outstanding_minor=0,base_outstanding_minor=0,version=version+1,updated_at=$1 WHERE company_id=$2 AND source_type='opening_import' AND source_id=$3").bind(plan.now().get()).bind(actor.company_id.as_str()).bind(&source.journal.id).execute(c).await?;
        }
    }
    Ok(())
}
