//! PMS property/room board read facts. These projections are presentation
//! facts only; they are not a current authorization proof, source purchase or
//! command capability. Cross-owner staff labels arrive through the public
//! People port supplied by composition.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const MAX_PROPERTIES: usize = 100;
pub const MAX_RECORDS: usize = 4_000;
pub const MAX_STAFF: usize = 1_000;
pub const PAGE_SIZE: usize = 50;
pub const MAX_PAGE: u32 = 10_000;

/// ECMAScript `String.prototype.trim` removes WhiteSpace and LineTerminator
/// code points. Rust's `str::trim` uses the Unicode White_Space property and
/// strips further characters such as U+0085 NEL. PMS selection text is
/// retained through Zod `.trim()` boundaries, so trim this exact closed set.
pub(crate) fn ecma_trim(value: &str) -> &str {
    value.trim_matches(|c: char| {
        matches!(
            c,
            '\u{0009}'
                | '\u{000A}'
                | '\u{000B}'
                | '\u{000C}'
                | '\u{000D}'
                | '\u{0020}'
                | '\u{00A0}'
                | '\u{1680}'
                | '\u{2000}'
                ..='\u{200A}'
                    | '\u{2028}'
                    | '\u{2029}'
                    | '\u{202F}'
                    | '\u{205F}'
                    | '\u{3000}'
                    | '\u{FEFF}'
        )
    })
}

/// Zod 4.6.7 string limits count Unicode scalars (`chars().count()`),
/// not UTF-16 code units (`String.length`) or UTF-8 bytes (`len()`).
pub(crate) fn cp_len(value: &str) -> usize {
    value.chars().count()
}

pub const PMS_PERMISSIONS: &[&str] = &[
    "pms.read",
    "pms.configure",
    "pms.reservations",
    "pms.frontdesk",
    "pms.cashier",
    "pms.housekeeping",
    "pms.inspect",
    "pms.maintenance",
    "pms.audit",
    "pms.approve",
    "pms.accounting.prepare",
    "pms.accounting.approve",
    "pms.reports",
    "finance.approve",
];

#[derive(Clone, Debug)]
pub struct PmsBoardActor {
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    /// Epoch milliseconds at which the selected work context expires.
    pub expires_at: i64,
}

