//! Native Commerce sales quotation/sales-order document facts.
//!
//! The qualified slice reads Commerce-owned quotation and sales-order headers
//! and lines and performs the owner-local lifecycle transitions that do not
//! move Inventory, post Finance or mutate Quality:
//! submit, return-to-draft, independent quotation approval, draft cancellation,
//! independent confirmed-quotation cancellation and quotation conversion to a
//! draft sales order.
//!
//! Sales-order approval still requires the retained Finance credit guard, and
//! confirmed sales-order cancellation still requires the retained Inventory
//! reservation/reversal guard. Both fail closed instead of weakening those
//! guards. Purchase orders, fulfillment, invoicing, returns and every other
//! Commerce workflow remain retained until their native slices exist.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
    TimestampMillis, UserId,
};
use serde_json::{Value, json};

pub const LINES_LIMIT: usize = 200;
pub const NOTE_LIMIT: usize = 2_000;
pub const REASON_LIMIT: usize = 1_000;
pub const DISCOUNT_REASON_MIN: usize = 5;
pub const RECEIPT_LIMIT: usize = 4 * 1024;

#[derive(Clone, Debug)]
pub struct CommerceOrderActor {
    pub user_id: UserId,
    /// Aliases retained from the server-authorized Identity context.
    pub person_user_ids: Vec<UserId>,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub assignment_id: AssignmentId,
    pub role: Option<String>,
    pub authority_revision: Option<i64>,
    pub assignment_version: Option<i64>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl CommerceOrderActor {
    fn same_person(&self, user_id: &str) -> bool {
        self.user_id.as_str() == user_id
            || self
                .person_user_ids
                .iter()
                .any(|alias| alias.as_str() == user_id)
    }

    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_named_module(&self, module: &str) -> bool {
        self.modules.iter().any(|value| value == module)
    }

    fn require_context(&self, module: &str, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has_named_module(module) {
            return Err(module_forbidden());
        }
        if now.get() >= self.expires_at {
            return Err(work_context_changed());
        }
        Ok(())
    }

    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require_context("sales", now)?;
        if !self.has("commerce.read") {
            return Err(permission_required());
        }
        Ok(())
    }

    pub fn require_write(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require_context("sales", now)?;
        if !self.has("commerce.sales") {
            return Err(permission_required());
        }
        Ok(())
    }

    /// Purchase goods receipt is an Inventory stock action even though this
    /// command owns the Commerce fulfillment. Department-scoped assignments
    /// cannot carry `inventory.prepare`; the caller must select a holding or
    /// branch workspace.
    pub fn require_receipt(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require_context("inventory", now)?;
        if self.department_id.is_some()
            || !self.has("inventory.prepare")
            || !self.has("commerce.read")
        {
            return Err(receipt_permission_required());
        }
        Ok(())
    }

    pub fn require_approve(&self) -> Result<(), BusinessError> {
        if !self.has("commerce.approve") {
            return Err(approval_required());
        }
        Ok(())
    }

    /// Retention-compatible authority projection for audit/outbox readers. It
    /// records the selected assignment and scope; it grants no authority.
    pub fn authority_json(&self) -> String {
        json!({
            "assignmentId": self.assignment_id.as_str(),
            "companyId": self.company_id.as_str(),
            "branchId": self.branch_id.as_ref().map(|value| value.as_str()),
            "departmentId": self.department_id.as_ref().map(|value| value.as_str()),
            "modules": self.modules,
            "permissions": self.permissions,
            "role": self.role,
            "revision": self.authority_revision,
            "assignmentVersion": self.assignment_version,
        })
        .to_string()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderKind {
    Quote,
    Sales,
}

impl OrderKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "quote" => Some(Self::Quote),
            "sales" => Some(Self::Sales),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Quote => "quote",
            Self::Sales => "sales",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderState {
    Draft,
    Submitted,
    Confirmed,
    PartiallyFulfilled,
    Fulfilled,
    Converted,
    Cancelled,
}

impl OrderState {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "submitted" => Some(Self::Submitted),
            "confirmed" => Some(Self::Confirmed),
            "partially_fulfilled" => Some(Self::PartiallyFulfilled),
            "fulfilled" => Some(Self::Fulfilled),
            "converted" => Some(Self::Converted),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Submitted => "submitted",
            Self::Confirmed => "confirmed",
            Self::PartiallyFulfilled => "partially_fulfilled",
            Self::Fulfilled => "fulfilled",
            Self::Converted => "converted",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderAction {
    Submit,
    Approve,
    Return,
    Cancel,
    Convert,
}

impl OrderAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "submit" => Some(Self::Submit),
            "approve" => Some(Self::Approve),
            "return" => Some(Self::Return),
            "cancel" => Some(Self::Cancel),
            "convert" => Some(Self::Convert),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Submit => "submit",
            Self::Approve => "approve",
            Self::Return => "return",
            Self::Cancel => "cancel",
            Self::Convert => "convert",
        }
    }

    pub fn operation(self) -> &'static str {
        match self {
            Self::Submit => "commerce.order.submit",
            Self::Approve => "commerce.order.approve",
            Self::Return => "commerce.order.return",
            Self::Cancel => "commerce.order.cancel",
            Self::Convert => "commerce.order.convert",
        }
    }

    /// Quotation approval requires the independent commerce approver
    /// permission. Confirmed cancellation is checked separately after the
    /// current order state is locked.
    pub fn needs_approver(self) -> bool {
        matches!(self, Self::Approve)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesOrderRow {
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesOrderLineRow {
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
    pub unit: String,
    pub tracking_mode: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesOrderDocument {
    pub order: SalesOrderRow,
    pub lines: Vec<SalesOrderLineRow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommerceOrderReceipt {
    pub id: String,
    pub state: String,
    pub version: i64,
    pub sales_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderCommandReceipt {
    pub request_hash: String,
    pub result_json: String,
    pub operation: Option<String>,
    pub actor_id: Option<String>,
    pub record_id: Option<String>,
    pub authority_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TransitionCommand {
    pub key: CommandKey,
    pub order_id: RecordId,
    pub action: OrderAction,
    pub expected_version: i64,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionPlan {
    pub order_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub action: OrderAction,
    pub expected_version: i64,
    pub expected_state: String,
    pub next_state: String,
    pub approved_by: Option<String>,
    pub set_discount_approval: bool,
    pub clear_discount_approval: bool,
    pub discount_approved_by: Option<String>,
    pub discount_approved_at: Option<i64>,
    pub sales_id: Option<String>,
    pub sales_number: Option<String>,
    pub quote_subtotal_minor: i64,
    pub quote_tax_minor: i64,
    pub actor_id: String,
    pub now: i64,
    pub receipt: CommerceOrderReceipt,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvertedReadback {
    pub order: SalesOrderRow,
    pub line_count: i64,
    pub line_subtotal_minor: i64,
    pub line_tax_minor: i64,
}

pub fn module_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMMERCE_MODULE_FORBIDDEN",
        "Your current role cannot access Sales and Commercial.",
    )
}

pub fn permission_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMMERCE_PERMISSION_REQUIRED",
        "Your selected role does not grant the required Commercial permission.",
    )
}

pub fn receipt_permission_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMMERCE_RECEIPT_PERMISSION_REQUIRED",
        "Your selected role does not grant the Inventory preparation permission for goods receipt.",
    )
}

pub fn approval_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMMERCE_APPROVAL_REQUIRED",
        "Your selected role does not grant Commercial approval.",
    )
}

pub fn work_context_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your selected work context changed. Review the current workspace and role.",
    )
}

pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCE_ORDER_SOURCE_INVALID",
        "The current Commerce order source is invalid.",
    )
}

pub fn too_large() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCE_ORDER_TOO_LARGE",
        "The Commerce order document is too large to load at once.",
    )
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_ORDER_REQUEST_INVALID",
        "Use one bounded Commerce order request.",
    )
}

pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCE_ORDER_SOURCE_INVALID",
        "The recorded Commerce order change could not be read back.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ORDER_NOT_FOUND",
        "Order not found in this workspace.",
    )
}

/// A stale version, non-draft state or changed prerequisite reaches the same
/// generic command-assertion conflict as the original TypeScript boundary.
pub fn version_conflict() -> BusinessError {
    transition_conflict()
}

/// Legacy Workflow maps a transition version/state assertion failure to the
/// same generic conflict as other failed command assertions.
pub(crate) fn transition_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONFLICT",
        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to a different request.",
    )
}

pub fn independent_approver_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "INDEPENDENT_APPROVER_REQUIRED",
        "A different manager must approve this document.",
    )
}

pub fn independent_canceller_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "INDEPENDENT_CANCELLER_REQUIRED",
        "A different manager must cancel an approved document.",
    )
}

pub fn discount_reason_required() -> BusinessError {
    BusinessError::invalid(
        "DISCOUNT_APPROVAL_REASON_REQUIRED",
        "Record the independent discount approval reason.",
    )
}

pub fn pricing_discount_reason_required() -> BusinessError {
    BusinessError::invalid(
        "DISCOUNT_REASON_REQUIRED",
        "Explain why this order exceeds the automatic discount threshold.",
    )
}

pub fn quote_required() -> BusinessError {
    BusinessError::invalid("QUOTE_REQUIRED", "Only a quotation can be converted.")
}

pub fn order_reversal_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ORDER_REVERSAL_REQUIRED",
        "This order has stock movements. Use the return workflow instead of cancellation.",
    )
}

pub fn sales_approval_finance_guard() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMERCE_SALES_APPROVAL_FINANCE_GUARD",
        "Sales-order approval still requires the retained Finance credit-limit guard.",
    )
}

pub fn sales_credit_limit_exceeded() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONFLICT",
        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
    )
}

pub(crate) fn valid_identifier(value: &str) -> bool {
    RecordId::new(value).is_ok()
}

/// ECMAScript `String.prototype.trim` whitespace: WhiteSpace plus
/// LineTerminator. Rust's `str::trim` additionally strips U+0085 (White_Space
/// in Unicode but not in JavaScript `trim`) and keeps U+FEFF, so the exact
/// set is spelled out to match the retained Zod `trim()` boundary.
pub(crate) fn js_trim(value: &str) -> &str {
    value.trim_matches(|character| {
        matches!(character,
            '\u{0009}'..='\u{000D}'
                | '\u{0020}'
                | '\u{00A0}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}')
    })
}

/// Plain JavaScript `String.prototype.length` counts UTF-16 code units,
/// retained only for the integer discount-minimum check.
pub(crate) fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

/// Zod string limits count Unicode scalar values (code points), not UTF-8
/// bytes or UTF-16 code units. Thai text (3 bytes, 1 code point) and astral
/// text (1 code point) must be measured the way the retained TypeScript
/// gateway measures them.
pub(crate) fn cp_len(value: &str) -> usize {
    value.chars().count()
}

fn valid_text(value: &str, limit: usize) -> bool {
    !value.is_empty() && cp_len(value) <= limit && !value.chars().any(char::is_control)
}

fn valid_note(value: &str, limit: usize) -> bool {
    cp_len(value) <= limit
        && !value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
}

fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

pub(crate) fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return false;
    }
    let year = value[0..4].parse::<i32>().ok();
    let month = value[5..7].parse::<u32>().ok();
    let day = value[8..10].parse::<u32>().ok();
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return false;
    };
    if !(1..=12).contains(&month) || day == 0 {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    day <= days
}

fn valid_timestamp_pair(created_at: i64, updated_at: i64) -> bool {
    created_at >= 0 && updated_at >= created_at
}

fn scope_allows(actor_scope: Option<&str>, source: Option<&str>) -> bool {
    actor_scope.is_none() || actor_scope == source
}

impl SalesOrderRow {
    pub fn kind(&self) -> Option<OrderKind> {
        OrderKind::parse(&self.kind)
    }

    pub fn state(&self) -> Option<OrderState> {
        OrderState::parse(&self.state)
    }

    pub fn validate(&self, actor: &CommerceOrderActor) -> Result<(), BusinessError> {
        self.validate_shape()?;
        if self.company_id != actor.company_id.as_str()
            || !scope_allows(
                actor.branch_id.as_ref().map(|value| value.as_str()),
                self.branch_id.as_deref(),
            )
            || !scope_allows(
                actor.department_id.as_ref().map(|value| value.as_str()),
                self.department_id.as_deref(),
            )
        {
            return Err(source_invalid());
        }
        Ok(())
    }

    pub fn validate_shape(&self) -> Result<(), BusinessError> {
        let Some(kind) = self.kind() else {
            return Err(source_invalid());
        };
        let Some(state) = self.state() else {
            return Err(source_invalid());
        };
        if !valid_identifier(&self.id)
            || !valid_text(&self.number, 64)
            || !valid_identifier(&self.party_id)
            || self
                .warehouse_id
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || !valid_currency(&self.currency)
            || !valid_date(&self.order_date)
            || !valid_date(&self.due_date)
            || self.due_date < self.order_date
            || !valid_note(&self.note, NOTE_LIMIT)
            || self.subtotal_minor < 0
            || self.tax_minor < 0
            || self
                .source_id
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || self.version < 1
            || !valid_identifier(&self.created_by)
            || self
                .approved_by
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || !valid_timestamp_pair(self.created_at, self.updated_at)
            || self
                .price_list_id
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || self
                .price_list_version
                .is_some_and(|value| value < 1 || self.price_list_id.is_none())
            || self
                .discount_reason
                .as_deref()
                .is_some_and(|value| !valid_note(value, REASON_LIMIT))
            || !matches!(self.discount_approval_required, 0 | 1)
            || self
                .discount_approved_by
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || self.discount_approved_at.is_some_and(|value| value < 0)
            || (state == OrderState::Converted && kind != OrderKind::Quote)
        {
            return Err(source_invalid());
        }
        Ok(())
    }

    pub fn to_wire(&self) -> Value {
        json!({
            "id": self.id,
            "company_id": self.company_id,
            "branch_id": self.branch_id,
            "department_id": self.department_id,
            "kind": self.kind,
            "number": self.number,
            "party_id": self.party_id,
            "warehouse_id": self.warehouse_id,
            "currency": self.currency,
            "order_date": self.order_date,
            "due_date": self.due_date,
            "note": self.note,
            "state": self.state,
            "subtotal_minor": self.subtotal_minor,
            "tax_minor": self.tax_minor,
            "source_id": self.source_id,
            "version": self.version,
            "created_by": self.created_by,
            "approved_by": self.approved_by,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "price_list_id": self.price_list_id,
            "price_list_version": self.price_list_version,
            "discount_reason": self.discount_reason,
            "discount_approval_required": self.discount_approval_required,
            "discount_approved_by": self.discount_approved_by,
            "discount_approved_at": self.discount_approved_at,
        })
    }
}

impl SalesOrderLineRow {
    fn validate(&self, order: &SalesOrderRow) -> Result<(), BusinessError> {
        if !valid_identifier(&self.id)
            || self.order_id != order.id
            || !valid_identifier(&self.product_id)
            || !valid_text(&self.description, NOTE_LIMIT)
            || self.quantity_milli <= 0
            || self.fulfilled_milli < 0
            || self.fulfilled_milli > self.quantity_milli
            || self.price_minor < 0
            || !(0..=10_000).contains(&self.tax_bps)
            || self.subtotal_minor < 0
            || self.tax_minor < 0
            || self.ordinal < 0
            || self.list_price_minor.is_some_and(|value| value < 0)
            || self
                .list_price_minor
                .is_some_and(|value| value < self.price_minor)
            || !(0..=10_000).contains(&self.discount_bps)
            || !matches!(self.item_type.as_str(), "stock" | "service")
            || !valid_text(&self.sku, 64)
            || !valid_text(&self.unit, 30)
            || !matches!(self.tracking_mode.as_str(), "none" | "lot" | "serial")
            || (self.item_type == "service" && self.tracking_mode != "none")
        {
            return Err(source_invalid());
        }
        Ok(())
    }

