//! Native POS workspace/day read facts.
//!
//! The workspace is a bounded owner read over the current POS settings,
//! registers, shifts, cash movements, the current business-date close and a
//! bounded page of sales with their lines and payments. It preserves the
//! retained TypeScript read in `packages/server-pos/src/application/pos.ts`
//! (`posWorkspace`, lines 274-370) and `pos-daily.ts` (business date,
//! `posDayReport` totals are not part of this slice): the same permission
//! (`pos.read`), the same `POS_BRANCH_REQUIRED` department guard, the same
//! Company/branch scoping, the same `POS_NOT_FOUND` scope failure for a
//! selected sale, the same bounded list sizes and the same row field names.
//!
//! Package entitlement evaluation stays with Commercial; composition supplies
//! the already-decided [`PosWorkspaceEntitlements`] and this owner enforces the
//! same `PACKAGE_REQUIRED` denial, `roomChargeEnabled` gate and `purchases`
//! projection that the retained wrapper applied (`adapters/pos.ts` 287-303).
//! No Finance, Inventory, Commerce, Organization, PMS or Work row is written,
//! and no cross-owner business effect is invented here.
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Bounded page sizes. The retained reads used `LIMIT 100` (shifts, cash
/// movements), `LIMIT 150` (sales) and a per-sale detail read; the native read
/// keeps those maxima and adds explicit limits for lines and payments.
pub const MAX_REGISTERS: usize = 200;
pub const MAX_SHIFTS: usize = 100;
pub const MAX_CASH_MOVEMENTS: usize = 100;
pub const MAX_SALES: usize = 150;
pub const MAX_LINES: usize = 200;
pub const MAX_PAYMENTS: usize = 100;
pub const SEARCH_MAX_CHARS: usize = 100;
pub const MAX_QUERY_BYTES: usize = 2_048;
pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

pub const PERMISSION_POS_READ: &str = "pos.read";

/// Exact projection of the retained `posPermissions(actor)` map
/// (`domain/pos-permissions.ts`), in its declared order.
pub const POS_PERMISSION_NAMES: [&str; 13] = [
    "pos.read",
    "pos.sell",
    "pos.kitchen",
    "pos.approve",
    "pos.configure",
    "pos.accounting.prepare",
    "pos.accounting.approve",
    "pos.payments.configure",
    "pos.reports",
    "finance.read",
    "finance.write",
    "finance.approve",
    "pms.approve",
];

const SALE_STATES: [&str; 6] = [
    "open",
    "held",
    "payment_pending",
    "completed",
    "cancelled",
    "merged",
];
const SHIFT_STATES: [&str; 3] = ["open", "closed", "reviewed"];
const PAYMENT_METHODS: [&str; 5] = ["cash", "transfer", "edc", "stripe_card", "stripe_promptpay"];
const PAYMENT_STATES: [&str; 6] = [
    "requested",
    "pending",
    "succeeded",
    "failed",
    "cancelled",
    "exception",
];
const FULFILLMENT_STATES: [&str; 6] = [
    "draft",
    "queued",
    "preparing",
    "ready",
    "delivered",
    "cancelled",
];
const DAY_STATES: [&str; 2] = ["prepared", "confirmed"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosReadActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl PosReadActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    /// Retained read policy. The retained TypeScript read required `pos.read`
    /// (`application/pos.ts:275`, `adapters/pos.ts:288`) and rejected a
    /// department work context with `POS_BRANCH_REQUIRED` (`pos.ts:277-278`).
    /// The `pos` module and work-context expiry keep the same native guard the
    /// POS command slices already use.
    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has(PERMISSION_POS_READ) {
            return Err(permission_required());
        }
        if !self.modules.iter().any(|value| value == "pos") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "POS_MODULE_FORBIDDEN",
                "Your current role cannot access POS.",
            ));
        }
        if self.expires_at <= 0 || now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        if self.department_id.is_some() {
            return Err(branch_required());
        }
        Ok(())
    }

    pub fn in_scope(&self, company_id: &str, branch_id: Option<&str>) -> bool {
        self.company_id.as_str() == company_id
            && match self.branch_id.as_ref() {
                Some(branch) => branch_id == Some(branch.as_str()),
                None => true,
            }
    }

    pub fn permissions(&self) -> BTreeMap<&'static str, bool> {
        POS_PERMISSION_NAMES
            .iter()
            .map(|name| (*name, self.has(name)))
            .collect()
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub fn company_id(&self) -> &CompanyId {
        &self.company_id
    }

    pub fn branch_id(&self) -> Option<&RecordId> {
        self.branch_id.as_ref()
    }
}

