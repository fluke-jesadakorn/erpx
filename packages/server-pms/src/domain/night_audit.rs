//! PMS day-close (night audit) command facts.
//!
//! The retained `night.start`, `night.resume` and `night.step` transitions are
//! reduced to PMS-owned facts: the property row, the `night` records of the
//! current business date, the retained stay/shift blocker set, and the room
//! capacity, stayover tasks and group allotments a completed close touches.
//! The retained day-control and Finance setup guards read other owners' counts;
//! they arrive as bounded facts through a public port and are rendered here
//! with the retained blocker text.
//!
//! Room nights are charged to guest accounts by the retained engine and become
//! Finance journals through `effects.postings`. That posting is deliberately
//! not implemented in this slice: `night.step` fails closed with
//! `PMS_NIGHT_POSTING_PORT_REQUIRED` whenever an in-house stay still lacks its
//! night charge, and this module never posts a journal or charges a folio.
use super::room_board::{
    PmsProperty, PmsPropertyRow, PmsRecord, cp_len, not_found, parse_date, record_not_found,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};

/// Bounded reads. The retained engine reads the complete hotel snapshot; these
/// bounds fail a command closed instead of loading an unbounded hotel.
pub const MAX_STAYS: usize = 4_000;
pub const MAX_SHIFTS: usize = 1_000;
pub const MAX_ROOM_TYPES: usize = 500;
/// Capacity sums every active room of every type, not one selected page.
pub const MAX_ROOMS: usize = 2_000;
pub const MAX_RATES: usize = 500;
pub const MAX_BLOCKS: usize = 1_000;
/// `night` records are unique per business date; the bound only guards the
/// retained "id or date" selection against corrupt owner data.
pub const MAX_NIGHTS: usize = 16;
pub const MAX_TASKS: usize = 1_000;
pub const MAX_NIGHT_ENTRIES: usize = 4_000;
pub const MAX_ID: usize = 128;
/// Retained `postNights` batch size; only used to describe the posting port.
pub const NIGHT_POSTING_BATCH: usize = 25;

/// Retained `pmsAccountKeys`, in the canonical config order.
pub const NIGHT_ACCOUNT_KINDS: [&str; 14] = [
    "cash",
    "bank",
    "edc",
    "receivable",
    "advance",
    "deposit",
    "revenue",
    "service",
    "tax",
    "prepaidTax",
    "inventory",
    "consumption",
    "commission",
    "cashDifference",
];

/// The retained day-close command family. Each operation keeps the retained
/// command name for ingress and the retained `pms.<name>` receipt operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmsNightAuditOperation {
    Start,
    Resume,
    Step,
}

impl PmsNightAuditOperation {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "night.start" => Some(Self::Start),
            "night.resume" => Some(Self::Resume),
            "night.step" => Some(Self::Step),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "night.start",
            Self::Resume => "night.resume",
            Self::Step => "night.step",
        }
    }

    /// Retained receipt, audit and outbox operation (`pms.${op}`).
    pub fn receipt_operation(self) -> &'static str {
        match self {
            Self::Start => "pms.night.start",
            Self::Resume => "pms.night.resume",
            Self::Step => "pms.night.step",
        }
    }

    /// Retained `operationPermission`: every `night.*` operation needs
    /// `pms.audit`. Composition still resolves and revalidates the permission.
    pub fn permission(self) -> &'static str {
        "pms.audit"
    }

    /// Retained `night.step` is only accepted from the durable day-close
    /// worker, never from a user command.
    pub fn worker_only(self) -> bool {
        matches!(self, Self::Step)
    }

    /// Whether the operation names an existing `night` record.
    pub fn target_is_night(self) -> bool {
        !matches!(self, Self::Start)
    }
}

/// Retained `night.start`/`night.resume`/`night.step` input. `expectedVersion`
/// is the current `pms_properties.version`; the retained worker (`night.step`)
/// reads the property inside its own serializable unit and carries no version.
#[derive(Clone, Debug)]
pub struct PmsNightAuditCommand {
    pub operation: PmsNightAuditOperation,
    pub property_id: String,
    pub expected_version: Option<i64>,
    /// `night` record id for `night.resume`/`night.step`.
    pub night_id: Option<String>,
    /// True only for the durable day-close worker.
    pub worker: bool,
}

impl PmsNightAuditCommand {
    pub fn operation(&self) -> PmsNightAuditOperation {
        self.operation
    }

    /// The `night` record this command names, when the operation names one.
    pub fn target_id(&self) -> Option<&str> {
        self.night_id.as_deref()
    }

    /// Whether the retained result id is the named night record. `night.start`
    /// mints the record, so its result id is new.
    pub fn result_id_is_target(&self) -> bool {
        self.operation.target_is_night()
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_id(&self.property_id)
            || self.expected_version.is_some_and(|version| version < 1)
            || (self.operation.target_is_night() != self.night_id.is_some())
            || self.night_id.as_deref().is_some_and(|id| !valid_id(id))
        {
            return Err(invalid());
        }
        Ok(())
    }
}

/// Retained `idField`: one to 128 Unicode scalars, never trimmed.
fn valid_id(value: &str) -> bool {
    !value.is_empty() && cp_len(value) <= MAX_ID && !value.chars().any(char::is_control)
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PMS_NIGHT_AUDIT_REQUEST_INVALID",
        "Use one bounded day-close action with the current hotel version.",
    )
}

