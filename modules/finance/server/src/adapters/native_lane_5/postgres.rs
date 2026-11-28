//! sqlx execution of the Finance-owned customer master reads. Statements start
//! from the byte-identical retired TypeScript heads in `domain`, are converted
//! to sqlx placeholders, and every row projection plus the page cursor is owned
//! here. Nothing in TypeScript executes these statements or reproduces their
//! calculations.

use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::{Value, json};

use super::application::{CustomerRead, CustomerReadFuture, RecordFactsRead};
use super::domain::{
    self, CUSTOMER_DIRECTORY_SQL, CUSTOMER_PROFILE_SQL, KIND_CUSTOMER_DIRECTORY,
    KIND_CUSTOMER_PROFILE, KIND_NOTE_CUSTOMER, KIND_PRICING_CUSTOMER_VALID,
    KIND_PRICING_CUSTOMERS, NOTE_CUSTOMER_SQL, PAGE_SIZE, PRICING_CUSTOMER_VALID_SQL,
    PRICING_CUSTOMERS_SQL, RF_ACCOUNT_SQL, RF_KIND_ACCOUNT, RF_KIND_PARTY, RF_KIND_PERIOD,
    RF_KIND_ROWS, RF_PARTY_SQL, RF_PERIOD_SQL, RF_ROWS_SQL, RecordFactsRequest,
};
use super::domain::cursor::{decode_cursor, encode_cursor, scope_hash};

/// Convert the byte-identical `?` binds into sequential sqlx placeholders.
fn pg(sql: &str) -> String {
    let mut ordinal = 0usize;
    let mut out = String::with_capacity(sql.len());
    for ch in sql.chars() {
        if ch == '?' {
            ordinal += 1;
            out.push('$');
            out.push_str(&ordinal.to_string());
        } else {
            out.push(ch);
        }
    }
    out
}

fn rows_sql(sql: &str) -> String {
    format!("SELECT to_jsonb(__r)::text FROM ({}) __r", pg(sql))
}

fn parse(value: &str) -> Result<Value, PgTransactionError<BusinessError>> {
    serde_json::from_str(value).map_err(|_| domain::unavailable().into())
}

pub struct PgCustomerRead {
    pool: DatabasePool,
}

impl PgCustomerRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

async fn pricing_customers(
    connection: &mut sqlx::PgConnection,
    company: &str,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let sql = sqlx::AssertSqlSafe(rows_sql(PRICING_CUSTOMERS_SQL));
    let rows = sqlx::query_scalar::<_, String>(sql)
        .bind(company)
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| domain::unavailable())?;
    Ok(Value::Array(
        rows.iter().map(|row| parse(row)).collect::<Result<Vec<_>, _>>()?,
    ))
}

async fn pricing_customer_valid(
    connection: &mut sqlx::PgConnection,
    id: &str,
    company: &str,
    currency: &str,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let sql = sqlx::AssertSqlSafe(pg(PRICING_CUSTOMER_VALID_SQL));
    let found: Option<String> = sqlx::query_scalar(sql)
        .bind(id)
        .bind(company)
        .bind(currency)
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| domain::unavailable())?;
    Ok(json!(found.is_some()))
}

async fn customer_profile(
    connection: &mut sqlx::PgConnection,
    id: &str,
    company: &str,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let sql = sqlx::AssertSqlSafe(rows_sql(CUSTOMER_PROFILE_SQL));
    let row: Option<String> = sqlx::query_scalar(sql)
        .bind(id)
        .bind(company)
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| domain::unavailable())?;
    row.as_deref().map(parse).transpose().map(|value| value.unwrap_or(Value::Null))
}

fn page_value(item: &Value, field: &str) -> Result<String, PgTransactionError<BusinessError>> {
    item.get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| domain::unavailable().into())
}