pub(crate) fn permission_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PERMISSION_REQUIRED",
        "Your selected role does not grant this permission.",
    )
}

pub(crate) fn branch_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "POS_BRANCH_REQUIRED",
        "Select a Company or Branch to read POS reports.",
    )
}

pub(crate) fn package_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "This package does not permit the requested POS workspace.",
    )
}

pub(crate) fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "POS_NOT_FOUND",
        "Record not found in this workspace.",
    )
}

pub(crate) fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "POS_REQUEST_INVALID",
        "Use one unambiguous POS workspace request.",
    )
}

pub(crate) fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POS_SOURCE_INVALID",
        "The current POS workspace source is invalid.",
    )
}

pub(crate) fn limit_exceeded() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POS_WORKSPACE_LIMIT",
        "This POS workspace exceeds its bounded read.",
    )
}

fn limit_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "POS_LIMIT_INVALID",
        "Use a whole page size between 1 and 150.",
    )
}

/// Retained `tab` selection (`application/pos.ts:282-300`). The read gates the
/// same list reads the retained `wants(...)` helper gated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PosWorkspaceTab {
    Sale,
    Kitchen,
    Shifts,
    History,
    Reports,
    Settings,
    Catalog,
}

impl PosWorkspaceTab {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "sale" => Self::Sale,
            "kitchen" => Self::Kitchen,
            "shifts" => Self::Shifts,
            "history" => Self::History,
            "reports" => Self::Reports,
            "settings" => Self::Settings,
            "catalog" => Self::Catalog,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sale => "sale",
            Self::Kitchen => "kitchen",
            Self::Shifts => "shifts",
            Self::History => "history",
            Self::Reports => "reports",
            Self::Settings => "settings",
            Self::Catalog => "catalog",
        }
    }

    /// `wants('sale')`: the retained list read only ran for no tab or `sale`.
    pub fn reads_sales(self) -> bool {
        matches!(self, Self::Sale)
    }

    /// `wants('sale','history','shifts')` for the shift session list.
    pub fn reads_shifts(self) -> bool {
        matches!(self, Self::Sale | Self::Shifts | Self::History)
    }

    /// `wants('shifts')` for the cash movement list.
    pub fn reads_cash(self) -> bool {
        matches!(self, Self::Shifts)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosWorkspaceQuery {
    pub tab: Option<PosWorkspaceTab>,
    pub sale_id: Option<String>,
    /// Already bounded search text; the adapter binds `%search%` exactly like
    /// the retained `historyQuery.slice(0, 100)` pattern.
    pub search: String,
    pub limit: usize,
}

impl Default for PosWorkspaceQuery {
    fn default() -> Self {
        Self {
            tab: None,
            sale_id: None,
            search: String::new(),
            limit: MAX_SALES,
        }
    }
}

impl PosWorkspaceQuery {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self
            .sale_id
            .as_deref()
            .is_some_and(|sale| RecordId::new(sale).is_err())
        {
            return Err(request_invalid());
        }
        if self.search.chars().count() > SEARCH_MAX_CHARS
            || self.search.chars().any(char::is_control)
        {
            return Err(request_invalid());
        }
        if !(1..=MAX_SALES).contains(&self.limit) {
            return Err(limit_invalid());
        }
        Ok(())
    }

    /// No tab means the retained `!section` branch: every gated list reads.
    pub fn reads_sales(&self) -> bool {
        self.tab.is_none_or(PosWorkspaceTab::reads_sales)
    }

    pub fn reads_shifts(&self) -> bool {
        self.tab.is_none_or(PosWorkspaceTab::reads_shifts)
    }

    pub fn reads_cash(&self) -> bool {
        self.tab.is_none_or(PosWorkspaceTab::reads_cash)
    }
}

