//! PMS guest-account and PMS-owned charge fact commands.
//!
//! These rules are the retained `guest_account.create`, `guest_account.transfer`,
//! `charge.add`, `charge.import_pos` and `charge.reverse` transitions reduced to
//! owner facts: the property row, the target guest account (plus the transfer
//! destination), the linked guest/reservation, the source entry and the
//! account's bounded refund facts. They never read a Finance journal, a cash or
//! card settlement, a People/Commercial projection or browser state.
//!
//! Journal posting, cash/card settlement, refunds, invoice/credit-note issuance
//! and minibar/linen stock are separate owner/provider ports. A charge whose
//! retained outcome needs an advance allocation fails closed with
//! [`PMS_GUEST_PAYMENT_PORT_REQUIRED`] instead of faking a settled balance, and
//! an adjustment that would leave a credit balance fails closed with
//! `PMS_GUEST_CREDIT_REQUIRES_FINANCE`. A command result is a property receipt
//! version, never a permission or a current authorization proof.
use super::room_board::{
    PmsBoardActor, PmsPropertyRow, PmsRecord, cp_len, ecma_trim, not_found, parse_date,
    record_not_found,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Retained `idField` limit (never trimmed) and `textField` limit (trimmed).
pub const MAX_ID: usize = 128;
pub const MAX_TEXT: usize = 2_000;
pub const MAX_SOURCE_KEY: usize = 500;
/// Retained `moneyField` (`0..=99999999999`) bound for every account amount.
pub const MAX_MONEY: i64 = 99_999_999_999;
/// Bounded per-account refund facts read by the retained `noRefund` guard.
pub const MAX_REFUNDS: usize = 500;
/// Bounded account entries read for one entry reference check.
pub const MAX_REFERENCES: usize = 2;

/// The retained guest-account/charge command family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmsGuestAccountOperation {
    Create,
    Update,
    ChargeAdd,
    ChargeImport,
    Adjustment,
    Transfer,
}

impl PmsGuestAccountOperation {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "guest_account.create" => Some(Self::Create),
            "guest_account.update" => Some(Self::Update),
            "charge.add" => Some(Self::ChargeAdd),
            "charge.import_pos" => Some(Self::ChargeImport),
            "guest_account.adjustment" => Some(Self::Adjustment),
            "guest_account.transfer" => Some(Self::Transfer),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "guest_account.create",
            Self::Update => "guest_account.update",
            Self::ChargeAdd => "charge.add",
            Self::ChargeImport => "charge.import_pos",
            Self::Adjustment => "guest_account.adjustment",
            Self::Transfer => "guest_account.transfer",
        }
    }

    /// Retained receipt, audit and outbox operation (`pms.${op}`).
    pub fn receipt_operation(self) -> &'static str {
        match self {
            Self::Create => "pms.guest_account.create",
            Self::Update => "pms.guest_account.update",
            Self::ChargeAdd => "pms.charge.add",
            Self::ChargeImport => "pms.charge.import_pos",
            Self::Adjustment => "pms.guest_account.adjustment",
            Self::Transfer => "pms.guest_account.transfer",
        }
    }

    /// Retained `operationPermission`: every `guest_account.*` and `charge.*`
    /// cashier operation needs `pms.cashier`. Composition still resolves and
    /// revalidates the permission.
    pub fn permission(self) -> &'static str {
        "pms.cashier"
    }

    /// Record family written by the command.
    pub fn aggregate(self) -> &'static str {
        match self {
            Self::Create | Self::Update => "guest_account",
            Self::ChargeAdd | Self::ChargeImport | Self::Adjustment | Self::Transfer => "entry",
        }
    }
}

/// Retained `guest_account.create` input. The retained engine derives `guestId`
/// from the reservation; this owner command requires the explicit guest and
/// validates the retained reservation linkage when one is supplied.
#[derive(Clone, Debug)]
pub struct PmsGuestAccountOpenInput {
    pub guest_id: String,
    pub reservation_id: String,
    pub name: String,
}

/// Retained account update over PMS-owned fields. Only the bill name is
/// mutable; state, money and Finance-owned invoice/credit references are not.
#[derive(Clone, Debug)]
pub struct PmsGuestAccountUpdateInput {
    pub account_id: String,
    pub name: String,
}

/// Retained `charge.add` input. The retained `amounts()` derivation from the
/// property tax/service configuration runs in [`plan_guest_account`].
#[derive(Clone, Debug)]
pub struct PmsChargeAddInput {
    pub account_id: String,
    pub price: i64,
    pub description: String,
    pub stay_id: String,
    pub source_key: String,
}

/// Retained `charge.import_pos` input: explicit components that must sum to the
/// total, with the provider source key that makes the import replayable.
#[derive(Clone, Debug)]
pub struct PmsChargeImportInput {
    pub account_id: String,
    pub stay_id: String,
    pub source_key: String,
    pub description: String,
    pub net: i64,
    pub service: i64,
    pub tax: i64,
    pub total: i64,
}

/// A PMS-owned adjustment entry. When `original_entry_id` is present the entry
/// is the retained `charge.reverse` negation; otherwise the signed components
/// are a bounded manual adjustment.
#[derive(Clone, Debug)]
pub struct PmsAdjustmentInput {
    pub account_id: String,
    pub original_entry_id: String,
    pub description: String,
    pub net: i64,
    pub service: i64,
    pub tax: i64,
    pub total: i64,
    pub source_key: String,
}

