//! Native Procurement supplier promise history facts.
//!
//! These are owner read projections only. They are not an authorization proof
//! and they do not record, revise or withdraw a supplier promise. Composition
//! retains the normal Account/Holding session, purchase admission and current
//! authority; the owner read enforces the Company, selected-context expiry,
//! `commerce.read` and the Procurement module for this bounded slice.
use super::valid_date;
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::Serialize;
use std::collections::BTreeMap;

const HISTORY_LIMIT: usize = 1_000;
const LINE_LIMIT: usize = 10_000;
const OPEN_LIMIT: usize = 10_000;
const REFERENCE_LIMIT: usize = 160;
const EVIDENCE_LIMIT: usize = 2_000;
const DESCRIPTION_LIMIT: usize = 2_000;
const TEXT_LIMIT: usize = 200;

#[derive(Clone, Debug)]
pub struct SupplierPromiseReadActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl SupplierPromiseReadActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self, module: &str) -> bool {
        self.modules.iter().any(|value| value == module)
    }

    pub fn can_read_promises(&self) -> bool {
        self.has("commerce.read") && self.has_module("procurement")
    }

    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("commerce.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant purchasing read access.",
            ));
        }
        if !self.has_module("procurement") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MODULE_FORBIDDEN",
                "Your current role cannot access Procurement.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }
}

pub(crate) fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PURCHASE_ORDER_NOT_FOUND",
        "Purchase order not found.",
    )
}

fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SUPPLIER_PROMISE_HISTORY_INVALID",
        "The current supplier promise history is invalid.",
    )
}

fn too_large() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "SUPPLIER_PROMISE_HISTORY_TOO_LARGE",
        "Review a smaller purchase order before loading its complete supplier history.",
    )
}

fn valid_identifier(value: &str) -> bool {
    RecordId::new(value).is_ok()
}


fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn valid_text(value: &str, minimum: usize, limit: usize) -> bool {
    let length = value.encode_utf16().count();
    let trimmed = value.trim_matches(|c: char| {
        matches!(
            c,
            '\u{0009}'..='\u{000d}'
                | '\u{0020}'
                | '\u{00a0}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
        )
    });
    (minimum..=limit).contains(&length) && trimmed == value
}

fn valid_supplier_text(value: &str, minimum: usize, limit: usize) -> bool {
    let length = value.encode_utf16().count();
    // Retained Zod uses JavaScript String.trim: U+FEFF is whitespace, while
    // U+0085 is not. Rust's Unicode whitespace set differs on both points.
    let trimmed = value.trim_matches(|c: char| matches!(c,
        '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
        '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
        '\u{205f}' | '\u{3000}' | '\u{feff}'));
    (minimum..=limit).contains(&length) && trimmed == value
}

fn valid_kind(value: &str) -> bool {
    matches!(value, "confirmed" | "withdrawn")
}

fn valid_order_state(value: &str) -> bool {
    matches!(
        value,
        "draft"
            | "submitted"
            | "confirmed"
            | "partially_fulfilled"
            | "fulfilled"
            | "converted"
            | "cancelled"
    )
}

#[derive(Clone, Debug, Serialize)]
pub struct SupplierPromiseRow {
    pub id: String,
    pub order_id: String,
    pub company_id: String,
    pub order_version: i64,
    pub supplier_id: String,
    pub currency: String,
    pub required_date: String,
    pub kind: String,
    pub confirmation_date: String,
    pub supplier_reference: String,
    pub evidence: String,
    pub recorded_by: String,
    pub created_at: i64,
}