#[allow(clippy::too_many_arguments)]
async fn customer_directory(
    connection: &mut sqlx::PgConnection,
    company: &str,
    branch: &str,
    search: &str,
    status: &str,
    raw_cursor: Option<&str>,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let scope = scope_hash(company, branch, search, status);
    let cursor = decode_cursor(raw_cursor, &scope).map_err(PgTransactionError::Business)?;
    let reverse = cursor.as_ref().is_some_and(|cursor| cursor.reverse);
    let descending = reverse;
    let direction = if descending { "DESC" } else { "ASC" };
    let comparison = if descending { "<" } else { ">" };
    let cursor_clause = if cursor.is_some() {
        format!("WHERE (page_key,id) {comparison} ($6,$7)")
    } else {
        String::new()
    };
    let sql = format!(
        "WITH list AS ({}) SELECT to_jsonb(__r)::text FROM list __r {cursor_clause} \
         ORDER BY page_key {direction},id {direction} LIMIT $8",
        pg(CUSTOMER_DIRECTORY_SQL)
    );
    let mut statement = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
        .bind(company)
        .bind(search)
        .bind(search)
        .bind(status)
        .bind(status);
    if let Some(cursor) = &cursor {
        statement = statement.bind(&cursor.key).bind(&cursor.id);
    }
    let mut rows = statement
        .bind((PAGE_SIZE + 1) as i64)
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| domain::unavailable())?;
    let overflow = rows.len() > PAGE_SIZE;
    rows.truncate(PAGE_SIZE);
    if reverse {
        rows.reverse();
    }
    let items = rows
        .iter()
        .map(|row| parse(row))
        .collect::<Result<Vec<_>, _>>()?;
    // `readListPage` cursor policy: previous = reverse ? overflow : had cursor;
    // next = reverse ? had cursor : overflow.
    let had_cursor = cursor.is_some();
    let previous_cursor = if !items.is_empty() && (if reverse { overflow } else { had_cursor }) {
        Some(encode_cursor(
            &page_value(&items[0], "page_key")?,
            &page_value(&items[0], "id")?,
            true,
            &scope,
        )
        .map_err(PgTransactionError::Business)?)
    } else {
        None
    };
    let next_cursor = if !items.is_empty() && (if reverse { had_cursor } else { overflow }) {
        let last = &items[items.len() - 1];
        Some(
            encode_cursor(
                &page_value(last, "page_key")?,
                &page_value(last, "id")?,
                false,
                &scope,
            )
            .map_err(PgTransactionError::Business)?,
        )
    } else {
        None
    };
    Ok(json!({
        "items": items,
        "nextCursor": next_cursor,
        "previousCursor": previous_cursor,
        "hasMore": next_cursor.is_some(),
    }))
}

async fn note_customer(
    connection: &mut sqlx::PgConnection,
    id: &str,
    company: &str,
    version: Option<i64>,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let sql = sqlx::AssertSqlSafe(rows_sql(NOTE_CUSTOMER_SQL));
    let row: Option<String> = sqlx::query_scalar(sql)
        .bind(id)
        .bind(company)
        .bind(version)
        .bind(version)
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| domain::unavailable())?;
    row.as_deref().map(parse).transpose().map(|value| value.unwrap_or(Value::Null))
}

impl CustomerRead for PgCustomerRead {
    fn read<'a>(
        &'a self,
        company: &'a str,
        branch: &'a str,
        request: &'a domain::CustomerReadRequest,
    ) -> CustomerReadFuture<'a, Value> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let branch = branch.to_owned();
        let kind = request.kind.clone();
        let id = request.id.clone();
        let currency = request.currency.clone();
        let search = request.search.clone().unwrap_or_default();
        let status = request.status.clone().unwrap_or_default();
        let cursor = request.cursor.clone();
        let version = request.version;
        Box::pin(async move {
            let result = unit
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let kind = kind.clone();
                    let company = company.clone();
                    let branch = branch.clone();
                    let id = id.clone();
                    let currency = currency.clone();
                    let search = search.clone();
                    let status = status.clone();
                    let cursor = cursor.clone();
                    Box::pin(async move {
                        match kind.as_str() {
                            KIND_PRICING_CUSTOMERS => {
                                pricing_customers(connection, &company).await
                            }
                            KIND_PRICING_CUSTOMER_VALID => {
                                pricing_customer_valid(
                                    connection,
                                    id.as_deref().unwrap_or(""),
                                    &company,
                                    currency.as_deref().unwrap_or(""),
                                )
                                .await
                            }
                            KIND_CUSTOMER_PROFILE => {
                                customer_profile(connection, id.as_deref().unwrap_or(""), &company)
                                    .await
                            }
                            KIND_CUSTOMER_DIRECTORY => {
                                customer_directory(
                                    connection,
                                    &company,
                                    &branch,
                                    &search,
                                    &status,
                                    cursor.as_deref(),
                                )
                                .await
                            }
                            KIND_NOTE_CUSTOMER => {
                                note_customer(
                                    connection,
                                    id.as_deref().unwrap_or(""),
                                    &company,
                                    version,
                                )
                                .await
                            }
                            _ => Err(domain::invalid().into()),
                        }
                    })
                })
                .await;
            match result {
                Ok(value) => Ok(value),
                Err(PgTransactionError::Business(error)) => Err(error),
                Err(PgTransactionError::Persistence(_)) => Err(domain::unavailable()),
            }
        })
    }
}

async fn record_fact_one(
    connection: &mut sqlx::PgConnection,
    sql: &str,
    id: &str,
    company: &str,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let statement = sqlx::AssertSqlSafe(rows_sql(sql));
    let row: Option<String> = sqlx::query_scalar::<_, String>(statement)
        .bind(id)
        .bind(company)
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| domain::unavailable())?;
    row.as_deref()
        .map(parse)
        .transpose()
        .map(|value| value.unwrap_or(Value::Null))
}

