//! Native Finance sales-defaults setup. The approved edition is the sole source
//! for customer-document accounts; prepare and independent approval stay in the
//! caller's transaction with the original command receipt.
use super::sales_invoice_create::{
    SalesInvoiceCommandActor, authority_json, fingerprint, receipt,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SalesDefaultsWorkspace {
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub editions: Vec<Value>,
    pub options: SalesDefaultsOptions,
    pub usable_defaults: Vec<String>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub issues: Vec<Value>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SalesDefaultsOptions {
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub charts: Vec<Value>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub accounts: Vec<Value>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub parties: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SalesDefaultsPrepareInput {
    pub chart_id: String,
    pub currency: String,
    pub receivable_account_id: String,
    pub revenue_account_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tax_account_id: Option<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SalesDefaultsDecisionInput {
    pub expected_version: i64,
    pub action: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SalesDefaultsResult {
    pub id: String,
    pub status: String,
    pub version: i64,
}

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::invalid(code, message).into()
}

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_SALES_DEFAULTS_UNAVAILABLE",
        "The sales defaults are unavailable.",
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

pub async fn lock_sales_defaults_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query("LOCK TABLE finance_sales_defaults IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *c)
        .await?;
    sqlx::query("LOCK TABLE finance_charts,finance_accounts,finance_parties IN SHARE MODE")
        .execute(&mut *c)
        .await?;
    Ok(())
}

fn parse(value: String) -> Result<Value, Error> {
    serde_json::from_str(&value).map_err(|_| unavailable())
}

fn valid_reason(value: &str) -> bool {
    let trimmed = value.trim();
    (10..=2000).contains(&trimmed.len())
}

fn validate_prepare(input: &SalesDefaultsPrepareInput) -> Result<(), Error> {
    if input.chart_id.is_empty()
        || input.currency.len() != 3
        || !input.currency.bytes().all(|byte| byte.is_ascii_uppercase())
        || input.receivable_account_id.is_empty()
        || input.revenue_account_id.is_empty()
        || input
            .tax_account_id
            .as_deref()
            .is_some_and(|value| value.is_empty())
        || !valid_reason(&input.reason)
    {
        return Err(invalid(
            "SALES_DEFAULTS_INVALID",
            "Choose a ledger, currency and the three active sales accounts with a reason.",
        ));
    }
    if input.receivable_account_id == input.revenue_account_id
        || input
            .tax_account_id
            .as_deref()
            .is_some_and(|value| {
                value == input.receivable_account_id || value == input.revenue_account_id
            })
    {
        return Err(invalid(
            "SALES_ACCOUNTS_INVALID",
            "Choose distinct receivable, revenue and VAT accounts.",
        ));
    }
    Ok(())
}

async fn validate_accounts(
    c: &mut PgConnection,
    company: &str,
    input: &SalesDefaultsPrepareInput,
) -> Result<(), Error> {
    let chart: Option<String> = sqlx::query_scalar(
        "SELECT status FROM finance_charts WHERE id=$1 AND company_id=$2 FOR SHARE",
    )
    .bind(&input.chart_id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?;
    if chart.as_deref() != Some("active") {
        return Err(invalid(
            "SALES_ACCOUNTS_INVALID",
            "Use active sales accounts on one Company ledger.",
        ));
    }
    let ids = [
        Some(input.receivable_account_id.as_str()),
        Some(input.revenue_account_id.as_str()),
        input.tax_account_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let rows = sqlx::query(
        "SELECT id,category,control_kind,status,chart_id,currency FROM finance_accounts \
         WHERE company_id=$1 AND chart_id=$2 AND currency=$3 AND id = ANY($4) FOR SHARE",
    )
    .bind(company)
    .bind(&input.chart_id)
    .bind(&input.currency)
    .bind(&ids)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() != ids.len() {
        return Err(invalid(
            "SALES_ACCOUNTS_INVALID",
            "Use active sales accounts on one Company ledger.",
        ));
    }
    let find = |id: &str| rows.iter().find(|row| row.get::<String, _>("id") == id);
    let receivable = find(&input.receivable_account_id).ok_or_else(|| {
        invalid(
            "SALES_ACCOUNTS_INVALID",
            "Use active sales accounts on one Company ledger.",
        )
    })?;
    let revenue = find(&input.revenue_account_id).ok_or_else(|| {
        invalid(
            "SALES_ACCOUNTS_INVALID",
            "Use active sales accounts on one Company ledger.",
        )
    })?;
    let tax = input
        .tax_account_id
        .as_deref()
        .and_then(find);
    if receivable.get::<String, _>("status") != "active"
        || revenue.get::<String, _>("status") != "active"
        || tax.is_some_and(|row| row.get::<String, _>("status") != "active")
        || receivable.get::<String, _>("control_kind") != "receivable"
        || revenue.get::<String, _>("category") != "revenue"
        || tax.is_some_and(|row| {
            row.get::<String, _>("category") != "liability"
                || row.get::<String, _>("control_kind") != "none"
        })
    {
        return Err(invalid(
            "SALES_ACCOUNTS_INVALID",
            "Use active sales accounts on one Company ledger.",
        ));
    }
    Ok(())
}

pub async fn read_sales_defaults(
    c: &mut PgConnection,
    company: &str,
) -> Result<SalesDefaultsWorkspace, Error> {
    serializable(c).await?;
    if company.is_empty() || company.len() > 200 {
        return Err(unavailable());
    }
    let charts: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id',id,'name',name,'base_currency',base_currency,'status',status)::text \
         FROM finance_charts WHERE company_id=$1 ORDER BY id",
    )
    .bind(company)
    .fetch_all(&mut *c)
    .await?
    .into_iter()
    .map(parse)
    .collect::<Result<_, _>>()?;
    let accounts: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id',id,'chart_id',chart_id,'code',code,'name',name,'category',category,\
         'currency',currency,'status',status,'control_kind',control_kind)::text \
         FROM finance_accounts WHERE company_id=$1 ORDER BY code,id",
    )
    .bind(company)
    .fetch_all(&mut *c)
    .await?
    .into_iter()
    .map(parse)
    .collect::<Result<_, _>>()?;
    let parties: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id',id,'name',name,'kind',kind,'status',status)::text \
         FROM finance_parties WHERE company_id=$1 AND kind='customer' AND person_id IS NULL ORDER BY name,id",
    )
    .bind(company)
    .fetch_all(&mut *c)
    .await?
    .into_iter()
    .map(parse)
    .collect::<Result<_, _>>()?;
    let editions: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id',id,'company_id',company_id,'chart_id',chart_id,'currency',currency,\
         'receivable_account_id',receivable_account_id,'revenue_account_id',revenue_account_id,\
         'tax_account_id',tax_account_id,'status',status,'version',version,'reason',reason,\
         'created_by',created_by,'reviewed_by',reviewed_by,'created_at',created_at,'updated_at',updated_at,\
         'review_reason',review_reason)::text \
         FROM finance_sales_defaults WHERE company_id=$1 ORDER BY id",
    )
    .bind(company)
    .fetch_all(&mut *c)
    .await?
    .into_iter()
    .map(parse)
    .collect::<Result<_, _>>()?;
    let usable_defaults: Vec<String> = sqlx::query_scalar(
        "SELECT d.id FROM finance_sales_defaults d \
         JOIN finance_charts c ON c.id=d.chart_id AND c.company_id=d.company_id AND c.status='active' \
         JOIN finance_accounts ar ON ar.id=d.receivable_account_id AND ar.company_id=d.company_id \
           AND ar.chart_id=d.chart_id AND ar.currency=d.currency AND ar.status='active' AND ar.control_kind='receivable' \
         JOIN finance_accounts rev ON rev.id=d.revenue_account_id AND rev.company_id=d.company_id \
           AND rev.chart_id=d.chart_id AND rev.currency=d.currency AND rev.status='active' AND rev.category='revenue' \
         LEFT JOIN finance_accounts tax ON tax.id=d.tax_account_id AND tax.company_id=d.company_id \
           AND tax.chart_id=d.chart_id AND tax.currency=d.currency \
         WHERE d.company_id=$1 AND d.status='approved' \
           AND (d.tax_account_id IS NULL OR (tax.status='active' AND tax.category='liability' AND tax.control_kind='none')) \
         ORDER BY d.id",
    )
    .bind(company)
    .fetch_all(&mut *c)
    .await?;
    let issues = editions
        .iter()
        .filter(|row| {
            row.get("status").and_then(Value::as_str) == Some("approved")
                && row
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| !usable_defaults.iter().any(|value| value == id))
        })
        .map(|row| {
            json!({
                "currency": row.get("currency").cloned().unwrap_or(Value::Null),
                "message": "Restore active receivable, revenue and VAT accounts on the approved ledger, or prepare new sales defaults.",
            })
        })
        .collect();
    Ok(SalesDefaultsWorkspace {
        editions,
        options: SalesDefaultsOptions {
            charts,
            accounts,
            parties,
        },
        usable_defaults,
        issues,
    })
}

async fn replay(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    operation: &str,
    record: &str,
    input_json: &str,
) -> Result<Option<SalesDefaultsResult>, Error> {
    let expected = fingerprint(actor, operation, record, input_json);
    let existing = sqlx::query(
        "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
    )
    .bind(command.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = existing else {
        return Ok(None);
    };
    let saved: String = row.try_get("request_hash")?;
    if saved != expected {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "IDEMPOTENCY_CONFLICT",
            "This command key belongs to a different request.",
        )
        .into());
    }
    let raw: Option<String> = row.try_get("result_json")?;
    Ok(Some(
        raw.and_then(|value| serde_json::from_str::<SalesDefaultsResult>(&value).ok())
            .ok_or_else(unavailable)?,
    ))
}

