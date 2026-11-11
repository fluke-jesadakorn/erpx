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
const SOURCE_LINE_LIMIT: usize = 10_000;
const REFERENCE_LIMIT: usize = 160;
const EVIDENCE_LIMIT: usize = 2_000;
const DESCRIPTION_LIMIT: usize = 2_000;
const TEXT_LIMIT: usize = 200;
const NOTE_LIMIT: usize = 20_000;
const NAME_LIMIT: usize = 2_000;

/// Retained order states in which a supplier confirmation can still be
/// recorded. The list is order policy only; purchase admission stays
/// composition-owned.
const RECORDABLE_ORDER_STATES: [&str; 2] = ["confirmed", "partially_fulfilled"];

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

    /// Owner IAM fact from the selected assignment. It permits the detail read
    /// to say whether this actor could record a promise; it is not commercial
    /// purchase admission and never substitutes for the composition-owned
    /// purchased-package check.
    pub fn can_purchase(&self) -> bool {
        self.has("commerce.purchase")
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

/// Database-origin display text. Unlike the supplier-input validators this
/// only bounds UTF-16 length, because retained rows are not re-trimmed by the
/// reader.
fn bounded_text(value: &str, minimum: usize, limit: usize) -> bool {
    (minimum..=limit).contains(&value.encode_utf16().count())
}

fn optional_identifier(value: Option<&str>) -> bool {
    value.is_none_or(valid_identifier)
}

/// The retained purchase-order header (`commerce_orders o.*` joined to the
/// supplier `finance_parties.name`). It is a read fact only and grants no
/// purchase authority.
#[derive(Clone, Debug, Serialize)]
pub struct SupplierPromiseOrderRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub kind: String,
    pub number: String,
    pub party_id: String,
    pub warehouse_id: Option<String>,
    pub currency: String,
    pub order_date: String,
    pub due_date: String,
    pub note: String,
    pub state: String,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub source_id: Option<String>,
    pub version: i64,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub price_list_id: Option<String>,
    pub price_list_version: Option<i64>,
    pub discount_reason: Option<String>,
    pub discount_approval_required: i64,
    pub discount_approved_by: Option<String>,
    pub discount_approved_at: Option<i64>,
    pub supplier_name: String,
}

