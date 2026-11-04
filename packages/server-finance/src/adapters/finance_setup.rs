//! Native Finance chart-of-accounts management: chart/account/period reads and
//! the chart, account, import, status and period writers.
use super::sales_invoice_create::{SalesInvoiceCommandActor, authority_json, fingerprint, receipt};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceChartAccounts {
    pub chart: Value,
    pub accounts: Vec<Value>,
    pub periods: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateChartInput {
    pub name: String,
    pub base_currency: String,
    pub accounting_basis: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateAccountInput {
    pub chart_id: String,
    pub code: String,
    pub name: String,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(default)]
    pub is_control: bool,
    #[serde(default = "default_control_kind")]
    pub control_kind: String,
}

fn default_control_kind() -> String {
    "none".into()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountImportRow {
    pub code: String,
    pub name: String,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_kind: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportAccountsInput {
    pub chart_id: String,
    pub rows: Vec<AccountImportRow>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeAccountStatusInput {
    pub status: String,
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreatePeriodInput {
    pub name: String,
    pub start_date: String,
    pub end_date: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartResult {
    pub id: String,
    pub name: String,
    pub base_currency: String,
    pub accounting_basis: String,
    pub company_id: String,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountResult {
    pub id: String,
    pub chart_id: String,
    pub code: String,
    pub name: String,
    pub category: String,
    pub currency: String,
    pub is_control: bool,
    pub control_kind: String,
    pub company_id: String,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountImportResult {
    pub chart_id: String,
    pub count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatusResult {
    pub id: String,
    pub chart_id: String,
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodResult {
    pub id: String,
    pub name: String,
    pub start_date: String,
    pub end_date: String,
    pub state: String,
    pub company_id: String,
    pub version: i64,
}

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::invalid(code, message).into()
}

fn conflict(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
}

fn not_found(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::NotFound, code, message).into()
}

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_SETUP_UNAVAILABLE",
        "Finance setup is unavailable.",
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

pub async fn lock_finance_setup_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query("LOCK TABLE finance_charts,finance_accounts,finance_periods IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *c)
        .await?;
    Ok(())
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

fn valid_category(value: &str) -> bool {
    matches!(
        value,
        "asset"
            | "liability"
            | "equity"
            | "revenue"
            | "expense"
            | "other_income"
            | "other_expense"
    )
}

fn valid_control(value: &str) -> bool {
    matches!(value, "none" | "receivable" | "payable" | "bank")
}

fn parse(value: String) -> Result<Value, Error> {
    serde_json::from_str(&value).map_err(|_| unavailable())
}

pub async fn read_chart_accounts(
    c: &mut PgConnection,
    company: &str,
    chart_id: &str,
) -> Result<FinanceChartAccounts, Error> {
    serializable(c).await?;
    if company.is_empty() || company.len() > 200 || chart_id.is_empty() || chart_id.len() > 200 {
        return Err(unavailable());
    }
    let chart = sqlx::query(
        "SELECT to_jsonb(c)::text FROM finance_charts c WHERE c.id=$1 AND c.company_id=$2",
    )
    .bind(chart_id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| not_found("CHART_NOT_FOUND", "Ledger not found in this Company."))?;
    let chart: String = chart.try_get(0)?;
    let accounts = sqlx::query_scalar::<_, String>(
        "SELECT to_jsonb(a)::text FROM finance_accounts a \
         WHERE a.company_id=$1 AND a.chart_id=$2 ORDER BY a.code,a.id",
    )
    .bind(company)
    .bind(chart_id)
    .fetch_all(&mut *c)
    .await?
    .into_iter()
    .map(parse)
    .collect::<Result<Vec<_>, _>>()?;
    let periods = sqlx::query_scalar::<_, String>(
        "SELECT to_jsonb(p)::text FROM finance_periods p \
         WHERE p.company_id=$1 ORDER BY p.start_date,p.id",
    )
    .bind(company)
    .fetch_all(&mut *c)
    .await?
    .into_iter()
    .map(parse)
    .collect::<Result<Vec<_>, _>>()?;
    Ok(FinanceChartAccounts {
        chart: parse(chart)?,
        accounts,
        periods,
    })
}

fn require_company(actor: &SalesInvoiceCommandActor) -> Result<(), Error> {
    if actor.company_id.is_empty()
        || actor.user_id.is_empty()
        || actor.assignment_id.is_empty()
        || actor.branch_id.is_some()
        || actor.department_id.is_some()
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "COMPANY_SCOPE_REQUIRED",
            "Use a Company Finance assignment for Finance setup.",
        )
        .into());
    }
    Ok(())
}

pub async fn create_chart(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    input: &CreateChartInput,
    now: i64,
) -> Result<ChartResult, Error> {
    serializable(c).await?;
    require_company(actor)?;
    if input.name.trim().is_empty()
        || input.name.trim().len() > 160
        || input.base_currency.len() != 3
        || !input.base_currency.bytes().all(|byte| byte.is_ascii_uppercase())
        || input.accounting_basis.trim().is_empty()
        || input.accounting_basis.trim().len() > 40
    {
        return Err(invalid(
            "INVALID_CHART",
            "Use a chart name, ISO currency and accounting basis.",
        ));
    }
    let id = format!("chart-{}", command.as_str());
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let operation = "finance.chart.create";
    let request_hash = fingerprint(actor, operation, &id, &input_json);
    let authority = authority_json(actor);
    if let Some(saved) = replay::<ChartResult>(c, command, operation, &request_hash).await? {
        return Ok(saved);
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_charts WHERE company_id=$1 AND name=$2)",
    )
    .bind(&actor.company_id)
    .bind(input.name.trim())
    .fetch_one(&mut *c)
    .await?;
    if exists {
        return Err(conflict(
            "CHART_EXISTS",
            "A chart with this name already exists in the Company.",
        ));
    }
    sqlx::query(
        "INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,created_by,created_at,updated_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$7)",
    )
    .bind(&id)
    .bind(&actor.company_id)
    .bind(input.name.trim())
    .bind(&input.base_currency)
    .bind(input.accounting_basis.trim())
    .bind(&actor.user_id)
    .bind(now)
    .execute(&mut *c)
    .await?;
    let result = ChartResult {
        id: id.clone(),
        name: input.name.trim().into(),
        base_currency: input.base_currency.clone(),
        accounting_basis: input.accounting_basis.trim().into(),
        company_id: actor.company_id.clone(),
        version: 1,
    };
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        operation,
        &request_hash,
        &id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        1,
        now,
    )
    .await?;
    Ok(result)
}

async fn chart(c: &mut PgConnection, company: &str, chart_id: &str) -> Result<(String, String), Error> {
    let row = sqlx::query(
        "SELECT id,base_currency FROM finance_charts WHERE id=$1 AND company_id=$2 FOR SHARE",
    )
    .bind(chart_id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| not_found("CHART_NOT_FOUND", "Ledger not found in this Company."))?;
    Ok((row.try_get("id")?, row.try_get("base_currency")?))
}

pub async fn create_account(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    input: &CreateAccountInput,
    now: i64,
) -> Result<AccountResult, Error> {
    serializable(c).await?;
    require_company(actor)?;
    if input.code.trim().is_empty()
        || input.code.trim().len() > 40
        || input.name.trim().is_empty()
        || input.name.trim().len() > 160
        || !valid_category(&input.category)
        || !valid_control(&input.control_kind)
        || input
            .currency
            .as_deref()
            .is_some_and(|value| value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase()))
    {
        return Err(invalid(
            "INVALID_ACCOUNT",
            "Use an account code, name, category and control kind.",
        ));
    }
    let (chart_id, base_currency) = chart(c, &actor.company_id, &input.chart_id).await?;
    let id = format!("acct-{}", command.as_str());
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let operation = "finance.account.create";
    let request_hash = fingerprint(actor, operation, &id, &input_json);
    let authority = authority_json(actor);
    if let Some(saved) = replay::<AccountResult>(c, command, operation, &request_hash).await? {
        return Ok(saved);
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_accounts WHERE chart_id=$1 AND code=$2)",
    )
    .bind(&chart_id)
    .bind(input.code.trim())
    .fetch_one(&mut *c)
    .await?;
    if exists {
        return Err(conflict(
            "ACCOUNT_CODE_EXISTS",
            "This account code already exists in the chart.",
        ));
    }
    let currency = input
        .currency
        .clone()
        .unwrap_or_else(|| base_currency.clone());
    let is_control = input.is_control || input.control_kind != "none";
    sqlx::query(
        "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,parent_id,currency,\
         is_control,control_kind,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$11)",
    )
    .bind(&id)
    .bind(&chart_id)
    .bind(&actor.company_id)
    .bind(input.code.trim())
    .bind(input.name.trim())
    .bind(&input.category)
    .bind(input.parent_id.as_deref())
    .bind(&currency)
    .bind(i64::from(is_control))
    .bind(&input.control_kind)
    .bind(now)
    .execute(&mut *c)
    .await?;
    let result = AccountResult {
        id: id.clone(),
        chart_id: chart_id.clone(),
        code: input.code.trim().into(),
        name: input.name.trim().into(),
        category: input.category.clone(),
        currency,
        is_control,
        control_kind: input.control_kind.clone(),
        company_id: actor.company_id.clone(),
        version: 1,
    };
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        operation,
        &request_hash,
        &id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        1,
        now,
    )
    .await?;
    Ok(result)
}

pub async fn import_accounts(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    input: &ImportAccountsInput,
    now: i64,
) -> Result<AccountImportResult, Error> {
    serializable(c).await?;
    require_company(actor)?;
    if input.rows.is_empty()
        || input.rows.len() > 500
        || input.rows.iter().any(|row| {
            row.code.trim().is_empty()
                || row.code.trim().len() > 40
                || row.name.trim().is_empty()
                || row.name.trim().len() > 160
                || !valid_category(&row.category)
                || row.control_kind.as_deref().is_some_and(|value| !valid_control(value))
                || row.currency.as_deref().is_some_and(|value| {
                    value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase())
                })
        })
    {
        return Err(invalid(
            "INVALID_ACCOUNT_IMPORT",
            "Use between 1 and 500 valid account rows.",
        ));
    }
    let (chart_id, base_currency) = chart(c, &actor.company_id, &input.chart_id).await?;
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let operation = "finance.account.import";
    let request_hash = fingerprint(actor, operation, &chart_id, &input_json);
    let authority = authority_json(actor);
    if let Some(saved) = replay::<AccountImportResult>(c, command, operation, &request_hash).await? {
        return Ok(saved);
    }
    let mut codes = std::collections::BTreeSet::new();
    for row in &input.rows {
        let code = row.code.trim().to_owned();
        if !codes.insert(code.clone()) {
            return Err(conflict(
                "ACCOUNT_CODE_EXISTS",
                "Account codes must be unique in the import.",
            ));
        }
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM finance_accounts WHERE chart_id=$1 AND code=$2)",
        )
        .bind(&chart_id)
        .bind(&code)
        .fetch_one(&mut *c)
        .await?;
        if exists {
            return Err(conflict(
                "ACCOUNT_CODE_EXISTS",
                "An imported account code already exists in the chart.",
            ));
        }
    }
    for (index, row) in input.rows.iter().enumerate() {
        let id = format!("acct-{}-{index}", command.as_str());
        let currency = row.currency.clone().unwrap_or_else(|| base_currency.clone());
        let control_kind = row.control_kind.clone().unwrap_or_else(|| "none".into());
        let is_control = control_kind != "none";
        sqlx::query(
            "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,\
             is_control,control_kind,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10)",
        )
        .bind(&id)
        .bind(&chart_id)
        .bind(&actor.company_id)
        .bind(row.code.trim())
        .bind(row.name.trim())
        .bind(&row.category)
        .bind(&currency)
        .bind(i64::from(is_control))
        .bind(&control_kind)
        .bind(now)
        .execute(&mut *c)
        .await?;
    }
    let result = AccountImportResult {
        chart_id: chart_id.clone(),
        count: input.rows.len(),
    };
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        operation,
        &request_hash,
        &chart_id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        1,
        now,
    )
    .await?;
    Ok(result)
}

