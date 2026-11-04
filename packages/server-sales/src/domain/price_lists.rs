//! Native Sales price-list facts and lifecycle policy.
//!
//! A price list is the governed pricing authority used before a quotation or
//! sales order is prepared. The owner-local lifecycle is `draft -> active` by
//! an independent Company sales manager, or `draft -> cancelled`. Only
//! `sales_price_lists`/`sales_price_list_items` and the shared command
//! receipt/audit/outbox records are written. Current Company currency, active
//! customer and active product facts are read through the repository as
//! same-transaction source checks; no Commerce, Finance or Inventory row is
//! mutated.
//!
//! Quotation/sales-order draft create/update remains Commerce-owned and is not
//! duplicated here. Sales-order approval still requires the retained Finance
//! credit guard, and confirmed sales-order cancellation still requires the
//! retained Inventory reservation/reversal guard.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis, UserId,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub const NAME_MIN: usize = 3;
pub const NAME_MAX: usize = 160;
pub const ITEMS_MIN: usize = 1;
pub const ITEMS_MAX: usize = 500;
pub const REASON_MIN: usize = 5;
pub const REASON_MAX: usize = 1_000;
pub const RECEIPT_LIMIT: usize = 4 * 1024;

#[derive(Clone, Debug)]
pub struct PriceListActor {
    pub user_id: UserId,
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

impl PriceListActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self) -> bool {
        self.modules.iter().any(|value| value == "sales")
    }

    /// Price lists are Company-scoped. A branch/department work context cannot
    /// narrow or re-use another scope's approved pricing.
    pub fn require_manage(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has_module() {
            return Err(module_forbidden());
        }
        if now.get() >= self.expires_at {
            return Err(work_context_changed());
        }
        if !self.has("commerce.read") || !self.has("commerce.sales") {
            return Err(permission_required());
        }
        if self.branch_id.is_some() || self.department_id.is_some() {
            return Err(company_scope_required());
        }
        Ok(())
    }

    pub fn require_approve(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require_manage(now)?;
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceListItemInput {
    pub product_id: String,
    /// Exact two-decimal minor units. The HTTP ingress parses the wire decimal
    /// before the domain sees it.
    pub price_minor: i64,
    pub tax_bps: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceListInput {
    pub name: String,
    pub currency: String,
    pub customer_id: Option<String>,
    pub valid_from: String,
    pub valid_until: String,
    pub max_auto_discount_bps: i64,
    pub items: Vec<PriceListItemInput>,
}

impl PriceListInput {
    pub fn normalized(mut self) -> Result<Self, BusinessError> {
        self.name = self.name.trim().to_owned();
        self.currency = self.currency.trim().to_ascii_uppercase();
        if self.name.chars().count() < NAME_MIN
            || self.name.chars().count() > NAME_MAX
            || self.name.chars().any(char::is_control)
        {
            return Err(request_invalid());
        }
        if self.currency.len() != 3 || !self.currency.bytes().all(|byte| byte.is_ascii_uppercase())
        {
            return Err(currency_invalid());
        }
        if let Some(customer) = &self.customer_id {
            validate_identifier(customer).map_err(|_| customer_invalid())?;
        }
        if !valid_date(&self.valid_from) || !valid_date(&self.valid_until) {
            return Err(dates_invalid());
        }
        if self.valid_until < self.valid_from {
            return Err(dates_invalid());
        }
        if !(0..=10_000).contains(&self.max_auto_discount_bps) {
            return Err(request_invalid());
        }
        if !(ITEMS_MIN..=ITEMS_MAX).contains(&self.items.len()) {
            return Err(request_invalid());
        }
        let mut products = BTreeSet::new();
        for item in &self.items {
            validate_identifier(&item.product_id).map_err(|_| product_invalid())?;
            if item.price_minor < 0 || !(0..=10_000).contains(&item.tax_bps) {
                return Err(request_invalid());
            }
            if !products.insert(item.product_id.as_str()) {
                return Err(product_duplicate());
            }
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceListUpdate {
    pub input: PriceListInput,
    pub expected_version: i64,
}

impl PriceListUpdate {
    pub fn normalized(self) -> Result<Self, BusinessError> {
        if self.expected_version < 1 {
            return Err(request_invalid());
        }
        Ok(Self {
            input: self.input.normalized()?,
            expected_version: self.expected_version,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PriceListDecisionAction {
    Activate,
    Cancel,
}

impl PriceListDecisionAction {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "activate" => Ok(Self::Activate),
            "cancel" => Ok(Self::Cancel),
            _ => Err(request_invalid()),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Activate => "activate",
            Self::Cancel => "cancel",
        }
    }

    pub fn operation(self) -> &'static str {
        match self {
            Self::Activate => "sales.price-list.activate",
            Self::Cancel => "sales.price-list.cancel",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceListDecision {
    pub expected_version: i64,
    pub action: PriceListDecisionAction,
    pub reason: String,
}

impl PriceListDecision {
    pub fn normalized(mut self) -> Result<Self, BusinessError> {
        self.reason = self.reason.trim().to_owned();
        if self.expected_version < 1
            || self.reason.chars().count() < REASON_MIN
            || self.reason.chars().count() > REASON_MAX
            || !valid_note(&self.reason, REASON_MAX)
        {
            return Err(request_invalid());
        }
        Ok(self)
    }

    pub fn operation(&self) -> &'static str {
        self.action.operation()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PriceListStatus {
    Draft,
    Active,
    Superseded,
    Cancelled,
}

impl PriceListStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "active" => Some(Self::Active),
            "superseded" => Some(Self::Superseded),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Superseded => "superseded",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceListRow {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub currency: String,
    pub customer_id: Option<String>,
    pub valid_from: String,
    pub valid_until: String,
    pub max_auto_discount_bps: i64,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub prepared_by: String,
    pub approved_by: Option<String>,
    pub approval_reason: Option<String>,
    pub activated_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceListItemRow {
    pub id: String,
    pub price_list_id: String,
    pub product_id: String,
    pub price_minor: i64,
    pub tax_bps: i64,
    pub ordinal: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceListDocument {
    pub list: PriceListRow,
    pub items: Vec<PriceListItemRow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceListReceipt {
    pub id: String,
    pub status: String,
    pub version: i64,
}

impl PriceListReceipt {
    pub fn to_wire(&self) -> Value {
        json!({
            "id": self.id,
            "status": self.status,
            "version": self.version,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceListCommandReceipt {
    pub request_hash: String,
    pub result_json: String,
    pub operation: Option<String>,
    pub actor_id: Option<String>,
    pub record_id: Option<String>,
    pub authority_json: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceListProductSource {
    pub id: String,
    pub version: i64,
}

pub fn module_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "SALES_PRICING_MODULE_FORBIDDEN",
        "Your current role cannot access Sales.",
    )
}

pub fn permission_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "SALES_PRICING_PERMISSION_REQUIRED",
        "Your selected role does not grant Sales pricing access.",
    )
}

pub fn approval_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "SALES_PRICING_APPROVAL_REQUIRED",
        "Your selected role does not grant Sales pricing approval.",
    )
}

pub fn company_scope_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMPANY_PRICING_SCOPE_REQUIRED",
        "Use a Company sales assignment to manage price lists.",
    )
}

pub fn work_context_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your selected work context changed. Review the current workspace and role.",
    )
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PRICE_LIST_REQUEST_INVALID",
        "Use one bounded price-list request.",
    )
}

pub fn dates_invalid() -> BusinessError {
    BusinessError::invalid(
        "PRICE_LIST_DATES_INVALID",
        "The end date must be on or after the start date.",
    )
}

pub fn currency_invalid() -> BusinessError {
    BusinessError::invalid(
        "PRICE_LIST_CURRENCY_INVALID",
        "Use the Company operating currency.",
    )
}

pub fn customer_invalid() -> BusinessError {
    BusinessError::invalid(
        "PRICE_LIST_CUSTOMER_INVALID",
        "Choose an active customer in this currency.",
    )
}

pub fn product_invalid() -> BusinessError {
    BusinessError::invalid(
        "PRICE_LIST_PRODUCT_INVALID",
        "Every price-list product must be active.",
    )
}

pub fn product_duplicate() -> BusinessError {
    BusinessError::invalid(
        "PRICE_LIST_PRODUCT_DUPLICATE",
        "Each product may appear once.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PRICE_LIST_NOT_FOUND",
        "Price list not found in this workspace.",
    )
}

pub fn locked() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PRICE_LIST_LOCKED",
        "Only a draft price list can be changed.",
    )
}

pub fn version_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PRICE_LIST_CHANGED",
        "The price list changed. Reload and try again.",
    )
}

pub fn independent_approver_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "INDEPENDENT_PRICING_APPROVER_REQUIRED",
        "Another Company sales manager must activate this price list.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to a different request.",
    )
}

pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SALES_PRICING_SOURCE_INVALID",
        "The current Sales pricing source is invalid.",
    )
}

pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SALES_PRICING_READBACK_INVALID",
        "The recorded price-list change could not be read back.",
    )
}

fn validate_identifier(value: &str) -> Result<(), ()> {
    RecordId::new(value).map(|_| ()).map_err(|_| ())
}

fn valid_note(value: &str, limit: usize) -> bool {
    value.len() <= limit
        && !value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
}

fn valid_date(value: &str) -> bool {
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

impl PriceListRow {
    pub fn status(&self) -> Option<PriceListStatus> {
        PriceListStatus::parse(&self.status)
    }

    pub fn validate(&self, company: &CompanyId) -> Result<(), BusinessError> {
        let Some(status) = self.status() else {
            return Err(source_invalid());
        };
        if self.company_id != company.as_str()
            || self.id.is_empty()
            || self.id.len() > 200
            || self.name.chars().count() < NAME_MIN
            || self.name.chars().count() > NAME_MAX
            || self.name.chars().any(char::is_control)
            || self.currency.len() != 3
            || !self.currency.bytes().all(|byte| byte.is_ascii_uppercase())
            || self
                .customer_id
                .as_deref()
                .is_some_and(|value| validate_identifier(value).is_err())
            || !valid_date(&self.valid_from)
            || !valid_date(&self.valid_until)
            || self.valid_until < self.valid_from
            || !(0..=10_000).contains(&self.max_auto_discount_bps)
            || self.version < 1
            || self.created_by.is_empty()
            || self.prepared_by.is_empty()
            || self
                .approved_by
                .as_deref()
                .is_some_and(|value| validate_identifier(value).is_err())
            || self
                .approval_reason
                .as_deref()
                .is_some_and(|value| !valid_note(value, REASON_MAX))
            || self.activated_at.is_some_and(|value| value < 0)
            || self.created_at < 0
            || self.updated_at < self.created_at
            || (status == PriceListStatus::Active
                && (self.approved_by.is_none()
                    || self.approved_by == Some(self.created_by.clone())
                    || self.approved_by == Some(self.prepared_by.clone())
                    || self.approval_reason.is_none()
                    || self.activated_at.is_none()))
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

impl PriceListItemRow {
    pub fn validate(&self, list_id: &str) -> Result<(), BusinessError> {
        if self.id.is_empty()
            || self.id.len() > 200
            || self.price_list_id != list_id
            || validate_identifier(&self.product_id).is_err()
            || self.price_minor < 0
            || !(0..=10_000).contains(&self.tax_bps)
            || self.ordinal < 0
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

/// Validate the exact persisted draft list and item rows before returning.
pub fn validate_readback(
    expected_list: &PriceListRow,
    expected_items: &[PriceListItemRow],
    actual: &PriceListDocument,
) -> Result<(), BusinessError> {
    actual
        .list
        .validate(&CompanyId::new(&actual.list.company_id).map_err(|_| readback_invalid())?)?;
    if actual.list != *expected_list {
        return Err(readback_invalid());
    }
    if actual.items.len() != expected_items.len() {
        return Err(readback_invalid());
    }
    let mut ids = BTreeSet::new();
    for (actual, expected) in actual.items.iter().zip(expected_items) {
        actual.validate(&expected_list.id)?;
        if !ids.insert(actual.id.as_str()) || actual != expected {
            return Err(readback_invalid());
        }
    }
    Ok(())
}

pub fn expected_create_list(
    actor: &PriceListActor,
    input: &PriceListInput,
    id: &str,
    now: i64,
) -> PriceListRow {
    PriceListRow {
        id: id.to_owned(),
        company_id: actor.company_id.as_str().to_owned(),
        name: input.name.clone(),
        currency: input.currency.clone(),
        customer_id: input.customer_id.clone(),
        valid_from: input.valid_from.clone(),
        valid_until: input.valid_until.clone(),
        max_auto_discount_bps: input.max_auto_discount_bps,
        status: PriceListStatus::Draft.as_str().into(),
        version: 1,
        created_by: actor.user_id.as_str().to_owned(),
        prepared_by: actor.user_id.as_str().to_owned(),
        approved_by: None,
        approval_reason: None,
        activated_at: None,
        created_at: now,
        updated_at: now,
    }
}

pub fn expected_update_list(
    existing: &PriceListRow,
    actor: &PriceListActor,
    input: &PriceListInput,
    now: i64,
) -> PriceListRow {
    PriceListRow {
        name: input.name.clone(),
        currency: input.currency.clone(),
        customer_id: input.customer_id.clone(),
        valid_from: input.valid_from.clone(),
        valid_until: input.valid_until.clone(),
        max_auto_discount_bps: input.max_auto_discount_bps,
        version: existing.version + 1,
        prepared_by: actor.user_id.as_str().to_owned(),
        updated_at: now,
        ..existing.clone()
    }
}

pub fn expected_decision_list(
    existing: &PriceListRow,
    actor: &PriceListActor,
    decision: &PriceListDecision,
    now: i64,
) -> PriceListRow {
    let (status, approved_by, approval_reason, activated_at) = match decision.action {
        PriceListDecisionAction::Activate => (
            PriceListStatus::Active.as_str(),
            Some(actor.user_id.as_str().to_owned()),
            Some(decision.reason.clone()),
            Some(now),
        ),
        PriceListDecisionAction::Cancel => (
            PriceListStatus::Cancelled.as_str(),
            None,
            Some(decision.reason.clone()),
            None,
        ),
    };
    PriceListRow {
        status: status.into(),
        version: existing.version + 1,
        approved_by,
        approval_reason,
        activated_at,
        updated_at: now,
        ..existing.clone()
    }
}

pub fn expected_items(
    list_id: &str,
    version: i64,
    updated: bool,
    input: &PriceListInput,
) -> Vec<PriceListItemRow> {
    input
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| PriceListItemRow {
            id: if updated {
                format!("{list_id}-v{version}-item-{index}")
            } else {
                format!("{list_id}-item-{index}")
            },
            price_list_id: list_id.to_owned(),
            product_id: item.product_id.clone(),
            price_minor: item.price_minor,
            tax_bps: item.tax_bps,
            ordinal: index as i64,
        })
        .collect()
}

pub fn command_identity(
    actor: &PriceListActor,
    operation: &str,
    record_id: &str,
    input: &Value,
) -> String {
    json!({
        "user": actor.user_id.as_str(),
        "assignment": actor.assignment_id.as_str(),
        "company": actor.company_id.as_str(),
        "branch": actor.branch_id.as_ref().map(|value| value.as_str()),
        "department": actor.department_id.as_ref().map(|value| value.as_str()),
        "operation": operation,
        "record": record_id,
        "input": input,
    })
    .to_string()
}

pub fn validate_hash(value: &str) -> Result<(), BusinessError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(source_invalid());
    }
    Ok(())
}

fn parse_receipt_result(value: &str) -> Result<PriceListReceipt, BusinessError> {
    if value.len() > RECEIPT_LIMIT {
        return Err(source_invalid());
    }
    let parsed: Value = serde_json::from_str(value).map_err(|_| source_invalid())?;
    let Some(object) = parsed.as_object() else {
        return Err(source_invalid());
    };
    if object.len() != 3
        || !object.contains_key("id")
        || !object.contains_key("status")
        || !object.contains_key("version")
    {
        return Err(source_invalid());
    }
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(source_invalid)?;
    let status = object
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(source_invalid)?;
    let version = object
        .get("version")
        .and_then(Value::as_i64)
        .ok_or_else(source_invalid)?;
    if validate_identifier(id).is_err() || PriceListStatus::parse(status).is_none() || version < 1 {
        return Err(source_invalid());
    }
    Ok(PriceListReceipt {
        id: id.to_owned(),
        status: status.to_owned(),
        version,
    })
}

pub fn replay(
    receipt: Option<PriceListCommandReceipt>,
    request_hash: &str,
    operation: &str,
    record_id: &str,
    expected_status: &str,
    expected_version: i64,
) -> Result<Option<PriceListReceipt>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_hash != request_hash {
        return Err(idempotency_conflict());
    }
    if receipt
        .operation
        .as_deref()
        .is_some_and(|value| value != operation)
        || receipt
            .record_id
            .as_deref()
            .is_some_and(|value| value != record_id)
    {
        return Err(idempotency_conflict());
    }
    let parsed = parse_receipt_result(&receipt.result_json)?;
    if parsed.id != record_id
        || parsed.status != expected_status
        || parsed.version != expected_version
    {
        return Err(source_invalid());
    }
    Ok(Some(parsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_currency_and_note_bounds_are_exact() {
        assert!(valid_date("2026-11-04"));
        assert!(valid_date("2028-02-29"));
        assert!(!valid_date("2026-02-29"));
        assert!(!valid_date("2026-13-01"));
        assert!(!valid_date("2026-1-01"));
        assert!(!valid_date("2026/11/04"));
        assert!(valid_note("reviewed margin", REASON_MAX));
        assert!(!valid_note(&"x".repeat(REASON_MAX + 1), REASON_MAX));
    }

    #[test]
    fn input_normalization_trims_name_and_uppercases_currency() {
        let input = PriceListInput {
            name: "  Standard pricing  ".into(),
            currency: "thb".into(),
            customer_id: None,
            valid_from: "2026-11-01".into(),
            valid_until: "2026-11-30".into(),
            max_auto_discount_bps: 500,
            items: vec![PriceListItemInput {
                product_id: "product-1".into(),
                price_minor: 1_000,
                tax_bps: 700,
            }],
        }
        .normalized()
        .unwrap();
        assert_eq!(input.name, "Standard pricing");
        assert_eq!(input.currency, "THB");
    }

    #[test]
    fn duplicate_products_and_reversed_dates_are_rejected() {
        let duplicate = PriceListInput {
            name: "Standard pricing".into(),
            currency: "THB".into(),
            customer_id: None,
            valid_from: "2026-11-01".into(),
            valid_until: "2026-11-30".into(),
            max_auto_discount_bps: 0,
            items: vec![
                PriceListItemInput {
                    product_id: "product-1".into(),
                    price_minor: 1_000,
                    tax_bps: 700,
                },
                PriceListItemInput {
                    product_id: "product-1".into(),
                    price_minor: 2_000,
                    tax_bps: 700,
                },
            ],
        };
        assert_eq!(
            duplicate.normalized().unwrap_err().code,
            "PRICE_LIST_PRODUCT_DUPLICATE"
        );
        let reversed = PriceListInput {
            name: "Standard pricing".into(),
            currency: "THB".into(),
            customer_id: None,
            valid_from: "2026-11-30".into(),
            valid_until: "2026-11-01".into(),
            max_auto_discount_bps: 0,
            items: vec![PriceListItemInput {
                product_id: "product-1".into(),
                price_minor: 1_000,
                tax_bps: 700,
            }],
        };
        assert_eq!(
            reversed.normalized().unwrap_err().code,
            "PRICE_LIST_DATES_INVALID"
        );
    }

    #[test]
    fn receipt_requires_the_exact_three_fields() {
        let exact = json!({"id":"price-list-1","status":"draft","version":1});
        assert!(parse_receipt_result(&exact.to_string()).is_ok());
        let extra = json!({"id":"price-list-1","status":"draft","version":1,"approved":true});
        assert_eq!(
            parse_receipt_result(&extra.to_string()).unwrap_err().code,
            "SALES_PRICING_SOURCE_INVALID"
        );
        let active_without_fields = json!({"id":"price-list-1","status":"active","version":2});
        assert!(parse_receipt_result(&active_without_fields.to_string()).is_ok());
    }
}
