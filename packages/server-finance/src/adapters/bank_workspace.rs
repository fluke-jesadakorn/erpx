//! Native Finance bank reconciliation workspace read. The caller owns the
//! Holding transaction, current IAM and purchase witness; this adapter owns the
//! Company/chart filters, cursor pages and selected movement evidence.
use base64::Engine;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

const PAGE_SIZE: usize = 30;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BankWorkspaceQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub print: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statement_cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_transaction_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BankWorkspacePage {
    pub items: Vec<Value>,
    pub next_cursor: Option<String>,
    pub previous_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BankWorkspace {
    pub accounts: Vec<Value>,
    pub statements: Vec<Value>,
    pub statement_page: Option<BankWorkspacePage>,
    pub transactions: Vec<Value>,
    pub transaction_page: Option<BankWorkspacePage>,
    pub payments: Vec<Value>,
    pub selected_transaction: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Cursor {
    key: String,
    id: String,
    reverse: bool,
    scope: String,
}

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_BANK_WORKSPACE_UNAVAILABLE",
        "The bank reconciliation workspace is unavailable.",
    )
    .into()
}

fn invalid_query() -> Error {
    BusinessError::invalid(
        "FINANCE_BANK_QUERY_INVALID",
        "Use the bank reconciliation filters.",
    )
    .into()
}

fn invalid_cursor() -> Error {
    BusinessError::invalid("INVALID_CURSOR", "Restart this search to load results.").into()
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

/// Serialized snapshot read of the retained bank workspace sources. Shared
/// table gates match the command order without taking writer ownership.
pub async fn lock_bank_workspace_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE finance_bank_statements,finance_bank_transactions,finance_payments,\
         finance_journals,finance_accounts,finance_charts,finance_book_rules IN SHARE MODE",
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

fn scope_hash(scope: &Value) -> Result<String, Error> {
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
    if cursor.scope.len() != 64 {
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

fn valid_query(query: &BankWorkspaceQuery) -> bool {
    let valid = |value: &Option<String>, max: usize| {
        value
            .as_deref()
            .is_none_or(|value| value.len() <= max && !value.chars().any(char::is_control))
    };
    valid(&query.print, 16)
        && valid(&query.transaction_cursor, 4096)
        && valid(&query.statement_cursor, 4096)
        && valid(&query.matched_transaction_id, 200)
        && valid(&query.new, 16)
        && valid(&query.chart, 200)
        && valid(&query.view, 40)
        && valid(&query.q, 200)
}

fn value_or_empty(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("")
}

async fn rows(c: &mut PgConnection, sql: &str, binds: &[&str]) -> Result<Vec<Value>, Error> {
    let mut statement = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql));
    for bind in binds {
        statement = statement.bind(*bind);
    }
    statement
        .fetch_all(&mut *c)
        .await?
        .into_iter()
        .map(parse)
        .collect()
}

async fn page(
    c: &mut PgConnection,
    base: &str,
    binds: &[&str],
    cursor: Option<&str>,
    scope: &str,
) -> Result<BankWorkspacePage, Error> {
    let cursor = cursor.map(decode_cursor).transpose()?;
    if cursor.as_ref().is_some_and(|cursor| cursor.scope != scope) {
        return Err(invalid_cursor());
    }
    let reverse = cursor.as_ref().is_some_and(|value| value.reverse);
    let descending = !reverse;
    let direction = if descending { "DESC" } else { "ASC" };
    let comparison = if descending { "<" } else { ">" };
    let key_position = binds.len() + 1;
    let cursor_clause = if cursor.is_some() {
        format!(
            "WHERE (page_key,id) {comparison} (${key_position},${})",
            key_position + 1
        )
    } else {
        String::new()
    };
    let sql = format!(
        "WITH list AS ({base}) SELECT value::text FROM list {cursor_clause} \
         ORDER BY page_key {direction},id {direction} LIMIT 31"
    );
    let mut statement = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql));
    for bind in binds {
        statement = statement.bind(*bind);
    }
    if let Some(cursor) = &cursor {
        statement = statement.bind(&cursor.key).bind(&cursor.id);
    }
    let mut rows = statement.fetch_all(&mut *c).await?;
    let overflow = rows.len() > PAGE_SIZE;
    rows.truncate(PAGE_SIZE);
    if reverse {
        rows.reverse();
    }
    let items = rows.into_iter().map(parse).collect::<Result<Vec<_>, _>>()?;
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
            scope,
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
            scope,
        )?)
    } else {
        None
    };
    let has_more = next_cursor.is_some();
    Ok(BankWorkspacePage {
        items,
        next_cursor,
        previous_cursor,
        has_more,
    })
}

const STATEMENT_BASE: &str = "SELECT (to_jsonb(s) || jsonb_build_object(\
    'currency',a.currency,'base_currency',c.base_currency,\
    'page_key',s.statement_date))::text AS value,s.statement_date AS page_key,s.id AS id \
    FROM finance_bank_statements s \
    JOIN finance_accounts a ON a.id=s.bank_account AND a.company_id=s.company_id \
    JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=s.company_id \
    WHERE s.company_id=$1 AND ($2='' OR a.chart_id=$2)";

