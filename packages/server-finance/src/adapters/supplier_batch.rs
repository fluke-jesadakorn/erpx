//! Native Finance supplier payment batches and bank instructions.
//!
//! Finance owns the batch/instruction rows, the reserved invoice versions, the
//! export receipt hash, the settlement journal, payment/allocation rows, the
//! bank match, the immutable Money snapshot and the command receipt. Current
//! IAM authority, the Commercial purchase boundary and the Workflow ledger are
//! supplied by the composition in the same serializable transaction; this
//! adapter never widens authority or falls back to a TypeScript writer.
use super::money_snapshot::{MoneySnapshotInput, prepare_money_snapshot};
use super::sales_invoice_create::{
    SalesInvoiceCommandActor, authority_json, fingerprint, receipt,
};
use crate::domain::money::units_to_decimal;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, TimestampMillis,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
use std::future::Future;

type Error = PgTransactionError<BusinessError>;

pub const SUPPLIER_BATCH_EVIDENCE_TARGET_TYPE: &str = "finance_supplier_batch";
pub const SUPPLIER_BATCH_EVIDENCE_BINDING_TYPE: &str = "finance_supplier_payment_confirmation";
pub const SUPPLIER_BATCH_EVIDENCE_PURPOSE_KEY: &str = "payment_proof";
pub const SUPPLIER_BATCH_EVIDENCE_COMPONENT_KIND: &str = "action";
pub const SUPPLIER_BATCH_EVIDENCE_MAX_SETS: usize = 20;

const MAX_ITEMS: usize = 50;

/// Configured cash account, pending purchase return and contractor claim
/// predicates are inlined below. They are the same closed Finance/Commerce
/// source guards the retained TypeScript repository uses; no request value is
/// interpolated.

