//! Native Work Projection report reads.
//!
//! These are the exact PostgreSQL projection queries behind the retained
//! `workReportUnit` adapter: mode predicates, visible/matching CTEs, counts,
//! money and estimate sums, stage counts, the 50-row page and the CSV ordering.
//! The caller supplies the already-authorized, already-shaped report rows; the
//! read performs matching, aggregation and pagination on that policy-scoped
//! projection only. It opens no transaction and grants no authority.
use serde_json::Value;
use sqlx::{AssertSqlSafe, PgConnection, Row};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkReportMode {
    Period,
    Open,
    Drafts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkReportProjection<'a> {
    pub mode: WorkReportMode,
    /// `JSON.stringify(rows)` from the authorized report assembly.
    pub scoped: &'a str,
    /// Bangkok start-of-day epoch millis; unused for `open` and `drafts`.
    pub start: i64,
    /// Bangkok end-of-day epoch millis; unused for `open` and `drafts`.
    pub end: i64,
    pub page: i64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkReportCounts {
    pub total: i64,
    pub open: i64,
    pub drafts: i64,
    pub unknown: i64,
    pub blocked: i64,
    pub units: f64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkReportCurrencyTotal {
    pub currency: String,
    pub amount: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkReportStageCount {
    pub stage: String,
    pub count: i64,
    pub oldest: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkReportRead {
    pub counts: WorkReportCounts,
    pub money: Vec<WorkReportCurrencyTotal>,
    pub estimates: Vec<WorkReportCurrencyTotal>,
    pub stage_counts: Vec<WorkReportStageCount>,
    /// The requested page; always 50 rows maximum.
    pub rows: Vec<Value>,
    /// Every matching row in CSV order; empty unless export was requested.
    pub all_rows: Vec<Value>,
}

/// Page size is the retained adapter's literal `LIMIT 50`.
pub const WORK_REPORT_PAGE_SIZE: i64 = 50;

fn cte(mode: WorkReportMode) -> &'static str {
    match mode {
        WorkReportMode::Period => {
            "WITH visible AS (SELECT value row FROM jsonb_array_elements($1::jsonb)), matching AS (SELECT row FROM visible WHERE (row->>'submittedAt')::bigint BETWEEN $2 AND $3) "
        }
        WorkReportMode::Open => {
            "WITH visible AS (SELECT value row FROM jsonb_array_elements($1::jsonb)), matching AS (SELECT row FROM visible WHERE (row->>'open')::boolean) "
        }
        WorkReportMode::Drafts => {
            "WITH visible AS (SELECT value row FROM jsonb_array_elements($1::jsonb)), matching AS (SELECT row FROM visible WHERE (row->>'draft')::boolean) "
        }
    }
}

fn bind_period<'a>(
    query: sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments>,
    projection: &WorkReportProjection<'_>,
) -> sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments> {
    let query = query.bind(projection.scoped.to_owned());
    match projection.mode {
        WorkReportMode::Period => query.bind(projection.start).bind(projection.end),
        WorkReportMode::Open | WorkReportMode::Drafts => query,
    }
}

/// Exact `counts` projection.
pub async fn read_work_report_counts(
    connection: &mut PgConnection,
    projection: &WorkReportProjection<'_>,
) -> Result<WorkReportCounts, sqlx::Error> {
    let sql = format!(
        "{cte}SELECT (SELECT COUNT(*) FROM matching) total,\
(SELECT COUNT(*) FROM visible WHERE (row->>'open')::boolean) open,\
(SELECT COUNT(*) FROM visible WHERE (row->>'draft')::boolean) drafts,\
(SELECT COUNT(*) FROM visible WHERE NOT (row->>'draft')::boolean AND row->>'submittedAt' IS NULL) unknown,\
(SELECT COUNT(*) FROM matching WHERE row->>'blocker' IS NOT NULL) blocked,\
(SELECT COALESCE(SUM((row->>'units')::numeric),0)::double precision FROM matching) units",
        cte = cte(projection.mode)
    );
    let row = bind_period(sqlx::query(AssertSqlSafe(sql)), projection)
        .fetch_one(&mut *connection)
        .await?;
    Ok(WorkReportCounts {
        total: row.try_get("total")?,
        open: row.try_get("open")?,
        drafts: row.try_get("drafts")?,
        unknown: row.try_get("unknown")?,
        blocked: row.try_get("blocked")?,
        units: row.try_get("units")?,
    })
}

async fn currency_totals(
    connection: &mut PgConnection,
    projection: &WorkReportProjection<'_>,
    field: &'static str,
) -> Result<Vec<WorkReportCurrencyTotal>, sqlx::Error> {
    let sql = format!(
        "{cte}SELECT row->>'currency' currency,SUM((row->>'{field}')::bigint)::bigint amount \
         FROM matching WHERE row->>'currency' IS NOT NULL AND row->>'{field}' IS NOT NULL \
         GROUP BY row->>'currency'",
        cte = cte(projection.mode)
    );
    let rows = bind_period(sqlx::query(AssertSqlSafe(sql)), projection)
        .fetch_all(&mut *connection)
        .await?;
    rows.into_iter()
        .map(|row| {
            Ok(WorkReportCurrencyTotal {
                currency: row.try_get("currency")?,
                amount: row.try_get("amount")?,
            })
        })
        .collect()
}

/// Exact `money` projection (`amount` in minor units).
pub async fn read_work_report_money(
    connection: &mut PgConnection,
    projection: &WorkReportProjection<'_>,
) -> Result<Vec<WorkReportCurrencyTotal>, sqlx::Error> {
    currency_totals(connection, projection, "amount").await
}

/// Exact `estimates` projection (`estimate` in minor units).
pub async fn read_work_report_estimates(
    connection: &mut PgConnection,
    projection: &WorkReportProjection<'_>,
) -> Result<Vec<WorkReportCurrencyTotal>, sqlx::Error> {
    currency_totals(connection, projection, "estimate").await
}

/// Exact `stageCounts` projection.
pub async fn read_work_report_stage_counts(
    connection: &mut PgConnection,
    projection: &WorkReportProjection<'_>,
) -> Result<Vec<WorkReportStageCount>, sqlx::Error> {
    let sql = format!(
        "{cte}SELECT row->>'stage' stage,COUNT(*) count,MAX((row->>'stageWaitDays')::int) oldest \
         FROM matching GROUP BY row->>'stage' ORDER BY COUNT(*) DESC",
        cte = cte(projection.mode)
    );
    let rows = bind_period(sqlx::query(AssertSqlSafe(sql)), projection)
        .fetch_all(&mut *connection)
        .await?;
    rows.into_iter()
        .map(|row| {
            Ok(WorkReportStageCount {
                stage: row.try_get("stage")?,
                count: row.try_get("count")?,
                oldest: row.try_get("oldest")?,
            })
        })
        .collect()
}

fn page_rows(rows: Vec<sqlx::postgres::PgRow>) -> Result<Vec<Value>, sqlx::Error> {
    rows.into_iter()
        .map(|row| row.try_get::<Value, _>("row"))
        .collect()
}

/// Exact `page` projection: newest `updated_at` first, 50 rows from `page`.
pub async fn read_work_report_page(
    connection: &mut PgConnection,
    projection: &WorkReportProjection<'_>,
) -> Result<Vec<Value>, sqlx::Error> {
    let limit = placeholder(projection.mode, 1);
    let offset_placeholder = placeholder(projection.mode, 2);
    let sql = format!(
        "{cte}SELECT row FROM matching ORDER BY (row->>'updated_at')::bigint DESC,row->>'id' LIMIT ${limit} OFFSET ${offset_placeholder}",
        cte = cte(projection.mode)
    );
    let offset = projection
        .page
        .saturating_sub(1)
        .saturating_mul(WORK_REPORT_PAGE_SIZE);
    let query = bind_period(sqlx::query(AssertSqlSafe(sql)), projection)
        .bind(WORK_REPORT_PAGE_SIZE)
        .bind(offset);
    page_rows(query.fetch_all(&mut *connection).await?)
}

/// Exact `allRows` projection for CSV export (same order, no limit).
pub async fn read_work_report_all_rows(
    connection: &mut PgConnection,
    projection: &WorkReportProjection<'_>,
) -> Result<Vec<Value>, sqlx::Error> {
    let sql = format!(
        "{cte}SELECT row FROM matching ORDER BY (row->>'updated_at')::bigint DESC,row->>'id'",
        cte = cte(projection.mode)
    );
    let query = bind_period(sqlx::query(AssertSqlSafe(sql)), projection);
    page_rows(query.fetch_all(&mut *connection).await?)
}

/// The next positional placeholder after the mode's fixed bindings.
fn placeholder(mode: WorkReportMode, extra: usize) -> usize {
    match mode {
        WorkReportMode::Period => 3 + extra,
        WorkReportMode::Open | WorkReportMode::Drafts => 1 + extra,
    }
}

/// Reads every projection the retained `workReportUnit` reads. `export_all`
/// adds the unbounded CSV ordering on the same authorized scoped rows.
pub async fn read_work_report_projection(
    connection: &mut PgConnection,
    projection: &WorkReportProjection<'_>,
    export_all: bool,
) -> Result<WorkReportRead, sqlx::Error> {
    let counts = read_work_report_counts(connection, projection).await?;
    let money = read_work_report_money(connection, projection).await?;
    let estimates = read_work_report_estimates(connection, projection).await?;
    let stage_counts = read_work_report_stage_counts(connection, projection).await?;
    let rows = read_work_report_page(connection, projection).await?;
    let all_rows = if export_all {
        read_work_report_all_rows(connection, projection).await?
    } else {
        Vec::new()
    };
    Ok(WorkReportRead {
        counts,
        money,
        estimates,
        stage_counts,
        rows,
        all_rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn period_placeholders_follow_the_fixed_bindings() {
        assert_eq!(placeholder(WorkReportMode::Period, 1), 4);
        assert_eq!(placeholder(WorkReportMode::Period, 2), 5);
        assert_eq!(placeholder(WorkReportMode::Open, 1), 2);
        assert_eq!(placeholder(WorkReportMode::Drafts, 2), 3);
    }

    #[test]
    fn modes_keep_the_retained_visible_and_matching_predicates() {
        assert!(cte(WorkReportMode::Open).contains("(row->>'open')::boolean"));
        assert!(cte(WorkReportMode::Drafts).contains("(row->>'draft')::boolean"));
        assert!(
            cte(WorkReportMode::Period).contains("(row->>'submittedAt')::bigint BETWEEN $2 AND $3")
        );
    }

    #[test]
    fn page_size_matches_the_retained_limit() {
        assert_eq!(WORK_REPORT_PAGE_SIZE, 50);
    }
}
