//! Native standalone supplier-bill (payable) create, edit, posting and
//! payable readback. Finance owns the invoice, line revision, Money evidence,
//! AP journal and command receipt in the caller's SERIALIZABLE transaction.
//! Field Service invoice sources are never consulted: only `kind='payable'`
//! invoices with `source_type='standalone'` are writable here, so matched
//! supplier bills and expense claims keep their existing owners.
use super::postgres_journal::PgJournalStore;
use super::postgres_journal_facts::{
    PgJournalPreparation, PgManualJournalCreateSources, embedded_currency_rules,
};
use super::sales_invoice_create::{SalesInvoiceCommandActor, authority_json, fingerprint, receipt};
use crate::{
    application::journal::{
        FinanceActor, JournalAuthority, JournalDraft, JournalStatus, prepare_journal,
    },
    domain::{
        company_money::FinanceCollation,
        journal::{JournalError, JournalLineInput, convert_at_fx},
    },
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};
use std::future::Future;

type Error = PgTransactionError<BusinessError>;

const MAX_SAFE: i128 = 9_007_199_254_740_991;
const RATE_SCALE: i128 = 1_000_000_000_000_000_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupplierBillLine {
    pub description: String,
    pub quantity: String,
    pub unit_price: String,
    pub tax_bps: i64,
    pub account_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_account_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupplierBillCreateInput {
    pub kind: String,
    pub party_id: String,
    pub invoice_number: String,
    pub invoice_date: String,
    pub due_date: String,
    pub currency: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_code: Option<String>,
    pub debit_account_id: String,
    pub credit_account_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_account_id: Option<String>,
    pub subtotal: String,
    #[serde(default = "zero_money")]
    pub tax: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<Vec<SupplierBillLine>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupplierBillEditInput {
    pub kind: String,
    pub party_id: String,
    pub invoice_number: String,
    pub invoice_date: String,
    pub due_date: String,
    pub currency: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_code: Option<String>,
    pub debit_account_id: String,
    pub credit_account_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_account_id: Option<String>,
    pub subtotal: String,
    #[serde(default = "zero_money")]
    pub tax: String,
    pub expected_version: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<Vec<SupplierBillLine>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupplierBillPostInput {
    pub period_id: String,
    pub entry_date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fx_rate: Option<String>,
    #[serde(default)]
    pub attachment_set_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fx_gain_account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fx_loss_account_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SupplierBillResult {
    pub id: String,
    pub number: String,
    pub total: String,
    pub outstanding: String,
    pub status: String,
    pub version: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reversal_journal_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupplierBillVoidInput {
    pub expected_version: i64,
    /// Required when voiding a posted bill: the open period and entry date of
    /// the reversal journal. Draft voids retain their existing shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SupplierBillPostResult {
    pub id: String,
    pub status: String,
    pub journal_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SupplierBillDetail {
    #[cfg_attr(feature = "contracts", ts(type = "Record<string, unknown>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Object))]
    pub invoice: Value,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub allocations: Vec<Value>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub lines: Vec<Value>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub line_history: Vec<Value>,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub thai_documents: Vec<Value>,
    #[cfg_attr(feature = "contracts", ts(type = "Record<string, unknown>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Object))]
    pub journal: Option<Value>,
}

#[derive(Clone)]
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

struct BillLine {
    description: String,
    quantity_millis: Option<i64>,
    unit_price_minor: Option<i64>,
    tax_bps: Option<i64>,
    subtotal_minor: i64,
    tax_minor: i64,
    total_minor: i64,
    account: Account,
    tax_account: Option<Account>,
    origin_kind: &'static str,
}

struct Invoice {
    id: String,
    kind: String,
    status: String,
    version: i64,
    source_type: Option<String>,
    source_id: Option<String>,
    source_version: Option<i64>,
    invoice_number: String,
    party_id: String,
    invoice_date: String,
    due_date: String,
    currency: String,
    subtotal_minor: i64,
    tax_minor: i64,
    total_minor: i64,
    outstanding_minor: i64,
    debit_account_id: String,
    credit_account_id: String,
    tax_account_id: Option<String>,
    line_revision: i64,
    line_mode: String,
    created_by: String,
}

fn zero_money() -> String {
    "0.00".into()
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
        "FINANCE_SUPPLIER_BILL_UNAVAILABLE",
        "The supplier bill could not be prepared.",
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

pub async fn lock_supplier_bill_write_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE finance_charts,finance_accounts,finance_parties,\
         company_fx_rates,company_currency_scales IN SHARE MODE",
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

pub async fn lock_supplier_bill_post_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    super::field_service_invoice_read::lock_service_invoice_sources(c).await?;
    Ok(())
}

pub async fn lock_supplier_bill_read_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await
}

fn decimal_units(value: &str, scale: u32) -> Result<i128, Error> {
    if value.is_empty() || value.len() > 64 {
        return Err(invalid(
            "INVALID_SUPPLIER_BILL_AMOUNT",
            "Use a positive quantity and unit price.",
        ));
    }
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > scale as usize
    {
        return Err(invalid(
            "INVALID_SUPPLIER_BILL_AMOUNT",
            "Use a positive quantity and unit price.",
        ));
    }
    let whole = whole.parse::<i128>().map_err(|_| {
        invalid(
            "INVALID_SUPPLIER_BILL_AMOUNT",
            "Use a positive quantity and unit price.",
        )
    })?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<width$}", width = scale as usize)
            .parse::<i128>()
            .map_err(|_| {
                invalid(
                    "INVALID_SUPPLIER_BILL_AMOUNT",
                    "Use a positive quantity and unit price.",
                )
            })?
    };
    whole
        .checked_mul(10i128.pow(scale))
        .and_then(|value| value.checked_add(fraction))
        .filter(|value| *value > 0 && *value <= MAX_SAFE)
        .ok_or_else(|| {
            invalid(
                "INVALID_SUPPLIER_BILL_AMOUNT",
                "Use a positive quantity and unit price.",
            )
        })
}

fn decimal_units_nonnegative(value: &str, scale: u32) -> Result<i128, Error> {
    if value.is_empty() || value.len() > 64 {
        return Err(invalid(
            "INVALID_SUPPLIER_BILL_AMOUNT",
            "Use a nonnegative tax amount.",
        ));
    }
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > scale as usize
    {
        return Err(invalid(
            "INVALID_SUPPLIER_BILL_AMOUNT",
            "Use a nonnegative tax amount.",
        ));
    }
    let whole = whole.parse::<i128>().map_err(|_| {
        invalid(
            "INVALID_SUPPLIER_BILL_AMOUNT",
            "Use a nonnegative tax amount.",
        )
    })?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<width$}", width = scale as usize)
            .parse::<i128>()
            .map_err(|_| {
                invalid(
                    "INVALID_SUPPLIER_BILL_AMOUNT",
                    "Use a nonnegative tax amount.",
                )
            })?
    };
    whole
        .checked_mul(10i128.pow(scale))
        .and_then(|value| value.checked_add(fraction))
        .filter(|value| *value >= 0)
        .ok_or_else(|| {
            invalid(
                "INVALID_SUPPLIER_BILL_AMOUNT",
                "Use a nonnegative tax amount.",
            )
        })
}

fn decimal_units_unbounded(value: &str, scale: u32) -> Result<i128, Error> {
    if value.is_empty() || value.len() > 80 {
        return Err(invalid(
            "MANUAL_RATE_REQUIRED",
            "A confirmed manual currency rate is required.",
        ));
    }
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > scale as usize
    {
        return Err(invalid(
            "MANUAL_RATE_REQUIRED",
            "A confirmed manual currency rate is required.",
        ));
    }
    let whole = whole.parse::<i128>().map_err(|_| {
        invalid(
            "MANUAL_RATE_REQUIRED",
            "A confirmed manual currency rate is required.",
        )
    })?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<width$}", width = scale as usize)
            .parse::<i128>()
            .map_err(|_| {
                invalid(
                    "MANUAL_RATE_REQUIRED",
                    "A confirmed manual currency rate is required.",
                )
            })?
    };
    whole
        .checked_mul(10i128.pow(scale))
        .and_then(|value| value.checked_add(fraction))
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            invalid(
                "MANUAL_RATE_REQUIRED",
                "A confirmed manual currency rate is required.",
            )
        })
}

