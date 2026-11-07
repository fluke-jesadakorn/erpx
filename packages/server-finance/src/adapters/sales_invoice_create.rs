//! Native standalone sales-invoice create. Finance owns the invoice, line
//! revision, Money evidence and command receipt in the caller's transaction.
use crate::domain::money::js_trim;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const MAX_SAFE: i128 = 9_007_199_254_740_991;
const RATE_SCALE: i128 = 1_000_000_000_000_000_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SalesInvoiceCreateInput {
    pub party_id: String,
    pub invoice_date: String,
    pub due_date: String,
    pub currency: String,
    pub completion_confirmed: bool,
    pub completion_note: String,
    pub lines: Vec<SalesInvoiceCreateLine>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SalesInvoiceCreateLine {
    pub description: String,
    pub quantity: String,
    pub unit_price: String,
    pub tax_bps: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SalesInvoiceCompletionEvidence {
    pub note: String,
    pub confirmed_by: String,
    pub confirmed_at: i64,
    pub invoice_revision: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SalesInvoiceCreateResult {
    pub id: String,
    pub number: String,
    pub total: String,
    pub outstanding: String,
    pub status: String,
    pub version: i64,
    pub completion_evidence: SalesInvoiceCompletionEvidence,
}

/// Current command identity. The composition supplies this after the retained
/// Account/Holding proof; the adapter never constructs or widens authority.
#[derive(Clone, Debug)]
pub struct SalesInvoiceCommandActor {
    pub user_id: String,
    pub assignment_id: String,
    pub role: String,
    pub session_id: String,
    pub revision: i64,
    pub assignment_version: i64,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub scope_kind: String,
    pub scope_node_id: String,
    /// Exact camelCase IAM projection used by the Work command fingerprint.
    pub iam_json: String,
}

struct Line {
    description: String,
    quantity_millis: i64,
    unit_price_minor: i64,
    tax_bps: i64,
    subtotal_minor: i64,
    tax_minor: i64,
    total_minor: i64,
    tax_account_id: Option<String>,
}

struct Account {
    id: String,
    chart_id: String,
    code: String,
    name: String,
    category: String,
    currency: String,
    version: i64,
    status: String,
    control_kind: String,
}

struct Party {
    id: String,
    code: String,
    name: String,
    kind: String,
    currency: String,
    tax_registration: Option<String>,
    version: i64,
    status: String,
}

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::invalid(code, message).into()
}

fn conflict(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
}

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_SALES_INVOICE_UNAVAILABLE",
        "The sales invoice could not be prepared.",
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

pub async fn lock_sales_invoice_create_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE finance_sales_defaults,finance_charts,finance_accounts,\
         finance_parties,company_fx_rates,company_currency_scales IN SHARE MODE",
    )
    .execute(&mut *c)
    .await?;
    sqlx::query("LOCK TABLE finance_sales_number_sequences IN ROW EXCLUSIVE MODE")
        .execute(&mut *c)
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

fn string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

fn optional(value: Option<&str>) -> String {
    value
        .and_then(|value| serde_json::to_string(value).ok())
        .unwrap_or_else(|| "null".into())
}

pub(crate) fn fingerprint(actor: &SalesInvoiceCommandActor, operation: &str, record: &str, input: &str) -> String {
    let authority = format!(
        "{{\"sessionId\":{},\"revision\":{},\"assignmentVersion\":{}}}",
        string(&actor.session_id),
        actor.revision,
        actor.assignment_version
    );
    hash(&format!(
        "{{\"actor\":{},\"assignment\":{},\"role\":{},\"authority\":{},\"company\":{},\"branch\":{},\"department\":{},\"operation\":{},\"recordId\":{},\"input\":{}}}",
        string(&actor.user_id),
        string(&actor.assignment_id),
        string(&actor.role),
        authority,
        string(&actor.company_id),
        optional(actor.branch_id.as_deref()),
        optional(actor.department_id.as_deref()),
        string(operation),
        string(record),
        input
    ))
}

pub(crate) fn authority_json(actor: &SalesInvoiceCommandActor) -> String {
    let iam: Value = serde_json::from_str(&actor.iam_json).unwrap_or(Value::Null);
    json!({
        "role": actor.role,
        "assignmentId": actor.assignment_id,
        "iam": iam,
        "sessionId": actor.session_id,
        "revision": actor.revision,
        "assignmentVersion": actor.assignment_version,
        "companyId": actor.company_id,
        "branchId": actor.branch_id,
        "departmentId": actor.department_id,
        "scopeKind": actor.scope_kind,
        "scopeNodeId": actor.scope_node_id,
    })
    .to_string()
}

fn decimal_units(value: &str, scale: u32) -> Result<i128, Error> {
    if value.is_empty() || value.len() > 64 {
        return Err(invalid("INVALID_INVOICE_LINE", "Use a positive quantity and unit price."));
    }
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > scale as usize
    {
        return Err(invalid("INVALID_INVOICE_LINE", "Use a positive quantity and unit price."));
    }
    let whole = whole
        .parse::<i128>()
        .map_err(|_| invalid("INVALID_INVOICE_LINE", "Use a positive quantity and unit price."))?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<width$}", width = scale as usize)
            .parse::<i128>()
            .map_err(|_| invalid("INVALID_INVOICE_LINE", "Use a positive quantity and unit price."))?
    };
    whole
        .checked_mul(10i128.pow(scale))
        .and_then(|value| value.checked_add(fraction))
        .filter(|value| *value > 0 && *value <= MAX_SAFE)
        .ok_or_else(|| invalid("INVALID_INVOICE_LINE", "Use a positive quantity and unit price."))
}