pub async fn change_account_status(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    chart_id: &str,
    account_id: &str,
    input: &ChangeAccountStatusInput,
    now: i64,
) -> Result<AccountStatusResult, Error> {
    serializable(c).await?;
    require_company(actor)?;
    if !matches!(input.status.as_str(), "active" | "inactive")
        || input.expected_version < 1
        || input.reason.trim().len() < 3
        || input.reason.trim().len() > 2000
    {
        return Err(invalid(
            "INVALID_ACCOUNT_STATUS",
            "Use an active or inactive status with the current version and a reason.",
        ));
    }
    let row = sqlx::query(
        "SELECT status,version FROM finance_accounts \
         WHERE id=$1 AND chart_id=$2 AND company_id=$3 FOR UPDATE",
    )
    .bind(account_id)
    .bind(chart_id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| not_found("ACCOUNT_NOT_FOUND", "The account is not available in this chart."))?;
    let current: String = row.try_get("status")?;
    let version: i64 = row.try_get("version")?;
    let operation = if input.status == "active" {
        "finance.account.active"
    } else {
        "finance.account.inactive"
    };
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let request_hash = fingerprint(actor, operation, account_id, &input_json);
    let authority = authority_json(actor);
    if let Some(saved) = replay::<AccountStatusResult>(c, command, operation, &request_hash).await? {
        return Ok(saved);
    }
    if version != input.expected_version {
        return Err(conflict(
            "VERSION_CONFLICT",
            "The account changed. Refresh before changing its status.",
        ));
    }
    if current == input.status {
        return Err(conflict(
            "ACCOUNT_STATUS_UNCHANGED",
            "The account is already in that status.",
        ));
    }
    if input.status == "inactive" {
        let draft: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM finance_journal_lines l JOIN finance_journals j \
             ON j.id=l.journal_id WHERE l.account_id=$1 AND j.company_id=$2 AND j.status='draft')",
        )
        .bind(account_id)
        .bind(&actor.company_id)
        .fetch_one(&mut *c)
        .await?;
        if draft {
            return Err(conflict(
                "ACCOUNT_HAS_DRAFT_JOURNAL",
                "Post or correct draft journals before making this account inactive.",
            ));
        }
        let open: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM finance_invoices WHERE company_id=$1 \
             AND outstanding_minor>0 AND status IN ('draft','posted','partially_paid') \
             AND (debit_account_id=$2 OR credit_account_id=$2))",
        )
        .bind(&actor.company_id)
        .bind(account_id)
        .fetch_one(&mut *c)
        .await?;
        if open {
            return Err(conflict(
                "ACCOUNT_HAS_OPEN_INVOICES",
                "Resolve open invoices before making this account inactive.",
            ));
        }
    }
    let next = input.expected_version + 1;
    let updated = sqlx::query(
        "UPDATE finance_accounts SET status=$1,version=$2,updated_at=$3 \
         WHERE id=$4 AND chart_id=$5 AND company_id=$6 AND version=$7",
    )
    .bind(&input.status)
    .bind(next)
    .bind(now)
    .bind(account_id)
    .bind(chart_id)
    .bind(&actor.company_id)
    .bind(input.expected_version)
    .execute(&mut *c)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(conflict(
            "VERSION_CONFLICT",
            "The account changed. Refresh before changing its status.",
        ));
    }
    let result = AccountStatusResult {
        id: account_id.to_owned(),
        chart_id: chart_id.to_owned(),
        status: input.status.clone(),
        version: next,
    };
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        operation,
        &request_hash,
        account_id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        next,
        now,
    )
    .await?;
    Ok(result)
}