pub fn record_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_NIGHT_AUDIT_RECORD_INVALID",
        "The hotel day-close record could not be read safely.",
    )
}

pub fn read_limit(message: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_NIGHT_AUDIT_READ_LIMIT",
        message,
    )
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn version_conflict() -> BusinessError {
    conflict(
        "PMS_VERSION_CONFLICT",
        "Hotel data changed. Load the latest version and review again.",
    )
}

/// The retained posting boundary. Posting the outstanding room nights charges
/// guest-account folios and Finance journals through `ports.trade`; until that
/// public consumer is mounted the day close must not claim the night settled.
pub fn posting_port_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_NIGHT_POSTING_PORT_REQUIRED",
        "Post the outstanding room nights through the Finance posting port before closing the day.",
    )
}

/// Owner command context. The board actor supplies Company/branch scope,
/// permissions, module and work-context expiry; receipt/audit facts add the
/// selected assignment identity without granting anything.
#[derive(Clone, Debug)]
pub struct PmsNightAuditActor {
    pub board: super::room_board::PmsBoardActor,
    pub assignment_id: String,
    pub role: String,
    pub revision: i64,
    pub assignment_version: i64,
}

impl PmsNightAuditActor {
    /// Mirror of the retained workspace gate (read permission, module, branch
    /// area, unexpired work context) followed by the retained `pms.audit`
    /// permission and the retained `PMS_WORKER_ONLY` guard for `night.step`.
    /// `room_board`'s workspace gate is crate-private, so the gate is repeated
    /// here instead of widened there.
    pub fn require_command(
        &self,
        operation: PmsNightAuditOperation,
        worker: bool,
        now: i64,
    ) -> Result<(), BusinessError> {
        let board = &self.board;
        if !board.has("pms.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission.",
            ));
        }
        if !board.modules.iter().any(|module| module == "pms") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MODULE_FORBIDDEN",
                "Hotels are unavailable to the selected role.",
            ));
        }
        if board.department_id.is_some() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PMS_BRANCH_REQUIRED",
                "Select a company or branch area.",
            ));
        }
        if now >= board.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        if !board.has(operation.permission()) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission.",
            ));
        }
        if operation.worker_only() && !worker {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PMS_WORKER_ONLY",
                "This command is only used by day-close work",
            ));
        }
        if self.assignment_id.is_empty()
            || cp_len(&self.assignment_id) > 200
            || self.assignment_id.chars().any(char::is_control)
            || self.role.is_empty()
            || cp_len(&self.role) > 200
            || self.role.chars().any(char::is_control)
            || self.revision < 0
            || self.assignment_version < 1
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PMS_COMMAND_CONTEXT_INVALID",
                "Your selected work context cannot submit this hotel action.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsNightAuditResult {
    pub id: String,
    pub property_id: String,
    pub version: i64,
}

/// Retained `hospitalityDayControls` counts. These are POS/Finance/Inventory
/// owned reads; the PMS command only decides whether each count blocks the
/// close. Display-only totals are not part of the guard.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PmsNightDayControlFacts {
    pub open_shifts: i64,
    pub unreviewed_variances: i64,
    pub open_bills: i64,
    pub pending_payments: i64,
    pub pending_hotel_transfers: i64,
    pub pending_charge_corrections: i64,
    pub unposted_booking_receipts: i64,
    pub failed_booking_refunds: i64,
    pub unposted_counter_receipts: i64,
    pub failed_counter_refunds: i64,
    pub unresolved_refunds: i64,
    pub failed_jobs: i64,
    pub missing_room_charges: i64,
    pub missing_direct_invoices: i64,
    pub unmatched_verified_transfers: i64,
    pub unmatched_verified_hotel_transfers: i64,
    pub unbalanced_journals: i64,
    pub stock_value_minor: i64,
    pub stock_ledger_minor: i64,
}

/// Retained `hospitalityDayControls` blocker text, in the retained order.
impl PmsNightDayControlFacts {
    pub fn blockers(&self) -> Vec<String> {
        let mut blockers = Vec::new();
        push_count(&mut blockers, self.open_shifts, |n| {
            format!("{n} restaurant cash session(s) still open")
        });
        push_count(&mut blockers, self.unreviewed_variances, |n| {
            format!("{n} restaurant cash variance(s) awaiting independent review")
        });
        push_count(&mut blockers, self.open_bills, |n| {
            format!("{n} restaurant bill(s) still open for this business date")
        });
        push_count(&mut blockers, self.pending_payments, |n| {
            format!("{n} restaurant payment(s) awaiting a result or bank verification")
        });
        push_count(&mut blockers, self.pending_hotel_transfers, |n| {
            format!("{n} hotel bank transfer(s) awaiting Finance verification")
        });
        push_count(&mut blockers, self.pending_charge_corrections, |n| {
            format!("{n} hotel guest account correction(s) awaiting independent review")
        });
        push_count(&mut blockers, self.unposted_booking_receipts, |n| {
            format!("{n} verified online hotel receipt(s) still need accounting recovery")
        });
        push_count(&mut blockers, self.failed_booking_refunds, |n| {
            format!("{n} online hotel refund(s) need recovery")
        });
        push_count(&mut blockers, self.unposted_counter_receipts, |n| {
            format!("{n} verified counter receipt(s) still need accounting recovery")
        });
        push_count(&mut blockers, self.failed_counter_refunds, |n| {
            format!("{n} counter refund(s) need recovery")
        });
        push_count(&mut blockers, self.unresolved_refunds, |n| {
            format!("{n} restaurant refund(s) awaiting resolution")
        });
        push_count(&mut blockers, self.failed_jobs, |n| {
            format!("{n} restaurant background job(s) need recovery")
        });
        push_count(&mut blockers, self.missing_room_charges, |n| {
            format!("{n} room charge(s) lack a hotel revenue posting")
        });
        push_count(&mut blockers, self.missing_direct_invoices, |n| {
            format!("{n} direct restaurant sale(s) lack an invoice or revenue posting")
        });
        push_count(&mut blockers, self.unmatched_verified_transfers, |n| {
            format!("{n} verified bank transfer(s) lack matching imported bank evidence")
        });
        push_count(
            &mut blockers,
            self.unmatched_verified_hotel_transfers,
            |n| format!("{n} verified hotel transfer(s) lack matching imported bank evidence"),
        );
        if self.stock_value_minor != self.stock_ledger_minor {
            blockers.push(
                "Restaurant stock movements do not match their inventory ledger posting".to_owned(),
            );
        }
        push_count(&mut blockers, self.unbalanced_journals, |n| {
            format!("{n} restaurant or hotel journal(s) are unbalanced")
        });
        blockers
    }