/// One item in a supplier batch preparation request.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupplierBatchItemInput {
    pub invoice_id: String,
    pub expected_version: i64,
    pub amount: String,
    pub beneficiary_name: String,
    pub beneficiary_account: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupplierBatchCreateInput {
    pub bank_account_id: String,
    pub planned_date: String,
    pub items: Vec<SupplierBatchItemInput>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupplierBatchDecisionInput {
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupplierBatchExportInput {
    pub expected_version: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupplierInstructionResultInput {
    pub expected_version: i64,
    /// `unknown` or `failed`.
    pub outcome: String,
    pub evidence: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupplierInstructionConfirmInput {
    pub expected_version: i64,
    pub bank_transaction_id: String,
    pub bank_version: i64,
    pub period_id: String,
    pub bank_reference: String,
    pub evidence: String,
    #[serde(default)]
    pub attachment_set_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplierBatchResult {
    pub id: String,
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplierBatchExportResult {
    pub id: String,
    pub content: String,
    pub sha256: String,
    pub rows: usize,
    pub confirmation_required: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplierInstructionResult {
    pub id: String,
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplierInstructionConfirmResult {
    pub id: String,
    pub status: String,
    pub version: i64,
    pub payment_id: String,
    pub journal_id: String,
}

#[derive(Clone, Debug)]
struct BankAccount {
    id: String,
    chart_id: String,
    currency: String,
    base_currency: String,
    status: String,
    category: String,
    chart_status: String,
}

#[derive(Clone, Debug)]
struct Batch {
    id: String,
    company_id: String,
    currency: String,
    bank_account_id: String,
    planned_date: String,
    status: String,
    created_by: String,
    approved_by: Option<String>,
    exported_by: Option<String>,
    export_command_id: Option<String>,
    export_hash: Option<String>,
    version: i64,
}

#[derive(Clone, Debug)]
struct Instruction {
    batch_id: String,
    invoice_id: String,
    amount_minor: i64,
    status: String,
    version: i64,
}

#[derive(Clone, Debug)]
struct Invoice {
    id: String,
    party_id: String,
    invoice_number: String,
    credit_account_id: String,
    chart_id: Option<String>,
    outstanding_minor: i64,
    base_outstanding_minor: Option<i64>,
    fx_rate_micros: Option<i64>,
}

#[derive(Clone, Debug)]
struct BankTransaction {
    id: String,
    transaction_date: String,
    amount_minor: i64,
    currency: String,
    bank_account: String,
    match_status: String,
    version: i64,
}

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::invalid(code, message).into()
}

fn conflict(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
}

fn forbidden(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Forbidden, code, message).into()
}

fn not_found(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::NotFound, code, message).into()
}

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_SUPPLIER_BATCH_UNAVAILABLE",
        "The supplier batch could not be prepared.",
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

/// Ordered writer leases for batch/instruction mutations. The composition
/// takes the Commercial/Workflow/IAM fences around this call.
pub async fn lock_supplier_batch_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE finance_supplier_batches,finance_supplier_instructions,finance_invoices,\
         finance_journals,finance_journal_lines,finance_payments,finance_allocations,\
         finance_bank_transactions,finance_bank_statements,finance_periods,finance_charts,\
         finance_accounts,finance_parties IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Read lease for list/workspace/export-recovery reads.
pub async fn lock_supplier_batch_read_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE finance_supplier_batches,finance_supplier_instructions IN SHARE MODE",
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Files evidence for the exact confirmation unit. The composition binds the
/// shared Records/Files port; Finance never writes file state itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupplierInstructionEvidenceRequest {
    pub set_ids: Vec<String>,
    pub target_id: String,
    pub expected_target_version: i64,
    pub command_id: String,
    pub binding_type: String,
    pub binding_id: String,
    pub purpose_key: String,
    pub component_kind: String,
    pub component_ref: String,
}

pub trait PgSupplierInstructionEvidence: Send {
    type Prepared: Send;
    fn prepare(
        &mut self,
        c: &mut PgConnection,
        request: SupplierInstructionEvidenceRequest,
    ) -> impl Future<Output = Result<Self::Prepared, Error>> + Send;
    fn require_current(
        &mut self,
        c: &mut PgConnection,
        prepared: &Self::Prepared,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

pub struct UnmountedSupplierInstructionEvidence;
impl PgSupplierInstructionEvidence for UnmountedSupplierInstructionEvidence {
    type Prepared = ();
    fn prepare(
        &mut self,
        _c: &mut PgConnection,
        request: SupplierInstructionEvidenceRequest,
    ) -> impl Future<Output = Result<Self::Prepared, Error>> + Send {
        async move {
            if request.set_ids.is_empty() {
                Ok(())
            } else {
                Err(invalid(
                    "SUPPLIER_ATTACHMENT_PURPOSE",
                    "Supplier payment evidence is unavailable in this assembly.",
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

/// Collaboration notification staging for each lifecycle transition. The
/// composition supplies the real recipient producer; the default is a no-op
/// because the outbox event still records the owner transition.
pub trait PgSupplierBatchNotifications: Send {
    fn stage(
        &mut self,
        c: &mut PgConnection,
        command_id: &str,
        batch_id: &str,
        message: &str,
        actor_id: &str,
        now: i64,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

pub struct NoSupplierBatchNotifications;
impl PgSupplierBatchNotifications for NoSupplierBatchNotifications {
    fn stage(
        &mut self,
        _c: &mut PgConnection,
        _command_id: &str,
        _batch_id: &str,
        _message: &str,
        _actor_id: &str,
        _now: i64,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move { Ok(()) }
    }
}

fn decimal_to_minor(value: &str) -> Result<i64, Error> {
    let value = value.trim();
    if value.is_empty() || value.len() > 32 {
        return Err(invalid("INVALID_AMOUNT", "Use a positive payment amount."));
    }
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 2
    {
        return Err(invalid("INVALID_AMOUNT", "Use a positive payment amount."));
    }
    let whole: i64 = whole
        .parse()
        .map_err(|_| invalid("INVALID_AMOUNT", "Use a positive payment amount."))?;
    let fraction: i64 = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().unwrap_or(0) * 10,
        _ => fraction.parse().unwrap_or(0),
    };
    let amount = whole
        .checked_mul(100)
        .and_then(|value| value.checked_add(fraction))
        .ok_or_else(|| invalid("INVALID_AMOUNT", "Use a positive payment amount."))?;
    if amount <= 0 {
        return Err(invalid("INVALID_AMOUNT", "Use a positive payment amount."));
    }
    Ok(amount)
}

fn minor_decimal(units: i64) -> String {
    units_to_decimal(&num_bigint::BigInt::from(units), 2).unwrap_or_else(|_| "0.00".into())
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

fn trimmed(value: &str, min: usize, max: usize) -> bool {
    let value = value.trim();
    value.encode_utf16().count() >= min && value.encode_utf16().count() <= max
}

fn validate_create(input: &SupplierBatchCreateInput) -> Result<Vec<i64>, Error> {
    if input.bank_account_id.trim().is_empty() || !valid_date(&input.planned_date) {
        return Err(invalid(
            "INVALID_SUPPLIER_BATCH",
            "Select an active bank ledger and planned date.",
        ));
    }
    if input.items.is_empty() || input.items.len() > MAX_ITEMS {
        return Err(invalid(
            "INVALID_SUPPLIER_BATCH",
            "Select between one and fifty supplier invoices.",
        ));
    }
    let mut ids = std::collections::HashSet::new();
    let mut amounts = Vec::with_capacity(input.items.len());
    for item in &input.items {
        if item.invoice_id.trim().is_empty() || item.expected_version <= 0 {
            return Err(invalid(
                "INVALID_SUPPLIER_BATCH",
                "Review each invoice at its current version.",
            ));
        }
        if !trimmed(&item.beneficiary_name, 2, 160) || !trimmed(&item.beneficiary_account, 3, 160) {
            return Err(invalid(
                "INVALID_SUPPLIER_BATCH",
                "Review the beneficiary name and account.",
            ));
        }
        if !ids.insert(item.invoice_id.clone()) {
            return Err(invalid(
                "DUPLICATE_INVOICE",
                "Select each invoice once.",
            ));
        }
        amounts.push(decimal_to_minor(&item.amount)?);
    }
    Ok(amounts)
}

fn csv_cell(value: &str) -> String {
    let escaped = value.replace('"', "\"\"");
    let escaped = match escaped.chars().next() {
        Some(first) if matches!(first, '=' | '+' | '@' | '-' | '\t' | '\r') => {
            format!("'{escaped}")
        }
        _ => escaped,
    };
    format!("\"{escaped}\"")
}

/// The exact retained export CSV: header + one row per instruction, CRLF.
pub fn supplier_export_content(batch: &Value, items: &[Value]) -> String {
    let header = [
        "instruction_id",
        "invoice",
        "date",
        "currency",
        "amount",
        "company_bank_ledger",
        "beneficiary",
        "beneficiary_account",
    ]
    .iter()
    .map(|value| csv_cell(value))
    .collect::<Vec<_>>()
    .join(",");
    let mut lines = vec![header];
    for item in items {
        let amount = item
            .get("amount_minor")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let row = [
            item.get("id").and_then(Value::as_str).unwrap_or("").to_owned(),
            item.get("invoice_number")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            batch
                .get("planned_date")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            batch
                .get("currency")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            minor_decimal(amount),
            batch
                .get("bank_account_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            item.get("beneficiary_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            item.get("beneficiary_account")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
        ];
        lines.push(row.iter().map(|value| csv_cell(value)).collect::<Vec<_>>().join(","));
    }
    lines.join("\r\n")
}

fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

async fn batch(c: &mut PgConnection, company: &str, id: &str) -> Result<Batch, Error> {
    let row = sqlx::query(
        "SELECT id,company_id,currency,bank_account_id,planned_date,status,created_by,\
         approved_by,exported_by,export_command_id,export_hash,version \
         FROM finance_supplier_batches WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| not_found("BATCH_NOT_FOUND", "Supplier batch not found."))?;
    Ok(Batch {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        currency: row.try_get("currency")?,
        bank_account_id: row.try_get("bank_account_id")?,
        planned_date: row.try_get("planned_date")?,
        status: row.try_get("status")?,
        created_by: row.try_get("created_by")?,
        approved_by: row.try_get("approved_by")?,
        exported_by: row.try_get("exported_by")?,
        export_command_id: row.try_get("export_command_id")?,
        export_hash: row.try_get("export_hash")?,
        version: row.try_get("version")?,
    })
}

async fn instruction(
    c: &mut PgConnection,
    company: &str,
    id: &str,
) -> Result<(Instruction, Batch), Error> {
    let row = sqlx::query(
        "SELECT i.batch_id,i.invoice_id,i.amount_minor,i.status,i.version \
         FROM finance_supplier_instructions i JOIN finance_supplier_batches b ON b.id=i.batch_id \
         WHERE i.id=$1 AND b.company_id=$2",
    )
    .bind(id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| not_found("INSTRUCTION_NOT_FOUND", "Supplier instruction not found."))?;
    let item = Instruction {
        batch_id: row.try_get("batch_id")?,
        invoice_id: row.try_get("invoice_id")?,
        amount_minor: row.try_get("amount_minor")?,
        status: row.try_get("status")?,
        version: row.try_get("version")?,
    };
    let owner = batch(c, company, &item.batch_id).await?;
    Ok((item, owner))
}

async fn bank_account(
    c: &mut PgConnection,
    company: &str,
    id: &str,
) -> Result<BankAccount, Error> {
    let row = sqlx::query(
        "SELECT a.id,a.chart_id,a.currency,a.status,a.category,c.base_currency,c.status chart_status \
         FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id \
         WHERE a.id=$1 AND a.company_id=$2 AND a.category='asset' AND a.status='active' \
         AND c.status='active'",
    )
    .bind(id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| {
        invalid(
            "BANK_ACCOUNT_INVALID",
            "Select an active bank ledger in the functional currency.",
        )
    })?;
    Ok(BankAccount {
        id: row.try_get("id")?,
        chart_id: row.try_get("chart_id")?,
        currency: row.try_get("currency")?,
        base_currency: row.try_get("base_currency")?,
        status: row.try_get("status")?,
        category: row.try_get("category")?,
        chart_status: row.try_get("chart_status")?,
    })
}

fn assert_functional_bank(account: &BankAccount) -> Result<(), Error> {
    if account.status != "active"
        || account.category != "asset"
        || account.chart_status != "active"
        || account.currency != account.base_currency
    {
        return Err(invalid(
            "BANK_ACCOUNT_INVALID",
            "Select an active bank ledger in the functional currency.",
        ));
    }
    Ok(())
}

async fn assert_open_period(
    c: &mut PgConnection,
    company: &str,
    date: &str,
) -> Result<(), Error> {
    let open: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_periods WHERE company_id=$1 AND state='open' \
         AND start_date<=$2 AND end_date>=$2)",
    )
    .bind(company)
    .bind(date)
    .fetch_one(&mut *c)
    .await?;
    if !open {
        return Err(conflict(
            "OPEN_PERIOD_REQUIRED",
            "Select an open Finance period that contains the payment date.",
        ));
    }
    Ok(())
}

fn same_person(actor: &SalesInvoiceCommandActor, id: &str) -> bool {
    actor.user_id == id
}

fn assert_company_treasury(actor: &SalesInvoiceCommandActor) -> Result<(), Error> {
    if actor.branch_id.is_some() || actor.department_id.is_some() {
        return Err(forbidden(
            "COMPANY_TREASURY_REQUIRED",
            "Use a Company finance assignment for supplier bank batches.",
        ));
    }
    Ok(())
}

async fn replay(
    c: &mut PgConnection,
    command: &CommandKey,
    request_hash: &str,
) -> Result<Option<Value>, Error> {
    let row = sqlx::query(
        "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1",
    )
    .bind(command.as_str())
    .fetch_optional(&mut *c)
    .await?;
    match row {
        None => Ok(None),
        Some(row) => {
            let stored: String = row.try_get("request_hash")?;
            if stored != request_hash {
                return Err(conflict(
                    "IDEMPOTENCY_KEY_REUSED",
                    "This command key was already used for a different request.",
                ));
            }
            let result: String = row.try_get("result_json")?;
            Ok(Some(
                serde_json::from_str(&result).map_err(|_| unavailable())?,
            ))
        }
    }
}

/// Prepare one draft supplier batch, reserving each source invoice.
#[allow(clippy::too_many_arguments)]
pub async fn create_supplier_batch<N: PgSupplierBatchNotifications>(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    input: &SupplierBatchCreateInput,
    source_only: bool,
    notifications: &mut N,
    now: i64,
) -> Result<SupplierBatchResult, Error> {
    serializable(c).await?;
    assert_company_treasury(actor)?;
    let amounts = validate_create(input)?;
    let id = format!("supplier-batch-{}", command.as_str());
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let request_hash = fingerprint(actor, "finance.supplier_batch.create", &id, &input_json);
    let authority = authority_json(actor);
    if let Some(result) = replay(c, command, &request_hash).await? {
        return serde_json::from_value(result).map_err(|_| unavailable());
    }
    let account = bank_account(c, &actor.company_id, &input.bank_account_id).await?;
    assert_functional_bank(&account)?;
    assert_open_period(c, &actor.company_id, &input.planned_date).await?;

    sqlx::query(
        "INSERT INTO finance_supplier_batches(id,company_id,currency,bank_account_id,planned_date,\
         created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$7)",
    )
    .bind(&id)
    .bind(&actor.company_id)
    .bind(&account.currency)
    .bind(&account.id)
    .bind(&input.planned_date)
    .bind(&actor.user_id)
    .bind(now)
    .execute(&mut *c)
    .await?;

    for (index, (item, amount)) in input.items.iter().zip(amounts.iter()).enumerate() {
        // Touch the invoice version to serialize every reservation and source
        // mutation before the instruction becomes visible.
        let allowed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM finance_invoices i \
             JOIN finance_journals j ON j.id=i.journal_id JOIN finance_parties p ON p.id=i.party_id \
             JOIN finance_charts chart ON chart.id=j.chart_id \
             WHERE i.id=$1 AND i.company_id=$2 AND i.version=$3 AND i.kind='payable' \
             AND i.source_type IN ('standalone','commerce_fulfillment') \
             AND i.status IN ('posted','partially_paid') \
             AND j.status='posted' AND j.chart_id=$4 AND chart.base_currency=$5 AND i.currency=$5 \
             AND p.status='active' AND p.kind='supplier' AND i.invoice_date<=$6 \
             AND i.credit_account_id<>$7 \
             AND NOT EXISTS(SELECT 1 FROM commerce_returns r WHERE r.invoice_id=i.id AND r.status IN ('requested','approved')) \
             AND NOT EXISTS(SELECT 1 FROM hr_payment_batches h WHERE h.company_id=i.company_id AND h.source_type='contractor' AND h.source_id=i.id AND h.status<>'cancelled') \
             AND i.outstanding_minor-COALESCE((SELECT SUM(s.amount_minor) \
             FROM finance_supplier_instructions s WHERE s.invoice_id=i.id \
             AND s.status IN ('reserved','exported','unknown')),0)>=$8)",
        )
        .bind(&item.invoice_id)
        .bind(&actor.company_id)
        .bind(item.expected_version)
        .bind(&account.chart_id)
        .bind(&account.currency)
        .bind(&input.planned_date)
        .bind(&account.id)
        .bind(amount)
        .fetch_one(&mut *c)
        .await?;
        if !allowed {
            return Err(invalid(
                "SUPPLIER_INVOICE_CHANGED",
                "Review each supplier invoice, its balance and its funding source.",
            ));
        }
        sqlx::query(
            "INSERT INTO finance_supplier_instructions(id,batch_id,invoice_id,invoice_version,\
             amount_minor,beneficiary_name,beneficiary_account,created_at,updated_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$8)",
        )
        .bind(format!("{id}-{index}"))
        .bind(&id)
        .bind(&item.invoice_id)
        .bind(item.expected_version + 1)
        .bind(amount)
        .bind(item.beneficiary_name.trim())
        .bind(item.beneficiary_account.trim())
        .bind(now)
        .execute(&mut *c)
        .await?;
        sqlx::query(
            "UPDATE finance_invoices SET version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3",
        )
        .bind(now)
        .bind(&item.invoice_id)
        .bind(&actor.company_id)
        .execute(&mut *c)
        .await?;
    }

    let owned: bool = sqlx::query_scalar(
        "SELECT (NOT $2 OR (EXISTS(SELECT 1 FROM finance_supplier_instructions owned_item WHERE owned_item.batch_id=$1) \
         AND NOT EXISTS(SELECT 1 FROM finance_supplier_instructions owned_item WHERE owned_item.batch_id=$1 \
         AND NOT EXISTS(SELECT 1 FROM (SELECT o.company_id,f.invoice_id id FROM commerce_orders o \
         JOIN commerce_fulfillments f ON f.order_id=o.id JOIN finance_invoices i ON i.id=f.invoice_id \
         AND i.company_id=o.company_id AND i.kind='payable' WHERE o.kind='purchase') owned_source \
         WHERE owned_source.id=owned_item.invoice_id AND owned_source.company_id=$3))))",
    )
    .bind(&id)
    .bind(source_only)
    .bind(&actor.company_id)
    .fetch_one(&mut *c)
    .await?;
    if !owned {
        return Err(forbidden(
            "PACKAGE_REQUIRED",
            "This supplier batch includes an invoice outside the purchased Supply source.",
        ));
    }

    let result = SupplierBatchResult {
        id: id.clone(),
        status: "draft".into(),
        version: 1,
    };
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        "finance.supplier_batch.create",
        &request_hash,
        &id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        1,
        now,
    )
    .await?;
    notifications
        .stage(
            c,
            command.as_str(),
            &id,
            "Supplier batch prepared: independent review required",
            &actor.user_id,
            now,
        )
        .await?;
    Ok(result)
}

/// Approve or cancel a draft batch. `can_approve` is the trusted Finance
/// approver capability supplied by the composition.
#[allow(clippy::too_many_arguments)]
pub async fn decide_supplier_batch<N: PgSupplierBatchNotifications>(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    id: &str,
    input: &SupplierBatchDecisionInput,
    decision: &str,
    can_approve: bool,
    notifications: &mut N,
    now: i64,
) -> Result<SupplierBatchResult, Error> {
    serializable(c).await?;
    assert_company_treasury(actor)?;
    if input.expected_version <= 0 || !trimmed(&input.reason, 3, 1000) {
        return Err(invalid(
            "INVALID_SUPPLIER_DECISION",
            "Record a review reason between three and one thousand characters.",
        ));
    }
    let operation = format!("finance.supplier_batch.{decision}");
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let request_hash = fingerprint(actor, &operation, id, &input_json);
    let authority = authority_json(actor);
    if let Some(result) = replay(c, command, &request_hash).await? {
        return serde_json::from_value(result).map_err(|_| unavailable());
    }
    let batch = batch(c, &actor.company_id, id).await?;
    if batch.version != input.expected_version {
        return Err(conflict(
            "VERSION_CONFLICT",
            "The supplier batch changed. Refresh and review again.",
        ));
    }
    match decision {
        "approve" => {
            if same_person(actor, &batch.created_by) {
                return Err(forbidden(
                    "INDEPENDENT_APPROVER_REQUIRED",
                    "A different person must review the source, amount and beneficiary.",
                ));
            }
            if batch.status != "draft" {
                return Err(conflict(
                    "BATCH_NOT_EDITABLE",
                    "Exported batches require individual bank-result review.",
                ));
            }
        }
        "cancel" => {
            if !same_person(actor, &batch.created_by) && !can_approve {
                return Err(forbidden(
                    "CANCEL_FORBIDDEN",
                    "The preparer or Finance approver must cancel this batch.",
                ));
            }
            if batch.status != "draft" && batch.status != "approved" {
                return Err(conflict(
                    "BATCH_NOT_EDITABLE",
                    "Exported batches require individual bank-result review.",
                ));
            }
        }
        _ => {
            return Err(invalid(
                "INVALID_SUPPLIER_DECISION",
                "Select approve or cancel.",
            ));
        }
    }
    let status = if decision == "approve" {
        "approved"
    } else {
        "cancelled"
    };
    if decision == "approve" {
        sqlx::query(
            "UPDATE finance_supplier_batches SET status='approved',approved_by=$1,\
             approval_reason=$2,version=version+1,updated_at=$3 WHERE id=$4 AND company_id=$5",
        )
        .bind(&actor.user_id)
        .bind(input.reason.trim())
        .bind(now)
        .bind(id)
        .bind(&actor.company_id)
        .execute(&mut *c)
        .await?;
    } else {
        sqlx::query(
            "UPDATE finance_supplier_batches SET status='cancelled',version=version+1,updated_at=$1 \
             WHERE id=$2 AND company_id=$3",
        )
        .bind(now)
        .bind(id)
        .bind(&actor.company_id)
        .execute(&mut *c)
        .await?;
        sqlx::query(
            "UPDATE finance_supplier_instructions SET status='cancelled',evidence=$1,resolved_by=$2,\
             version=version+1,updated_at=$3 WHERE batch_id=$4 AND status='reserved'",
        )
        .bind(input.reason.trim())
        .bind(&actor.user_id)
        .bind(now)
        .bind(id)
        .execute(&mut *c)
        .await?;
    }
    let result = SupplierBatchResult {
        id: id.to_owned(),
        status: status.into(),
        version: input.expected_version + 1,
    };
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        &operation,
        &request_hash,
        id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        input.expected_version + 1,
        now,
    )
    .await?;
    let message = if decision == "approve" {
        "Supplier batch approved: independent Treasury export required"
    } else {
        "Supplier batch cancelled: review source invoices before preparing again"
    };
    notifications
        .stage(c, command.as_str(), id, message, &actor.user_id, now)
        .await?;
    Ok(result)
}

async fn export_items(c: &mut PgConnection, id: &str) -> Result<Vec<Value>, Error> {
    let rows = sqlx::query(
        "SELECT to_jsonb(s)::text item,i.invoice_number FROM finance_supplier_instructions s \
         JOIN finance_invoices i ON i.id=s.invoice_id WHERE s.batch_id=$1 ORDER BY s.id",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    rows.into_iter()
        .map(|row| {
            let text: String = row.try_get("item")?;
            let invoice_number: String = row.try_get("invoice_number")?;
            let mut value: Value =
                serde_json::from_str(&text).map_err(|_| unavailable())?;
            if let Value::Object(ref mut object) = value {
                object.insert("invoice_number".into(), Value::String(invoice_number));
            }
            Ok(value)
        })
        .collect()
}

/// Export the approved batch as the exact bank CSV, retaining its SHA-256 in
/// the command receipt and batch row.
#[allow(clippy::too_many_arguments)]
pub async fn export_supplier_batch<N: PgSupplierBatchNotifications>(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    id: &str,
    input: &SupplierBatchExportInput,
    notifications: &mut N,
    now: i64,
) -> Result<SupplierBatchExportResult, Error> {
    serializable(c).await?;
    assert_company_treasury(actor)?;
    if input.expected_version <= 0 {
        return Err(invalid(
            "INVALID_SUPPLIER_EXPORT",
            "Review the current supplier batch version.",
        ));
    }
    let operation = "finance.supplier_batch.export";
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let request_hash = fingerprint(actor, operation, id, &input_json);
    let authority = authority_json(actor);
    if let Some(result) = replay(c, command, &request_hash).await? {
        return serde_json::from_value(result).map_err(|_| unavailable());
    }
    let batch = batch(c, &actor.company_id, id).await?;
    if batch.version != input.expected_version {
        return Err(conflict(
            "VERSION_CONFLICT",
            "The supplier batch changed. Refresh and review again.",
        ));
    }
    if same_person(actor, &batch.created_by)
        || batch
            .approved_by
            .as_deref()
            .is_some_and(|value| same_person(actor, value))
    {
        return Err(forbidden(
            "INDEPENDENT_PAYER_REQUIRED",
            "A third person must export and confirm this approved batch.",
        ));
    }
    if batch.status != "approved" {
        return Err(conflict(
            "BATCH_NOT_APPROVED",
            "Export an independently approved batch.",
        ));
    }
    let account = bank_account(c, &actor.company_id, &batch.bank_account_id).await?;
    if account.currency != batch.currency || account.base_currency != batch.currency {
        return Err(invalid(
            "BANK_ACCOUNT_INVALID",
            "Select an active bank ledger in the functional currency.",
        ));
    }
    assert_open_period(c, &actor.company_id, &batch.planned_date).await?;
    let balances: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS(SELECT 1 FROM finance_supplier_instructions s \
         JOIN finance_invoices i ON i.id=s.invoice_id JOIN finance_parties p ON p.id=i.party_id \
         JOIN finance_journals j ON j.id=i.journal_id WHERE s.batch_id=$1 \
         AND (s.status<>'reserved' OR i.status NOT IN ('posted','partially_paid') \
         OR p.status<>'active' OR j.status<>'posted' \
         OR i.outstanding_minor<(SELECT SUM(r.amount_minor) FROM finance_supplier_instructions r \
         WHERE r.invoice_id=i.id AND r.status IN ('reserved','exported','unknown')) \
         OR EXISTS(SELECT 1 FROM commerce_returns cr WHERE cr.invoice_id=i.id AND cr.status IN ('requested','approved'))))",
    )
    .bind(id)
    .fetch_one(&mut *c)
    .await?;
    if !balances {
        return Err(conflict(
            "SUPPLIER_SOURCE_CHANGED",
            "Review every reserved invoice balance before exporting.",
        ));
    }
    let items = export_items(c, id).await?;
    let batch_json = json!({"planned_date": batch.planned_date, "currency": batch.currency, "bank_account_id": batch.bank_account_id});
    let content = supplier_export_content(&batch_json, &items);
    let sha256 = sha256_hex(&content);
    sqlx::query(
        "UPDATE finance_supplier_batches SET status='exported',exported_by=$1,\
         export_command_id=$2,export_hash=$3,version=version+1,updated_at=$4 \
         WHERE id=$5 AND company_id=$6",
    )
    .bind(&actor.user_id)
    .bind(command.as_str())
    .bind(&sha256)
    .bind(now)
    .bind(id)
    .bind(&actor.company_id)
    .execute(&mut *c)
    .await?;
    sqlx::query(
        "UPDATE finance_supplier_instructions SET status='exported',version=version+1,updated_at=$1 \
         WHERE batch_id=$2 AND status='reserved'",
    )
    .bind(now)
    .bind(id)
    .execute(&mut *c)
    .await?;
    let result = SupplierBatchExportResult {
        id: id.to_owned(),
        content,
        sha256,
        rows: items.len(),
        confirmation_required: true,
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
    notifications
        .stage(
            c,
            command.as_str(),
            id,
            "Supplier batch exported: confirm each bank result before settlement",
            &actor.user_id,
            now,
        )
        .await?;
    Ok(result)
}

/// Recover the retained export content from its original command receipt.
pub async fn recover_supplier_export(
    c: &mut PgConnection,
    company: &str,
    id: &str,
) -> Result<SupplierBatchExportResult, Error> {
    let batch = batch(c, company, id).await?;
    let Some(command_id) = batch.export_command_id else {
        return Err(not_found(
            "EXPORT_NOT_FOUND",
            "No exported file exists.",
        ));
    };
    let result: Option<String> =
        sqlx::query_scalar("SELECT result_json FROM command_receipts WHERE command_id=$1")
            .bind(&command_id)
            .fetch_optional(&mut *c)
            .await?;
    let result = result.ok_or_else(|| {
        not_found("EXPORT_NOT_FOUND", "No exported file exists.")
    })?;
    serde_json::from_str(&result).map_err(|_| unavailable())
}

/// Record one bank result (`unknown` or `failed`) for an exported instruction.
#[allow(clippy::too_many_arguments)]
pub async fn record_supplier_instruction_result<N: PgSupplierBatchNotifications>(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    id: &str,
    input: &SupplierInstructionResultInput,
    can_approve: bool,
    notifications: &mut N,
    now: i64,
) -> Result<SupplierInstructionResult, Error> {
    serializable(c).await?;
    assert_company_treasury(actor)?;
    if input.expected_version <= 0
        || !matches!(input.outcome.as_str(), "unknown" | "failed")
        || !trimmed(&input.evidence, 3, 1000)
    {
        return Err(invalid(
            "INVALID_SUPPLIER_RESULT",
            "Record unknown or failed with a review note.",
        ));
    }
    let operation = "finance.supplier_batch.result";
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let request_hash = fingerprint(actor, operation, id, &input_json);
    let authority = authority_json(actor);
    if let Some(result) = replay(c, command, &request_hash).await? {
        return serde_json::from_value(result).map_err(|_| unavailable());
    }
    let (item, batch) = instruction(c, &actor.company_id, id).await?;
    if item.version != input.expected_version {
        return Err(conflict(
            "VERSION_CONFLICT",
            "The supplier instruction changed. Refresh and review again.",
        ));
    }
    if !matches!(item.status.as_str(), "exported" | "unknown") {
        return Err(conflict(
            "INSTRUCTION_NOT_PENDING",
            "Only an unresolved exported instruction can receive a bank result.",
        ));
    }
    if input.outcome == "unknown" {
        if same_person(actor, &batch.created_by)
            || batch
                .approved_by
                .as_deref()
                .is_some_and(|value| same_person(actor, value))
        {
            return Err(forbidden(
                "INDEPENDENT_PAYER_REQUIRED",
                "A third person must export and confirm this approved batch.",
            ));
        }
    } else {
        if !can_approve {
            return Err(forbidden(
                "INDEPENDENT_REVIEW_REQUIRED",
                "Another Finance approver must verify that the bank did not pay.",
            ));
        }
        if batch
            .exported_by
            .as_deref()
            .is_some_and(|value| same_person(actor, value))
        {
            return Err(forbidden(
                "INDEPENDENT_REVIEW_REQUIRED",
                "Another Finance approver must verify that the bank did not pay.",
            ));
        }
    }
    sqlx::query(
        "UPDATE finance_supplier_instructions SET status=$1,evidence=$2,resolved_by=$3,\
         version=version+1,updated_at=$4 WHERE id=$5",
    )
    .bind(&input.outcome)
    .bind(input.evidence.trim())
    .bind(&actor.user_id)
    .bind(now)
    .bind(id)
    .execute(&mut *c)
    .await?;
    sqlx::query(
        "UPDATE finance_invoices SET version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3",
    )
    .bind(now)
    .bind(&item.invoice_id)
    .bind(&actor.company_id)
    .execute(&mut *c)
    .await?;
    sqlx::query(
        "UPDATE finance_supplier_batches SET version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3",
    )
    .bind(now)
    .bind(&batch.id)
    .bind(&actor.company_id)
    .execute(&mut *c)
    .await?;
    let result = SupplierInstructionResult {
        id: id.to_owned(),
        status: input.outcome.clone(),
        version: input.expected_version + 1,
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
    notifications
        .stage(
            c,
            command.as_str(),
            &batch.id,
            "Supplier bank result updated: review remaining instructions",
            &actor.user_id,
            now,
        )
        .await?;
    Ok(result)
}

struct FixedClock(i64);
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(self.0)
    }
}

async fn invoice_row(
    c: &mut PgConnection,
    company: &str,
    id: &str,
) -> Result<Invoice, Error> {
    let row = sqlx::query(
        "SELECT i.id,i.party_id,i.invoice_number,i.credit_account_id,j.chart_id,\
         i.outstanding_minor,i.base_outstanding_minor,i.fx_rate_micros \
         FROM finance_invoices i LEFT JOIN finance_journals j ON j.id=i.journal_id \
         WHERE i.id=$1 AND i.company_id=$2",
    )
    .bind(id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| not_found("INVOICE_NOT_FOUND", "Source invoice is unavailable."))?;
    Ok(Invoice {
        id: row.try_get("id")?,
        party_id: row.try_get("party_id")?,
        invoice_number: row.try_get("invoice_number")?,
        credit_account_id: row.try_get("credit_account_id")?,
        chart_id: row.try_get("chart_id")?,
        outstanding_minor: row.try_get("outstanding_minor")?,
        base_outstanding_minor: row.try_get("base_outstanding_minor")?,
        fx_rate_micros: row.try_get("fx_rate_micros")?,
    })
}

async fn bank_transaction(
    c: &mut PgConnection,
    company: &str,
    id: &str,
) -> Result<BankTransaction, Error> {
    let row = sqlx::query(
        "SELECT t.id,t.transaction_date,t.amount_minor,t.currency,s.bank_account,\
         t.match_status,t.version FROM finance_bank_transactions t \
         JOIN finance_bank_statements s ON s.id=t.statement_id \
         WHERE t.id=$1 AND t.company_id=$2",
    )
    .bind(id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| not_found("BANK_TRANSACTION_NOT_FOUND", "Select the exact bank debit."))?;
    Ok(BankTransaction {
        id: row.try_get("id")?,
        transaction_date: row.try_get("transaction_date")?,
        amount_minor: row.try_get("amount_minor")?,
        currency: row.try_get("currency")?,
        bank_account: row.try_get("bank_account")?,
        match_status: row.try_get("match_status")?,
        version: row.try_get("version")?,
    })
}

#[allow(clippy::too_many_arguments)]
async fn insert_payment_journal(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    invoice: &Invoice,
    batch: &Batch,
    bank: &BankTransaction,
    amount: i64,
    input: &SupplierInstructionConfirmInput,
    instruction_id: &str,
    key: &str,
    now: i64,
) -> Result<(String, String), Error> {
    let chart_id = invoice
        .chart_id
        .as_deref()
        .ok_or_else(|| conflict("SUPPLIER_CHART_REQUIRED", "The source invoice has no posted chart."))?;
    let period: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_periods WHERE id=$1 AND company_id=$2 \
         AND state='open' AND start_date<=$3 AND end_date>=$3)",
    )
    .bind(&input.period_id)
    .bind(&actor.company_id)
    .bind(&bank.transaction_date)
    .fetch_one(&mut *c)
    .await?;
    if !period {
        return Err(conflict(
            "OPEN_PERIOD_REQUIRED",
            "Select an open Finance period that contains the payment date.",
        ));
    }
    let chart: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_charts WHERE id=$1 AND company_id=$2 \
         AND status='active' AND base_currency=$3)",
    )
    .bind(chart_id)
    .bind(&actor.company_id)
    .bind(&batch.currency)
    .fetch_one(&mut *c)
    .await?;
    if !chart {
        return Err(conflict(
            "SUPPLIER_CHART_CHANGED",
            "The posted invoice chart or functional currency changed.",
        ));
    }
    for account in [&invoice.credit_account_id, &batch.bank_account_id] {
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM finance_accounts WHERE id=$1 AND company_id=$2 \
             AND chart_id=$3 AND status='active')",
        )
        .bind(account)
        .bind(&actor.company_id)
        .bind(chart_id)
        .fetch_one(&mut *c)
        .await?;
        if !active {
            return Err(conflict(
                "SUPPLIER_ACCOUNT_CHANGED",
                "A supplier or bank ledger changed before confirmation.",
            ));
        }
    }
    let journal_id = format!("journal-{key}");
    let number = format!("SB-{}", key);
    let memo = format!(
        "Supplier payment {}: {}",
        invoice.invoice_number,
        input.bank_reference.trim()
    );
    sqlx::query(
        "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,\
         currency,fx_rate_micros,source_type,source_id,memo,is_opening,status,created_by,posted_by,\
         posted_at,created_at,updated_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,1000000,'supplier_batch',$8,$9,0,'posted',$10,$10,$11,$11,$11)",
    )
    .bind(&journal_id)
    .bind(&actor.company_id)
    .bind(chart_id)
    .bind(&input.period_id)
    .bind(&number)
    .bind(&bank.transaction_date)
    .bind(&batch.currency)
    .bind(instruction_id)
    .bind(&memo)
    .bind(&actor.user_id)
    .bind(now)
    .execute(&mut *c)
    .await?;
    for (ordinal, account, debit, credit) in [
        (0_i64, invoice.credit_account_id.as_str(), amount, 0_i64),
        (1, batch.bank_account_id.as_str(), 0, amount),
    ] {
        sqlx::query(
            "INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,\
             credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) \
             VALUES($1,$2,$3,'',$4,$5,$4,$5,'{}',$6)",
        )
        .bind(format!("{journal_id}-{ordinal}"))
        .bind(&journal_id)
        .bind(account)
        .bind(debit)
        .bind(credit)
        .bind(ordinal)
        .execute(&mut *c)
        .await?;
    }
    Ok((journal_id, number))
}

/// Confirm one exported instruction against its exact bank debit and post the
/// supplier settlement. A different Treasury payer must sign the confirmation.
#[allow(clippy::too_many_arguments)]
pub async fn confirm_supplier_instruction<
    E: PgSupplierInstructionEvidence,
    N: PgSupplierBatchNotifications,
>(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    id: &str,
    input: &SupplierInstructionConfirmInput,
    evidence: &mut E,
    notifications: &mut N,
    now: i64,
) -> Result<SupplierInstructionConfirmResult, Error> {
    serializable(c).await?;
    assert_company_treasury(actor)?;
    super::money_snapshot::lock_money_snapshot_sources(c).await?;
    if input.expected_version <= 0
        || input.bank_version <= 0
        || input.bank_transaction_id.trim().is_empty()
        || input.period_id.trim().is_empty()
        || !trimmed(&input.bank_reference, 2, 160)
        || !trimmed(&input.evidence, 3, 1000)
        || input.attachment_set_ids.len() > SUPPLIER_BATCH_EVIDENCE_MAX_SETS
    {
        return Err(invalid(
            "INVALID_SUPPLIER_CONFIRMATION",
            "Review the bank reference, period and payment evidence.",
        ));
    }
    let operation = "finance.supplier_batch.confirm";
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let request_hash = fingerprint(actor, operation, id, &input_json);
    let authority = authority_json(actor);
    if let Some(result) = replay(c, command, &request_hash).await? {
        return serde_json::from_value(result).map_err(|_| unavailable());
    }
    let (item, batch) = instruction(c, &actor.company_id, id).await?;
    if item.version != input.expected_version {
        return Err(conflict(
            "VERSION_CONFLICT",
            "The supplier instruction changed. Refresh and review again.",
        ));
    }
    if !matches!(item.status.as_str(), "exported" | "unknown") {
        return Err(conflict(
            "INSTRUCTION_NOT_PENDING",
            "Confirm an exported instruction.",
        ));
    }
    if same_person(actor, &batch.created_by)
        || batch
            .approved_by
            .as_deref()
            .is_some_and(|value| same_person(actor, value))
    {
        return Err(forbidden(
            "INDEPENDENT_PAYER_REQUIRED",
            "A third person must export and confirm this approved batch.",
        ));
    }
    let invoice = invoice_row(c, &actor.company_id, &item.invoice_id).await?;
    let amount = item.amount_minor;
    // This bank-batch owner only settles the functional currency and preserves
    // the invoice's retained carrying amount.
    if invoice.base_outstanding_minor != Some(invoice.outstanding_minor)
        || invoice.outstanding_minor < amount
        || invoice.fx_rate_micros != Some(1_000_000)
    {
        return Err(conflict(
            "SUPPLIER_CARRYING_MISMATCH",
            "Reconcile the source invoice's functional carrying balance before confirming this supplier payment.",
        ));
    }
    let bank = bank_transaction(c, &actor.company_id, &input.bank_transaction_id).await?;
    if bank.version != input.bank_version
        || bank.currency != batch.currency
        || bank.bank_account != batch.bank_account_id
        || bank.amount_minor != -amount
        || bank.transaction_date < batch.planned_date
        || !matches!(bank.match_status.as_str(), "unmatched" | "proposed")
    {
        return Err(invalid(
            "BANK_EVIDENCE_MISMATCH",
            "Match the approved bank account, currency, date and exact outgoing amount.",
        ));
    }
    assert_open_period(c, &actor.company_id, &bank.transaction_date).await?;

    let mut prepared_evidence = None;
    if !input.attachment_set_ids.is_empty() {
        let binding_id = id.to_owned();
        let prepared = evidence
            .prepare(
                c,
                SupplierInstructionEvidenceRequest {
                    set_ids: input.attachment_set_ids.clone(),
                    target_id: batch.id.clone(),
                    expected_target_version: batch.version,
                    command_id: command.as_str().to_owned(),
                    binding_type: SUPPLIER_BATCH_EVIDENCE_BINDING_TYPE.into(),
                    binding_id,
                    purpose_key: SUPPLIER_BATCH_EVIDENCE_PURPOSE_KEY.into(),
                    component_kind: SUPPLIER_BATCH_EVIDENCE_COMPONENT_KIND.into(),
                    component_ref: format!("supplier-instruction:{id}"),
                },
            )
            .await?;
        prepared_evidence = Some(prepared);
    }

    let (journal_id, _number) = insert_payment_journal(
        c,
        actor,
        &invoice,
        &batch,
        &bank,
        amount,
        input,
        id,
        command.as_str(),
        now,
    )
    .await?;
    let payment_id = format!("supplier-payment-{}", command.as_str());
    let snapshot = prepare_money_snapshot(
        c,
        &CompanyId::new(&actor.company_id).map_err(|_| invalid("INVALID_COMPANY", "Invalid Company."))?,
        MoneySnapshotInput {
            source_type: "finance_supplier_instruction".into(),
            source_id: id.to_owned(),
            amount_kind: "settled".into(),
            transaction_date: bank.transaction_date.clone(),
            currency: batch.currency.clone(),
            currency_scale: Some(2),
            amount_units: amount,
        },
        &FixedClock(now),
    )
    .await?;
    let retained_snapshot = snapshot.persist(c).await?;

    sqlx::query(
        "INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,\
         currency,amount_minor,outstanding_minor,bank_account,status,journal_id,fx_rate_micros,\
         base_amount_minor,created_by,approved_by,released_by,created_at,updated_at) \
         VALUES($1,$2,$3,'payment',$4,$5,$6,$7,0,$8,'settled',$9,1000000,$7,$10,$11,$12,$13,$13)",
    )
    .bind(&payment_id)
    .bind(&actor.company_id)
    .bind(&invoice.party_id)
    .bind(format!("SB-{}", command.as_str()))
    .bind(&bank.transaction_date)
    .bind(&batch.currency)
    .bind(amount)
    .bind(&batch.bank_account_id)
    .bind(&journal_id)
    .bind(&batch.created_by)
    .bind(&batch.approved_by)
    .bind(&actor.user_id)
    .bind(now)
    .execute(&mut *c)
    .await?;
    sqlx::query(
        "INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,\
         base_invoice_minor,base_payment_minor,created_by,created_at) \
         VALUES($1,$2,$3,$4,$5,$5,$5,$6,$7)",
    )
    .bind(format!("allocation-{}", command.as_str()))
    .bind(&actor.company_id)
    .bind(&payment_id)
    .bind(&invoice.id)
    .bind(amount)
    .bind(&actor.user_id)
    .bind(now)
    .execute(&mut *c)
    .await?;
    sqlx::query(
        "UPDATE finance_invoices SET outstanding_minor=outstanding_minor-$1,\
         base_outstanding_minor=base_outstanding_minor-$1,\
         status=CASE WHEN outstanding_minor=$1 THEN 'paid' ELSE 'partially_paid' END,\
         version=version+1,updated_at=$2 WHERE id=$3 AND company_id=$4",
    )
    .bind(amount)
    .bind(now)
    .bind(&invoice.id)
    .bind(&actor.company_id)
    .execute(&mut *c)
    .await?;
    sqlx::query(
        "UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',\
         matched_id=$1,version=version+1 WHERE id=$2 AND company_id=$3",
    )
    .bind(&payment_id)
    .bind(&bank.id)
    .bind(&actor.company_id)
    .execute(&mut *c)
    .await?;
    sqlx::query(
        "UPDATE finance_supplier_instructions SET status='confirmed',bank_reference=$1,evidence=$2,\
         payment_id=$3,bank_transaction_id=$4,resolved_by=$5,version=version+1,updated_at=$6 \
         WHERE id=$7",
    )
    .bind(input.bank_reference.trim())
    .bind(input.evidence.trim())
    .bind(&payment_id)
    .bind(&bank.id)
    .bind(&actor.user_id)
    .bind(now)
    .bind(id)
    .execute(&mut *c)
    .await?;
    sqlx::query(
        "UPDATE finance_supplier_batches SET version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3",
    )
    .bind(now)
    .bind(&batch.id)
    .bind(&actor.company_id)
    .execute(&mut *c)
    .await?;

    let result = SupplierInstructionConfirmResult {
        id: id.to_owned(),
        status: "confirmed".into(),
        version: input.expected_version + 1,
        payment_id: payment_id.clone(),
        journal_id: journal_id.clone(),
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
    if let Some(prepared) = prepared_evidence.as_ref() {
        evidence.require_current(c, prepared).await?;
    }
    retained_snapshot.require_current(c).await?;
    notifications
        .stage(
            c,
            command.as_str(),
            &batch.id,
            "Supplier payment matched: source invoice and accounting updated",
            &actor.user_id,
            now,
        )
        .await?;
    Ok(result)
}

/// List recent supplier batches with pending/failed/unknown instruction counts.
pub async fn list_supplier_batches(
    c: &mut PgConnection,
    company: &str,
    source_only: bool,
    for_work: bool,
) -> Result<Vec<Value>, Error> {
    let sql = if for_work {
        "SELECT (to_jsonb(b)||jsonb_build_object(\
         'pending_count',(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status IN ('reserved','exported','unknown')),\
         'failed_count',(SELECT count(*) FROM finance_supplier_instructions i JOIN finance_invoices f ON f.id=i.invoice_id WHERE i.batch_id=b.id AND i.status='failed' AND f.outstanding_minor>0),\
         'unknown_count',(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status='unknown')))::text row \
         FROM finance_supplier_batches b WHERE b.company_id=$1 AND (NOT $2 OR (EXISTS(SELECT 1 FROM finance_supplier_instructions owned_item WHERE owned_item.batch_id=b.id) \
         AND NOT EXISTS(SELECT 1 FROM finance_supplier_instructions owned_item WHERE owned_item.batch_id=b.id \
         AND NOT EXISTS(SELECT 1 FROM (SELECT o.company_id,f.invoice_id id FROM commerce_orders o \
         JOIN commerce_fulfillments f ON f.order_id=o.id JOIN finance_invoices i ON i.id=f.invoice_id \
         AND i.company_id=o.company_id AND i.kind='payable' WHERE o.kind='purchase') owned_source \
         WHERE owned_source.id=owned_item.invoice_id AND owned_source.company_id=b.company_id)))) \
         ORDER BY b.updated_at DESC"
    } else {
        "SELECT (to_jsonb(b)||jsonb_build_object(\
         'pending_count',(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status IN ('reserved','exported','unknown')),\
         'failed_count',(SELECT count(*) FROM finance_supplier_instructions i JOIN finance_invoices f ON f.id=i.invoice_id WHERE i.batch_id=b.id AND i.status='failed' AND f.outstanding_minor>0),\
         'unknown_count',(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status='unknown')))::text row \
         FROM finance_supplier_batches b WHERE b.company_id=$1 AND (NOT $2 OR (EXISTS(SELECT 1 FROM finance_supplier_instructions owned_item WHERE owned_item.batch_id=b.id) \
         AND NOT EXISTS(SELECT 1 FROM finance_supplier_instructions owned_item WHERE owned_item.batch_id=b.id \
         AND NOT EXISTS(SELECT 1 FROM (SELECT o.company_id,f.invoice_id id FROM commerce_orders o \
         JOIN commerce_fulfillments f ON f.order_id=o.id JOIN finance_invoices i ON i.id=f.invoice_id \
         AND i.company_id=o.company_id AND i.kind='payable' WHERE o.kind='purchase') owned_source \
         WHERE owned_source.id=owned_item.invoice_id AND owned_source.company_id=b.company_id)))) \
         ORDER BY b.updated_at DESC LIMIT 200"
    };
    let rows = sqlx::query(sql)
        .bind(company)
        .bind(source_only)
        .fetch_all(&mut *c)
        .await?;
    rows.into_iter()
        .map(|row| {
            let text: String = row.try_get("row")?;
            serde_json::from_str(&text).map_err(|_| unavailable())
        })
        .collect()
}

/// The supplier batch workspace: selected batch/items plus open payables, cash
/// ledgers, open periods and candidate bank debits.
pub async fn supplier_batch_workspace(
    c: &mut PgConnection,
    company: &str,
    id: Option<&str>,
    can_pay: bool,
    source_only: bool,
) -> Result<Value, Error> {
    let selected = match id {
        Some(id) => Some(batch(c, company, id).await?),
        None => None,
    };
    let items = match id {
        Some(id) => {
            let rows = sqlx::query(
                "SELECT (to_jsonb(s)||jsonb_build_object('invoice_number',i.invoice_number,\
                 'currency',i.currency))::text row FROM finance_supplier_instructions s \
                 JOIN finance_invoices i ON i.id=s.invoice_id WHERE s.batch_id=$1 ORDER BY s.id",
            )
            .bind(id)
            .fetch_all(&mut *c)
            .await?;
            rows.into_iter()
                .map(|row| {
                    let text: String = row.try_get("row")?;
                    serde_json::from_str(&text).map_err(|_| unavailable())
                })
                .collect::<Result<Vec<Value>, Error>>()?
        }
        None => Vec::new(),
    };
    let invoices: Vec<Value> = if id.is_none() {
        let rows = sqlx::query(
            "SELECT (to_jsonb(i)||jsonb_build_object('party_name',p.name,\
             'available_minor',i.outstanding_minor-COALESCE((SELECT SUM(s.amount_minor) \
             FROM finance_supplier_instructions s WHERE s.invoice_id=i.id \
             AND s.status IN ('reserved','exported','unknown')),0)))::text row \
             FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id \
             WHERE i.company_id=$1 AND i.kind='payable' \
             AND i.source_type IN ('standalone','commerce_fulfillment') \
             AND i.status IN ('posted','partially_paid') \
             AND (NOT $2 OR EXISTS(SELECT 1 FROM (SELECT o.company_id,f.invoice_id id FROM commerce_orders o \
             JOIN commerce_fulfillments f ON f.order_id=o.id JOIN finance_invoices i ON i.id=f.invoice_id \
             AND i.company_id=o.company_id AND i.kind='payable' WHERE o.kind='purchase') owner_invoice \
             WHERE owner_invoice.id=i.id AND owner_invoice.company_id=i.company_id)) \
             ORDER BY i.due_date,i.id LIMIT 200",
        )
        .bind(company)
        .bind(source_only)
        .fetch_all(&mut *c)
        .await?;
        rows.into_iter()
            .map(|row| {
                let text: String = row.try_get("row")?;
                serde_json::from_str(&text).map_err(|_| unavailable())
            })
            .collect::<Result<Vec<Value>, Error>>()?
    } else {
        Vec::new()
    };
    let accounts: Vec<Value> = sqlx::query(
        "SELECT (to_jsonb(a)||jsonb_build_object('currency',a.currency))::text row \
         FROM finance_accounts a JOIN finance_charts ch ON ch.id=a.chart_id \
         WHERE a.company_id=$1 AND a.category='asset' AND a.status='active' \
         AND a.id=(SELECT cash_account_id FROM commerce_settings WHERE company_id=a.company_id) \
         AND ch.status='active' AND a.currency=ch.base_currency ORDER BY a.code",
    )
        .bind(company)
        .fetch_all(&mut *c)
        .await?
        .into_iter()
        .map(|row| {
            let text: String = row.try_get("row")?;
            serde_json::from_str(&text).map_err(|_| unavailable())
        })
        .collect::<Result<Vec<Value>, Error>>()?;
    let periods: Vec<Value> = sqlx::query(
        "SELECT (to_jsonb(p))::text row FROM finance_periods p WHERE p.company_id=$1 \
         AND p.state='open' ORDER BY p.start_date",
    )
    .bind(company)
    .fetch_all(&mut *c)
    .await?
    .into_iter()
    .map(|row| {
        let text: String = row.try_get("row")?;
        serde_json::from_str(&text).map_err(|_| unavailable())
    })
    .collect::<Result<Vec<Value>, Error>>()?;
    let bank: Vec<Value> = match selected.as_ref().filter(|_| can_pay) {
        Some(batch) => {
            sqlx::query(
                "SELECT (to_jsonb(t)||jsonb_build_object('bank_account',s.bank_account))::text row \
                 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id \
                 WHERE t.company_id=$1 AND s.bank_account=$2 AND t.match_status IN ('unmatched','proposed') \
                 AND (NOT $3 OR (t.currency=$4 AND t.transaction_date>=$5 \
                 AND EXISTS(SELECT 1 FROM finance_supplier_instructions instruction \
                 WHERE instruction.batch_id=$6 AND instruction.status IN ('exported','unknown') \
                 AND t.amount_minor=-instruction.amount_minor))) \
                 ORDER BY t.transaction_date DESC LIMIT 200",
            )
            .bind(company)
            .bind(&batch.bank_account_id)
            .bind(source_only)
            .bind(&batch.currency)
            .bind(&batch.planned_date)
            .bind(&batch.id)
                .fetch_all(&mut *c)
                .await?
                .into_iter()
                .map(|row| {
                    let text: String = row.try_get("row")?;
                    serde_json::from_str(&text).map_err(|_| unavailable())
                })
                .collect::<Result<Vec<Value>, Error>>()?
        }
        None => Vec::new(),
    };
    Ok(json!({
        "batch": selected.map(|batch| json!({
            "id": batch.id,
            "company_id": batch.company_id,
            "currency": batch.currency,
            "bank_account_id": batch.bank_account_id,
            "planned_date": batch.planned_date,
            "status": batch.status,
            "created_by": batch.created_by,
            "approved_by": batch.approved_by,
            "exported_by": batch.exported_by,
            "export_command_id": batch.export_command_id,
            "export_hash": batch.export_hash,
            "version": batch.version,
        })),
        "items": items,
        "invoices": invoices,
        "accounts": accounts,
        "periods": periods,
        "bank": bank,
    }))
}
