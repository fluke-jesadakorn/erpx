//! Pure Support customer ticket-portal rules.
//!
//! This module owns the reviewed customer summary, independent review, version
//! and acknowledgement decisions. It never reads a clock, database, HTTP request
//! or provider. Callers supply the already-authorized actor, the current owner
//! snapshot and the exact command key.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_ID: usize = 200;
pub const MAX_REASON: usize = 2_000;
pub const MAX_SUMMARY: usize = 4_000;
pub const MAX_MESSAGE: usize = 4_000;
pub const MAX_NOTE: usize = 500;
pub const MAX_LIST: usize = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingStatus {
    Draft,
    Approved,
    Rejected,
    Revoked,
}

impl BindingStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            "revoked" => Some(Self::Revoked),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Revoked => "revoked",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecisionAction {
    Approve,
    Reject,
    Revoke,
}

impl DecisionAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "approve" => Some(Self::Approve),
            "reject" => Some(Self::Reject),
            "revoke" => Some(Self::Revoke),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Revoke => "revoke",
        }
    }
}

/// Current Support staff actor. Authority booleans only describe an already
/// verified context; this module does not grant them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortalActor {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub user_id: String,
    pub assignment_id: String,
    pub support_manage: bool,
}

/// Verified external customer session identity. No Holding staff authority is
/// inferred from this value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortalCustomer {
    pub user_id: String,
    pub email: String,
    pub holding_id: Option<String>,
}

