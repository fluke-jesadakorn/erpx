//! Native Finance sales-invoice list read for the Sales workspace invoices
//! tab. The projection is source-neutral over `finance_invoices`; branch and
//! department scope is resolved from the original Commerce fulfillment lineage.
use base64::Engine;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

const PAGE_SIZE: usize = 30;

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SalesInvoiceListQuery {
    pub tab: String,
    pub q: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SalesInvoiceListPage {
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub items: Vec<Value>,
    pub next_cursor: Option<String>,
    pub previous_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SalesInvoiceList {
    pub query: SalesInvoiceListQuery,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub summary: Vec<Value>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub received: Vec<Value>,
    pub page: SalesInvoiceListPage,
    pub permission_required: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Cursor {
    key: String,
    id: String,
    reverse: bool,
    scope: String,
}

fn invalid_cursor() -> Error {
    BusinessError::invalid(
        "INVALID_CURSOR",
        "Restart the invoice list from its first page.",
    )
    .into()
}

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_SALES_INVOICE_LIST_UNAVAILABLE",
        "The sales invoice list is unavailable.",
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

pub async fn lock_sales_invoice_list_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE finance_invoices,finance_parties,finance_payments,\
         finance_customer_receipts,finance_journals,finance_allocations,\
         commerce_fulfillments,commerce_orders IN SHARE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
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

fn scope_hash(
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    query: &SalesInvoiceListQuery,
) -> Result<String, Error> {
    let scope = json!([
        company,
        branch,
        department,
        query.tab,
        query.q,
        query.status,
        query.start_date,
        query.end_date,
        query.currency,
        query.queue
    ]);
    let value = json!([scope, true, PAGE_SIZE]);
    serde_json::to_string(&value)
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
    let cursor = Cursor {
        key: key.to_owned(),
        id: id.to_owned(),
        reverse,
        scope: scope.to_owned(),
    };
    serde_json::to_vec(&cursor)
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

fn validate(query: &SalesInvoiceListQuery) -> Result<(), Error> {
    let invalid = |code: &'static str, message: &'static str| {
        BusinessError::invalid(code, message).into()
    };
    if query.tab != "invoices" {
        return Err(invalid(
            "FINANCE_SALES_TAB_UNSUPPORTED",
            "The native sales invoice list serves the invoices tab.",
        ));
    }
    if query.q.len() > 200 || query.status.len() > 40 {
        return Err(invalid(
            "FINANCE_SALES_QUERY_INVALID",
            "Use the sales invoice list filters.",
        ));
    }
    if query
        .start_date
        .as_deref()
        .is_some_and(|value| !valid_date(value))
        || query
            .end_date
            .as_deref()
            .is_some_and(|value| !valid_date(value))
    {
        return Err(invalid(
            "FINANCE_SALES_QUERY_INVALID",
            "Use ISO dates in the sales invoice list.",
        ));
    }
    if query
        .currency
        .as_deref()
        .is_some_and(|value| value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase()))
    {
        return Err(invalid(
            "FINANCE_SALES_QUERY_INVALID",
            "Use an ISO currency in the sales invoice list.",
        ));
    }
    if query
        .queue
        .as_deref()
        .is_some_and(|value| !matches!(value, "awaiting" | "overdue" | "received"))
    {
        return Err(invalid(
            "FINANCE_SALES_QUERY_INVALID",
            "Use a known sales invoice queue.",
        ));
    }
    Ok(())
}

const SUMMARY_SQL: &str = "SELECT jsonb_build_object(\
    'currency',i.currency,\
    'awaiting_minor',COALESCE(SUM(CASE WHEN i.status IN ('posted','partially_paid') THEN i.outstanding_minor ELSE 0 END),0),\
    'overdue_minor',COALESCE(SUM(CASE WHEN i.status IN ('posted','partially_paid') AND i.due_date<(now() AT TIME ZONE 'Asia/Bangkok')::date::text THEN i.outstanding_minor ELSE 0 END),0)\
    )::text \
    FROM finance_invoices i JOIN finance_parties party ON party.id=i.party_id \
    WHERE i.company_id=$1 AND i.kind='receivable' \
      AND (($2::text IS NULL AND $3::text IS NULL) OR EXISTS(\
        SELECT 1 FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id \
        WHERE f.invoice_id=i.id AND o.company_id=i.company_id \
          AND ($2::text IS NULL OR o.branch_id=$2) \
          AND ($3::text IS NULL OR o.department_id=$3))) \
      AND ($4='' OR strpos(lower(i.invoice_number||' '||party.name),lower($4))>0) \
      AND ($5='' OR i.invoice_date>=$5) AND ($6='' OR i.invoice_date<=$6) \
    GROUP BY i.currency ORDER BY i.currency";

const RECEIVED_SQL: &str = "SELECT jsonb_build_object('currency',p.currency,'received_minor',COALESCE(SUM(p.amount_minor),0))::text \
    FROM finance_payments p \
    LEFT JOIN finance_customer_receipts r ON r.payment_id=p.id \
    JOIN finance_parties party ON party.id=p.party_id \
    JOIN finance_journals j ON j.id=p.journal_id \
    WHERE p.company_id=$1 AND p.kind='receipt' AND p.status='settled' AND j.status='posted' \
      AND (($2::text IS NULL AND $3::text IS NULL) OR (EXISTS(SELECT 1 FROM finance_allocations WHERE payment_id=p.id) AND NOT EXISTS(\
        SELECT 1 FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id \
        WHERE a.payment_id=p.id AND NOT (($2::text IS NULL AND $3::text IS NULL) OR EXISTS(\
          SELECT 1 FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id \
          WHERE f.invoice_id=i.id AND o.company_id=i.company_id \
            AND ($2::text IS NULL OR o.branch_id=$2) \
            AND ($3::text IS NULL OR o.department_id=$3)))))) \
      AND ($4='' OR strpos(lower(COALESCE(r.document_number,'')||' '||party.name||' '||p.payment_number),lower($4))>0 \
        OR EXISTS(SELECT 1 FROM finance_allocations a JOIN finance_invoices related ON related.id=a.invoice_id \
          WHERE a.payment_id=p.id AND strpos(lower(related.invoice_number),lower($4))>0)) \
      AND ($5='' OR p.payment_date>=$5) AND ($6='' OR p.payment_date<=$6) \
    GROUP BY p.currency ORDER BY p.currency";

const PAGE_SQL: &str = "SELECT value::text FROM list {cursor} ORDER BY page_key {direction},id {direction} LIMIT 31";

pub async fn read_sales_invoice_list(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    query: &SalesInvoiceListQuery,
) -> Result<SalesInvoiceList, Error> {
    serializable(c).await?;
    validate(query)?;
    if company.is_empty() || company.len() > 200 {
        return Err(unavailable());
    }
    let scope = scope_hash(company, branch, department, query)?;
    let cursor = query.cursor.as_deref().map(decode_cursor).transpose()?;
    if cursor.as_ref().is_some_and(|cursor| cursor.scope != scope) {
        return Err(invalid_cursor());
    }
    let summary: Vec<Value> = sqlx::query_scalar::<_, String>(SUMMARY_SQL)
        .bind(company)
        .bind(branch)
        .bind(department)
        .bind(&query.q)
        .bind(query.start_date.as_deref().unwrap_or(""))
        .bind(query.end_date.as_deref().unwrap_or(""))
        .fetch_all(&mut *c)
        .await?
        .into_iter()
        .map(parse)
        .collect::<Result<_, _>>()?;
    let received: Vec<Value> = sqlx::query_scalar::<_, String>(RECEIVED_SQL)
        .bind(company)
        .bind(branch)
        .bind(department)
        .bind(&query.q)
        .bind(query.start_date.as_deref().unwrap_or(""))
        .bind(query.end_date.as_deref().unwrap_or(""))
        .fetch_all(&mut *c)
        .await?
        .into_iter()
        .map(parse)
        .collect::<Result<_, _>>()?;

    let inner = "SELECT (to_jsonb(i) || jsonb_build_object(\
        'number',i.invoice_number,\
        'date',i.invoice_date,\
        'party_name',party.name,\
        'page_key',i.created_at::text)) AS value,\
        i.created_at::text AS page_key,i.id AS id \
        FROM finance_invoices i JOIN finance_parties party ON party.id=i.party_id \
        WHERE i.company_id=$1 AND i.kind='receivable' \
          AND (($2::text IS NULL AND $3::text IS NULL) OR EXISTS(\
            SELECT 1 FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id \
            WHERE f.invoice_id=i.id AND o.company_id=i.company_id \
              AND ($2::text IS NULL OR o.branch_id=$2) \
              AND ($3::text IS NULL OR o.department_id=$3))) \
          AND ($4='' OR strpos(lower(i.invoice_number||' '||party.name),lower($4))>0) \
          AND ($5='' OR i.invoice_date>=$5) AND ($6='' OR i.invoice_date<=$6) \
          AND ($7='' OR i.status=$7) AND ($8='' OR i.currency=$8) \
          AND ($9 NOT IN ('awaiting','overdue') OR i.status IN ('posted','partially_paid') AND i.outstanding_minor>0) \
          AND ($9!='overdue' OR i.due_date<(now() AT TIME ZONE 'Asia/Bangkok')::date::text)";
    let reverse = cursor.as_ref().is_some_and(|value| value.reverse);
    let descending = !reverse;
    let direction = if descending { "DESC" } else { "ASC" };
    let comparison = if descending { "<" } else { ">" };
    let cursor_clause = if cursor.is_some() {
        format!("WHERE (page_key,id) {comparison} ($10,$11)")
    } else {
        String::new()
    };
    let sql = format!(
        "WITH list AS ({inner}) {}",
        PAGE_SQL
            .replace("{cursor}", &cursor_clause)
            .replace("{direction}", direction)
    );
    let mut statement = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
        .bind(company)
        .bind(branch)
        .bind(department)
        .bind(&query.q)
        .bind(query.start_date.as_deref().unwrap_or(""))
        .bind(query.end_date.as_deref().unwrap_or(""))
        .bind(&query.status)
        .bind(query.currency.as_deref().unwrap_or(""))
        .bind(query.queue.as_deref().unwrap_or(""));
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
            first
                .get("page_key")
                .and_then(Value::as_str)
                .ok_or_else(unavailable)?,
            first
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(unavailable)?,
            true,
            &scope,
        )?)
    } else {
        None
    };
    let next_cursor = if !items.is_empty() && (reverse || overflow) {
        let last = &items[items.len() - 1];
        Some(encode_cursor(
            last.get("page_key")
                .and_then(Value::as_str)
                .ok_or_else(unavailable)?,
            last.get("id")
                .and_then(Value::as_str)
                .ok_or_else(unavailable)?,
            false,
            &scope,
        )?)
    } else {
        None
    };
    let has_more = next_cursor.is_some();
    Ok(SalesInvoiceList {
        query: query.clone(),
        summary,
        received,
        page: SalesInvoiceListPage {
            items,
            next_cursor,
            previous_cursor,
            has_more,
        },
        permission_required: None,
    })
}
