//! Native Finance opening-balance preview and validation. The import re-runs
//! this preview and reuses the native journal preparation/store.
use super::postgres_journal::PgJournalStore;
use super::postgres_journal_facts::{PgJournalPreparation, embedded_currency_rules};
use super::sales_invoice_create::SalesInvoiceCommandActor;
use crate::application::journal::{
    FinanceActor, JournalAuthority, JournalDraft, JournalStatus, prepare_journal,
};
use crate::domain::company_money::FinanceCollation;
use crate::domain::journal::{JournalError, JournalLineInput};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpeningInput {
    pub chart_id: String,
    pub period_id: String,
    pub entry_date: String,
    pub currency: String,
    pub lines: Vec<OpeningLine>,
    #[serde(default)]
    pub subledger_items: Vec<OpeningSubledgerItem>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpeningLine {
    pub account_id: String,
    #[serde(default)]
    pub description: String,
    pub debit: String,
    pub credit: String,
    #[serde(default = "default_kind")]
    pub subledger_kind: String,
}

fn default_kind() -> String {
    "none".into()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpeningSubledgerItem {
    pub kind: String,
    pub party_id: String,
    pub reference: String,
    pub due_date: String,
    pub control_account_id: String,
    pub offset_account_id: String,
    pub amount: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpeningPreviewLine {
    pub account_id: String,
    pub account_code: String,
    pub account_name: String,
    pub description: String,
    pub debit: String,
    pub credit: String,
    pub subledger_kind: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpeningPreview {
    pub preview_token: String,
    pub chart_id: String,
    pub period_id: String,
    pub entry_date: String,
    pub currency: String,
    pub debit: String,
    pub credit: String,
    pub lines: Vec<OpeningPreviewLine>,
    pub subledger_items: Vec<OpeningSubledgerItem>,
    pub subledger_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpeningImportInput {
    pub chart_id: String,
    pub period_id: String,
    pub entry_date: String,
    pub currency: String,
    pub lines: Vec<OpeningLine>,
    #[serde(default)]
    pub subledger_items: Vec<OpeningSubledgerItem>,
    pub preview_token: String,
}

impl OpeningImportInput {
    pub fn definition(&self) -> OpeningInput {
        OpeningInput {
            chart_id: self.chart_id.clone(),
            period_id: self.period_id.clone(),
            entry_date: self.entry_date.clone(),
            currency: self.currency.clone(),
            lines: self.lines.clone(),
            subledger_items: self.subledger_items.clone(),
        }
    }
}

struct Account {
    id: String,
    chart_id: String,
    currency: Option<String>,
    status: String,
    is_control: i64,
    control_kind: String,
    code: String,
    name: String,
}

struct Party {
    id: String,
    kind: String,
    currency: String,
    status: String,
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
        "FINANCE_OPENING_UNAVAILABLE",
        "The opening balance could not be prepared.",
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

pub async fn lock_finance_opening_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE finance_charts,finance_accounts,finance_parties,finance_periods,\
         finance_journals,finance_invoices IN SHARE ROW EXCLUSIVE MODE",
    )
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

fn decimal_units(value: &str) -> Result<i128, Error> {
    if value.is_empty() || value.len() > 32 {
        return Err(invalid("INVALID_OPENING_LINE", "Use a positive debit or credit."));
    }
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 2
    {
        return Err(invalid("INVALID_OPENING_LINE", "Use a positive debit or credit."));
    }
    let whole = whole
        .parse::<i128>()
        .map_err(|_| invalid("INVALID_OPENING_LINE", "Use a positive debit or credit."))?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<2}")
            .parse::<i128>()
            .map_err(|_| invalid("INVALID_OPENING_LINE", "Use a positive debit or credit."))?
    };
    whole
        .checked_mul(100)
        .and_then(|value| value.checked_add(fraction))
        .filter(|value| *value >= 0 && *value <= 9_007_199_254_740_991)
        .ok_or_else(|| invalid("INVALID_OPENING_LINE", "Use a positive debit or credit."))
}

fn minor_decimal(value: i128) -> String {
    format!("{}.{:02}", value / 100, value % 100)
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

pub async fn read_opening_preview(
    c: &mut PgConnection,
    company: &str,
    input: &OpeningInput,
) -> Result<OpeningPreview, Error> {
    serializable(c).await?;
    if company.is_empty()
        || company.len() > 200
        || input.chart_id.is_empty()
        || input.chart_id.len() > 200
        || input.period_id.is_empty()
        || input.period_id.len() > 200
        || !valid_date(&input.entry_date)
        || input.currency.len() != 3
        || !input.currency.bytes().all(|byte| byte.is_ascii_uppercase())
        || input.lines.len() < 2
        || input.lines.len() > 500
        || input.subledger_items.len() > 2000
    {
        return Err(invalid(
            "INVALID_OPENING_BALANCE",
            "Use a chart, open period, currency and at least two opening lines.",
        ));
    }
    let chart = sqlx::query(
        "SELECT id,base_currency,status FROM finance_charts WHERE id=$1 AND company_id=$2",
    )
    .bind(&input.chart_id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| not_found("CHART_NOT_FOUND", "Active chart not found."))?;
    let base_currency: String = chart.try_get("base_currency")?;
    if chart.try_get::<String, _>("status")? != "active" {
        return Err(not_found("CHART_NOT_FOUND", "Active chart not found."));
    }
    if input.currency != base_currency {
        return Err(invalid(
            "OPENING_CURRENCY_MISMATCH",
            "Opening balances must use the chart's functional currency.",
        ));
    }
    let period = sqlx::query(
        "SELECT id,state,start_date,end_date FROM finance_periods WHERE id=$1 AND company_id=$2",
    )
    .bind(&input.period_id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| not_found("PERIOD_NOT_FOUND", "Opening period not found."))?;
    let start: String = period.try_get("start_date")?;
    let end: String = period.try_get("end_date")?;
    if period.try_get::<String, _>("state")? != "open"
        || input.entry_date < start
        || input.entry_date > end
    {
        return Err(conflict(
            "PERIOD_CLOSED",
            "The opening period does not accept postings on this date.",
        ));
    }
    let existing: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_journals WHERE company_id=$1 AND chart_id=$2 \
         AND is_opening=1 AND status IN ('draft','in_review','posted'))",
    )
    .bind(company)
    .bind(&input.chart_id)
    .fetch_one(&mut *c)
    .await?;
    if existing {
        return Err(conflict(
            "OPENING_BALANCE_EXISTS",
            "Reverse or finish the existing opening balance before importing another one.",
        ));
    }
    let mut line_accounts = std::collections::BTreeSet::new();
    for line in &input.lines {
        if !line_accounts.insert(line.account_id.clone()) {
            return Err(invalid(
                "DUPLICATE_OPENING_ACCOUNT",
                "Each account may appear once in an opening balance import.",
            ));
        }
    }
    let mut ids: Vec<String> = line_accounts.iter().cloned().collect();
    for item in &input.subledger_items {
        ids.push(item.offset_account_id.clone());
        ids.push(item.control_account_id.clone());
    }
    ids.sort();
    ids.dedup();
    let account_rows = sqlx::query(
        "SELECT id,chart_id,currency,status,is_control,control_kind,code,name \
         FROM finance_accounts WHERE company_id=$1 AND id=ANY($2)",
    )
    .bind(company)
    .bind(&ids)
    .fetch_all(&mut *c)
    .await?;
    let mut accounts = std::collections::BTreeMap::new();
    for row in account_rows {
        let account = Account {
            id: row.try_get("id")?,
            chart_id: row.try_get("chart_id")?,
            currency: row.try_get("currency")?,
            status: row.try_get("status")?,
            is_control: row.try_get("is_control")?,
            control_kind: row.try_get("control_kind")?,
            code: row.try_get("code")?,
            name: row.try_get("name")?,
        };
        accounts.insert(account.id.clone(), account);
    }
    if accounts.len() != ids.len()
        || accounts.values().any(|account| {
            account.status != "active"
                || account.chart_id != input.chart_id
                || account.currency.as_deref() != Some(input.currency.as_str())
        })
    {
        return Err(invalid(
            "ACCOUNT_INVALID",
            "Every opening balance account must be active and belong to this chart.",
        ));
    }
    let mut debit = 0i128;
    let mut credit = 0i128;
    let mut normalized_lines = Vec::with_capacity(input.lines.len());
    let mut control_lines = std::collections::BTreeMap::new();
    for line in &input.lines {
        let account = accounts.get(&line.account_id).ok_or_else(|| {
            invalid("ACCOUNT_INVALID", "Every opening balance account must be active.")
        })?;
        let debit_minor = decimal_units(&line.debit)?;
        let credit_minor = decimal_units(&line.credit)?;
        if (debit_minor > 0) == (credit_minor > 0) {
            return Err(invalid(
                "INVALID_OPENING_LINE",
                "Each opening line requires one positive debit or credit.",
            ));
        }
        if matches!(account.control_kind.as_str(), "receivable" | "payable")
            && line.subledger_kind != account.control_kind
        {
            return Err(invalid(
                "OPENING_CONTROL_SCHEDULE_REQUIRED",
                "A control account requires its matching opening schedule.",
            ));
        }
        if line.subledger_kind != "none" {
            if account.is_control != 1 {
                return Err(invalid(
                    "OPENING_CONTROL_ACCOUNT_REQUIRED",
                    "Receivable and payable opening lines must use control accounts.",
                ));
            }
            if line.subledger_kind == "receivable" && debit_minor <= 0 {
                return Err(invalid(
                    "OPENING_RECEIVABLE_DIRECTION",
                    "Opening receivables must be a debit balance.",
                ));
            }
            if line.subledger_kind == "payable" && credit_minor <= 0 {
                return Err(invalid(
                    "OPENING_PAYABLE_DIRECTION",
                    "Opening payables must be a credit balance.",
                ));
            }
            control_lines.insert(line.account_id.clone(), (line.subledger_kind.clone(), debit_minor, credit_minor));
        }
        debit += debit_minor;
        credit += credit_minor;
        normalized_lines.push(OpeningPreviewLine {
            account_id: line.account_id.clone(),
            account_code: account.code.clone(),
            account_name: account.name.clone(),
            description: line.description.trim().into(),
            debit: minor_decimal(debit_minor),
            credit: minor_decimal(credit_minor),
            subledger_kind: line.subledger_kind.clone(),
        });
    }
    if debit != credit {
        return Err(invalid(
            "UNBALANCED_OPENING_BALANCE",
            "Opening debits and credits must balance.",
        ));
    }
    let mut references = std::collections::BTreeSet::new();
    let mut schedule = std::collections::BTreeMap::new();
    let party_ids: Vec<String> = input
        .subledger_items
        .iter()
        .map(|item| item.party_id.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let party_rows = if party_ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query("SELECT id,kind,currency,status FROM finance_parties WHERE company_id=$1 AND id=ANY($2)")
            .bind(company)
            .bind(&party_ids)
            .fetch_all(&mut *c)
            .await?
    };
    let mut parties = std::collections::BTreeMap::new();
    for row in party_rows {
        let party = Party {
            id: row.try_get("id")?,
            kind: row.try_get("kind")?,
            currency: row.try_get("currency")?,
            status: row.try_get("status")?,
        };
        parties.insert(party.id.clone(), party);
    }
    let mut normalized_items = Vec::with_capacity(input.subledger_items.len());
    for item in &input.subledger_items {
        if !matches!(item.kind.as_str(), "receivable" | "payable")
            || !valid_date(&item.due_date)
            || item.reference.trim().is_empty()
            || item.reference.trim().len() > 120
        {
            return Err(invalid(
                "INVALID_OPENING_SUBLEDGER",
                "Use a valid opening subledger reference and due date.",
            ));
        }
        let amount = decimal_units(&item.amount)?;
        if amount <= 0 {
            return Err(invalid(
                "INVALID_OPENING_SUBLEDGER_AMOUNT",
                "Each opening subledger amount must be greater than zero.",
            ));
        }
        let party = parties.get(&item.party_id).ok_or_else(|| {
            invalid("OPENING_PARTY_INVALID", "Opening subledger parties must be active.")
        })?;
        if party.status != "active"
            || party.kind != (if item.kind == "receivable" { "customer" } else { "supplier" })
            || party.currency != input.currency
        {
            return Err(invalid(
                "OPENING_PARTY_INVALID",
                "Opening subledger parties must match the kind and currency.",
            ));
        }
        let control = control_lines.get(&item.control_account_id).ok_or_else(|| {
            invalid(
                "OPENING_CONTROL_LINE_REQUIRED",
                "Every subledger item must identify its matching control line.",
            )
        })?;
        if control.0 != item.kind {
            return Err(invalid(
                "OPENING_CONTROL_KIND_MISMATCH",
                "The opening subledger kind does not match its control line.",
            ));
        }
        let offset = accounts.get(&item.offset_account_id).ok_or_else(|| {
            invalid("OPENING_OFFSET_ACCOUNT_INVALID", "Use an active offset account.")
        })?;
        if offset.status != "active"
            || offset.chart_id != input.chart_id
            || offset.id == item.control_account_id
        {
            return Err(invalid(
                "OPENING_OFFSET_ACCOUNT_INVALID",
                "Use a different active offset account in this chart.",
            ));
        }
        let key = format!("{}:{}", item.kind, item.reference.trim().to_lowercase());
        if !references.insert(key) {
            return Err(invalid(
                "DUPLICATE_OPENING_REFERENCE",
                "Opening subledger references must be unique for each kind.",
            ));
        }
        let scheduled = schedule.entry(item.control_account_id.clone()).or_insert(0i128);
        *scheduled += amount;
        normalized_items.push(OpeningSubledgerItem {
            kind: item.kind.clone(),
            party_id: item.party_id.clone(),
            reference: item.reference.trim().into(),
            due_date: item.due_date.clone(),
            control_account_id: item.control_account_id.clone(),
            offset_account_id: item.offset_account_id.clone(),
            amount: minor_decimal(amount),
        });
    }
    for (account_id, (kind, debit, credit)) in &control_lines {
        let expected = if kind == "receivable" { *debit } else { *credit };
        if schedule.get(account_id).copied().unwrap_or(0) != expected {
            accounts.get(account_id).ok_or_else(unavailable)?;
            return Err(invalid(
                "OPENING_SUBLEDGER_MISMATCH",
                "The opening control line does not match its subledger schedule.",
            ));
        }
    }
    for item in &input.subledger_items {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM finance_invoices WHERE company_id=$1 AND kind=$2 \
             AND lower(invoice_number)=lower($3))",
        )
        .bind(company)
        .bind(&item.kind)
        .bind(item.reference.trim())
        .fetch_one(&mut *c)
        .await?;
        if exists {
            return Err(conflict(
                "OPENING_REFERENCE_EXISTS",
                "An invoice already exists for an opening subledger reference.",
            ));
        }
    }
    let token = hash(
        &serde_json::to_string(&json!({
            "company": company,
            "input": input,
            "chart": {"id": input.chart_id, "currency": base_currency, "status": "active"},
            "period": {"id": input.period_id, "state": "open", "start": start, "end": end},
            "accounts": accounts.values().map(|account| json!({
                "id": account.id, "status": account.status, "isControl": account.is_control,
                "controlKind": account.control_kind, "currency": account.currency
            })).collect::<Vec<_>>(),
            "parties": parties.values().map(|party| json!({
                "id": party.id, "kind": party.kind, "currency": party.currency, "status": party.status
            })).collect::<Vec<_>>()
        }))
        .map_err(|_| unavailable())?,
    );
    Ok(OpeningPreview {
        preview_token: token,
        chart_id: input.chart_id.clone(),
        period_id: input.period_id.clone(),
        entry_date: input.entry_date.clone(),
        currency: input.currency.clone(),
        debit: minor_decimal(debit),
        credit: minor_decimal(credit),
        lines: normalized_lines,
        subledger_items: normalized_items,
        subledger_count: input.subledger_items.len(),
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_opening_invoices(
    c: &mut PgConnection,
    company: &str,
    journal_id: &str,
    entry_date: &str,
    input: &OpeningInput,
    created_by: &str,
    now: i64,
) -> Result<(), Error> {
    for (index, item) in input.subledger_items.iter().enumerate() {
        let amount = decimal_units(&item.amount)?;
        let amount = i64::try_from(amount).map_err(|_| unavailable())?;
        let receivable = item.kind == "receivable";
        let (debit, credit) = if receivable {
            (&item.control_account_id, &item.offset_account_id)
        } else {
            (&item.offset_account_id, &item.control_account_id)
        };
        sqlx::query(
            "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,\
             due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,\
             base_total_minor,base_outstanding_minor,debit_account_id,credit_account_id,\
             source_type,source_id,status,journal_id,created_by,created_at,updated_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,0,$9,$9,$9,$9,$10,$11,'opening_import',$12,'draft',$12,$13,$14,$14)",
        )
        .bind(format!("{journal_id}-subledger-{index}"))
        .bind(company)
        .bind(&item.party_id)
        .bind(&item.kind)
        .bind(item.reference.trim())
        .bind(entry_date)
        .bind(&item.due_date)
        .bind(&input.currency)
        .bind(amount)
        .bind(debit)
        .bind(credit)
        .bind(journal_id)
        .bind(created_by)
        .bind(now)
        .execute(&mut *c)
        .await?;
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpeningImportResult {
    pub id: String,
    pub journal_id: String,
    pub number: String,
    pub debit: String,
    pub credit: String,
    pub status: String,
    pub version: u32,
    pub subledger_count: usize,
    pub preview_token: String,
}

struct OpeningPreparationAuthority<'a> {
    actor: &'a FinanceActor,
}

impl JournalAuthority for OpeningPreparationAuthority<'_> {
    type Error = PgTransactionError<JournalError>;

    async fn authorize(
        &mut self,
        actor: &FinanceActor,
        draft: &JournalDraft,
    ) -> Result<(), Self::Error> {
        if actor != self.actor
            || draft.source_type != "opening_import"
            || draft.source_id.as_deref() != Some(draft.chart_id.as_str())
            || !draft.is_opening
        {
            return Err(JournalError {
                kind: FailureKind::Forbidden,
                code: "OPENING_SOURCE_INVALID",
                message: "The opening balance source is not the current chart.".into(),
            }
            .into());
        }
        Ok(())
    }
}

fn string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

fn optional(value: Option<&str>) -> String {
    value
        .map(string)
        .unwrap_or_else(|| "null".into())
}

fn opening_fingerprint(
    actor: &SalesInvoiceCommandActor,
    operation: &str,
    record: &str,
    input: &str,
) -> String {
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

fn opening_authority_json(actor: &SalesInvoiceCommandActor) -> String {
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

#[allow(clippy::too_many_arguments)]
async fn opening_receipt(
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
        "state": result.get("status").cloned().unwrap_or(Value::Null),
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

/// Import a previewed opening balance and its subledger schedules in one
/// serializable command. The preview is recomputed from the same source facts so
/// the client token cannot authorize stale chart/period/account/party state.
#[allow(clippy::too_many_arguments)]
pub async fn import_opening_balance<C: Clock + Sync>(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    finance: &FinanceActor,
    clock: &C,
    command: &CommandKey,
    input: &OpeningInput,
    preview_token: &str,
    now: i64,
) -> Result<OpeningImportResult, Error> {
    serializable(c).await?;
    if actor.company_id.is_empty()
        || actor.user_id.is_empty()
        || actor.assignment_id.is_empty()
        || actor.branch_id.is_some()
        || actor.department_id.is_some()
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "COMPANY_SCOPE_REQUIRED",
            "Use a Company Finance assignment for opening balances.",
        )
        .into());
    }
    let journal_id = format!("journal-{}", command.as_str());
    let operation = "finance.journal.create";
    let input_json = serde_json::to_string(&json!({
        "journal": {
            "chartId": input.chart_id,
            "periodId": input.period_id,
            "entryDate": input.entry_date,
            "currency": input.currency,
            "sourceType": "opening_import",
            "sourceId": input.chart_id,
            "isOpening": true,
            "lines": input.lines,
        },
        "subledgerItems": input.subledger_items,
    }))
    .map_err(|_| unavailable())?;
    let request_hash = opening_fingerprint(actor, operation, &journal_id, &input_json);
    let authority = opening_authority_json(actor);
    let saved = sqlx::query(
        "SELECT operation,request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
    )
    .bind(command.as_str())
    .fetch_optional(&mut *c)
    .await?;
    if let Some(row) = saved {
        let saved_operation: String = row.try_get("operation")?;
        let saved_hash: String = row.try_get("request_hash")?;
        if saved_operation != operation || saved_hash != request_hash {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            ));
        }
        let raw: Option<String> = row.try_get("result_json")?;
        return raw
            .and_then(|value| serde_json::from_str::<OpeningImportResult>(&value).ok())
            .ok_or_else(unavailable);
    }
    let preview = read_opening_preview(c, &actor.company_id, input).await?;
    if preview.preview_token != preview_token {
        return Err(conflict(
            "OPENING_PREVIEW_STALE",
            "The chart, period, accounts or opening balances changed. Preview the import again.",
        ));
    }
    let draft = JournalDraft {
        chart_id: input.chart_id.clone(),
        period_id: input.period_id.clone(),
        entry_date: input.entry_date.clone(),
        currency: input.currency.clone(),
        fx_rate: None,
        is_opening: true,
        memo: "Verified opening trial balance and subledger schedules".into(),
        source_type: "opening_import".into(),
        source_id: Some(input.chart_id.clone()),
        lines: preview
            .lines
            .iter()
            .map(|line| JournalLineInput {
                account_id: line.account_id.clone(),
                description: Some(line.description.clone()),
                debit: line.debit.clone(),
                credit: line.credit.clone(),
                dimensions: None,
            })
            .collect(),
    };
    let rules = embedded_currency_rules()
        .map_err(|_| invalid("CURRENCY_CATALOGUE_UNAVAILABLE", "Currency rules are unavailable."))?;
    let collation = FinanceCollation::english().map_err(|_| {
        invalid(
            "FINANCE_COLLATION_UNAVAILABLE",
            "Finance collation is unavailable.",
        )
    })?;
    let mut preparation_authority = OpeningPreparationAuthority { actor: finance };
    let plan = {
        let mut ports = PgJournalPreparation::new(
            &mut *c,
            finance,
            &mut preparation_authority,
            clock,
            &rules,
            &collation,
        )
        .map_err(opening_journal_error)?;
        prepare_journal(
            &mut ports,
            finance,
            &draft,
            command,
            JournalStatus::Draft,
        )
        .await
        .map_err(opening_journal_error)?
    };
    PgJournalStore::new(
        finance.holding_id.clone(),
        finance.company_id.clone(),
        &mut *c,
    )
    .persist(&plan)
    .await
    .map_err(opening_journal_error)?;
    insert_opening_invoices(
        c,
        &actor.company_id,
        plan.journal_id(),
        &input.entry_date,
        input,
        &actor.user_id,
        now,
    )
    .await?;
    let result = OpeningImportResult {
        id: plan.journal_id().to_owned(),
        journal_id: plan.journal_id().to_owned(),
        number: plan.number().to_owned(),
        debit: plan.result().debit.clone(),
        credit: plan.result().credit.clone(),
        status: "draft".into(),
        version: plan.result().version,
        subledger_count: input.subledger_items.len(),
        preview_token: preview.preview_token,
    };
    opening_receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        operation,
        &request_hash,
        plan.journal_id(),
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        1,
        now,
    )
    .await?;
    Ok(result)
}

fn opening_journal_error(error: PgTransactionError<JournalError>) -> Error {
    match error {
        PgTransactionError::Persistence(error) => PgTransactionError::Persistence(error),
        PgTransactionError::Business(error) => BusinessError::new(
            error.kind,
            error.code,
            "The opening balance journal could not be created.",
        )
        .into(),
    }
}