fn decimal_units_unbounded(value: &str, scale: u32) -> Result<i128, Error> {
    if value.is_empty() || value.len() > 80 {
        return Err(invalid("MANUAL_RATE_REQUIRED", "A confirmed manual currency rate is required."));
    }
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > scale as usize
    {
        return Err(invalid("MANUAL_RATE_REQUIRED", "A confirmed manual currency rate is required."));
    }
    let whole = whole
        .parse::<i128>()
        .map_err(|_| invalid("MANUAL_RATE_REQUIRED", "A confirmed manual currency rate is required."))?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<width$}", width = scale as usize)
            .parse::<i128>()
            .map_err(|_| invalid("MANUAL_RATE_REQUIRED", "A confirmed manual currency rate is required."))?
    };
    whole
        .checked_mul(10i128.pow(scale))
        .and_then(|value| value.checked_add(fraction))
        .filter(|value| *value > 0)
        .ok_or_else(|| invalid("MANUAL_RATE_REQUIRED", "A confirmed manual currency rate is required."))
}

fn minor_decimal(value: i64) -> String {
    format!("{}.{:02}", value / 100, value % 100)
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

/// Zod `z.string().trim().min().max()` length: ECMAScript `trim`, then
/// UTF-16 code-unit length. Rust `str::trim` (Unicode `White_Space`, which
/// drops U+0085 but keeps U+FEFF) and UTF-8 byte length both diverge: a
/// 2,000-char Thai note is 6,000 bytes, a 1,000-emoji note is 2,000 UTF-16
/// units but 4,000 bytes. Free text (`completionNote`, line `description`)
/// must use this; ascii-gated `party_id` / `currency` / dates and decimal
/// money (digits/dot only) keep byte lengths below — for every value they
/// accept, byte length equals code-point length, and their caps are byte
/// defense-in-depth (`party_id` 200-byte ID cap, 64/80-byte decimal caps),
/// not Zod text lengths. Zod 4 counts code points, not UTF-16 units.
fn trimmed_cp_len(value: &str) -> usize {
    js_trim(value).chars().count()
}

fn validate(input: &SalesInvoiceCreateInput) -> Result<(), Error> {
    if input.party_id.is_empty()
        || input.party_id.len() > 200
        || !valid_date(&input.invoice_date)
        || !valid_date(&input.due_date)
        || input.due_date < input.invoice_date
        || input.currency.len() != 3
        || !input.currency.bytes().all(|byte| byte.is_ascii_uppercase())
        || !input.completion_confirmed
        || !(3..=2000).contains(&trimmed_cp_len(&input.completion_note))
        || input.lines.is_empty()
        || input.lines.len() > 100
    {
        return Err(invalid(
            "INVALID_SALES_INVOICE",
            "Complete the customer, dates, completion evidence and service lines.",
        ));
    }
    for line in &input.lines {
        if trimmed_cp_len(&line.description) == 0
            || trimmed_cp_len(&line.description) > 1000
            || !(0..=10_000).contains(&line.tax_bps)
        {
            return Err(invalid("INVALID_INVOICE_LINE", "Use a valid service line."));
        }
    }
    Ok(())
}

fn calculate_lines(
    input: &SalesInvoiceCreateInput,
    tax_account: Option<&str>,
) -> Result<Vec<Line>, Error> {
    let mut lines = Vec::with_capacity(input.lines.len());
    for line in &input.lines {
        let quantity_millis = decimal_units(&line.quantity, 3)?;
        let unit_price_minor = decimal_units(&line.unit_price, 2)?;
        let subtotal = (quantity_millis * unit_price_minor + 500) / 1000;
        let tax = (subtotal * line.tax_bps as i128 + 5_000) / 10_000;
        let total = subtotal + tax;
        if subtotal <= 0 || tax < 0 || total > MAX_SAFE {
            return Err(invalid(
                "INVALID_INVOICE_AMOUNT",
                "Provide a positive subtotal and nonnegative tax within supported precision.",
            ));
        }
        if tax > 0 && tax_account.is_none() {
            return Err(invalid(
                "TAX_ACCOUNT_REQUIRED",
                "Map a separate output tax account before recording tax.",
            ));
        }
        lines.push(Line {
            description: js_trim(&line.description).into(),
            quantity_millis: i64::try_from(quantity_millis).map_err(|_| unavailable())?,
            unit_price_minor: i64::try_from(unit_price_minor).map_err(|_| unavailable())?,
            tax_bps: line.tax_bps,
            subtotal_minor: i64::try_from(subtotal).map_err(|_| unavailable())?,
            tax_minor: i64::try_from(tax).map_err(|_| unavailable())?,
            total_minor: i64::try_from(total).map_err(|_| unavailable())?,
            tax_account_id: (tax > 0).then(|| tax_account.unwrap_or_default().to_owned()),
        });
    }
    Ok(lines)
}

async fn account(
    c: &mut PgConnection,
    company: &str,
    chart: &str,
    currency: &str,
    id: &str,
) -> Result<Account, Error> {
    let row = sqlx::query(
        "SELECT id,chart_id,code,name,category,currency,version,status,control_kind \
         FROM finance_accounts WHERE company_id=$1 AND chart_id=$2 AND currency=$3 AND id=$4 FOR SHARE",
    )
    .bind(company)
    .bind(chart)
    .bind(currency)
    .bind(id)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| invalid("SALES_ACCOUNTS_INVALID", "Use active sales accounts on one Company ledger."))?;
    let control_kind: Option<String> = row.try_get("control_kind")?;
    Ok(Account {
        id: row.try_get("id")?,
        chart_id: row.try_get("chart_id")?,
        code: row.try_get("code")?,
        name: row.try_get("name")?,
        category: row.try_get("category")?,
        currency: row.try_get("currency")?,
        version: row.try_get("version")?,
        status: row.try_get("status")?,
        control_kind: control_kind.unwrap_or_else(|| "none".into()),
    })
}

