//! Native Finance journal list read. The initial-stock purchase filter is
//! embedded verbatim from the Finance owner predicate and kept in sync by the
//! `finance.initial-stock-journal-filter.unit` test.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_JOURNAL_LIST_UNAVAILABLE",
        "The journal list is unavailable.",
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

pub async fn lock_journal_list_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE finance_charts,finance_accounts,finance_periods,\
         finance_dimension_definitions,finance_dimension_values,finance_journals,\
         finance_journal_lines,finance_book_postings,finance_manual_draft_editions,\
         finance_accounting_profiles,command_receipts,inventory_counts,\
         company_package_grants,company_package_history_rights IN SHARE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct JournalList {
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub charts: Vec<Value>,
    #[cfg_attr(feature = "contracts", ts(type = "Record<string, unknown>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Object))]
    pub chart: Value,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub accounts: Vec<Value>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub periods: Vec<Value>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub dimensions: Vec<Value>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub dimension_values: Vec<Value>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub journals: Vec<Value>,
    pub next_cursor: Option<String>,
}

fn parse(value: String) -> Result<Value, Error> {
    serde_json::from_str(&value).map_err(|_| unavailable())
}

fn parse_cursor(value: &str) -> Result<(i64, String), Error> {
    let (created, id) = value.split_once(':').ok_or_else(|| {
        BusinessError::invalid(
            "FINANCE_JOURNAL_CURSOR_INVALID",
            "Use the cursor from the current journal page.",
        )
    })?;
    let created_at = created.parse::<i64>().map_err(|_| {
        BusinessError::invalid(
            "FINANCE_JOURNAL_CURSOR_INVALID",
            "Use the cursor from the current journal page.",
        )
    })?;
    if !(0..=9_007_199_254_740_991).contains(&created_at) || id.is_empty() || id.len() > 200 {
        return Err(BusinessError::invalid(
            "FINANCE_JOURNAL_CURSOR_INVALID",
            "Use the cursor from the current journal page.",
        )
        .into());
    }
    Ok((created_at, id.to_owned()))
}

const JOURNAL_SQL: &str = concat!(
    "WITH finance_journal_clock AS MATERIALIZED(SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms) ",
    "SELECT (to_jsonb(j) || jsonb_build_object(",
    "'page_key',j.created_at::text,",
    "'debit',COALESCE((SELECT SUM(debit_minor) FROM finance_journal_lines WHERE journal_id=j.id),0),",
    "'credit',COALESCE((SELECT SUM(credit_minor) FROM finance_journal_lines WHERE journal_id=j.id),0),",
    "'line_count',(SELECT COUNT(*) FROM finance_journal_lines WHERE journal_id=j.id)",
    "))::text FROM finance_journals j ",
    "WHERE j.chart_id=$1 AND j.company_id=$2 AND ($3='' OR $3='all' OR j.status=$3) ",
    "AND (SELECT ",
    include_str!("initial_stock_journal_filter.sql"),
    " FROM finance_journal_clock) ",
    "AND ($4::bigint IS NULL OR j.created_at<$4 OR (j.created_at=$4 AND j.id<$5)) ",
    "ORDER BY j.created_at DESC,j.id DESC LIMIT 51"
);

