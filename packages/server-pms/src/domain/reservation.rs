//! PMS staff reservation lifecycle facts.
//!
//! These rules are the retained `reservation.create`, `reservation.change` and
//! `reservation.cancel` staff transitions reduced to owner facts: the property
//! row, the target guest/reservation/block, the bounded room types, rooms,
//! rates, active blocks and nonterminal stays used by the retained capacity
//! reconciliation, and the guest accounts of the changed reservation. They
//! never read a client source label, an Account session, a Finance balance or a
//! People/Commercial HTTP projection. A command result is a property receipt
//! version, never a permission or a current authorization proof.
//!
//! Public online booking, payment/prepayment, Stripe/email delivery and Finance
//! posting are deliberately absent. A reservation line whose rate belongs to a
//! Finance party (company/agent market) is refused until composition supplies
//! the Finance party scope port.
use super::room_board::{
    PmsPropertyRow, PmsRecord, cp_len, ecma_trim, not_found, parse_date, record_not_found,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Bounded reads. The retained engine reads the complete hotel snapshot; these
/// bounds match the landed board/command readers so a reservation fails closed
/// instead of loading an unbounded hotel.
pub const MAX_ROOMS: usize = 500;
pub const MAX_ROOM_TYPES: usize = 500;
pub const MAX_RATES: usize = 500;
pub const MAX_BLOCKS: usize = 1_000;
pub const MAX_STAYS: usize = 4_000;
pub const MAX_ACCOUNTS: usize = 500;
/// Retained `rooms: z.array(roomSelectionSchema).min(1).max(50)`.
pub const MAX_LINES: usize = 50;
/// Retained `childAges: z.array(...).max(20)`.
pub const MAX_CHILD_AGES: usize = 20;
/// Retained `dates()` refuses a single range longer than 730 nights.
pub const MAX_NIGHTS: usize = 730;
/// Retained `textField` (`.trim().max(2000)`) and `idField` limits.
pub const MAX_TEXT: usize = 2_000;
pub const MAX_ID: usize = 128;
pub const MAX_CAMPAIGN: usize = 60;
pub const MAX_GUEST_NAME: usize = 200;
pub const MAX_GUEST_EMAIL: usize = 254;
pub const MAX_GUEST_PHONE: usize = 80;
pub const MAX_REASON: usize = 2_000;
/// Capacity reconciliation is O(room types × affected nights) and each night
/// scans the bounded stay/block set.
pub const MAX_CAPACITY_EVALUATIONS: usize = 200_000;

/// The retained staff reservation command family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmsReservationOperation {
    Create,
    Change,
    Cancel,
}

impl PmsReservationOperation {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "reservation.create" => Some(Self::Create),
            "reservation.change" => Some(Self::Change),
            "reservation.cancel" => Some(Self::Cancel),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "reservation.create",
            Self::Change => "reservation.change",
            Self::Cancel => "reservation.cancel",
        }
    }

    /// Retained receipt, audit and outbox operation (`pms.${op}`).
    pub fn receipt_operation(self) -> &'static str {
        match self {
            Self::Create => "pms.reservation.create",
            Self::Change => "pms.reservation.change",
            Self::Cancel => "pms.reservation.cancel",
        }
    }

    /// Retained `operationPermission` default for `reservation.*`.
    pub fn permission(self) -> &'static str {
        "pms.reservations"
    }

    fn history_action(self) -> &'static str {
        match self {
            Self::Create => "Reservation confirmed",
            Self::Change => "Reservation changed",
            Self::Cancel => "Reservation cancelled",
        }
    }
}

/// Retained `roomSelectionSchema`. `quantity` defaults to one.
#[derive(Clone, Debug)]
pub struct PmsReservationLine {
    pub type_id: String,
    pub rate_id: String,
    pub arrival: String,
    pub departure: String,
    pub adults: i64,
    pub child_ages: Vec<i64>,
    pub quantity: i64,
}

/// Retained `guestInputSchema` used only when the command has no `guestId`.
#[derive(Clone, Debug)]
pub struct PmsReservationGuestInput {
    pub name: String,
    pub email: String,
    pub phone: String,
}

#[derive(Clone, Debug)]
pub struct PmsReservationCreateInput {
    pub guest_id: Option<String>,
    pub guest: Option<PmsReservationGuestInput>,
    pub source: String,
    pub campaign_code: String,
    pub external_ref: String,
    pub group_name: String,
    pub notes: String,
    pub block_id: String,
    pub rooms: Vec<PmsReservationLine>,
}

/// Bounded staff change. The retained online `stay.replace` keeps the accepted
/// nightly price snapshot; this staff path recomputes the owner price from the
/// selected rate, so it refuses a reservation with financial activity instead
/// of inventing a payment difference.
#[derive(Clone, Debug)]
pub struct PmsReservationChangeInput {
    pub reservation_id: String,
    pub expected_reservation_version: Option<i64>,
    pub reason: String,
    pub notes: Option<String>,
    pub group_name: Option<String>,
    pub campaign_code: Option<String>,
    pub rooms: Vec<PmsReservationLine>,
}

/// Retained `reservation.cancel` input. `fee` is retained only to refuse the
/// Finance-funded fee path explicitly; this owner command never posts a fee.
#[derive(Clone, Debug)]
pub struct PmsReservationCancelInput {
    pub reservation_id: String,
    pub expected_reservation_version: Option<i64>,
    pub reason: String,
    pub fee: i64,
}

#[derive(Clone, Debug)]
pub enum PmsReservationCommandInput {
    Create(PmsReservationCreateInput),
    Change(PmsReservationChangeInput),
    Cancel(PmsReservationCancelInput),
}

impl PmsReservationCommandInput {
    fn operation(&self) -> PmsReservationOperation {
        match self {
            Self::Create(_) => PmsReservationOperation::Create,
            Self::Change(_) => PmsReservationOperation::Change,
            Self::Cancel(_) => PmsReservationOperation::Cancel,
        }
    }

    /// Reservation id this command names. Absent only for creation.
    pub fn reservation_id(&self) -> Option<&str> {
        match self {
            Self::Create(_) => None,
            Self::Change(input) => Some(&input.reservation_id),
            Self::Cancel(input) => Some(&input.reservation_id),
        }
    }

    /// Whether the retained result id is the loaded target reservation.
    pub fn result_id_is_target(&self) -> bool {
        !matches!(self, Self::Create(_))
    }
}

#[derive(Clone, Debug)]
pub struct PmsReservationCommand {
    pub operation: PmsReservationOperation,
    pub property_id: String,
    /// Current `pms_properties.version`. Required: this command family never
    /// accepts a stale property snapshot.
    pub expected_version: i64,
    pub input: PmsReservationCommandInput,
}

impl PmsReservationCommand {
    pub fn operation(&self) -> PmsReservationOperation {
        self.operation
    }