/// Composition-owned entitlement decision. The owner never infers a purchase
/// grant; it applies the retained wrapper's denial and projection.
#[derive(Clone, Debug, PartialEq)]
pub struct PosWorkspaceEntitlements {
    pub package_enforced: bool,
    pub sales_history: bool,
    pub kitchen_history: bool,
    pub shifts_history: bool,
    pub room_charge_enabled: bool,
    pub purchases: Option<Value>,
}

impl Default for PosWorkspaceEntitlements {
    fn default() -> Self {
        Self {
            package_enforced: false,
            sales_history: true,
            kitchen_history: true,
            shifts_history: true,
            room_charge_enabled: true,
            purchases: None,
        }
    }
}

impl PosWorkspaceEntitlements {
    /// `adapters/pos.ts:292-294`: the required capability follows the tab.
    pub fn require_capability(&self, tab: Option<PosWorkspaceTab>) -> Result<(), BusinessError> {
        if !self.package_enforced {
            return Ok(());
        }
        let allowed = match tab {
            Some(PosWorkspaceTab::Kitchen) => self.kitchen_history,
            Some(PosWorkspaceTab::Shifts) => self.shifts_history,
            _ => self.sales_history,
        };
        if allowed {
            Ok(())
        } else {
            Err(package_required())
        }
    }