    pub fn is_clear(&self) -> bool {
        self.blockers().is_empty()
    }
}

fn push_count(blockers: &mut Vec<String>, value: i64, message: impl FnOnce(i64) -> String) {
    if value != 0 {
        blockers.push(message(value));
    }
}

/// Finance accounts the retained `validSetup` guard revalidates for the hotel
/// configuration. The port resolves them from the company chart; the PMS
/// domain keeps the retained order and error codes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PmsNightSetupScope {
    pub company_id: String,
    pub chart_id: String,
    pub currency: String,
    /// `(kind, account id)` in [`NIGHT_ACCOUNT_KINDS`] order.
    pub accounts: Vec<(String, String)>,
    pub booking_fee_account: Option<String>,
    pub online_accounts: Vec<String>,
}

impl PmsNightSetupScope {
    pub fn from_property(property: &PmsProperty) -> Self {
        let config = &property.config;
        let value = |key: &str| {
            config
                .get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let account = |kind: &str| {
            config
                .get("accounts")
                .and_then(serde_json::Value::as_object)
                .and_then(|accounts| accounts.get(kind))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        // A configured but incomplete fee/online section resolves an empty
        // account id so the Finance guard still fails closed.
        let named = |key: &str| {
            config
                .get(key)
                .and_then(serde_json::Value::as_object)
                .map(|value| {
                    value
                        .get("account")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_owned()
                })
        };
        Self {
            company_id: property.company_id.clone(),
            chart_id: value("chartId"),
            currency: value("currency"),
            accounts: NIGHT_ACCOUNT_KINDS
                .iter()
                .map(|kind| ((*kind).to_owned(), account(kind)))
                .collect(),
            booking_fee_account: named("bookingFees"),
            online_accounts: config
                .get("onlineAccounting")
                .and_then(serde_json::Value::as_object)
                .map(|online| {
                    ["refundPayable", "unallocated"]
                        .iter()
                        .map(|key| {
                            online
                                .get(*key)
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default()
                                .to_owned()
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

/// Retained `validSetup` answers. `accounts_valid` is aligned with
/// [`PmsNightSetupScope::accounts`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PmsNightSetupFacts {
    pub chart_valid: bool,
    pub accounts_valid: Vec<bool>,
    pub booking_fee_account_valid: bool,
    pub online_accounts_valid: bool,
}

impl Default for PmsNightSetupFacts {
    fn default() -> Self {
        Self {
            chart_valid: true,
            accounts_valid: vec![true; NIGHT_ACCOUNT_KINDS.len()],
            booking_fee_account_valid: true,
            online_accounts_valid: true,
        }
    }
}

/// Retained `validSetup`: the chart and every configured control account must
/// still exist as the retained category in the same company chart and currency.
pub fn require_setup(
    property: &PmsProperty,
    scope: &PmsNightSetupScope,
    facts: &PmsNightSetupFacts,
) -> Result<(), BusinessError> {
    if !facts.chart_valid {
        return Err(BusinessError::invalid(
            "PMS_CHART_INVALID",
            "Select this company's THB chart of accounts",
        ));
    }
    let distinct = property
        .config
        .get("accounts")
        .and_then(serde_json::Value::as_object)
        .map(|accounts| {
            accounts
                .values()
                .filter_map(serde_json::Value::as_str)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        })
        .unwrap_or(0);
    if distinct != NIGHT_ACCOUNT_KINDS.len() {
        return Err(BusinessError::invalid(
            "PMS_ACCOUNTS_DISTINCT",
            "Configure a separate control account for each category",
        ));
    }
    if facts.accounts_valid.len() != scope.accounts.len()
        || facts.accounts_valid.iter().any(|valid| !valid)
    {
        return Err(BusinessError::invalid(
            "PMS_ACCOUNT_INVALID",
            "A hotel control account must keep its retained category in the same chart and currency.",
        ));
    }
    if scope.booking_fee_account.is_some() && !facts.booking_fee_account_valid {
        return Err(BusinessError::invalid(
            "PMS_FEE_ACCOUNT_INVALID",
            "Cancellation fees require a separate active revenue account in the company chart",
        ));
    }
    if !scope.online_accounts.is_empty() && !facts.online_accounts_valid {
        return Err(BusinessError::invalid(
            "PMS_ONLINE_ACCOUNT_INVALID",
            "Online refund and unallocated accounts must be active company liability accounts",
        ));
    }
    Ok(())
}

/// Authoritative owner snapshot loaded under the property row lock.
#[derive(Clone, Debug)]
pub struct PmsNightAuditFacts {
    pub property: PmsPropertyRow,
    /// Target `night` record (`night.resume`, `night.step`).
    pub night: Option<PmsRecord>,
    /// `night` records of the current business date, including the target.
    pub nights_on_date: Vec<PmsRecord>,
    /// Retained stay selection: nonterminal stays plus every stay covering the
    /// business date (so `checked_out` stays still count as sold rooms).
    pub stays: Vec<PmsRecord>,
    /// Shifts the retained blocker reads: open or closed with a variance.
    pub shifts: Vec<PmsRecord>,
    pub room_types: Vec<PmsRecord>,
    pub rooms: Vec<PmsRecord>,
    pub rates: Vec<PmsRecord>,
    /// Every block: maintenance availability and group allotment release.
    pub blocks: Vec<PmsRecord>,
    /// Stayover tasks of the next business date (retained `task()` dedupe).
    pub stayover_tasks: Vec<PmsRecord>,
    /// Entries carrying `night:{stayId}:{businessDate}` this close still owes.
    pub night_entries: Vec<PmsRecord>,
}

impl PmsNightAuditFacts {
    pub fn empty(property: PmsPropertyRow) -> Self {
        Self {
            property,
            night: None,
            nights_on_date: Vec::new(),
            stays: Vec::new(),
            shifts: Vec::new(),
            room_types: Vec::new(),
            rooms: Vec::new(),
            rates: Vec::new(),
            blocks: Vec::new(),
            stayover_tasks: Vec::new(),
            night_entries: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmsNightJobPlan {
    /// Retained `insertPmsJobs`: one pending job per property/business date.
    Enqueue,
    /// Retained `updatePmsJobs('completed')` after the date advanced.
    Complete,
}

#[derive(Clone, Debug)]
pub struct PmsNightAuditPlan {
    pub result: PmsNightAuditResult,
    /// Updated night record (`night.resume`, `night.step`).
    pub night: Option<PmsRecord>,
    /// Created night record (`night.start`).
    pub new_night: Option<PmsRecord>,
    /// New stayover tasks of the next business date.
    pub tasks: Vec<PmsRecord>,
    /// Group allotments the retained close released.
    pub blocks: Vec<PmsRecord>,
    pub property_state: String,
    pub property_business_date: String,
    pub property_version: i64,
    pub job: PmsNightJobPlan,
}

/// Cross-owner guard input. `night.start`/`night.resume` read the retained
/// day-control counts and Finance setup answers; the worker-only `night.step`
/// reads PMS-owned facts only.
pub enum PmsNightAuditGuard<'a> {
    User {
        controls: &'a PmsNightDayControlFacts,
        setup: &'a PmsNightSetupFacts,
        scope: &'a PmsNightSetupScope,
    },
    Worker,
}

fn object(
    data: &serde_json::Value,
) -> Result<&serde_json::Map<String, serde_json::Value>, BusinessError> {
    data.as_object().ok_or_else(record_invalid)
}

fn text(data: &serde_json::Value, key: &'static str) -> Result<String, BusinessError> {
    data.get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(record_invalid)
}

fn optional_text(data: &serde_json::Value, key: &'static str) -> Option<String> {
    data.get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn boolean(data: &serde_json::Value, key: &'static str) -> Result<bool, BusinessError> {
    data.get(key)
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(record_invalid)
}

fn integer(data: &serde_json::Value, key: &'static str) -> Result<i64, BusinessError> {
    data.get(key)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(record_invalid)
}

fn valid_date(value: &str) -> bool {
    value.len() == 10 && parse_date(value).is_some()
}

fn plus_days(date: &str, days: i64) -> Result<String, BusinessError> {
    parse_date(date)
        .map(|day| format_day(day + days))
        .ok_or_else(record_invalid)
}

/// The business date a completed `night.step` advances to.
pub fn next_business_date(date: &str) -> Result<String, BusinessError> {
    plus_days(date, 1)
}

fn format_day(days: i64) -> String {
    // Civil-from-days, the inverse of `room_board::parse_date`.
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
    format!(
        "{:04}-{:02}-{:02}",
        year + i64::from(month <= 2),
        month,
        day
    )
}

#[derive(Clone, Debug)]
struct NightFacts {
    date: String,
    state: String,
}

impl NightFacts {
    fn read(record: &PmsRecord) -> Result<Self, BusinessError> {
        if record.kind != "night" {
            return Err(record_invalid());
        }
        let date = text(&record.data, "date")?;
        let state = text(&record.data, "state")?;
        if !valid_date(&date) || !matches!(state.as_str(), "pending" | "blocked" | "completed") {
            return Err(record_invalid());
        }
        Ok(Self { date, state })
    }
}

#[derive(Clone, Debug)]
struct StayFacts {
    id: String,
    state: String,
    arrival: String,
    departure: String,
    room_id: String,
}

impl StayFacts {
    fn read(record: &PmsRecord) -> Result<Self, BusinessError> {
        if record.kind != "stay" {
            return Err(record_invalid());
        }
        let state = text(&record.data, "state")?;
        let arrival = text(&record.data, "arrival")?;
        let departure = text(&record.data, "departure")?;
        if !matches!(
            state.as_str(),
            "reserved" | "in_house" | "checked_out" | "cancelled" | "no_show"
        ) || !valid_date(&arrival)
            || !valid_date(&departure)
        {
            return Err(record_invalid());
        }
        Ok(Self {
            id: record.id.clone(),
            state,
            arrival,
            departure,
            room_id: optional_text(&record.data, "roomId").unwrap_or_default(),
        })
    }

    fn covers(&self, date: &str) -> bool {
        self.arrival.as_str() <= date && date < self.departure.as_str()
    }
}

#[derive(Clone, Debug)]
struct ShiftFacts {
    name: String,
    state: String,
    variance: i64,
}

impl ShiftFacts {
    fn read(record: &PmsRecord) -> Result<Self, BusinessError> {
        if record.kind != "shift" {
            return Err(record_invalid());
        }
        let name = text(&record.data, "name")?;
        let state = text(&record.data, "state")?;
        if !matches!(state.as_str(), "open" | "closed" | "reviewed") {
            return Err(record_invalid());
        }
        Ok(Self {
            name,
            state,
            variance: integer(&record.data, "variance").unwrap_or(0),
        })
    }
}

#[derive(Clone, Debug)]
struct RoomFacts {
    type_id: String,
    active: bool,
    cleaning_cycle: i64,
}

impl RoomFacts {
    fn read(record: &PmsRecord) -> Result<Self, BusinessError> {
        if record.kind != "room" {
            return Err(record_invalid());
        }
        let type_id = text(&record.data, "typeId")?;
        if type_id.is_empty() {
            return Err(record_invalid());
        }
        Ok(Self {
            type_id,
            active: boolean(&record.data, "active")?,
            cleaning_cycle: record
                .data
                .get("cleaningCycle")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
        })
    }
}

#[derive(Clone, Debug)]
struct BlockFacts {
    kind: String,
    state: String,
    room_id: String,
    from: String,
    release_date: String,
}

impl BlockFacts {
    fn read(record: &PmsRecord) -> Result<Self, BusinessError> {
        if record.kind != "block" {
            return Err(record_invalid());
        }
        let kind = text(&record.data, "kind")?;
        let state = text(&record.data, "state")?;
        let from = text(&record.data, "from")?;
        let release_date = text(&record.data, "releaseDate")?;
        if !matches!(kind.as_str(), "group" | "maintenance")
            || !matches!(state.as_str(), "active" | "released")
            || !valid_date(&from)
            || (kind == "group" && !valid_date(&release_date))
        {
            return Err(record_invalid());
        }
        Ok(Self {
            kind,
            state,
            room_id: optional_text(&record.data, "roomId").unwrap_or_default(),
            from,
            release_date,
        })
    }
}

#[derive(Clone, Debug)]
struct TaskFacts {
    room_cycle: i64,
}

impl TaskFacts {
    fn read(record: &PmsRecord) -> Result<Self, BusinessError> {
        if record.kind != "task" {
            return Err(record_invalid());
        }
        let kind = text(&record.data, "type")?;
        let state = text(&record.data, "state")?;
        if !matches!(
            kind.as_str(),
            "departure" | "stayover" | "arrival" | "maintenance" | "discrepancy"
        ) || !matches!(
            state.as_str(),
            "pending" | "working" | "clean" | "done" | "superseded"
        ) {
            return Err(record_invalid());
        }
        Ok(Self {
            room_cycle: record
                .data
                .get("roomCycle")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
        })
    }
}

fn rate_active(record: &PmsRecord) -> Result<bool, BusinessError> {
    if record.kind != "rate" {
        return Err(record_invalid());
    }
    Ok(record
        .data
        .get("active")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false))
}

/// Retained `readiness(s)`: the pilot hotel must have a room type, an active
/// room, an active rate, an approved setup, a chart with every control account
/// and THB minor units. Extends the board readiness with the per-key account
/// requirement the retained engine uses.
pub fn readiness(facts: &PmsNightAuditFacts) -> Result<Vec<String>, BusinessError> {
    let property = facts.property.view()?;
    let config = &property.config;
    let mut checks = Vec::new();
    if facts.room_types.is_empty() {
        checks.push("Add a room type".to_owned());
    }
    if !facts
        .rooms
        .iter()
        .map(RoomFacts::read)
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|room| room.active)
    {
        checks.push("Add rooms".to_owned());
    }
    if !facts
        .rates
        .iter()
        .map(rate_active)
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|active| *active)
    {
        checks.push("Add a rate plan".to_owned());
    }
    let approved = config
        .get("approvedBy")
        .is_some_and(|value| !value.is_null() && value.as_str() != Some(""));
    if !approved {
        checks.push("Have Finance approve the setup".to_owned());
    }
    let chart = config
        .get("chartId")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| !value.is_empty());
    let accounts = config
        .get("accounts")
        .and_then(serde_json::Value::as_object);
    let accounts_complete = chart
        && NIGHT_ACCOUNT_KINDS.iter().all(|kind| {
            accounts
                .and_then(|accounts| accounts.get(*kind))
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| !value.is_empty())
        });
    if !accounts_complete {
        checks.push("Complete the chart of accounts setup".to_owned());
    }
    if config.get("currency").and_then(serde_json::Value::as_str) != Some("THB")
        || config.get("minorUnit").and_then(serde_json::Value::as_i64) != Some(2)
    {
        checks.push("The pilot supports THB".to_owned());
    }
    Ok(checks)
}

/// Retained `nightBlockers(s)`: readiness, overdue stays and unreviewed shift
/// variances, in that order. The retained message joins them with `·`.
pub fn night_blockers(facts: &PmsNightAuditFacts) -> Result<Vec<String>, BusinessError> {
    let date = facts.property.business_date.as_str();
    if !valid_date(date) {
        return Err(record_invalid());
    }
    let mut blockers = readiness(facts)?;
    for stay in facts.stays.iter().map(StayFacts::read) {
        let stay = stay?;
        let blocked = (stay.state == "reserved" && stay.arrival.as_str() <= date)
            || (stay.state == "in_house" && stay.departure.as_str() <= date);
        if blocked {
            let action = if stay.state == "reserved" {
                "Not checked in / No-show"
            } else {
                "Checkout due"
            };
            let short: String = stay.id.chars().take(8).collect();
            blockers.push(format!("Process {short}: {action}"));
        }
    }
    for shift in facts.shifts.iter().map(ShiftFacts::read) {
        let shift = shift?;
        if shift.state == "open" || (shift.state == "closed" && shift.variance != 0) {
            blockers.push(format!("Close/review shift variance for {}", shift.name));
        }
    }
    Ok(blockers)
}

fn touch(record: &mut PmsRecord, now: i64) {
    record.version += 1;
    record.updated_at = now;
}

/// Deterministic identity for the night record this family creates. The
/// retained engine mints a UUID; deriving it from the retained command key
/// keeps one key on one business date while `PMS_NIGHT_EXISTS` still rejects a
/// second close of the same date.
fn created_night_id(key: &CommandKey, property_id: &str, business_date: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(
        format!(
            "pms-night-create:{}:{property_id}:{business_date}",
            key.as_str()
        )
        .as_bytes(),
    );
    format!("pms-night-{}", hex::encode(&digest[..16]))
}

/// Retained `task()` identity for a generated room task.
fn room_task_id(room_id: &str, kind: &str, date: &str, stay_id: &str, cycle: i64) -> String {
    use sha2::{Digest, Sha256};
    let digest =
        Sha256::digest(format!("pms-task:{room_id}:{kind}:{date}:{stay_id}:{cycle}").as_bytes());
    format!("pms-task-{}", hex::encode(&digest[..16]))
}

/// Plan one retained day-close command. The property row is already locked and
/// `command.expected_version` must match it unless the retained worker calls
/// `night.step`. Every stored fact is validated before any row is written.
pub fn plan_night_audit(
    actor: &PmsNightAuditActor,
    key: &CommandKey,
    facts: &PmsNightAuditFacts,
    guard: PmsNightAuditGuard<'_>,
    command: &PmsNightAuditCommand,
    now: i64,
) -> Result<PmsNightAuditPlan, BusinessError> {
    // Retained ordering: the permission and worker gates run before the
    // command's own bounds and before any owner read.
    actor.require_command(command.operation(), command.worker, now)?;
    command.validate()?;
    let operation = command.operation();
    let worker_guard = matches!(guard, PmsNightAuditGuard::Worker);
    if (operation == PmsNightAuditOperation::Step) != command.worker
        || (operation == PmsNightAuditOperation::Step) != worker_guard
    {
        return Err(invalid());
    }
    if facts.property.id != command.property_id
        || facts.property.company_id != actor.board.company_id
        || actor
            .board
            .branch_id
            .as_ref()
            .is_some_and(|branch| branch != &facts.property.branch_id)
        || !valid_date(&facts.property.business_date)
        || facts.property.version < 1
        || !matches!(facts.property.state.as_str(), "setup" | "open" | "closing")
    {
        return Err(not_found());
    }
    if command.worker {
        if command
            .expected_version
            .is_some_and(|version| version != facts.property.version)
        {
            return Err(version_conflict());
        }
    } else if command.expected_version != Some(facts.property.version) {
        return Err(version_conflict());
    }
    // Retained `night.start` runs `hospitalityDayControls` before the engine's
    // own state checks, so a blocked day always reports the day-control code.
    if operation == PmsNightAuditOperation::Start
        && let PmsNightAuditGuard::User { controls, .. } = guard
        && !controls.is_clear()
    {
        return Err(conflict(
            "PMS_DAY_RECONCILIATION_BLOCKED",
            "Resolve the outstanding restaurant, hotel and Finance day-control items before the night audit.",
        ));
    }
    if facts.property.state == "closing" && operation == PmsNightAuditOperation::Start {
        return Err(conflict(
            "PMS_DAY_CLOSING",
            "Day close is in progress. Please wait for it to finish",
        ));
    }
    if facts.property.state == "setup" {
        return Err(conflict(
            "PMS_SETUP_REQUIRED",
            "Review readiness and open the hotel first",
        ));
    }
    if facts.stays.len() > MAX_STAYS
        || facts.shifts.len() > MAX_SHIFTS
        || facts.room_types.len() > MAX_ROOM_TYPES
        || facts.rooms.len() > MAX_ROOMS
        || facts.rates.len() > MAX_RATES
        || facts.blocks.len() > MAX_BLOCKS
        || facts.nights_on_date.len() > MAX_NIGHTS
        || facts.stayover_tasks.len() > MAX_TASKS
        || facts.night_entries.len() > MAX_NIGHT_ENTRIES
    {
        return Err(read_limit(
            "The hotel has more day-close facts than this command can read.",
        ));
    }
    if facts
        .night
        .as_ref()
        .is_some_and(|night| command.night_id.as_deref().is_some_and(|id| night.id != id))
    {
        return Err(record_invalid());
    }
    match operation {
        PmsNightAuditOperation::Start => plan_start(actor, key, facts, guard, command, now),
        PmsNightAuditOperation::Resume => plan_resume(actor, facts, guard, command, now),
        PmsNightAuditOperation::Step => plan_step(facts, command, now),
    }
}

fn empty_plan(facts: &PmsNightAuditFacts) -> PmsNightAuditPlan {
    PmsNightAuditPlan {
        result: PmsNightAuditResult {
            id: String::new(),
            property_id: facts.property.id.clone(),
            version: facts.property.version + 1,
        },
        night: None,
        new_night: None,
        tasks: Vec::new(),
        blocks: Vec::new(),
        property_state: facts.property.state.clone(),
        property_business_date: facts.property.business_date.clone(),
        property_version: facts.property.version + 1,
        job: PmsNightJobPlan::Enqueue,
    }
}

fn plan_start(
    actor: &PmsNightAuditActor,
    key: &CommandKey,
    facts: &PmsNightAuditFacts,
    guard: PmsNightAuditGuard<'_>,
    command: &PmsNightAuditCommand,
    now: i64,
) -> Result<PmsNightAuditPlan, BusinessError> {
    let _ = command;
    let PmsNightAuditGuard::User { setup, scope, .. } = guard else {
        return Err(invalid());
    };
    if !night_blockers(facts)?.is_empty() {
        return Err(conflict(
            "PMS_NIGHT_BLOCKED",
            "Resolve the hotel readiness, overdue stay and shift-variance items before the night audit.",
        ));
    }
    if facts.nights_on_date.iter().any(|night| {
        night.data.get("date").and_then(serde_json::Value::as_str)
            == Some(facts.property.business_date.as_str())
    }) {
        return Err(conflict(
            "PMS_NIGHT_EXISTS",
            "This business date already has a day-close job",
        ));
    }
    let property = facts.property.view()?;
    require_setup(&property, scope, setup)?;
    let mut plan = empty_plan(facts);
    let id = created_night_id(key, &facts.property.id, &facts.property.business_date);
    let night = PmsRecord {
        id: id.clone(),
        kind: "night".to_owned(),
        version: 1,
        updated_at: now,
        data: serde_json::json!({
            "date": facts.property.business_date,
            "state": "pending",
            "initiatedBy": actor.board.user_id,
            "message": "",
        }),
    };
    plan.result.id = id;
    plan.new_night = Some(night);
    plan.property_state = "closing".to_owned();
    plan.job = PmsNightJobPlan::Enqueue;
    Ok(plan)
}

fn plan_resume(
    actor: &PmsNightAuditActor,
    facts: &PmsNightAuditFacts,
    guard: PmsNightAuditGuard<'_>,
    command: &PmsNightAuditCommand,
    now: i64,
) -> Result<PmsNightAuditPlan, BusinessError> {
    let PmsNightAuditGuard::User { setup, scope, .. } = guard else {
        return Err(invalid());
    };
    let mut night = facts
        .night
        .clone()
        .filter(|night| command.night_id.as_deref().is_some_and(|id| night.id == id))
        .ok_or_else(record_not_found)?;
    let existing = NightFacts::read(&night)?;
    if existing.date != facts.property.business_date
        || facts.property.state != "closing"
        || existing.state == "completed"
    {
        return Err(conflict("PMS_NIGHT_CLOSED", "Day-close job is complete"));
    }
    let property = facts.property.view()?;
    require_setup(&property, scope, setup)?;
    object(&night.data)?;
    night.data["state"] = serde_json::Value::String("pending".to_owned());
    night.data["message"] = serde_json::Value::String(String::new());
    night.data["initiatedBy"] = serde_json::Value::String(actor.board.user_id.clone());
    let mut plan = empty_plan(facts);
    plan.result.id = night.id.clone();
    touch(&mut night, now);
    plan.night = Some(night);
    plan.job = PmsNightJobPlan::Enqueue;
    Ok(plan)
}

fn plan_step(
    facts: &PmsNightAuditFacts,
    command: &PmsNightAuditCommand,
    now: i64,
) -> Result<PmsNightAuditPlan, BusinessError> {
    let mut night = facts
        .night
        .clone()
        .filter(|night| command.night_id.as_deref().is_some_and(|id| night.id == id))
        .ok_or_else(record_not_found)?;
    let existing = NightFacts::read(&night)?;
    if facts.property.state != "closing"
        || existing.date != facts.property.business_date
        || existing.state == "completed"
    {
        return Err(conflict("PMS_NIGHT_CLOSED", "Business date has changed"));
    }
    let date = facts.property.business_date.clone();
    let stays = facts
        .stays
        .iter()
        .map(StayFacts::read)
        .collect::<Result<Vec<_>, _>>()?;
    let in_house = stays
        .iter()
        .filter(|stay| stay.state == "in_house" && stay.covers(&date))
        .collect::<Vec<_>>();
    let posted: std::collections::BTreeSet<&str> = facts
        .night_entries
        .iter()
        .map(|entry| {
            if entry.kind != "entry" {
                return Err(record_invalid());
            }
            entry
                .data
                .get("sourceKey")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(record_invalid)
        })
        .collect::<Result<_, _>>()?;
    let outstanding = in_house
        .iter()
        .filter(|stay| {
            let key = format!("night:{}:{date}", stay.id);
            !posted.contains(key.as_str())
        })
        .count();
    if outstanding != 0 {
        return Err(posting_port_required());
    }
    let mut plan = empty_plan(facts);
    let capacity = room_capacity(facts, &date)?;
    let sold = stays
        .iter()
        .filter(|stay| {
            matches!(stay.state.as_str(), "in_house" | "checked_out") && stay.covers(&date)
        })
        .count() as i64;
    let next = plus_days(&date, 1)?;
    for stay in &in_house {
        if let Some(task) = stayover_task(facts, stay, &next, now)? {
            plan.tasks.push(task);
        }
    }
    for block in &facts.blocks {
        let block_facts = BlockFacts::read(block)?;
        if block_facts.kind == "group" && block_facts.release_date.as_str() <= next.as_str() {
            let mut released = block.clone();
            object(&released.data)?;
            released.data["state"] = serde_json::Value::String("released".to_owned());
            touch(&mut released, now);
            plan.blocks.push(released);
        }
    }
    object(&night.data)?;
    night.data["roomMetrics"] = serde_json::json!({ "capacity": capacity, "sold": sold });
    night.data["state"] = serde_json::Value::String("completed".to_owned());
    night.data["completedAt"] = serde_json::Value::from(now);
    plan.result.id = night.id.clone();
    touch(&mut night, now);
    plan.night = Some(night);
    plan.property_state = "open".to_owned();
    plan.property_business_date = next;
    plan.job = PmsNightJobPlan::Complete;
    Ok(plan)
}

/// Retained `dailyRoomReport` capacity: every active room of a type that is not
/// under an active maintenance block on the date.
fn room_capacity(facts: &PmsNightAuditFacts, date: &str) -> Result<i64, BusinessError> {
    let mut unavailable = std::collections::BTreeSet::new();
    for block in &facts.blocks {
        let block = BlockFacts::read(block)?;
        if block.kind == "maintenance" && block.state == "active" && block.from.as_str() <= date {
            unavailable.insert(block.room_id);
        }
    }
    let mut types = std::collections::BTreeSet::new();
    for room_type in &facts.room_types {
        if room_type.kind != "room_type" {
            return Err(record_invalid());
        }
        types.insert(room_type.id.as_str());
    }
    let mut capacity = 0_i64;
    for record in &facts.rooms {
        let room = RoomFacts::read(record)?;
        if room.active && types.contains(room.type_id.as_str()) && !unavailable.contains(&record.id)
        {
            capacity += 1;
        }
    }
    Ok(capacity)
}

/// Retained `task(roomId, "stayover", stayId, businessDate + 1)` dedupe and
/// creation.
fn stayover_task(
    facts: &PmsNightAuditFacts,
    stay: &StayFacts,
    date: &str,
    now: i64,
) -> Result<Option<PmsRecord>, BusinessError> {
    let room = facts
        .rooms
        .iter()
        .find(|room| room.id == stay.room_id)
        .cloned()
        .ok_or_else(record_not_found)?;
    let room_facts = RoomFacts::read(&room)?;
    for task in &facts.stayover_tasks {
        let existing = TaskFacts::read(task)?;
        let same_room = task.data.get("roomId").and_then(serde_json::Value::as_str)
            == Some(stay.room_id.as_str());
        let same_stay = optional_text(&task.data, "stayId").as_deref() == Some(stay.id.as_str());
        let same_date = optional_text(&task.data, "date").as_deref() == Some(date);
        let kind = optional_text(&task.data, "type").as_deref() == Some("stayover");
        if same_room
            && same_stay
            && same_date
            && kind
            && existing.room_cycle == room_facts.cleaning_cycle
        {
            return Ok(None);
        }
    }
    Ok(Some(PmsRecord {
        id: room_task_id(
            &stay.room_id,
            "stayover",
            date,
            &stay.id,
            room_facts.cleaning_cycle,
        ),
        kind: "task".to_owned(),
        version: 1,
        updated_at: now,
        data: serde_json::json!({
            "roomCycle": room_facts.cleaning_cycle,
            "roomId": stay.room_id,
            "type": "stayover",
            "stayId": stay.id,
            "date": date,
            "assigneeId": "",
            "priority": "normal",
            "state": "pending",
            "checklist": [
                "Bathroom",
                "Bed and linen",
                "Floor and dust",
                "Minibar",
                "Equipment and safety"
            ],
            "checked": [],
            "notes": "",
            "completedBy": "",
            "inspectedBy": "",
        }),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plus_days_matches_calendar_dates() {
        assert_eq!(plus_days("2026-11-30", 1).unwrap(), "2026-12-01");
        assert_eq!(plus_days("2026-12-31", 1).unwrap(), "2027-01-01");
        assert_eq!(plus_days("2028-02-28", 1).unwrap(), "2028-02-29");
    }

    #[test]
    fn day_control_blockers_follow_the_retained_order() {
        let controls = PmsNightDayControlFacts {
            open_shifts: 1,
            unbalanced_journals: 2,
            stock_value_minor: 1,
            stock_ledger_minor: 3,
            ..PmsNightDayControlFacts::default()
        };
        let blockers = controls.blockers();
        assert_eq!(
            blockers,
            vec![
                "1 restaurant cash session(s) still open".to_owned(),
                "Restaurant stock movements do not match their inventory ledger posting".to_owned(),
                "2 restaurant or hotel journal(s) are unbalanced".to_owned(),
            ]
        );
    }
}