impl SupplierPromiseRow {
    fn validate(&self, company: &CompanyId, order_id: &str) -> Result<(), BusinessError> {
        if self.order_id != order_id
            || self.company_id != company.as_str()
            || !valid_identifier(&self.id)
            || !valid_identifier(&self.supplier_id)
            || !valid_identifier(&self.recorded_by)
            || self.order_version < 1
            || self.created_at < 0
            || !valid_currency(&self.currency)
            || !valid_date(&self.required_date)
            || !valid_kind(&self.kind)
            || !valid_date(&self.confirmation_date)
            || !valid_supplier_text(&self.supplier_reference, 3, REFERENCE_LIMIT)
            || !valid_supplier_text(&self.evidence, 5, EVIDENCE_LIMIT)
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SupplierPromiseLineRow {
    pub id: String,
    pub promise_id: String,
    pub order_line_id: String,
    pub ordinal: i64,
    pub quantity_milli: i64,
    pub fulfilled_snapshot_milli: i64,
    pub promised_date: String,
}

impl SupplierPromiseLineRow {
    fn validate(&self, headers: &BTreeMap<&str, &SupplierPromiseRow>) -> Result<(), BusinessError> {
        let header = headers
            .get(self.promise_id.as_str())
            .ok_or_else(source_invalid)?;
        if header.kind != "confirmed" {
            return Err(source_invalid());
        }
        if !valid_identifier(&self.id)
            || !valid_identifier(&self.order_line_id)
            || !(0..=1_000_000).contains(&self.ordinal)
            || self.quantity_milli <= 0
            || self.fulfilled_snapshot_milli < 0
            || !valid_date(&self.promised_date)
            || self.promised_date < header.confirmation_date
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct OpenPromiseRow {
    pub id: String,
    pub promise_id: String,
    pub order_id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub order_number: String,
    pub order_state: String,
    pub order_line_id: String,
    pub ordinal: i64,
    pub quantity_milli: i64,
    pub fulfilled_snapshot_milli: i64,
    pub promised_date: String,
    pub product_id: String,
    pub description: String,
    pub fulfilled_milli: i64,
    pub prior_quantity: i64,
    pub remaining_milli: i64,
}

impl OpenPromiseRow {
    fn validate(
        &self,
        company: &CompanyId,
        order_id: &str,
        headers: &BTreeMap<&str, &SupplierPromiseRow>,
        lines: &BTreeMap<&str, &SupplierPromiseLineRow>,
    ) -> Result<(), BusinessError> {
        let header = headers
            .get(self.promise_id.as_str())
            .ok_or_else(source_invalid)?;
        let line = lines.get(self.id.as_str()).ok_or_else(source_invalid)?;
        if self.order_id != order_id
            || self.company_id != company.as_str()
            || self.promise_id != line.promise_id
            || self.order_line_id != line.order_line_id
            || self.ordinal != line.ordinal
            || self.quantity_milli != line.quantity_milli
            || self.fulfilled_snapshot_milli != line.fulfilled_snapshot_milli
            || self.promised_date != line.promised_date
            || !valid_date(&self.promised_date)
            || !valid_identifier(&self.branch_id)
            || self
                .department_id
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || !valid_text(&self.order_number, 1, TEXT_LIMIT)
            || !valid_order_state(&self.order_state)
            || !valid_identifier(&self.product_id)
            || !valid_text(&self.description, 1, DESCRIPTION_LIMIT)
            || self.fulfilled_milli < 0
            || self.prior_quantity < 0
            || self.remaining_milli < 0
            || self.remaining_milli > self.quantity_milli
            || header.kind != "confirmed"
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SupplierPromiseHistory {
    pub order_id: String,
    pub history: Vec<SupplierPromiseRow>,
    pub lines: Vec<SupplierPromiseLineRow>,
    pub open: Vec<OpenPromiseRow>,
}

pub(crate) fn validate_history(
    company: &CompanyId,
    order_id: &str,
    history: Vec<SupplierPromiseRow>,
    lines: Vec<SupplierPromiseLineRow>,
    open: Vec<OpenPromiseRow>,
) -> Result<SupplierPromiseHistory, BusinessError> {
    if history.len() > HISTORY_LIMIT || lines.len() > LINE_LIMIT || open.len() > OPEN_LIMIT {
        return Err(too_large());
    }
    let mut headers: BTreeMap<&str, &SupplierPromiseRow> = BTreeMap::new();
    for row in &history {
        row.validate(company, order_id)?;
        if headers.insert(row.id.as_str(), row).is_some() {
            return Err(source_invalid());
        }
    }
    let mut promise_lines: BTreeMap<&str, &SupplierPromiseLineRow> = BTreeMap::new();
    for line in &lines {
        line.validate(&headers)?;
        if promise_lines.insert(line.id.as_str(), line).is_some() {
            return Err(source_invalid());
        }
    }
    let mut open_ids = std::collections::BTreeSet::new();
    for row in &open {
        row.validate(company, order_id, &headers, &promise_lines)?;
        if !open_ids.insert(row.id.as_str()) {
            return Err(source_invalid());
        }
    }
    Ok(SupplierPromiseHistory {
        order_id: order_id.to_owned(),
        history,
        lines,
        open,
    })
}