pub async fn prepare_sales_defaults(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    input: &SalesDefaultsPrepareInput,
    now: i64,
) -> Result<SalesDefaultsResult, Error> {
    serializable(c).await?;
    validate_prepare(input)?;
    if actor.company_id.is_empty()
        || actor.user_id.is_empty()
        || actor.assignment_id.is_empty()
        || actor.branch_id.is_some()
        || actor.department_id.is_some()
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "COMPANY_SCOPE_REQUIRED",
            "Use a Company Finance assignment for sales configuration.",
        )
        .into());
    }
    let id = format!("sales-defaults-{}", command.as_str());
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    if let Some(result) = replay(
        c,
        actor,
        command,
        "finance.sales-defaults.prepare",
        &id,
        &input_json,
    )
    .await?
    {
        return Ok(result);
    }
    validate_accounts(c, &actor.company_id, input).await?;
    let result = SalesDefaultsResult {
        id: id.clone(),
        status: "draft".into(),
        version: 1,
    };
    sqlx::query(
        "INSERT INTO finance_sales_defaults(id,company_id,chart_id,currency,receivable_account_id,\
         revenue_account_id,tax_account_id,reason,created_by,status,version,created_at,updated_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'draft',1,$10,$10)",
    )
    .bind(&id)
    .bind(&actor.company_id)
    .bind(&input.chart_id)
    .bind(&input.currency)
    .bind(&input.receivable_account_id)
    .bind(&input.revenue_account_id)
    .bind(input.tax_account_id.as_deref())
    .bind(input.reason.trim())
    .bind(&actor.user_id)
    .bind(now)
    .execute(&mut *c)
    .await?;
    receipt(
        c,
        actor,
        &authority_json(actor),
        command.as_str(),
        "finance.sales-defaults.prepare",
        &fingerprint(
            actor,
            "finance.sales-defaults.prepare",
            &id,
            &input_json,
        ),
        &id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        1,
        now,
    )
    .await?;
    Ok(result)
}