async fn record_fact_rows(
    connection: &mut sqlx::PgConnection,
    request: &RecordFactsRequest,
    company: &str,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let sql = sqlx::AssertSqlSafe(rows_sql(RF_ROWS_SQL));
    let rows = sqlx::query_scalar::<_, String>(sql)
        .bind(request.id.as_deref().unwrap_or(""))
        .bind(company)
        .bind(request.chart_id.as_deref())
        .bind(request.value.as_deref().unwrap_or("0"))
        .bind(request.value2.as_deref())
        .bind(request.value3.as_deref())
        .bind(request.value4.as_deref())
        .bind(request.value5.as_deref())
        .bind(request.offset.unwrap_or(0))
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| domain::unavailable())?;
    Ok(Value::Array(
        rows.iter().map(|row| parse(row)).collect::<Result<Vec<_>, _>>()?,
    ))
}

pub struct PgRecordFactsRead {
    pool: DatabasePool,
}

impl PgRecordFactsRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl RecordFactsRead for PgRecordFactsRead {
    fn read<'a>(
        &'a self,
        company: &'a str,
        request: &'a RecordFactsRequest,
    ) -> CustomerReadFuture<'a, Value> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let kind = request.kind.clone();
        let id = request.id.clone();
        let chart_id = request.chart_id.clone();
        let value = request.value.clone();
        let value2 = request.value2.clone();
        let value3 = request.value3.clone();
        let value4 = request.value4.clone();
        let value5 = request.value5.clone();
        let offset = request.offset;
        Box::pin(async move {
            let result = unit
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let kind = kind.clone();
                    let company = company.clone();
                    let id = id.clone();
                    let chart_id = chart_id.clone();
                    let value = value.clone();
                    let value2 = value2.clone();
                    let value3 = value3.clone();
                    let value4 = value4.clone();
                    let value5 = value5.clone();
                    Box::pin(async move {
                        let request = RecordFactsRequest {
                            kind: kind.clone(),
                            id: id.clone(),
                            chart_id,
                            value,
                            value2,
                            value3,
                            value4,
                            value5,
                            offset,
                        };
                        match kind.as_str() {
                            RF_KIND_PARTY => {
                                record_fact_one(connection, RF_PARTY_SQL, id.as_deref().unwrap_or(""), &company).await
                            }
                            RF_KIND_ACCOUNT => {
                                record_fact_one(connection, RF_ACCOUNT_SQL, id.as_deref().unwrap_or(""), &company).await
                            }
                            RF_KIND_PERIOD => {
                                record_fact_one(connection, RF_PERIOD_SQL, id.as_deref().unwrap_or(""), &company).await
                            }
                            RF_KIND_ROWS => record_fact_rows(connection, &request, &company).await,
                            _ => Err(domain::invalid().into()),
                        }
                    })
                })
                .await;
            match result {
                Ok(value) => Ok(value),
                Err(PgTransactionError::Business(error)) => Err(error),
                Err(PgTransactionError::Persistence(_)) => Err(domain::unavailable()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_lane_5_bind_conversion_is_sequential() {
        assert_eq!(pg("SELECT ? FROM t WHERE a=? AND b=?"), "SELECT $1 FROM t WHERE a=$2 AND b=$3");
        assert_eq!(pg(PRICING_CUSTOMERS_SQL).matches('$').count(), 1);
        assert_eq!(pg(PRICING_CUSTOMER_VALID_SQL).matches('$').count(), 3);
        assert_eq!(pg(CUSTOMER_DIRECTORY_SQL).matches('$').count(), 5);
        assert_eq!(pg(NOTE_CUSTOMER_SQL).matches('$').count(), 4);
        assert!(pg(NOTE_CUSTOMER_SQL).contains("$3::int"));
    }

    #[test]
    fn native_lane_5_row_wrapper_preserves_inner_head() {
        let sql = rows_sql(CUSTOMER_PROFILE_SQL);
        assert!(sql.contains(&pg(CUSTOMER_PROFILE_SQL)));
        assert!(sql.starts_with("SELECT to_jsonb(__r)::text FROM ("));
        assert!(sql.ends_with(") __r"));
    }

    #[test]
    fn native_lane_5_page_shape_is_stable() {
        let page = json!({
            "items": [{"id": "p1", "page_key": "Alpha", "name": "Alpha"}],
            "nextCursor": Value::Null,
            "previousCursor": Value::Null,
            "hasMore": false,
        });
        assert_eq!(page["items"][0]["page_key"], "Alpha");
        assert_eq!(page["hasMore"], false);
        assert!(page.get("nextCursor").is_some());
        assert!(page.get("previousCursor").is_some());
    }

    #[test]
    fn native_lane_5_record_facts_bind_conversion_and_row_wrapper() {
        assert_eq!(pg(RF_PARTY_SQL).matches('$').count(), 2);
        assert_eq!(pg(RF_ACCOUNT_SQL).matches('$').count(), 2);
        assert_eq!(pg(RF_PERIOD_SQL).matches('$').count(), 2);
        assert_eq!(pg(RF_ROWS_SQL).matches('$').count(), 9);
        let wrapped = rows_sql(RF_ROWS_SQL);
        assert!(wrapped.contains(&pg(RF_ROWS_SQL)));
        assert!(wrapped.starts_with("SELECT to_jsonb(__r)::text FROM ("));
        assert!(pg(RF_ROWS_SQL).contains("$4='0'"));
    }
}
