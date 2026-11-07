//! Native Finance trial balance and statements. Base amounts are the historical
//! ledger authority; mutable report state is never used.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ReportDates {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct BalanceSheet {
    pub assets: String,
    pub liabilities: String,
    pub equity: String,
    pub recorded_equity: String,
    pub unclosed_profit: String,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ProfitAndLoss {
    pub revenue: String,
    pub expenses: String,
    pub other_income: String,
    pub other_expenses: String,
    pub net_profit: String,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FinancialReport {
    pub dates: ReportDates,
    pub balance_sheet: BalanceSheet,
    pub profit_and_loss: ProfitAndLoss,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub trial_balance: Vec<Value>,
}

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::invalid(code, message).into()
}

fn not_found(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::NotFound, code, message).into()
}

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_REPORT_UNAVAILABLE",
        "The financial report is unavailable.",
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

pub async fn lock_finance_report_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await
}

fn valid_date(value: &str) -> bool {
    if value.len() != 10
        || value.as_bytes()[4] != b'-'
        || value.as_bytes()[7] != b'-'
        || !value
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
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

fn minor_decimal(value: i128) -> String {
    let negative = value < 0;
    let absolute = value.abs();
    format!(
        "{}{}.{:02}",
        if negative { "-" } else { "" },
        absolute / 100,
        absolute % 100
    )
}

fn parse(value: String) -> Result<Value, Error> {
    serde_json::from_str(&value).map_err(|_| unavailable())
}

async fn trial_query(c: &mut PgConnection, company: &str, chart: Option<&str>, start: Option<&str>, end: Option<&str>, exclude_close: bool) -> Result<Vec<Value>, Error> {
    let sql = format!(
        "SELECT (to_jsonb(a) || jsonb_build_object(\
         'debit',COALESCE(SUM(l.base_debit_minor),0)::text,\
         'credit',COALESCE(SUM(l.base_credit_minor),0)::text,\
         'balance',(COALESCE(SUM(l.base_debit_minor),0)-COALESCE(SUM(l.base_credit_minor),0))::text))::text \
         FROM finance_journal_lines l \
         JOIN finance_journals j ON j.id=l.journal_id \
         JOIN finance_accounts a ON a.id=l.account_id \
         WHERE j.company_id=$1 AND j.status IN ('posted','reversed') \
           AND ($2::text IS NULL OR j.chart_id=$2) \
           AND ($3::text IS NULL OR j.entry_date>=$3) \
           AND ($4::text IS NULL OR j.entry_date<=$4) \
           {}\
         GROUP BY a.id,a.code,a.name,a.category ORDER BY a.code",
        if exclude_close {
            "AND NOT (j.source_type='fiscal_close' OR (j.source_type='book_projection' AND EXISTS (SELECT 1 FROM finance_journals fiscal_origin WHERE fiscal_origin.id=j.source_id AND fiscal_origin.company_id=j.company_id AND fiscal_origin.source_type='fiscal_close'))) "
        } else {
            ""
        }
    );
    sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
        .bind(company)
        .bind(chart)
        .bind(start)
        .bind(end)
        .fetch_all(&mut *c)
        .await?
        .into_iter()
        .map(parse)
        .collect()
}

fn category(rows: &[Value], name: &str) -> Result<i128, Error> {
    let mut total = 0i128;
    for row in rows {
        if row.get("category").and_then(Value::as_str) == Some(name) {
            let balance = row
                .get("balance")
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<i128>().ok())
                .ok_or_else(unavailable)?;
            total = total.checked_add(balance).ok_or_else(unavailable)?;
        }
    }
    Ok(total)
}

#[allow(clippy::too_many_arguments)]
pub async fn read_financial_report(
    c: &mut PgConnection,
    company: &str,
    chart_id: Option<&str>,
    period_id: Option<&str>,
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> Result<FinancialReport, Error> {
    serializable(c).await?;
    if company.is_empty() || company.len() > 200 {
        return Err(unavailable());
    }
    if chart_id.is_some_and(|value| value.is_empty() || value.len() > 200)
        || period_id.is_some_and(|value| value.is_empty() || value.len() > 200)
        || start_date.is_some_and(|value| !valid_date(value))
        || end_date.is_some_and(|value| !valid_date(value))
    {
        return Err(invalid(
            "FINANCE_REPORT_QUERY_INVALID",
            "Use a valid ledger, period and date range.",
        ));
    }
    if let Some(chart_id) = chart_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM finance_charts WHERE id=$1 AND company_id=$2)",
        )
        .bind(chart_id)
        .bind(company)
        .fetch_one(&mut *c)
        .await?;
        if !exists {
            return Err(not_found("CHART_NOT_FOUND", "Ledger not found in this Company."));
        }
    }
    let (start_date, end_date) = if let Some(period_id) = period_id {
        let row = sqlx::query(
            "SELECT start_date,end_date FROM finance_periods WHERE id=$1 AND company_id=$2",
        )
        .bind(period_id)
        .bind(company)
        .fetch_optional(&mut *c)
        .await?
        .ok_or_else(|| not_found("PERIOD_NOT_FOUND", "Report period not found in this Company."))?;
        (Some(row.try_get::<String, _>("start_date")?), Some(row.try_get::<String, _>("end_date")?))
    } else {
        (start_date.map(str::to_owned), end_date.map(str::to_owned))
    };
    if start_date.as_deref().zip(end_date.as_deref()).is_some_and(|(start, end)| start > end) {
        return Err(invalid(
            "FINANCE_REPORT_QUERY_INVALID",
            "The report start date must not follow its end date.",
        ));
    }
    let trial_balance = trial_query(
        c,
        company,
        chart_id,
        start_date.as_deref(),
        end_date.as_deref(),
        false,
    )
    .await?;
    let balance = trial_query(c, company, chart_id, None, end_date.as_deref(), false).await?;
    let income = trial_query(
        c,
        company,
        chart_id,
        start_date.as_deref(),
        end_date.as_deref(),
        true,
    )
    .await?;
    let recorded_equity = -category(&balance, "equity")?;
    let unclosed_profit = -category(&balance, "revenue")?
        - category(&balance, "other_income")?
        - category(&balance, "expense")?
        - category(&balance, "other_expense")?;
    let net_profit = -category(&income, "revenue")?
        - category(&income, "expense")?
        - category(&income, "other_income")?
        - category(&income, "other_expense")?;
    Ok(FinancialReport {
        dates: ReportDates {
            start_date,
            end_date,
        },
        balance_sheet: BalanceSheet {
            assets: minor_decimal(category(&balance, "asset")?),
            liabilities: minor_decimal(-category(&balance, "liability")?),
            equity: minor_decimal(recorded_equity + unclosed_profit),
            recorded_equity: minor_decimal(recorded_equity),
            unclosed_profit: minor_decimal(unclosed_profit),
        },
        profit_and_loss: ProfitAndLoss {
            revenue: minor_decimal(-category(&income, "revenue")?),
            expenses: minor_decimal(category(&income, "expense")?),
            other_income: minor_decimal(-category(&income, "other_income")?),
            other_expenses: minor_decimal(category(&income, "other_expense")?),
            net_profit: minor_decimal(net_profit),
        },
        trial_balance,
    })
}