impl PmsBoardActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }
    fn require_workspace(&self, now: i64) -> Result<(), BusinessError> {
        if !self.has("pms.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission.",
            ));
        }
        if !self.modules.iter().any(|module| module == "pms") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MODULE_FORBIDDEN",
                "Hotels are unavailable to the selected role.",
            ));
        }
        if self.department_id.is_some() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PMS_BRANCH_REQUIRED",
                "Select a company or branch area.",
            ));
        }
        if now >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }
    /// Retained workspace + single-permission gate for owner commands that are
    /// not room-board/front-desk specific (payments, refunds, accounting).
    pub fn require_action(&self, permission: &str, now: i64) -> Result<(), BusinessError> {
        self.require_workspace(now)?;
        if !self.has(permission) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission.",
            ));
        }
        Ok(())
    }
    pub fn require_board(&self, now: i64) -> Result<(), BusinessError> {
        self.require_workspace(now)?;
        if !self.can_room_board() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PMS_HOUSEKEEPING_REQUIRED",
                "Your selected role cannot open the room board.",
            ));
        }
        Ok(())
    }
    pub fn require_front_desk(&self, now: i64) -> Result<(), BusinessError> {
        self.require_workspace(now)?;
        if !self.can_front_desk() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PMS_FRONTDESK_REQUIRED",
                "Your selected role cannot open the front desk.",
            ));
        }
        Ok(())
    }
    pub fn can_room_board(&self) -> bool {
        self.has("pms.housekeeping") || self.has("pms.inspect") || self.has("pms.maintenance")
    }
    pub fn can_front_desk(&self) -> bool {
        self.has("pms.frontdesk")
    }
    pub fn require_section(&self, section: PmsBoardSection) -> Result<(), BusinessError> {
        let allowed = match section {
            PmsBoardSection::Rooms => self.has("pms.housekeeping") || self.has("pms.maintenance"),
            PmsBoardSection::Housekeeping => {
                self.has("pms.housekeeping") || self.has("pms.inspect")
            }
        };
        if !allowed {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PMS_HOUSEKEEPING_REQUIRED",
                "Your selected role cannot open this room board.",
            ));
        }
        Ok(())
    }
    pub fn can_guest_details(&self) -> bool {
        self.has("pms.reservations") || self.has("pms.frontdesk") || self.has("pms.cashier")
    }
    pub fn can_financial(&self) -> bool {
        self.has("pms.cashier") || self.has("pms.reports")
    }
    pub fn can_pricing(&self) -> bool {
        self.can_guest_details()
            || self.has("pms.configure")
            || self.has("pms.accounting.prepare")
            || self.has("pms.accounting.approve")
            || self.has("pms.reports")
    }
    pub fn can_accounting(&self) -> bool {
        self.has("pms.accounting.prepare")
            || self.has("pms.accounting.approve")
            || self.has("pms.reports")
    }
    pub fn can_manage_setup(&self) -> bool {
        self.has("pms.configure") || self.can_accounting()
    }
    pub fn can_review_booking_media(&self) -> bool {
        self.has("records.review") || self.has("commerce.approve")
    }
    pub fn permissions(&self) -> Vec<String> {
        PMS_PERMISSIONS
            .iter()
            .filter(|permission| self.has(permission))
            .map(|permission| (*permission).to_owned())
            .collect()
    }
    pub fn visibility(&self) -> PmsBoardVisibility {
        PmsBoardVisibility {
            guest_details: self.can_guest_details(),
            pricing: self.can_pricing(),
            financial: self.can_financial(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PmsBoardVisibility {
    pub guest_details: bool,
    pub pricing: bool,
    pub financial: bool,
}

/// Current Commercial package entitlement projected through a public owner
/// port. The owner read never invents Commercial SQL; composition binds this
/// to the retained package boundary before enabling the native reader.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PmsBoardEntitlement {
    pub enforced: bool,
    pub history_allowed: bool,
}

impl PmsBoardEntitlement {
    pub fn require_history(self) -> Result<(), BusinessError> {
        if self.enforced && !self.history_allowed {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PACKAGE_REQUIRED",
                "This package does not permit the requested Hotel workspace.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmsBoardSection {
    Rooms,
    Housekeeping,
}

impl PmsBoardSection {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "rooms" => Some(Self::Rooms),
            "housekeeping" => Some(Self::Housekeeping),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rooms => "rooms",
            Self::Housekeeping => "housekeeping",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PmsBoardSelection {
    pub section: PmsBoardSection,
    pub property_id: Option<String>,
    /// One of `""`, `dirty`, `clean` or `inspected`.
    pub status: String,
    pub q: String,
    pub building: String,
    pub floor: String,
    pub type_id: Option<String>,
    pub page: u32,
    pub selected_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

impl Default for PmsBoardSelection {
    fn default() -> Self {
        Self {
            section: PmsBoardSection::Rooms,
            property_id: None,
            status: String::new(),
            q: String::new(),
            building: String::new(),
            floor: String::new(),
            type_id: None,
            page: 0,
            selected_id: None,
            from: None,
            to: None,
        }
    }
}

impl PmsBoardSelection {
    /// Retained Zod `q`, `building` and `floor` use `.trim().max(...)`;
    /// `typeId`/`property`/`selectedId` use `idField` (char length only,
    /// no trim). Measure the trimmed text in Unicode scalars.
    pub fn normalize(&mut self) {
        self.q = ecma_trim(&self.q).to_owned();
        self.building = ecma_trim(&self.building).to_owned();
        self.floor = ecma_trim(&self.floor).to_owned();
    }
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !matches!(self.status.as_str(), "" | "dirty" | "clean" | "inspected")
            || cp_len(ecma_trim(&self.q)) > 200
            || cp_len(ecma_trim(&self.building)) > 120
            || cp_len(ecma_trim(&self.floor)) > 40
            || self
                .type_id
                .as_ref()
                .is_some_and(|value| cp_len(value) > 200)
            || self.page > MAX_PAGE
            || self
                .property_id
                .as_ref()
                .is_some_and(|value| value.is_empty() || cp_len(value) > 200)
            || self
                .selected_id
                .as_ref()
                .is_some_and(|value| value.is_empty() || cp_len(value) > 200)
        {
            return Err(BusinessError::invalid(
                "PMS_BOARD_REQUEST_INVALID",
                "Use one bounded room board selection.",
            ));
        }
        match (&self.from, &self.to) {
            (None, None) => Ok(()),
            (Some(from), Some(to)) => {
                let from = parse_date(from).ok_or_else(invalid_range)?;
                let to = parse_date(to).ok_or_else(invalid_range)?;
                if from > to || to - from > 93 {
                    return Err(invalid_range());
                }
                Ok(())
            }
            _ => Err(invalid_range()),
        }
    }
}

fn invalid_range() -> BusinessError {
    BusinessError::invalid(
        "PMS_LIST_RANGE_INVALID",
        "Choose a list date range of at most 93 days.",
    )
}

pub fn read_limit(message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, "PMS_BOARD_READ_LIMIT", message)
}

pub fn read_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_BOARD_READ_INVALID",
        "The hotel room board could not be read.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PMS_NOT_FOUND",
        "Hotel not found in this workspace.",
    )
}

pub fn record_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PMS_RECORD_NOT_FOUND",
        "Hotel item not found in this workspace.",
    )
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PmsPropertySummary {
    pub id: String,
    pub name: String,
    #[serde(rename = "branchId")]
    pub branch_id: String,
    pub state: String,
    #[serde(rename = "businessDate")]
    pub business_date: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PmsProperty {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub name: String,
    pub business_date: String,
    pub state: String,
    pub version: i64,
    pub config: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PmsRecord {
    pub id: String,
    pub kind: String,
    pub version: i64,
    pub updated_at: i64,
    pub data: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PmsStaff {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsRoomSummary {
    pub total: i64,
    pub active: i64,
    pub dirty: i64,
    pub clean: i64,
    pub inspected: i64,
    pub dnd: i64,
    pub occupied: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsListMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_summary: Option<PmsRoomSummary>,
    pub section: String,
    pub kind: String,
    pub ids: Vec<String>,
    pub from: String,
    pub to: String,
    pub page: u32,
    pub q: String,
    pub status: String,
    pub building: String,
    pub floor: String,
    #[serde(rename = "typeId")]
    pub type_id: String,
    pub has_next: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PmsSnapshot {
    pub property: PmsProperty,
    pub records: Vec<PmsRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_meta: Option<PmsListMeta>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsBookingReadiness {
    pub email: bool,
    pub accounting: bool,
    pub online_accounting: bool,
    pub fees: bool,
    pub connect: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmsWorkspace {
    pub properties: Vec<PmsPropertySummary>,
    pub snapshot: Option<PmsSnapshot>,
    pub default_date: String,
    pub readiness: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub front_ready: Option<bool>,
    pub can_review_booking_media: bool,
    pub booking_readiness: PmsBookingReadiness,
    pub can_manage_setup: bool,
    pub permissions: Vec<String>,
    pub available_payment_methods: Vec<String>,
    pub user_id: String,
    pub branches: Vec<serde_json::Value>,
    pub staff: Vec<PmsStaff>,
    pub accounts: Vec<serde_json::Value>,
    pub charts: Vec<serde_json::Value>,
    pub parties: Vec<serde_json::Value>,
    pub products: Vec<serde_json::Value>,
    pub warehouses: Vec<serde_json::Value>,
    pub documents: Vec<serde_json::Value>,
    pub transfer_requests: Vec<serde_json::Value>,
    pub charge_corrections: Vec<serde_json::Value>,
    pub stripe_connection_ready: bool,
    pub stripe_payments: Vec<serde_json::Value>,
    pub counter_refunds: Vec<serde_json::Value>,
    pub booking_refunds: Vec<serde_json::Value>,
    pub booking_delivery: Vec<serde_json::Value>,
    pub ledger: Option<serde_json::Value>,
    pub control_balances: Option<serde_json::Value>,
    pub day_controls: Option<serde_json::Value>,
}

impl PmsWorkspace {
    pub fn value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Clone, Debug)]
pub struct PmsPropertyRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub name: String,
    pub business_date: String,
    pub state: String,
    pub config_json: String,
    pub version: i64,
}

impl PmsPropertyRow {
    pub fn view(&self) -> Result<PmsProperty, BusinessError> {
        let config = serde_json::from_str(&self.config_json).map_err(|_| read_invalid())?;
        Ok(PmsProperty {
            id: self.id.clone(),
            company_id: self.company_id.clone(),
            branch_id: self.branch_id.clone(),
            name: self.name.clone(),
            business_date: self.business_date.clone(),
            state: self.state.clone(),
            version: self.version,
            config,
        })
    }
    pub fn summary(&self) -> PmsPropertySummary {
        PmsPropertySummary {
            id: self.id.clone(),
            name: self.name.clone(),
            branch_id: self.branch_id.clone(),
            state: self.state.clone(),
            business_date: self.business_date.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PmsBoardSetup {
    pub has_room_types: bool,
    pub has_active_rooms: bool,
    pub has_active_rates: bool,
}

#[derive(Clone, Debug)]
pub struct PmsBoardData {
    pub property: PmsPropertyRow,
    pub records: Vec<PmsRecord>,
    pub ids: Vec<String>,
    pub has_next: bool,
    pub room_summary: PmsRoomSummary,
    pub setup: PmsBoardSetup,
    pub from: String,
    pub to: String,
}

pub fn validate_properties(
    actor: &PmsBoardActor,
    rows: Vec<PmsPropertyRow>,
) -> Result<Vec<PmsPropertyRow>, BusinessError> {
    if rows.len() > MAX_PROPERTIES {
        return Err(read_limit("The hotel list exceeds its bounded read."));
    }
    for row in &rows {
        if row.id.is_empty()
            || row.id.len() > 200
            || row.company_id != actor.company_id
            || actor
                .branch_id
                .as_ref()
                .is_some_and(|branch| branch != &row.branch_id)
            || row.name.chars().count() > 400
            || row.business_date.len() != 10
            || row.version < 1
        {
            return Err(read_limit("The hotel list contains an invalid value."));
        }
    }
    Ok(rows)
}

pub fn validate_board_data(
    actor: &PmsBoardActor,
    data: PmsBoardData,
) -> Result<PmsBoardData, BusinessError> {
    if data.property.company_id != actor.company_id
        || actor
            .branch_id
            .as_ref()
            .is_some_and(|branch| branch != &data.property.branch_id)
        || data.records.len() > MAX_RECORDS
        || data.ids.len() > PAGE_SIZE
        || data.from.len() != 10
        || data.to.len() != 10
        || data.from > data.to
    {
        return Err(read_invalid());
    }
    for record in &data.records {
        validate_record(record)?;
    }
    let visible: std::collections::BTreeSet<&str> =
        data.records.iter().map(|row| row.id.as_str()).collect();
    if data.ids.iter().any(|id| !visible.contains(id.as_str())) {
        return Err(read_invalid());
    }
    Ok(data)
}

pub(crate) fn validate_record(record: &PmsRecord) -> Result<(), BusinessError> {
    if record.id.is_empty()
        || record.id.len() > 200
        || record.kind.is_empty()
        || record.kind.len() > 40
        || record.version < 1
        || record.updated_at < 0
        || !record.data.is_object()
    {
        return Err(read_invalid());
    }
    Ok(())
}

pub fn visible_records(
    actor: &PmsBoardActor,
    records: Vec<PmsRecord>,
) -> Result<Vec<PmsRecord>, BusinessError> {
    let mut visible = Vec::with_capacity(records.len());
    for mut record in records {
        validate_record(&record)?;
        if !actor.can_pricing() && record.kind == "rate" {
            continue;
        }
        if !actor.can_guest_details()
            && matches!(
                record.kind.as_str(),
                "guest" | "reservation" | "guest_account" | "entry" | "refund" | "credit" | "shift"
            )
        {
            continue;
        }
        if record.kind == "stay" {
            if !actor.can_financial()
                && let Some(data) = record.data.as_object_mut()
            {
                data.insert("prices".into(), serde_json::Value::Array(Vec::new()));
            }
            if !actor.can_guest_details()
                && let Some(data) = record.data.as_object_mut()
            {
                data.remove("upgrades");
                data.remove("bookingPolicy");
            }
        }
        if record.kind == "guest_account"
            && !actor.can_financial()
            && let Some(data) = record.data.as_object_mut()
        {
            for key in ["due", "advance", "advanceTax", "deposit"] {
                data.insert(key.into(), serde_json::Value::from(0));
            }
        }
        visible.push(record);
    }
    if visible.len() > MAX_RECORDS {
        return Err(read_limit("The hotel room board exceeds its bounded read."));
    }
    Ok(visible)
}

pub fn default_date(now: i64) -> String {
    // The retained PMS read uses Asia/Bangkok. An epoch conversion without a
    // timezone dependency is sufficient because commands still carry their
    // own selected business date and authorization.
    format_date((now + 7 * 60 * 60 * 1_000).div_euclid(86_400_000))
}

pub fn readiness(property: &PmsProperty, records: &[PmsRecord]) -> Vec<String> {
    let setup = PmsBoardSetup {
        has_room_types: records.iter().any(|record| record.kind == "room_type"),
        has_active_rooms: records.iter().any(|record| {
            record.kind == "room"
                && record
                    .data
                    .get("active")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
        }),
        has_active_rates: records.iter().any(|record| {
            record.kind == "rate"
                && record
                    .data
                    .get("active")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
        }),
    };
    readiness_from_facts(property, setup)
}

pub fn readiness_from_facts(property: &PmsProperty, setup: PmsBoardSetup) -> Vec<String> {
    let config = &property.config;
    let account_complete = config
        .get("chartId")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| !value.is_empty())
        && config
            .get("accounts")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|accounts| {
                !accounts.is_empty()
                    && accounts
                        .values()
                        .all(|value| value.as_str().is_some_and(|value| !value.is_empty()))
            });
    let approved = config
        .get("approvedBy")
        .is_some_and(|value| !value.is_null() && value.as_str() != Some(""));
    let currency = config.get("currency").and_then(serde_json::Value::as_str);
    let minor_unit = config.get("minorUnit").and_then(serde_json::Value::as_i64);
    let mut checks = Vec::new();
    if !setup.has_room_types {
        checks.push("Add a room type".to_owned());
    }
    if !setup.has_active_rooms {
        checks.push("Add rooms".to_owned());
    }
    if !setup.has_active_rates {
        checks.push("Add a rate plan".to_owned());
    }
    if !approved {
        checks.push("Have Finance approve the setup".to_owned());
    }
    if !account_complete {
        checks.push("Complete the chart of accounts setup".to_owned());
    }
    if currency != Some("THB") || minor_unit != Some(2) {
        checks.push("The pilot supports THB".to_owned());
    }
    checks
}

pub fn booking_readiness(property: &PmsProperty) -> PmsBookingReadiness {
    let config = &property.config;
    let approved = |key: &str| {
        config
            .get(key)
            .and_then(serde_json::Value::as_object)
            .and_then(|value| value.get("approvedBy"))
            .is_some_and(|value| !value.is_null() && value.as_str() != Some(""))
    };
    PmsBookingReadiness {
        email: false,
        accounting: config
            .get("approvedBy")
            .is_some_and(|value| !value.is_null() && value.as_str() != Some("")),
        online_accounting: approved("onlineAccounting"),
        fees: approved("bookingFees"),
        connect: false,
    }
}

pub fn redact_config(actor: &PmsBoardActor, property: &mut PmsProperty) {
    if actor.can_accounting() {
        return;
    }
    if let Some(config) = property.config.as_object_mut() {
        config.insert("chartId".into(), serde_json::Value::String(String::new()));
        if let Some(accounts) = config.get_mut("accounts").and_then(|v| v.as_object_mut()) {
            for value in accounts.values_mut() {
                *value = serde_json::Value::String(String::new());
            }
        }
    }
}

pub fn staff(mut rows: Vec<PmsStaff>) -> Result<Vec<PmsStaff>, BusinessError> {
    if rows.len() > MAX_STAFF {
        return Err(read_limit("The hotel staff list exceeds its bounded read."));
    }
    rows.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.id.cmp(&right.id))
    });
    rows.dedup_by(|left, right| left.id == right.id);
    if rows
        .iter()
        .any(|row| row.id.is_empty() || row.id.len() > 200 || row.name.chars().count() > 400)
    {
        return Err(read_limit(
            "The hotel staff list contains an invalid value.",
        ));
    }
    Ok(rows)
}

pub(crate) fn parse_date(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year = value.get(0..4)?.parse::<i64>().ok()?;
    let month = value.get(5..7)?.parse::<i64>().ok()?;
    let day = value.get(8..10)?.parse::<i64>().ok()?;
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 0,
    }
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month, day)
}

fn format_date(days: i64) -> String {
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}