    pub fn to_wire(&self) -> Value {
        json!({
            "id": self.id,
            "order_id": self.order_id,
            "product_id": self.product_id,
            "description": self.description,
            "quantity_milli": self.quantity_milli,
            "fulfilled_milli": self.fulfilled_milli,
            "price_minor": self.price_minor,
            "tax_bps": self.tax_bps,
            "subtotal_minor": self.subtotal_minor,
            "tax_minor": self.tax_minor,
            "ordinal": self.ordinal,
            "list_price_minor": self.list_price_minor,
            "discount_bps": self.discount_bps,
            "item_type": self.item_type,
            "sku": self.sku,
            "unit": self.unit,
            "tracking_mode": self.tracking_mode,
        })
    }
}

impl SalesOrderDocument {
    pub fn to_wire(&self) -> Value {
        json!({
            "order": self.order.to_wire(),
            "lines": self.lines.iter().map(SalesOrderLineRow::to_wire).collect::<Vec<_>>(),
        })
    }
}

impl CommerceOrderReceipt {
    pub fn to_wire(&self) -> Value {
        json!({
            "id": self.id,
            "state": self.state,
            "version": self.version,
            "salesId": self.sales_id,
        })
    }
}

impl TransitionCommand {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.expected_version < 1 {
            return Err(request_invalid());
        }
        if let Some(reason) = &self.reason {
            let trimmed = js_trim(reason);
            if cp_len(trimmed) > REASON_LIMIT || !valid_note(trimmed, REASON_LIMIT) {
                return Err(request_invalid());
            }
        }
        Ok(())
    }
}

/// Validate one bounded owner document. Duplicate ids, out-of-order lines,
/// foreign scope and totals that do not match the stored lines are rejected
/// before presentation.
pub fn validate_document(
    actor: &CommerceOrderActor,
    order: SalesOrderRow,
    lines: Vec<SalesOrderLineRow>,
    now: TimestampMillis,
) -> Result<SalesOrderDocument, BusinessError> {
    actor.require_read(now)?;
    order.validate(actor)?;
    if lines.is_empty() || lines.len() > LINES_LIMIT {
        return Err(if lines.is_empty() {
            source_invalid()
        } else {
            too_large()
        });
    }
    let mut seen_ids = std::collections::BTreeSet::new();
    let mut seen_products = std::collections::BTreeSet::new();
    let mut previous = None;
    let mut subtotal = 0_i64;
    let mut tax = 0_i64;
    for line in &lines {
        line.validate(&order)?;
        if !seen_ids.insert(line.id.as_str()) || !seen_products.insert(line.product_id.as_str()) {
            return Err(source_invalid());
        }
        if previous.is_some_and(|value| line.ordinal <= value) {
            return Err(source_invalid());
        }
        previous = Some(line.ordinal);
        subtotal = subtotal
            .checked_add(line.subtotal_minor)
            .ok_or_else(source_invalid)?;
        tax = tax.checked_add(line.tax_minor).ok_or_else(source_invalid)?;
    }
    if subtotal != order.subtotal_minor
        || tax != order.tax_minor
        || (lines.iter().any(|line| line.item_type == "stock") && order.warehouse_id.is_none())
    {
        return Err(source_invalid());
    }
    Ok(SalesOrderDocument { order, lines })
}

pub fn transition_identity(actor: &CommerceOrderActor, command: &TransitionCommand) -> String {
    json!({
        "user": actor.user_id.as_str(),
        "assignment": actor.assignment_id.as_str(),
        "company": actor.company_id.as_str(),
        "branch": actor.branch_id.as_ref().map(|value| value.as_str()),
        "department": actor.department_id.as_ref().map(|value| value.as_str()),
        "operation": command.action.operation(),
        "input": {
            "id": command.order_id.as_str(),
            "action": command.action.as_str(),
            "expectedVersion": command.expected_version,
            "reason": command.reason.as_deref().map(js_trim),
        },
    })
    .to_string()
}

/// Authorize and plan one exact-version transition. This function performs no
/// I/O; the application has already locked the scoped order and counted
/// fulfillments.
/// Which cross-owner guards are available to this transition. `Retained`
/// keeps the fail-closed quote-only slice; `Native` requires the caller to
/// provide the Finance credit and Inventory cancellation ports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionGuardMode {
    Retained,
    Native,
}

pub fn authorize_transition(
    actor: &CommerceOrderActor,
    command: &TransitionCommand,
    order: &SalesOrderRow,
    fulfillment_count: i64,
    request_hash: &str,
    now: TimestampMillis,
) -> Result<TransitionPlan, BusinessError> {
    authorize_transition_with_guard_mode(
        actor,
        command,
        order,
        fulfillment_count,
        request_hash,
        now,
        TransitionGuardMode::Retained,
    )
}

/// Match the legacy actor-facing transition checks before version/CAS.
/// Current database authority is still checked separately by the application.
pub(crate) fn validate_transition_actor(
    actor: &CommerceOrderActor,
    command: &TransitionCommand,
    order: &SalesOrderRow,
) -> Result<(), BusinessError> {
    order.validate(actor)?;
    let state = order.state().ok_or_else(source_invalid)?;
    if command.action == OrderAction::Approve
        && (!actor.has("commerce.approve") || actor.same_person(&order.created_by))
    {
        return Err(independent_approver_required());
    }
    if command.action == OrderAction::Cancel
        && matches!(
            state,
            OrderState::Confirmed | OrderState::PartiallyFulfilled | OrderState::Fulfilled
        )
        && (!actor.has("commerce.approve") || actor.same_person(&order.created_by))
    {
        return Err(independent_canceller_required());
    }
    Ok(())
}

pub(crate) fn validate_transition_approval_reason(
    command: &TransitionCommand,
    order: &SalesOrderRow,
) -> Result<(), BusinessError> {
    if command.action == OrderAction::Approve
        && order.discount_approval_required == 1
        && !command
            .reason
            .as_deref()
            .map(js_trim)
            .is_some_and(|r| utf16_len(r) >= DISCOUNT_REASON_MIN && cp_len(r) <= REASON_LIMIT)
    {
        return Err(discount_reason_required());
    }
    Ok(())
}

