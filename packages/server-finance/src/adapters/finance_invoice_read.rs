//! Native Finance invoice list and historical aging read. The aging projection
//! reuses the dated accounting-evidence CTE; mutable workflow status is never
//! the historical balance authority.
use base64::Engine;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

const PAGE_SIZE: usize = 30;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct InvoicePageQuery {
    pub q: String,
    pub kind: String,
    pub status: String,
    pub queue: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct InvoicePage {
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub items: Vec<Value>,
    pub next_cursor: Option<String>,
    pub previous_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Cursor {
    key: String,
    id: String,
    reverse: bool,
    scope: String,
}

fn invalid_cursor() -> Error {
    BusinessError::invalid("INVALID_CURSOR", "Restart this search to load results.").into()
}

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_INVOICE_READ_UNAVAILABLE",
        "The invoice view is unavailable.",
    )
    .into()
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_OWNER_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// The historical read spans many dated sources; serializable snapshot isolation
/// is the consistency guard and no writer table locks are taken.
pub async fn lock_finance_invoice_read_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await
}

fn hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn scope_hash(company: &str, query: &InvoicePageQuery) -> Result<String, Error> {
    let scope = json!([
        company,
        "invoices",
        query.chart_id,
        query.q,
        query.kind,
        query.status,
        query.queue
    ]);
    serde_json::to_string(&json!([scope, true, PAGE_SIZE]))
        .map(|raw| hash(&raw))
        .map_err(|_| unavailable())
}

fn decode_cursor(raw: &str) -> Result<Cursor, Error> {
    if raw.is_empty() || raw.len() > 4096 {
        return Err(invalid_cursor());
    }
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| invalid_cursor())?;
    let cursor: Cursor = serde_json::from_slice(&bytes).map_err(|_| invalid_cursor())?;
    if cursor.key.is_empty() || cursor.id.is_empty() || cursor.scope.len() > 200 {
        return Err(invalid_cursor());
    }
    Ok(cursor)
}

fn encode_cursor(key: &str, id: &str, reverse: bool, scope: &str) -> Result<String, Error> {
    serde_json::to_vec(&Cursor {
        key: key.to_owned(),
        id: id.to_owned(),
        reverse,
        scope: scope.to_owned(),
    })
    .map(|raw| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw))
    .map_err(|_| unavailable())
}

fn parse(value: String) -> Result<Value, Error> {
    serde_json::from_str(&value).map_err(|_| unavailable())
}