/// Retained `guest_account.transfer` input: one unpaid positive entry moved to
/// another open account of the same property.
#[derive(Clone, Debug)]
pub struct PmsTransferInput {
    pub entry_id: String,
    pub to_account_id: String,
    pub reason: String,
}

/// One typed input per operation. The variant must match
/// [`PmsGuestAccountCommand::operation`].
#[derive(Clone, Debug)]
pub enum PmsGuestAccountInput {
    Create(PmsGuestAccountOpenInput),
    Update(PmsGuestAccountUpdateInput),
    ChargeAdd(PmsChargeAddInput),
    ChargeImport(PmsChargeImportInput),
    Adjustment(PmsAdjustmentInput),
    Transfer(PmsTransferInput),
}

impl PmsGuestAccountInput {
    fn operation(&self) -> PmsGuestAccountOperation {
        match self {
            Self::Create(_) => PmsGuestAccountOperation::Create,
            Self::Update(_) => PmsGuestAccountOperation::Update,
            Self::ChargeAdd(_) => PmsGuestAccountOperation::ChargeAdd,
            Self::ChargeImport(_) => PmsGuestAccountOperation::ChargeImport,
            Self::Adjustment(_) => PmsGuestAccountOperation::Adjustment,
            Self::Transfer(_) => PmsGuestAccountOperation::Transfer,
        }
    }

    /// The guest account or entry this command names, when the retained input
    /// names one.
    pub fn target_id(&self) -> Option<&str> {
        match self {
            Self::Create(_) => None,
            Self::Update(input) => Some(&input.account_id),
            Self::ChargeAdd(input) => Some(&input.account_id),
            Self::ChargeImport(input) => Some(&input.account_id),
            Self::Adjustment(input) => Some(&input.account_id),
            Self::Transfer(input) => Some(&input.entry_id),
        }
    }

    /// Account id this command must load, when the input names one directly.
    pub fn account_id(&self) -> Option<&str> {
        match self {
            Self::Create(_) | Self::Transfer(_) => None,
            Self::Update(input) => Some(&input.account_id),
            Self::ChargeAdd(input) => Some(&input.account_id),
            Self::ChargeImport(input) => Some(&input.account_id),
            Self::Adjustment(input) => Some(&input.account_id),
        }
    }

    /// Source key carried by the command, when the retained input carries one.
    pub fn source_key(&self) -> Option<&str> {
        match self {
            Self::ChargeAdd(input) if !input.source_key.is_empty() => Some(&input.source_key),
            Self::ChargeImport(input) => Some(&input.source_key),
            Self::Adjustment(input) if !input.source_key.is_empty() => Some(&input.source_key),
            Self::Create(_)
            | Self::Update(_)
            | Self::ChargeAdd(_)
            | Self::Adjustment(_)
            | Self::Transfer(_) => None,
        }
    }