    /// Retained Zod normalization boundaries. `textField` values are stored
    /// trimmed; ids and dates are never trimmed.
    pub fn normalize(&mut self) {
        match &mut self.input {
            PmsReservationCommandInput::Create(input) => {
                input.source = ecma_trim(&input.source).to_owned();
                input.campaign_code = ecma_trim(&input.campaign_code).to_owned();
                input.external_ref = ecma_trim(&input.external_ref).to_owned();
                input.group_name = ecma_trim(&input.group_name).to_owned();
                input.notes = ecma_trim(&input.notes).to_owned();
                if let Some(guest) = &mut input.guest {
                    guest.name = ecma_trim(&guest.name).to_owned();
                    guest.email = ecma_trim(&guest.email).to_owned();
                    guest.phone = ecma_trim(&guest.phone).to_owned();
                }
            }
            PmsReservationCommandInput::Change(input) => {
                input.reason = ecma_trim(&input.reason).to_owned();
                input.notes = input
                    .notes
                    .as_deref()
                    .map(|value| ecma_trim(value).to_owned());
                input.group_name = input
                    .group_name
                    .as_deref()
                    .map(|value| ecma_trim(value).to_owned());
                input.campaign_code = input
                    .campaign_code
                    .as_deref()
                    .map(|value| ecma_trim(value).to_owned());
            }
            PmsReservationCommandInput::Cancel(input) => {
                input.reason = ecma_trim(&input.reason).to_owned();
            }
        }
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.input.operation() != self.operation
            || !valid_id(&self.property_id)
            || self.expected_version < 1
        {
            return Err(invalid());
        }
        match &self.input {
            PmsReservationCommandInput::Create(input) => {
                if !(1..=MAX_TEXT).contains(&cp_len(&input.source))
                    || cp_len(&input.campaign_code) > MAX_CAMPAIGN
                    || cp_len(&input.external_ref) > MAX_TEXT
                    || cp_len(&input.group_name) > MAX_TEXT
                    || cp_len(&input.notes) > MAX_TEXT
                    || (!input.block_id.is_empty() && !valid_id(&input.block_id))
                    || input.guest_id.as_deref().is_some_and(|id| !valid_id(id))
                    || !(1..=MAX_LINES).contains(&input.rooms.len())
                {
                    return Err(invalid());
                }
                if let Some(guest) = &input.guest
                    && (!(1..=MAX_GUEST_NAME).contains(&cp_len(&guest.name))
                        || cp_len(&guest.email) > MAX_GUEST_EMAIL
                        || !valid_email(&guest.email)
                        || !(1..=MAX_GUEST_PHONE).contains(&cp_len(&guest.phone)))
                {
                    return Err(invalid());
                }
                for line in &input.rooms {
                    validate_line(line)?;
                }
            }
            PmsReservationCommandInput::Change(input) => {
                if !valid_id(&input.reservation_id)
                    || !(1..=MAX_REASON).contains(&cp_len(&input.reason))
                    || cp_len(&input.reason) < 3
                    || input.notes.as_deref().is_some_and(|v| cp_len(v) > MAX_TEXT)
                    || input
                        .group_name
                        .as_deref()
                        .is_some_and(|v| cp_len(v) > MAX_TEXT)
                    || input
                        .campaign_code
                        .as_deref()
                        .is_some_and(|v| cp_len(v) > MAX_CAMPAIGN)
                    || input
                        .expected_reservation_version
                        .is_some_and(|version| version < 1)
                    || !(1..=MAX_LINES).contains(&input.rooms.len())
                {
                    return Err(invalid());
                }
                for line in &input.rooms {
                    validate_line(line)?;
                }
            }
            PmsReservationCommandInput::Cancel(input) => {
                if !valid_id(&input.reservation_id)
                    || !(1..=MAX_REASON).contains(&cp_len(&input.reason))
                    || cp_len(&input.reason) < 3
                    || input
                        .expected_reservation_version
                        .is_some_and(|version| version < 1)
                    || !(0..=99_999_999_999).contains(&input.fee)
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}

fn validate_line(line: &PmsReservationLine) -> Result<(), BusinessError> {
    if !valid_id(&line.type_id)
        || !valid_id(&line.rate_id)
        || !valid_date(&line.arrival)
        || !valid_date(&line.departure)
        || !(1..=50).contains(&line.adults)
        || line.child_ages.len() > MAX_CHILD_AGES
        || line.child_ages.iter().any(|age| !(0..=17).contains(age))
        || !(1..=200).contains(&line.quantity)
    {
        return Err(invalid());
    }
    Ok(())
}

/// Retained `dateField`: one strict `YYYY-MM-DD` calendar date.
fn valid_date(value: &str) -> bool {
    value.len() == 10 && parse_date(value).is_some()
}

/// Retained `idField`: one to 128 Unicode scalars, never trimmed.
fn valid_id(value: &str) -> bool {
    !value.is_empty() && cp_len(value) <= MAX_ID && !value.chars().any(char::is_control)
}

/// Retained `z.email()` boundary is wider than this ASCII check; the retained
/// mandatory shape (one local part, one domain label with a dot, no whitespace
/// or control characters) is enforced and the stored value is the trimmed one.
fn valid_email(value: &str) -> bool {
    let mut parts = value.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PMS_RESERVATION_REQUEST_INVALID",
        "Use one bounded hotel reservation action with the current hotel version.",
    )
}

pub fn record_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_RESERVATION_RECORD_INVALID",
        "The hotel reservation could not be read safely.",
    )
}

pub fn read_limit(message: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_RESERVATION_READ_LIMIT",
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

/// Retained `PMS_PARTY_INVALID` guard is a Finance owner fact. Until
/// composition supplies the Finance party scope port, a party-priced rate is
/// refused instead of inventing a local check.
fn finance_party_port_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_RESERVATION_FINANCE_PARTY_PORT_REQUIRED",
        "Company and agent rate parties are not available in this workspace.",
    )
}

/// Owner command context. The board actor supplies Company/branch scope,
/// permissions, module and work-context expiry; receipt/audit facts add the
/// selected assignment identity without granting anything.
#[derive(Clone, Debug)]
pub struct PmsReservationActor {
    pub board: super::room_board::PmsBoardActor,
    pub assignment_id: String,
    pub role: String,
    pub revision: i64,
    pub assignment_version: i64,
}