fn unique_invoice_number(error: sqlx::Error) -> Error {
    if error
        .as_database_error()
        .and_then(|value| value.code())
        .as_deref()
        == Some("23505")
    {
        return conflict(
            "INVOICE_NUMBER_CONFLICT",
            "Use a unique supplier bill number for this Company.",
        );
    }
    error.into()
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

fn company_actor(actor: &SalesInvoiceCommandActor, message: &'static str) -> Result<(), Error> {
    if actor.company_id.is_empty()
        || actor.user_id.is_empty()
        || actor.assignment_id.is_empty()
        || actor.branch_id.is_some()
        || actor.department_id.is_some()
    {
        return Err(
            BusinessError::new(FailureKind::Forbidden, "COMPANY_SCOPE_REQUIRED", message).into(),
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_header(
    kind: &str,
    party_id: &str,
    invoice_number: &str,
    invoice_date: &str,
    due_date: &str,
    currency: &str,
    tax_code: Option<&str>,
    debit_account_id: &str,
    credit_account_id: &str,
    tax_account_id: Option<&str>,
    lines: Option<&[SupplierBillLine]>,
) -> Result<(), Error> {
    if kind != "payable"
        || party_id.is_empty()
        || party_id.len() > 200
        || invoice_number.trim().is_empty()
        || invoice_number.trim().len() > 100
        || !valid_date(invoice_date)
        || !valid_date(due_date)
        || due_date < invoice_date
        || currency.len() != 3
        || !currency.bytes().all(|byte| byte.is_ascii_uppercase())
        || tax_code.is_some_and(|value| value.trim().is_empty() || value.trim().len() > 64)
        || debit_account_id.is_empty()
        || credit_account_id.is_empty()
        || debit_account_id == credit_account_id
        || tax_account_id.is_some_and(|value| {
            value.is_empty() || value == debit_account_id || value == credit_account_id
        })
        || lines.is_some_and(|rows| rows.is_empty() || rows.len() > 100)
    {
        return Err(invalid(
            "INVALID_SUPPLIER_BILL",
            "Complete the supplier, dates, accounts and bill lines.",
        ));
    }
    Ok(())
}

async fn account(
    c: &mut PgConnection,
    company: &str,
    chart: &str,
    currency: &str,
    id: &str,
) -> Result<Account, Error> {
    let row = sqlx::query(
        "SELECT id,chart_id,code,name,category,currency,version,status,\
         COALESCE(control_kind,'none') control_kind \
         FROM finance_accounts WHERE company_id=$1 AND chart_id=$2 AND currency=$3 AND id=$4 FOR SHARE",
    )
    .bind(company)
    .bind(chart)
    .bind(currency)
    .bind(id)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| {
        invalid(
            "SUPPLIER_BILL_ACCOUNTS_INVALID",
            "Use active expense, asset and payable accounts on one Company ledger.",
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

fn debit_allowed(account: &Account, chart: &str, currency: &str) -> bool {
    account.status == "active"
        && account.chart_id == chart
        && account.currency == currency
        && matches!(
            account.category.as_str(),
            "expense" | "asset" | "other_expense"
        )
        && account.control_kind == "none"
}

/// A matched Commerce fulfillment debits the owner's goods-received/clearing
/// liability account (control_kind none), not an expense account. Standalone
/// and expense-claim bills retain the expense/asset-only rule.
fn debit_allowed_for_source(
    account: &Account,
    chart: &str,
    currency: &str,
    source_type: &str,
) -> bool {
    debit_allowed(account, chart, currency)
        || source_type == "commerce_fulfillment"
            && account.status == "active"
            && account.chart_id == chart
            && account.currency == currency
            && account.category == "liability"
            && account.control_kind == "none"
}

fn payable_allowed(account: &Account, chart: &str, currency: &str) -> bool {
    account.status == "active"
        && account.chart_id == chart
        && account.currency == currency
        && account.category == "liability"
        && account.control_kind == "payable"
}

fn tax_allowed(account: &Account, chart: &str, currency: &str) -> bool {
    account.status == "active"
        && account.chart_id == chart
        && account.currency == currency
        && account.category == "asset"
        && account.control_kind == "none"
}

async fn supplier_bill_posting_lines<M: PgSupplierBillMatch>(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    invoice: &Invoice,
    source_type: &str,
    matches: &mut M,
) -> Result<
    (
        String,
        Vec<JournalLineInput>,
        Option<SupplierBillMatchFacts>,
    ),
    Error,
> {
    let matched = if source_type == "commerce_fulfillment" {
        matches.load(c, &actor.company_id, &invoice.id).await?
    } else {
        None
    };
    if let Some(matched) = matched.as_ref() {
        let tax_account = invoice.tax_account_id.as_deref().unwrap_or("");
        if matched.status != "invoiced"
            || matched.invoice_id.as_deref() != Some(invoice.id.as_str())
            || !matched.quantity_matches
            || matched.currency != invoice.currency
            || matched.supplier_id != invoice.party_id
            || matched.invoice_date != invoice.invoice_date
            || matched.due_date != invoice.due_date
            || matched.clearing_account_id != invoice.debit_account_id
            || matched.payable_account_id != invoice.credit_account_id
            || matched.tax_account_id != tax_account
            || matched.subtotal_minor != invoice.subtotal_minor
            || matched.tax_minor != invoice.tax_minor
            || matched.subtotal_minor + matched.tax_minor != invoice.total_minor
        {
            return Err(conflict(
                "SUPPLIER_BILL_SOURCE_CHANGED",
                "Review the matched invoice source before posting.",
            ));
        }
        let chart = matched.chart_id.clone();
        let clearing = account(
            c,
            &actor.company_id,
            &chart,
            &invoice.currency,
            &matched.clearing_account_id,
        )
        .await?;
        let payable = account(
            c,
            &actor.company_id,
            &chart,
            &invoice.currency,
            &matched.payable_account_id,
        )
        .await?;
        let tax = account(
            c,
            &actor.company_id,
            &chart,
            &invoice.currency,
            &matched.tax_account_id,
        )
        .await?;
        let variance_minor = matched.subtotal_minor - matched.expected_subtotal_minor;
        let variance = match matched.variance_account_id.as_deref() {
            Some(id) if variance_minor != 0 => {
                Some(account(c, &actor.company_id, &chart, &invoice.currency, id).await?)
            }
            _ => None,
        };
        if !debit_allowed_for_source(&clearing, &chart, &invoice.currency, source_type)
            || !payable_allowed(&payable, &chart, &invoice.currency)
            || !tax_allowed(&tax, &chart, &invoice.currency)
            || clearing.id == payable.id
            || clearing.id == tax.id
            || payable.id == tax.id
            || variance.as_ref().is_some_and(|value| {
                !debit_allowed(value, &chart, &invoice.currency)
                    || value.id == clearing.id
                    || value.id == payable.id
                    || value.id == tax.id
            })
            || (variance_minor != 0 && variance.is_none())
            || matched.expected_subtotal_minor < 0
        {
            return Err(invalid(
                "SUPPLIER_BILL_ACCOUNTS_INVALID",
                "Use active clearing, variance, tax and payable accounts on one Company ledger.",
            ));
        }
        let description = Some(format!("Supplier bill {}", invoice.invoice_number));
        let mut lines = Vec::new();
        if matched.expected_subtotal_minor > 0 {
            lines.push(JournalLineInput {
                account_id: clearing.id.clone(),
                description: description.clone(),
                debit: minor_decimal(matched.expected_subtotal_minor),
                credit: "0".into(),
                dimensions: None,
            });
        }
        if variance_minor != 0 {
            lines.push(JournalLineInput {
                account_id: variance
                    .as_ref()
                    .ok_or_else(|| {
                        invalid(
                            "SUPPLIER_BILL_VARIANCE_ACCOUNT_REQUIRED",
                            "This matched bill needs a variance posting account.",
                        )
                    })?
                    .id
                    .clone(),
                description: description.clone(),
                debit: if variance_minor > 0 {
                    minor_decimal(variance_minor)
                } else {
                    "0".into()
                },
                credit: if variance_minor < 0 {
                    minor_decimal(-variance_minor)
                } else {
                    "0".into()
                },
                dimensions: None,
            });
        }
        if matched.tax_minor > 0 {
            lines.push(JournalLineInput {
                account_id: tax.id.clone(),
                description: description.clone(),
                debit: minor_decimal(matched.tax_minor),
                credit: "0".into(),
                dimensions: None,
            });
        }
        lines.push(JournalLineInput {
            account_id: payable.id.clone(),
            description,
            debit: "0".into(),
            credit: minor_decimal(matched.subtotal_minor + matched.tax_minor),
            dimensions: None,
        });
        return Ok((chart, lines, Some(matched.clone())));
    }
    let chart = chart_for_debit(c, &actor.company_id, &invoice.debit_account_id).await?;
    let debit = account(
        c,
        &actor.company_id,
        &chart,
        &invoice.currency,
        &invoice.debit_account_id,
    )
    .await?;
    let credit = account(
        c,
        &actor.company_id,
        &chart,
        &invoice.currency,
        &invoice.credit_account_id,
    )
    .await?;
    let tax = match invoice.tax_account_id.as_deref() {
        Some(id) => Some(account(c, &actor.company_id, &chart, &invoice.currency, id).await?),
        None => None,
    };
    if !debit_allowed_for_source(&debit, &chart, &invoice.currency, source_type)
        || !payable_allowed(&credit, &chart, &invoice.currency)
        || tax.as_ref().is_some_and(|value| {
            !tax_allowed(value, &chart, &invoice.currency)
                || value.id == debit.id
                || value.id == credit.id
        })
        || (invoice.tax_minor > 0 && tax.is_none())
        || invoice.subtotal_minor <= 0
        || invoice.total_minor != invoice.subtotal_minor + invoice.tax_minor
        || invoice.outstanding_minor != invoice.total_minor
    {
        return Err(invalid(
            "SUPPLIER_BILL_ACCOUNTS_INVALID",
            "Use active expense, asset and payable accounts on one Company ledger.",
        ));
    }
    let description = Some(format!("Supplier bill {}", invoice.invoice_number));
    let mut lines = vec![
        JournalLineInput {
            account_id: debit.id.clone(),
            description: description.clone(),
            debit: minor_decimal(invoice.subtotal_minor),
            credit: "0".into(),
            dimensions: None,
        },
        JournalLineInput {
            account_id: credit.id.clone(),
            description: description.clone(),
            debit: "0".into(),
            credit: minor_decimal(invoice.total_minor),
            dimensions: None,
        },
    ];
    if invoice.tax_minor > 0 {
        lines.push(JournalLineInput {
            account_id: tax
                .as_ref()
                .ok_or_else(|| {
                    invalid(
                        "TAX_ACCOUNT_REQUIRED",
                        "This supplier bill needs a separate input tax posting account.",
                    )
                })?
                .id
                .clone(),
            description,
            debit: minor_decimal(invoice.tax_minor),
            credit: "0".into(),
            dimensions: None,
        });
    }
    Ok((chart, lines, None))
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

async fn party(c: &mut PgConnection, company: &str, id: &str) -> Result<Party, Error> {
    let row = sqlx::query(
        "SELECT id,code,name,kind,currency,tax_registration,version,status,person_id \
         FROM finance_parties WHERE id=$1 AND company_id=$2 AND status='active' FOR SHARE",
    )
    .bind(id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| invalid("PARTY_INVALID", "The supplier is unavailable."))?;
    let party = Party {
        id: row.try_get("id")?,
        code: row.try_get("code")?,
        name: row.try_get("name")?,
        kind: row.try_get("kind")?,
        currency: row.try_get("currency")?,
        tax_registration: row.try_get("tax_registration")?,
        version: row.try_get("version")?,
        status: row.try_get("status")?,
    };
    let person_id: Option<String> = row.try_get("person_id")?;
    if party.kind != "supplier" || party.status != "active" || person_id.is_some() {
        return Err(invalid("PARTY_INVALID", "The supplier is unavailable."));
    }
    Ok(party)
}

async fn require_currency_scale(
    c: &mut PgConnection,
    company: &str,
    currency: &str,
) -> Result<(), Error> {
    let configured_scale: Option<i16> = sqlx::query_scalar(
        "SELECT scale FROM company_currency_scales WHERE company_id=$1 AND currency=$2",
    )
    .bind(company)
    .bind(currency)
    .fetch_optional(&mut *c)
    .await?;
    if configured_scale.is_some_and(|scale| scale != 2) {
        return Err(invalid(
            "FINANCE_CURRENCY_SCALE_UNSUPPORTED",
            "This Finance ledger currently accepts currencies with two minor-unit decimals.",
        ));
    }
    Ok(())
}

async fn chart_for_debit(
    c: &mut PgConnection,
    company: &str,
    debit_account_id: &str,
) -> Result<String, Error> {
    let chart_id: String =
        sqlx::query_scalar("SELECT chart_id FROM finance_accounts WHERE company_id=$1 AND id=$2")
            .bind(company)
            .bind(debit_account_id)
            .fetch_optional(&mut *c)
            .await?
            .ok_or_else(|| {
                invalid(
                    "SUPPLIER_BILL_ACCOUNTS_INVALID",
                    "Use active expense, asset and payable accounts on one Company ledger.",
                )
            })?;
    let chart_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM finance_charts WHERE id=$1 AND company_id=$2 FOR SHARE",
    )
    .bind(&chart_id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?;
    if chart_status.as_deref() != Some("active") {
        return Err(invalid(
            "SUPPLIER_BILL_ACCOUNTS_INVALID",
            "Use active expense, asset and payable accounts on one Company ledger.",
        ));
    }
    Ok(chart_id)
}

#[allow(clippy::too_many_arguments)]
async fn bill_line(
    c: &mut PgConnection,
    company: &str,
    chart: &str,
    currency: &str,
    control_id: &str,
    header_tax: Option<&Account>,
    input: &SupplierBillLine,
) -> Result<BillLine, Error> {
    if input.description.trim().is_empty()
        || input.description.trim().len() > 1000
        || !(0..=10_000).contains(&input.tax_bps)
    {
        return Err(invalid(
            "INVALID_SUPPLIER_BILL_LINE",
            "Use a valid bill line.",
        ));
    }
    let quantity_millis = decimal_units(&input.quantity, 3)?;
    let unit_price_minor = decimal_units(&input.unit_price, 2)?;
    let subtotal = (quantity_millis * unit_price_minor + 500) / 1000;
    let tax = (subtotal * input.tax_bps as i128 + 5_000) / 10_000;
    let total = subtotal + tax;
    if subtotal <= 0 || tax < 0 || total > MAX_SAFE {
        return Err(invalid(
            "INVALID_SUPPLIER_BILL_AMOUNT",
            "Provide a positive subtotal and nonnegative tax within supported precision.",
        ));
    }
    let net = account(c, company, chart, currency, &input.account_id).await?;
    if !debit_allowed(&net, chart, currency) || net.id == control_id {
        return Err(invalid(
            "SUPPLIER_BILL_ACCOUNTS_INVALID",
            "Use active expense, asset and payable accounts on one Company ledger.",
        ));
    }
    let tax_account = if tax > 0 {
        let selected = match (input.tax_account_id.as_deref(), header_tax) {
            (Some(value), Some(header)) if value != header.id => {
                return Err(invalid(
                    "SUPPLIER_BILL_ACCOUNTS_INVALID",
                    "Use one input tax account for this supplier bill.",
                ));
            }
            (Some(value), _) => account(c, company, chart, currency, value).await?,
            (None, Some(header)) => header.clone(),
            (None, None) => {
                return Err(invalid(
                    "TAX_ACCOUNT_REQUIRED",
                    "Map a separate input tax account before recording tax.",
                ));
            }
        };
        if !tax_allowed(&selected, chart, currency)
            || selected.id == control_id
            || selected.id == net.id
        {
            return Err(invalid(
                "SUPPLIER_BILL_ACCOUNTS_INVALID",
                "Use active expense, asset and payable accounts on one Company ledger.",
            ));
        }
        Some(selected)
    } else {
        None
    };
    Ok(BillLine {
        description: input.description.trim().into(),
        quantity_millis: Some(i64::try_from(quantity_millis).map_err(|_| unavailable())?),
        unit_price_minor: Some(i64::try_from(unit_price_minor).map_err(|_| unavailable())?),
        tax_bps: Some(input.tax_bps),
        subtotal_minor: i64::try_from(subtotal).map_err(|_| unavailable())?,
        tax_minor: i64::try_from(tax).map_err(|_| unavailable())?,
        total_minor: i64::try_from(total).map_err(|_| unavailable())?,
        account: net,
        tax_account,
        origin_kind: "native_line",
    })
}

fn whole_line(
    description: String,
    subtotal: i64,
    tax: i64,
    total: i64,
    debit: &Account,
    tax_account: Option<&Account>,
) -> BillLine {
    BillLine {
        description,
        quantity_millis: None,
        unit_price_minor: None,
        tax_bps: None,
        subtotal_minor: subtotal,
        tax_minor: tax,
        total_minor: total,
        account: Account {
            id: debit.id.clone(),
            chart_id: debit.chart_id.clone(),
            code: debit.code.clone(),
            name: debit.name.clone(),
            category: debit.category.clone(),
            currency: debit.currency.clone(),
            version: debit.version,
            status: debit.status.clone(),
            control_kind: debit.control_kind.clone(),
        },
        tax_account: tax_account.cloned(),
        origin_kind: "whole_document",
    }
}

/// Recompute the complete bill line revision. Native lines must sum exactly to
/// the header subtotal/tax, and tax always needs an input-tax asset account.
#[allow(clippy::too_many_arguments)]
async fn calculate_lines(
    c: &mut PgConnection,
    company: &str,
    chart: &str,
    currency: &str,
    control_id: &str,
    debit: &Account,
    header_tax: Option<&Account>,
    invoice_number: &str,
    subtotal_input: &str,
    tax_input: &str,
    input: Option<&[SupplierBillLine]>,
) -> Result<(Vec<BillLine>, i64, i64, &'static str), Error> {
    match input {
        Some(rows) => {
            let mut lines = Vec::with_capacity(rows.len());
            for row in rows {
                lines.push(
                    bill_line(c, company, chart, currency, control_id, header_tax, row).await?,
                );
            }
            let subtotal: i64 = lines.iter().map(|line| line.subtotal_minor).sum();
            let tax: i64 = lines.iter().map(|line| line.tax_minor).sum();
            if tax > 0 && header_tax.is_none() {
                return Err(invalid(
                    "TAX_ACCOUNT_REQUIRED",
                    "Map a separate input tax account before recording tax.",
                ));
            }
            if subtotal_input != minor_decimal(subtotal) || tax_input != minor_decimal(tax) {
                return Err(invalid(
                    "SUPPLIER_BILL_LINE_TOTAL_MISMATCH",
                    "The header subtotal and tax must equal the sum of the calculated lines.",
                ));
            }
            Ok((lines, subtotal, tax, "native_lines"))
        }
        None => {
            let subtotal = decimal_units(subtotal_input, 2)?;
            let tax = if tax_input.is_empty() {
                0
            } else {
                decimal_units_nonnegative(tax_input, 2)?
            };
            let subtotal = i64::try_from(subtotal).map_err(|_| unavailable())?;
            let tax = i64::try_from(tax).map_err(|_| unavailable())?;
            if tax > 0 && header_tax.is_none() {
                return Err(invalid(
                    "TAX_ACCOUNT_REQUIRED",
                    "Map a separate input tax account before recording tax.",
                ));
            }
            let total = subtotal
                .checked_add(tax)
                .filter(|value| *value > 0 && *value as i128 <= MAX_SAFE)
                .ok_or_else(|| {
                    invalid(
                        "INVALID_SUPPLIER_BILL_AMOUNT",
                        "The bill total exceeds supported precision.",
                    )
                })?;
            Ok((
                vec![whole_line(
                    format!("Whole document {invoice_number}"),
                    subtotal,
                    tax,
                    total,
                    debit,
                    header_tax,
                )],
                subtotal,
                tax,
                "whole_document",
            ))
        }
    }
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
        if latest
            .as_ref()
            .is_none_or(|(_, _, current)| confirmed > *current)
        {
            latest = Some((id, rate, confirmed));
        }
    }
    let (rate_id, rate, fallback) = match dated {
        Some((id, rate, _)) => (id, rate, false),
        None => latest
            .map(|(id, rate, _)| (id, rate, true))
            .ok_or_else(|| {
                invalid(
                    "MANUAL_RATE_REQUIRED",
                    "A confirmed manual currency rate is required.",
                )
            })?,
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

async fn existing_invoice(
    c: &mut PgConnection,
    company: &str,
    id: &str,
) -> Result<Option<Invoice>, Error> {
    let row = sqlx::query(
        "SELECT id,kind,status,version,source_type,source_id,source_version,invoice_number,\
         party_id,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,\
         outstanding_minor,debit_account_id,credit_account_id,tax_account_id,line_revision,\
         line_mode,created_by \
         FROM finance_invoices WHERE id=$1 AND company_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    Ok(Some(Invoice {
        id: row.try_get("id")?,
        kind: row.try_get("kind")?,
        status: row.try_get("status")?,
        version: row.try_get("version")?,
        source_type: row.try_get("source_type")?,
        source_id: row.try_get("source_id")?,
        source_version: row.try_get("source_version")?,
        invoice_number: row.try_get("invoice_number")?,
        party_id: row.try_get("party_id")?,
        invoice_date: row.try_get("invoice_date")?,
        due_date: row.try_get("due_date")?,
        currency: row.try_get("currency")?,
        subtotal_minor: row.try_get("subtotal_minor")?,
        tax_minor: row.try_get("tax_minor")?,
        total_minor: row.try_get("total_minor")?,
        outstanding_minor: row.try_get("outstanding_minor")?,
        debit_account_id: row.try_get("debit_account_id")?,
        credit_account_id: row.try_get("credit_account_id")?,
        tax_account_id: row.try_get("tax_account_id")?,
        line_revision: row.try_get("line_revision")?,
        line_mode: row.try_get("line_mode")?,
        created_by: row.try_get("created_by")?,
    }))
}

fn editable(invoice: &Invoice, expected_version: i64) -> Result<(), Error> {
    if invoice.status != "draft" || invoice.version != expected_version {
        return Err(conflict(
            "VERSION_CONFLICT",
            "Refresh the supplier bill draft before editing it.",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_line_versions(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    invoice_id: &str,
    revision: i64,
    lines: &[BillLine],
    invoice_number: &str,
    invoice_date: &str,
    currency: &str,
    tax_code: Option<&str>,
    party: &Party,
    control: &Account,
    now: i64,
) -> Result<(), Error> {
    let party_value = party_json(party);
    let control_value = account_json(control);
    for (ordinal, line) in lines.iter().enumerate() {
        let snapshot = json!({
            "schema": "finance.invoice.line.v1",
            "companyId": actor.company_id,
            "invoiceDate": invoice_date,
            "invoiceNumber": invoice_number,
            "kind": "payable",
            "taxCode": tax_code,
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
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)",
        )
        .bind(format!("{invoice_id}:v{revision}:{ordinal}"))
        .bind(&actor.company_id)
        .bind(invoice_id)
        .bind(revision)
        .bind(i32::try_from(ordinal).map_err(|_| unavailable())?)
        .bind(&line.description)
        .bind(line.quantity_millis)
        .bind(line.unit_price_minor)
        .bind(
            line.tax_bps
                .map(i32::try_from)
                .transpose()
                .map_err(|_| unavailable())?,
        )
        .bind(line.subtotal_minor)
        .bind(line.tax_minor)
        .bind(line.total_minor)
        .bind(currency)
        .bind(&line.account.id)
        .bind(line.tax_account.as_ref().map(|value| value.id.as_str()))
        .bind(snapshot.to_string())
        .bind(line.origin_kind)
        .bind(&actor.user_id)
        .bind(now)
        .execute(&mut *c)
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_draft_money(
    c: &mut PgConnection,
    company: &str,
    invoice_id: &str,
    revision: i64,
    invoice_date: &str,
    currency: &str,
    total: i64,
    now: i64,
) -> Result<(), Error> {
    let money = fx(c, company, currency, invoice_date, total).await?;
    sqlx::query(
        "INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,\
         transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,\
         used_latest_fallback,captured_at) \
         VALUES($1,'finance_invoice',$2,$3,$4::date,$5,2,$6::numeric,$7::numeric,$8,$9,$10)",
    )
    .bind(company)
    .bind(invoice_id)
    .bind(format!("draft:v{revision}"))
    .bind(invoice_date)
    .bind(currency)
    .bind(total.to_string())
    .bind(money.usd_units.to_string())
    .bind(money.rate_id.as_deref())
    .bind(money.used_latest_fallback)
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(())
}

async fn replay(
    c: &mut PgConnection,
    command: &CommandKey,
    request_hash: &str,
) -> Result<Option<SupplierBillResult>, Error> {
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
    if saved != request_hash {
        return Err(conflict(
            "IDEMPOTENCY_CONFLICT",
            "This command key belongs to a different request.",
        ));
    }
    let raw: Option<String> = row.try_get("result_json")?;
    raw.and_then(|value| serde_json::from_str::<SupplierBillResult>(&value).ok())
        .map(Some)
        .ok_or_else(unavailable)
}

#[allow(clippy::too_many_arguments)]
async fn resolved_bill(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    input: &SupplierBillCreateInput,
    expected_version: Option<i64>,
    current: Option<&Invoice>,
) -> Result<
    (
        String,
        Party,
        Account,
        Account,
        Option<Account>,
        Vec<BillLine>,
        i64,
        i64,
        &'static str,
    ),
    Error,
> {
    let chart = chart_for_debit(c, &actor.company_id, &input.debit_account_id).await?;
    let debit = account(
        c,
        &actor.company_id,
        &chart,
        &input.currency,
        &input.debit_account_id,
    )
    .await?;
    let credit = account(
        c,
        &actor.company_id,
        &chart,
        &input.currency,
        &input.credit_account_id,
    )
    .await?;
    let tax = match input.tax_account_id.as_deref() {
        Some(id) => Some(account(c, &actor.company_id, &chart, &input.currency, id).await?),
        None => None,
    };
    if !debit_allowed(&debit, &chart, &input.currency)
        || !payable_allowed(&credit, &chart, &input.currency)
        || tax.as_ref().is_some_and(|value| {
            !tax_allowed(value, &chart, &input.currency)
                || value.id == debit.id
                || value.id == credit.id
        })
    {
        return Err(invalid(
            "SUPPLIER_BILL_ACCOUNTS_INVALID",
            "Use active expense, asset and payable accounts on one Company ledger.",
        ));
    }
    let supplier = party(c, &actor.company_id, &input.party_id).await?;
    if supplier.currency != input.currency {
        return Err(invalid(
            "PARTY_INVALID",
            "The supplier currency does not match the bill.",
        ));
    }
    require_currency_scale(c, &actor.company_id, &input.currency).await?;
    let invoice_number = input.invoice_number.trim();
    let number_taken: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_invoices WHERE company_id=$1 AND kind='payable' \
         AND invoice_number=$2 AND id<>$3)",
    )
    .bind(&actor.company_id)
    .bind(invoice_number)
    .bind(current.map(|value| value.id.as_str()).unwrap_or(""))
    .fetch_one(&mut *c)
    .await?;
    if number_taken {
        return Err(conflict(
            "INVOICE_NUMBER_CONFLICT",
            "Use a unique supplier bill number for this Company.",
        ));
    }
    let (lines, subtotal, tax_total, mode) = calculate_lines(
        c,
        &actor.company_id,
        &chart,
        &input.currency,
        &credit.id,
        &debit,
        tax.as_ref(),
        invoice_number,
        &input.subtotal,
        &input.tax,
        input.lines.as_deref(),
    )
    .await?;
    subtotal
        .checked_add(tax_total)
        .filter(|value| *value > 0 && *value as i128 <= MAX_SAFE)
        .ok_or_else(|| {
            invalid(
                "INVALID_SUPPLIER_BILL_AMOUNT",
                "The bill total exceeds supported precision.",
            )
        })?;
    if let Some(expected) = expected_version
        && current.is_none_or(|invoice| invoice.version != expected)
    {
        return Err(conflict(
            "VERSION_CONFLICT",
            "Refresh the supplier bill draft before editing it.",
        ));
    }
    Ok((
        chart, supplier, debit, credit, tax, lines, subtotal, tax_total, mode,
    ))
}

pub async fn void_supplier_bill(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    id: &str,
    input: &SupplierBillVoidInput,
    now: i64,
) -> Result<SupplierBillResult, Error> {
    serializable(c).await?;
    company_actor(
        actor,
        "Use a Company Finance assignment for supplier bills.",
    )?;
    if id.is_empty() || id.len() > 128 || input.expected_version < 1 {
        return Err(invalid(
            "INVALID_SUPPLIER_BILL",
            "Use a current supplier bill draft and version.",
        ));
    }
    let operation = "finance.invoice.void";
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let request_hash = fingerprint(actor, operation, id, &input_json);
    let authority = authority_json(actor);
    if let Some(result) = replay(c, command, &request_hash).await? {
        return Ok(result);
    }
    let row = sqlx::query(
        "SELECT status,kind,source_type,version,total_minor,invoice_number \
         FROM finance_invoices WHERE id=$1 AND company_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| not_found("INVOICE_NOT_FOUND", "Supplier bill not found."))?;
    let status: String = row.try_get("status")?;
    let kind: String = row.try_get("kind")?;
    let source_type: String = row.try_get("source_type")?;
    let version: i64 = row.try_get("version")?;
    if kind != "payable" || !matches!(source_type.as_str(), "standalone" | "commerce_fulfillment") {
        return Err(not_found("INVOICE_NOT_FOUND", "Supplier bill not found."));
    }
    if status != "draft" {
        return Err(conflict(
            "INVOICE_NOT_DRAFT",
            "Only a draft supplier bill can be voided.",
        ));
    }
    if version != input.expected_version {
        return Err(conflict(
            "VERSION_CONFLICT",
            "The supplier bill changed. Refresh and review again.",
        ));
    }
    sqlx::query(
        "UPDATE finance_invoices SET status='voided',outstanding_minor=0,version=version+1,updated_at=$1 \
         WHERE id=$2 AND company_id=$3 AND status='draft' AND version=$4",
    )
    .bind(now)
    .bind(id)
    .bind(&actor.company_id)
    .bind(input.expected_version)
    .execute(&mut *c)
    .await?;
    let result = SupplierBillResult {
        id: id.to_owned(),
        number: row.try_get("invoice_number")?,
        total: minor_decimal(row.try_get("total_minor")?),
        outstanding: "0.00".into(),
        status: "voided".into(),
        version: input.expected_version + 1,
        reversal_journal_id: None,
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
        input.expected_version + 1,
        now,
    )
    .await?;
    Ok(result)
}

pub const SUPPLIER_BILL_EVIDENCE_TARGET_TYPE: &str = "supplier_bill";
pub const SUPPLIER_BILL_EVIDENCE_PURPOSE_KEY: &str = "bill_match_support";
pub const SUPPLIER_BILL_EVIDENCE_COMPONENT_KIND: &str = "action";
pub const SUPPLIER_BILL_EVIDENCE_COMPONENT_REF: &str = "invoice-review";
pub const SUPPLIER_BILL_EVIDENCE_BINDING_TYPE: &str = "finance_supplier_bill_post";
pub const SUPPLIER_BILL_EVIDENCE_MAX_SETS: usize = 20;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupplierBillEvidenceRequest {
    pub set_ids: Vec<String>,
    pub target_id: String,
    pub expected_target_version: i64,
    pub command_id: String,
    pub binding_type: String,
    pub binding_id: String,
}

/// The compiled composition supplies the shared Records/Files evidence port
/// bound to this exact serializable Finance unit. Finance never binds file
/// state itself and never falls back to a TypeScript posting path.
pub trait PgSupplierBillEvidence: Send {
    type Prepared: Send;
    fn prepare(
        &mut self,
        c: &mut PgConnection,
        request: SupplierBillEvidenceRequest,
    ) -> impl Future<Output = Result<Self::Prepared, Error>> + Send;
    fn require_current(
        &mut self,
        c: &mut PgConnection,
        prepared: &Self::Prepared,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupplierBillSourceFacts {
    pub source_type: String,
    pub source_id: String,
    pub source_version: i64,
    pub status: String,
    pub invoice_id: Option<String>,
    pub posted: bool,
}

/// Owner-supplied read-only witness for a non-standalone payable source.
/// Finance never reads Commerce/Procurement/People tables itself.
pub trait PgSupplierBillSource: Send {
    fn load(
        &mut self,
        c: &mut PgConnection,
        company_id: &str,
        source_type: &str,
        source_id: &str,
    ) -> impl Future<Output = Result<Option<SupplierBillSourceFacts>, Error>> + Send;
}

struct UnmountedSupplierBillSource;
impl PgSupplierBillSource for UnmountedSupplierBillSource {
    fn load(
        &mut self,
        _c: &mut PgConnection,
        _company_id: &str,
        _source_type: &str,
        _source_id: &str,
    ) -> impl Future<Output = Result<Option<SupplierBillSourceFacts>, Error>> + Send {
        async move {
            Err(invalid(
                "SUPPLIER_BILL_SOURCE_UNSUPPORTED",
                "This supplier-bill source is unavailable in this assembly.",
            ))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupplierBillMatchFacts {
    pub match_id: String,
    pub match_version: i64,
    pub status: String,
    pub invoice_id: Option<String>,
    pub currency: String,
    pub chart_id: String,
    pub supplier_id: String,
    pub invoice_date: String,
    pub due_date: String,
    pub expected_subtotal_minor: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub clearing_account_id: String,
    pub payable_account_id: String,
    pub tax_account_id: String,
    pub variance_account_id: Option<String>,
    pub quantity_matches: bool,
}

/// Procurement-owned matched-bill witness and lifecycle port. Finance consumes
/// the reviewed match facts and asks Procurement to advance/return the match in
/// the same serializable unit.
pub trait PgSupplierBillMatch: Send {
    fn load(
        &mut self,
        c: &mut PgConnection,
        company_id: &str,
        invoice_id: &str,
    ) -> impl Future<Output = Result<Option<SupplierBillMatchFacts>, Error>> + Send;
    fn stage_post(
        &mut self,
        c: &mut PgConnection,
        company_id: &str,
        match_id: &str,
        expected_version: i64,
        invoice_id: &str,
        actor_id: &str,
        command_id: &str,
        now: i64,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn stage_return(
        &mut self,
        c: &mut PgConnection,
        company_id: &str,
        match_id: &str,
        expected_version: i64,
        invoice_id: &str,
        actor_id: &str,
        command_id: &str,
        now: i64,
        reason: &str,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

struct UnmountedSupplierBillMatch;
impl PgSupplierBillMatch for UnmountedSupplierBillMatch {
    fn load(
        &mut self,
        _c: &mut PgConnection,
        _company_id: &str,
        _invoice_id: &str,
    ) -> impl Future<Output = Result<Option<SupplierBillMatchFacts>, Error>> + Send {
        async move {
            Err(invalid(
                "SUPPLIER_BILL_MATCH_UNSUPPORTED",
                "The Procurement supplier-bill match is unavailable in this assembly.",
            ))
        }
    }
    fn stage_post(
        &mut self,
        _c: &mut PgConnection,
        _company_id: &str,
        _match_id: &str,
        _expected_version: i64,
        _invoice_id: &str,
        _actor_id: &str,
        _command_id: &str,
        _now: i64,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move {
            Err(invalid(
                "SUPPLIER_BILL_MATCH_UNSUPPORTED",
                "The Procurement supplier-bill match is unavailable in this assembly.",
            ))
        }
    }
    fn stage_return(
        &mut self,
        _c: &mut PgConnection,
        _company_id: &str,
        _match_id: &str,
        _expected_version: i64,
        _invoice_id: &str,
        _actor_id: &str,
        _command_id: &str,
        _now: i64,
        _reason: &str,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move {
            Err(invalid(
                "SUPPLIER_BILL_MATCH_UNSUPPORTED",
                "The Procurement supplier-bill match is unavailable in this assembly.",
            ))
        }
    }
}

struct UnmountedSupplierBillEvidence;
impl PgSupplierBillEvidence for UnmountedSupplierBillEvidence {
    type Prepared = ();
    fn prepare(
        &mut self,
        _c: &mut PgConnection,
        request: SupplierBillEvidenceRequest,
    ) -> impl Future<Output = Result<Self::Prepared, Error>> + Send {
        async move {
            if request.set_ids.is_empty() {
                Ok(())
            } else {
                Err(invalid(
                    "SUPPLIER_BILL_EVIDENCE_UNSUPPORTED",
                    "Supplier-bill review evidence is unavailable in this assembly.",
                ))
            }
        }
    }
    fn require_current(
        &mut self,
        _c: &mut PgConnection,
        _prepared: &Self::Prepared,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move { Ok(()) }
    }
}

/// Void a posted standalone supplier bill by posting its exact reversing
/// journal in the same serializable Finance unit. The reversing journal copies
/// the original posted lines with debit/credit swapped, reverses the original
/// journal, and advances the bill version. A different Finance approver must
/// sign the correction; a draft keeps the plain draft-void path.
pub async fn void_posted_supplier_bill(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    journal_actor: &FinanceActor,
    command: &CommandKey,
    id: &str,
    same_person_as_creator: bool,
    input: &SupplierBillVoidInput,
    now: i64,
) -> Result<SupplierBillResult, Error> {
    serializable(c).await?;
    company_actor(
        actor,
        "Use a Company Finance assignment for supplier bills.",
    )?;
    if journal_actor.user_id.as_str() != actor.user_id
        || journal_actor.company_id.as_str() != actor.company_id
        || journal_actor.assignment_id.as_str() != actor.assignment_id
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "FINANCE_CURRENT_AUTHORITY_REQUIRED",
            "Choose the current Finance approver.",
        )
        .into());
    }
    let period_id = input
        .period_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or_else(|| {
            invalid(
                "INVALID_SUPPLIER_BILL_VOID",
                "Choose an open period and entry date for the reversing journal.",
            )
        })?;
    let entry_date = input
        .entry_date
        .as_deref()
        .filter(|value| valid_date(value))
        .ok_or_else(|| {
            invalid(
                "INVALID_SUPPLIER_BILL_VOID",
                "Choose an open period and entry date for the reversing journal.",
            )
        })?;
    let memo = input.memo.as_deref().unwrap_or("").trim().to_owned();
    if memo.len() > 500 || memo.chars().any(char::is_control) {
        return Err(invalid(
            "INVALID_SUPPLIER_BILL_VOID",
            "Use a reason of at most 500 characters.",
        ));
    }
    if id.is_empty() || id.len() > 128 || input.expected_version < 1 {
        return Err(invalid(
            "INVALID_SUPPLIER_BILL",
            "Use a current posted supplier bill and version.",
        ));
    }
    let operation = "finance.invoice.void";
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let request_hash = fingerprint(actor, operation, id, &input_json);
    let authority = authority_json(actor);
    if let Some(result) = replay(c, command, &request_hash).await? {
        return Ok(result);
    }
    let row = sqlx::query(
        "SELECT status,kind,source_type,version,total_minor,outstanding_minor,invoice_number,         journal_id,created_by,approved_by          FROM finance_invoices WHERE id=$1 AND company_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| not_found("INVOICE_NOT_FOUND", "Supplier bill not found."))?;
    let status: String = row.try_get("status")?;
    let kind: String = row.try_get("kind")?;
    let source_type: String = row.try_get("source_type")?;
    let version: i64 = row.try_get("version")?;
    let journal_id: Option<String> = row.try_get("journal_id")?;
    let created_by: String = row.try_get("created_by")?;
    let approved_by: Option<String> = row.try_get("approved_by")?;
    if kind != "payable" || !matches!(source_type.as_str(), "standalone" | "commerce_fulfillment") {
        return Err(not_found("INVOICE_NOT_FOUND", "Supplier bill not found."));
    }
    if status != "posted" {
        return Err(conflict(
            "INVOICE_NOT_POSTED",
            "Only a posted supplier bill can be reversed here.",
        ));
    }
    if version != input.expected_version {
        return Err(conflict(
            "VERSION_CONFLICT",
            "The supplier bill changed. Refresh and review again.",
        ));
    }
    if same_person_as_creator
        || created_by == journal_actor.user_id.as_str()
        || approved_by.as_deref() == Some(journal_actor.user_id.as_str())
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "INDEPENDENT_APPROVER_REQUIRED",
            "A different Finance approver must reverse a posted supplier bill.",
        )
        .into());
    }
    let open_period: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_periods WHERE id=$1 AND company_id=$2 \
         AND state='open' AND start_date<=$3 AND end_date>=$3)",
    )
    .bind(period_id)
    .bind(&actor.company_id)
    .bind(entry_date)
    .fetch_one(&mut *c)
    .await?;
    if !open_period {
        return Err(conflict(
            "SUPPLIER_BILL_PERIOD_INVALID",
            "Choose an open Company period containing the reversal date.",
        ));
    }
    let allocated: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_allocations WHERE company_id=$1 AND invoice_id=$2)",
    )
    .bind(&actor.company_id)
    .bind(id)
    .fetch_one(&mut *c)
    .await?;
    if allocated {
        return Err(conflict(
            "SUPPLIER_BILL_ALLOCATED",
            "Reverse the applied payments before voiding this supplier bill.",
        ));
    }
    let journal_id = journal_id.ok_or_else(|| {
        conflict(
            "SUPPLIER_BILL_JOURNAL_REQUIRED",
            "The posted supplier bill has no retained journal.",
        )
    })?;
    let original = sqlx::query(
        "SELECT id,status FROM finance_journals WHERE id=$1 AND company_id=$2 FOR UPDATE",
    )
    .bind(&journal_id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| {
        conflict(
            "SUPPLIER_BILL_JOURNAL_REQUIRED",
            "The posted supplier bill journal is unavailable.",
        )
    })?;
    if original.try_get::<String, _>("status")? != "posted" {
        return Err(conflict(
            "SUPPLIER_BILL_ALREADY_REVERSED",
            "This supplier bill journal is no longer posted.",
        ));
    }
    let already: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_journals WHERE company_id=$1 AND          source_type='invoice_reversal' AND source_id=$2)",
    )
    .bind(&actor.company_id)
    .bind(id)
    .fetch_one(&mut *c)
    .await?;
    if already {
        return Err(conflict(
            "SUPPLIER_BILL_ALREADY_REVERSED",
            "This supplier bill already has a reversing journal.",
        ));
    }
    let reversal_id = format!("invoice-reversal-{}", command.as_str());
    let journal_number = format!("AP-REV-{}", command.as_str());
    let memo = if memo.is_empty() {
        format!(
            "Reversal of supplier bill {}",
            row.try_get::<String, _>("invoice_number")?
        )
    } else {
        memo
    };
    let inserted = sqlx::query(
        "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,         entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,         posted_by,posted_at,created_at,updated_at)          SELECT $1,company_id,chart_id,$2,$3,$4,currency,fx_rate_micros,         'invoice_reversal',$5,$6,'posted',$7,$7,$8,$8,$8          FROM finance_journals WHERE id=$9 AND company_id=$10 AND status='posted'",
    )
    .bind(&reversal_id)
    .bind(period_id)
    .bind(&journal_number)
    .bind(entry_date)
    .bind(id)
    .bind(&memo)
    .bind(journal_actor.user_id.as_str())
    .bind(now)
    .bind(&journal_id)
    .bind(&actor.company_id)
    .execute(&mut *c)
    .await?;
    if inserted.rows_affected() != 1 {
        return Err(conflict(
            "SUPPLIER_BILL_JOURNAL_REQUIRED",
            "The posted supplier bill journal changed before the reversal committed.",
        ));
    }
    let lines = sqlx::query(
        "INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,         credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal)          SELECT $1||'-'||ordinal,$1,account_id,description,credit_minor,debit_minor,         base_credit_minor,base_debit_minor,dimension_json,ordinal          FROM finance_journal_lines WHERE journal_id=$2",
    )
    .bind(&reversal_id)
    .bind(&journal_id)
    .execute(&mut *c)
    .await?;
    if lines.rows_affected() == 0 {
        return Err(conflict(
            "SUPPLIER_BILL_JOURNAL_REQUIRED",
            "The posted supplier bill journal has no retained lines.",
        ));
    }
    let reversed = sqlx::query(
        "UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=$1,         reversed_at=$2,updated_at=$2 WHERE id=$3 AND company_id=$4 AND status='posted'",
    )
    .bind(journal_actor.user_id.as_str())
    .bind(now)
    .bind(&journal_id)
    .bind(&actor.company_id)
    .execute(&mut *c)
    .await?;
    if reversed.rows_affected() != 1 {
        return Err(conflict(
            "SUPPLIER_BILL_ALREADY_REVERSED",
            "This supplier bill journal changed before the reversal committed.",
        ));
    }
    let updated = sqlx::query(
        "UPDATE finance_invoices SET status='voided',outstanding_minor=0,         base_outstanding_minor=0,version=version+1,updated_at=$1          WHERE id=$2 AND company_id=$3 AND status='posted' AND version=$4          AND kind='payable' AND source_type IN ('standalone','commerce_fulfillment')",
    )
    .bind(now)
    .bind(id)
    .bind(&actor.company_id)
    .bind(input.expected_version)
    .execute(&mut *c)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(conflict(
            "VERSION_CONFLICT",
            "The supplier bill changed. Refresh and review again.",
        ));
    }
    let result = SupplierBillResult {
        id: id.to_owned(),
        number: row.try_get("invoice_number")?,
        total: minor_decimal(row.try_get("total_minor")?),
        outstanding: "0.00".into(),
        status: "voided".into(),
        version: input.expected_version + 1,
        reversal_journal_id: Some(reversal_id),
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
        input.expected_version + 1,
        now,
    )
    .await?;
    Ok(result)
}

pub async fn create_supplier_bill(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    input: &SupplierBillCreateInput,
    now: i64,
) -> Result<SupplierBillResult, Error> {
    serializable(c).await?;
    validate_header(
        &input.kind,
        &input.party_id,
        &input.invoice_number,
        &input.invoice_date,
        &input.due_date,
        &input.currency,
        input.tax_code.as_deref(),
        &input.debit_account_id,
        &input.credit_account_id,
        input.tax_account_id.as_deref(),
        input.lines.as_deref(),
    )?;
    company_actor(
        actor,
        "Use a Company Finance assignment for supplier bills.",
    )?;
    let invoice_id = format!("invoice-{}-bill", command.as_str());
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let request_hash = fingerprint(
        actor,
        "finance.supplier-bill.create",
        &invoice_id,
        &input_json,
    );
    let authority = authority_json(actor);
    if let Some(result) = replay(c, command, &request_hash).await? {
        return Ok(result);
    }
    let (_, supplier, debit, credit, tax, lines, subtotal, tax_total, mode) =
        resolved_bill(c, actor, input, None, None).await?;
    let total = subtotal + tax_total;
    let invoice_number = input.invoice_number.trim();
    let tax_code = input
        .tax_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    sqlx::query(
        "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,\
         due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,tax_code,\
         debit_account_id,credit_account_id,tax_account_id,source_type,source_id,created_by,\
         created_at,updated_at,line_mode,line_revision) \
         VALUES($1,$2,$3,'payable',$4,$5,$6,$7,$8,$9,$10,$10,$11,$12,$13,$14,'standalone',NULL,\
         $15,$16,$16,$17,1)",
    )
    .bind(&invoice_id)
    .bind(&actor.company_id)
    .bind(&supplier.id)
    .bind(invoice_number)
    .bind(&input.invoice_date)
    .bind(&input.due_date)
    .bind(&input.currency)
    .bind(subtotal)
    .bind(tax_total)
    .bind(total)
    .bind(tax_code)
    .bind(&debit.id)
    .bind(&credit.id)
    .bind(tax.as_ref().map(|value| value.id.as_str()))
    .bind(&actor.user_id)
    .bind(now)
    .bind(mode)
    .execute(&mut *c)
    .await
    .map_err(unique_invoice_number)?;
    insert_line_versions(
        c,
        actor,
        &invoice_id,
        1,
        &lines,
        invoice_number,
        &input.invoice_date,
        &input.currency,
        tax_code,
        &supplier,
        &credit,
        now,
    )
    .await?;
    insert_draft_money(
        c,
        &actor.company_id,
        &invoice_id,
        1,
        &input.invoice_date,
        &input.currency,
        total,
        now,
    )
    .await?;
    let result = SupplierBillResult {
        id: invoice_id.clone(),
        number: invoice_number.into(),
        total: minor_decimal(total),
        outstanding: minor_decimal(total),
        status: "draft".into(),
        version: 1,
        reversal_journal_id: None,
    };
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        "finance.supplier-bill.create",
        &request_hash,
        &invoice_id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        1,
        now,
    )
    .await?;
    Ok(result)
}

pub async fn edit_supplier_bill(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    id: &str,
    input: &SupplierBillEditInput,
    now: i64,
) -> Result<Option<SupplierBillResult>, Error> {
    serializable(c).await?;
    validate_header(
        &input.kind,
        &input.party_id,
        &input.invoice_number,
        &input.invoice_date,
        &input.due_date,
        &input.currency,
        input.tax_code.as_deref(),
        &input.debit_account_id,
        &input.credit_account_id,
        input.tax_account_id.as_deref(),
        input.lines.as_deref(),
    )?;
    company_actor(
        actor,
        "Use a Company Finance assignment for supplier bills.",
    )?;
    if input.expected_version < 1 {
        return Err(invalid(
            "INVALID_SUPPLIER_BILL",
            "Complete the supplier, dates, accounts and bill lines.",
        ));
    }
    let Some(existing) = existing_invoice(c, &actor.company_id, id).await? else {
        return Ok(None);
    };
    if existing.kind != "payable" || existing.source_type.as_deref() != Some("standalone") {
        return Ok(None);
    }
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let request_hash = fingerprint(actor, "finance.supplier-bill.update", id, &input_json);
    let authority = authority_json(actor);
    if let Some(result) = replay(c, command, &request_hash).await? {
        return Ok(Some(result));
    }
    editable(&existing, input.expected_version)?;
    if input.lines.is_none() && existing.line_mode == "native_lines" {
        return Err(invalid(
            "INVOICE_LINES_REQUIRED",
            "Send the complete line revision when editing a multi-line supplier bill.",
        ));
    }
    let (_, supplier, debit, credit, tax, lines, subtotal, tax_total, mode) = resolved_bill(
        c,
        actor,
        &create_input(input),
        Some(input.expected_version),
        Some(&existing),
    )
    .await?;
    let total = subtotal + tax_total;
    let revision = input.expected_version + 1;
    let invoice_number = input.invoice_number.trim();
    let tax_code = input
        .tax_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let updated = sqlx::query(
        "UPDATE finance_invoices SET party_id=$1,invoice_number=$2,invoice_date=$3,due_date=$4,\
         currency=$5,subtotal_minor=$6,tax_minor=$7,total_minor=$8,outstanding_minor=$8,\
         tax_code=$9,debit_account_id=$10,credit_account_id=$11,tax_account_id=$12,\
         line_mode=$13,line_revision=$14,version=$14,updated_at=$15 \
         WHERE id=$16 AND company_id=$17 AND kind='payable' AND status='draft' AND version=$18 \
         AND source_type='standalone'",
    )
    .bind(&supplier.id)
    .bind(invoice_number)
    .bind(&input.invoice_date)
    .bind(&input.due_date)
    .bind(&input.currency)
    .bind(subtotal)
    .bind(tax_total)
    .bind(total)
    .bind(tax_code)
    .bind(&debit.id)
    .bind(&credit.id)
    .bind(tax.as_ref().map(|value| value.id.as_str()))
    .bind(mode)
    .bind(revision)
    .bind(now)
    .bind(id)
    .bind(&actor.company_id)
    .bind(input.expected_version)
    .execute(&mut *c)
    .await
    .map_err(unique_invoice_number)?;
    if updated.rows_affected() != 1 {
        return Err(conflict(
            "VERSION_CONFLICT",
            "Refresh the supplier bill draft before editing it.",
        ));
    }
    insert_line_versions(
        c,
        actor,
        id,
        revision,
        &lines,
        invoice_number,
        &input.invoice_date,
        &input.currency,
        tax_code,
        &supplier,
        &credit,
        now,
    )
    .await?;
    insert_draft_money(
        c,
        &actor.company_id,
        id,
        revision,
        &input.invoice_date,
        &input.currency,
        total,
        now,
    )
    .await?;
    let result = SupplierBillResult {
        id: id.into(),
        number: invoice_number.into(),
        total: minor_decimal(total),
        outstanding: minor_decimal(total),
        status: "draft".into(),
        version: revision,
        reversal_journal_id: None,
    };
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        "finance.supplier-bill.update",
        &request_hash,
        id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        revision,
        now,
    )
    .await?;
    Ok(Some(result))
}

fn create_input(input: &SupplierBillEditInput) -> SupplierBillCreateInput {
    SupplierBillCreateInput {
        kind: input.kind.clone(),
        party_id: input.party_id.clone(),
        invoice_number: input.invoice_number.clone(),
        invoice_date: input.invoice_date.clone(),
        due_date: input.due_date.clone(),
        currency: input.currency.clone(),
        tax_code: input.tax_code.clone(),
        debit_account_id: input.debit_account_id.clone(),
        credit_account_id: input.credit_account_id.clone(),
        tax_account_id: input.tax_account_id.clone(),
        subtotal: input.subtotal.clone(),
        tax: input.tax.clone(),
        lines: input.lines.clone(),
    }
}

fn journal_error(error: PgTransactionError<JournalError>) -> Error {
    match error {
        PgTransactionError::Persistence(error) => PgTransactionError::Persistence(error),
        PgTransactionError::Business(error) => BusinessError::new(
            error.kind,
            error.code,
            "The supplier bill journal could not be posted.",
        )
        .into(),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn post_supplier_bill<A, C>(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    journal_actor: &FinanceActor,
    command: &CommandKey,
    id: &str,
    same_person_as_creator: bool,
    input: &SupplierBillPostInput,
    clock: &C,
    authority: &mut A,
) -> Result<SupplierBillPostResult, Error>
where
    A: JournalAuthority<Error = PgTransactionError<JournalError>> + Send,
    C: Clock + Sync,
{
    post_supplier_bill_with_evidence(
        c,
        actor,
        journal_actor,
        command,
        id,
        same_person_as_creator,
        input,
        clock,
        authority,
        &mut UnmountedSupplierBillEvidence,
        &mut UnmountedSupplierBillSource,
        &mut UnmountedSupplierBillMatch,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn post_supplier_bill_with_evidence<A, C, E, S, M>(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    journal_actor: &FinanceActor,
    command: &CommandKey,
    id: &str,
    same_person_as_creator: bool,
    input: &SupplierBillPostInput,
    clock: &C,
    authority: &mut A,
    evidence: &mut E,
    source: &mut S,
    matches: &mut M,
) -> Result<SupplierBillPostResult, Error>
where
    A: JournalAuthority<Error = PgTransactionError<JournalError>> + Send,
    C: Clock + Sync,
    E: PgSupplierBillEvidence,
    S: PgSupplierBillSource,
    M: PgSupplierBillMatch,
{
    serializable(c).await?;
    company_actor(
        actor,
        "Use a Company Finance assignment to post a supplier bill.",
    )?;
    if journal_actor.user_id.as_str() != actor.user_id
        || journal_actor.company_id.as_str() != actor.company_id
        || journal_actor.assignment_id.as_str() != actor.assignment_id
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "FINANCE_CURRENT_AUTHORITY_REQUIRED",
            "Choose the current Finance approver.",
        )
        .into());
    }
    if input.period_id.trim().is_empty()
        || input.period_id.len() > 128
        || !valid_date(&input.entry_date)
        || input.expected_version.is_some_and(|value| value < 1)
    {
        return Err(invalid(
            "INVALID_SUPPLIER_BILL_POST",
            "Use an open period, an entry date and a supplier bill draft.",
        ));
    }
    if input.attachment_set_ids.len() > SUPPLIER_BILL_EVIDENCE_MAX_SETS
        || input
            .attachment_set_ids
            .iter()
            .any(|value| value.is_empty())
    {
        return Err(invalid(
            "INVALID_SUPPLIER_BILL_POST",
            "Choose up to twenty review-evidence file sets.",
        ));
    }
    if input.fx_gain_account_id.is_some() || input.fx_loss_account_id.is_some() {
        return Err(invalid(
            "SUPPLIER_BILL_GOODS_FX_UNSUPPORTED",
            "Goods FX accounts apply only to matched supplier bills.",
        ));
    }
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let request_hash = fingerprint(actor, "finance.supplier-bill.post", id, &input_json);
    let authority_evidence = authority_json(actor);
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
            .and_then(|value| serde_json::from_str::<SupplierBillPostResult>(&value).ok())
            .ok_or_else(unavailable);
    }
    let invoice = existing_invoice(c, &actor.company_id, id)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "INVOICE_NOT_FOUND",
                "A postable supplier bill draft was not found.",
            )
        })?;
    let source_type = invoice
        .source_type
        .as_deref()
        .filter(|value| matches!(*value, "standalone" | "commerce_fulfillment"))
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "INVOICE_NOT_FOUND",
                "A postable supplier bill draft was not found.",
            )
        })?;
    if invoice.kind != "payable" {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "INVOICE_NOT_FOUND",
            "A postable supplier bill draft was not found.",
        )
        .into());
    }
    if source_type != "standalone" {
        let source_id = invoice
            .source_id
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                conflict(
                    "SUPPLIER_BILL_SOURCE_REQUIRED",
                    "The supplier bill has no current owner source.",
                )
            })?;
        let facts = source
            .load(c, &actor.company_id, source_type, source_id)
            .await?
            .ok_or_else(|| {
                conflict(
                    "SUPPLIER_BILL_SOURCE_CHANGED",
                    "The supplier bill source is no longer current.",
                )
            })?;
        if facts.source_type != source_type
            || facts.source_id != source_id
            || facts.invoice_id.as_deref() != Some(id)
            || facts.source_version < 1
            || invoice.source_version != Some(facts.source_version)
            || (source_type == "commerce_fulfillment"
                && (facts.status != "purchase" || !facts.posted))
        {
            return Err(conflict(
                "SUPPLIER_BILL_SOURCE_CHANGED",
                "The supplier bill source is no longer current.",
            ));
        }
    }
    if invoice.status != "draft" {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "INVOICE_NOT_FOUND",
            "A postable supplier bill draft was not found.",
        )
        .into());
    }
    if same_person_as_creator || invoice.created_by == journal_actor.user_id.as_str() {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "INDEPENDENT_APPROVER_REQUIRED",
            "A different finance approver must post a supplier bill.",
        )
        .into());
    }
    if let Some(expected) = input.expected_version
        && expected != invoice.version
    {
        return Err(conflict(
            "VERSION_CONFLICT",
            "The supplier bill changed. Refresh it before posting.",
        ));
    }
    let prepared_evidence = if input.attachment_set_ids.is_empty() {
        None
    } else {
        Some(
            evidence
                .prepare(
                    c,
                    SupplierBillEvidenceRequest {
                        set_ids: input.attachment_set_ids.clone(),
                        target_id: id.to_owned(),
                        expected_target_version: invoice.version,
                        command_id: command.as_str().to_owned(),
                        binding_type: SUPPLIER_BILL_EVIDENCE_BINDING_TYPE.into(),
                        binding_id: format!("{id}:{}", invoice.version + 1),
                    },
                )
                .await?,
        )
    };
    let (chart, lines, matched) =
        supplier_bill_posting_lines(c, actor, &invoice, source_type, matches).await?;
    let draft = JournalDraft {
        chart_id: chart.clone(),
        period_id: input.period_id.trim().into(),
        entry_date: input.entry_date.clone(),
        currency: invoice.currency.clone(),
        fx_rate: input.fx_rate.clone(),
        is_opening: false,
        memo: format!("Invoice {}", invoice.invoice_number),
        source_type: "invoice".into(),
        source_id: Some(id.into()),
        lines,
    };
    let sources = PgManualJournalCreateSources::capture(c, journal_actor, &draft)
        .await
        .map_err(journal_error)?;
    let rules = embedded_currency_rules().map_err(|_| unavailable())?;
    let collation = FinanceCollation::english().map_err(|_| unavailable())?;
    let mut ports =
        PgJournalPreparation::new(c, journal_actor, authority, clock, &rules, &collation)
            .map_err(journal_error)?;
    let plan = prepare_journal(
        &mut ports,
        journal_actor,
        &draft,
        command,
        JournalStatus::Posted,
    )
    .await
    .map_err(journal_error)?;
    sources.require_current(c).await.map_err(journal_error)?;
    if plan.actor() != journal_actor
        || !matches!(plan.result().status, JournalStatus::Posted)
        || plan.draft().source_type != "invoice"
        || plan.draft().source_id.as_deref() != Some(id)
        || plan.draft().is_opening
        || plan.draft().currency != invoice.currency
        || plan.draft().lines != draft.lines
    {
        return Err(conflict(
            "INVOICE_SOURCE_CHANGED",
            "The supplier bill changed. Refresh it before posting.",
        ));
    }
    let base_total = convert_at_fx(invoice.total_minor, plan.fx()).map_err(|error| {
        BusinessError::new(
            error.kind,
            error.code,
            "The supplier bill journal could not be posted.",
        )
    })?;
    let before: Value = sqlx::query_scalar(
        "SELECT to_jsonb(i) FROM finance_invoices i WHERE company_id=$1 AND id=$2",
    )
    .bind(&actor.company_id)
    .bind(id)
    .fetch_one(&mut *c)
    .await?;
    let mut expected = before.clone();
    let map = expected.as_object_mut().ok_or_else(unavailable)?;
    for (key, value) in [
        ("status", json!("posted")),
        ("journal_id", json!(plan.journal_id())),
        ("fx_rate_micros", json!(plan.fx())),
        ("base_total_minor", json!(base_total)),
        ("base_outstanding_minor", json!(base_total)),
        ("approved_by", json!(journal_actor.user_id.as_str())),
        ("posted_at", json!(plan.now().get())),
        ("version", json!(invoice.version + 1)),
        ("updated_at", json!(plan.now().get())),
    ] {
        map.insert(key.into(), value);
    }
    PgJournalStore::new(
        journal_actor.holding_id.clone(),
        journal_actor.company_id.clone(),
        c,
    )
    .persist(&plan)
    .await
    .map_err(journal_error)?;
    let changed = sqlx::query(
        "UPDATE finance_invoices SET status='posted',journal_id=$3,fx_rate_micros=$4,\
         base_total_minor=$5,base_outstanding_minor=$5,approved_by=$6,posted_at=$7,\
         version=version+1,updated_at=$7 \
         WHERE company_id=$1 AND id=$2 AND kind='payable' AND status='draft' AND version=$8 \
         AND to_jsonb(finance_invoices)=$9",
    )
    .bind(&actor.company_id)
    .bind(id)
    .bind(plan.journal_id())
    .bind(plan.fx())
    .bind(base_total)
    .bind(journal_actor.user_id.as_str())
    .bind(plan.now().get())
    .bind(invoice.version)
    .bind(&before)
    .execute(&mut *c)
    .await?;
    if changed.rows_affected() != 1 {
        return Err(conflict(
            "INVOICE_SOURCE_CHANGED",
            "The supplier bill changed. Refresh it before posting.",
        ));
    }
    let actual: Value = sqlx::query_scalar(
        "SELECT to_jsonb(i) FROM finance_invoices i WHERE company_id=$1 AND id=$2",
    )
    .bind(&actor.company_id)
    .bind(id)
    .fetch_one(&mut *c)
    .await?;
    if actual != expected {
        return Err(conflict(
            "INVOICE_SOURCE_CHANGED",
            "The supplier bill changed. Refresh it before posting.",
        ));
    }
    if let Some(matched) = matched.as_ref() {
        matches
            .stage_post(
                c,
                &actor.company_id,
                &matched.match_id,
                matched.match_version,
                id,
                actor.user_id.as_str(),
                command.as_str(),
                plan.now().get(),
            )
            .await?;
    }
    let money = sqlx::query(
        "INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,\
         transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,\
         used_latest_fallback,captured_at,carried_source_type,carried_source_id,carried_amount_kind) \
         SELECT s.company_id,'finance_invoice',$2,'total',$3::date,s.currency,s.currency_scale,\
         s.amount_units,s.usd_units,s.fx_rate_id,s.used_latest_fallback,$4,s.source_type,\
         s.source_id,s.amount_kind \
         FROM money_transaction_snapshots s \
         WHERE s.company_id=$1 AND s.source_type='finance_invoice' AND s.source_id=$2 \
         AND s.amount_kind=$5 AND s.amount_units=$6::numeric",
    )
    .bind(&actor.company_id)
    .bind(id)
    .bind(&invoice.invoice_date)
    .bind(plan.now().get())
    .bind(format!("draft:v{}", invoice.line_revision))
    .bind(invoice.total_minor.to_string())
    .execute(&mut *c)
    .await?;
    if money.rows_affected() != 1 {
        return Err(conflict(
            "MONEY_SOURCE_REQUIRED",
            "Retain the supplier bill's draft Money evidence before posting it.",
        ));
    }
    let result = SupplierBillPostResult {
        id: id.into(),
        status: "posted".into(),
        journal_id: plan.journal_id().into(),
    };
    receipt(
        c,
        actor,
        &authority_evidence,
        command.as_str(),
        "finance.supplier-bill.post",
        &request_hash,
        id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        invoice.version + 1,
        plan.now().get(),
    )
    .await?;
    if let Some(prepared) = prepared_evidence.as_ref() {
        // Files re-reads the exact bound sets/links after every posting effect;
        // drift aborts the whole serializable unit.
        evidence.require_current(c, prepared).await?;
    }
    Ok(result)
}