impl PortalCustomer {
    pub fn normalized_email(&self) -> String {
        self.email.to_lowercase()
    }
    pub fn identity_key(&self) -> String {
        digest(&serde_json::json!({
            "userId": self.user_id,
            "email": self.normalized_email(),
            "holdingId": self.holding_id,
        }))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalTicket {
    pub id: String,
    pub version: i64,
    pub title: String,
    pub status: String,
    pub source_kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalBindingRow {
    pub id: String,
    pub company_id: String,
    pub ticket_id: String,
    pub ticket_version: i64,
    pub customer_name: String,
    pub email: String,
    pub subject: String,
    pub summary: String,
    pub expires_at: i64,
    pub status: BindingStatus,
    pub version: i64,
    pub prepared_by: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalResponse {
    pub id: String,
    pub version: i64,
    pub message: String,
    pub published_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalRights {
    pub new_work: bool,
    pub settle_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalBindingView {
    pub id: String,
    pub ticket_id: String,
    pub ticket_version: i64,
    pub customer_name: String,
    pub email: String,
    pub subject: String,
    pub summary: String,
    pub expires_at: i64,
    pub status: BindingStatus,
    pub version: i64,
    pub can_approve: bool,
    pub can_reject: bool,
    pub can_revoke: bool,
    pub can_respond: bool,
    pub response: Option<PortalResponse>,
    pub acknowledged_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalWorkspace {
    pub company_id: String,
    pub schema_available: bool,
    pub available: bool,
    pub context_key: Option<String>,
    pub can_prepare: bool,
    pub ticket: Option<PortalTicket>,
    pub bindings: Vec<PortalBindingView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalProjectionItem {
    pub id: String,
    pub ticket_id: String,
    pub subject: String,
    pub summary: String,
    pub customer_name: String,
    pub version: i64,
    pub expires_at: i64,
    pub response: Option<PortalResponse>,
    pub acknowledged_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalProjection {
    pub schema_available: bool,
    pub context_key: String,
    pub items: Vec<PortalProjectionItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareInput {
    pub ticket_id: String,
    pub ticket_version: i64,
    pub customer_name: String,
    pub email: String,
    pub subject: String,
    pub summary: String,
    pub expires_at: i64,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionInput {
    pub expected_version: i64,
    pub action: DecisionAction,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseInput {
    pub expected_binding_version: i64,
    pub expected_response_version: i64,
    pub message: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcknowledgementInput {
    pub expected_binding_version: i64,
    pub response_id: String,
    pub response_version: i64,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareResult {
    pub id: String,
    pub status: &'static str,
    pub version: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionResult {
    pub id: String,
    pub status: BindingStatus,
    pub version: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseResult {
    pub id: String,
    pub binding_id: String,
    pub version: i64,
    pub status: &'static str,
    pub message: String,
    pub published_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcknowledgementResult {
    pub id: String,
    pub binding_id: String,
    pub response_id: String,
    pub response_version: i64,
    pub status: &'static str,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingInsert {
    pub id: String,
    pub company_id: String,
    pub ticket_id: String,
    pub ticket_version: i64,
    pub customer_name: String,
    pub email: String,
    pub subject: String,
    pub summary: String,
    pub expires_at: i64,
    pub status: BindingStatus,
    pub version: i64,
    pub prepared_by: String,
    pub command_id: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventInsert {
    pub id: String,
    pub binding_id: String,
    pub company_id: String,
    pub version: i64,
    pub action: &'static str,
    pub actor_id: String,
    pub command_id: String,
    pub reason: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponseInsert {
    pub id: String,
    pub company_id: String,
    pub binding_id: String,
    pub binding_version: i64,
    pub response_version: i64,
    pub message: String,
    pub reason: String,
    pub actor_id: String,
    pub command_id: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcknowledgementInsert {
    pub id: String,
    pub company_id: String,
    pub binding_id: String,
    pub binding_version: i64,
    pub response_id: String,
    pub response_version: i64,
    pub user_id: String,
    pub email: String,
    pub request_key: String,
    pub request_hash: String,
    pub note: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortalPlan {
    Prepare {
        binding: BindingInsert,
        event: EventInsert,
        result: PrepareResult,
    },
    Decide {
        status: BindingStatus,
        version: i64,
        event: EventInsert,
        result: DecisionResult,
    },
    Respond {
        response: ResponseInsert,
        result: ResponseResult,
    },
    Acknowledge {
        acknowledgement: AcknowledgementInsert,
        result: AcknowledgementResult,
        binding_id: String,
        response_id: String,
        user_id: String,
        company_id: String,
        email: String,
    },
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

/// SHA-256 of the canonical JSON bytes. Kept local so the owner crate does not
/// need a new direct dependency for the existing `contextKey` wire contract.
pub fn digest(value: &Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    let digest = sha256(&bytes);
    let mut text = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(text, "{byte:02x}");
    }
    text
}

fn sha256(input: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut state = [
        0x6a09e667_u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = input.to_vec();
    let bit_len = (input.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in message.chunks_exact(64) {
        let mut words = [0_u32; 64];
        for (index, word) in chunk.chunks_exact(4).enumerate() {
            words[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ (!e & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }
    let mut output = [0_u8; 32];
    for (index, word) in state.iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    output
}

fn js_trim(value: &str) -> &str {
    value.trim_matches(|character| {
        matches!(
            character,
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
    })
}

fn units(value: &str) -> usize {
    value.encode_utf16().count()
}

fn bounded(value: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&units(value))
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, BusinessError> {
    value.get(key).and_then(Value::as_str).ok_or_else(|| {
        invalid(
            "VALIDATION_FAILED",
            "Provide a valid Support Portal request.",
        )
    })
}

fn version(value: &Value, key: &str) -> Result<i64, BusinessError> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            invalid(
                "VALIDATION_FAILED",
                "Provide a valid Support Portal version.",
            )
        })
}

fn email(value: &str) -> Result<String, BusinessError> {
    if units(value) > 254 || value.contains(char::is_whitespace) {
        return Err(invalid(
            "VALIDATION_FAILED",
            "Provide a valid customer email.",
        ));
    }
    let mut parts = value.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if local.is_empty() || domain.is_empty() || parts.next().is_some() || !domain.contains('.') {
        return Err(invalid(
            "VALIDATION_FAILED",
            "Provide a valid customer email.",
        ));
    }
    Ok(value.to_lowercase())
}

/// Exact command-key shape used by the TypeScript writer.
pub fn validate_command_key(key: &str) -> Result<(), BusinessError> {
    let valid = (8..=180).contains(&key.len())
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.' | b'-'));
    if valid {
        Ok(())
    } else {
        Err(invalid(
            "COMMAND_KEY_REQUIRED",
            "Use a valid Idempotency-Key.",
        ))
    }
}

pub fn parse_selection(raw: &Value) -> Result<Option<String>, BusinessError> {
    let ticket = raw.get("ticketId").and_then(Value::as_str);
    let binding = raw.get("bindingId").and_then(Value::as_str);
    if ticket.is_some() && binding.is_some() {
        return Err(invalid(
            "VALIDATION_FAILED",
            "Choose one ticket or binding.",
        ));
    }
    for value in [ticket, binding].into_iter().flatten() {
        if !bounded(value, 1, MAX_ID) {
            return Err(invalid("VALIDATION_FAILED", "Choose a valid selection."));
        }
    }
    Ok(binding.or(ticket).map(str::to_owned))
}

pub fn parse_prepare(raw: &Value) -> Result<PrepareInput, BusinessError> {
    let ticket_id = text(raw, "ticketId")?;
    if !bounded(ticket_id, 1, MAX_ID) {
        return Err(invalid("VALIDATION_FAILED", "Choose a valid ticket."));
    }
    let customer_name = js_trim(text(raw, "customerName")?);
    let subject = js_trim(text(raw, "subject")?);
    let summary = js_trim(text(raw, "summary")?);
    let reason = js_trim(text(raw, "reason")?);
    if !bounded(customer_name, 1, 160)
        || !bounded(subject, 3, 160)
        || !bounded(summary, 3, MAX_SUMMARY)
        || !bounded(reason, 3, MAX_REASON)
    {
        return Err(invalid(
            "VALIDATION_FAILED",
            "Provide a complete reviewed customer summary.",
        ));
    }
    Ok(PrepareInput {
        ticket_id: ticket_id.to_owned(),
        ticket_version: version(raw, "ticketVersion")?,
        customer_name: customer_name.to_owned(),
        email: email(js_trim(text(raw, "email")?))?,
        subject: subject.to_owned(),
        summary: summary.to_owned(),
        expires_at: version(raw, "expiresAt")?,
        reason: reason.to_owned(),
    })
}

pub fn parse_decision(raw: &Value) -> Result<DecisionInput, BusinessError> {
    let action = text(raw, "action")
        .ok()
        .and_then(DecisionAction::parse)
        .ok_or_else(|| invalid("VALIDATION_FAILED", "Choose a valid decision."))?;
    let reason = js_trim(text(raw, "reason")?);
    if !bounded(reason, 3, MAX_REASON) {
        return Err(invalid("VALIDATION_FAILED", "Provide a decision reason."));
    }
    Ok(DecisionInput {
        expected_version: version(raw, "expectedVersion")?,
        action,
        reason: reason.to_owned(),
    })
}

pub fn parse_response(raw: &Value) -> Result<ResponseInput, BusinessError> {
    let message = js_trim(text(raw, "message")?);
    let reason = js_trim(text(raw, "reason")?);
    if !bounded(message, 3, MAX_MESSAGE) || !bounded(reason, 3, MAX_REASON) {
        return Err(invalid(
            "VALIDATION_FAILED",
            "Provide a complete customer response.",
        ));
    }
    let expected_response_version = raw
        .get("expectedResponseVersion")
        .and_then(Value::as_i64)
        .filter(|value| *value >= 0)
        .ok_or_else(|| invalid("VALIDATION_FAILED", "Provide a valid response version."))?;
    Ok(ResponseInput {
        expected_binding_version: version(raw, "expectedBindingVersion")?,
        expected_response_version,
        message: message.to_owned(),
        reason: reason.to_owned(),
    })
}

pub fn parse_acknowledgement(raw: &Value) -> Result<AcknowledgementInput, BusinessError> {
    let response_id = text(raw, "responseId")?;
    if !bounded(response_id, 1, MAX_ID) {
        return Err(invalid("VALIDATION_FAILED", "Choose a valid response."));
    }
    let note = raw.get("note").and_then(Value::as_str).unwrap_or("");
    if units(note) > MAX_NOTE {
        return Err(invalid("VALIDATION_FAILED", "Keep the note short."));
    }
    Ok(AcknowledgementInput {
        expected_binding_version: version(raw, "expectedBindingVersion")?,
        response_id: response_id.to_owned(),
        response_version: version(raw, "responseVersion")?,
        note: note.to_owned(),
    })
}

fn require_current_ticket(ticket: &PortalTicket, expected: i64) -> Result<(), BusinessError> {
    if ticket.version != expected {
        return Err(conflict(
            "SUPPORT_PORTAL_TICKET_CHANGED",
            "Reload the actual ticket before preparing access.",
        ));
    }
    if ticket.status == "cancelled" {
        return Err(conflict(
            "SUPPORT_PORTAL_TICKET_UNAVAILABLE",
            "A cancelled ticket cannot receive new customer access.",
        ));
    }
    Ok(())
}

fn require_independent(actor: &PortalActor, prepared_by: &str) -> Result<(), BusinessError> {
    if actor.user_id == prepared_by {
        return Err(forbidden(
            "INDEPENDENT_SUPPORT_PORTAL_REVIEW_REQUIRED",
            "A different Support manager must review customer access.",
        ));
    }
    Ok(())
}

pub fn plan_prepare(
    actor: &PortalActor,
    input: &PrepareInput,
    ticket: &PortalTicket,
    key: &str,
    now: i64,
) -> Result<PortalPlan, BusinessError> {
    validate_command_key(key)?;
    require_current_ticket(ticket, input.ticket_version)?;
    if input.expires_at <= now {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "SUPPORT_PORTAL_EXPIRY_REQUIRED",
            "Choose a future expiry.",
        ));
    }
    let id = format!("support-portal-{key}");
    Ok(PortalPlan::Prepare {
        binding: BindingInsert {
            id: id.clone(),
            company_id: actor.company_id.clone(),
            ticket_id: input.ticket_id.clone(),
            ticket_version: input.ticket_version,
            customer_name: input.customer_name.clone(),
            email: input.email.clone(),
            subject: input.subject.clone(),
            summary: input.summary.clone(),
            expires_at: input.expires_at,
            status: BindingStatus::Draft,
            version: 1,
            prepared_by: actor.user_id.clone(),
            command_id: key.to_owned(),
            created_at: now,
            updated_at: now,
        },
        event: EventInsert {
            id: format!("support-portal-event-{key}"),
            binding_id: id.clone(),
            company_id: actor.company_id.clone(),
            version: 1,
            action: "prepare",
            actor_id: actor.user_id.clone(),
            command_id: key.to_owned(),
            reason: input.reason.clone(),
            created_at: now,
        },
        result: PrepareResult {
            id,
            status: "draft",
            version: 1,
        },
    })
}

pub fn plan_decision(
    actor: &PortalActor,
    binding: &PortalBindingRow,
    ticket: &PortalTicket,
    input: &DecisionInput,
    key: &str,
    now: i64,
) -> Result<PortalPlan, BusinessError> {
    validate_command_key(key)?;
    if input.expected_version != binding.version {
        return Err(conflict(
            "SUPPORT_PORTAL_VERSION_CHANGED",
            "Reload the current customer access.",
        ));
    }
    let valid_state = match input.action {
        DecisionAction::Revoke => binding.status == BindingStatus::Approved,
        _ => binding.status == BindingStatus::Draft,
    };
    if !valid_state {
        return Err(conflict(
            "SUPPORT_PORTAL_STATE_CHANGED",
            "This customer access no longer permits that decision.",
        ));
    }
    if input.action != DecisionAction::Revoke {
        require_independent(actor, &binding.prepared_by)?;
    }
    if input.action == DecisionAction::Approve {
        if ticket.version != binding.ticket_version {
            return Err(conflict(
                "SUPPORT_PORTAL_TICKET_CHANGED",
                "The ticket changed after the summary was prepared. Reject and prepare a new review.",
            ));
        }
        if binding.expires_at <= now {
            return Err(conflict(
                "SUPPORT_PORTAL_BINDING_EXPIRED",
                "Prepare a new access review with a future expiry.",
            ));
        }
    }
    let status = match input.action {
        DecisionAction::Approve => BindingStatus::Approved,
        DecisionAction::Reject => BindingStatus::Rejected,
        DecisionAction::Revoke => BindingStatus::Revoked,
    };
    let version = input.expected_version + 1;
    Ok(PortalPlan::Decide {
        status,
        version,
        event: EventInsert {
            id: format!("support-portal-event-{key}"),
            binding_id: binding.id.clone(),
            company_id: actor.company_id.clone(),
            version,
            action: input.action.as_str(),
            actor_id: actor.user_id.clone(),
            command_id: key.to_owned(),
            reason: input.reason.clone(),
            created_at: now,
        },
        result: DecisionResult {
            id: binding.id.clone(),
            status,
            version,
        },
    })
}

pub fn plan_response(
    actor: &PortalActor,
    binding: &PortalBindingRow,
    current_response_version: i64,
    input: &ResponseInput,
    key: &str,
    now: i64,
) -> Result<PortalPlan, BusinessError> {
    validate_command_key(key)?;
    if binding.status != BindingStatus::Approved || binding.expires_at <= now {
        return Err(forbidden(
            "SUPPORT_PORTAL_BINDING_UNAVAILABLE",
            "Customer access was revoked, expired or is not approved.",
        ));
    }
    if actor.user_id == binding.prepared_by {
        return Err(forbidden(
            "INDEPENDENT_SUPPORT_PORTAL_RESPONDER_REQUIRED",
            "A different Support manager must publish the response.",
        ));
    }
    if binding.version != input.expected_binding_version
        || current_response_version != input.expected_response_version
    {
        return Err(conflict(
            "SUPPORT_PORTAL_VERSION_CHANGED",
            "Reload the exact binding and response before publishing.",
        ));
    }
    let version = input.expected_response_version + 1;
    let id = format!("support-portal-response-{key}");
    Ok(PortalPlan::Respond {
        response: ResponseInsert {
            id: id.clone(),
            company_id: actor.company_id.clone(),
            binding_id: binding.id.clone(),
            binding_version: input.expected_binding_version,
            response_version: version,
            message: input.message.clone(),
            reason: input.reason.clone(),
            actor_id: actor.user_id.clone(),
            command_id: key.to_owned(),
            created_at: now,
        },
        result: ResponseResult {
            id,
            binding_id: binding.id.clone(),
            version,
            status: "published",
            message: input.message.clone(),
            published_at: now,
        },
    })
}

/// Customer acknowledgement. The caller has already verified the private
/// customer session and current purchase/release scope; this function still
/// rechecks the approved binding identity, exact published response and replay
/// hash.
pub fn plan_acknowledgement(
    customer: &PortalCustomer,
    binding: &PortalBindingRow,
    current_response: &PortalResponse,
    input: &AcknowledgementInput,
    key: &str,
    now: i64,
) -> Result<PortalPlan, BusinessError> {
    validate_command_key(key)?;
    if binding.status != BindingStatus::Approved || binding.expires_at <= now {
        return Err(forbidden(
            "SUPPORT_PORTAL_BINDING_UNAVAILABLE",
            "Customer access was revoked, expired or is not approved.",
        ));
    }
    if binding.email != customer.normalized_email() {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SUPPORT_PORTAL_NOT_FOUND",
            "This verified customer ticket is unavailable.",
        ));
    }
    if binding.version != input.expected_binding_version
        || current_response.id != input.response_id
        || current_response.version != input.response_version
    {
        return Err(conflict(
            "SUPPORT_PORTAL_VERSION_CHANGED",
            "Reload the exact published response before acknowledging it.",
        ));
    }
    let request_hash = digest(&serde_json::json!({
        "actor": {
            "userId": customer.user_id,
            "email": customer.normalized_email(),
            "holdingId": customer.holding_id,
        },
        "bindingId": binding.id,
        "input": {
            "expectedBindingVersion": input.expected_binding_version,
            "responseId": input.response_id,
            "responseVersion": input.response_version,
            "note": input.note,
        },
    }));
    let id = format!("support-portal-ack-{key}");
    Ok(PortalPlan::Acknowledge {
        acknowledgement: AcknowledgementInsert {
            id: id.clone(),
            company_id: binding.company_id.clone(),
            binding_id: binding.id.clone(),
            binding_version: binding.version,
            response_id: input.response_id.clone(),
            response_version: input.response_version,
            user_id: customer.user_id.clone(),
            email: customer.normalized_email(),
            request_key: key.to_owned(),
            request_hash,
            note: input.note.clone(),
            created_at: now,
        },
        result: AcknowledgementResult {
            id,
            binding_id: binding.id.clone(),
            response_id: input.response_id.clone(),
            response_version: input.response_version,
            status: "acknowledged",
            created_at: now,
        },
        binding_id: binding.id.clone(),
        response_id: input.response_id.clone(),
        user_id: customer.user_id.clone(),
        company_id: binding.company_id.clone(),
        email: customer.normalized_email(),
    })
}

/// Presentation flags for one retained binding. A caller that has not verified
/// `support.manage` receives no action permission.
pub fn binding_view(
    actor: &PortalActor,
    binding: &PortalBindingRow,
    rights: &PortalRights,
    response: Option<PortalResponse>,
    acknowledged_at: Option<i64>,
    now: i64,
) -> PortalBindingView {
    let manage = actor.support_manage;
    let independent = actor.user_id != binding.prepared_by;
    let draft = binding.status == BindingStatus::Draft;
    let approved = binding.status == BindingStatus::Approved;
    let future = binding.expires_at > now;
    PortalBindingView {
        id: binding.id.clone(),
        ticket_id: binding.ticket_id.clone(),
        ticket_version: binding.ticket_version,
        customer_name: binding.customer_name.clone(),
        email: binding.email.clone(),
        subject: binding.subject.clone(),
        summary: binding.summary.clone(),
        expires_at: binding.expires_at,
        status: binding.status,
        version: binding.version,
        can_approve: manage && independent && draft && future && rights.new_work,
        can_reject: manage && independent && draft && rights.settle_existing,
        can_revoke: manage && approved && rights.settle_existing,
        can_respond: manage && independent && approved && future && rights.settle_existing,
        response,
        acknowledged_at,
    }
}

pub fn can_prepare(
    actor: &PortalActor,
    rights: &PortalRights,
    ticket: Option<&PortalTicket>,
) -> bool {
    actor.support_manage
        && rights.new_work
        && ticket.is_some_and(|ticket| ticket.status != "cancelled")
}

/// Ask the caller to reject an invalid current response version.
pub fn response_version(response: Option<&PortalResponse>) -> i64 {
    response.map_or(0, |response| response.version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn actor(user: &str, manage: bool) -> PortalActor {
        PortalActor {
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            user_id: user.into(),
            assignment_id: "assignment-1".into(),
            support_manage: manage,
        }
    }

    fn ticket() -> PortalTicket {
        PortalTicket {
            id: "ticket-1".into(),
            version: 3,
            title: "Printer offline".into(),
            status: "open".into(),
            source_kind: "general".into(),
        }
    }

    fn binding(status: BindingStatus, version: i64) -> PortalBindingRow {
        PortalBindingRow {
            id: "support-portal-key-0001".into(),
            company_id: "company".into(),
            ticket_id: "ticket-1".into(),
            ticket_version: 3,
            customer_name: "Customer".into(),
            email: "customer@example.test".into(),
            subject: "Printer update".into(),
            summary: "We are reviewing the printer issue.".into(),
            expires_at: 1_000,
            status,
            version,
            prepared_by: "preparer".into(),
        }
    }

    #[test]
    fn prepare_requires_current_ticket_future_expiry_and_normalized_email() {
        let input = parse_prepare(&json!({
            "ticketId":"ticket-1","ticketVersion":3,"customerName":" Customer ",
            "email":"Customer@Example.Test","subject":"Printer update",
            "summary":"We are reviewing the printer issue.","expiresAt":2000,
            "reason":"Customer asked for an update"
        }))
        .unwrap();
        assert_eq!(input.customer_name, "Customer");
        assert_eq!(input.email, "customer@example.test");
        assert!(matches!(
            plan_prepare(&actor("manager", true), &input, &ticket(), "support-key-0001", 1_000),
            Ok(PortalPlan::Prepare { ref result, .. }) if result.status == "draft"
                && result.version == 1 && result.id == "support-portal-support-key-0001"
        ));
        let mut stale = input.clone();
        stale.ticket_version = 2;
        assert!(matches!(
            plan_prepare(&actor("manager", true), &stale, &ticket(), "support-key-0002", 1_000),
            Err(error) if error.code == "SUPPORT_PORTAL_TICKET_CHANGED"
        ));
        let mut expired = input;
        expired.expires_at = 1_000;
        assert!(matches!(
            plan_prepare(&actor("manager", true), &expired, &ticket(), "support-key-0003", 1_000),
            Err(error) if error.code == "SUPPORT_PORTAL_EXPIRY_REQUIRED"
        ));
    }

    #[test]
    fn decisions_require_independence_state_and_version() {
        let input = parse_decision(&json!({
            "expectedVersion":1,"action":"approve","reason":"Reviewed the summary"
        }))
        .unwrap();
        assert!(matches!(
            plan_decision(&actor("preparer", true), &binding(BindingStatus::Draft, 1), &ticket(), &input, "decision-key-0001", 500),
            Err(error) if error.code == "INDEPENDENT_SUPPORT_PORTAL_REVIEW_REQUIRED"
        ));
        let plan = plan_decision(
            &actor("reviewer", true),
            &binding(BindingStatus::Draft, 1),
            &ticket(),
            &input,
            "decision-key-0002",
            500,
        )
        .unwrap();
        assert!(matches!(
            plan,
            PortalPlan::Decide {
                status: BindingStatus::Approved,
                version: 2,
                ..
            }
        ));
        let mut repeated = input;
        repeated.expected_version = 2;
        assert!(matches!(
            plan_decision(&actor("reviewer", true), &binding(BindingStatus::Approved, 2), &ticket(), &repeated, "decision-key-0003", 500),
            Err(error) if error.code == "SUPPORT_PORTAL_STATE_CHANGED"
        ));
    }

    #[test]
    fn responses_require_approved_independent_and_exact_versions() {
        let input = parse_response(&json!({
            "expectedBindingVersion":2,"expectedResponseVersion":0,
            "message":"The printer is repaired.","reason":"Engineer confirmed repair"
        }))
        .unwrap();
        assert!(matches!(
            plan_response(&actor("preparer", true), &binding(BindingStatus::Approved, 2), 0, &input, "response-key-0001", 500),
            Err(error) if error.code == "INDEPENDENT_SUPPORT_PORTAL_RESPONDER_REQUIRED"
        ));
        let plan = plan_response(
            &actor("reviewer", true),
            &binding(BindingStatus::Approved, 2),
            0,
            &input,
            "response-key-0002",
            500,
        )
        .unwrap();
        assert!(matches!(
            plan,
            PortalPlan::Respond { ref result, .. }
                if result.version == 1 && result.status == "published"
                    && result.id == "support-portal-response-response-key-0002"
        ));
        let stale = ResponseInput {
            expected_binding_version: 1,
            ..input
        };
        assert!(matches!(
            plan_response(&actor("reviewer", true), &binding(BindingStatus::Approved, 2), 0, &stale, "response-key-0003", 500),
            Err(error) if error.code == "SUPPORT_PORTAL_VERSION_CHANGED"
        ));
    }

    #[test]
    fn acknowledgement_binds_customer_identity_and_exact_response() {
        let customer = PortalCustomer {
            user_id: "customer-user".into(),
            email: "Customer@Example.Test".into(),
            holding_id: Some("h001".into()),
        };
        let response = PortalResponse {
            id: "support-portal-response-key".into(),
            version: 1,
            message: "The printer is repaired.".into(),
            published_at: 500,
        };
        let input = parse_acknowledgement(&json!({
            "expectedBindingVersion":2,"responseId":"support-portal-response-key",
            "responseVersion":1,"note":"Thank you"
        }))
        .unwrap();
        assert!(matches!(
            plan_acknowledgement(&customer, &binding(BindingStatus::Approved, 2), &response, &input, "ack-key-00000001", 600),
            Ok(PortalPlan::Acknowledge { ref result, .. })
                if result.status == "acknowledged" && result.response_version == 1
        ));
        let other = PortalCustomer {
            email: "other@example.test".into(),
            ..customer
        };
        assert!(matches!(
            plan_acknowledgement(&other, &binding(BindingStatus::Approved, 2), &response, &input, "ack-key-00000002", 600),
            Err(error) if error.code == "SUPPORT_PORTAL_NOT_FOUND"
        ));
    }

    #[test]
    fn binding_view_never_grants_unverified_actions() {
        let view = binding_view(
            &actor("reviewer", false),
            &binding(BindingStatus::Draft, 1),
            &PortalRights {
                new_work: true,
                settle_existing: true,
            },
            None,
            None,
            500,
        );
        assert!(!view.can_approve && !view.can_reject && !view.can_revoke && !view.can_respond);
        let manage = binding_view(
            &actor("reviewer", true),
            &binding(BindingStatus::Approved, 2),
            &PortalRights {
                new_work: false,
                settle_existing: true,
            },
            None,
            None,
            500,
        );
        assert!(manage.can_revoke && manage.can_respond && !manage.can_approve);
    }
}