impl PmsReservationActor {
    /// Mirror of the retained workspace gate (read permission, module, branch
    /// area, unexpired work context) followed by the retained
    /// `operationPermission` for `reservation.*`.
    pub fn require_command(
        &self,
        operation: PmsReservationOperation,
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
pub struct PmsReservationResult {
    pub id: String,
    pub property_id: String,
    pub version: i64,
}

/// Authoritative owner snapshot loaded under the property row lock. Fields are
/// loaded only for the operations that need them; every list is bounded.
#[derive(Clone, Debug)]
pub struct PmsReservationFacts {
    pub property: PmsPropertyRow,
    /// Target guest for creation when the command names one.
    pub guest: Option<PmsRecord>,
    /// Target reservation for change/cancel.
    pub reservation: Option<PmsRecord>,
    /// Target group allotment when the command names one.
    pub block: Option<PmsRecord>,
    pub rooms: Vec<PmsRecord>,
    pub room_types: Vec<PmsRecord>,
    pub rates: Vec<PmsRecord>,
    /// Active maintenance/group blocks: availability and quota facts.
    pub blocks: Vec<PmsRecord>,
    /// Nonterminal stays (`reserved`, `in_house`), including siblings of the
    /// target reservation.
    pub stays: Vec<PmsRecord>,
    /// Guest accounts of the target reservation (change/cancel guards).
    pub accounts: Vec<PmsRecord>,
    /// Retained `cancellationEligibility` funding guard: any guest-account
    /// entry or refund for this reservation's accounts. Payment posting stays
    /// with Finance; this reads only PMS-owned rows.
    pub financial_activity: bool,
    /// Retained duplicate channel reference guard (`source` + `externalRef`),
    /// including archived reservations.
    pub external_ref_exists: bool,
}

impl PmsReservationFacts {
    pub fn empty(property: PmsPropertyRow) -> Self {
        Self {
            property,
            guest: None,
            reservation: None,
            block: None,
            rooms: Vec::new(),
            room_types: Vec::new(),
            rates: Vec::new(),
            blocks: Vec::new(),
            stays: Vec::new(),
            accounts: Vec::new(),
            financial_activity: false,
            external_ref_exists: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PmsReservationPlan {
    pub result: PmsReservationResult,
    /// The reservation row to insert (`version == 1`) or update.
    pub reservation: PmsRecord,
    /// New guest row created from the retained `guestInputSchema`.
    pub guest: Option<PmsRecord>,
    /// New or updated stay rows belonging to the reservation.
    pub stays: Vec<PmsRecord>,
    /// New guest-account rows created for `reservation.create` stays.
    pub accounts: Vec<PmsRecord>,
    /// Updated group allotment (`pickedStayIds`).
    pub block: Option<PmsRecord>,
    /// Retained `effects.sources` key for the channel reference.
    pub source_key: Option<String>,
    /// Property version after this command. Audit/outbox rows use this version.
    pub property_version: i64,
}

fn object(data: &Value) -> Result<&serde_json::Map<String, Value>, BusinessError> {
    data.as_object().ok_or_else(record_invalid)
}

fn text(data: &Value, key: &'static str) -> Result<String, BusinessError> {
    data.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(record_invalid)
}

fn optional_text(data: &Value, key: &'static str) -> Option<String> {
    data.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn boolean(data: &Value, key: &'static str) -> Result<bool, BusinessError> {
    data.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(record_invalid)
}

fn integer(data: &Value, key: &'static str) -> Result<i64, BusinessError> {
    data.get(key)
        .and_then(Value::as_i64)
        .ok_or_else(record_invalid)
}

fn money(data: &Value, key: &'static str) -> Result<i64, BusinessError> {
    let value = integer(data, key)?;
    (0..=99_999_999_999)
        .contains(&value)
        .then_some(value)
        .ok_or_else(record_invalid)
}

fn safe(value: i128) -> Result<i64, BusinessError> {
    if (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&value) {
        i64::try_from(value).map_err(|_| record_invalid())
    } else {
        Err(BusinessError::new(
            FailureKind::Conflict,
            "PMS_MONEY_OVERFLOW",
            "Booking total exceeds supported amount",
        ))
    }
}

#[derive(Clone, Copy, Debug)]
struct TaxConfig {
    tax_bps: i64,
    service_bps: i64,
    tax_inclusive: bool,
}

fn tax_config(property: &PmsPropertyRow) -> Result<TaxConfig, BusinessError> {
    let config: Value =
        serde_json::from_str(&property.config_json).map_err(|_| record_invalid())?;
    object(&config)?;
    let bps = |key: &'static str| match config.get(key) {
        None => Ok(0),
        Some(Value::Number(value)) => value
            .as_i64()
            .filter(|value| (0..=10_000).contains(value))
            .ok_or_else(record_invalid),
        Some(_) => Err(record_invalid()),
    };
    let tax_inclusive = match config.get("taxInclusive") {
        None => false,
        Some(Value::Bool(value)) => *value,
        Some(_) => return Err(record_invalid()),
    };
    Ok(TaxConfig {
        tax_bps: bps("taxBps")?,
        service_bps: bps("serviceBps")?,
        tax_inclusive,
    })
}

/// Retained `amounts(price, config)` integer rounding.
fn amounts(price: i64, config: TaxConfig) -> Result<(i64, i64, i64, i64), BusinessError> {
    let round = |n: i128, d: i128| -> i128 { (n + d / 2) / d };
    let price = i128::from(price);
    let net = if config.tax_inclusive {
        round(
            price * 100_000_000,
            i128::from(10_000 + config.service_bps) * i128::from(10_000 + config.tax_bps),
        )
    } else {
        price
    };
    let service = round(net * i128::from(config.service_bps), 10_000);
    let tax = if config.tax_inclusive {
        price - net - service
    } else {
        round((net + service) * i128::from(config.tax_bps), 10_000)
    };
    let total = net + service + tax;
    Ok((safe(net)?, safe(service)?, safe(tax)?, safe(total)?))
}

#[derive(Clone, Debug)]
struct NightPrice {
    date: String,
    net: i64,
    service: i64,
    tax: i64,
    total: i64,
}

impl NightPrice {
    fn value(&self) -> Value {
        serde_json::json!({
            "date": self.date,
            "net": self.net,
            "service": self.service,
            "tax": self.tax,
            "total": self.total,
        })
    }
}

#[derive(Clone, Debug)]
struct ChildBand {
    min_age: i64,
    max_age: i64,
    price: i64,
}

#[derive(Clone, Debug)]
struct CalendarDay {
    date: String,
    price: Option<i64>,
    stop_sell: bool,
    closed_arrival: bool,
    closed_departure: bool,
}

#[derive(Clone, Debug)]
struct RateFacts {
    type_id: String,
    active: bool,
    stop_sell: bool,
    closed_arrival: bool,
    closed_departure: bool,
    from: String,
    to: String,
    price: i64,
    extra_adult: i64,
    included_guests: i64,
    min_nights: i64,
    max_nights: i64,
    party_id: String,
    child_bands: Vec<ChildBand>,
    calendar: Vec<CalendarDay>,
}

fn rate_facts(record: &PmsRecord) -> Result<RateFacts, BusinessError> {
    if record.kind != "rate" {
        return Err(record_invalid());
    }
    object(&record.data)?;
    let type_id = text(&record.data, "typeId")?;
    let from = text(&record.data, "from")?;
    let to = text(&record.data, "to")?;
    if type_id.is_empty()
        || from.len() != 10
        || to.len() != 10
        || parse_date(&from).is_none()
        || parse_date(&to).is_none()
    {
        return Err(record_invalid());
    }
    let defaulted = |key: &'static str, default: i64, min: i64, max: i64| match record.data.get(key)
    {
        None => Ok(default),
        Some(Value::Number(value)) => value
            .as_i64()
            .filter(|value| (*value >= min) && (*value <= max))
            .ok_or_else(record_invalid),
        Some(_) => Err(record_invalid()),
    };
    let child_bands = match record.data.get("childBands") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(values)) => {
            if values.len() > 18 {
                return Err(record_invalid());
            }
            values
                .iter()
                .map(|value| {
                    let min_age = integer(value, "minAge")?;
                    let max_age = integer(value, "maxAge")?;
                    if !(0..=17).contains(&min_age) || !(0..=17).contains(&max_age) {
                        return Err(record_invalid());
                    }
                    Ok(ChildBand {
                        min_age,
                        max_age,
                        price: money(value, "price")?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?
        }
        Some(_) => return Err(record_invalid()),
    };
    let calendar = match record.data.get("calendar") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                let date = text(value, "date")?;
                if date.len() != 10 || parse_date(&date).is_none() {
                    return Err(record_invalid());
                }
                let price = match value.get("price") {
                    None => None,
                    Some(Value::Number(value)) => Some(
                        value
                            .as_i64()
                            .filter(|price| (0..=99_999_999_999).contains(price))
                            .ok_or_else(record_invalid)?,
                    ),
                    Some(_) => return Err(record_invalid()),
                };
                Ok(CalendarDay {
                    date,
                    price,
                    stop_sell: boolean(value, "stopSell")?,
                    closed_arrival: boolean(value, "closedArrival")?,
                    closed_departure: boolean(value, "closedDeparture")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(record_invalid()),
    };
    Ok(RateFacts {
        type_id,
        active: boolean(&record.data, "active")?,
        stop_sell: boolean(&record.data, "stopSell")?,
        closed_arrival: boolean(&record.data, "closedArrival")?,
        closed_departure: boolean(&record.data, "closedDeparture")?,
        from,
        to,
        price: money(&record.data, "price")?,
        extra_adult: defaulted("extraAdult", 0, 0, 99_999_999_999)?,
        included_guests: defaulted("includedGuests", 2, 1, 50)?,
        min_nights: defaulted("minNights", 1, 1, 730)?,
        max_nights: defaulted("maxNights", 365, 1, 730)?,
        party_id: optional_text(&record.data, "partyId").unwrap_or_default(),
        child_bands,
        calendar,
    })
}

fn room_type_capacity(record: &PmsRecord) -> Result<i64, BusinessError> {
    if record.kind != "room_type" {
        return Err(record_invalid());
    }
    object(&record.data)?;
    integer(&record.data, "capacity")
        .ok()
        .filter(|capacity| (1..=50).contains(capacity))
        .ok_or_else(record_invalid)
}

#[derive(Clone, Debug)]
struct CapacityStay {
    id: String,
    type_id: String,
    arrival: String,
    departure: String,
    periods: Vec<(String, String, String)>,
}

impl CapacityStay {
    /// Retained `stayTypeOnDate`: the first covering room-type period wins.
    fn type_on(&self, date: &str) -> &str {
        self.periods
            .iter()
            .find(|(_, from, to)| from.as_str() <= date && date < to.as_str())
            .map(|(type_id, _, _)| type_id.as_str())
            .unwrap_or(self.type_id.as_str())
    }
    fn covers(&self, date: &str) -> bool {
        self.arrival.as_str() <= date && date < self.departure.as_str()
    }
}

fn capacity_stay(record: &PmsRecord) -> Result<CapacityStay, BusinessError> {
    let state = text(&record.data, "state")?;
    if !matches!(state.as_str(), "reserved" | "in_house") {
        return Err(record_invalid());
    }
    let type_id = text(&record.data, "typeId")?;
    let arrival = text(&record.data, "arrival")?;
    let departure = text(&record.data, "departure")?;
    if type_id.is_empty()
        || arrival.len() != 10
        || departure.len() != 10
        || parse_date(&arrival).is_none()
        || parse_date(&departure).is_none()
    {
        return Err(record_invalid());
    }
    let periods = match record.data.get("roomTypePeriods") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                let type_id = text(value, "typeId")?;
                let from = text(value, "from")?;
                let to = text(value, "to")?;
                if type_id.is_empty()
                    || from.len() != 10
                    || to.len() != 10
                    || parse_date(&from).is_none()
                    || parse_date(&to).is_none()
                {
                    return Err(record_invalid());
                }
                Ok((type_id, from, to))
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(record_invalid()),
    };
    Ok(CapacityStay {
        id: record.id.clone(),
        type_id,
        arrival,
        departure,
        periods,
    })
}

#[derive(Clone, Debug)]
struct CapacityBlock {
    id: String,
    kind: String,
    state: String,
    room_id: String,
    type_id: String,
    from: String,
    to: String,
    quantity: i64,
    release_date: String,
    quota_only: bool,
    expires_at: Option<i64>,
    picked: Vec<String>,
    booking_change_id: String,
    claims: Vec<(String, String)>,
}

impl CapacityBlock {
    fn covers(&self, date: &str) -> bool {
        self.from.as_str() <= date && date < self.to.as_str()
    }
    fn live_at(&self, as_of: i64) -> bool {
        self.expires_at.is_none_or(|value| value > as_of)
    }
}

fn capacity_block(record: &PmsRecord) -> Result<CapacityBlock, BusinessError> {
    let kind = text(&record.data, "kind")?;
    let state = text(&record.data, "state")?;
    if !matches!(kind.as_str(), "group" | "maintenance")
        || !matches!(state.as_str(), "active" | "released")
    {
        return Err(record_invalid());
    }
    let from = text(&record.data, "from")?;
    let to = text(&record.data, "to")?;
    if from.len() != 10
        || to.len() != 10
        || parse_date(&from).is_none()
        || parse_date(&to).is_none()
    {
        return Err(record_invalid());
    }
    let picked = match record.data.get("pickedStayIds") {
        None => Vec::new(),
        Some(Value::Array(values)) => {
            if values.len() > MAX_STAYS {
                return Err(record_invalid());
            }
            values
                .iter()
                .map(|value| value.as_str().map(str::to_owned).ok_or_else(record_invalid))
                .collect::<Result<Vec<_>, _>>()?
        }
        Some(_) => return Err(record_invalid()),
    };
    let claims = match record.data.get("groupPickupClaims") {
        None => Vec::new(),
        Some(Value::Array(values)) => {
            if values.len() > MAX_STAYS {
                return Err(record_invalid());
            }
            values
                .iter()
                .map(|value| {
                    let block_id = text(value, "blockId")?;
                    let stay_id = text(value, "stayId")?;
                    if block_id.is_empty() || stay_id.is_empty() {
                        return Err(record_invalid());
                    }
                    Ok((block_id, stay_id))
                })
                .collect::<Result<Vec<_>, _>>()?
        }
        Some(_) => return Err(record_invalid()),
    };
    let expires_at = match record.data.get("expiresAt") {
        None | Some(Value::Null) => None,
        Some(Value::Number(value)) => Some(value.as_i64().ok_or_else(record_invalid)?),
        Some(_) => return Err(record_invalid()),
    };
    Ok(CapacityBlock {
        id: record.id.clone(),
        kind,
        state,
        room_id: optional_text(&record.data, "roomId").unwrap_or_default(),
        type_id: optional_text(&record.data, "typeId").unwrap_or_default(),
        from,
        to,
        quantity: integer(&record.data, "quantity")?,
        release_date: optional_text(&record.data, "releaseDate").unwrap_or_default(),
        quota_only: record
            .data
            .get("quotaOnly")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        expires_at,
        picked,
        booking_change_id: optional_text(&record.data, "bookingChangeId").unwrap_or_default(),
        claims,
    })
}

/// Retained `dates(from, to)`: `from` inclusive to `to` exclusive, with the
/// retained 730-night refusal.
fn day_range(from: &str, to: &str) -> Result<Vec<String>, BusinessError> {
    let (Some(from_day), Some(to_day)) = (parse_date(from), parse_date(to)) else {
        return Err(invalid());
    };
    let nights = to_day - from_day;
    if nights < 0 {
        return Err(invalid());
    }
    if nights as usize > MAX_NIGHTS {
        return Err(BusinessError::invalid(
            "PMS_STAY_TOO_LONG",
            "Choose a stay of at most 730 nights.",
        ));
    }
    Ok((0..nights)
        .map(|offset| format_day(from_day + offset))
        .collect())
}

fn plus_day(date: &str) -> Result<String, BusinessError> {
    Ok(format_day(parse_date(date).ok_or_else(record_invalid)? + 1))
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

/// Retained `groupPickupCount`.
fn group_pickup_count(
    block: &CapacityBlock,
    stays: &[CapacityStay],
    blocks: &[CapacityBlock],
    date: &str,
    as_of: i64,
    exclude_stay: &str,
) -> i64 {
    let picked: std::collections::BTreeSet<&str> =
        block.picked.iter().map(String::as_str).collect();
    let live: Vec<&CapacityStay> = stays
        .iter()
        .filter(|stay| stay.id != exclude_stay && picked.contains(stay.id.as_str()))
        .collect();
    let live_ids: std::collections::BTreeSet<&str> =
        live.iter().map(|stay| stay.id.as_str()).collect();
    let mut used: std::collections::BTreeSet<&str> = live
        .iter()
        .filter(|stay| stay.covers(date) && stay.type_on(date) == block.type_id)
        .map(|stay| stay.id.as_str())
        .collect();
    for hold in blocks {
        if hold.state != "active"
            || hold.booking_change_id.is_empty()
            || !hold.covers(date)
            || !hold.live_at(as_of)
        {
            continue;
        }
        for (claim_block, stay_id) in &hold.claims {
            if claim_block == &block.id
                && stay_id != exclude_stay
                && live_ids.contains(stay_id.as_str())
            {
                used.insert(stay_id.as_str());
            }
        }
    }
    used.len() as i64
}

/// The bounded availability set used by the retained `availability()` and
/// `reconcileCapacity()` guards. Created stays are appended so later lines of
/// one command observe the rooms they already consumed.
struct CapacityBook<'a> {
    rooms: &'a [PmsRecord],
    stays: Vec<CapacityStay>,
    blocks: Vec<CapacityBlock>,
    business_date: String,
    as_of: i64,
}

impl<'a> CapacityBook<'a> {
    fn new(facts: &'a PmsReservationFacts, now: i64) -> Result<Self, BusinessError> {
        let stays = facts
            .stays
            .iter()
            .map(capacity_stay)
            .collect::<Result<Vec<_>, _>>()?;
        let blocks = facts
            .blocks
            .iter()
            .map(capacity_block)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            rooms: &facts.rooms,
            stays,
            blocks,
            business_date: facts.property.business_date.clone(),
            as_of: now,
        })
    }

    fn push(&mut self, stay: &PmsRecord) -> Result<(), BusinessError> {
        self.stays.push(capacity_stay(stay)?);
        Ok(())
    }

    fn push_pick(&mut self, block_id: &str, stay_id: &str) {
        if let Some(block) = self.blocks.iter_mut().find(|block| block.id == block_id) {
            block.picked.push(stay_id.to_owned());
        }
    }

    fn available(&self, type_id: &str, date: &str, exclude_stay: &str, exclude_block: &str) -> i64 {
        let unavailable: Vec<&str> = self
            .blocks
            .iter()
            .filter(|block| {
                block.kind == "maintenance"
                    && block.state == "active"
                    && block.from.as_str() <= date
            })
            .map(|block| block.room_id.as_str())
            .collect();
        let capacity = self
            .rooms
            .iter()
            .filter(|room| {
                room.data.get("typeId").and_then(Value::as_str) == Some(type_id)
                    && room.data.get("active").and_then(Value::as_bool) == Some(true)
                    && !unavailable.contains(&room.id.as_str())
            })
            .count() as i64;
        let occupied = self
            .stays
            .iter()
            .filter(|stay| {
                stay.id != exclude_stay && stay.type_on(date) == type_id && stay.covers(date)
            })
            .count() as i64;
        let held: i64 = self
            .blocks
            .iter()
            .filter(|block| {
                block.id != exclude_block
                    && block.kind == "group"
                    && block.state == "active"
                    && !block.quota_only
                    && block.live_at(self.as_of)
                    && block.type_id == type_id
                    && block.covers(date)
                    && block.release_date.as_str() > self.business_date.as_str()
            })
            .map(|block| {
                (block.quantity
                    - group_pickup_count(
                        block,
                        &self.stays,
                        &self.blocks,
                        date,
                        self.as_of,
                        exclude_stay,
                    ))
                .max(0)
            })
            .sum();
        capacity - occupied - held
    }

    /// Retained `checkCapacity`: one night must have a free room.
    fn check_capacity(
        &self,
        type_id: &str,
        from: &str,
        to: &str,
        exclude_stay: &str,
        exclude_block: &str,
        evaluations: &mut usize,
    ) -> Result<(), BusinessError> {
        for date in day_range(from, to)? {
            *evaluations += 1;
            if *evaluations > MAX_CAPACITY_EVALUATIONS {
                return Err(read_limit(
                    "The hotel has more capacity nights than this command can read.",
                ));
            }
            if self.available(type_id, &date, exclude_stay, exclude_block) <= 0 {
                // The retained engine reports the offending night
                // (`Not enough rooms on ${date}`); the shared command error
                // message is a static string, so the night is not interpolated.
                return Err(conflict("PMS_SOLD_OUT", "Not enough rooms on this date"));
            }
        }
        Ok(())
    }

    /// Retained `reconcileCapacity`: every room type must have nonnegative
    /// availability on every night any nonterminal stay or active block touches.
    fn reconcile(
        &self,
        room_types: &[PmsRecord],
        evaluations: &mut usize,
    ) -> Result<(), BusinessError> {
        if room_types.len() > MAX_ROOM_TYPES {
            return Err(read_limit(
                "The hotel has more room types than this command can read.",
            ));
        }
        let mut dates: Vec<String> = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for stay in &self.stays {
            for date in day_range(&stay.arrival, &stay.departure)? {
                if seen.insert(date.clone()) {
                    dates.push(date);
                }
            }
        }
        for block in &self.blocks {
            for date in day_range(&block.from, &block.to)? {
                if seen.insert(date.clone()) {
                    dates.push(date);
                }
            }
        }
        for room_type in room_types {
            if room_type.kind != "room_type" || room_type.id.is_empty() {
                return Err(record_invalid());
            }
            for date in &dates {
                *evaluations += 1;
                if *evaluations > MAX_CAPACITY_EVALUATIONS {
                    return Err(read_limit(
                        "The hotel has more capacity nights than this command can read.",
                    ));
                }
                if self.available(&room_type.id, date, "", "") < 0 {
                    return Err(conflict("PMS_SOLD_OUT", "Not enough rooms on this date"));
                }
            }
        }
        Ok(())
    }
}

/// Retained `nightlyPrices`. The rate, room type and tax configuration are all
/// PMS-owned rows; a party-priced rate needs the Finance party port.
fn nightly_prices(
    facts: &PmsReservationFacts,
    line: &PmsReservationLine,
    config: TaxConfig,
) -> Result<Vec<NightPrice>, BusinessError> {
    let rate_record = facts
        .rates
        .iter()
        .find(|rate| rate.id == line.rate_id)
        .ok_or_else(record_not_found)?;
    let rate = rate_facts(rate_record)?;
    if !rate.party_id.is_empty() {
        return Err(finance_party_port_required());
    }
    let room_type = facts
        .room_types
        .iter()
        .find(|room_type| room_type.id == line.type_id)
        .ok_or_else(record_not_found)?;
    let capacity = room_type_capacity(room_type)?;
    if line.departure <= line.arrival {
        return Err(conflict(
            "PMS_INVALID_DATES",
            "Departure must follow arrival",
        ));
    }
    let nights = day_range(&line.arrival, &line.departure)?;
    if !rate.active
        || rate.type_id != line.type_id
        || rate.stop_sell
        || rate.closed_arrival
        || rate.closed_departure
        || line.arrival.as_str() < rate.from.as_str()
        || line.departure.as_str() > plus_day(&rate.to)?.as_str()
    {
        return Err(conflict(
            "PMS_RATE_UNAVAILABLE",
            "Rate is unavailable for these dates",
        ));
    }
    if (nights.len() as i64) < rate.min_nights
        || (nights.len() as i64) > rate.max_nights
        || line.adults + line.child_ages.len() as i64 > capacity
    {
        return Err(conflict(
            "PMS_RATE_RESTRICTION",
            "Stay length or occupancy exceeds this room's limits",
        ));
    }
    let arrival_day = rate.calendar.iter().find(|day| day.date == line.arrival);
    let departure_day = rate.calendar.iter().find(|day| day.date == line.departure);
    if arrival_day.is_some_and(|day| day.closed_arrival)
        || departure_day.is_some_and(|day| day.closed_departure)
    {
        return Err(conflict(
            "PMS_RATE_RESTRICTION",
            "Arrival or departure is closed",
        ));
    }
    let mut children = 0_i64;
    for age in &line.child_ages {
        let band = rate
            .child_bands
            .iter()
            .find(|band| band.min_age <= *age && *age <= band.max_age)
            .ok_or_else(|| {
                conflict(
                    "PMS_CHILD_RATE_REQUIRED",
                    "This rate has no configured price for a child's age",
                )
            })?;
        children = children.checked_add(band.price).ok_or_else(|| {
            conflict(
                "PMS_MONEY_OVERFLOW",
                "Booking total exceeds supported amount",
            )
        })?;
    }
    let mut prices = Vec::with_capacity(nights.len());
    for date in nights {
        let override_day = rate.calendar.iter().find(|day| day.date == date);
        if override_day.is_some_and(|day| day.stop_sell) {
            return Err(conflict("PMS_RATE_UNAVAILABLE", "Stop-sell on this date"));
        }
        let price = override_day.and_then(|day| day.price).unwrap_or(rate.price);
        let price = price
            .checked_add(
                (line.adults - rate.included_guests)
                    .max(0)
                    .checked_mul(rate.extra_adult)
                    .ok_or_else(|| {
                        conflict(
                            "PMS_MONEY_OVERFLOW",
                            "Booking total exceeds supported amount",
                        )
                    })?,
            )
            .and_then(|price| price.checked_add(children))
            .ok_or_else(|| {
                conflict(
                    "PMS_MONEY_OVERFLOW",
                    "Booking total exceeds supported amount",
                )
            })?;
        let (net, service, tax, total) = amounts(price, config)?;
        prices.push(NightPrice {
            date,
            net,
            service,
            tax,
            total,
        });
    }
    Ok(prices)
}

/// Retained `block` quota guard for one selected line.
fn check_block_quota(
    book: &CapacityBook<'_>,
    block_id: &str,
    line: &PmsReservationLine,
) -> Result<(), BusinessError> {
    if block_id.is_empty() {
        return Ok(());
    }
    let unavailable = || {
        conflict(
            "PMS_BLOCK_UNAVAILABLE",
            "Group quota is unavailable or full",
        )
    };
    let block = book
        .blocks
        .iter()
        .find(|block| block.id == block_id)
        .ok_or_else(unavailable)?;
    if block.kind != "group"
        || block.state != "active"
        || !block.booking_change_id.is_empty()
        || block.release_date.as_str() <= book.business_date.as_str()
        || block.type_id != line.type_id
        || line.arrival.as_str() < block.from.as_str()
        || line.departure.as_str() > block.to.as_str()
    {
        return Err(unavailable());
    }
    for date in day_range(&line.arrival, &line.departure)? {
        if group_pickup_count(block, &book.stays, &book.blocks, &date, book.as_of, "")
            >= block.quantity
        {
            return Err(unavailable());
        }
    }
    Ok(())
}

fn push_block_pick(block: &mut PmsRecord, stay_id: &str) -> Result<(), BusinessError> {
    if block.data.get("pickedStayIds").is_none() {
        block.data["pickedStayIds"] = Value::Array(Vec::new());
    }
    let Some(Value::Array(values)) = block.data.get_mut("pickedStayIds") else {
        return Err(record_invalid());
    };
    if values.len() >= MAX_STAYS {
        return Err(read_limit(
            "The hotel block has more picked stays than this command can write.",
        ));
    }
    values.push(Value::String(stay_id.to_owned()));
    Ok(())
}

fn history(record: &PmsRecord) -> Result<Vec<Value>, BusinessError> {
    match record.data.get("history") {
        None => Ok(Vec::new()),
        Some(Value::Array(values)) => Ok(values.clone()),
        Some(_) => Err(record_invalid()),
    }
}

fn touch(record: &mut PmsRecord, now: i64) {
    record.version += 1;
    record.updated_at = now;
}

/// Plan one retained staff reservation command. The property row is already
/// locked and `command.expected_version` must match it. Every stored fact is
/// validated before any row is written, so malformed owner data fails closed
/// instead of inventing availability.
pub fn plan_reservation_command(
    actor: &PmsReservationActor,
    key: &CommandKey,
    facts: &PmsReservationFacts,
    command: &PmsReservationCommand,
    now: i64,
) -> Result<PmsReservationPlan, BusinessError> {
    command.validate()?;
    if facts.property.id != command.property_id
        || facts.property.company_id != actor.board.company_id
        || actor
            .board
            .branch_id
            .as_ref()
            .is_some_and(|branch| branch != &facts.property.branch_id)
        || facts.property.business_date.len() != 10
        || parse_date(&facts.property.business_date).is_none()
        || facts.property.version < 1
    {
        return Err(not_found());
    }
    if facts.property.version != command.expected_version {
        return Err(version_conflict());
    }
    match facts.property.state.as_str() {
        "open" => {}
        "closing" if matches!(command.operation(), PmsReservationOperation::Cancel) => {}
        "closing" => {
            return Err(conflict(
                "PMS_DAY_CLOSING",
                "Day close is in progress. Please wait for it to finish.",
            ));
        }
        "setup" => {
            return Err(conflict(
                "PMS_SETUP_REQUIRED",
                "Review readiness and open the hotel first.",
            ));
        }
        _ => return Err(record_invalid()),
    }
    if facts.rooms.len() > MAX_ROOMS
        || facts.room_types.len() > MAX_ROOM_TYPES
        || facts.rates.len() > MAX_RATES
        || facts.blocks.len() > MAX_BLOCKS
        || facts.stays.len() > MAX_STAYS
        || facts.accounts.len() > MAX_ACCOUNTS
    {
        return Err(read_limit(
            "The hotel has more reservation facts than this command can read.",
        ));
    }
    let property_version = facts.property.version + 1;
    match &command.input {
        PmsReservationCommandInput::Create(input) => {
            plan_create(actor, key, facts, input, now, property_version)
        }
        PmsReservationCommandInput::Change(input) => {
            plan_change(actor, facts, input, now, property_version)
        }
        PmsReservationCommandInput::Cancel(input) => {
            plan_cancel(actor, facts, input, now, property_version)
        }
    }
}

fn plan_create(
    actor: &PmsReservationActor,
    key: &CommandKey,
    facts: &PmsReservationFacts,
    input: &PmsReservationCreateInput,
    now: i64,
    property_version: i64,
) -> Result<PmsReservationPlan, BusinessError> {
    let (guest_id, planned_guest) = match &input.guest_id {
        Some(guest_id) => {
            let guest = facts
                .guest
                .as_ref()
                .filter(|guest| guest.id == *guest_id && guest.kind == "guest")
                .ok_or_else(record_not_found)?;
            object(&guest.data)?;
            (guest.id.clone(), None)
        }
        None => {
            let guest = input.guest.as_ref().ok_or_else(|| {
                BusinessError::invalid("PMS_GUEST_REQUIRED", "Enter guest details")
            })?;
            let record = PmsRecord {
                id: created_record_id("guest", key, &facts.property.id, ""),
                kind: "guest".to_owned(),
                version: 1,
                updated_at: now,
                data: serde_json::json!({
                    "name": guest.name,
                    "email": guest.email,
                    "phone": guest.phone,
                    "nationality": "",
                    "documentRef": "",
                    "notes": ""
                }),
            };
            (record.id.clone(), Some(record))
        }
    };
    if !input.external_ref.is_empty() && facts.external_ref_exists {
        return Err(conflict(
            "PMS_EXTERNAL_REF_EXISTS",
            "A booking reference for this channel already exists",
        ));
    }
    let config = tax_config(&facts.property)?;
    let business_date = facts.property.business_date.as_str();
    let mut block = match &facts.block {
        Some(record) if !input.block_id.is_empty() && record.id == input.block_id => {
            let block = capacity_block(record)?;
            if block.kind != "group" {
                return Err(record_invalid());
            }
            Some(record.clone())
        }
        Some(_) | None if input.block_id.is_empty() => None,
        Some(_) | None => return Err(record_not_found()),
    };
    let mut book = CapacityBook::new(facts, now)?;
    let mut evaluations = 0_usize;
    let reservation_id = created_record_id("reservation", key, &facts.property.id, "");
    let number = reservation_number(key, business_date)?;
    let mut stays = Vec::with_capacity(input.rooms.len());
    let mut accounts = Vec::with_capacity(input.rooms.len());
    for (index, line) in input.rooms.iter().enumerate() {
        if line.arrival.as_str() < business_date || line.departure <= line.arrival {
            return Err(conflict(
                "PMS_INVALID_DATES",
                "Stay date cannot be in the past and departure must be after arrival",
            ));
        }
        let prices = nightly_prices(facts, line, config)?;
        for quantity in 0..line.quantity {
            check_block_quota(&book, &input.block_id, line)?;
            book.check_capacity(
                &line.type_id,
                &line.arrival,
                &line.departure,
                "",
                &input.block_id,
                &mut evaluations,
            )?;
            let stay_id = created_record_id(
                "stay",
                key,
                &facts.property.id,
                &format!("{index}:{quantity}"),
            );
            let account_id = created_record_id(
                "guest-account",
                key,
                &facts.property.id,
                &format!("{index}:{quantity}"),
            );
            let stay = PmsRecord {
                id: stay_id.clone(),
                kind: "stay".to_owned(),
                version: 1,
                updated_at: now,
                data: serde_json::json!({
                    "reservationId": reservation_id,
                    "guestId": guest_id,
                    "typeId": line.type_id,
                    "rateId": line.rate_id,
                    "roomId": "",
                    "arrival": line.arrival,
                    "departure": line.departure,
                    "adults": line.adults,
                    "childAges": line.child_ages,
                    "state": "reserved",
                    "guestAccountId": account_id,
                    "prices": prices.iter().map(NightPrice::value).collect::<Vec<_>>(),
                    "segments": []
                }),
            };
            book.push(&stay)?;
            let account = PmsRecord {
                id: account_id,
                kind: "guest_account".to_owned(),
                version: 1,
                updated_at: now,
                data: serde_json::json!({
                    "guestId": guest_id,
                    "reservationId": reservation_id,
                    "name": format!("{number} · {}", quantity + 1),
                    "partyId": "",
                    "state": "open",
                    "due": 0,
                    "advance": 0,
                    "advanceTax": 0,
                    "deposit": 0
                }),
            };
            if let Some(block) = &mut block {
                push_block_pick(block, &stay_id)?;
                book.push_pick(&input.block_id, &stay_id);
            }
            stays.push(stay);
            accounts.push(account);
        }
    }
    book.reconcile(&facts.room_types, &mut evaluations)?;
    let reservation = PmsRecord {
        id: reservation_id.clone(),
        kind: "reservation".to_owned(),
        version: 1,
        updated_at: now,
        data: serde_json::json!({
            "number": number,
            "guestId": guest_id,
            "source": input.source,
            "campaignCode": input.campaign_code,
            "externalRef": input.external_ref,
            "groupName": input.group_name,
            "notes": input.notes,
            "state": "confirmed",
            "createdBy": actor.board.user_id,
            "history": [{
                "at": now,
                "actor": actor.board.user_id,
                "action": PmsReservationOperation::Create.history_action()
            }]
        }),
    };
    let source_key = (!input.external_ref.is_empty())
        .then(|| format!("reservation:{}:{}", input.source, input.external_ref));
    Ok(PmsReservationPlan {
        result: PmsReservationResult {
            id: reservation_id,
            property_id: facts.property.id.clone(),
            version: property_version,
        },
        reservation,
        guest: planned_guest,
        stays,
        accounts,
        block,
        source_key,
        property_version,
    })
}

fn reservation_number(key: &CommandKey, business_date: &str) -> Result<String, BusinessError> {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(key.as_str().as_bytes());
    let suffix = hex::encode(&digest[..4]);
    Ok(format!("PMS-{}-{suffix}", business_date.replace('-', "")))
}

fn plan_change(
    actor: &PmsReservationActor,
    facts: &PmsReservationFacts,
    input: &PmsReservationChangeInput,
    now: i64,
    property_version: i64,
) -> Result<PmsReservationPlan, BusinessError> {
    let mut reservation = target_reservation(facts, &input.reservation_id)?;
    if text(&reservation.data, "state")? != "confirmed" {
        return Err(conflict(
            "PMS_RESERVATION_CLOSED",
            "Booking is not confirmed",
        ));
    }
    check_expected_version(&reservation, input.expected_reservation_version)?;
    let mut siblings = reservation_stays(facts, &reservation.id)?;
    if siblings.is_empty() || siblings.len() != input.rooms.len() {
        return Err(contact_hotel());
    }
    if siblings
        .iter()
        .any(|stay| !matches!(text(&stay.data, "state").as_deref(), Ok("reserved")))
    {
        return Err(conflict(
            "PMS_ALREADY_CHECKED_IN",
            "Handle the checked-in stay first",
        ));
    }
    if facts.financial_activity {
        return Err(contact_hotel());
    }
    siblings.sort_by(|left, right| left.id.cmp(&right.id));
    let config = tax_config(&facts.property)?;
    let business_date = facts.property.business_date.as_str();
    let book = CapacityBook::new(facts, now)?;
    let mut evaluations = 0_usize;
    let mut stays = Vec::with_capacity(siblings.len());
    let mut changed = Vec::with_capacity(siblings.len());
    for (mut stay, line) in siblings.into_iter().zip(input.rooms.iter()) {
        if line.arrival.as_str() < business_date || line.departure <= line.arrival {
            return Err(conflict(
                "PMS_INVALID_DATES",
                "Stay date cannot be in the past and departure must be after arrival",
            ));
        }
        let prices = nightly_prices(facts, line, config)?;
        book.check_capacity(
            &line.type_id,
            &line.arrival,
            &line.departure,
            &stay.id,
            "",
            &mut evaluations,
        )?;
        stay.data["typeId"] = Value::String(line.type_id.clone());
        stay.data["rateId"] = Value::String(line.rate_id.clone());
        stay.data["arrival"] = Value::String(line.arrival.clone());
        stay.data["departure"] = Value::String(line.departure.clone());
        stay.data["adults"] = Value::from(line.adults);
        stay.data["childAges"] = Value::Array(
            line.child_ages
                .iter()
                .map(|age| Value::from(*age))
                .collect(),
        );
        stay.data["prices"] =
            Value::Array(prices.iter().map(NightPrice::value).collect::<Vec<_>>());
        stay.data["roomId"] = Value::String(String::new());
        stay.data["segments"] = Value::Array(Vec::new());
        if let Some(object) = stay.data.as_object_mut() {
            object.remove("roomTypePeriods");
        }
        object(&stay.data)?;
        changed.push(stay.id.clone());
        touch(&mut stay, now);
        stays.push(stay);
    }
    let mut after = CapacityBook::new(facts, now)?;
    after
        .stays
        .retain(|stay| !changed.iter().any(|id| id == &stay.id));
    for stay in &stays {
        after.push(stay)?;
    }
    after.reconcile(&facts.room_types, &mut evaluations)?;
    if let Some(notes) = &input.notes {
        reservation.data["notes"] = Value::String(notes.clone());
    }
    if let Some(group_name) = &input.group_name {
        reservation.data["groupName"] = Value::String(group_name.clone());
    }
    if let Some(campaign_code) = &input.campaign_code {
        reservation.data["campaignCode"] = Value::String(campaign_code.clone());
    }
    let mut entries = history(&reservation)?;
    entries.push(serde_json::json!({
        "at": now,
        "actor": actor.board.user_id,
        "action": PmsReservationOperation::Change.history_action(),
        "reason": input.reason
    }));
    reservation.data["history"] = Value::Array(entries);
    object(&reservation.data)?;
    touch(&mut reservation, now);
    Ok(PmsReservationPlan {
        result: PmsReservationResult {
            id: reservation.id.clone(),
            property_id: facts.property.id.clone(),
            version: property_version,
        },
        reservation,
        guest: None,
        stays,
        accounts: Vec::new(),
        block: None,
        source_key: None,
        property_version,
    })
}

fn plan_cancel(
    actor: &PmsReservationActor,
    facts: &PmsReservationFacts,
    input: &PmsReservationCancelInput,
    now: i64,
    property_version: i64,
) -> Result<PmsReservationPlan, BusinessError> {
    let mut reservation = target_reservation(facts, &input.reservation_id)?;
    if text(&reservation.data, "state")? != "confirmed" {
        return Err(conflict(
            "PMS_RESERVATION_CLOSED",
            "Booking is not confirmed",
        ));
    }
    check_expected_version(&reservation, input.expected_reservation_version)?;
    if input.fee > 0 {
        return Err(conflict(
            "PMS_FEE_ACCOUNT_REQUIRED",
            "Finance must approve separate cancellation and no-show fee accounting",
        ));
    }
    let mut stays = reservation_stays(facts, &reservation.id)?;
    if stays
        .iter()
        .any(|stay| !matches!(text(&stay.data, "state").as_deref(), Ok("reserved")))
    {
        return Err(conflict(
            "PMS_ALREADY_CHECKED_IN",
            "Handle the checked-in stay first",
        ));
    }
    for stay in &mut stays {
        object(&stay.data)?;
        stay.data["state"] = Value::String("cancelled".to_owned());
        touch(stay, now);
    }
    let notes = optional_text(&reservation.data, "notes").unwrap_or_default();
    reservation.data["notes"] = Value::String(format!("{notes}\n{}", input.reason));
    reservation.data["state"] = Value::String("cancelled".to_owned());
    let mut entries = history(&reservation)?;
    entries.push(serde_json::json!({
        "at": now,
        "actor": actor.board.user_id,
        "action": PmsReservationOperation::Cancel.history_action(),
        "reason": input.reason
    }));
    reservation.data["history"] = Value::Array(entries);
    object(&reservation.data)?;
    touch(&mut reservation, now);
    Ok(PmsReservationPlan {
        result: PmsReservationResult {
            id: reservation.id.clone(),
            property_id: facts.property.id.clone(),
            version: property_version,
        },
        reservation,
        guest: None,
        stays,
        accounts: Vec::new(),
        block: None,
        source_key: None,
        property_version,
    })
}

fn contact_hotel() -> BusinessError {
    conflict(
        "PMS_CONTACT_HOTEL",
        "This room can no longer be changed online. Contact the hotel",
    )
}

fn target_reservation(
    facts: &PmsReservationFacts,
    reservation_id: &str,
) -> Result<PmsRecord, BusinessError> {
    match &facts.reservation {
        Some(reservation)
            if reservation.id == reservation_id && reservation.kind == "reservation" =>
        {
            object(&reservation.data)?;
            Ok(reservation.clone())
        }
        _ => Err(record_not_found()),
    }
}

fn reservation_stays(
    facts: &PmsReservationFacts,
    reservation_id: &str,
) -> Result<Vec<PmsRecord>, BusinessError> {
    let mut stays = Vec::new();
    for stay in &facts.stays {
        if stay.kind != "stay" {
            return Err(record_invalid());
        }
        if optional_text(&stay.data, "reservationId").as_deref() == Some(reservation_id) {
            object(&stay.data)?;
            stays.push(stay.clone());
        }
    }
    Ok(stays)
}

fn check_expected_version(
    reservation: &PmsRecord,
    expected: Option<i64>,
) -> Result<(), BusinessError> {
    if reservation.version < 1 || expected.is_some_and(|version| version != reservation.version) {
        return Err(version_conflict());
    }
    Ok(())
}

/// Deterministic identity for a record this family creates. The retained
/// engine mints a UUID; deriving it from the retained command key keeps one key
/// on one record while the retained duplicate guards still reject a second row
/// that would collide (duplicate channel reference).
fn created_record_id(
    prefix: &'static str,
    key: &CommandKey,
    property_id: &str,
    scope: &str,
) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(
        format!("pms-{prefix}-create:{}:{property_id}:{scope}", key.as_str()).as_bytes(),
    );
    format!("pms-{prefix}-{}", hex::encode(&digest[..16]))
}