/// Payable list/readback. The existing native invoice page already owns the
/// payable list (`kind=payable`); this returns the current detail revision.
pub async fn read_supplier_bill(
    c: &mut PgConnection,
    company: &str,
    id: &str,
) -> Result<SupplierBillDetail, Error> {
    serializable(c).await?;
    if company.is_empty() || company.len() > 200 || id.is_empty() || id.len() > 200 {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "INVOICE_NOT_FOUND",
            "Supplier bill not found in this Company.",
        )
        .into());
    }
    let invoice: Option<Value> = sqlx::query_scalar(
        "SELECT (to_jsonb(i) || jsonb_build_object(\
          'party_code',p.code,'party_name',p.name,\
          'snapshot_usd_units',s.usd_units::text,\
          'snapshot_source_units',s.amount_units::text,\
          'snapshot_rate_id',s.fx_rate_id)) \
         FROM finance_invoices i \
         JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id \
         LEFT JOIN money_transaction_snapshots s ON s.company_id=i.company_id \
           AND s.source_type='finance_invoice' AND s.source_id=i.id AND s.amount_kind='total' \
         WHERE i.id=$1 AND i.company_id=$2 AND i.kind='payable'",
    )
    .bind(id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?;
    let invoice = invoice.ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "INVOICE_NOT_FOUND",
            "Supplier bill not found in this Company.",
        )
    })?;
    let revision = invoice
        .get("line_revision")
        .and_then(Value::as_i64)
        .ok_or_else(unavailable)?;
    let lines = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(l) FROM finance_invoice_line_versions l \
         WHERE l.invoice_id=$1 AND l.company_id=$2 AND l.source_version=$3 \
         ORDER BY l.ordinal,l.id",
    )
    .bind(id)
    .bind(company)
    .bind(revision)
    .fetch_all(&mut *c)
    .await?;
    let line_history = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object('source_version',source_version,'origin_kind',origin_kind,\
          'line_count',COUNT(*),'subtotal_minor',SUM(subtotal_minor),'tax_minor',SUM(tax_minor),\
          'created_at',MIN(created_at)) \
         FROM finance_invoice_line_versions WHERE invoice_id=$1 AND company_id=$2 \
         GROUP BY source_version,origin_kind ORDER BY source_version",
    )
    .bind(id)
    .bind(company)
    .fetch_all(&mut *c)
    .await?;
    let allocations = sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(a) || jsonb_build_object('payment_number',p.payment_number,\
          'status',p.status,'payment_slip_id',p.payment_slip_id) \
         FROM finance_allocations a JOIN finance_payments p \
           ON p.id=a.payment_id AND p.company_id=a.company_id \
         WHERE a.invoice_id=$1 AND a.company_id=$2 ORDER BY a.created_at,a.id",
    )
    .bind(id)
    .bind(company)
    .fetch_all(&mut *c)
    .await?;
    let thai_documents = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object('id',id,'document_number',document_number,\
          'document_kind',document_kind,'edition',edition,'issue_date',issue_date) \
         FROM finance_issued_thai_documents WHERE invoice_id=$1 AND company_id=$2 ORDER BY edition",
    )
    .bind(id)
    .bind(company)
    .fetch_all(&mut *c)
    .await?;
    let journal = match invoice.get("journal_id").and_then(Value::as_str) {
        Some(journal_id) => {
            sqlx::query_scalar::<_, Value>(
                "SELECT jsonb_build_object('id',id,'journal_number',journal_number,\
              'entry_date',entry_date,'status',status,'version',version) \
             FROM finance_journals WHERE id=$1 AND company_id=$2",
            )
            .bind(journal_id)
            .bind(company)
            .fetch_optional(&mut *c)
            .await?
        }
        None => None,
    };
    Ok(SupplierBillDetail {
        invoice,
        allocations,
        lines,
        line_history,
        thai_documents,
        journal,
    })
}