pub async fn decide_sales_defaults(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    id: &str,
    input: &SalesDefaultsDecisionInput,
    now: i64,
) -> Result<SalesDefaultsResult, Error> {
    serializable(c).await?;
    if !matches!(input.action.as_str(), "approve" | "reject")
        || input.expected_version < 1
        || !valid_reason(&input.reason)
        || id.is_empty()
        || id.len() > 200
    {
        return Err(invalid(
            "SALES_DEFAULTS_DECISION_INVALID",
            "Review the current sales defaults with a reason.",
        ));
    }
    if actor.company_id.is_empty()
        || actor.user_id.is_empty()
        || actor.assignment_id.is_empty()
        || actor.branch_id.is_some()
        || actor.department_id.is_some()
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "COMPANY_SCOPE_REQUIRED",
            "Use a Company Finance assignment for sales configuration.",
        )
        .into());
    }
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    if let Some(result) = replay(
        c,
        actor,
        command,
        "finance.sales-defaults.decide",
        id,
        &input_json,
    )
    .await?
    {
        return Ok(result);
    }
    let row = sqlx::query(
        "SELECT chart_id,currency,receivable_account_id,revenue_account_id,tax_account_id,\
         reason,created_by,status,version FROM finance_sales_defaults \
         WHERE id=$1 AND company_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "SALES_DEFAULTS_NOT_FOUND",
            "Sales defaults were not found.",
        )
    })?;
    let status: String = row.try_get("status")?;
    let version: i64 = row.try_get("version")?;
    let created_by: String = row.try_get("created_by")?;
    if status != "draft" || version != input.expected_version {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "VERSION_CONFLICT",
            "Refresh the sales defaults before review.",
        )
        .into());
    }
    if created_by == actor.user_id {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "INDEPENDENT_APPROVER_REQUIRED",
            "A different Finance approver must review these defaults.",
        )
        .into());
    }
    let chart_id: String = row.try_get("chart_id")?;
    let currency: String = row.try_get("currency")?;
    let prepare_input = SalesDefaultsPrepareInput {
        chart_id,
        currency: currency.clone(),
        receivable_account_id: row.try_get("receivable_account_id")?,
        revenue_account_id: row.try_get("revenue_account_id")?,
        tax_account_id: row.try_get("tax_account_id")?,
        reason: row.try_get("reason")?,
    };
    validate_accounts(c, &actor.company_id, &prepare_input).await?;
    let next_status = if input.action == "approve" {
        "approved"
    } else {
        "rejected"
    };
    let next_version = input.expected_version + 1;
    if input.action == "approve" {
        sqlx::query(
            "UPDATE finance_sales_defaults SET status='superseded',version=version+1,updated_at=$1 \
             WHERE company_id=$2 AND currency=$3 AND status='approved' AND id<>$4",
        )
        .bind(now)
        .bind(&actor.company_id)
        .bind(&currency)
        .bind(id)
        .execute(&mut *c)
        .await?;
    }
    sqlx::query(
        "UPDATE finance_sales_defaults SET status=$1,reviewed_by=$2,review_reason=$3,\
         version=$4,updated_at=$5 WHERE id=$6 AND company_id=$7",
    )
    .bind(next_status)
    .bind(&actor.user_id)
    .bind(input.reason.trim())
    .bind(next_version)
    .bind(now)
    .bind(id)
    .bind(&actor.company_id)
    .execute(&mut *c)
    .await?;
    let result = SalesDefaultsResult {
        id: id.to_owned(),
        status: next_status.into(),
        version: next_version,
    };
    receipt(
        c,
        actor,
        &authority_json(actor),
        command.as_str(),
        "finance.sales-defaults.decide",
        &fingerprint(
            actor,
            "finance.sales-defaults.decide",
            id,
            &input_json,
        ),
        id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        next_version,
        now,
    )
    .await?;
    Ok(result)
}