    /// Entry id a reversal operation must load, when the input names one.
    pub fn original_entry_id(&self) -> Option<&str> {
        match self {
            Self::Adjustment(input) if !input.original_entry_id.is_empty() => {
                Some(&input.original_entry_id)
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PmsGuestAccountCommand {
    pub operation: PmsGuestAccountOperation,
    pub property_id: String,
    /// Current `pms_properties.version`. Required: this command family never
    /// accepts a stale property snapshot.
    pub expected_version: i64,
    pub input: PmsGuestAccountInput,
}

impl PmsGuestAccountCommand {
    pub fn operation(&self) -> PmsGuestAccountOperation {
        self.operation
    }

    /// Retained `.trim()` boundaries for names, descriptions, reasons and
    /// source keys. Ids are never trimmed.
    pub fn normalize(&mut self) {
        match &mut self.input {
            PmsGuestAccountInput::Create(input) => {
                input.name = ecma_trim(&input.name).to_owned();
            }
            PmsGuestAccountInput::Update(input) => {
                input.name = ecma_trim(&input.name).to_owned();
            }
            PmsGuestAccountInput::ChargeAdd(input) => {
                input.description = ecma_trim(&input.description).to_owned();
                input.source_key = ecma_trim(&input.source_key).to_owned();
            }
            PmsGuestAccountInput::ChargeImport(input) => {
                input.description = ecma_trim(&input.description).to_owned();
                input.source_key = ecma_trim(&input.source_key).to_owned();
            }
            PmsGuestAccountInput::Adjustment(input) => {
                input.description = ecma_trim(&input.description).to_owned();
                input.source_key = ecma_trim(&input.source_key).to_owned();
            }
            PmsGuestAccountInput::Transfer(input) => {
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
            PmsGuestAccountInput::Create(input) => {
                if !valid_id(&input.guest_id)
                    || (!input.reservation_id.is_empty() && !valid_id(&input.reservation_id))
                    || !valid_text(&input.name)
                {
                    return Err(invalid());
                }
            }
            PmsGuestAccountInput::Update(input) => {
                if !valid_id(&input.account_id) || !valid_text(&input.name) {
                    return Err(invalid());
                }
            }
            PmsGuestAccountInput::ChargeAdd(input) => {
                if !valid_id(&input.account_id)
                    || !(1..=MAX_MONEY).contains(&input.price)
                    || !valid_text(&input.description)
                    || (!input.stay_id.is_empty() && !valid_id(&input.stay_id))
                    || !valid_optional_source_key(&input.source_key)
                {
                    return Err(invalid());
                }
            }
            PmsGuestAccountInput::ChargeImport(input) => {
                if !valid_id(&input.account_id)
                    || !valid_source_key(&input.source_key)
                    || !valid_text(&input.description)
                    || (!input.stay_id.is_empty() && !valid_id(&input.stay_id))
                    || !valid_money(input.net)
                    || !valid_money(input.service)
                    || !valid_money(input.tax)
                    || !valid_money(input.total)
                    || checked_total(input.net, input.service, input.tax) != Some(input.total)
                    || input.total <= 0
                {
                    return Err(invalid());
                }
            }
            PmsGuestAccountInput::Adjustment(input) => {
                if !valid_id(&input.account_id)
                    || (!input.original_entry_id.is_empty() && !valid_id(&input.original_entry_id))
                    || !valid_text(&input.description)
                    || !valid_signed_money(input.net)
                    || !valid_signed_money(input.service)
                    || !valid_signed_money(input.tax)
                    || !valid_signed_money(input.total)
                    || checked_total(input.net, input.service, input.tax) != Some(input.total)
                    || input.total == 0
                    || !valid_optional_source_key(&input.source_key)
                {
                    return Err(invalid());
                }
            }
            PmsGuestAccountInput::Transfer(input) => {
                if !valid_id(&input.entry_id)
                    || !valid_id(&input.to_account_id)
                    || !valid_text(&input.reason)
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && cp_len(value) <= MAX_ID && !value.chars().any(char::is_control)
}

fn valid_text(value: &str) -> bool {
    (1..=MAX_TEXT).contains(&cp_len(ecma_trim(value)))
}

fn valid_source_key(value: &str) -> bool {
    let value = ecma_trim(value);
    !value.is_empty() && cp_len(value) <= MAX_SOURCE_KEY && !value.chars().any(char::is_control)
}

fn valid_optional_source_key(value: &str) -> bool {
    value.is_empty() || valid_source_key(value)
}

fn valid_money(value: i64) -> bool {
    (0..=MAX_MONEY).contains(&value)
}

fn valid_signed_money(value: i64) -> bool {
    value.unsigned_abs() <= MAX_MONEY.unsigned_abs()
}

fn checked_total(net: i64, service: i64, tax: i64) -> Option<i64> {
    net.checked_add(service)?.checked_add(tax)
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PMS_GUEST_ACCOUNT_REQUEST_INVALID",
        "Use one bounded guest account action with the current hotel version.",
    )
}

pub fn record_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_GUEST_ACCOUNT_RECORD_INVALID",
        "The guest account record could not be read safely.",
    )
}

pub fn read_limit(message: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_GUEST_ACCOUNT_READ_LIMIT",
        message,
    )
}

/// Explicit fail-closed contract for the retained advance allocation path.
pub fn payment_port_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_GUEST_PAYMENT_PORT_REQUIRED",
        "Advance allocation needs the Finance guest-payment port; submit it after the payment provider posts.",
    )
}

fn credit_requires_finance() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_GUEST_CREDIT_REQUIRES_FINANCE",
        "A credit balance needs the Finance credit-note and advance port.",
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

/// Owner command context. The board actor supplies Company/branch scope,
/// permissions, module and work-context expiry; the assignment facts are
/// retained receipt evidence without granting anything.
#[derive(Clone, Debug)]
pub struct PmsGuestAccountActor {
    pub board: PmsBoardActor,
    pub assignment_id: String,
    pub role: String,
    pub revision: i64,
    pub assignment_version: i64,
}

impl PmsGuestAccountActor {
    /// Mirror of the retained workspace gate (read permission, module, branch
    /// area, unexpired work context) followed by the retained
    /// `operationPermission` for this operation.
    pub fn require_command(
        &self,
        operation: PmsGuestAccountOperation,
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
pub struct PmsGuestAccountResult {
    pub id: String,
    pub account_id: String,
    pub property_id: String,
    pub version: i64,
    pub state: String,
    pub due: i64,
    pub advance: i64,
    pub deposit: i64,
}

/// Authoritative owner snapshot loaded under the property row lock. Every list
/// is bounded.
#[derive(Clone, Debug)]
pub struct PmsGuestAccountFacts {
    pub property: PmsPropertyRow,
    /// Target guest account (`update`, `charge.*`, `adjustment`, transfer
    /// source).
    pub account: Option<PmsRecord>,
    /// Transfer destination account.
    pub destination: Option<PmsRecord>,
    /// Linked guest record for `guest_account.create`.
    pub guest: Option<PmsRecord>,
    /// Linked reservation record for `guest_account.create`.
    pub reservation: Option<PmsRecord>,
    /// Original entry for `charge.reverse`-style adjustments and transfers.
    pub original_entry: Option<PmsRecord>,
    /// Existing entry retained for this command's source key, when any.
    pub source_entry: Option<PmsRecord>,
    /// Bounded `requested`/`approved` refund facts for the retained `noRefund`
    /// guard.
    pub account_refunds: Vec<PmsRecord>,
    /// Bounded `requested`/`approved` refund facts of the transfer destination.
    pub destination_refunds: Vec<PmsRecord>,
    /// Entries already referencing the original entry (`originalId`).
    pub original_references: Vec<PmsRecord>,
}

impl PmsGuestAccountFacts {
    pub fn empty(property: PmsPropertyRow) -> Self {
        Self {
            property,
            account: None,
            destination: None,
            guest: None,
            reservation: None,
            original_entry: None,
            source_entry: None,
            account_refunds: Vec::new(),
            destination_refunds: Vec::new(),
            original_references: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PmsGuestAccountPlan {
    pub result: PmsGuestAccountResult,
    /// Updated target account (never also a created account).
    pub account: Option<PmsRecord>,
    /// New account (`guest_account.create`).
    pub new_account: Option<PmsRecord>,
    /// Updated transfer destination account.
    pub destination: Option<PmsRecord>,
    /// New entry this command created.
    pub new_entry: Option<PmsRecord>,
    /// New destination entry for a transfer.
    pub new_destination_entry: Option<PmsRecord>,
    /// Provider source key to retain for the new entry.
    pub source_key: Option<String>,
    /// Property version after this command. Audit/outbox rows use this version.
    pub property_version: i64,
}

#[derive(Clone, Debug)]
struct AccountFacts {
    state: String,
    due: i64,
    advance: i64,
    deposit: i64,
    refund_reserved: i64,
    deposit_refund_reserved: i64,
    invoice_id: String,
}

impl AccountFacts {
    fn read(record: &PmsRecord) -> Result<Self, BusinessError> {
        if record.kind != "guest_account" {
            return Err(record_invalid());
        }
        let data = record.data.as_object().ok_or_else(record_invalid)?;
        if !valid_text(&text(data, "name")?) {
            return Err(record_invalid());
        }
        // Retained linkage fields must be present strings; they are preserved,
        // never rewritten by this command family.
        let _guest_id = text(data, "guestId")?;
        let _reservation_id = text(data, "reservationId")?;
        let state = text(data, "state")?;
        if !matches!(state.as_str(), "open" | "credit" | "closed") {
            return Err(record_invalid());
        }
        Ok(Self {
            state,
            due: money(data, "due")?,
            advance: money(data, "advance")?,
            deposit: money(data, "deposit")?,
            refund_reserved: optional_money(data, "refundReserved")?,
            deposit_refund_reserved: optional_money(data, "depositRefundReserved")?,
            invoice_id: optional_text(data, "invoiceId"),
        })
    }

    fn is_open(&self) -> bool {
        matches!(self.state.as_str(), "open" | "credit")
    }

    fn advance_available(&self) -> i64 {
        self.advance.saturating_sub(self.refund_reserved)
    }
}

#[derive(Clone, Debug)]
struct EntryFacts {
    account_id: String,
    kind: String,
    total: i64,
    net: i64,
    service: i64,
    tax: i64,
    method: String,
}

impl EntryFacts {
    fn read(record: &PmsRecord) -> Result<Self, BusinessError> {
        if record.kind != "entry" {
            return Err(record_invalid());
        }
        let data = record.data.as_object().ok_or_else(record_invalid)?;
        let kind = text(data, "kind")?;
        if !matches!(
            kind.as_str(),
            "charge" | "payment" | "allocation" | "refund" | "adjustment" | "transfer" | "deposit"
        ) {
            return Err(record_invalid());
        }
        let account_id = text(data, "guestAccountId")?;
        if !valid_id(&account_id) {
            return Err(record_invalid());
        }
        let net = signed_money(data, "net")?;
        let service = signed_money(data, "service")?;
        let tax = signed_money(data, "tax")?;
        let total = signed_money(data, "total")?;
        if checked_total(net, service, tax) != Some(total) {
            return Err(record_invalid());
        }
        Ok(Self {
            account_id,
            kind,
            total,
            net,
            service,
            tax,
            method: optional_text(data, "method"),
        })
    }
}

fn text(
    data: &serde_json::Map<String, serde_json::Value>,
    key: &'static str,
) -> Result<String, BusinessError> {
    data.get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(record_invalid)
}

fn optional_text(data: &serde_json::Map<String, serde_json::Value>, key: &'static str) -> String {
    data.get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn money(
    data: &serde_json::Map<String, serde_json::Value>,
    key: &'static str,
) -> Result<i64, BusinessError> {
    let value = data
        .get(key)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(record_invalid)?;
    if !valid_money(value) {
        return Err(record_invalid());
    }
    Ok(value)
}

fn signed_money(
    data: &serde_json::Map<String, serde_json::Value>,
    key: &'static str,
) -> Result<i64, BusinessError> {
    let value = data
        .get(key)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(record_invalid)?;
    if !valid_signed_money(value) {
        return Err(record_invalid());
    }
    Ok(value)
}

fn optional_money(
    data: &serde_json::Map<String, serde_json::Value>,
    key: &'static str,
) -> Result<i64, BusinessError> {
    match data.get(key) {
        None | Some(serde_json::Value::Null) => Ok(0),
        Some(serde_json::Value::Number(value)) => {
            let value = value.as_i64().ok_or_else(record_invalid)?;
            if !valid_money(value) {
                return Err(record_invalid());
            }
            Ok(value)
        }
        Some(_) => Err(record_invalid()),
    }
}

/// Retained `amounts()` derivation from the property tax/service configuration.
pub fn price_amounts(
    price: i64,
    tax_bps: i64,
    service_bps: i64,
    tax_inclusive: bool,
) -> Result<(i64, i64, i64, i64), BusinessError> {
    if !(1..=MAX_MONEY).contains(&price)
        || !(0..=10_000).contains(&tax_bps)
        || !(0..=10_000).contains(&service_bps)
    {
        return Err(record_invalid());
    }
    let round = |value: i128, divisor: i128| (value + divisor / 2) / divisor;
    let price = i128::from(price);
    let net = if tax_inclusive {
        round(
            price * 100_000_000,
            i128::from(10_000 + service_bps) * i128::from(10_000 + tax_bps),
        )
    } else {
        price
    };
    let service = round(net * i128::from(service_bps), 10_000);
    let tax = if tax_inclusive {
        price - net - service
    } else {
        round((net + service) * i128::from(tax_bps), 10_000)
    };
    let total = net + service + tax;
    let convert = |value: i128| {
        i64::try_from(value)
            .ok()
            .filter(|value| valid_money(*value))
            .ok_or_else(record_invalid)
    };
    Ok((
        convert(net)?,
        convert(service)?,
        convert(tax)?,
        convert(total)?,
    ))
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PropertyConfig {
    #[serde(default)]
    tax_bps: i64,
    #[serde(default)]
    service_bps: i64,
    #[serde(default)]
    tax_inclusive: bool,
}

fn property_config(property: &PmsPropertyRow) -> Result<PropertyConfig, BusinessError> {
    let config: PropertyConfig =
        serde_json::from_str(&property.config_json).map_err(|_| record_invalid())?;
    if !(0..=10_000).contains(&config.tax_bps) || !(0..=10_000).contains(&config.service_bps) {
        return Err(record_invalid());
    }
    Ok(config)
}

fn require_approved_chart(property: &PmsPropertyRow) -> Result<(), BusinessError> {
    let value: serde_json::Value =
        serde_json::from_str(&property.config_json).map_err(|_| record_invalid())?;
    let approved = value
        .get("approvedBy")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| !ecma_trim(value).is_empty());
    if !approved {
        return Err(conflict(
            "PMS_SETUP_REQUIRED",
            "Have Finance approve the chart of accounts first",
        ));
    }
    Ok(())
}

fn open_account(facts: &PmsGuestAccountFacts) -> Result<(PmsRecord, AccountFacts), BusinessError> {
    let record = facts.account.clone().ok_or_else(record_not_found)?;
    let account = AccountFacts::read(&record)?;
    if !account.is_open() {
        return Err(conflict("PMS_GUEST_ACCOUNT_CLOSED", "The bill is closed"));
    }
    Ok((record, account))
}

fn no_refund(account: &AccountFacts, refunds: &[PmsRecord]) -> Result<(), BusinessError> {
    if account.refund_reserved > 0 || account.deposit_refund_reserved > 0 {
        return Err(conflict(
            "PMS_REFUND_PENDING",
            "Complete pending refunds first",
        ));
    }
    for refund in refunds {
        let data = refund.data.as_object().ok_or_else(record_invalid)?;
        if refund.kind != "refund" {
            return Err(record_invalid());
        }
        let state = text(data, "state")?;
        if !matches!(
            state.as_str(),
            "requested" | "approved" | "rejected" | "paid"
        ) {
            return Err(record_invalid());
        }
        if matches!(state.as_str(), "requested" | "approved") {
            return Err(conflict(
                "PMS_REFUND_PENDING",
                "Complete pending refunds first",
            ));
        }
    }
    Ok(())
}

fn require_invoice_free(account: &AccountFacts) -> Result<(), BusinessError> {
    if !account.invoice_id.is_empty() {
        return Err(conflict(
            "PMS_INVOICE_ISSUED",
            "An invoice has been issued. Add new items to a new bill",
        ));
    }
    Ok(())
}

fn require_no_advance(account: &AccountFacts) -> Result<(), BusinessError> {
    if account.advance_available() > 0 {
        return Err(payment_port_required());
    }
    Ok(())
}

fn touch(record: &mut PmsRecord, now: i64) {
    record.version += 1;
    record.updated_at = now;
}

/// Deterministic identity for a record this family creates. The retained
/// engine mints a UUID; deriving it from the retained command key keeps one key
/// on one record.
fn created_record_id(
    prefix: &'static str,
    key: &CommandKey,
    property_id: &str,
    scope: &str,
) -> String {
    let digest = Sha256::digest(
        format!("pms-{prefix}-create:{}:{property_id}:{scope}", key.as_str()).as_bytes(),
    );
    format!("pms-{prefix}-{}", hex::encode(&digest[..16]))
}

fn account_result(
    id: String,
    account_id: String,
    property_id: &str,
    version: i64,
    facts: &AccountFacts,
) -> PmsGuestAccountResult {
    PmsGuestAccountResult {
        id,
        account_id,
        property_id: property_id.to_owned(),
        version,
        state: facts.state.clone(),
        due: facts.due,
        advance: facts.advance,
        deposit: facts.deposit,
    }
}

fn due_after(due: i64, total: i64) -> Result<i64, BusinessError> {
    due.checked_add(total).ok_or_else(record_invalid)
}

fn require_money(value: i64) -> Result<i64, BusinessError> {
    if !valid_money(value) {
        return Err(record_invalid());
    }
    Ok(value)
}

#[allow(clippy::too_many_arguments)]
fn entry_record(
    id: String,
    account_id: &str,
    business_date: &str,
    kind: &'static str,
    description: &str,
    totals: (i64, i64, i64, i64),
    stay_id: &str,
    source_key: &str,
    original_id: &str,
    actor_id: &str,
    now: i64,
) -> PmsRecord {
    let (net, service, tax, total) = totals;
    PmsRecord {
        id,
        kind: "entry".to_owned(),
        version: 1,
        updated_at: now,
        data: serde_json::json!({
            "guestAccountId": account_id,
            "stayId": stay_id,
            "date": business_date,
            "kind": kind,
            "description": description,
            "net": net,
            "service": service,
            "tax": tax,
            "total": total,
            "method": "",
            "reference": "",
            "sourceKey": source_key,
            "originalId": original_id,
            "shiftId": "",
            "actorId": actor_id,
            "at": now,
        }),
    }
}

/// Plan one retained guest-account/charge command. The property row is already
/// locked and `command.expected_version` must match it. Every stored fact is
/// validated before any row is written, so malformed owner data fails closed
/// instead of inventing a balance.
pub fn plan_guest_account(
    actor: &PmsGuestAccountActor,
    key: &CommandKey,
    facts: &PmsGuestAccountFacts,
    command: &PmsGuestAccountCommand,
    now: i64,
) -> Result<PmsGuestAccountPlan, BusinessError> {
    let mut command = command.clone();
    command.normalize();
    command.validate()?;
    let command = &command;
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
        "closing" => {
            return Err(conflict(
                "PMS_DAY_CLOSING",
                "Day close is in progress. Please wait for it to finish",
            ));
        }
        "setup" => {
            return Err(conflict(
                "PMS_SETUP_REQUIRED",
                "Review readiness and open the hotel first",
            ));
        }
        _ => return Err(record_invalid()),
    }
    if facts.account_refunds.len() > MAX_REFUNDS || facts.destination_refunds.len() > MAX_REFUNDS {
        return Err(read_limit(
            "The guest account has more refund facts than this command can read.",
        ));
    }
    let property_version = facts.property.version + 1;
    let mut plan = PmsGuestAccountPlan {
        result: PmsGuestAccountResult {
            id: String::new(),
            account_id: String::new(),
            property_id: facts.property.id.clone(),
            version: property_version,
            state: "open".to_owned(),
            due: 0,
            advance: 0,
            deposit: 0,
        },
        account: None,
        new_account: None,
        destination: None,
        new_entry: None,
        new_destination_entry: None,
        source_key: None,
        property_version,
    };
    match &command.input {
        PmsGuestAccountInput::Create(input) => plan_create(key, facts, input, now, &mut plan)?,
        PmsGuestAccountInput::Update(input) => plan_update(facts, input, now, &mut plan)?,
        PmsGuestAccountInput::ChargeAdd(input) => {
            let price = input.price;
            plan_charge(
                key,
                facts,
                input.account_id.clone(),
                input.stay_id.clone(),
                input.source_key.clone(),
                input.description.clone(),
                ChargeAmounts::Price(price),
                &actor.board.user_id,
                now,
                &mut plan,
            )?
        }
        PmsGuestAccountInput::ChargeImport(input) => plan_charge(
            key,
            facts,
            input.account_id.clone(),
            input.stay_id.clone(),
            input.source_key.clone(),
            input.description.clone(),
            ChargeAmounts::Import {
                net: input.net,
                service: input.service,
                tax: input.tax,
                total: input.total,
            },
            &actor.board.user_id,
            now,
            &mut plan,
        )?,
        PmsGuestAccountInput::Adjustment(input) => {
            plan_adjustment(key, facts, input, &actor.board.user_id, now, &mut plan)?
        }
        PmsGuestAccountInput::Transfer(input) => {
            plan_transfer(key, facts, input, &actor.board.user_id, now, &mut plan)?
        }
    }
    if plan.result.id.is_empty() || plan.result.account_id.is_empty() {
        return Err(record_invalid());
    }
    Ok(plan)
}

fn plan_create(
    key: &CommandKey,
    facts: &PmsGuestAccountFacts,
    input: &PmsGuestAccountOpenInput,
    now: i64,
    plan: &mut PmsGuestAccountPlan,
) -> Result<(), BusinessError> {
    let guest = facts
        .guest
        .as_ref()
        .filter(|record| record.kind == "guest" && record.id == input.guest_id)
        .ok_or_else(record_not_found)?;
    let data = guest.data.as_object().ok_or_else(record_invalid)?;
    if !valid_text(&text(data, "name")?) {
        return Err(record_invalid());
    }
    if !input.reservation_id.is_empty() {
        let reservation = facts
            .reservation
            .as_ref()
            .filter(|record| record.kind == "reservation" && record.id == input.reservation_id)
            .ok_or_else(record_not_found)?;
        let data = reservation.data.as_object().ok_or_else(record_invalid)?;
        if text(data, "guestId")? != input.guest_id {
            return Err(conflict(
                "PMS_GUEST_LINKAGE",
                "The reservation belongs to another guest",
            ));
        }
    }
    let id = created_record_id("guest-account", key, &facts.property.id, "");
    let account = PmsRecord {
        id: id.clone(),
        kind: "guest_account".to_owned(),
        version: 1,
        updated_at: now,
        data: serde_json::json!({
            "guestId": input.guest_id,
            "reservationId": input.reservation_id,
            "name": input.name,
            "partyId": "",
            "state": "open",
            "due": 0,
            "advance": 0,
            "advanceTax": 0,
            "deposit": 0,
        }),
    };
    let created = AccountFacts::read(&account)?;
    let result = account_result(
        id.clone(),
        id,
        &plan.result.property_id,
        plan.result.version,
        &created,
    );
    plan.new_account = Some(account);
    plan.result = result;
    Ok(())
}

fn plan_update(
    facts: &PmsGuestAccountFacts,
    input: &PmsGuestAccountUpdateInput,
    now: i64,
    plan: &mut PmsGuestAccountPlan,
) -> Result<(), BusinessError> {
    let (mut record, _) = open_account(facts)?;
    if record.id != input.account_id {
        return Err(record_not_found());
    }
    record.data["name"] = serde_json::Value::String(input.name.clone());
    let updated = AccountFacts::read(&record)?;
    touch(&mut record, now);
    let result = account_result(
        record.id.clone(),
        record.id.clone(),
        &plan.result.property_id,
        plan.result.version,
        &updated,
    );
    plan.result = result;
    plan.account = Some(record);
    Ok(())
}

enum ChargeAmounts {
    Price(i64),
    Import {
        net: i64,
        service: i64,
        tax: i64,
        total: i64,
    },
}

#[allow(clippy::too_many_arguments)]
fn plan_charge(
    key: &CommandKey,
    facts: &PmsGuestAccountFacts,
    account_id: String,
    stay_id: String,
    source_key: String,
    description: String,
    amounts: ChargeAmounts,
    user_id: &str,
    now: i64,
    plan: &mut PmsGuestAccountPlan,
) -> Result<(), BusinessError> {
    let (mut record, account) = open_account(facts)?;
    if record.id != account_id {
        return Err(record_not_found());
    }
    no_refund(&account, &facts.account_refunds)?;
    if let Some(prior) = &facts.source_entry {
        if prior.kind != "entry" {
            return Err(record_invalid());
        }
        let result = account_result(
            prior.id.clone(),
            account_id,
            &plan.result.property_id,
            plan.result.version,
            &account,
        );
        plan.result = result;
        return Ok(());
    }
    require_invoice_free(&account)?;
    require_no_advance(&account)?;
    require_approved_chart(&facts.property)?;
    let totals = match amounts {
        ChargeAmounts::Price(price) => {
            let config = property_config(&facts.property)?;
            price_amounts(
                price,
                config.tax_bps,
                config.service_bps,
                config.tax_inclusive,
            )?
        }
        ChargeAmounts::Import {
            net,
            service,
            tax,
            total,
        } => (net, service, tax, total),
    };
    let due = require_money(due_after(account.due, totals.3)?)?;
    let entry_id = created_record_id("entry", key, &facts.property.id, &account_id);
    record.data["due"] = serde_json::Value::from(due);
    touch(&mut record, now);
    let mut updated = account;
    updated.due = due;
    let result = account_result(
        entry_id.clone(),
        record.id.clone(),
        &plan.result.property_id,
        plan.result.version,
        &updated,
    );
    plan.result = result;
    plan.account = Some(record);
    plan.new_entry = Some(entry_record(
        entry_id,
        &account_id,
        &facts.property.business_date,
        "charge",
        &description,
        totals,
        &stay_id,
        &source_key,
        "",
        user_id,
        now,
    ));
    if !source_key.is_empty() {
        plan.source_key = Some(source_key);
    }
    Ok(())
}

fn plan_adjustment(
    key: &CommandKey,
    facts: &PmsGuestAccountFacts,
    input: &PmsAdjustmentInput,
    user_id: &str,
    now: i64,
    plan: &mut PmsGuestAccountPlan,
) -> Result<(), BusinessError> {
    let (mut record, account) = open_account(facts)?;
    if record.id != input.account_id {
        return Err(record_not_found());
    }
    no_refund(&account, &facts.account_refunds)?;
    if let Some(prior) = &facts.source_entry {
        if prior.kind != "entry" {
            return Err(record_invalid());
        }
        let result = account_result(
            prior.id.clone(),
            record.id.clone(),
            &plan.result.property_id,
            plan.result.version,
            &account,
        );
        plan.result = result;
        return Ok(());
    }
    require_invoice_free(&account)?;
    require_approved_chart(&facts.property)?;
    let totals = (input.net, input.service, input.tax, input.total);
    if !input.original_entry_id.is_empty() {
        let original = facts
            .original_entry
            .as_ref()
            .filter(|entry| entry.kind == "entry" && entry.id == input.original_entry_id)
            .ok_or_else(record_not_found)?;
        let original = EntryFacts::read(original)?;
        if original.account_id != record.id
            || !matches!(original.kind.as_str(), "charge" | "transfer")
            || original.total <= 0
            || original.method == "opening"
        {
            return Err(conflict(
                "PMS_ALREADY_REVERSED",
                "This record cannot be reversed or has already been reversed",
            ));
        }
        if !facts.original_references.is_empty() {
            return Err(conflict(
                "PMS_ALREADY_REVERSED",
                "This record cannot be reversed or has already been reversed",
            ));
        }
        if totals
            != (
                -original.net,
                -original.service,
                -original.tax,
                -original.total,
            )
        {
            return Err(conflict(
                "PMS_REVERSAL_TOTAL",
                "Use the exact negation of the source entry",
            ));
        }
    } else if totals.3 > 0 {
        require_no_advance(&account)?;
    }
    let due = due_after(account.due, totals.3)?;
    if due < 0 {
        return Err(credit_requires_finance());
    }
    let due = require_money(due)?;
    let entry_id = created_record_id("entry", key, &facts.property.id, &record.id);
    record.data["due"] = serde_json::Value::from(due);
    touch(&mut record, now);
    let mut updated = account;
    updated.due = due;
    let result = account_result(
        entry_id.clone(),
        record.id.clone(),
        &plan.result.property_id,
        plan.result.version,
        &updated,
    );
    plan.result = result;
    plan.account = Some(record);
    plan.new_entry = Some(entry_record(
        entry_id,
        &input.account_id,
        &facts.property.business_date,
        "adjustment",
        &input.description,
        totals,
        "",
        &input.source_key,
        &input.original_entry_id,
        user_id,
        now,
    ));
    if !input.source_key.is_empty() {
        plan.source_key = Some(input.source_key.clone());
    }
    Ok(())
}

fn plan_transfer(
    key: &CommandKey,
    facts: &PmsGuestAccountFacts,
    input: &PmsTransferInput,
    user_id: &str,
    now: i64,
    plan: &mut PmsGuestAccountPlan,
) -> Result<(), BusinessError> {
    let original_record = facts
        .original_entry
        .as_ref()
        .filter(|entry| entry.kind == "entry" && entry.id == input.entry_id)
        .ok_or_else(record_not_found)?;
    let original = EntryFacts::read(original_record)?;
    let (mut from_record, from) = open_account(facts)?;
    if from_record.id != original.account_id {
        return Err(record_not_found());
    }
    let destination_record = facts.destination.clone().ok_or_else(record_not_found)?;
    let destination = AccountFacts::read(&destination_record)?;
    if !destination.is_open() {
        return Err(conflict("PMS_GUEST_ACCOUNT_CLOSED", "The bill is closed"));
    }
    no_refund(&from, &facts.account_refunds)?;
    no_refund(&destination, &facts.destination_refunds)?;
    if from_record.id == destination_record.id || destination_record.id != input.to_account_id {
        return Err(conflict(
            "PMS_TRANSFER_INVALID",
            "Select an expense item and a different destination bill",
        ));
    }
    if !matches!(original.kind.as_str(), "charge" | "transfer")
        || original.total <= 0
        || original.method == "opening"
    {
        return Err(conflict(
            "PMS_TRANSFER_INVALID",
            "Select an expense item and a different destination bill",
        ));
    }
    if !facts.original_references.is_empty() {
        return Err(conflict(
            "PMS_ENTRY_REFERENCED",
            "This record has already been changed",
        ));
    }
    require_invoice_free(&from)?;
    require_invoice_free(&destination)?;
    if from.due < original.total {
        return Err(conflict(
            "PMS_PAID_CHARGE",
            "Only unpaid expense amounts can be transferred",
        ));
    }
    require_no_advance(&destination)?;
    let from_due = require_money(due_after(from.due, -original.total)?)?;
    let to_due = require_money(due_after(destination.due, original.total)?)?;
    let from_entry_id = created_record_id("entry", key, &facts.property.id, &from_record.id);
    let to_entry_id = created_record_id(
        "entry",
        key,
        &facts.property.id,
        &format!("to:{}", destination_record.id),
    );
    if from_entry_id == to_entry_id {
        return Err(record_invalid());
    }
    from_record.data["due"] = serde_json::Value::from(from_due);
    touch(&mut from_record, now);
    let mut destination_updated = destination_record.clone();
    destination_updated.data["due"] = serde_json::Value::from(to_due);
    touch(&mut destination_updated, now);
    let mut from_after = from;
    from_after.due = from_due;
    let result = account_result(
        from_entry_id.clone(),
        from_record.id.clone(),
        &plan.result.property_id,
        plan.result.version,
        &from_after,
    );
    plan.result = result;
    plan.account = Some(from_record.clone());
    plan.destination = Some(destination_updated);
    plan.new_entry = Some(entry_record(
        from_entry_id.clone(),
        &from_record.id,
        &facts.property.business_date,
        "transfer",
        &input.reason,
        (
            -original.net,
            -original.service,
            -original.tax,
            -original.total,
        ),
        "",
        "",
        &original_record.id,
        user_id,
        now,
    ));
    plan.new_destination_entry = Some(entry_record(
        to_entry_id,
        &destination_record.id,
        &facts.property.business_date,
        "transfer",
        &input.reason,
        (original.net, original.service, original.tax, original.total),
        "",
        "",
        &from_entry_id,
        user_id,
        now,
    ));
    Ok(())
}
