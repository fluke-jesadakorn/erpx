//! Native PMS public-booking access link, private mail and rate-limit facts.
//!
//! This slice owns the retained website booking *delivery* boundary: the
//! private management link (issue, verify, hash), the encrypted mail payload,
//! the booking summary rendered into that payload, the bounded delivery retry
//! policy and the public request bucket. It never reads an Account session, a
//! provider credential, a Finance balance or a Commercial package row.
//!
//! Public quote pricing, payment settlement/allocation, refunds and the
//! Stripe transaction remain with their missing native producers; nothing here
//! invents a receipt, a posting or a provider result.
use super::room_board::{PmsPropertyRow, PmsRecord, ecma_trim};
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Retained private link lifetime (`now + 7 * 86400000`).
pub const TOKEN_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// Retained `authorizedBooking` bound on the presented token.
pub const MAX_TOKEN_LEN: usize = 200;
/// Retained `attempts >= 8` terminal delivery policy.
pub const MAX_MAIL_ATTEMPTS: i64 = 8;
/// Retained delivery lease (`now + 120000`).
pub const MAIL_LEASE_MS: i64 = 120_000;
/// Retained package-blocked retry delay.
pub const MAIL_BLOCK_RETRY_MS: i64 = 60_000;
/// Retained exponential retry ceiling (`min(3600000, ...)`).
pub const MAIL_RETRY_CAP_MS: i64 = 3_600_000;
/// Retained drain turn bound (`for i < 10`).
pub const DELIVERY_BATCH: usize = 10;
/// Retained public rate-limit window.
pub const RATE_WINDOW_MS: i64 = 60_000;
/// Retained default requests per window.
pub const DEFAULT_RATE_LIMIT: i64 = 30;
/// Retained stale bucket retention (`Math.floor(now/60000) - 60`).
pub const RATE_LIMIT_CLEANUP_BUCKETS: i64 = 60;
/// Bounded booking summary room list.
pub const MAX_SUMMARY_ROOMS: usize = 50;
/// Bounded guest-account list for the summary.
pub const MAX_SUMMARY_ACCOUNTS: usize = 500;
/// Bounded folio entry list for the summary.
pub const MAX_SUMMARY_ENTRIES: usize = 4_000;

/// The two retained signed-link audiences. The prefix is part of the signed
/// value (`access:{id}` / `payment:{id}`), so a payment link can never be
/// presented as a management link.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmsBookingLinkKind {
    Access,
    Payment,
}

impl PmsBookingLinkKind {
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Access => "access",
            Self::Payment => "payment",
        }
    }
}

/// Retained `pms_booking_emails.kind` values (see migration 1016).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmsBookingMailKind {
    Confirmation,
    Cancellation,
    Access,
    Change,
    Refund,
}

impl PmsBookingMailKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "confirmation" => Some(Self::Confirmation),
            "cancellation" => Some(Self::Cancellation),
            "access" => Some(Self::Access),
            "change" => Some(Self::Change),
            "refund" => Some(Self::Refund),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confirmation => "confirmation",
            Self::Cancellation => "cancellation",
            Self::Access => "access",
            Self::Change => "change",
            Self::Refund => "refund",
        }
    }

    /// Retained mail subject label.
    pub fn subject_label(self) -> &'static str {
        match self {
            Self::Confirmation => "Booking confirmation",
            Self::Cancellation => "Cancellation",
            Self::Access => "Booking access",
            Self::Change => "Booking updated",
            Self::Refund => "Refund confirmed",
        }
    }
}

pub fn link_expired() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unauthenticated,
        "PMS_LINK_EXPIRED",
        "Request a new booking access link",
    )
}

pub fn record_invalid() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "PMS_BOOKING_RECORD_INVALID",
        "The booking mail facts could not be read safely.",
    )
}

pub fn read_limit(message: &'static str) -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "PMS_BOOKING_READ_LIMIT",
        message,
    )
}

pub fn rate_limited() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        "PMS_RATE_LIMIT",
        "Too many requests. Please wait a minute.",
    )
}

// ---------------------------------------------------------------------------
// Base64url (RFC 4648 §5, no padding) and HMAC-SHA256.
//
// The retained TypeScript signs with `createHmac("sha256", secret)`, base64url
// encoding and a `pms-booking:` domain prefix. `sha2` is already an owner
// dependency; the small encoders below keep the package dependency set and
// lockfile unchanged.
// ---------------------------------------------------------------------------

