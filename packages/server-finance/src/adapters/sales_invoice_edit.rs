//! Native standalone sales-invoice edit and void. The caller's transaction owns
//! the invoice version guard, line revision, Money evidence and command receipt;
//! non-sales/non-standalone invoices return `None` so their owner path stays
//! authoritative.
use super::sales_invoice_create::{
    SalesInvoiceCommandActor, authority_json, fingerprint, receipt,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const MAX_SAFE: i128 = 9_007_199_254_740_991;
const RATE_SCALE: i128 = 1_000_000_000_000_000_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SalesInvoiceEditInput {
    pub kind: String,
    pub party_id: String,
    pub invoice_number: String,
    pub invoice_date: String,
    pub due_date: String,
    pub currency: String,
    pub subtotal: String,
    pub tax: String,
    pub debit_account_id: String,
    pub credit_account_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tax_account_id: Option<String>,
    pub expected_version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<Vec<SalesInvoiceEditLine>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SalesInvoiceEditLine {
    pub description: String,
    pub quantity: String,
    pub unit_price: String,
    pub tax_bps: i64,
    pub account_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tax_account_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SalesInvoiceVoidInput {
    pub expected_version: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesInvoiceEditResult {
    pub id: String,
    pub total: String,
    pub outstanding: String,
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesInvoiceVoidResult {
    pub id: String,
    pub status: String,
    pub version: i64,
}

struct Existing {
    kind: String,
    status: String,
    version: i64,
    source_type: Option<String>,
    line_mode: String,
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

struct Line {
    description: String,
    quantity_millis: i64,
    unit_price_minor: i64,
    tax_bps: i64,
    subtotal_minor: i64,
    tax_minor: i64,
    total_minor: i64,
    account: Account,
    tax_account: Option<Account>,
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

pub async fn lock_sales_invoice_edit_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE finance_charts,finance_accounts,finance_parties,\
         company_fx_rates,company_currency_scales IN SHARE MODE",
    )
    .execute(&mut *c)
    .await?;
    Ok(())
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

fn decimal_units_nonnegative(value: &str, scale: u32) -> Result<i128, Error> {
    if value.is_empty() || value.len() > 64 {
        return Err(invalid("INVALID_INVOICE_AMOUNT", "Use a nonnegative tax amount."));
    }
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > scale as usize
    {
        return Err(invalid("INVALID_INVOICE_AMOUNT", "Use a nonnegative tax amount."));
    }
    let whole = whole
        .parse::<i128>()
        .map_err(|_| invalid("INVALID_INVOICE_AMOUNT", "Use a nonnegative tax amount."))?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<width$}", width = scale as usize)
            .parse::<i128>()
            .map_err(|_| invalid("INVALID_INVOICE_AMOUNT", "Use a nonnegative tax amount."))?
    };
    whole
        .checked_mul(10i128.pow(scale))
        .and_then(|value| value.checked_add(fraction))
        .filter(|value| *value >= 0)
        .ok_or_else(|| invalid("INVALID_INVOICE_AMOUNT", "Use a nonnegative tax amount."))
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

fn validate(input: &SalesInvoiceEditInput) -> Result<(), Error> {
    if input.kind != "receivable"
        || input.party_id.is_empty()
        || input.party_id.len() > 200
        || input.invoice_number.trim().is_empty()
        || input.invoice_number.trim().len() > 100
        || !valid_date(&input.invoice_date)
        || !valid_date(&input.due_date)
        || input.due_date < input.invoice_date
        || input.currency.len() != 3
        || !input.currency.bytes().all(|byte| byte.is_ascii_uppercase())
        || input.expected_version < 1
        || input.debit_account_id.is_empty()
        || input.credit_account_id.is_empty()
        || input.debit_account_id == input.credit_account_id
        || input
            .tax_account_id
            .as_deref()
            .is_some_and(|value| {
                value.is_empty()
                    || value == input.debit_account_id
                    || value == input.credit_account_id
            })
        || input.lines.as_ref().is_some_and(|lines| lines.is_empty() || lines.len() > 100)
    {
        return Err(invalid(
            "INVALID_SALES_INVOICE",
            "Complete the customer, dates, accounts and invoice lines.",
        ));
    }
    Ok(())
}

async fn account(
    c: &mut PgConnection,
    company: &str,
    id: &str,
) -> Result<Account, Error> {
    let row = sqlx::query(
        "SELECT id,chart_id,code,name,category,currency,version,status,\
         COALESCE(control_kind,'none') control_kind \
         FROM finance_accounts WHERE company_id=$1 AND id=$2 FOR SHARE",
    )
    .bind(company)
    .bind(id)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| {
        invalid(
            "SALES_ACCOUNTS_INVALID",
            "Use active sales accounts on one Company ledger.",
        )
    })?;
    Ok(Account {
        id: row.try_get("id")?,
        chart_id: row.try_get("chart_id")?,
        code: row.try_get("code")?,
        name: row.try_get("name")?,
        category: row.try_get("category")?,
        currency: row.try_get("currency")?,
        version: row.try_get("version")?,
        status: row.try_get("status")?,
        control_kind: row.try_get("control_kind")?,
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
    let numerator = amount_minor as i128 * RATE_SCALE * 100;
    let denominator = source * 100;
    let usd = (numerator + denominator / 2) / denominator;
    Ok(Fx {
        usd_units: usd,
        rate_id: Some(rate_id),
        used_latest_fallback: fallback,
    })
}

async fn existing(
    c: &mut PgConnection,
    company: &str,
    id: &str,
) -> Result<Option<Existing>, Error> {
    let row = sqlx::query(
        "SELECT kind,status,version,source_type,line_mode \
         FROM finance_invoices WHERE id=$1 AND company_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    Ok(Some(Existing {
        kind: row.try_get("kind")?,
        status: row.try_get("status")?,
        version: row.try_get("version")?,
        source_type: row.try_get("source_type")?,
        line_mode: row.try_get("line_mode")?,
    }))
}

fn require_editable(existing: &Existing, expected_version: i64) -> Result<(), Error> {
    if existing.status != "draft" || existing.version != expected_version {
        return Err(conflict(
            "VERSION_CONFLICT",
            "Refresh the standalone draft before editing it.",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn line(
    c: &mut PgConnection,
    company: &str,
    chart_id: &str,
    currency: &str,
    control_id: &str,
    header_tax_id: Option<&str>,
    input: &SalesInvoiceEditLine,
) -> Result<Line, Error> {
    if input.description.trim().is_empty()
        || input.description.trim().len() > 500
        || !(0..=10_000).contains(&input.tax_bps)
    {
        return Err(invalid("INVALID_INVOICE_LINE", "Use a valid service line."));
    }
    let quantity_millis = decimal_units(&input.quantity, 3)?;
    let unit_price_minor = decimal_units(&input.unit_price, 2)?;
    let subtotal = (quantity_millis * unit_price_minor + 500) / 1000;
    let tax = (subtotal * input.tax_bps as i128 + 5_000) / 10_000;
    let total = subtotal + tax;
    if subtotal <= 0 || tax < 0 || total > MAX_SAFE {
        return Err(invalid(
            "INVALID_INVOICE_AMOUNT",
            "Provide a positive subtotal and nonnegative tax within supported precision.",
        ));
    }
    let net = account(c, company, &input.account_id).await?;
    if net.status != "active"
        || net.category != "revenue"
        || net.chart_id != chart_id
        || net.currency != currency
        || net.id == control_id
    {
        return Err(invalid(
            "SALES_ACCOUNTS_INVALID",
            "Use active sales accounts on one Company ledger.",
        ));
    }
    let tax_account = if tax > 0 {
        let id = input
            .tax_account_id
            .as_deref()
            .or(header_tax_id)
            .ok_or_else(|| {
                invalid(
                    "TAX_ACCOUNT_REQUIRED",
                    "Map a separate output tax account before recording tax.",
                )
            })?;
        let tax_account = account(c, company, id).await?;
        if tax_account.status != "active"
            || tax_account.category != "liability"
            || tax_account.control_kind != "none"
            || tax_account.chart_id != chart_id
            || tax_account.currency != currency
            || tax_account.id == control_id
            || tax_account.id == net.id
        {
            return Err(invalid(
                "SALES_ACCOUNTS_INVALID",
                "Use active sales accounts on one Company ledger.",
            ));
        }
        Some(tax_account)
    } else {
        None
    };
    Ok(Line {
        description: input.description.trim().into(),
        quantity_millis: i64::try_from(quantity_millis).map_err(|_| unavailable())?,
        unit_price_minor: i64::try_from(unit_price_minor).map_err(|_| unavailable())?,
        tax_bps: input.tax_bps,
        subtotal_minor: i64::try_from(subtotal).map_err(|_| unavailable())?,
        tax_minor: i64::try_from(tax).map_err(|_| unavailable())?,
        total_minor: i64::try_from(total).map_err(|_| unavailable())?,
        account: net,
        tax_account,
    })
}

pub async fn edit_sales_invoice(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    id: &str,
    input: &SalesInvoiceEditInput,
    now: i64,
) -> Result<Option<SalesInvoiceEditResult>, Error> {
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
    let Some(existing) = existing(c, &actor.company_id, id).await? else {
        return Ok(None);
    };
    if existing.kind != "receivable" || existing.source_type.as_deref() != Some("standalone") {
        return Ok(None);
    }
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let operation = "finance.invoice.update";
    let request_hash = fingerprint(actor, operation, id, &input_json);
    let authority = authority_json(actor);
    let saved = sqlx::query(
        "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
    )
    .bind(command.as_str())
    .fetch_optional(&mut *c)
    .await?;
    if let Some(row) = saved {
        let saved_hash: String = row.try_get("request_hash")?;
        if saved_hash != request_hash {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            ));
        }
        let raw: Option<String> = row.try_get("result_json")?;
        return raw
            .and_then(|value| serde_json::from_str::<SalesInvoiceEditResult>(&value).ok())
            .map(Some)
            .ok_or_else(unavailable);
    }
    require_editable(&existing, input.expected_version)?;
    let chart_id: String = sqlx::query_scalar(
        "SELECT chart_id FROM finance_accounts WHERE company_id=$1 AND id=$2",
    )
    .bind(&actor.company_id)
    .bind(&input.debit_account_id)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| {
        invalid(
            "SALES_ACCOUNTS_INVALID",
            "Use active sales accounts on one Company ledger.",
        )
    })?;
    let chart_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM finance_charts WHERE id=$1 AND company_id=$2 FOR SHARE",
    )
    .bind(&chart_id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *c)
    .await?;
    if chart_status.as_deref() != Some("active") {
        return Err(invalid(
            "SALES_ACCOUNTS_INVALID",
            "Use active sales accounts on one Company ledger.",
        ));
    }
    let debit = account(c, &actor.company_id, &input.debit_account_id).await?;
    let credit = account(c, &actor.company_id, &input.credit_account_id).await?;
    if debit.status != "active"
        || debit.control_kind != "receivable"
        || debit.chart_id != chart_id
        || debit.currency != input.currency
        || credit.status != "active"
        || credit.category != "revenue"
        || credit.chart_id != chart_id
        || credit.currency != input.currency
        || credit.id == debit.id
    {
        return Err(invalid(
            "SALES_ACCOUNTS_INVALID",
            "Use active sales accounts on one Company ledger.",
        ));
    }
    let tax = match input.tax_account_id.as_deref() {
        Some(value) => {
            let tax = account(c, &actor.company_id, value).await?;
            if tax.status != "active"
                || tax.category != "liability"
                || tax.control_kind != "none"
                || tax.chart_id != chart_id
                || tax.currency != input.currency
                || tax.id == debit.id
                || tax.id == credit.id
            {
                return Err(invalid(
                    "SALES_ACCOUNTS_INVALID",
                    "Use active sales accounts on one Company ledger.",
                ));
            }
            Some(tax)
        }
        None => None,
    };
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
    let number_taken: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_invoices WHERE company_id=$1 AND kind='receivable' \
         AND invoice_number=$2 AND id<>$3)",
    )
    .bind(&actor.company_id)
    .bind(input.invoice_number.trim())
    .bind(id)
    .fetch_one(&mut *c)
    .await?;
    if number_taken {
        return Err(conflict(
            "INVOICE_NUMBER_CONFLICT",
            "Use a unique invoice number for this Company.",
        ));
    }
    let revision = input.expected_version + 1;
    let (lines, subtotal, tax_total, line_mode) = match input.lines.as_deref() {
        Some(inputs) => {
            let mut lines = Vec::with_capacity(inputs.len());
            for input_line in inputs {
                lines.push(
                    line(
                        c,
                        &actor.company_id,
                        &chart_id,
                        &input.currency,
                        &debit.id,
                        tax.as_ref().map(|value| value.id.as_str()),
                        input_line,
                    )
                    .await?,
                );
            }
            let subtotal: i64 = lines.iter().map(|line| line.subtotal_minor).sum();
            let tax_total: i64 = lines.iter().map(|line| line.tax_minor).sum();
            if input.subtotal != minor_decimal(subtotal) || input.tax != minor_decimal(tax_total) {
                return Err(invalid(
                    "INVOICE_LINE_TOTAL_MISMATCH",
                    "The header subtotal and tax must equal the sum of the calculated lines.",
                ));
            }
            (lines, subtotal, tax_total, "native_lines")
        }
        None => {
            if existing.line_mode == "native_lines" {
                return Err(invalid(
                    "INVOICE_LINES_REQUIRED",
                    "Send the complete line revision when editing a multi-line invoice.",
                ));
            }
            let subtotal = decimal_units(&input.subtotal, 2)?;
            let tax_total = if input.tax.is_empty() {
                0
            } else {
                decimal_units_nonnegative(&input.tax, 2)?
            };
            let subtotal = i64::try_from(subtotal).map_err(|_| unavailable())?;
            let tax_total = i64::try_from(tax_total).map_err(|_| unavailable())?;
            if tax_total > 0 && tax.is_none() {
                return Err(invalid(
                    "TAX_ACCOUNT_REQUIRED",
                    "Map a separate output tax account before recording tax.",
                ));
            }
            (Vec::new(), subtotal, tax_total, "whole_document")
        }
    };
    let total = subtotal
        .checked_add(tax_total)
        .filter(|value| *value > 0 && *value as i128 <= MAX_SAFE)
        .ok_or_else(|| invalid("INVALID_INVOICE_AMOUNT", "The invoice total exceeds supported precision."))?;
    let money = fx(c, &actor.company_id, &input.currency, &input.invoice_date, total).await?;
    let updated = sqlx::query(
        "UPDATE finance_invoices SET party_id=$1,invoice_number=$2,invoice_date=$3,due_date=$4,\
         currency=$5,subtotal_minor=$6,tax_minor=$7,total_minor=$8,outstanding_minor=$8,\
         tax_account_id=$9,debit_account_id=$10,credit_account_id=$11,line_mode=$12,\
         line_revision=$13,version=$14,updated_at=$15 \
         WHERE id=$16 AND company_id=$17 AND status='draft' AND version=$18 AND source_type='standalone'",
    )
    .bind(&party.id)
    .bind(input.invoice_number.trim())
    .bind(&input.invoice_date)
    .bind(&input.due_date)
    .bind(&input.currency)
    .bind(subtotal)
    .bind(tax_total)
    .bind(total)
    .bind(tax.as_ref().map(|value| value.id.as_str()))
    .bind(&debit.id)
    .bind(&credit.id)
    .bind(line_mode)
    .bind(revision)
    .bind(revision)
    .bind(now)
    .bind(id)
    .bind(&actor.company_id)
    .bind(input.expected_version)
    .execute(&mut *c)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(conflict(
            "VERSION_CONFLICT",
            "Refresh the standalone draft before editing it.",
        ));
    }
    sqlx::query(
        "DELETE FROM finance_invoice_line_versions WHERE invoice_id=$1 AND company_id=$2 \
         AND source_version=$3",
    )
    .bind(id)
    .bind(&actor.company_id)
    .bind(revision)
    .execute(&mut *c)
    .await?;
    let party_value = party_json(&party);
    let control_value = account_json(&debit);
    if lines.is_empty() {
        let snapshot = json!({
            "schema": "finance.invoice.line.v1",
            "companyId": actor.company_id,
            "invoiceDate": input.invoice_date,
            "invoiceNumber": input.invoice_number.trim(),
            "kind": "receivable",
            "taxCode": Value::Null,
            "party": party_value,
            "netAccount": account_json(&credit),
            "taxAccount": tax.as_ref().map(account_json),
            "controlAccount": control_value,
        });
        sqlx::query(
            "INSERT INTO finance_invoice_line_versions(id,company_id,invoice_id,source_version,\
             ordinal,description,quantity_millis,unit_price_minor,tax_bps,subtotal_minor,tax_minor,\
             total_minor,currency,account_id,tax_account_id,source_snapshot_json,origin_kind,\
             created_by,created_at) \
             VALUES($1,$2,$3,$4,0,$5,NULL,NULL,NULL,$6,$7,$8,$9,$10,$11,$12,'historical_header',$13,$14)",
        )
        .bind(format!("{id}:v{revision}:0"))
        .bind(&actor.company_id)
        .bind(id)
        .bind(revision)
        .bind(format!("Whole document {}", input.invoice_number.trim()))
        .bind(subtotal)
        .bind(tax_total)
        .bind(total)
        .bind(&input.currency)
        .bind(&credit.id)
        .bind(tax.as_ref().map(|value| value.id.as_str()))
        .bind(snapshot.to_string())
        .bind(&actor.user_id)
        .bind(now)
        .execute(&mut *c)
        .await?;
    } else {
        for (ordinal, line) in lines.iter().enumerate() {
            let snapshot = json!({
                "schema": "finance.invoice.line.v1",
                "companyId": actor.company_id,
                "invoiceDate": input.invoice_date,
                "invoiceNumber": input.invoice_number.trim(),
                "kind": "receivable",
                "taxCode": Value::Null,
                "party": party_value,
                "netAccount": account_json(&line.account),
                "taxAccount": line.tax_account.as_ref().map(account_json),
                "controlAccount": control_value,
            });
            sqlx::query(
                "INSERT INTO finance_invoice_line_versions(id,company_id,invoice_id,source_version,\
                 ordinal,description,quantity_millis,unit_price_minor,tax_bps,subtotal_minor,tax_minor,\
                 total_minor,currency,account_id,tax_account_id,source_snapshot_json,origin_kind,\
                 created_by,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,'native_line',$17,$18)",
            )
            .bind(format!("{id}:v{revision}:{ordinal}"))
            .bind(&actor.company_id)
            .bind(id)
            .bind(revision)
            .bind(i32::try_from(ordinal).map_err(|_| unavailable())?)
            .bind(&line.description)
            .bind(line.quantity_millis)
            .bind(line.unit_price_minor)
            .bind(i32::try_from(line.tax_bps).map_err(|_| unavailable())?)
            .bind(line.subtotal_minor)
            .bind(line.tax_minor)
            .bind(line.total_minor)
            .bind(&input.currency)
            .bind(&line.account.id)
            .bind(line.tax_account.as_ref().map(|value| value.id.as_str()))
            .bind(snapshot.to_string())
            .bind(&actor.user_id)
            .bind(now)
            .execute(&mut *c)
            .await?;
        }
    }
    sqlx::query(
        "INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,\
         transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,\
         used_latest_fallback,captured_at) \
         VALUES($1,'finance_invoice',$2,$3,$4::date,$5,2,$6::numeric,$7::numeric,$8,$9,$10)",
    )
    .bind(&actor.company_id)
    .bind(id)
    .bind(format!("draft:v{revision}"))
    .bind(&input.invoice_date)
    .bind(&input.currency)
    .bind(total.to_string())
    .bind(money.usd_units.to_string())
    .bind(money.rate_id.as_deref())
    .bind(money.used_latest_fallback)
    .bind(now)
    .execute(&mut *c)
    .await?;
    let result = SalesInvoiceEditResult {
        id: id.to_owned(),
        total: minor_decimal(total),
        outstanding: minor_decimal(total),
        status: "draft".into(),
        version: revision,
    };
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        operation,
        &request_hash,
        id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        revision,
        now,
    )
    .await?;
    Ok(Some(result))
}

