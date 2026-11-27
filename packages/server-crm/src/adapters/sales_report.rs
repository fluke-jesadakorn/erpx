//! Native CRM report. The current company, selected scope, and source history
//! entitlements come from composition on the same serializable owner unit.
use crate::domain::sales_report::{
    Filters, SalesReportQuery, SalesReportRead, SalesReportScope, SourceRows, Window, report,
    request_invalid, scope_denied,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const REPORT_SOURCES: &str = include_str!("sales_report_sources.sql");

pub async fn read_sales_report(
    connection: &mut PgConnection,
    scope: &SalesReportScope,
    query: SalesReportQuery,
    now: i64,
) -> Result<SalesReportRead, Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("CRM_SALES_REPORT_READ_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("CRM_SALES_REPORT_SERIALIZABLE_REQUIRED").into());
    }
    query.validate().map_err(Error::Business)?;
    if !scope.access.crm {
        return Err(Error::Business(scope_denied(
            "REPORT_ACCESS_REQUIRED",
            "The original current report authority and purchased history are required.",
        )));
    }
    let branch_id = query.branch_id.filter(|value| !value.is_empty());
    let property_id = query.property_id.filter(|value| !value.is_empty());
    let campaign_id = query.campaign_id.filter(|value| !value.is_empty());
    let owner_id = query.owner_id.filter(|value| !value.is_empty());
    if property_id.is_some() && !scope.access.hotel
        || campaign_id.is_some() && !scope.access.marketing
    {
        return Err(Error::Business(scope_denied(
            "REPORT_FILTER_UNAVAILABLE",
            "That report filter is unavailable under current source access.",
        )));
    }
    if let (Some(requested), Some(current)) = (&branch_id, &scope.branch_id)
        && requested != current
    {
        return Err(Error::Business(scope_denied(
            "REPORT_SCOPE",
            "Branch is outside your workspace.",
        )));
    }
    let (from, to, window) = date_window(connection, query.from, query.to, now).await?;
    let filters = Filters {
        from,
        to,
        branch_id,
        property_id,
        campaign_id,
        owner_id,
    };
    let row = sqlx::query(REPORT_SOURCES)
        .bind(&scope.company_id)
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .bind(filters.branch_id.as_deref())
        .bind(filters.property_id.as_deref())
        .bind(filters.campaign_id.as_deref())
        .bind(filters.owner_id.as_deref())
        .bind(&filters.from)
        .bind(&filters.to)
        .bind(window.end)
        .bind(&window.today)
        .bind(scope.sales_orders)
        .bind(scope.access.hotel)
        .bind(scope.access.marketing)
        .bind(scope.access.sales_revenue)
        .bind(scope.access.hotel_revenue)
        .fetch_one(&mut *connection)
        .await?;
    let sources: sqlx::types::Json<SourceRows> = row.try_get("sources")?;
    let mut options: Value = row.try_get("options")?;
    options["access"] = json!(scope.access);
    Ok(SalesReportRead {
        report: report(&filters, &window, sources.0, &scope.access)?,
        options,
    })
}

async fn date_window(
    connection: &mut PgConnection,
    from: Option<String>,
    to: Option<String>,
    now: i64,
) -> Result<(String, String, Window), Error> {
    let row = sqlx::query(
        "WITH wanted AS (
            SELECT coalesce($1::text, to_char(bangkok - interval '30 days','YYYY-MM-DD')) AS from_date,
                   coalesce($2::text, to_char(bangkok,'YYYY-MM-DD')) AS to_date
            FROM (SELECT to_timestamp($3::bigint::double precision / 1000) AT TIME ZONE 'Asia/Bangkok' AS bangkok) local_time
        ), dates AS (
            SELECT from_date, to_date,
                CASE WHEN pg_input_is_valid(from_date,'date') THEN
                    (extract(epoch FROM (from_date::date::timestamp AT TIME ZONE 'Asia/Bangkok')) * 1000)::bigint END AS start_at,
                CASE WHEN pg_input_is_valid(to_date,'date') THEN
                    (extract(epoch FROM ((to_date::date + 1)::timestamp AT TIME ZONE 'Asia/Bangkok')) * 1000)::bigint - 1 END AS end_at
            FROM wanted
        )
        SELECT *,to_char(to_timestamp(least(end_at,$3::bigint)::double precision / 1000)
                         AT TIME ZONE 'Asia/Bangkok','YYYY-MM-DD') AS today
        FROM dates",
    )
    .bind(from).bind(to).bind(now).fetch_one(connection).await?;
    let start = row.try_get::<Option<i64>, _>("start_at")?;
    let end = row.try_get::<Option<i64>, _>("end_at")?;
    let (Some(start), Some(end)) = (start, end) else {
        return Err(Error::Business(request_invalid()));
    };
    if start > end {
        return Err(Error::Business(BusinessError::invalid(
            "REPORT_DATE_RANGE",
            "The start date must not follow the end date.",
        )));
    }
    Ok((
        row.try_get("from_date")?,
        row.try_get("to_date")?,
        Window {
            start,
            end,
            today: row.try_get("today")?,
        },
    ))
}