const BASE64URL: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64url_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        out.push(BASE64URL[(first >> 2) as usize] as char);
        out.push(BASE64URL[(((first & 0b11) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(BASE64URL[(((second & 0b1111) << 2) | (third >> 6)) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(BASE64URL[(third & 0b11_1111) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
fn base64url_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
fn base64url_decode(value: &str) -> Option<Vec<u8>> {
    if value.is_empty() {
        return Some(Vec::new());
    }
    if value.len() % 4 == 1 || !value.is_ascii() {
        return None;
    }
    let mut out = Vec::with_capacity(value.len() / 4 * 3 + 2);
    let bytes = value.as_bytes();
    for chunk in bytes.chunks(4) {
        let mut buffer = 0_u32;
        for byte in chunk {
            buffer = (buffer << 6) | u32::from(base64url_value(*byte)?);
        }
        let padding = 4 - chunk.len();
        buffer <<= 6 * padding as u32;
        out.push((buffer >> 16) as u8);
        if chunk.len() > 2 {
            out.push((buffer >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(buffer as u8);
        }
    }
    Some(out)
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut normalized = [0_u8; BLOCK];
    if key.len() > BLOCK {
        normalized[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        normalized[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    for byte in normalized {
        inner.update([byte ^ 0x36]);
    }
    inner.update(message);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    for byte in normalized {
        outer.update([byte ^ 0x5c]);
    }
    outer.update(inner);
    outer.finalize().into()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (left, right) in left.iter().zip(right.iter()) {
        difference |= left ^ right;
    }
    difference == 0
}

/// Retained `sign(secret, value)` = `base64url(HMAC-SHA256(secret,
/// "pms-booking:" + value))`, no padding.
pub fn sign_booking_value(secret: &str, value: &str) -> String {
    base64url_encode(&hmac_sha256(
        secret.as_bytes(),
        format!("pms-booking:{value}").as_bytes(),
    ))
}

/// Retained `tokenValue`: `{id}.{sign(secret, "access:{id}")}`.
pub fn booking_link_token(secret: &str, kind: PmsBookingLinkKind, id: &str) -> String {
    format!(
        "{id}.{}",
        sign_booking_value(secret, &format!("{}:{id}", kind.prefix()))
    )
}

/// Split a presented link into id and signature. Bounds and character class are
/// enforced here so an oversized or structured token never reaches the store.
pub fn split_booking_link(kind: PmsBookingLinkKind, token: &str) -> Result<(&str, &str), BusinessError> {
    if token.is_empty() || token.len() > MAX_TOKEN_LEN || token.chars().any(char::is_control) {
        return Err(link_expired());
    }
    let mut pieces = token.split('.');
    let (Some(id), Some(signature)) = (pieces.next(), pieces.next()) else {
        return Err(link_expired());
    };
    if pieces.next().is_some() || id.is_empty() || signature.is_empty() {
        return Err(link_expired());
    }
    Ok((id, signature))
}

/// Verify the retained signature for one audience and return the link id.
pub fn verify_booking_link(
    secret: &str,
    kind: PmsBookingLinkKind,
    token: &str,
) -> Result<String, BusinessError> {
    let (id, signature) = split_booking_link(kind, token)?;
    let expected = sign_booking_value(secret, &format!("{}:{id}", kind.prefix()));
    if !constant_time_eq(signature.as_bytes(), expected.as_bytes()) {
        return Err(link_expired());
    }
    Ok(id.to_owned())
}

/// Retained `hash(token)` = hex SHA-256. Only the digest is stored.
pub fn booking_token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// `encodeURIComponent`, restricted to the retained URL suffixes.
pub fn encode_uri_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*'
            | b'\'' | b'(' | b')' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Retained `bookingPublicUrl`: a registered Holding origin wins over the
/// application origin.
pub fn booking_public_url(
    origin: &str,
    holding_origin: Option<&str>,
    slug: &str,
    suffix: &str,
) -> String {
    match holding_origin {
        Some(holding) => format!("{holding}/book{suffix}"),
        None => format!("{origin}/book/{}{suffix}", encode_uri_component(slug)),
    }
}

// ---------------------------------------------------------------------------
// Delivery policy.
// ---------------------------------------------------------------------------

/// Retained delivery outcome for one claimed mail.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PmsBookingDeliveryOutcome {
    /// Provider accepted the payload.
    Sent,
    /// Retryable failure: `state` is `pending` until the attempt budget is
    /// exhausted, then `failed`.
    Retry { state: &'static str, next_attempt_at: i64 },
    /// Commercial package refusal: return the mail to `pending` without
    /// consuming the attempt and retry after one minute.
    Blocked { code: String, next_attempt_at: i64 },
}

/// Retained package/enforcement codes that pause rather than consume delivery
/// attempts. An unknown code never blocks.
pub const BLOCKED_MAIL_CODES: [&str; 7] = [
    "PACKAGE_REQUIRED",
    "PACKAGE_INACTIVE",
    "PACKAGE_DRAINING",
    "DEPENDENCY_REQUIRED",
    "PACKAGE_CAPABILITY_CHANGED",
    "PACKAGE_ENFORCEMENT_ENVIRONMENT",
    "PACKAGE_ENFORCEMENT_CONFIGURATION",
];

pub fn blocked_mail_code(code: &str) -> bool {
    !code.is_empty() && BLOCKED_MAIL_CODES.contains(&code)
}

/// Retained `attempts >= 8 ? 'failed' : 'pending'` and
/// `now + min(3600000, 1000 * 2 ** attempts)`.
pub fn delivery_retry_outcome(attempts: i64, now: i64) -> PmsBookingDeliveryOutcome {
    let attempt = attempts.clamp(0, 31);
    let backoff = 1000_i64
        .checked_shl(attempt as u32)
        .unwrap_or(MAIL_RETRY_CAP_MS)
        .min(MAIL_RETRY_CAP_MS);
    PmsBookingDeliveryOutcome::Retry {
        state: if attempts >= MAX_MAIL_ATTEMPTS {
            "failed"
        } else {
            "pending"
        },
        next_attempt_at: now.saturating_add(backoff),
    }
}

/// Classify one failed delivery attempt. A package refusal pauses the mail;
/// every other failure consumes the attempt budget.
pub fn delivery_failure_outcome(
    attempts: i64,
    code: Option<&str>,
    now: i64,
) -> PmsBookingDeliveryOutcome {
    match code.filter(|code| blocked_mail_code(code)) {
        Some(code) => PmsBookingDeliveryOutcome::Blocked {
            code: code.to_owned(),
            next_attempt_at: now.saturating_add(MAIL_BLOCK_RETRY_MS),
        },
        None => delivery_retry_outcome(attempts, now),
    }
}

/// Retained delivery lease deadline for a claimed mail.
pub fn mail_lease_deadline(now: i64) -> i64 {
    now.saturating_add(MAIL_LEASE_MS)
}

/// Retained stale-bucket cutoff.
pub fn stale_rate_limit_bucket(now: i64) -> i64 {
    now.div_euclid(RATE_WINDOW_MS) - RATE_LIMIT_CLEANUP_BUCKETS
}

pub fn rate_limit_bucket(now: i64) -> i64 {
    now.div_euclid(RATE_WINDOW_MS)
}

/// Retained `count > limit` refusal (the increment is returned, so a request
/// that reaches exactly the limit still passes).
pub fn rate_limit_exceeded(count: i64, limit: i64) -> bool {
    count > limit
}

// ---------------------------------------------------------------------------
// Booking summary rendered into the private mail payload.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct PmsBookingMailFacts {
    pub property: PmsPropertyRow,
    pub reservation: PmsRecord,
    pub guest: PmsRecord,
    pub room_types: Vec<PmsRecord>,
    pub stays: Vec<PmsRecord>,
    pub accounts: Vec<PmsRecord>,
    pub entries: Vec<PmsRecord>,
    /// Retained `records(s, 'refund')` guard for cancellation eligibility.
    pub refunds: Vec<PmsRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsBookingSummaryRoom {
    pub id: String,
    #[serde(rename = "typeId")]
    pub type_id: String,
    #[serde(rename = "rateId")]
    pub rate_id: String,
    #[serde(rename = "guestAccountId")]
    pub guest_account_id: String,
    #[serde(rename = "roomName")]
    pub room_name: String,
    pub arrival: String,
    pub departure: String,
    pub adults: i64,
    #[serde(rename = "childAges")]
    pub child_ages: Vec<i64>,
    pub state: String,
    pub nights: Value,
    #[serde(rename = "policy")]
    pub policy: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsBookingCancellation {
    pub allowed: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsBookingHotel {
    pub id: String,
    pub name: String,
    pub address: String,
    #[serde(rename = "contactEmail")]
    pub contact_email: String,
    #[serde(rename = "contactPhone")]
    pub contact_phone: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsBookingGuest {
    pub name: String,
    pub email: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsBookingSummary {
    #[serde(rename = "postedRoomTotal")]
    pub posted_room_total: i64,
    pub id: String,
    pub number: String,
    pub state: String,
    pub hotel: PmsBookingHotel,
    pub guest: PmsBookingGuest,
    #[serde(rename = "paymentState")]
    pub payment_state: String,
    pub currency: String,
    pub version: i64,
    pub history: Value,
    pub total: i64,
    pub advance: i64,
    #[serde(rename = "guestAccountDue")]
    pub guest_account_due: i64,
    pub policy: Value,
    pub rooms: Vec<PmsBookingSummaryRoom>,
    pub cancellation: PmsBookingCancellation,
}

fn object(data: &Value) -> Option<&serde_json::Map<String, Value>> {
    data.as_object()
}

fn text(data: &Value, key: &str) -> Option<String> {
    data.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn integer(data: &Value, key: &str) -> Option<i64> {
    data.get(key).and_then(Value::as_i64)
}

fn money(value: Option<&Value>) -> Result<i64, BusinessError> {
    match value {
        None | Some(Value::Null) => Ok(0),
        Some(value) => value.as_i64().ok_or_else(record_invalid),
    }
}

fn money_field(data: &Value, key: &str) -> Result<i64, BusinessError> {
    money(data.get(key))
}

fn record_state(record: &PmsRecord) -> Option<String> {
    text(&record.data, "state")
}

fn reservation_id_of(record: &PmsRecord) -> Option<&str> {
    record
        .data
        .get("reservationId")
        .and_then(Value::as_str)
}

fn source_key_of(record: &PmsRecord) -> Option<&str> {
    record.data.get("sourceKey").and_then(Value::as_str)
}

fn booking_config(config: &Value) -> Option<&Value> {
    config.get("booking")
}

/// Retained `cancellationEligibility` guard.
pub fn cancellation_eligibility(
    facts: &PmsBookingMailFacts,
    now: i64,
) -> PmsBookingCancellation {
    let reservation = &facts.reservation;
    let denied = |reason: &str| PmsBookingCancellation {
        allowed: false,
        reason: reason.to_owned(),
    };
    if record_state(reservation).as_deref() == Some("cancelled") {
        return denied("This booking is already cancelled");
    }
    if facts.property.state != "open" {
        return denied("Hotel day close is in progress. Please try again shortly");
    }
    let reservation_policy = reservation.data.get("bookingPolicy");
    let deadline = reservation_policy
        .and_then(|policy| policy.get("deadline"))
        .and_then(Value::as_i64);
    let deadline_passed = facts.stays.iter().any(|stay| match stay.data.get("bookingPolicy") {
        Some(policy) => {
            policy
                .get("deadline")
                .and_then(Value::as_i64)
                .is_some_and(|deadline| now >= deadline)
                || policy
                    .get("collection")
                    .and_then(|collection| collection.get("cancellationsAllowed"))
                    .and_then(Value::as_bool)
                    == Some(false)
        }
        None => deadline.is_some_and(|deadline| now >= deadline),
    });
    if reservation_policy.is_none() || deadline_passed {
        return denied("The free-cancellation deadline has passed. Contact the hotel");
    }
    if record_state(reservation).as_deref() != Some("confirmed")
        || facts.stays.is_empty()
        || facts
            .stays
            .iter()
            .any(|stay| record_state(stay).as_deref() != Some("reserved"))
    {
        return denied("Contact the hotel to change this booking");
    }
    let accounts: std::collections::BTreeSet<&str> = facts
        .accounts
        .iter()
        .filter(|account| reservation_id_of(account) == Some(reservation.id.as_str()))
        .map(|account| account.id.as_str())
        .collect();
    let financial_activity = facts.entries.iter().any(|entry| {
        entry
            .data
            .get("guestAccountId")
            .and_then(Value::as_str)
            .is_some_and(|account| accounts.contains(account))
            && !(text(&entry.data, "kind").as_deref() == Some("payment")
                && text(&entry.data, "method").as_deref() == Some("stripe"))
    });
    let financial_activity = financial_activity
        || facts.refunds.iter().any(|refund| {
            refund
                .data
                .get("guestAccountId")
                .and_then(Value::as_str)
                .is_some_and(|account| accounts.contains(account))
        });
    if financial_activity {
        return denied("This booking has financial activity. Contact the hotel for cancellation");
    }
    let fee = facts.stays.iter().any(|stay| {
        stay.data
            .get("bookingPolicy")
            .and_then(|policy| policy.get("collection"))
            .and_then(|collection| collection.get("cancellationFeePercent"))
            .and_then(Value::as_i64)
            .is_some_and(|fee| fee > 0)
    });
    PmsBookingCancellation {
        allowed: true,
        reason: if fee {
            "Review cancellation fees and refund before confirming".to_owned()
        } else {
            "Cancel all rooms without a fee".to_owned()
        },
    }
}

/// Build the retained `bookingSummary` projection used by the private mail and
/// management link. Every numeric field is an exact minor-unit integer; a
/// fractional or missing value fails closed instead of inventing money.
pub fn booking_summary(
    facts: &PmsBookingMailFacts,
    now: i64,
) -> Result<PmsBookingSummary, BusinessError> {
    object(&facts.reservation.data).ok_or_else(record_invalid)?;
    object(&facts.guest.data).ok_or_else(record_invalid)?;
    if facts.stays.len() > MAX_SUMMARY_ROOMS
        || facts.accounts.len() > MAX_SUMMARY_ACCOUNTS
        || facts.entries.len() > MAX_SUMMARY_ENTRIES
    {
        return Err(read_limit(
            "The booking has more folio facts than this mail can read.",
        ));
    }
    let config: Value =
        serde_json::from_str(&facts.property.config_json).map_err(|_| record_invalid())?;
    let booking = booking_config(&config).cloned().unwrap_or(Value::Null);
    let guest_id = text(&facts.reservation.data, "guestId").ok_or_else(record_invalid)?;
    if facts.guest.id != guest_id {
        return Err(record_invalid());
    }
    let stays: Vec<&PmsRecord> = facts
        .stays
        .iter()
        .filter(|stay| reservation_id_of(stay) == Some(facts.reservation.id.as_str()))
        .collect();
    let accounts: Vec<&PmsRecord> = facts
        .accounts
        .iter()
        .filter(|account| reservation_id_of(account) == Some(facts.reservation.id.as_str()))
        .collect();
    let account_ids: std::collections::BTreeSet<&str> =
        accounts.iter().map(|account| account.id.as_str()).collect();
    let entries: Vec<&PmsRecord> = facts
        .entries
        .iter()
        .filter(|entry| {
            entry
                .data
                .get("guestAccountId")
                .and_then(Value::as_str)
                .is_some_and(|account| account_ids.contains(account))
        })
        .collect();
    let by_id: std::collections::BTreeMap<&str, &PmsRecord> = facts
        .entries
        .iter()
        .map(|entry| (entry.id.as_str(), entry))
        .collect();
    let mut posted_room_total = 0_i64;
    for entry in &entries {
        let mut current: Option<&PmsRecord> = Some(entry);
        let mut seen = std::collections::BTreeSet::new();
        while let Some(node) = current {
            if !seen.insert(node.id.as_str()) {
                break;
            }
            if source_key_of(node).is_some_and(|key| key.starts_with("night:")) {
                posted_room_total = posted_room_total.saturating_add(money_field(&entry.data, "total")?);
                break;
            }
            current = node
                .data
                .get("originalId")
                .and_then(Value::as_str)
                .and_then(|id| by_id.get(id).copied());
        }
    }
    let mut total = 0_i64;
    let mut rooms = Vec::with_capacity(stays.len());
    for stay in &stays {
        if matches!(
            record_state(stay).as_deref(),
            Some("cancelled" | "no_show")
        ) {
            continue;
        }
        let prices = stay
            .data
            .get("prices")
            .and_then(Value::as_array)
            .ok_or_else(record_invalid)?;
        for price in prices {
            total = total.saturating_add(money_field(price, "total")?);
        }
    }
    // The retained room list is not filtered by terminal state, only `total` is.
    for stay in &stays {
        let type_id = text(&stay.data, "typeId").ok_or_else(record_invalid)?;
        let room_name = facts
            .room_types
            .iter()
            .find(|room_type| room_type.id == type_id)
            .and_then(|room_type| text(&room_type.data, "name"))
            .ok_or_else(record_invalid)?;
        let child_ages = match stay.data.get("childAges") {
            Some(Value::Array(values)) => values
                .iter()
                .map(|value| value.as_i64().ok_or_else(record_invalid))
                .collect::<Result<Vec<_>, _>>()?,
            Some(Value::Null) | None => Vec::new(),
            Some(_) => return Err(record_invalid()),
        };
        let policy = stay
            .data
            .get("bookingPolicy")
            .cloned()
            .or_else(|| facts.reservation.data.get("bookingPolicy").cloned())
            .unwrap_or(Value::Null);
        rooms.push(PmsBookingSummaryRoom {
            id: stay.id.clone(),
            type_id,
            rate_id: text(&stay.data, "rateId").ok_or_else(record_invalid)?,
            guest_account_id: text(&stay.data, "guestAccountId").ok_or_else(record_invalid)?,
            room_name,
            arrival: text(&stay.data, "arrival").ok_or_else(record_invalid)?,
            departure: text(&stay.data, "departure").ok_or_else(record_invalid)?,
            adults: integer(&stay.data, "adults").ok_or_else(record_invalid)?,
            child_ages,
            state: text(&stay.data, "state").ok_or_else(record_invalid)?,
            nights: stay
                .data
                .get("prices")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new())),
            policy,
        });
    }
    let mut advance = 0_i64;
    let mut guest_account_due = 0_i64;
    for account in &accounts {
        advance = advance.saturating_add(money_field(&account.data, "advance")?);
        guest_account_due =
            guest_account_due.saturating_add(money_field(&account.data, "due")?);
    }
    let payment_state = if entries
        .iter()
        .any(|entry| source_key_of(entry).is_some_and(|key| key.starts_with("online-payment:")))
    {
        "received"
    } else {
        "pay_at_hotel"
    };
    let hotel = PmsBookingHotel {
        id: facts.property.id.clone(),
        name: facts.property.name.clone(),
        address: booking
            .get("address")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        contact_email: booking
            .get("contactEmail")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        contact_phone: booking
            .get("contactPhone")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
    };
    let summary = PmsBookingSummary {
        posted_room_total,
        id: facts.reservation.id.clone(),
        number: text(&facts.reservation.data, "number").ok_or_else(record_invalid)?,
        state: text(&facts.reservation.data, "state").ok_or_else(record_invalid)?,
        hotel,
        guest: PmsBookingGuest {
            name: text(&facts.guest.data, "name").ok_or_else(record_invalid)?,
            email: text(&facts.guest.data, "email").ok_or_else(record_invalid)?,
        },
        payment_state: payment_state.to_owned(),
        currency: text(&config, "currency").ok_or_else(record_invalid)?,
        version: facts.reservation.version,
        history: facts
            .reservation
            .data
            .get("history")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new())),
        total,
        advance,
        guest_account_due,
        policy: facts
            .reservation
            .data
            .get("bookingPolicy")
            .cloned()
            .unwrap_or(Value::Null),
        rooms,
        cancellation: cancellation_eligibility(facts, now),
    };
    Ok(summary)
}

/// One `pms_booking_tokens` row to insert for a freshly queued link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PmsBookingTokenRow {
    pub id: String,
    pub property_id: String,
    pub reservation_id: String,
    pub token_hash: String,
    pub expires_at: i64,
    pub created_at: i64,
}

/// One `pms_booking_emails` row to insert.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PmsBookingEmailRow {
    pub id: String,
    pub property_id: String,
    pub reservation_id: String,
    pub token_id: String,
    pub holding_slug: String,
    pub kind: PmsBookingMailKind,
    pub payload_json: String,
    pub next_attempt_at: i64,
    pub created_at: i64,
}

/// A claimed mail under the retained delivery lease. `attempts` is already
/// incremented by the claim statement, matching the retained `RETURNING *`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PmsBookingEmailClaim {
    pub id: String,
    pub property_id: String,
    pub reservation_id: String,
    pub token_id: String,
    pub holding_slug: String,
    pub kind: String,
    pub attempts: i64,
    pub payload_json: String,
}

/// Resolved link target. It is an identity reference, never authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PmsBookingTokenRef {
    pub property_id: String,
    pub reservation_id: String,
}

/// One queued private link and its exact mail row.
#[derive(Clone, Debug)]
pub struct PmsBookingQueuedMail {
    pub token: String,
    pub token_id: String,
    pub email_id: String,
    pub expires_at: i64,
    pub payload_json: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsBookingMailPayload {
    pub from: String,
    pub to: Vec<String>,
    pub subject: String,
    pub text: String,
}

/// Retained private-mail subject.
pub fn booking_mail_subject(kind: PmsBookingMailKind, hotel: &str, number: &str) -> String {
    format!("{hotel} · {} {number}", kind.subject_label())
}

/// Build the exact retained mail payload for one kind. The subject label is
/// kind-specific; the body content is the retained shared template.
pub fn booking_mail_payload(
    kind: PmsBookingMailKind,
    from: &str,
    summary: &PmsBookingSummary,
    link: &str,
) -> PmsBookingMailPayload {
    let subject = booking_mail_subject(kind, &summary.hotel.name, &summary.number);
    let rooms = summary
        .rooms
        .iter()
        .map(|room| format!("{}: {} to {}", room.room_name, room.arrival, room.departure))
        .collect::<Vec<_>>()
        .join("\n");
    let policy = summary
        .policy
        .get("text")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .unwrap_or("Contact the hotel for booking terms.");
    let text = format!(
        "{subject}\n\n{rooms}\n\n{policy}\n\nView your booking: {link}\nThis private link expires in seven days. Request another from the booking website.\n\n{phone}",
        phone = summary.hotel.contact_phone,
    );
    PmsBookingMailPayload {
        from: from.to_owned(),
        to: vec![summary.guest.email.clone()],
        subject,
        text,
    }
}

/// Retained oversized management-link guard used before any store read.
pub fn validate_presented_token(token: &str) -> Result<(), BusinessError> {
    if token.is_empty() || token.len() > MAX_TOKEN_LEN || token.chars().any(char::is_control) {
        return Err(link_expired());
    }
    Ok(())
}

/// Retained `recoverBooking` trimmed, bounded credential fields.
pub fn normalize_recovery_email(value: &str) -> Result<String, BusinessError> {
    let value = ecma_trim(value);
    if value.is_empty() || value.len() > 254 || !value.contains('@') {
        return Err(BusinessError::invalid(
            "PMS_RECOVERY_INPUT",
            "Enter the booking number and email address.",
        ));
    }
    Ok(value.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_rfc_4231_case_two() {
        let digest = hmac_sha256(&[0x0b; 20], b"Hi There");
        assert_eq!(
            hex::encode(digest),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn signing_matches_retained_typescript_hmac() {
        // Computed with Node's createHmac("sha256", "secret") over
        // "pms-booking:access:abc" then base64url.
        assert_eq!(
            sign_booking_value("secret", "access:abc"),
            "pY3lPejE-inp0nVQVU-_MZCFSIMTXWJuhQxNrfjUzrU"
        );
        assert_eq!(
            booking_token_hash("abc.def"),
            "ebb3127bf5c7c4b4e42b51710f4946c1c1d05b331d2379dd15e3a5431ed93416"
        );
    }

    #[test]
    fn base64url_round_trip() {
        for bytes in [b"".as_slice(), b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            let encoded = base64url_encode(bytes);
            assert!(!encoded.contains('='));
            assert_eq!(base64url_decode(&encoded).unwrap(), bytes);
        }
        assert_eq!(base64url_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64url_decode("Zm9vYmFy").unwrap(), b"foobar");
        assert!(base64url_decode("A").is_none());
        assert!(base64url_decode("****").is_none());
    }

    #[test]
    fn link_tokens_verify_only_for_their_audience() {
        let token = booking_link_token("secret", PmsBookingLinkKind::Access, "booking-1");
        assert_eq!(
            verify_booking_link("secret", PmsBookingLinkKind::Access, &token).unwrap(),
            "booking-1"
        );
        assert_eq!(
            verify_booking_link("secret", PmsBookingLinkKind::Payment, &token)
                .unwrap_err()
                .code,
            "PMS_LINK_EXPIRED"
        );
        assert_eq!(
            verify_booking_link("other", PmsBookingLinkKind::Access, &token)
                .unwrap_err()
                .code,
            "PMS_LINK_EXPIRED"
        );
        assert!(split_booking_link(PmsBookingLinkKind::Access, &"a".repeat(201)).is_err());
        assert!(split_booking_link(PmsBookingLinkKind::Access, "one.two.three").is_err());
        assert!(split_booking_link(PmsBookingLinkKind::Access, "one").is_err());
    }

    #[test]
    fn delivery_outcomes_preserve_retry_budget() {
        assert_eq!(
            delivery_failure_outcome(0, None, 1_000),
            PmsBookingDeliveryOutcome::Retry {
                state: "pending",
                next_attempt_at: 2_000
            }
        );
        assert_eq!(
            delivery_failure_outcome(7, None, 1_000),
            PmsBookingDeliveryOutcome::Retry {
                state: "pending",
                next_attempt_at: 1_000 + 128_000
            }
        );
        assert_eq!(
            delivery_failure_outcome(8, None, 1_000),
            PmsBookingDeliveryOutcome::Retry {
                state: "failed",
                next_attempt_at: 1_000 + 256_000
            }
        );
        assert_eq!(
            delivery_failure_outcome(3, Some("PACKAGE_REQUIRED"), 1_000),
            PmsBookingDeliveryOutcome::Blocked {
                code: "PACKAGE_REQUIRED".to_owned(),
                next_attempt_at: 61_000
            }
        );
        assert_eq!(
            delivery_failure_outcome(3, Some("PMS_PAYMENT_REVIEW"), 1_000),
            PmsBookingDeliveryOutcome::Retry {
                state: "pending",
                next_attempt_at: 9_000
            }
        );
        assert!(!blocked_mail_code(""));
    }

    #[test]
    fn rate_limit_math_matches_retained_window() {
        assert_eq!(rate_limit_bucket(0), 0);
        assert_eq!(rate_limit_bucket(59_999), 0);
        assert_eq!(rate_limit_bucket(60_000), 1);
        assert_eq!(stale_rate_limit_bucket(60_000 * 70), 10);
        assert!(!rate_limit_exceeded(30, 30));
        assert!(rate_limit_exceeded(31, 30));
        assert_eq!(rate_limited().code, "PMS_RATE_LIMIT");
    }

    #[test]
    fn public_url_prefers_the_registered_holding_origin() {
        assert_eq!(
            booking_public_url(
                "https://console-dev-erpx.jesadakorn.com",
                Some("https://acme.jesadakorn.com"),
                "acme",
                "/manage#token=abc.def"
            ),
            "https://acme.jesadakorn.com/book/manage#token=abc.def"
        );
        assert_eq!(
            booking_public_url(
                "https://console-dev-erpx.jesadakorn.com",
                None,
                "acme hotel",
                "/manage#token=abc.def"
            ),
            "https://console-dev-erpx.jesadakorn.com/book/acme%20hotel/manage#token=abc.def"
        );
    }

    fn property() -> PmsPropertyRow {
        PmsPropertyRow {
            id: "property".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            name: "Riverside".into(),
            business_date: "2026-11-04".into(),
            state: "open".into(),
            config_json: serde_json::json!({
                "currency": "THB",
                "booking": {
                    "address": "1 River Road",
                    "contactEmail": "stay@example.com",
                    "contactPhone": "+66 2 000 0000"
                }
            })
            .to_string(),
            version: 4,
        }
    }

    fn record(id: &str, kind: &str, version: i64, data: Value) -> PmsRecord {
        PmsRecord {
            id: id.into(),
            kind: kind.into(),
            version,
            updated_at: 10,
            data,
        }
    }

    fn facts() -> PmsBookingMailFacts {
        PmsBookingMailFacts {
            property: property(),
            reservation: record(
                "reservation-1",
                "reservation",
                3,
                serde_json::json!({
                    "number": "BK-1",
                    "state": "confirmed",
                    "guestId": "guest-1",
                    "bookingPolicy": {"deadline": 9_000, "text": "Free cancellation until 2026-11-08"},
                    "history": [{"at": 1, "actor": "staff", "action": "created"}]
                }),
            ),
            guest: record(
                "guest-1",
                "guest",
                1,
                serde_json::json!({"name": "Ada", "email": "ada@example.com"}),
            ),
            room_types: vec![record(
                "type-1",
                "room_type",
                1,
                serde_json::json!({"name": "Deluxe"}),
            )],
            stays: vec![record(
                "stay-1",
                "stay",
                2,
                serde_json::json!({
                    "reservationId": "reservation-1",
                    "typeId": "type-1",
                    "rateId": "rate-1",
                    "guestAccountId": "account-1",
                    "arrival": "2026-11-10",
                    "departure": "2026-11-12",
                    "adults": 2,
                    "childAges": [],
                    "state": "reserved",
                    "prices": [{"date": "2026-11-10", "total": 1_000}, {"date": "2026-11-11", "total": 1_000}]
                }),
            )],
            accounts: vec![record(
                "account-1",
                "guest_account",
                1,
                serde_json::json!({"reservationId": "reservation-1", "advance": 500, "due": 1_500}),
            )],
            entries: vec![record(
                "entry-1",
                "entry",
                1,
                serde_json::json!({"guestAccountId": "account-1", "sourceKey": "night:2026-11-10", "total": 1_000, "kind": "charge"}),
            )],
            refunds: Vec::new(),
        }
    }

    #[test]
    fn summary_and_mail_payload_match_retained_shape() {
        let summary = booking_summary(&facts(), 1_000).unwrap();
        assert_eq!(summary.total, 2_000);
        assert_eq!(summary.advance, 500);
        assert_eq!(summary.guest_account_due, 1_500);
        assert_eq!(summary.posted_room_total, 1_000);
        assert_eq!(summary.payment_state, "pay_at_hotel");
        assert_eq!(summary.rooms.len(), 1);
        assert_eq!(summary.rooms[0].room_name, "Deluxe");
        assert_eq!(summary.rooms[0].nights.as_array().unwrap().len(), 2);
        // A posted folio entry is retained financial activity, so the website
        // must not offer a free cancellation for it.
        assert!(!summary.cancellation.allowed);
        assert_eq!(
            summary.cancellation.reason,
            "This booking has financial activity. Contact the hotel for cancellation"
        );

        let mut clean = facts();
        clean.entries.clear();
        let clean = booking_summary(&clean, 1_000).unwrap();
        assert!(clean.cancellation.allowed);
        assert_eq!(clean.cancellation.reason, "Cancel all rooms without a fee");
        assert_eq!(clean.posted_room_total, 0);

        let payload = booking_mail_payload(
            PmsBookingMailKind::Confirmation,
            "stay@example.com",
            &summary,
            "https://acme.jesadakorn.com/book/manage#token=abc",
        );
        assert_eq!(payload.to, vec!["ada@example.com".to_owned()]);
        assert_eq!(payload.subject, "Riverside · Booking confirmation BK-1");
        assert!(payload.text.contains("Deluxe: 2026-11-10 to 2026-11-12"));
        assert!(payload.text.contains("View your booking: https://acme.jesadakorn.com/book/manage#token=abc"));
        assert!(payload.text.contains("+66 2 000 0000"));
        assert_eq!(
            booking_mail_subject(PmsBookingMailKind::Refund, "Riverside", "BK-1"),
            "Riverside · Refund confirmed BK-1"
        );
        assert_eq!(
            serde_json::to_value(&payload).unwrap()["subject"],
            "Riverside · Booking confirmation BK-1"
        );
    }

    #[test]
    fn cancellation_eligibility_fails_closed() {
        let mut with_payment = facts();
        with_payment.entries.push(record(
            "entry-2",
            "entry",
            1,
            serde_json::json!({"guestAccountId": "account-1", "kind": "payment", "method": "cash"}),
        ));
        assert_eq!(
            cancellation_eligibility(&with_payment, 1_000).reason,
            "This booking has financial activity. Contact the hotel for cancellation"
        );

        let mut expired = facts();
        expired.reservation.data["bookingPolicy"] = serde_json::json!({"deadline": 500});
        assert!(!cancellation_eligibility(&expired, 1_000).allowed);

        let mut closed = facts();
        closed.property.state = "closing".into();
        assert_eq!(
            cancellation_eligibility(&closed, 1_000).reason,
            "Hotel day close is in progress. Please try again shortly"
        );

        let mut paid = facts();
        paid.reservation.data["state"] = serde_json::json!("payment_pending");
        assert_eq!(
            cancellation_eligibility(&paid, 1_000).reason,
            "Contact the hotel to change this booking"
        );
    }

    #[test]
    fn summary_rejects_fractional_money() {
        let mut broken = facts();
        broken.accounts[0].data["advance"] = serde_json::json!(1.5);
        assert_eq!(
            booking_summary(&broken, 1_000).unwrap_err().code,
            "PMS_BOOKING_RECORD_INVALID"
        );
    }

    #[test]
    fn recovery_input_is_trimmed_and_bounded() {
        assert_eq!(
            normalize_recovery_email("  Ada@Example.com ").unwrap(),
            "ada@example.com"
        );
        assert!(normalize_recovery_email("").is_err());
        assert!(normalize_recovery_email("not-an-email").is_err());
    }
}