fn account_json(account: &Account) -> Value {
    json!({
        "id": account.id,
        "chart_id": account.chart_id,
        "code": account.code,
        "name": account.name,
        "category": account.category,
        "currency": account.currency,
        "version": account.version,
        "status": account.status,
    })
}

fn party_json(party: &Party) -> Value {
    json!({
        "id": party.id,
        "code": party.code,
        "name": party.name,
        "kind": party.kind,
        "currency": party.currency,
        "tax_registration": party.tax_registration,
        "version": party.version,
        "status": party.status,
    })
}

struct Fx {
    usd_units: i128,
    rate_id: Option<String>,
    used_latest_fallback: bool,
}

async fn fx(
    c: &mut PgConnection,
    company: &str,
    currency: &str,
    date: &str,
    amount_minor: i64,
) -> Result<Fx, Error> {
    if currency == "USD" {
        return Ok(Fx {
            usd_units: amount_minor as i128,
            rate_id: None,
            used_latest_fallback: false,
        });
    }
    let rows = sqlx::query(
        "SELECT id,effective_date::text effective_date,currency_per_usd::text rate,confirmed_at \
         FROM company_fx_rates WHERE company_id=$1 AND currency=$2 \
         ORDER BY effective_date DESC,confirmed_at DESC,id DESC",
    )
    .bind(company)
    .bind(currency)
    .fetch_all(&mut *c)
    .await?;
    let mut dated = None;
    let mut latest: Option<(String, String, i64)> = None;
    for row in rows {
        let id: String = row.try_get("id")?;
        let effective: String = row.try_get("effective_date")?;
        let rate: String = row.try_get("rate")?;
        let confirmed: i64 = row.try_get("confirmed_at")?;
        if effective.as_str() <= date && dated.is_none() {
            dated = Some((id.clone(), rate.clone(), confirmed));
        }
        if latest.as_ref().is_none_or(|(_, _, current)| confirmed > *current) {
            latest = Some((id, rate, confirmed));
        }
    }
    let (rate_id, rate, fallback) = match dated {
        Some((id, rate, _)) => (id, rate, false),
        None => latest
            .map(|(id, rate, _)| (id, rate, true))
            .ok_or_else(|| invalid("MANUAL_RATE_REQUIRED", "A confirmed manual currency rate is required."))?,
    };
    let source = decimal_units_unbounded(&rate, 18)?;
    if source <= 0 {
        return Err(invalid("MANUAL_RATE_REQUIRED", "A confirmed manual currency rate is required."));
    }
    let numerator = amount_minor as i128 * RATE_SCALE * 100;
    let denominator = source * 100;
    let usd = (numerator + denominator / 2) / denominator;
    Ok(Fx {
        usd_units: usd,
        rate_id: Some(rate_id),
        used_latest_fallback: fallback,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn receipt(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    authority: &str,
    command: &str,
    operation: &str,
    request_hash: &str,
    record: &str,
    result: &Value,
    version: i64,
    now: i64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(command)
    .bind(operation)
    .bind(&actor.user_id)
    .bind(request_hash)
    .bind(record)
    .bind(result.to_string())
    .bind(now)
    .bind(authority)
    .execute(&mut *c)
    .await?;
    let record_type = operation.split('.').take(2).collect::<Vec<_>>().join("_");
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$9,$4,$7,$8)",
    )
    .bind(command)
    .bind(&actor.user_id)
    .bind(&actor.assignment_id)
    .bind(operation)
    .bind(&record_type)
    .bind(record)
    .bind(now)
    .bind(authority)
    .bind(version)
    .execute(&mut *c)
    .await?;
    let payload = json!({
        "authority": serde_json::from_str::<Value>(authority).unwrap_or(Value::Null),
        "id": record,
        "state": result.get("status").or_else(|| result.get("state")).cloned().unwrap_or(Value::Null),
        "companyId": actor.company_id,
        "branchId": actor.branch_id,
        "departmentId": actor.department_id,
        "version": version,
    });
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$7,$5,$6)",
    )
    .bind(command)
    .bind(operation)
    .bind(&record_type)
    .bind(record)
    .bind(payload.to_string())
    .bind(now)
    .bind(version)
    .execute(&mut *c)
    .await?;
    Ok(())
}