pub async fn create_period(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    input: &CreatePeriodInput,
    now: i64,
) -> Result<PeriodResult, Error> {
    serializable(c).await?;
    require_company(actor)?;
    if input.name.trim().is_empty()
        || input.name.trim().len() > 100
        || !valid_date(&input.start_date)
        || !valid_date(&input.end_date)
        || input.end_date < input.start_date
    {
        return Err(invalid(
            "INVALID_PERIOD",
            "Use a period name and a valid start/end date range.",
        ));
    }
    let id = format!("period-{}", command.as_str());
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let operation = "finance.period.create";
    let request_hash = fingerprint(actor, operation, &id, &input_json);
    let authority = authority_json(actor);
    if let Some(saved) = replay::<PeriodResult>(c, command, operation, &request_hash).await? {
        return Ok(saved);
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_periods WHERE company_id=$1 \
         AND start_date=$2 AND end_date=$3)",
    )
    .bind(&actor.company_id)
    .bind(&input.start_date)
    .bind(&input.end_date)
    .fetch_one(&mut *c)
    .await?;
    if exists {
        return Err(conflict(
            "PERIOD_EXISTS",
            "A period with this date range already exists.",
        ));
    }
    sqlx::query(
        "INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) \
         VALUES($1,$2,$3,$4,$5,$6)",
    )
    .bind(&id)
    .bind(&actor.company_id)
    .bind(input.name.trim())
    .bind(&input.start_date)
    .bind(&input.end_date)
    .bind(now)
    .execute(&mut *c)
    .await?;
    let result = PeriodResult {
        id: id.clone(),
        name: input.name.trim().into(),
        start_date: input.start_date.clone(),
        end_date: input.end_date.clone(),
        state: "open".into(),
        company_id: actor.company_id.clone(),
        version: 0,
    };
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        operation,
        &request_hash,
        &id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        0,
        now,
    )
    .await?;
    Ok(result)
}

async fn replay<T: serde::de::DeserializeOwned>(
    c: &mut PgConnection,
    command: &CommandKey,
    operation: &str,
    request_hash: &str,
) -> Result<Option<T>, Error> {
    let saved = sqlx::query(
        "SELECT operation,request_hash,result_json FROM command_receipts \
         WHERE command_id=$1 FOR UPDATE",
    )
    .bind(command.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = saved else {
        return Ok(None);
    };
    let saved_operation: String = row.try_get("operation")?;
    let saved_hash: String = row.try_get("request_hash")?;
    if saved_operation != operation || saved_hash != request_hash {
        return Err(conflict(
            "IDEMPOTENCY_CONFLICT",
            "This command key belongs to a different request.",
        ));
    }
    let raw: Option<String> = row.try_get("result_json")?;
    raw.and_then(|value| serde_json::from_str::<T>(&value).ok())
        .map(Some)
        .ok_or_else(unavailable)
}