pub fn authorize_transition_with_guard_mode(
    actor: &CommerceOrderActor,
    command: &TransitionCommand,
    order: &SalesOrderRow,
    fulfillment_count: i64,
    request_hash: &str,
    now: TimestampMillis,
    guard_mode: TransitionGuardMode,
) -> Result<TransitionPlan, BusinessError> {
    actor.require_write(now)?;
    validate_transition_actor(actor, command, order)?;
    validate_transition_approval_reason(command, order)?;
    let state = order.state().ok_or_else(source_invalid)?;
    let kind = order.kind().ok_or_else(source_invalid)?;
    if command.action == OrderAction::Convert && kind != OrderKind::Quote {
        return Err(quote_required());
    }
    if command.action == OrderAction::Cancel
        && matches!(
            state,
            OrderState::Confirmed | OrderState::PartiallyFulfilled | OrderState::Fulfilled
        )
        && fulfillment_count != 0
    {
        return Err(order_reversal_required());
    }
    if order.version != command.expected_version {
        return Err(transition_conflict());
    }
    if state == OrderState::Converted && command.action != OrderAction::Convert {
        return Err(transition_conflict());
    }
    let reason = command.reason.as_deref().map(js_trim);
    let expected_state;
    let next_state;
    let mut approved_by = order.approved_by.clone();
    let mut set_discount_approval = false;
    let mut clear_discount_approval = false;
    let mut discount_approved_by = order.discount_approved_by.clone();
    let mut discount_approved_at = order.discount_approved_at;
    let mut sales_id = None;
    let mut sales_number = None;
    match command.action {
        OrderAction::Submit => {
            expected_state = OrderState::Draft;
            next_state = OrderState::Submitted;
        }
        OrderAction::Return => {
            expected_state = OrderState::Submitted;
            next_state = OrderState::Draft;
            clear_discount_approval = true;
            discount_approved_by = None;
            discount_approved_at = None;
        }
        OrderAction::Approve => {
            if kind != OrderKind::Quote && guard_mode == TransitionGuardMode::Retained {
                return Err(sales_approval_finance_guard());
            }
            expected_state = OrderState::Submitted;
            next_state = OrderState::Confirmed;
            actor.require_approve()?;
            if actor.same_person(&order.created_by) {
                return Err(independent_approver_required());
            }
            if order.discount_approval_required == 1 {
                let reason = reason.filter(|value| {
                    utf16_len(value) >= DISCOUNT_REASON_MIN && cp_len(value) <= REASON_LIMIT
                });
                if reason.is_none() {
                    return Err(discount_reason_required());
                }
                set_discount_approval = true;
                discount_approved_by = Some(actor.user_id.as_str().to_owned());
                discount_approved_at = Some(now.get());
            }
            approved_by = Some(actor.user_id.as_str().to_owned());
        }
        OrderAction::Cancel => {
            if state == OrderState::Draft {
                expected_state = OrderState::Draft;
                next_state = OrderState::Cancelled;
            } else if state == OrderState::Confirmed
                && (kind == OrderKind::Quote || guard_mode == TransitionGuardMode::Native)
            {
                expected_state = OrderState::Confirmed;
                next_state = OrderState::Cancelled;
                actor.require_approve()?;
                if actor.same_person(&order.created_by) {
                    return Err(independent_canceller_required());
                }
                if fulfillment_count != 0 {
                    return Err(order_reversal_required());
                }
            } else if state == OrderState::Confirmed && kind == OrderKind::Sales {
                return Err(order_reversal_required());
            } else {
                return Err(transition_conflict());
            }
        }
        OrderAction::Convert => {
            if kind != OrderKind::Quote {
                return Err(quote_required());
            }
            if state != OrderState::Confirmed {
                return Err(transition_conflict());
            }
            expected_state = OrderState::Confirmed;
            next_state = OrderState::Converted;
            sales_id = Some(format!("order-{}", command.key.as_str()));
            sales_number = Some(format!(
                "SO-{}",
                request_hash
                    .get(..24)
                    .ok_or_else(source_invalid)?
                    .to_ascii_uppercase()
            ));
        }
    };
    if order.state() != Some(expected_state) {
        return Err(transition_conflict());
    }
    let receipt = CommerceOrderReceipt {
        id: order.id.clone(),
        state: next_state.as_str().into(),
        version: command.expected_version + 1,
        sales_id: sales_id.clone(),
    };
    Ok(TransitionPlan {
        order_id: order.id.clone(),
        company_id: order.company_id.clone(),
        branch_id: order.branch_id.clone(),
        department_id: order.department_id.clone(),
        action: command.action,
        expected_version: command.expected_version,
        expected_state: expected_state.as_str().into(),
        next_state: next_state.as_str().into(),
        approved_by,
        set_discount_approval,
        clear_discount_approval,
        discount_approved_by,
        discount_approved_at,
        sales_id,
        sales_number,
        quote_subtotal_minor: order.subtotal_minor,
        quote_tax_minor: order.tax_minor,
        actor_id: actor.user_id.as_str().to_owned(),
        now: now.get(),
        receipt,
    })
}

fn parse_receipt_result(value: &str) -> Result<CommerceOrderReceipt, BusinessError> {
    if value.len() > RECEIPT_LIMIT {
        return Err(source_invalid());
    }
    let parsed: Value = serde_json::from_str(value).map_err(|_| source_invalid())?;
    let Some(object) = parsed.as_object() else {
        return Err(source_invalid());
    };
    if object.len() != 4
        || !object.contains_key("id")
        || !object.contains_key("state")
        || !object.contains_key("version")
        || !object.contains_key("salesId")
    {
        return Err(source_invalid());
    }
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(source_invalid)?;
    let state = object
        .get("state")
        .and_then(Value::as_str)
        .ok_or_else(source_invalid)?;
    let version = object
        .get("version")
        .and_then(Value::as_i64)
        .ok_or_else(source_invalid)?;
    let sales_id = match object.get("salesId") {
        Some(Value::Null) => None,
        Some(Value::String(value)) if valid_identifier(value) => Some(value.clone()),
        _ => return Err(source_invalid()),
    };
    if !valid_identifier(id)
        || OrderState::parse(state).is_none()
        || version < 1
        || (state == "converted") != sales_id.is_some()
    {
        return Err(source_invalid());
    }
    Ok(CommerceOrderReceipt {
        id: id.to_owned(),
        state: state.to_owned(),
        version,
        sales_id,
    })
}

pub fn expected_receipt_state(action: OrderAction) -> &'static str {
    match action {
        OrderAction::Submit => "submitted",
        OrderAction::Approve => "confirmed",
        OrderAction::Return => "draft",
        OrderAction::Cancel => "cancelled",
        OrderAction::Convert => "converted",
    }
}

pub fn replay(
    receipt: Option<OrderCommandReceipt>,
    request_hash: &str,
    order_id: &str,
    expected_version: i64,
    action: OrderAction,
) -> Result<Option<CommerceOrderReceipt>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_hash != request_hash {
        return Err(idempotency_conflict());
    }
    let parsed = parse_receipt_result(&receipt.result_json)?;
    if parsed.id != order_id
        || parsed.state != expected_receipt_state(action)
        || parsed.version != expected_version + 1
        || parsed.sales_id.is_some() != (action == OrderAction::Convert)
    {
        return Err(source_invalid());
    }
    Ok(Some(parsed))
}

pub fn validate_hash(value: &str) -> Result<(), BusinessError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(source_invalid());
    }
    Ok(())
}

pub fn validate_readback(plan: &TransitionPlan, row: &SalesOrderRow) -> Result<(), BusinessError> {
    // Scope was already enforced under lock and the persisted row is re-read
    // through the same scoped query; this is a structural readback check.
    row.validate_shape()?;
    if row.company_id != plan.company_id
        || row.branch_id != plan.branch_id
        || row.department_id != plan.department_id
        || row.id != plan.order_id
        || row.state != plan.next_state
        || row.version != plan.expected_version + 1
        || row.approved_by != plan.approved_by
        || row.discount_approved_by != plan.discount_approved_by
        || row.discount_approved_at != plan.discount_approved_at
    {
        return Err(readback_invalid());
    }
    Ok(())
}

pub fn validate_conversion_readback(
    plan: &TransitionPlan,
    quote: &SalesOrderRow,
    converted: &ConvertedReadback,
    copied_lines: u64,
) -> Result<(), BusinessError> {
    let row = &converted.order;
    row.validate_shape()?;
    if copied_lines == 0
        || converted.line_count != copied_lines as i64
        || converted.line_subtotal_minor != plan.quote_subtotal_minor
        || converted.line_tax_minor != plan.quote_tax_minor
        || Some(row.id.as_str()) != plan.sales_id.as_deref()
        || row.company_id != plan.company_id
        || row.branch_id != plan.branch_id
        || row.department_id != plan.department_id
        || row.kind != "sales"
        || row.state != "draft"
        || row.version != 1
        || row.source_id.as_deref() != Some(plan.order_id.as_str())
        || row.number != plan.sales_number.clone().unwrap_or_default()
        || row.party_id != quote.party_id
        || row.warehouse_id != quote.warehouse_id
        || row.currency != quote.currency
        || row.order_date != quote.order_date
        || row.due_date != quote.due_date
        || row.note != quote.note
        || row.subtotal_minor != quote.subtotal_minor
        || row.tax_minor != quote.tax_minor
        || row.price_list_id != quote.price_list_id
        || row.price_list_version != quote.price_list_version
        || row.discount_reason != quote.discount_reason
        || row.discount_approval_required != quote.discount_approval_required
        || row.discount_approved_by != quote.discount_approved_by
        || row.discount_approved_at != quote.discount_approved_at
        || row.created_by != plan.actor_id
        || row.approved_by.is_some()
        || row.created_at != plan.now
        || row.updated_at != plan.now
    {
        return Err(readback_invalid());
    }
    Ok(())
}