fn valid_date(value: &str) -> bool {
    value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

fn valid_calendar_date(value: &str) -> bool {
    if !valid_date(value) {
        return false;
    }
    let year = value[0..4].parse::<i32>().ok();
    let month = value[5..7].parse::<usize>().ok();
    let day = value[8..10].parse::<u32>().ok();
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    (1..=12).contains(&month) && day >= 1 && day <= days[month - 1]
}

fn validate_page(query: &InvoicePageQuery) -> Result<(), Error> {
    let invalid = |code: &'static str, message: &'static str| {
        BusinessError::invalid(code, message).into()
    };
    if !matches!(query.kind.as_str(), "" | "receivable" | "payable")
        || !matches!(query.queue.as_str(), "" | "open")
        || query.q.len() > 200
        || query.status.len() > 40
        || query
            .chart_id
            .as_deref()
            .is_some_and(|value| value.is_empty() || value.len() > 200)
    {
        return Err(invalid(
            "FINANCE_INVOICE_QUERY_INVALID",
            "Use the invoice list filters.",
        ));
    }
    Ok(())
}

pub async fn read_invoice_page(
    c: &mut PgConnection,
    company: &str,
    query: &InvoicePageQuery,
) -> Result<InvoicePage, Error> {
    serializable(c).await?;
    validate_page(query)?;
    if company.is_empty() || company.len() > 200 {
        return Err(unavailable());
    }
    let scope = scope_hash(company, query)?;
    let cursor = query.cursor.as_deref().map(decode_cursor).transpose()?;
    if cursor.as_ref().is_some_and(|cursor| cursor.scope != scope) {
        return Err(invalid_cursor());
    }
    let inner = "SELECT (to_jsonb(i) || jsonb_build_object(\
        'party_name',p.name,\
        'snapshot_usd_units',s.usd_units::text,\
        'snapshot_source_units',s.amount_units::text,\
        'snapshot_rate_id',s.fx_rate_id,\
        'page_key',i.invoice_date||':'||i.invoice_number)) AS value,\
        i.invoice_date||':'||i.invoice_number AS page_key,i.id AS id \
        FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id \
        LEFT JOIN money_transaction_snapshots s ON s.company_id=i.company_id \
          AND s.source_type='finance_invoice' AND s.source_id=i.id AND s.amount_kind='total' \
        WHERE i.company_id=$1 \
          AND ($2::text IS NULL OR EXISTS(SELECT 1 FROM finance_accounts a \
            WHERE a.id=i.debit_account_id AND a.company_id=i.company_id AND a.chart_id=$2)) \
          AND ($3='' OR i.kind=$3) AND ($4='' OR i.status=$4) \
          AND ($5<>'open' OR (i.status IN ('posted','partially_paid') AND i.outstanding_minor>0)) \
          AND ($6='' OR strpos(lower(i.invoice_number||' '||p.name),lower($6))>0)";
    let reverse = cursor.as_ref().is_some_and(|value| value.reverse);
    let descending = !reverse;
    let direction = if descending { "DESC" } else { "ASC" };
    let comparison = if descending { "<" } else { ">" };
    let cursor_clause = if cursor.is_some() {
        format!("WHERE (page_key,id) {comparison} ($7,$8)")
    } else {
        String::new()
    };
    let sql = format!(
        "WITH list AS ({inner}) SELECT value::text FROM list {cursor_clause} \
         ORDER BY page_key {direction},id {direction} LIMIT 31"
    );
    let mut statement = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
        .bind(company)
        .bind(query.chart_id.as_deref())
        .bind(&query.kind)
        .bind(&query.status)
        .bind(&query.queue)
        .bind(&query.q);
    if let Some(cursor) = &cursor {
        statement = statement.bind(&cursor.key).bind(&cursor.id);
    }
    let mut rows = statement.fetch_all(&mut *c).await?;
    let overflow = rows.len() > PAGE_SIZE;
    rows.truncate(PAGE_SIZE);
    if reverse {
        rows.reverse();
    }
    let items = rows
        .into_iter()
        .map(parse)
        .collect::<Result<Vec<_>, _>>()?;
    let previous_cursor = if !items.is_empty() && (reverse || cursor.is_some()) {
        let first = &items[0];
        Some(encode_cursor(
            first.get("page_key").and_then(Value::as_str).ok_or_else(unavailable)?,
            first.get("id").and_then(Value::as_str).ok_or_else(unavailable)?,
            true,
            &scope,
        )?)
    } else {
        None
    };
    let next_cursor = if !items.is_empty() && (reverse || overflow) {
        let last = &items[items.len() - 1];
        Some(encode_cursor(
            last.get("page_key").and_then(Value::as_str).ok_or_else(unavailable)?,
            last.get("id").and_then(Value::as_str).ok_or_else(unavailable)?,
            false,
            &scope,
        )?)
    } else {
        None
    };
    let has_more = next_cursor.is_some();
    Ok(InvoicePage {
        items,
        next_cursor,
        previous_cursor,
        has_more,
    })
}

const INVOICE_HISTORY_CTE: &str = r#"WITH history_cutoff AS (
  SELECT $1::text company_id, $2::text as_of
), historical_invoice_balances AS (
  SELECT i.*, a.chart_id,
    COALESCE(j.entry_date,i.invoice_date) recognition_date,
    i.total_minor
      - COALESCE(settlements.transaction_minor,0)
      - COALESCE(credits.transaction_minor,0)
      - COALESCE(billing_credits.transaction_minor,0)
      - COALESCE(opening_reversal.transaction_minor,0)
      + COALESCE(refunds.transaction_minor,0) historical_outstanding_minor,
    (CASE WHEN i.base_total_minor=0 AND i.fx_rate_micros=1000000 THEN i.total_minor ELSE i.base_total_minor END)
      - COALESCE(settlements.base_minor,0)
      - COALESCE(credits.base_minor,0)
      - COALESCE(billing_credits.base_minor,0)
      - COALESCE(opening_reversal.base_minor,0)
      + COALESCE(refunds.base_minor,0)
      + COALESCE(revaluation.base_minor,0) historical_base_outstanding_minor,
    (j.id IS NULL OR COALESCE(settlements.legacy_payment_evidence,false)) legacy_date_evidence
  FROM finance_invoices i
  JOIN history_cutoff h ON h.company_id=i.company_id
  JOIN finance_accounts a ON a.id=CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END AND a.company_id=i.company_id
  LEFT JOIN finance_journals j ON j.id=i.journal_id AND j.company_id=i.company_id
  LEFT JOIN LATERAL (
    SELECT SUM(x.amount_minor) transaction_minor,BOOL_OR(pj.id IS NULL) legacy_payment_evidence,
      SUM(CASE WHEN x.base_invoice_minor=0 AND i.fx_rate_micros=1000000 THEN x.amount_minor ELSE x.base_invoice_minor END) base_minor
    FROM finance_allocations x
    JOIN finance_payments p ON p.id=x.payment_id AND p.company_id=i.company_id
    LEFT JOIN finance_journals pj ON pj.id=p.journal_id AND pj.company_id=i.company_id
    LEFT JOIN finance_payment_reversals pr ON pr.payment_id=p.id
    LEFT JOIN finance_journals rj ON rj.id=pr.reversal_journal_id AND rj.company_id=i.company_id
    WHERE x.invoice_id=i.id AND COALESCE(pj.entry_date,p.payment_date)<=h.as_of
      AND (pj.status IN ('posted','reversed') OR (pj.id IS NULL AND p.status='settled'))
      AND (rj.id IS NULL OR rj.entry_date>h.as_of)
  ) settlements ON true
  LEFT JOIN LATERAL (
    SELECT SUM(r.total_minor) transaction_minor,
      SUM((SELECT COALESCE(SUM(CASE WHEN i.kind='receivable' THEN l.base_credit_minor-l.base_debit_minor ELSE l.base_debit_minor-l.base_credit_minor END),0)
        FROM finance_journal_lines l WHERE (l.journal_id=cj.id OR l.journal_id IN (
          SELECT fj.id FROM finance_journals fj WHERE fj.id=r.fx_journal_id AND fj.company_id=i.company_id
            AND fj.status IN ('posted','reversed') AND fj.entry_date<=h.as_of)) AND l.account_id=a.id)) base_minor
    FROM (SELECT "id","company_id","branch_id","order_id","fulfillment_id","invoice_id","kind","number","return_date","reason","status","subtotal_minor","tax_minor","total_minor","stock_journal_id","credit_journal_id","version","requested_by","approved_by","created_at","updated_at","credited_minor","refund_minor","base_credit_minor","base_credited_minor","base_refund_minor","fx_journal_id","goods_fx_journal_id" FROM commerce_returns) r
    JOIN finance_journals cj ON cj.id=r.credit_journal_id AND cj.company_id=i.company_id
    WHERE r.invoice_id=i.id AND cj.status IN ('posted','reversed') AND cj.entry_date<=h.as_of
  ) credits ON true
  LEFT JOIN LATERAL (
    SELECT SUM(c.total_minor) transaction_minor,
      SUM((SELECT COALESCE(SUM(l.base_credit_minor-l.base_debit_minor),0) FROM finance_journal_lines l WHERE l.journal_id=cj.id AND l.account_id=a.id)) base_minor
    FROM finance_billing_corrections c JOIN finance_journals cj ON cj.id=c.credit_journal_id AND cj.company_id=i.company_id
    WHERE c.invoice_id=i.id AND c.company_id=i.company_id AND c.status='posted' AND cj.status='posted' AND cj.entry_date<=h.as_of
  ) billing_credits ON true
  LEFT JOIN LATERAL (
    SELECT SUM(CASE WHEN rj.id IS NOT NULL THEN i.total_minor ELSE 0 END) transaction_minor,
      SUM(CASE WHEN rj.id IS NULL THEN 0 WHEN i.base_total_minor=0 AND i.fx_rate_micros=1000000 THEN i.total_minor ELSE i.base_total_minor END) base_minor
    FROM finance_journals rj WHERE rj.company_id=i.company_id AND rj.source_type='reversal'
      AND rj.source_id=j.id AND rj.status IN ('posted','reversed') AND rj.entry_date<=h.as_of
      AND i.source_type='opening_import'
  ) opening_reversal ON true
  LEFT JOIN LATERAL (
    SELECT SUM(p.amount_minor) transaction_minor,
      SUM((SELECT COALESCE(SUM(CASE WHEN i.kind='receivable' THEN l.base_debit_minor-l.base_credit_minor ELSE l.base_credit_minor-l.base_debit_minor END),0)
        FROM finance_journal_lines l WHERE (l.journal_id=pj.id OR l.journal_id=rfj.id) AND l.account_id=a.id)) base_minor
    FROM finance_refund_obligations o JOIN finance_refund_instructions ri ON ri.obligation_id=o.id
    JOIN finance_payments p ON p.id=ri.payment_id AND p.company_id=i.company_id
    JOIN finance_journals pj ON pj.id=p.journal_id AND pj.company_id=i.company_id
    LEFT JOIN finance_journals rfj ON rfj.id=ri.fx_journal_id AND rfj.company_id=i.company_id AND rfj.status IN ('posted','reversed') AND rfj.entry_date<=h.as_of
    WHERE o.invoice_id=i.id AND pj.status IN ('posted','reversed') AND pj.entry_date<=h.as_of
  ) refunds ON true
  LEFT JOIN LATERAL (
    SELECT SUM(l.adjustment_minor) base_minor FROM finance_fx_revaluation_lines l
    JOIN finance_fx_revaluation_runs r ON r.id=l.run_id AND r.company_id=i.company_id
    JOIN finance_periods rp ON rp.id=r.period_id
    WHERE l.invoice_id=i.id AND r.status='posted' AND rp.end_date<=h.as_of
  ) revaluation ON true
  WHERE COALESCE(j.entry_date,i.invoice_date)<=h.as_of
    AND (j.status IN ('posted','reversed') OR (j.id IS NULL AND i.status IN ('posted','partially_paid','paid')))
)"#;