pub async fn void_sales_invoice(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    id: &str,
    input: &SalesInvoiceVoidInput,
    now: i64,
) -> Result<Option<SalesInvoiceVoidResult>, Error> {
    serializable(c).await?;
    if input.expected_version < 1
        || actor.company_id.is_empty()
        || actor.user_id.is_empty()
        || actor.assignment_id.is_empty()
        || actor.branch_id.is_some()
        || actor.department_id.is_some()
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "COMPANY_SCOPE_REQUIRED",
            "Use a Company Finance assignment to void a sales invoice.",
        )
        .into());
    }
    let Some(existing) = existing(c, &actor.company_id, id).await? else {
        return Ok(None);
    };
    if existing.kind != "receivable" || existing.source_type.as_deref() != Some("standalone") {
        return Ok(None);
    }
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let operation = "finance.invoice.void";
    let request_hash = fingerprint(actor, operation, id, &input_json);
    let authority = authority_json(actor);
    let saved = sqlx::query(
        "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
    )
    .bind(command.as_str())
    .fetch_optional(&mut *c)
    .await?;
    if let Some(row) = saved {
        let saved_hash: String = row.try_get("request_hash")?;
        if saved_hash != request_hash {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            ));
        }
        let raw: Option<String> = row.try_get("result_json")?;
        return raw
            .and_then(|value| serde_json::from_str::<SalesInvoiceVoidResult>(&value).ok())
            .map(Some)
            .ok_or_else(unavailable);
    }
    require_editable(&existing, input.expected_version)?;
    let revision = input.expected_version + 1;
    let updated = sqlx::query(
        "UPDATE finance_invoices SET status='voided',outstanding_minor=0,version=$1,updated_at=$2 \
         WHERE id=$3 AND company_id=$4 AND status='draft' AND version=$5 AND source_type='standalone'",
    )
    .bind(revision)
    .bind(now)
    .bind(id)
    .bind(&actor.company_id)
    .bind(input.expected_version)
    .execute(&mut *c)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(conflict(
            "VERSION_CONFLICT",
            "Refresh the standalone draft before voiding it.",
        ));
    }
    let result = SalesInvoiceVoidResult {
        id: id.to_owned(),
        status: "voided".into(),
        version: revision,
    };
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        operation,
        &request_hash,
        id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        revision,
        now,
    )
    .await?;
    Ok(Some(result))
}