    /// The retained `purchases` projection was only present when package
    /// enforcement ran.
    pub fn purchases(&self) -> Value {
        if self.package_enforced {
            self.purchases.clone().unwrap_or(Value::Null)
        } else {
            Value::Null
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosRegisterRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub name: String,
    pub mode: String,
    pub active: bool,
    pub version: i64,
    pub warehouse_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosSettingsRow {
    pub enabled: bool,
    pub currency: String,
    pub timezone: String,
    pub cashier_discount_bps: i64,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosShiftRow {
    pub id: String,
    pub register_id: String,
    pub register_name: String,
    pub company_id: String,
    pub branch_id: String,
    pub business_date: String,
    pub opened_by: String,
    pub opened_at: i64,
    pub opening_minor: i64,
    pub state: String,
    pub counted_minor: Option<i64>,
    pub expected_minor: Option<i64>,
    pub variance_minor: Option<i64>,
    pub closed_by: Option<String>,
    pub closed_at: Option<i64>,
    pub version: i64,
    pub cash_expected_minor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosCashMovementRow {
    pub id: String,
    pub shift_id: String,
    pub register_id: String,
    pub amount_minor: i64,
    pub reason: String,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PosCashSummary {
    pub pending_count: i64,
    pub pending_minor: i64,
    pub approved_count: i64,
    pub approved_minor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosDayCloseRow {
    pub id: String,
    pub state: String,
    pub prepared_by: String,
    pub prepared_at: i64,
    pub confirmed_by: Option<String>,
    pub confirmed_at: Option<i64>,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosSaleRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub register_id: String,
    pub shift_id: String,
    pub number: String,
    pub business_date: String,
    pub state: String,
    pub currency: String,
    pub party_id: Option<String>,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub discount_minor: i64,
    pub total_minor: i64,
    pub created_by: String,
    pub created_at: i64,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosLineRow {
    pub id: String,
    pub sale_id: String,
    pub fulfillment_id: String,
    pub fulfillment_state: String,
    pub product_id: String,
    pub name: String,
    pub quantity_milli: i64,
    pub price_minor: i64,
    pub discount_minor: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub returned_milli: i64,
    pub refunded_minor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosPaymentRow {
    pub id: String,
    pub sale_id: String,
    pub method: String,
    pub state: String,
    pub amount_minor: i64,
    pub tendered_minor: i64,
    pub reference: String,
    pub created_by: String,
    pub created_at: i64,
    pub version: i64,
    pub provider_confirmed: bool,
}

/// Raw owner facts before policy projection and derived aggregates.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PosWorkspaceSource {
    pub registers: Vec<PosRegisterRow>,
    pub settings: Option<PosSettingsRow>,
    pub shifts: Vec<PosShiftRow>,
    pub cash_movements: Vec<PosCashMovementRow>,
    pub cash_summary: PosCashSummary,
    pub day_close: Option<PosDayCloseRow>,
    pub sales: Vec<PosSaleRow>,
    pub sale: Option<PosSaleRow>,
    pub lines: Vec<PosLineRow>,
    pub payments: Vec<PosPaymentRow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosDay {
    pub business_date: String,
    /// `open` when no close exists for the current business date.
    pub state: String,
    pub close: Option<PosDayCloseRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PosWorkspace {
    pub business_date: String,
    pub registers: Vec<PosRegisterRow>,
    pub settings: Option<PosSettingsRow>,
    pub shifts: Vec<PosShiftRow>,
    pub cash_movements: Vec<PosCashMovementRow>,
    pub cash_summary: PosCashSummary,
    pub day: PosDay,
    pub sales: Vec<PosSaleRow>,
    pub sale: Option<PosSaleRow>,
    pub lines: Vec<PosLineRow>,
    pub payments: Vec<PosPaymentRow>,
    pub permissions: BTreeMap<&'static str, bool>,
    pub entitlements: PosWorkspaceEntitlements,
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
}

/// Exact top-level wire contract of `data`. The HTTP ingress rejects any
/// response that is not this object, so a partial or extended read cannot be
/// presented to a caller.
pub const WORKSPACE_KEYS: [&str; 17] = [
    "businessDate",
    "registers",
    "settings",
    "shifts",
    "cashMovements",
    "cashSummary",
    "day",
    "sales",
    "sale",
    "lines",
    "payments",
    "permissions",
    "entitlements",
    "purchases",
    "userId",
    "companyId",
    "branchId",
];

fn text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.chars().count() <= maximum && !value.chars().any(char::is_control)
}

fn identifier(value: &str) -> bool {
    RecordId::new(value).is_ok()
}

fn nonnegative(value: i64) -> bool {
    (0..=MAX_SAFE_INTEGER).contains(&value)
}

fn positive(value: i64) -> bool {
    (1..=MAX_SAFE_INTEGER).contains(&value)
}

fn signed(value: i64) -> bool {
    (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&value)
}

pub(crate) fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

fn currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn valid_register(register: &PosRegisterRow, actor: &PosReadActor) -> bool {
    identifier(&register.id)
        && actor.in_scope(&register.company_id, Some(register.branch_id.as_str()))
        && identifier(&register.branch_id)
        && identifier(&register.warehouse_id)
        && text(&register.name, 200)
        && matches!(register.mode.as_str(), "retail" | "restaurant")
        && positive(register.version)
        && text(&register.warehouse_name, 200)
}

fn valid_settings(settings: &PosSettingsRow) -> bool {
    currency(&settings.currency)
        && settings.timezone == "Asia/Bangkok"
        && (0..=10_000).contains(&settings.cashier_discount_bps)
        && positive(settings.version)
}

fn valid_shift(shift: &PosShiftRow, actor: &PosReadActor, registers: &BTreeSet<&str>) -> bool {
    identifier(&shift.id)
        && identifier(&shift.register_id)
        && registers.contains(shift.register_id.as_str())
        && actor.in_scope(&shift.company_id, Some(shift.branch_id.as_str()))
        && valid_date(&shift.business_date)
        && identifier(&shift.opened_by)
        && nonnegative(shift.opened_at)
        && nonnegative(shift.opening_minor)
        && SHIFT_STATES.contains(&shift.state.as_str())
        && shift.counted_minor.is_none_or(nonnegative)
        && shift.expected_minor.is_none_or(nonnegative)
        && shift.variance_minor.is_none_or(signed)
        && shift.closed_by.as_deref().is_none_or(identifier)
        && shift.closed_at.is_none_or(nonnegative)
        && positive(shift.version)
        && signed(shift.cash_expected_minor)
        && text(&shift.register_name, 200)
}

fn valid_cash_movement(movement: &PosCashMovementRow, shifts: &BTreeSet<&str>) -> bool {
    identifier(&movement.id)
        && shifts.contains(movement.shift_id.as_str())
        && identifier(&movement.register_id)
        && movement.amount_minor != 0
        && signed(movement.amount_minor)
        && text(&movement.reason, 1_000)
        && identifier(&movement.created_by)
        && movement.approved_by.as_deref().is_none_or(identifier)
        && nonnegative(movement.created_at)
}

fn valid_summary(summary: &PosCashSummary) -> bool {
    nonnegative(summary.pending_count)
        && nonnegative(summary.pending_minor)
        && nonnegative(summary.approved_count)
        && nonnegative(summary.approved_minor)
}

fn valid_day_close(close: &PosDayCloseRow, business_date: &str) -> bool {
    identifier(&close.id)
        && DAY_STATES.contains(&close.state.as_str())
        && identifier(&close.prepared_by)
        && nonnegative(close.prepared_at)
        && positive(close.version)
        && match close.state.as_str() {
            "prepared" => close.confirmed_by.is_none() && close.confirmed_at.is_none(),
            _ => {
                close
                    .confirmed_by
                    .as_deref()
                    .is_some_and(|reviewer| identifier(reviewer) && reviewer != close.prepared_by)
                    && close.confirmed_at.is_some_and(nonnegative)
            }
        }
        && valid_date(business_date)
}

fn valid_sale(sale: &PosSaleRow, actor: &PosReadActor, registers: &BTreeSet<&str>) -> bool {
    identifier(&sale.id)
        && actor.in_scope(&sale.company_id, Some(sale.branch_id.as_str()))
        && identifier(&sale.branch_id)
        && identifier(&sale.register_id)
        && registers.contains(sale.register_id.as_str())
        && identifier(&sale.shift_id)
        && text(&sale.number, 200)
        && valid_date(&sale.business_date)
        && SALE_STATES.contains(&sale.state.as_str())
        && currency(&sale.currency)
        && sale.party_id.as_deref().is_none_or(identifier)
        && nonnegative(sale.subtotal_minor)
        && nonnegative(sale.tax_minor)
        && nonnegative(sale.discount_minor)
        && nonnegative(sale.total_minor)
        && identifier(&sale.created_by)
        && nonnegative(sale.created_at)
        && positive(sale.version)
}

fn valid_line(line: &PosLineRow, sale_id: &str) -> bool {
    identifier(&line.id)
        && line.sale_id == sale_id
        && identifier(&line.fulfillment_id)
        && FULFILLMENT_STATES.contains(&line.fulfillment_state.as_str())
        && identifier(&line.product_id)
        && text(&line.name, 200)
        && positive(line.quantity_milli)
        && nonnegative(line.price_minor)
        && nonnegative(line.discount_minor)
        && nonnegative(line.subtotal_minor)
        && nonnegative(line.tax_minor)
        && (0..=line.quantity_milli).contains(&line.returned_milli)
        && nonnegative(line.refunded_minor)
}

fn valid_payment(payment: &PosPaymentRow, sale_id: &str) -> bool {
    identifier(&payment.id)
        && payment.sale_id == sale_id
        && PAYMENT_METHODS.contains(&payment.method.as_str())
        && PAYMENT_STATES.contains(&payment.state.as_str())
        && positive(payment.amount_minor)
        && payment.tendered_minor >= payment.amount_minor
        && payment.tendered_minor <= MAX_SAFE_INTEGER
        && payment.reference.chars().count() <= 200
        && !payment.reference.chars().any(char::is_control)
        && identifier(&payment.created_by)
        && nonnegative(payment.created_at)
        && positive(payment.version)
}

/// Validate the bounded source, apply Company/branch isolation and build the
/// exact owner payload. Every array is bounded, every identifier and amount is
/// checked, and a selected sale must be visible in the current scope.
pub fn assemble(
    actor: &PosReadActor,
    query: &PosWorkspaceQuery,
    entitlements: &PosWorkspaceEntitlements,
    business_date: &str,
    source: PosWorkspaceSource,
) -> Result<PosWorkspace, BusinessError> {
    query.validate()?;
    entitlements.require_capability(query.tab)?;
    if !valid_date(business_date) {
        return Err(source_invalid());
    }
    if source.registers.len() > MAX_REGISTERS
        || source.shifts.len() > MAX_SHIFTS
        || source.cash_movements.len() > MAX_CASH_MOVEMENTS
        || source.sales.len() > query.limit
        || source.lines.len() > MAX_LINES
        || source.payments.len() > MAX_PAYMENTS
    {
        return Err(limit_exceeded());
    }

    let mut register_ids = BTreeSet::new();
    for register in &source.registers {
        if !valid_register(register, actor) || !register_ids.insert(register.id.as_str()) {
            return Err(source_invalid());
        }
    }

    if source
        .settings
        .as_ref()
        .is_some_and(|value| !valid_settings(value))
    {
        return Err(source_invalid());
    }

    let mut shift_ids = BTreeSet::new();
    for shift in &source.shifts {
        if !valid_shift(shift, actor, &register_ids) || !shift_ids.insert(shift.id.as_str()) {
            return Err(source_invalid());
        }
    }

    let mut movement_ids = BTreeSet::new();
    for movement in &source.cash_movements {
        if !valid_cash_movement(movement, &shift_ids) || !movement_ids.insert(movement.id.as_str())
        {
            return Err(source_invalid());
        }
    }

    if !valid_summary(&source.cash_summary) {
        return Err(source_invalid());
    }

    if source
        .day_close
        .as_ref()
        .is_some_and(|close| !valid_day_close(close, business_date))
    {
        return Err(source_invalid());
    }

    let mut sale_ids = BTreeSet::new();
    for sale in &source.sales {
        if !valid_sale(sale, actor, &register_ids) || !sale_ids.insert(sale.id.as_str()) {
            return Err(source_invalid());
        }
    }

    let day_state = source
        .day_close
        .as_ref()
        .map_or_else(|| "open".to_owned(), |close| close.state.clone());

    let (sale, lines, payments) = match query.sale_id.as_deref() {
        Some(sale_id) => {
            let sale = source.sale.ok_or_else(not_found)?;
            if sale.id != sale_id || !valid_sale(&sale, actor, &register_ids) {
                return Err(not_found());
            }
            let mut line_ids = BTreeSet::new();
            for line in &source.lines {
                if !valid_line(line, sale_id) || !line_ids.insert(line.id.as_str()) {
                    return Err(source_invalid());
                }
            }
            let mut payment_ids = BTreeSet::new();
            for payment in &source.payments {
                if !valid_payment(payment, sale_id) || !payment_ids.insert(payment.id.as_str()) {
                    return Err(source_invalid());
                }
            }
            (Some(sale), source.lines, source.payments)
        }
        None => {
            if source.sale.is_some() || !source.lines.is_empty() || !source.payments.is_empty() {
                return Err(source_invalid());
            }
            (None, Vec::new(), Vec::new())
        }
    };

    Ok(PosWorkspace {
        business_date: business_date.to_owned(),
        registers: source.registers,
        settings: source.settings,
        shifts: source.shifts,
        cash_movements: source.cash_movements,
        cash_summary: source.cash_summary,
        day: PosDay {
            business_date: business_date.to_owned(),
            state: day_state,
            close: source.day_close,
        },
        sales: source.sales,
        sale,
        lines,
        payments,
        permissions: actor.permissions(),
        entitlements: entitlements.clone(),
        user_id: actor.user_id.clone(),
        company_id: actor.company_id.as_str().to_owned(),
        branch_id: actor
            .branch_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
    })
}

fn option_string(value: &Option<String>) -> Value {
    value.clone().map_or(Value::Null, Value::String)
}

fn option_number(value: Option<i64>) -> Value {
    value.map_or(Value::Null, |value| json!(value))
}

impl PosRegisterRow {
    fn to_wire(&self) -> Value {
        json!({
            "id": self.id,
            "company_id": self.company_id,
            "branch_id": self.branch_id,
            "warehouse_id": self.warehouse_id,
            "name": self.name,
            "mode": self.mode,
            "active": self.active,
            "version": self.version,
            "warehouse_name": self.warehouse_name,
        })
    }
}

impl PosSettingsRow {
    fn to_wire(&self) -> Value {
        json!({
            "enabled": self.enabled,
            "currency": self.currency,
            "timezone": self.timezone,
            "cashier_discount_bps": self.cashier_discount_bps,
            "version": self.version,
        })
    }
}

impl PosShiftRow {
    fn to_wire(&self) -> Value {
        json!({
            "id": self.id,
            "register_id": self.register_id,
            "register_name": self.register_name,
            "company_id": self.company_id,
            "branch_id": self.branch_id,
            "business_date": self.business_date,
            "opened_by": self.opened_by,
            "opened_at": self.opened_at,
            "opening_minor": self.opening_minor,
            "state": self.state,
            "counted_minor": option_number(self.counted_minor),
            "expected_minor": option_number(self.expected_minor),
            "variance_minor": option_number(self.variance_minor),
            "closed_by": option_string(&self.closed_by),
            "closed_at": option_number(self.closed_at),
            "version": self.version,
            "cash_expected_minor": self.cash_expected_minor,
        })
    }
}

impl PosCashMovementRow {
    fn to_wire(&self) -> Value {
        json!({
            "id": self.id,
            "shift_id": self.shift_id,
            "register_id": self.register_id,
            "amount_minor": self.amount_minor,
            "reason": self.reason,
            "created_by": self.created_by,
            "approved_by": option_string(&self.approved_by),
            "created_at": self.created_at,
        })
    }
}

impl PosCashSummary {
    fn to_wire(&self) -> Value {
        json!({
            "pendingCount": self.pending_count,
            "pendingMinor": self.pending_minor,
            "approvedCount": self.approved_count,
            "approvedMinor": self.approved_minor,
        })
    }
}

impl PosDayCloseRow {
    fn to_wire(&self) -> Value {
        json!({
            "id": self.id,
            "state": self.state,
            "prepared_by": self.prepared_by,
            "prepared_at": self.prepared_at,
            "confirmed_by": option_string(&self.confirmed_by),
            "confirmed_at": option_number(self.confirmed_at),
            "version": self.version,
        })
    }
}

impl PosSaleRow {
    fn to_wire(&self) -> Value {
        json!({
            "id": self.id,
            "company_id": self.company_id,
            "branch_id": self.branch_id,
            "register_id": self.register_id,
            "shift_id": self.shift_id,
            "number": self.number,
            "business_date": self.business_date,
            "state": self.state,
            "currency": self.currency,
            "party_id": option_string(&self.party_id),
            "subtotal_minor": self.subtotal_minor,
            "tax_minor": self.tax_minor,
            "discount_minor": self.discount_minor,
            "total_minor": self.total_minor,
            "created_by": self.created_by,
            "created_at": self.created_at,
            "version": self.version,
        })
    }
}

impl PosLineRow {
    fn to_wire(&self) -> Value {
        json!({
            "id": self.id,
            "sale_id": self.sale_id,
            "fulfillment_id": self.fulfillment_id,
            "fulfillment_state": self.fulfillment_state,
            "product_id": self.product_id,
            "name": self.name,
            "quantity_milli": self.quantity_milli,
            "price_minor": self.price_minor,
            "discount_minor": self.discount_minor,
            "subtotal_minor": self.subtotal_minor,
            "tax_minor": self.tax_minor,
            "returned_milli": self.returned_milli,
            "refunded_minor": self.refunded_minor,
        })
    }
}

impl PosPaymentRow {
    fn to_wire(&self) -> Value {
        json!({
            "id": self.id,
            "sale_id": self.sale_id,
            "method": self.method,
            "state": self.state,
            "amount_minor": self.amount_minor,
            "tendered_minor": self.tendered_minor,
            "reference": self.reference,
            "created_by": self.created_by,
            "created_at": self.created_at,
            "version": self.version,
            "provider_payment_confirmed": self.provider_confirmed,
        })
    }
}

impl PosDay {
    fn to_wire(&self) -> Value {
        json!({
            "businessDate": self.business_date,
            "state": self.state,
            "close": self.close.as_ref().map_or(Value::Null, PosDayCloseRow::to_wire),
        })
    }
}

impl PosWorkspace {
    pub fn to_wire(&self) -> Value {
        let mut object = Map::new();
        object.insert("businessDate".into(), json!(self.business_date));
        object.insert(
            "registers".into(),
            Value::Array(self.registers.iter().map(PosRegisterRow::to_wire).collect()),
        );
        object.insert(
            "settings".into(),
            self.settings
                .as_ref()
                .map_or(Value::Null, PosSettingsRow::to_wire),
        );
        object.insert(
            "shifts".into(),
            Value::Array(self.shifts.iter().map(PosShiftRow::to_wire).collect()),
        );
        object.insert(
            "cashMovements".into(),
            Value::Array(
                self.cash_movements
                    .iter()
                    .map(PosCashMovementRow::to_wire)
                    .collect(),
            ),
        );
        object.insert("cashSummary".into(), self.cash_summary.to_wire());
        object.insert("day".into(), self.day.to_wire());
        object.insert(
            "sales".into(),
            Value::Array(self.sales.iter().map(PosSaleRow::to_wire).collect()),
        );
        object.insert(
            "sale".into(),
            self.sale.as_ref().map_or(Value::Null, PosSaleRow::to_wire),
        );
        object.insert(
            "lines".into(),
            Value::Array(self.lines.iter().map(PosLineRow::to_wire).collect()),
        );
        object.insert(
            "payments".into(),
            Value::Array(self.payments.iter().map(PosPaymentRow::to_wire).collect()),
        );
        object.insert(
            "permissions".into(),
            Value::Object(
                self.permissions
                    .iter()
                    .map(|(name, allowed)| ((*name).to_owned(), json!(allowed)))
                    .collect(),
            ),
        );
        object.insert(
            "entitlements".into(),
            json!({
                "packageEnforced": self.entitlements.package_enforced,
                "roomChargeEnabled": self.entitlements.room_charge_enabled,
            }),
        );
        object.insert("purchases".into(), self.entitlements.purchases());
        object.insert("userId".into(), json!(self.user_id));
        object.insert("companyId".into(), json!(self.company_id));
        object.insert(
            "branchId".into(),
            self.branch_id.clone().map_or(Value::Null, Value::String),
        );
        Value::Object(object)
    }
}

/// Current POS business date. `pos_settings.timezone` is constrained to
/// `Asia/Bangkok` (UTC+7, no daylight saving), matching the retained
/// `businessDate()` helper (`domain/pos.ts:47`).
pub fn business_date(now: TimestampMillis) -> String {
    const BANGKOK_OFFSET_MILLIS: i64 = 7 * 60 * 60 * 1_000;
    civil_date(
        now.get()
            .saturating_add(BANGKOK_OFFSET_MILLIS)
            .div_euclid(86_400_000),
    )
}

/// Howard Hinnant's civil-from-days algorithm, exact for the full `i64` day
/// range the read can receive.
fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!("{year:04}-{month:02}-{day:02}")
}