const TRANSACTION_BASE: &str = "SELECT (to_jsonb(t) || jsonb_build_object(\
    'bank_account',s.bank_account,'statement_number',s.statement_number,\
    'page_key',t.transaction_date))::text AS value,t.transaction_date AS page_key,t.id AS id \
    FROM finance_bank_transactions t \
    JOIN finance_bank_statements s ON s.id=t.statement_id AND s.company_id=t.company_id \
    JOIN finance_accounts a ON a.id=s.bank_account AND a.company_id=t.company_id \
    WHERE t.company_id=$1 AND ($2='' OR a.chart_id=$2) \
    AND ($3<>'unmatched' OR t.match_status<>'matched') \
    AND ($4='' OR strpos(lower(t.external_id||' '||COALESCE(t.description,'')),lower($4))>0)";

pub async fn read_bank_workspace(
    c: &mut PgConnection,
    company: &str,
    query: &BankWorkspaceQuery,
) -> Result<BankWorkspace, Error> {
    serializable(c).await?;
    if company.is_empty() || company.len() > 200 || !valid_query(query) {
        return Err(invalid_query());
    }
    let chart = value_or_empty(&query.chart);
    let view = value_or_empty(&query.view);
    let search = value_or_empty(&query.q);
    let matched = query
        .matched_transaction_id
        .as_deref()
        .filter(|value| !value.is_empty());
    let paged = query.print.as_deref() != Some("1");

    let accounts = if query.new.as_deref() == Some("1") {
        rows(
            c,
            "SELECT jsonb_build_object('id',a.id,'chart_id',a.chart_id,'code',a.code,\
             'name',a.name,'currency',a.currency,'base_currency',c.base_currency)::text \
             FROM finance_accounts a \
             JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=a.company_id \
             WHERE a.company_id=$1 AND a.category='asset' \
             AND a.control_kind IN ('none','bank') AND a.status='active' AND c.status='active' \
             AND NOT EXISTS(SELECT 1 FROM finance_book_rules r \
               WHERE r.company_id=a.company_id AND r.secondary_chart_id=a.chart_id AND r.status='approved') \
             ORDER BY CASE WHEN a.control_kind='bank' THEN 0 ELSE 1 END,a.code,a.id",
            &[company],
        )
        .await?
    } else {
        Vec::new()
    };

    let statement_scope = scope_hash(&json!([company, "bank-statements", chart]))?;
    let (statements, statement_page) = if paged {
        let page = page(
            c,
            STATEMENT_BASE,
            &[company, chart],
            query.statement_cursor.as_deref(),
            &statement_scope,
        )
        .await?;
        (page.items.clone(), Some(page))
    } else {
        let sql = format!("{STATEMENT_BASE} ORDER BY s.statement_date DESC,s.id");
        (rows(c, &sql, &[company, chart]).await?, None)
    };

    let transaction_scope =
        scope_hash(&json!([company, "bank-transactions", chart, view, search]))?;
    let (transactions, transaction_page) = if paged {
        let page = page(
            c,
            TRANSACTION_BASE,
            &[company, chart, view, search],
            query.transaction_cursor.as_deref(),
            &transaction_scope,
        )
        .await?;
        (page.items.clone(), Some(page))
    } else {
        let sql = format!("{TRANSACTION_BASE} ORDER BY t.transaction_date DESC,t.id");
        (rows(c, &sql, &[company, chart, view, search]).await?, None)
    };

    let payments = if matched.is_some() {
        rows(
            c,
            "SELECT to_jsonb(p)::text FROM finance_payments p \
             JOIN finance_journals j ON j.id=p.journal_id AND j.company_id=p.company_id \
             WHERE p.company_id=$1 AND p.status IN ('released','settled') AND j.status='posted' \
             AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions t \
               WHERE t.company_id=p.company_id AND t.matched_type='payment' \
               AND t.matched_id=p.id AND t.match_status='matched') \
             ORDER BY p.payment_date DESC",
            &[company],
        )
        .await?
    } else {
        Vec::new()
    };

    let selected_transaction = match matched {
        Some(id) => {
            let found = rows(
                c,
                "SELECT (to_jsonb(t) || jsonb_build_object(\
                 'bank_account',s.bank_account,'statement_number',s.statement_number))::text \
                 FROM finance_bank_transactions t \
                 JOIN finance_bank_statements s ON s.id=t.statement_id AND s.company_id=t.company_id \
                 WHERE t.id=$1 AND t.company_id=$2",
                &[id, company],
            )
            .await?
            .into_iter()
            .next();
            Some(found.ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "BANK_TRANSACTION_NOT_FOUND",
                    "Bank movement not found in this Company.",
                )
            })?)
        }
        None => None,
    };

    Ok(BankWorkspace {
        accounts,
        statements,
        statement_page,
        transactions,
        transaction_page,
        payments,
        selected_transaction,
    })
}