pub async fn read_journal_list(
    c: &mut PgConnection,
    company: &str,
    chart_id: Option<&str>,
    status: &str,
    cursor: Option<&str>,
) -> Result<JournalList, Error> {
    serializable(c).await?;
    if company.is_empty() || company.len() > 200 {
        return Err(unavailable());
    }
    let charts: Vec<Value> = sqlx::query_scalar::<_, String>(
        "SELECT to_jsonb(c)::text FROM finance_charts c WHERE c.company_id=$1 ORDER BY c.id",
    )
    .bind(company)
    .fetch_all(&mut *c)
    .await?
    .into_iter()
    .map(parse)
    .collect::<Result<_, _>>()?;
    if charts.is_empty() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "CHART_REQUIRED",
            "Create a chart of accounts first.",
        )
        .into());
    }
    let chart_row = match chart_id {
        Some(id) => charts
            .iter()
            .find(|row| row.get("id").and_then(Value::as_str) == Some(id))
            .cloned()
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "CHART_NOT_FOUND",
                    "Ledger not found in this Company.",
                )
            })?,
        None => {
            let statutory: Option<String> = sqlx::query_scalar(
                "SELECT statutory_chart_id FROM finance_accounting_profiles WHERE company_id=$1 AND status='approved' ORDER BY effective_date DESC,id DESC LIMIT 1",
            )
            .bind(company)
            .fetch_optional(&mut *c)
            .await?;
            statutory
                .and_then(|id| {
                    charts
                        .iter()
                        .find(|row| row.get("id").and_then(Value::as_str) == Some(id.as_str()))
                        .cloned()
                })
                .or_else(|| {
                    charts
                        .iter()
                        .find(|row| row.get("name").and_then(Value::as_str) == Some("Operations ledger"))
                        .cloned()
                })
                .unwrap_or_else(|| charts[0].clone())
        }
    };
    let chart = chart_row
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(unavailable)?
        .to_owned();
    let accounts: Vec<Value> = sqlx::query_scalar::<_, String>(
        "SELECT to_jsonb(a)::text FROM finance_accounts a WHERE a.company_id=$1 AND a.chart_id=$2 ORDER BY a.code,a.id LIMIT 10001",
    )
    .bind(company)
    .bind(&chart)
    .fetch_all(&mut *c)
    .await?
    .into_iter()
    .map(parse)
    .collect::<Result<_, _>>()?;
    let periods: Vec<Value> = sqlx::query_scalar::<_, String>(
        "SELECT to_jsonb(p)::text FROM finance_periods p WHERE p.company_id=$1 ORDER BY p.start_date DESC,p.id LIMIT 10001",
    )
    .bind(company)
    .fetch_all(&mut *c)
    .await?
    .into_iter()
    .map(parse)
    .collect::<Result<_, _>>()?;
    let dimensions: Vec<Value> = sqlx::query_scalar::<_, String>(
        "SELECT jsonb_build_object('id',d.id,'code',d.code,'name',d.name,'required',d.required)::text FROM finance_dimension_definitions d WHERE d.company_id=$1 AND d.status='active' ORDER BY d.code LIMIT 10001",
    )
    .bind(company)
    .fetch_all(&mut *c)
    .await?
    .into_iter()
    .map(parse)
    .collect::<Result<_, _>>()?;
    let dimension_values: Vec<Value> = sqlx::query_scalar::<_, String>(
        "SELECT jsonb_build_object('id',v.id,'dimension_id',v.dimension_id,'code',v.code,'name',v.name)::text FROM finance_dimension_values v WHERE v.company_id=$1 AND v.status='active' ORDER BY v.code LIMIT 10001",
    )
    .bind(company)
    .fetch_all(&mut *c)
    .await?
    .into_iter()
    .map(parse)
    .collect::<Result<_, _>>()?;
    let (created_at, id) = match cursor {
        Some(value) => {
            let (created, id) = parse_cursor(value)?;
            (Some(created), id)
        }
        None => (None, String::new()),
    };
    let mut journals: Vec<Value> = sqlx::query_scalar::<_, String>(JOURNAL_SQL)
        .bind(&chart)
        .bind(company)
        .bind(status)
        .bind(created_at)
        .bind(&id)
        .fetch_all(&mut *c)
        .await?
        .into_iter()
        .map(parse)
        .collect::<Result<_, _>>()?;
    let next_cursor = if journals.len() > 50 {
        journals.truncate(50);
        journals.last().and_then(|row| {
            Some(format!(
                "{}:{}",
                row.get("created_at")?.as_i64()?,
                row.get("id")?.as_str()?
            ))
        })
    } else {
        None
    };
    Ok(JournalList {
        charts,
        chart: chart_row,
        accounts,
        periods,
        dimensions,
        dimension_values,
        journals,
        next_cursor,
    })
}