pub async fn create_sales_invoice(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    input: &SalesInvoiceCreateInput,
    now: i64,
) -> Result<SalesInvoiceCreateResult, Error> {
    serializable(c).await?;
    validate(input)?;
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
    let mut normalized_input = input.clone();
    normalized_input.completion_note = js_trim(&normalized_input.completion_note).to_owned();
    for line in &mut normalized_input.lines {
        line.description = js_trim(&line.description).to_owned();
    }
    let input_json = serde_json::to_string(&normalized_input).map_err(|_| unavailable())?;
    let invoice_id = format!("invoice-{}-invoice", command.as_str());
    let request_hash = fingerprint(actor, "finance.sales-invoice.prepare", &invoice_id, &input_json);
    let authority = authority_json(actor);
    let existing = sqlx::query(
        "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
    )
    .bind(command.as_str())
    .fetch_optional(&mut *c)
    .await?;
    if let Some(row) = existing {
        let saved: String = row.try_get("request_hash")?;
        if saved != request_hash {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            ));
        }
        let raw: Option<String> = row.try_get("result_json")?;
        return raw
            .and_then(|value| serde_json::from_str::<SalesInvoiceCreateResult>(&value).ok())
            .ok_or_else(unavailable);
    }

    let defaults = sqlx::query(
        "SELECT chart_id,currency,receivable_account_id,revenue_account_id,tax_account_id \
         FROM finance_sales_defaults WHERE company_id=$1 AND currency=$2 AND status='approved' FOR SHARE",
    )
    .bind(&actor.company_id)
    .bind(&input.currency)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| conflict("SALES_DEFAULTS_REQUIRED", "Ask Finance to approve sales defaults for this currency."))?;
    let chart_id: String = defaults.try_get("chart_id")?;
    let receivable_id: String = defaults.try_get("receivable_account_id")?;
    let revenue_id: String = defaults.try_get("revenue_account_id")?;
    let tax_id: Option<String> = defaults.try_get("tax_account_id")?;
    let chart_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM finance_charts WHERE id=$1 AND company_id=$2 FOR SHARE",
    )
    .bind(&chart_id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *c)
    .await?;
    if chart_status.as_deref() != Some("active") {
        return Err(invalid("SALES_ACCOUNTS_INVALID", "Use active sales accounts on one Company ledger."));
    }
    let receivable = account(c, &actor.company_id, &chart_id, &input.currency, &receivable_id).await?;
    let revenue = account(c, &actor.company_id, &chart_id, &input.currency, &revenue_id).await?;
    let tax = match tax_id.as_deref() {
        Some(id) => Some(account(c, &actor.company_id, &chart_id, &input.currency, id).await?),
        None => None,
    };
    if receivable.status != "active"
        || revenue.status != "active"
        || tax.as_ref().is_some_and(|value| value.status != "active")
        || receivable.control_kind != "receivable"
        || revenue.category != "revenue"
        || tax
            .as_ref()
            .is_some_and(|value| value.category != "liability" || value.control_kind != "none")
        || receivable.id == revenue.id
        || tax.as_ref().is_some_and(|value| value.id == receivable.id || value.id == revenue.id)
    {
        return Err(invalid("SALES_ACCOUNTS_INVALID", "Use active sales accounts on one Company ledger."));
    }
    let party_row = sqlx::query(
        "SELECT id,code,name,kind,currency,tax_registration,version,status,person_id \
         FROM finance_parties WHERE id=$1 AND company_id=$2 AND status='active' FOR SHARE",
    )
    .bind(&input.party_id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| invalid("PARTY_INVALID", "The invoice party is unavailable."))?;
    let party = Party {
        id: party_row.try_get("id")?,
        code: party_row.try_get("code")?,
        name: party_row.try_get("name")?,
        kind: party_row.try_get("kind")?,
        currency: party_row.try_get("currency")?,
        tax_registration: party_row.try_get("tax_registration")?,
        version: party_row.try_get("version")?,
        status: party_row.try_get("status")?,
    };
    let person_id: Option<String> = party_row.try_get("person_id")?;
    if party.kind != "customer"
        || party.currency != input.currency
        || party.status != "active"
        || person_id.is_some()
    {
        return Err(invalid("PARTY_INVALID", "The invoice party is unavailable."));
    }
    let configured_scale: Option<i16> = sqlx::query_scalar(
        "SELECT scale FROM company_currency_scales WHERE company_id=$1 AND currency=$2",
    )
    .bind(&actor.company_id)
    .bind(&input.currency)
    .fetch_optional(&mut *c)
    .await?;
    if configured_scale.is_some_and(|scale| scale != 2) {
        return Err(invalid(
            "FINANCE_CURRENCY_SCALE_UNSUPPORTED",
            "This Finance ledger currently accepts currencies with two minor-unit decimals.",
        ));
    }
    let lines = calculate_lines(input, tax.as_ref().map(|value| value.id.as_str()))?;
    let subtotal: i64 = lines.iter().map(|line| line.subtotal_minor).sum();
    let tax_total: i64 = lines.iter().map(|line| line.tax_minor).sum();
    let total = subtotal
        .checked_add(tax_total)
        .filter(|value| *value > 0 && *value as i128 <= MAX_SAFE)
        .ok_or_else(|| invalid("INVALID_INVOICE_AMOUNT", "The invoice total exceeds supported precision."))?;
    let number: String = sqlx::query_scalar("SELECT erpx_sales_document_number($1,'INV',$2)")
        .bind(&actor.company_id)
        .bind(&input.invoice_date)
        .fetch_one(&mut *c)
        .await?;
    let money = fx(c, &actor.company_id, &input.currency, &input.invoice_date, total).await?;
    sqlx::query(
        "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,\
         subtotal_minor,tax_minor,total_minor,outstanding_minor,tax_code,debit_account_id,credit_account_id,\
         tax_account_id,source_type,source_id,created_by,created_at,updated_at) \
         VALUES($1,$2,$3,'receivable',$4,$5,$6,$7,$8,$9,$10,$10,NULL,$11,$12,$13,'standalone',NULL,$14,$15,$15)",
    )
    .bind(&invoice_id)
    .bind(&actor.company_id)
    .bind(&party.id)
    .bind(&number)
    .bind(&input.invoice_date)
    .bind(&input.due_date)
    .bind(&input.currency)
    .bind(subtotal)
    .bind(tax_total)
    .bind(total)
    .bind(&receivable.id)
    .bind(&revenue.id)
    .bind(tax.as_ref().map(|value| value.id.as_str()))
    .bind(&actor.user_id)
    .bind(now)
    .execute(&mut *c)
    .await?;
    sqlx::query("UPDATE finance_invoices SET line_mode='native_lines',line_revision=1 WHERE id=$1 AND company_id=$2")
        .bind(&invoice_id)
        .bind(&actor.company_id)
        .execute(&mut *c)
        .await?;
    let party_value = party_json(&party);
    let revenue_value = account_json(&revenue);
    let tax_value = tax.as_ref().map(account_json);
    let control_value = account_json(&receivable);
    for (ordinal, line) in lines.iter().enumerate() {
        let snapshot = json!({
            "schema": "finance.invoice.line.v1",
            "companyId": actor.company_id,
            "invoiceDate": input.invoice_date,
            "invoiceNumber": number,
            "kind": "receivable",
            "taxCode": Value::Null,
            "party": party_value,
            "netAccount": revenue_value,
            "taxAccount": tax_value,
            "controlAccount": control_value,
        });
        sqlx::query(
            "INSERT INTO finance_invoice_line_versions(id,company_id,invoice_id,source_version,ordinal,description,\
             quantity_millis,unit_price_minor,tax_bps,subtotal_minor,tax_minor,total_minor,currency,account_id,\
             tax_account_id,source_snapshot_json,origin_kind,created_by,created_at) \
             VALUES($1,$2,$3,1,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,'native_line',$16,$17)",
        )
        .bind(format!("{invoice_id}:v1:{ordinal}"))
        .bind(&actor.company_id)
        .bind(&invoice_id)
        .bind(i32::try_from(ordinal).map_err(|_| unavailable())?)
        .bind(&line.description)
        .bind(line.quantity_millis)
        .bind(line.unit_price_minor)
        .bind(i32::try_from(line.tax_bps).map_err(|_| unavailable())?)
        .bind(line.subtotal_minor)
        .bind(line.tax_minor)
        .bind(line.total_minor)
        .bind(&input.currency)
        .bind(&revenue.id)
        .bind(&line.tax_account_id)
        .bind(snapshot.to_string())
        .bind(&actor.user_id)
        .bind(now)
        .execute(&mut *c)
        .await?;
    }
    sqlx::query(
        "INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,\
         currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at) \
         VALUES($1,'finance_invoice',$2,'draft:v1',$3::date,$4,2,$5::numeric,$6::numeric,$7,$8,$9)",
    )
    .bind(&actor.company_id)
    .bind(&invoice_id)
    .bind(&input.invoice_date)
    .bind(&input.currency)
    .bind(total.to_string())
    .bind(money.usd_units.to_string())
    .bind(money.rate_id.as_deref())
    .bind(money.used_latest_fallback)
    .bind(now)
    .execute(&mut *c)
    .await?;

    let inner = json!({
        "id": invoice_id,
        "total": minor_decimal(total),
        "outstanding": minor_decimal(total),
        "status": "draft",
        "version": 1,
    });
    let inner_input = json!({
        "partyId": &input.party_id,
        "kind": "receivable",
        "invoiceNumber": &number,
        "invoiceDate": &input.invoice_date,
        "dueDate": &input.due_date,
        "currency": &input.currency,
        "subtotal": minor_decimal(subtotal),
        "tax": minor_decimal(tax_total),
        "taxAccountId": tax.as_ref().map(|value| value.id.clone()),
        "sourceType": Value::Null,
        "sourceId": Value::Null,
        "debitAccountId": &receivable.id,
        "creditAccountId": &revenue.id,
        "lines": &input.lines,
    });
    let inner_json = serde_json::to_string(&inner_input).map_err(|_| unavailable())?;
    let inner_hash = fingerprint(
        actor,
        "finance.invoice.create",
        &invoice_id,
        &inner_json,
    );
    receipt(
        c,
        actor,
        &authority,
        &format!("{}-invoice", command.as_str()),
        "finance.invoice.create",
        &inner_hash,
        &invoice_id,
        &inner,
        1,
        now,
    )
    .await?;
    let result = SalesInvoiceCreateResult {
        id: invoice_id.clone(),
        number: number.clone(),
        total: minor_decimal(total),
        outstanding: minor_decimal(total),
        status: "draft".into(),
        version: 1,
        completion_evidence: SalesInvoiceCompletionEvidence {
            note: js_trim(&input.completion_note).into(),
            confirmed_by: actor.user_id.clone(),
            confirmed_at: now,
            invoice_revision: 1,
        },
    };
    let outer = serde_json::to_value(&result).map_err(|_| unavailable())?;
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        "finance.sales-invoice.prepare",
        &request_hash,
        &invoice_id,
        &outer,
        1,
        now,
    )
    .await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn error_code(error: Error) -> &'static str {
        match error {
            PgTransactionError::Business(error) => error.code,
            PgTransactionError::Persistence(_) => "PERSISTENCE",
        }
    }
    fn input() -> SalesInvoiceCreateInput {
        SalesInvoiceCreateInput {
            party_id: "party-1".into(),
            invoice_date: "2026-11-01".into(),
            due_date: "2026-11-30".into(),
            currency: "THB".into(),
            completion_confirmed: true,
            completion_note: "Work completed".into(),
            lines: vec![SalesInvoiceCreateLine {
                description: "Service".into(),
                quantity: "1".into(),
                unit_price: "100.00".into(),
                tax_bps: 0,
            }],
        }
    }
    #[test]
    fn trimmed_note_and_description_use_js_trim_and_codepoint_length_like_zod() {
        // Baseline ascii, plus JS-trim of NBSP/ideographic space.
        assert!(validate(&input()).is_ok());
        let mut with_js_space = input();
        with_js_space.completion_note = "\u{00a0}\u{3000}Work completed\u{202f}".into();
        assert!(validate(&with_js_space).is_ok());
        // U+FEFF is ECMAScript whitespace (trimmed) but not Rust `White_Space`:
        // after JS trim this note is "ab" (< 3) and must fail, whereas the old
        // Rust-trim byte check accepted it.
        let mut feff_note = input();
        feff_note.completion_note = "\u{feff}\u{feff}ab".into();
        assert_eq!(
            error_code(validate(&feff_note).unwrap_err()),
            "INVALID_SALES_INVOICE"
        );
        // U+0085 is Rust whitespace but NOT ECMAScript trim: three NEL chars
        // are a valid 3-unit note under Zod and must pass.
        let mut nel_note = input();
        nel_note.completion_note = "\u{0085}\u{0085}\u{0085}".into();
        assert!(validate(&nel_note).is_ok());
        // BMP text: 2,000 Thai chars are 6,000 UTF-8 bytes but 2,000 UTF-16
        // units — Zod accepts, the old byte check rejected.
        let mut thai_note = input();
        thai_note.completion_note = "ก".repeat(2000);
        assert!(validate(&thai_note).is_ok());
        let mut thai_long = input();
        thai_long.completion_note = "ก".repeat(2001);
        assert!(validate(&thai_long).is_err());
        // Astral text: Zod 4 counts code points: 2,000 emoji is the
        // 2,000-code-point Zod maximum, 2,001 must fail.
        let mut emoji_note = input();
        emoji_note.completion_note = "🛠".repeat(2000);
        assert!(validate(&emoji_note).is_ok());
        let mut emoji_long = input();
        emoji_long.completion_note = "🛠".repeat(2001);
        assert!(validate(&emoji_long).is_err());
        // Description: 1..=1000 UTF-16 after JS trim.
        let mut thai_desc = input();
        thai_desc.lines[0].description = "ก".repeat(1000);
        assert!(validate(&thai_desc).is_ok());
        let mut emoji_desc = input();
        emoji_desc.lines[0].description = "🛠".repeat(1000);
        assert!(validate(&emoji_desc).is_ok());
        let mut desc_long = input();
        desc_long.lines[0].description = "🛠".repeat(1001);
        assert_eq!(
            error_code(validate(&desc_long).unwrap_err()),
            "INVALID_INVOICE_LINE"
        );
        // FEFF-only description is empty after JS trim; NEL-only is content.
        let mut feff_desc = input();
        feff_desc.lines[0].description = "\u{feff}".into();
        assert!(validate(&feff_desc).is_err());
        let mut nel_desc = input();
        nel_desc.lines[0].description = "\u{0085}".into();
        assert!(validate(&nel_desc).is_ok());
    }
    #[test]
    fn calculated_line_stores_the_js_trimmed_description() {
        let mut value = input();
        value.lines[0].description = "\u{feff}  Service  \u{00a0}".into();
        let lines = calculate_lines(&value, None).unwrap();
        assert_eq!(lines[0].description, "Service");
    }
}