pub async fn read_invoice_aging(
    c: &mut PgConnection,
    company: &str,
    kind: &str,
    as_of: &str,
    chart_id: Option<&str>,
) -> Result<Vec<Value>, Error> {
    serializable(c).await?;
    if !matches!(kind, "receivable" | "payable")
        || !valid_calendar_date(as_of)
        || company.is_empty()
        || company.len() > 200
        || chart_id.is_some_and(|value| value.is_empty() || value.len() > 200)
    {
        return Err(BusinessError::invalid(
            "FINANCE_INVOICE_AGING_QUERY_INVALID",
            "Use a valid aging kind, cutoff date and ledger.",
        )
        .into());
    }
    let sql = format!(
        "{INVOICE_HISTORY_CTE} \
         SELECT (to_jsonb(i) || jsonb_build_object(\
           'party_code',p.code,'party_name',p.name,\
           'outstanding_minor',i.historical_outstanding_minor,\
           'base_outstanding_minor',i.historical_base_outstanding_minor,\
           'bucket',CASE \
             WHEN $2::date<=i.due_date::date THEN 'current' \
             WHEN ($2::date-i.due_date::date)<=30 THEN '1-30' \
             WHEN ($2::date-i.due_date::date)<=60 THEN '31-60' \
             WHEN ($2::date-i.due_date::date)<=90 THEN '61-90' \
             ELSE '90+' END,\
           'as_of',$2::text))::text \
         FROM historical_invoice_balances i \
         JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id \
         WHERE i.kind=$3 AND i.historical_outstanding_minor>0 \
           AND ($4::text IS NULL OR i.chart_id=$4) \
         ORDER BY i.due_date,i.id"
    );
    sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
        .bind(company)
        .bind(as_of)
        .bind(kind)
        .bind(chart_id)
        .fetch_all(&mut *c)
        .await?
        .into_iter()
        .map(parse)
        .collect()
}