pub const SAVE_LINES_LIMIT: usize = 20;
const MAX_SAFE_INTEGER: i128 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveOrderLineInput {
    pub product_id: String,
    pub quantity: String,
    pub price: String,
    pub tax_bps: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveOrderInput {
    pub kind: OrderKind,
    pub party_id: String,
    pub warehouse_id: Option<String>,
    pub order_date: String,
    pub due_date: String,
    pub note: String,
    pub price_list_id: Option<String>,
    pub discount_reason: Option<String>,
    pub lines: Vec<SaveOrderLineInput>,
    pub expected_version: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct SaveOrderCommand {
    pub key: CommandKey,
    pub order_id: Option<RecordId>,
    pub input: SaveOrderInput,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveOrderReceipt {
    pub id: String,
    pub version: i64,
}

impl SaveOrderReceipt {
    pub fn to_wire(&self) -> Value {
        json!({ "id": self.id, "version": self.version })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveOrderProduct {
    pub id: String,
    pub name: String,
    pub item_type: String,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveOrderWarehouse {
    pub id: String,
    pub branch_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveOrderPriceItem {
    pub product_id: String,
    pub price_minor: i64,
    pub tax_bps: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveOrderPriceList {
    pub id: String,
    pub version: i64,
    pub max_auto_discount_bps: i64,
    pub items: Vec<SaveOrderPriceItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveOrderLinePlan {
    pub id: String,
    pub order_id: String,
    pub product_id: String,
    pub description: String,
    pub quantity_milli: i64,
    pub price_minor: i64,
    pub tax_bps: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub ordinal: i64,
    pub list_price_minor: i64,
    pub discount_bps: i64,
    pub item_type: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveOrderPlan {
    pub create: bool,
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
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub price_list_id: Option<String>,
    pub price_list_version: Option<i64>,
    pub discount_reason: Option<String>,
    pub discount_approval_required: i64,
    pub version: i64,
    pub created_by: String,
    pub updated_at: i64,
    pub lines: Vec<SaveOrderLinePlan>,
}

pub fn save_setup_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMERCE_SETUP_REQUIRED",
        "Configure the company posting accounts before processing orders.",
    )
}

/// The original save path validates the party through a command assertion, so
/// a foreign/missing party is a generic conflict, not a field-validation error.
pub fn party_invalid() -> BusinessError {
    transition_conflict()
}

pub fn warehouse_invalid() -> BusinessError {
    BusinessError::invalid(
        "WAREHOUSE_INVALID",
        "Choose an active warehouse in this workspace.",
    )
}

pub fn warehouse_required() -> BusinessError {
    BusinessError::invalid("WAREHOUSE_REQUIRED", "Choose a warehouse for stock items.")
}

pub fn product_invalid() -> BusinessError {
    BusinessError::invalid("PRODUCT_INVALID", "Choose an active product.")
}

pub fn order_kind_locked() -> BusinessError {
    BusinessError::invalid("ORDER_KIND_LOCKED", "The document type cannot change.")
}

pub fn invalid_dates() -> BusinessError {
    BusinessError::invalid("INVALID_DATES", "Due date must be on or after order date.")
}

pub fn duplicate_product() -> BusinessError {
    BusinessError::invalid("DUPLICATE_PRODUCT", "Use one line per product.")
}

pub fn order_amount_invalid() -> BusinessError {
    BusinessError::invalid(
        "ORDER_AMOUNT_INVALID",
        "Order total must be positive and within supported limits.",
    )
}

pub fn price_list_invalid() -> BusinessError {
    BusinessError::invalid(
        "PRICE_LIST_INVALID",
        "Choose an active price list valid for this customer and order date.",
    )
}

pub fn price_list_item_required() -> BusinessError {
    BusinessError::invalid(
        "PRICE_LIST_ITEM_REQUIRED",
        "Every order product must exist on the selected price list.",
    )
}

pub fn price_list_tax_required() -> BusinessError {
    BusinessError::invalid(
        "PRICE_LIST_TAX_REQUIRED",
        "Use the approved tax treatment captured by the price list.",
    )
}

pub fn order_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONFLICT",
        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
    )
}

impl SaveOrderCommand {
    pub fn operation(&self) -> &'static str {
        if self.order_id.is_some() {
            "commerce.order.update"
        } else {
            "commerce.order.create"
        }
    }

    pub fn record_id(&self) -> String {
        self.order_id
            .as_ref()
            .map(|id| id.as_str().to_owned())
            .unwrap_or_else(|| format!("order-{}", self.key.as_str()))
    }

    pub fn expected_version(&self) -> Option<i64> {
        self.input.expected_version
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        let input = &self.input;
        if !valid_identifier(&input.party_id)
            || input
                .warehouse_id
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || !valid_date(&input.order_date)
            || !valid_date(&input.due_date)
            || !valid_note(&input.note, NOTE_LIMIT)
            || input
                .price_list_id
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || input
                .discount_reason
                .as_deref()
                .is_some_and(|value| !valid_note(js_trim(value), REASON_LIMIT))
            || input.lines.is_empty()
            || input.lines.len() > SAVE_LINES_LIMIT
            || input
                .expected_version
                .is_some_and(|value| !(1..=MAX_SAFE_INTEGER as i64).contains(&value))
        {
            return Err(request_invalid());
        }
        if input.due_date < input.order_date {
            return Err(invalid_dates());
        }
        let mut products = std::collections::BTreeSet::new();
        for line in &input.lines {
            if !valid_identifier(&line.product_id)
                || !(0..=10_000).contains(&line.tax_bps)
                || parse_decimal_units(&line.quantity, 3)?.is_none()
                || parse_decimal_units(&line.price, 2)?.is_none()
            {
                return Err(request_invalid());
            }
            if !products.insert(line.product_id.as_str()) {
                return Err(duplicate_product());
            }
            if parse_decimal_units(&line.quantity, 3)? == Some(0) {
                return Err(request_invalid());
            }
        }
        Ok(())
    }
}

/// Parse the bounded decimal strings accepted by the retained Zod schema.
/// Returns `Ok(None)` when the text does not match the schema and
/// `Ok(Some(units))` for the exact scaled integer.
pub(crate) fn parse_decimal_units(value: &str, scale: u32) -> Result<Option<i64>, BusinessError> {
    let value = js_trim(value);
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return Ok(None);
    }
    let mut dot = None;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'.' {
            if dot.is_some() {
                return Ok(None);
            }
            dot = Some(index);
        } else if !byte.is_ascii_digit() {
            return Ok(None);
        }
    }
    let (whole, fraction) = match dot {
        Some(index) => (&value[..index], &value[index + 1..]),
        None => (value, ""),
    };
    if whole.is_empty() || (dot.is_some() && fraction.is_empty()) {
        return Ok(None);
    }
    if fraction.len() > scale as usize
        && fraction[scale as usize..].bytes().any(|byte| byte != b'0')
    {
        return Ok(None);
    }
    let mut units: i128 = 0;
    for byte in whole.bytes() {
        units = units * 10 + i128::from(byte - b'0');
        if units > MAX_SAFE_INTEGER {
            return Ok(None);
        }
    }
    let factor = 10_i128.pow(scale);
    units *= factor;
    if units > MAX_SAFE_INTEGER {
        return Ok(None);
    }
    let padded = format!(
        "{:0<width$}",
        &fraction[..fraction.len().min(scale as usize)],
        width = scale as usize
    );
    for byte in padded.bytes() {
        units += i128::from(byte - b'0');
    }
    if units > MAX_SAFE_INTEGER {
        return Ok(None);
    }
    i64::try_from(units)
        .map(Some)
        .map_err(|_| request_invalid())
}

fn scaled_amount(amount: i64, numerator: i64, denominator: i64) -> Result<i64, BusinessError> {
    if amount < 0 || numerator < 0 || denominator <= 0 {
        return Err(order_amount_invalid());
    }
    let value = (i128::from(amount) * i128::from(numerator) + i128::from(denominator / 2))
        / i128::from(denominator);
    if value > MAX_SAFE_INTEGER {
        return Err(order_amount_invalid());
    }
    i64::try_from(value).map_err(|_| order_amount_invalid())
}

fn line_amounts(
    quantity_milli: i64,
    price_minor: i64,
    tax_bps: i64,
) -> Result<(i64, i64), BusinessError> {
    let subtotal = scaled_amount(price_minor, quantity_milli, 1_000)?;
    let tax = scaled_amount(subtotal, tax_bps, 10_000)?;
    Ok((subtotal, tax))
}

pub fn sales_order_number(key: &CommandKey) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(key.as_str().as_bytes());
    let mut suffix = String::with_capacity(24);
    for byte in &digest[..12] {
        use std::fmt::Write;
        let _ = write!(suffix, "{byte:02X}");
    }
    format!("SO-{suffix}")
}

pub fn save_identity(actor: &CommerceOrderActor, command: &SaveOrderCommand) -> String {
    json!({
        "user": actor.user_id.as_str(),
        "assignment": actor.assignment_id.as_str(),
        "company": actor.company_id.as_str(),
        "branch": actor.branch_id.as_ref().map(|value| value.as_str()),
        "department": actor.department_id.as_ref().map(|value| value.as_str()),
        "operation": command.operation(),
        "recordId": command.record_id(),
        "input": {
            "kind": command.input.kind.as_str(),
            "partyId": command.input.party_id,
            "warehouseId": command.input.warehouse_id,
            "orderDate": command.input.order_date,
            "dueDate": command.input.due_date,
            "note": command.input.note,
            "priceListId": command.input.price_list_id,
            "discountReason": command.input.discount_reason.as_deref().map(js_trim),
            "expectedVersion": command.input.expected_version,
            "lines": command.input.lines.iter().map(|line| json!({
                "productId": line.product_id,
                "quantity": line.quantity,
                "price": line.price,
                "taxBps": line.tax_bps,
            })).collect::<Vec<_>>(),
        },
    })
    .to_string()
}

#[derive(Debug)]
pub(crate) struct PreparedSaveValues {
    pub(crate) subtotal: i128,
    pub(crate) tax_total: i128,
    pub(crate) requires_approval: bool,
    pub(crate) reason: Option<String>,
    pub(crate) lines: Vec<SaveOrderLinePlan>,
}

/// Calculate owner facts independently of later party and version guards.
/// This does not authorize or perform any write.
pub(crate) fn prepare_save_values(
    command: &SaveOrderCommand,
    warehouse: &Option<SaveOrderWarehouse>,
    products: &[Option<SaveOrderProduct>],
    price_list: &Option<SaveOrderPriceList>,
) -> Result<PreparedSaveValues, BusinessError> {
    let input = &command.input;
    if input.warehouse_id.is_some() && warehouse.is_none() {
        return Err(warehouse_invalid());
    }
    if products.len() != input.lines.len() {
        return Err(request_invalid());
    }
    let price_items = price_list
        .as_ref()
        .map(|list| {
            list.items
                .iter()
                .map(|item| (item.product_id.as_str(), item))
                .collect::<std::collections::BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut subtotal = 0_i128;
    let mut tax_total = 0_i128;
    let mut requires_approval = false;
    let mut lines = Vec::with_capacity(input.lines.len());
    for (index, line) in input.lines.iter().enumerate() {
        let product = products[index].as_ref().ok_or_else(product_invalid)?;
        if !matches!(product.item_type.as_str(), "stock" | "service") {
            return Err(product_invalid());
        }
        if product.item_type == "stock" && warehouse.is_none() {
            return Err(warehouse_required());
        }
        let quantity_milli = parse_decimal_units(&line.quantity, 3)?
            .filter(|value| *value > 0)
            .ok_or_else(request_invalid)?;
        let price_minor = parse_decimal_units(&line.price, 2)?
            .filter(|value| *value >= 0)
            .ok_or_else(request_invalid)?;
        let (list_price, discount_bps) = match &price_list {
            Some(list) => {
                let item = price_items
                    .get(line.product_id.as_str())
                    .ok_or_else(price_list_item_required)?;
                if item.tax_bps != line.tax_bps {
                    return Err(price_list_tax_required());
                }
                let base = item.price_minor;
                let discount = if base > 0 && price_minor < base {
                    ((i128::from(base) - i128::from(price_minor)) * 10_000 / i128::from(base))
                        .try_into()
                        .map_err(|_| order_amount_invalid())?
                } else {
                    0
                };
                let _ = list;
                (base, discount)
            }
            None => (price_minor, 0),
        };
        if discount_bps > 10_000 {
            return Err(order_amount_invalid());
        }
        let (line_subtotal, line_tax) = line_amounts(quantity_milli, price_minor, line.tax_bps)?;
        subtotal += i128::from(line_subtotal);
        tax_total += i128::from(line_tax);
        requires_approval |= price_list
            .as_ref()
            .is_some_and(|list| discount_bps > list.max_auto_discount_bps);
        lines.push(SaveOrderLinePlan {
            id: format!("{}-line-{index}", command.record_id()),
            order_id: command.record_id(),
            product_id: product.id.clone(),
            description: product.name.clone(),
            quantity_milli,
            price_minor,
            tax_bps: line.tax_bps,
            subtotal_minor: line_subtotal,
            tax_minor: line_tax,
            ordinal: index as i64,
            list_price_minor: list_price,
            discount_bps,
            item_type: product.item_type.clone(),
        });
    }
    let reason = input
        .discount_reason
        .as_deref()
        .map(js_trim)
        .filter(|value| !value.is_empty());
    if requires_approval && reason.is_none_or(|value| utf16_len(value) < DISCOUNT_REASON_MIN) {
        return Err(pricing_discount_reason_required());
    }
    let total = subtotal + tax_total;
    if subtotal <= 0 || subtotal > MAX_SAFE_INTEGER || total > MAX_SAFE_INTEGER {
        return Err(order_amount_invalid());
    }
    Ok(PreparedSaveValues {
        subtotal,
        tax_total,
        requires_approval,
        reason: reason.map(str::to_owned),
        lines,
    })
}

/// Build one exact draft create/update plan after all scoped source facts were
/// read in the caller's serializable transaction.
#[allow(clippy::too_many_arguments)]
pub fn build_save_plan(
    actor: &CommerceOrderActor,
    command: &SaveOrderCommand,
    currency: String,
    existing: Option<SalesOrderRow>,
    warehouse: Option<SaveOrderWarehouse>,
    party_valid: bool,
    products: &[Option<SaveOrderProduct>],
    price_list: Option<SaveOrderPriceList>,
    number: String,
    now: TimestampMillis,
) -> Result<SaveOrderPlan, BusinessError> {
    build_save_plan_inner(
        actor,
        command,
        currency,
        existing,
        warehouse,
        party_valid,
        products,
        price_list,
        number,
        now,
        false,
    )
}

/// Native save plan whose party/version guards run after Finance Money. The
/// caller must write Money first and enforce the version/state at the owner
/// CAS, matching the original TypeScript effect order.
#[allow(clippy::too_many_arguments)]
pub fn build_save_plan_deferred(
    actor: &CommerceOrderActor,
    command: &SaveOrderCommand,
    currency: String,
    existing: Option<SalesOrderRow>,
    warehouse: Option<SaveOrderWarehouse>,
    party_valid: bool,
    products: &[Option<SaveOrderProduct>],
    price_list: Option<SaveOrderPriceList>,
    number: String,
    now: TimestampMillis,
) -> Result<SaveOrderPlan, BusinessError> {
    build_save_plan_inner(
        actor,
        command,
        currency,
        existing,
        warehouse,
        party_valid,
        products,
        price_list,
        number,
        now,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_save_plan_inner(
    actor: &CommerceOrderActor,
    command: &SaveOrderCommand,
    currency: String,
    existing: Option<SalesOrderRow>,
    warehouse: Option<SaveOrderWarehouse>,
    party_valid: bool,
    products: &[Option<SaveOrderProduct>],
    price_list: Option<SaveOrderPriceList>,
    number: String,
    now: TimestampMillis,
    defer_guards: bool,
) -> Result<SaveOrderPlan, BusinessError> {
    actor.require_write(now)?;
    command.validate()?;
    let input = &command.input;
    let create = command.order_id.is_none();
    let expected_version = match (create, input.expected_version) {
        (true, _) => None,
        (false, Some(version)) => Some(version),
        (false, None) => return Err(version_conflict()),
    };
    if let Some(existing) = &existing {
        if existing.kind() != Some(input.kind) {
            return Err(order_kind_locked());
        }
        if !defer_guards && existing.state() != Some(OrderState::Draft) {
            return Err(version_conflict());
        }
        if !defer_guards && Some(existing.version) != expected_version {
            return Err(version_conflict());
        }
    } else if !create {
        return Err(not_found());
    }
    if !defer_guards && !party_valid {
        return Err(party_invalid());
    }
    let PreparedSaveValues {
        subtotal,
        tax_total,
        requires_approval,
        reason,
        lines,
    } = prepare_save_values(command, &warehouse, products, &price_list)?;
    let (branch_id, department_id, created_by, version, order_number) = match &existing {
        Some(existing) => (
            warehouse
                .as_ref()
                .map(|value| value.branch_id.clone())
                .or_else(|| {
                    actor
                        .branch_id
                        .as_ref()
                        .map(|value| value.as_str().to_owned())
                }),
            existing.department_id.clone(),
            existing.created_by.clone(),
            if defer_guards {
                expected_version.ok_or_else(version_conflict)? + 1
            } else {
                existing.version + 1
            },
            existing.number.clone(),
        ),
        None => (
            warehouse
                .as_ref()
                .map(|value| value.branch_id.clone())
                .or_else(|| {
                    actor
                        .branch_id
                        .as_ref()
                        .map(|value| value.as_str().to_owned())
                }),
            actor
                .department_id
                .as_ref()
                .map(|value| value.as_str().to_owned()),
            actor.user_id.as_str().to_owned(),
            1,
            number,
        ),
    };
    if !valid_text(&order_number, 64) {
        return Err(source_invalid());
    }
    Ok(SaveOrderPlan {
        create,
        id: command.record_id(),
        company_id: actor.company_id.as_str().to_owned(),
        branch_id,
        department_id,
        kind: input.kind.as_str().to_owned(),
        number: order_number,
        party_id: input.party_id.clone(),
        warehouse_id: input.warehouse_id.clone(),
        currency,
        order_date: input.order_date.clone(),
        due_date: input.due_date.clone(),
        note: input.note.clone(),
        subtotal_minor: subtotal as i64,
        tax_minor: tax_total as i64,
        price_list_id: price_list.as_ref().map(|list| list.id.clone()),
        price_list_version: price_list.as_ref().map(|list| list.version),
        discount_reason: reason,
        discount_approval_required: i64::from(requires_approval),
        version,
        created_by,
        updated_at: now.get(),
        lines,
    })
}

fn parse_save_receipt_result(value: &str) -> Result<SaveOrderReceipt, BusinessError> {
    if value.len() > RECEIPT_LIMIT {
        return Err(source_invalid());
    }
    let parsed: Value = serde_json::from_str(value).map_err(|_| source_invalid())?;
    let Some(object) = parsed.as_object() else {
        return Err(source_invalid());
    };
    if object.len() != 2 || !object.contains_key("id") || !object.contains_key("version") {
        return Err(source_invalid());
    }
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(source_invalid)?;
    let version = object
        .get("version")
        .and_then(Value::as_i64)
        .ok_or_else(source_invalid)?;
    if !valid_identifier(id) || !(1..=MAX_SAFE_INTEGER as i64).contains(&version) {
        return Err(source_invalid());
    }
    Ok(SaveOrderReceipt {
        id: id.to_owned(),
        version,
    })
}

pub fn replay_save(
    receipt: Option<OrderCommandReceipt>,
    request_hash: &str,
    command: &SaveOrderCommand,
) -> Result<Option<SaveOrderReceipt>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_hash != request_hash {
        return Err(idempotency_conflict());
    }
    let parsed = parse_save_receipt_result(&receipt.result_json)?;
    let expected_version = match (command.order_id.is_some(), command.expected_version()) {
        (true, Some(version)) => version + 1,
        (true, None) => return Err(source_invalid()),
        (false, _) => 1,
    };
    if parsed.id != command.record_id()
        || parsed.version != expected_version
        || receipt.operation.as_deref() != Some(command.operation())
        || receipt.record_id.as_deref() != Some(command.record_id().as_str())
    {
        return Err(source_invalid());
    }
    Ok(Some(parsed))
}

pub fn validate_save_readback(
    plan: &SaveOrderPlan,
    row: &SalesOrderRow,
    lines: &[SalesOrderLineRow],
) -> Result<(), BusinessError> {
    row.validate_shape()?;
    if row.id != plan.id
        || row.company_id != plan.company_id
        || row.branch_id != plan.branch_id
        || row.department_id != plan.department_id
        || row.kind != plan.kind
        || row.number != plan.number
        || row.party_id != plan.party_id
        || row.warehouse_id != plan.warehouse_id
        || row.currency != plan.currency
        || row.order_date != plan.order_date
        || row.due_date != plan.due_date
        || row.note != plan.note
        || row.state != "draft"
        || row.subtotal_minor != plan.subtotal_minor
        || row.tax_minor != plan.tax_minor
        || row.version != plan.version
        || row.created_by != plan.created_by
        || row.updated_at != plan.updated_at
        || row.price_list_id != plan.price_list_id
        || row.price_list_version != plan.price_list_version
        || row.discount_reason != plan.discount_reason
        || row.discount_approval_required != plan.discount_approval_required
        || row.approved_by.is_some()
        || row.discount_approved_by.is_some()
        || row.discount_approved_at.is_some()
        || row.source_id.is_some()
        || lines.len() != plan.lines.len()
    {
        return Err(readback_invalid());
    }
    for (row, line) in lines.iter().zip(&plan.lines) {
        if row.id != line.id
            || row.order_id != plan.id
            || row.product_id != line.product_id
            || row.description != line.description
            || row.quantity_milli != line.quantity_milli
            || row.fulfilled_milli != 0
            || row.price_minor != line.price_minor
            || row.tax_bps != line.tax_bps
            || row.subtotal_minor != line.subtotal_minor
            || row.tax_minor != line.tax_minor
            || row.ordinal != line.ordinal
            || row.list_price_minor != Some(line.list_price_minor)
            || row.discount_bps != line.discount_bps
            || row.item_type != line.item_type
        {
            return Err(readback_invalid());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_and_currency_validation_are_bounded() {
        assert!(valid_date("2026-11-04"));
        assert!(valid_date("2028-02-29"));
        assert!(!valid_date("2026-02-29"));
        assert!(!valid_date("2026-13-01"));
        assert!(!valid_date("2026-00-01"));
        assert!(!valid_date("2026-1-01"));
        assert!(!valid_date("2026/11/04"));
        assert!(valid_currency("THB"));
        assert!(!valid_currency("thb"));
        assert!(!valid_currency("TH"));
    }

    #[test]
    fn js_trim_matches_ecmascript_whitespace_set() {
        assert_eq!(js_trim("\u{FEFF} reason \u{00A0}"), "reason");
        assert_eq!(js_trim("\u{2000}x\u{2028}"), "x");
        assert_eq!(js_trim("\u{0085}abc\u{0085}"), "\u{0085}abc\u{0085}");
        assert_eq!(js_trim("a\u{200B}"), "a\u{200B}");
        assert_eq!(cp_len("😀"), 1);
        assert_eq!(cp_len("ก"), 1);
        assert!(valid_note(&"ก".repeat(1_000), REASON_LIMIT));
        assert!(!valid_note(&"ก".repeat(1_001), REASON_LIMIT));
        assert!(valid_note(&"😀".repeat(1_000), REASON_LIMIT));
        assert!(!valid_note(&"😀".repeat(1_001), REASON_LIMIT));
    }

    #[test]
    fn receipt_requires_the_exact_converted_sales_identity() {
        let null_sales = json!({"id":"order-1","state":"submitted","version":2,"salesId":null});
        assert!(parse_receipt_result(&null_sales.to_string()).is_ok());
        let converted =
            json!({"id":"quote-1","state":"converted","version":4,"salesId":"order-key"});
        assert!(parse_receipt_result(&converted.to_string()).is_ok());
        let missing = json!({"id":"quote-1","state":"converted","version":4,"salesId":null});
        assert_eq!(
            parse_receipt_result(&missing.to_string()).unwrap_err().code,
            "COMMERCE_ORDER_SOURCE_INVALID"
        );
    }

    #[test]
    fn save_plan_requires_the_delegated_discount_reason_and_uses_integer_money() {
        let actor = CommerceOrderActor {
            user_id: UserId::new("user").unwrap(),
            person_user_ids: vec![],
            holding_id: HoldingId::new("h001").unwrap(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: Some(RecordId::new("branch").unwrap()),
            department_id: None,
            assignment_id: AssignmentId::new("assignment").unwrap(),
            role: None,
            authority_revision: None,
            assignment_version: None,
            permissions: vec!["commerce.sales".into()],
            modules: vec!["sales".into()],
            expires_at: i64::MAX,
        };
        let command = |reason: Option<&str>| SaveOrderCommand {
            key: CommandKey::new("commerce-order-save-test-0001").unwrap(),
            order_id: None,
            input: SaveOrderInput {
                kind: OrderKind::Quote,
                party_id: "party-1".into(),
                warehouse_id: None,
                order_date: "2026-11-10".into(),
                due_date: "2026-11-15".into(),
                note: String::new(),
                price_list_id: None,
                discount_reason: reason.map(str::to_owned),
                lines: vec![SaveOrderLineInput {
                    product_id: "product-1".into(),
                    quantity: "2".into(),
                    price: "80".into(),
                    tax_bps: 700,
                }],
                expected_version: None,
            },
        };
        let product = SaveOrderProduct {
            id: "product-1".into(),
            name: "Service".into(),
            item_type: "service".into(),
            version: 1,
        };
        let price_list = SaveOrderPriceList {
            id: "list-customer".into(),
            version: 1,
            max_auto_discount_bps: 1_000,
            items: vec![SaveOrderPriceItem {
                product_id: "product-1".into(),
                price_minor: 10_500,
                tax_bps: 700,
            }],
        };
        assert_eq!(
            build_save_plan(
                &actor,
                &command(None),
                "THB".into(),
                None,
                None,
                true,
                &[Some(product.clone())],
                Some(price_list.clone()),
                "QT-2026-000001".into(),
                TimestampMillis::new(1_800_000_000_000),
            )
            .unwrap_err()
            .code,
            "DISCOUNT_REASON_REQUIRED"
        );
        let ready = command(Some("Strategic launch order approved"));
        let prepared = prepare_save_values(
            &ready,
            &None,
            &[Some(product.clone())],
            &Some(price_list.clone()),
        )
        .unwrap();
        assert_eq!((prepared.subtotal, prepared.tax_total), (16_000, 1_120));
        assert_eq!(prepared.lines[0].discount_bps, 2_380);
        assert_eq!(
            prepare_save_values(&ready, &None, &[None], &Some(price_list.clone()))
                .unwrap_err()
                .code,
            "PRODUCT_INVALID"
        );
        // The original save path validates the party through a command
        // assertion, so it is a generic conflict rather than a field error.
        assert_eq!(
            build_save_plan(
                &actor,
                &ready,
                "THB".into(),
                None,
                None,
                false,
                &[None],
                Some(price_list.clone()),
                "QT-2026-000001".into(),
                TimestampMillis::new(1_800_000_000_000)
            )
            .unwrap_err()
            .code,
            "CONFLICT"
        );
        let plan = build_save_plan(
            &actor,
            &command(Some("Strategic launch order approved")),
            "THB".into(),
            None,
            None,
            true,
            &[Some(product)],
            Some(price_list),
            "QT-2026-000001".into(),
            TimestampMillis::new(1_800_000_000_000),
        )
        .unwrap();
        assert_eq!(plan.subtotal_minor, 16_000);
        assert_eq!(plan.tax_minor, 1_120);
        assert_eq!(plan.lines[0].discount_bps, 2_380);
        assert_eq!(plan.lines[0].list_price_minor, 10_500);
        assert_eq!(plan.discount_approval_required, 1);
    }

    #[test]
    fn document_total_overflow_is_rejected() {
        let order = SalesOrderRow {
            id: "order-1".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            kind: "quote".into(),
            number: "QT-1".into(),
            party_id: "party-1".into(),
            warehouse_id: None,
            currency: "THB".into(),
            order_date: "2026-11-01".into(),
            due_date: "2026-11-02".into(),
            note: String::new(),
            state: "draft".into(),
            subtotal_minor: i64::MAX,
            tax_minor: 0,
            source_id: None,
            version: 1,
            created_by: "user".into(),
            approved_by: None,
            created_at: 1,
            updated_at: 1,
            price_list_id: None,
            price_list_version: None,
            discount_reason: None,
            discount_approval_required: 0,
            discount_approved_by: None,
            discount_approved_at: None,
        };
        let actor = CommerceOrderActor {
            user_id: UserId::new("user").unwrap(),
            person_user_ids: vec![],
            holding_id: HoldingId::new("h001").unwrap(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: None,
            department_id: None,
            assignment_id: AssignmentId::new("assignment").unwrap(),
            role: None,
            authority_revision: None,
            assignment_version: None,
            permissions: vec!["commerce.read".into()],
            modules: vec!["sales".into()],
            expires_at: i64::MAX,
        };
        let half = i64::MAX / 2 + 1;
        let line = SalesOrderLineRow {
            id: "line-1".into(),
            order_id: "order-1".into(),
            product_id: "product-1".into(),
            description: "Service".into(),
            quantity_milli: 1_000,
            fulfilled_milli: 0,
            price_minor: half,
            tax_bps: 0,
            subtotal_minor: half,
            tax_minor: 0,
            ordinal: 0,
            list_price_minor: None,
            discount_bps: 0,
            item_type: "service".into(),
            sku: "SVC-1".into(),
            unit: "each".into(),
            tracking_mode: "none".into(),
        };
        let mut second = line.clone();
        second.id = "line-2".into();
        second.ordinal = 1;
        second.product_id = "product-2".into();
        assert_eq!(
            validate_document(&actor, order, vec![line, second], TimestampMillis::new(1))
                .unwrap_err()
                .code,
            "COMMERCE_ORDER_SOURCE_INVALID"
        );
    }
}