impl SupplierPromiseOrderRow {
    fn validate(&self, company: &CompanyId, order_id: &str) -> Result<(), BusinessError> {
        if self.id != order_id
            || self.company_id != company.as_str()
            || self.kind != "purchase"
            || !valid_identifier(&self.id)
            || !valid_identifier(&self.party_id)
            || !valid_identifier(&self.created_by)
            || !optional_identifier(self.branch_id.as_deref())
            || !optional_identifier(self.department_id.as_deref())
            || !optional_identifier(self.warehouse_id.as_deref())
            || !optional_identifier(self.approved_by.as_deref())
            || !optional_identifier(self.price_list_id.as_deref())
            || !optional_identifier(self.discount_approved_by.as_deref())
            || !self
                .source_id
                .as_deref()
                .is_none_or(|value| valid_identifier(value))
            || !bounded_text(&self.number, 1, TEXT_LIMIT)
            || !bounded_text(&self.supplier_name, 1, NAME_LIMIT)
            || !bounded_text(&self.note, 0, NOTE_LIMIT)
            || !self
                .discount_reason
                .as_deref()
                .is_none_or(|value| bounded_text(value, 1, NOTE_LIMIT))
            || !valid_currency(&self.currency)
            || !valid_date(&self.order_date)
            || !valid_date(&self.due_date)
            || !valid_order_state(&self.state)
            || !(0..=1).contains(&self.discount_approval_required)
            || self.version < 1
            || self.subtotal_minor < 0
            || self.tax_minor < 0
            || self.price_list_version.is_some_and(|value| value < 1)
            || self.created_at < 0
            || self.updated_at < self.created_at
            || self.discount_approved_at.is_some_and(|value| value < 0)
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

/// Raw `commerce_order_lines` row joined to its product SKU, before the
/// procurement-owned outstanding/unconfirmed projection is applied.
#[derive(Clone, Debug)]
pub struct SupplierPromiseSourceLineRow {
    pub id: String,
    pub order_id: String,
    pub product_id: String,
    pub description: String,
    pub quantity_milli: i64,
    pub fulfilled_milli: i64,
    pub price_minor: i64,
    pub tax_bps: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub ordinal: i64,
    pub list_price_minor: Option<i64>,
    pub discount_bps: i64,
    pub item_type: String,
    pub sku: String,
}

impl SupplierPromiseSourceLineRow {
    fn validate(&self, order_id: &str) -> Result<(), BusinessError> {
        if self.order_id != order_id
            || !valid_identifier(&self.id)
            || !valid_identifier(&self.product_id)
            || !bounded_text(&self.description, 0, DESCRIPTION_LIMIT)
            || !bounded_text(&self.sku, 1, TEXT_LIMIT)
            || !matches!(self.item_type.as_str(), "stock" | "service")
            || self.quantity_milli <= 0
            || self.fulfilled_milli < 0
            || self.fulfilled_milli > self.quantity_milli
            || self.price_minor < 0
            || !(0..=10_000).contains(&self.tax_bps)
            || self.subtotal_minor < 0
            || self.tax_minor < 0
            || !(0..=1_000_000).contains(&self.ordinal)
            || self.list_price_minor.is_some_and(|value| value < 0)
            || !(0..=10_000).contains(&self.discount_bps)
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

/// The retained order-line projection: the Commerce line plus the product SKU
/// and the Procurement-owned still-to-receive/unconfirmed quantities.
#[derive(Clone, Debug, Serialize)]
pub struct SupplierPromiseOrderLineRow {
    pub id: String,
    pub order_id: String,
    pub product_id: String,
    pub description: String,
    pub quantity_milli: i64,
    pub fulfilled_milli: i64,
    pub price_minor: i64,
    pub tax_bps: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub ordinal: i64,
    pub list_price_minor: Option<i64>,
    pub discount_bps: i64,
    pub item_type: String,
    pub sku: String,
    pub outstanding_milli: i64,
    pub unconfirmed_milli: i64,
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

/// Commercial purchased-package overlay. Composition fills all four fields from
/// the retained `readPurchasedAccess(..., capability = "supply.purchasing")`
/// result and passes them in; Procurement never reads grants or mints an
/// entitlement. The owner only intersects `settle_existing` with its own IAM
/// and order-state facts when computing `canRecord`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplierPromisePurchaseAccess {
    pub new_work: bool,
    pub settle_existing: bool,
    pub history: bool,
    pub revision: String,
}

/// The retained supplier-promise detail shape. `order` and `lines` are
/// read-only Commerce purchase facts; `history`/`schedule`/`open` are
/// Procurement promise facts; `purchase` is the composition-supplied package
/// overlay (omitted when enforcement is disabled, exactly like the retained
/// `undefined`). `canRecord` is the owner intersection of the current IAM
/// `commerce.purchase` fact, the order state and `purchase.settleExisting`.
#[derive(Clone, Debug, Serialize)]
pub struct SupplierPromiseDetail {
    pub order: SupplierPromiseOrderRow,
    pub lines: Vec<SupplierPromiseOrderLineRow>,
    pub history: Vec<SupplierPromiseRow>,
    pub schedule: Vec<SupplierPromiseLineRow>,
    pub open: Vec<OpenPromiseRow>,
    #[serde(rename = "canRecord")]
    pub can_record: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purchase: Option<SupplierPromisePurchaseAccess>,
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

/// Retained `promiseDetailLines`: still-to-receive is the unfulfilled order
/// quantity and *unconfirmed* subtracts only currently open supplier promises.
pub(crate) fn detail_lines(
    order_id: &str,
    source: Vec<SupplierPromiseSourceLineRow>,
    open: &[OpenPromiseRow],
) -> Result<Vec<SupplierPromiseOrderLineRow>, BusinessError> {
    if source.len() > SOURCE_LINE_LIMIT {
        return Err(too_large());
    }
    let mut promised: BTreeMap<&str, i64> = BTreeMap::new();
    for row in open {
        let total = promised.entry(row.order_line_id.as_str()).or_default();
        *total = total.checked_add(row.remaining_milli).ok_or_else(source_invalid)?;
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut lines = Vec::with_capacity(source.len());
    for row in source {
        row.validate(order_id)?;
        if !ids.insert(row.id.clone()) {
            return Err(source_invalid());
        }
        let outstanding = (row.quantity_milli - row.fulfilled_milli).max(0);
        let already = promised.get(row.id.as_str()).copied().unwrap_or(0);
        let unconfirmed = (outstanding - already).max(0);
        lines.push(SupplierPromiseOrderLineRow {
            id: row.id,
            order_id: row.order_id,
            product_id: row.product_id,
            description: row.description,
            quantity_milli: row.quantity_milli,
            fulfilled_milli: row.fulfilled_milli,
            price_minor: row.price_minor,
            tax_bps: row.tax_bps,
            subtotal_minor: row.subtotal_minor,
            tax_minor: row.tax_minor,
            ordinal: row.ordinal,
            list_price_minor: row.list_price_minor,
            discount_bps: row.discount_bps,
            item_type: row.item_type,
            sku: row.sku,
            outstanding_milli: outstanding,
            unconfirmed_milli: unconfirmed,
        });
    }
    Ok(lines)
}

/// Owner record policy intersected with the composition-supplied commercial
/// overlay. When composition passes `None` (enforcement disabled) the retained
/// `access?.settleExisting ?? true` semantics apply.
pub(crate) fn can_record(
    actor: &SupplierPromiseReadActor,
    order: &SupplierPromiseOrderRow,
    purchase: Option<&SupplierPromisePurchaseAccess>,
) -> bool {
    actor.can_purchase()
        && RECORDABLE_ORDER_STATES.contains(&order.state.as_str())
        && purchase.is_none_or(|access| access.settle_existing)
}

pub(crate) fn validate_order(
    order: &SupplierPromiseOrderRow,
    company: &CompanyId,
    order_id: &str,
) -> Result<(), BusinessError> {
    order.validate(company, order_id)
}
