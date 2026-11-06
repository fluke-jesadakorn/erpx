//! Native Legal record reads and the bounded document/hold/archive lifecycle.
//!
//! This module owns only facts and policy. Current IAM authority, Holding
//! selection, purchases and persistence are supplied by the caller.
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const REGISTER_PAGE_SIZE: i64 = 30;
pub const REGISTER_MAX_PAGE: i64 = 100_000;
pub const SEARCH_LIMIT: usize = 200;
pub const STATE_LIMIT: usize = 40;
pub const DOCUMENT_LIMIT: usize = 100;
pub const DETAIL_LIMIT: usize = 100;
pub const DOCUMENT_BODY_LIMIT: usize = 200_000;
pub const DOCUMENT_NAME_LIMIT: usize = 200;
pub const REASON_MIN: usize = 3;
pub const REASON_MAX: usize = 4_000;
pub const HOLD_LIMIT: usize = 50;

const KINDS: [&str; 12] = [
    "request",
    "matter",
    "contract",
    "obligation",
    "compliance",
    "license",
    "corporate",
    "ip",
    "privacy",
    "counsel",
    "template",
    "rule",
];

const TRANSITIONS: [(&str, &[&str]); 11] = [
    ("draft", &["review", "cancelled"]),
    ("review", &["draft", "approval", "rejected"]),
    ("approval", &["draft", "approved", "rejected"]),
    ("approved", &["active", "draft"]),
    ("active", &["completed", "closed", "expired", "terminated"]),
    ("completed", &["active", "archived"]),
    ("closed", &["active", "archived"]),
    ("expired", &["draft", "archived"]),
    ("terminated", &["archived"]),
    ("rejected", &["draft", "archived"]),
    ("cancelled", &["draft", "archived"]),
];

/// Zod 4.6.7 string length counts Unicode scalars (`chars().count()`),
/// not UTF-16 code units (`String.length`) or UTF-8 bytes.
fn cp_len(value: &str) -> usize {
    value.chars().count()
}

/// JavaScript `String.prototype.trim()` parity for zod `.trim()` fields.
/// Rust's `str::trim` additionally trims NEL (U+0085) and omits ZWNBSP
/// (U+FEFF); JS trims FEFF and not NEL.
fn js_trim(value: &str) -> &str {
    value.trim_matches(|c: char| c == '\u{FEFF}' || (c.is_whitespace() && c != '\u{0085}'))
}

/// A constructible projection only. It never proves current IAM authority.
#[derive(Clone, Debug)]
pub struct LegalRecordsActor {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub user_id: UserId,
    pub assignment_id: erpx_server_shared::public::AssignmentId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub role: String,
    pub permissions: BTreeSet<String>,
}

impl LegalRecordsActor {
    pub fn can(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }

    pub fn is_requester(&self) -> bool {
        self.role == "legal_requester"
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalRegisterQuery {
    pub kind: Option<String>,
    pub search: String,
    pub state: String,
    pub page: i64,
    pub mine: bool,
}

impl LegalRegisterQuery {
    pub fn parse(
        kind: Option<&str>,
        search: &str,
        state: &str,
        page: i64,
        mine: bool,
    ) -> Result<Self, BusinessError> {
        if let Some(kind) = kind
            && !KINDS.contains(&kind)
        {
            return Err(invalid(
                "LEGAL_KIND_INVALID",
                "Choose a current Legal record type.",
            ));
        }
        if cp_len(search) > SEARCH_LIMIT || search.chars().any(char::is_control) {
            return Err(invalid(
                "LEGAL_SEARCH_INVALID",
                "Use a shorter Legal search.",
            ));
        }
        if cp_len(state) > STATE_LIMIT || state.chars().any(char::is_control) {
            return Err(invalid(
                "LEGAL_STATE_INVALID",
                "Choose a current Legal state.",
            ));
        }
        if !(1..=REGISTER_MAX_PAGE).contains(&page) {
            return Err(invalid(
                "LEGAL_PAGE_INVALID",
                "Choose a current Legal page.",
            ));
        }
        Ok(Self {
            kind: kind.map(str::to_owned),
            search: search.to_owned(),
            state: state.to_owned(),
            page,
            mine,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct LegalRecord {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub classification: String,
    pub country: String,
    pub jurisdiction: String,
    pub language: String,
    pub currency: String,
    pub timezone: String,
    pub priority: String,
    pub due_date: Option<String>,
    pub owner_id: String,
    pub created_by: String,
    pub state: String,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub data: Value,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct LegalStateCount {
    pub state: String,
    pub count: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct LegalRegister {
    pub records: Vec<LegalRecord>,
    pub total: i64,
    pub page: i64,
    pub counts: Vec<LegalStateCount>,
    pub enabled: bool,
    pub can_edit: bool,
    pub can_hold: bool,
    pub can_release_hold: bool,
    pub can_share: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct LegalDocumentVersion {
    pub id: String,
    pub item_id: String,
    pub version: i64,
    pub name: String,
    pub media_type: String,
    pub checksum: String,
    pub bytes: i64,
    pub body: String,
    pub created_by: String,
    pub created_at: i64,
    pub locked: bool,
    pub object_key: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LegalEvent {
    pub id: String,
    pub item_id: String,
    pub action: String,
    pub actor_id: String,
    pub note: String,
    pub version: i64,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct LegalMember {
    pub user_id: String,
    pub access: String,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct LegalHoldView {
    pub id: String,
    pub reason: String,
    pub state: String,
    pub direct: bool,
    pub acknowledged_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LegalLinkView {
    pub target_id: String,
    pub relation: String,
    pub title: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct LegalCommentView {
    pub id: String,
    pub body: String,
    pub visibility: String,
    pub created_by: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct LegalSignerView {
    pub user_id: String,
    pub ordinal: i64,
    pub state: String,
    pub signed_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LegalEnvelopeView {
    pub id: String,
    pub state: String,
    pub document_id: String,
    pub expires_at: i64,
    pub consent: String,
    pub signers: Vec<LegalSignerView>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LegalShareView {
    pub id: String,
    pub email: String,
    pub document_id: String,
    pub user_id: Option<String>,
    pub expires_at: i64,
    pub revoked_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LegalSubmissionView {
    pub id: String,
    pub name: String,
    pub checksum: String,
    pub created_by: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct LegalAccessEvent {
    pub id: String,
    pub document_id: Option<String>,
    pub actor_id: String,
    pub action: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct LegalDetail {
    pub item: LegalRecord,
    pub documents: Vec<LegalDocumentVersion>,
    pub events: Vec<LegalEvent>,
    pub members: Vec<LegalMember>,
    pub holds: Vec<LegalHoldView>,
    pub links: Vec<LegalLinkView>,
    pub comments: Vec<LegalCommentView>,
    pub envelopes: Vec<LegalEnvelopeView>,
    pub shares: Vec<LegalShareView>,
    pub submissions: Vec<LegalSubmissionView>,
    pub access: Vec<LegalAccessEvent>,
    pub can_edit: bool,
    pub can_hold: bool,
    pub can_release_hold: bool,
    pub can_share: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalHoldSource {
    pub id: String,
    pub state: String,
    pub created_by: String,
    pub release_requested_by: Option<String>,
    pub same_person_creator: bool,
    pub same_person_release_requester: bool,
}

#[derive(Clone, Debug)]
pub struct LegalCommandSource {
    pub item: LegalRecord,
    pub enabled: bool,
    pub editor: bool,
    pub frozen: bool,
    pub holds: Vec<LegalHoldSource>,
    pub next_document_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalCommandEnvelope {
    pub action: String,
    pub id: String,
    pub expected_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegalCommand {
    AddDocument { name: String, body: String },
    CreateHold { reason: String },
    RequestHoldRelease { hold_id: String, reason: String },
    ReleaseHold { hold_id: String, reason: String },
    Archive { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegalCommandResult {
    pub id: String,
    pub version: i64,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegalCommandEffect {
    AddDocument {
        document_id: String,
        document_version: i64,
        name: String,
        body: String,
        checksum: String,
        bytes: i64,
    },
    CreateHold {
        hold_id: String,
        reason: String,
    },
    RequestHoldRelease {
        hold_id: String,
    },
    ReleaseHold {
        hold_id: String,
    },
    Archive {
        reason: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalCommandPlan {
    pub item_id: String,
    pub expected_version: i64,
    pub next_version: i64,
    pub new_state: String,
    pub action: String,
    pub operation: String,
    pub note: String,
    pub effect: LegalCommandEffect,
    pub result: LegalCommandResult,
}

pub fn authorize_register(actor: &LegalRecordsActor) -> Result<(), BusinessError> {
    require(actor, "legal.read")
}

pub fn authorize_detail(actor: &LegalRecordsActor) -> Result<(), BusinessError> {
    require(actor, "legal.read")
}

pub fn require(actor: &LegalRecordsActor, permission: &str) -> Result<(), BusinessError> {
    if actor.can(permission) {
        Ok(())
    } else {
        Err(BusinessError::new(
            FailureKind::Forbidden,
            "PERMISSION_REQUIRED",
            "Your selected role does not grant this permission.",
        ))
    }
}

pub fn parse_detail_id(id: &str) -> Result<&str, BusinessError> {
    if id.is_empty() || id.len() > 200 || id.chars().any(char::is_control) {
        return Err(invalid(
            "LEGAL_RECORD_INVALID",
            "Choose a current Legal record.",
        ));
    }
    Ok(id)
}

pub fn parse_envelope(raw: &Value) -> Result<LegalCommandEnvelope, BusinessError> {
    let object = raw
        .as_object()
        .ok_or_else(|| invalid("LEGAL_ACTION_REQUIRED", "Choose a current Legal action."))?;
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("LEGAL_ACTION_REQUIRED", "Choose a current Legal action."))?;
    if !matches!(
        action,
        "document" | "hold.create" | "hold.request-release" | "hold.release" | "transition"
    ) {
        return Err(invalid(
            "LEGAL_ACTION_UNSUPPORTED",
            "This Legal action is not available through the native record command.",
        ));
    }
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("LEGAL_RECORD_REQUIRED", "Choose a current Legal record."))?;
    parse_detail_id(id)?;
    let expected_version = object
        .get("expectedVersion")
        .and_then(Value::as_i64)
        .filter(|version| *version > 0)
        .ok_or_else(|| {
            invalid(
                "LEGAL_VERSION_REQUIRED",
                "Refresh the Legal record before saving.",
            )
        })?;
    if action == "transition" {
        let state = object.get("state").and_then(Value::as_str);
        if state != Some("archived") {
            return Err(invalid(
                "LEGAL_ACTION_UNSUPPORTED",
                "Only the archive transition is available through the native record command.",
            ));
        }
    }
    Ok(LegalCommandEnvelope {
        action: action.to_owned(),
        id: id.to_owned(),
        expected_version,
    })
}

pub fn command_permission(action: &str) -> Result<&'static str, BusinessError> {
    match action {
        "document" | "transition" => Ok("legal.edit"),
        "hold.create" | "hold.request-release" => Ok("legal.hold"),
        "hold.release" => Ok("legal.hold.release"),
        _ => Err(invalid(
            "LEGAL_ACTION_UNSUPPORTED",
            "This Legal action is not available through the native record command.",
        )),
    }
}

pub fn parse_command(
    raw: &Value,
    envelope: &LegalCommandEnvelope,
) -> Result<LegalCommand, BusinessError> {
    let object = raw
        .as_object()
        .expect("the envelope already proved an object");
    match envelope.action.as_str() {
        "document" => {
            let name = object.get("name").and_then(Value::as_str).ok_or_else(|| {
                invalid("LEGAL_DOCUMENT_NAME", "Name the Legal document version.")
            })?;
            if name.is_empty()
                || cp_len(name) > DOCUMENT_NAME_LIMIT
                || name.chars().any(char::is_control)
            {
                return Err(invalid(
                    "LEGAL_DOCUMENT_NAME",
                    "Use a document name of at most 200 characters.",
                ));
            }
            let body = object.get("body").and_then(Value::as_str).ok_or_else(|| {
                invalid("LEGAL_DOCUMENT_BODY", "Author the Legal document version.")
            })?;
            if body.is_empty() || body.len() > DOCUMENT_BODY_LIMIT {
                return Err(invalid(
                    "LEGAL_DOCUMENT_BODY",
                    "Use nonempty document text of at most 200000 bytes.",
                ));
            }
            Ok(LegalCommand::AddDocument {
                name: name.to_owned(),
                body: body.to_owned(),
            })
        }
        "hold.create" => Ok(LegalCommand::CreateHold {
            reason: reason(object)?,
        }),
        "hold.request-release" => Ok(LegalCommand::RequestHoldRelease {
            hold_id: hold_id(object)?,
            reason: reason(object)?,
        }),
        "hold.release" => Ok(LegalCommand::ReleaseHold {
            hold_id: hold_id(object)?,
            reason: reason(object)?,
        }),
        "transition" => Ok(LegalCommand::Archive {
            reason: reason(object)?,
        }),
        _ => unreachable!("the envelope rejects unsupported actions"),
    }
}

fn hold_id(object: &serde_json::Map<String, Value>) -> Result<String, BusinessError> {
    let id = object
        .get("holdId")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("LEGAL_HOLD_REQUIRED", "Choose a current Legal hold."))?;
    if id.is_empty() || id.len() > 200 || id.chars().any(char::is_control) {
        return Err(invalid(
            "LEGAL_HOLD_REQUIRED",
            "Choose a current Legal hold.",
        ));
    }
    Ok(id.to_owned())
}

fn reason(object: &serde_json::Map<String, Value>) -> Result<String, BusinessError> {
    let reason = object
        .get("reason")
        .and_then(Value::as_str)
        .map(js_trim)
        .ok_or_else(|| invalid("LEGAL_REASON_REQUIRED", "Record the Legal reason."))?;
    if cp_len(reason) < REASON_MIN || cp_len(reason) > REASON_MAX {
        return Err(invalid(
            "LEGAL_REASON_REQUIRED",
            "Record a Legal reason between 3 and 4000 characters.",
        ));
    }
    Ok(reason.to_owned())
}

pub fn plan_command(
    actor: &LegalRecordsActor,
    source: &LegalCommandSource,
    envelope: &LegalCommandEnvelope,
    command: &LegalCommand,
    key: &str,
) -> Result<LegalCommandPlan, BusinessError> {
    let permission = command_permission(match command {
        LegalCommand::AddDocument { .. } => "document",
        LegalCommand::CreateHold { .. } => "hold.create",
        LegalCommand::RequestHoldRelease { .. } => "hold.request-release",
        LegalCommand::ReleaseHold { .. } => "hold.release",
        LegalCommand::Archive { .. } => "transition",
    })?;
    require(actor, permission)?;
    if actor.is_requester() && !matches!(command, LegalCommand::AddDocument { .. }) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "LEGAL_REQUESTER_SCOPE",
            "Requesters can only add their draft question.",
        ));
    }
    if source.item.id != envelope.id || source.item.company_id != actor.company_id.as_str() {
        return Err(not_found());
    }
    if source.item.version != envelope.expected_version {
        return Err(version_conflict());
    }
    if !source.enabled {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "LEGAL_DISABLED",
            "Legal is read-only until enabled for this Company.",
        ));
    }
    if source.frozen {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "LEGAL_CONTENT_FROZEN",
            "Document retention is in progress. Review or resume the preserved manifest before changing content.",
        ));
    }
    let item = &source.item;
    let (action, note, effect, result_document, result_hold, new_state) = match command {
        LegalCommand::AddDocument { name, body } => {
            if !source.editor {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "LEGAL_EDITOR_REQUIRED",
                    "An editor membership is required.",
                ));
            }
            if item.state != "draft" {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "LEGAL_TRANSITION",
                    "New content requires a draft record.",
                ));
            }
            let version = source.next_document_version.max(1);
            let document_id = derived_id("legal-document", key);
            let checksum = sha256_hex(body.as_bytes());
            let bytes = i64::try_from(body.len()).map_err(|_| {
                invalid(
                    "LEGAL_DOCUMENT_BODY",
                    "Use a smaller Legal document version.",
                )
            })?;
            (
                "document".to_owned(),
                format!("Document version {version}"),
                LegalCommandEffect::AddDocument {
                    document_id: document_id.clone(),
                    document_version: version,
                    name: name.clone(),
                    body: body.clone(),
                    checksum,
                    bytes,
                },
                Some(document_id),
                None,
                item.state.clone(),
            )
        }
        LegalCommand::CreateHold { reason } => {
            let hold_id = derived_id("legal-hold", key);
            (
                "hold.create".to_owned(),
                reason.clone(),
                LegalCommandEffect::CreateHold {
                    hold_id: hold_id.clone(),
                    reason: reason.clone(),
                },
                None,
                Some(hold_id),
                item.state.clone(),
            )
        }
        LegalCommand::RequestHoldRelease { hold_id, reason } => {
            let hold = hold(source, hold_id)?;
            if hold.state != "active" {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "LEGAL_HOLD_STATE",
                    "This Legal hold is not active.",
                ));
            }
            (
                "hold.request-release".to_owned(),
                reason.clone(),
                LegalCommandEffect::RequestHoldRelease {
                    hold_id: hold_id.clone(),
                },
                None,
                Some(hold_id.clone()),
                item.state.clone(),
            )
        }
        LegalCommand::ReleaseHold { hold_id, reason } => {
            let hold = hold(source, hold_id)?;
            if hold.state != "release_requested" {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "LEGAL_HOLD_STATE",
                    "This Legal hold is not awaiting release.",
                ));
            }
            if hold.same_person_creator || hold.same_person_release_requester {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "LEGAL_INDEPENDENT_REVIEW",
                    "An independent reviewer must release this hold.",
                ));
            }
            (
                "hold.release".to_owned(),
                reason.clone(),
                LegalCommandEffect::ReleaseHold {
                    hold_id: hold_id.clone(),
                },
                None,
                Some(hold_id.clone()),
                item.state.clone(),
            )
        }
        LegalCommand::Archive { reason } => {
            if !source.editor {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "LEGAL_EDITOR_REQUIRED",
                    "An editor membership is required.",
                ));
            }
            if !allowed_transitions(&item.kind, &item.state).contains(&"archived") {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "LEGAL_TRANSITION",
                    "This Legal record cannot be archived from its current state.",
                ));
            }
            (
                "transition".to_owned(),
                reason.clone(),
                LegalCommandEffect::Archive {
                    reason: reason.clone(),
                },
                None,
                None,
                "archived".to_owned(),
            )
        }
    };
    let next_version = envelope.expected_version.checked_add(1).ok_or_else(|| {
        invalid(
            "LEGAL_VERSION_INVALID",
            "Refresh the Legal record before saving.",
        )
    })?;
    Ok(LegalCommandPlan {
        item_id: envelope.id.clone(),
        expected_version: envelope.expected_version,
        next_version,
        new_state,
        action: action.clone(),
        operation: format!("legal.{action}"),
        note,
        effect,
        result: LegalCommandResult {
            id: envelope.id.clone(),
            version: next_version,
            state: result_state(command, item),
            document_id: result_document,
            hold_id: result_hold,
        },
    })
}

fn result_state(command: &LegalCommand, item: &LegalRecord) -> String {
    match command {
        LegalCommand::Archive { .. } => "archived".to_owned(),
        _ => item.state.clone(),
    }
}

fn hold<'a>(
    source: &'a LegalCommandSource,
    id: &str,
) -> Result<&'a LegalHoldSource, BusinessError> {
    source
        .holds
        .iter()
        .find(|hold| hold.id == id)
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "LEGAL_HOLD_NOT_FOUND",
                "Legal hold is unavailable in your current scope.",
            )
        })
}

pub fn allowed_transitions(kind: &str, state: &str) -> Vec<&'static str> {
    if kind == "contract" {
        match state {
            "approval" => return vec!["draft", "approved", "rejected"],
            "approved" => return vec!["draft"],
            "signing" => return vec!["cancelled"],
            "executed" => return vec!["active"],
            "active" => return vec!["expired", "terminated"],
            _ => {}
        }
    }
    TRANSITIONS
        .iter()
        .find(|(current, _)| *current == state)
        .map(|(_, states)| states.to_vec())
        .unwrap_or_default()
}

pub fn derived_id(prefix: &str, key: &str) -> String {
    format!("{prefix}-{}", &sha256_hex(key.as_bytes())[..32])
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

pub(crate) fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

pub(crate) fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "LEGAL_NOT_FOUND",
        "Legal record is unavailable in your current scope.",
    )
}

pub(crate) fn version_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "LEGAL_VERSION_CONFLICT",
        "This record changed. Refresh before saving.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn actor(role: &str, permissions: &[&str]) -> LegalRecordsActor {
        LegalRecordsActor {
            holding_id: HoldingId::new("holding").unwrap(),
            company_id: CompanyId::new("company").unwrap(),
            user_id: UserId::new("user").unwrap(),
            assignment_id: erpx_server_shared::public::AssignmentId::new("assignment").unwrap(),
            branch_id: None,
            department_id: None,
            role: role.into(),
            permissions: permissions
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
        }
    }

    fn item(state: &str) -> LegalRecord {
        LegalRecord {
            id: "item".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            kind: "contract".into(),
            title: "Synthetic contract".into(),
            description: String::new(),
            classification: "internal".into(),
            country: "TH".into(),
            jurisdiction: "Thailand".into(),
            language: "th".into(),
            currency: "THB".into(),
            timezone: "Asia/Bangkok".into(),
            priority: "normal".into(),
            due_date: None,
            owner_id: "user".into(),
            created_by: "user".into(),
            state: state.into(),
            version: 1,
            created_at: 0,
            updated_at: 0,
            data: json!({}),
        }
    }

    fn source(state: &str) -> LegalCommandSource {
        LegalCommandSource {
            item: item(state),
            enabled: true,
            editor: true,
            frozen: false,
            holds: vec![
                LegalHoldSource {
                    id: "hold-active".into(),
                    state: "active".into(),
                    created_by: "user".into(),
                    release_requested_by: None,
                    same_person_creator: true,
                    same_person_release_requester: false,
                },
                LegalHoldSource {
                    id: "hold-requested".into(),
                    state: "release_requested".into(),
                    created_by: "user".into(),
                    release_requested_by: Some("requester".into()),
                    same_person_creator: true,
                    same_person_release_requester: false,
                },
            ],
            next_document_version: 2,
        }
    }

    #[test]
    fn register_query_rejects_unknown_kinds_and_oversized_inputs() {
        assert!(LegalRegisterQuery::parse(Some("contract"), "", "", 1, false).is_ok());
        assert_eq!(
            LegalRegisterQuery::parse(Some("unknown"), "", "", 1, false)
                .unwrap_err()
                .code,
            "LEGAL_KIND_INVALID"
        );
        assert_eq!(
            LegalRegisterQuery::parse(None, &"a".repeat(201), "", 1, false)
                .unwrap_err()
                .code,
            "LEGAL_SEARCH_INVALID"
        );
        assert_eq!(
            LegalRegisterQuery::parse(None, "", "", 0, false)
                .unwrap_err()
                .code,
            "LEGAL_PAGE_INVALID"
        );
    }

    #[test]
    fn register_query_counts_chars_for_search_and_state() {
        // 200 Thai scalars: 200 chars (allowed), 600 UTF-8 bytes.
        assert!(LegalRegisterQuery::parse(None, &"ก".repeat(200), "", 1, false).is_ok());
        assert_eq!(
            LegalRegisterQuery::parse(None, &"ก".repeat(201), "", 1, false)
                .unwrap_err()
                .code,
            "LEGAL_SEARCH_INVALID"
        );
        // 200 astral scalars: 200 chars (allowed), 800 UTF-8 bytes.
        assert!(LegalRegisterQuery::parse(None, &"😀".repeat(200), "", 1, false).is_ok());
        assert_eq!(
            LegalRegisterQuery::parse(None, &"😀".repeat(201), "", 1, false)
                .unwrap_err()
                .code,
            "LEGAL_SEARCH_INVALID"
        );
        assert!(LegalRegisterQuery::parse(None, "", &"ก".repeat(40), 1, false).is_ok());
        assert_eq!(
            LegalRegisterQuery::parse(None, "", &"ก".repeat(41), 1, false)
                .unwrap_err()
                .code,
            "LEGAL_STATE_INVALID"
        );
    }

    #[test]
    fn command_envelope_rejects_unknown_actions_and_versions() {
        assert_eq!(
            parse_envelope(&json!({"action":"delete","id":"item","expectedVersion":1}))
                .unwrap_err()
                .code,
            "LEGAL_ACTION_UNSUPPORTED"
        );
        assert_eq!(
            parse_envelope(&json!({"action":"document","id":"item"}))
                .unwrap_err()
                .code,
            "LEGAL_VERSION_REQUIRED"
        );
        assert_eq!(
            parse_envelope(
                &json!({"action":"transition","id":"item","expectedVersion":1,"state":"active"})
            )
            .unwrap_err()
            .code,
            "LEGAL_ACTION_UNSUPPORTED"
        );
    }

    #[test]
    fn document_command_requires_an_editor_a_draft_and_bounded_content() {
        let raw = json!({"action":"document","id":"item","expectedVersion":1,"name":"Agreement","body":"Signed text"});
        let envelope = parse_envelope(&raw).unwrap();
        let command = parse_command(&raw, &envelope).unwrap();
        assert_eq!(
            plan_command(
                &actor("legal_counsel", &["legal.read", "legal.edit"]),
                &source("draft"),
                &envelope,
                &command,
                "legal-command-key-0001"
            )
            .unwrap()
            .effect,
            LegalCommandEffect::AddDocument {
                document_id: derived_id("legal-document", "legal-command-key-0001"),
                document_version: 2,
                name: "Agreement".into(),
                body: "Signed text".into(),
                checksum: sha256_hex(b"Signed text"),
                bytes: 11,
            }
        );
        let mut locked = source("draft");
        locked.editor = false;
        assert_eq!(
            plan_command(
                &actor("legal_counsel", &["legal.read", "legal.edit"]),
                &locked,
                &envelope,
                &command,
                "legal-command-key-0001"
            )
            .unwrap_err()
            .code,
            "LEGAL_EDITOR_REQUIRED"
        );
        assert_eq!(
            plan_command(
                &actor("legal_counsel", &["legal.read", "legal.edit"]),
                &source("approved"),
                &envelope,
                &command,
                "legal-command-key-0001"
            )
            .unwrap_err()
            .code,
            "LEGAL_TRANSITION"
        );
        let oversized = json!({"action":"document","id":"item","expectedVersion":1,"name":"Agreement","body":"a".repeat(DOCUMENT_BODY_LIMIT + 1)});
        let oversized_envelope = parse_envelope(&oversized).unwrap();
        assert_eq!(
            parse_command(&oversized, &oversized_envelope)
                .unwrap_err()
                .code,
            "LEGAL_DOCUMENT_BODY"
        );
    }

    #[test]
    fn document_name_counts_chars() {
        let thai_name = "ก".repeat(200);
        let raw = json!({"action":"document","id":"item","expectedVersion":1,"name":thai_name,"body":"Signed text"});
        let envelope = parse_envelope(&raw).unwrap();
        assert!(parse_command(&raw, &envelope).is_ok());
        let oversized = json!({"action":"document","id":"item","expectedVersion":1,"name":"ก".repeat(201),"body":"Signed text"});
        let oversized_envelope = parse_envelope(&oversized).unwrap();
        assert_eq!(
            parse_command(&oversized, &oversized_envelope)
                .unwrap_err()
                .code,
            "LEGAL_DOCUMENT_NAME"
        );
    }

    #[test]
    fn reason_uses_js_trim_and_char_length_parity() {
        let reason_for = |reason: &str| {
            let raw =
                json!({"action":"hold.create","id":"item","expectedVersion":1,"reason":reason});
            let envelope = parse_envelope(&raw).unwrap();
            match parse_command(&raw, &envelope) {
                Ok(LegalCommand::CreateHold { reason }) => Ok(reason),
                Ok(_) => unreachable!("fixture uses hold.create"),
                Err(error) => Err(error.code),
            }
        };
        // JS trim strips ASCII spaces: "  abc  " -> "abc" (3 chars) passes.
        assert_eq!(reason_for("  abc  "), Ok("abc".to_owned()));
        // 4000 Thai chars: 4000 chars (allowed), 12000 bytes.
        assert_eq!(reason_for(&"ก".repeat(4000)), Ok("ก".repeat(4000)));
        assert_eq!(reason_for(&"ก".repeat(4001)), Err("LEGAL_REASON_REQUIRED"));
        // JS trim strips ZWNBSP (U+FEFF): padding must not inflate length.
        assert_eq!(
            reason_for("\u{FEFF}ab\u{FEFF}"),
            Err("LEGAL_REASON_REQUIRED")
        );
        // JS trim does not strip NEL (U+0085): those units count.
        assert_eq!(
            reason_for("\u{0085}ab\u{0085}"),
            Ok("\u{0085}ab\u{0085}".to_owned())
        );
        // Astral text: 3 chars passes the 3-char minimum,
        // while 2 chars fails.
        assert_eq!(reason_for("😀😀😀"), Ok("😀😀😀".to_owned()));
        assert_eq!(reason_for("😀😀"), Err("LEGAL_REASON_REQUIRED"));
    }

    #[test]
    fn hold_release_requires_an_independent_reviewer_and_the_requested_state() {
        let raw = json!({"action":"hold.release","id":"item","expectedVersion":1,"holdId":"hold-requested","reason":"Independent release"});
        let envelope = parse_envelope(&raw).unwrap();
        let command = parse_command(&raw, &envelope).unwrap();
        assert_eq!(
            plan_command(
                &actor("legal_manager", &["legal.read", "legal.hold.release"]),
                &source("active"),
                &envelope,
                &command,
                "legal-command-key-0002"
            )
            .unwrap_err()
            .code,
            "LEGAL_INDEPENDENT_REVIEW"
        );
        let mut independent = source("active");
        independent.holds[1].same_person_creator = false;
        independent.holds[1].same_person_release_requester = false;
        assert_eq!(
            plan_command(
                &actor("legal_manager", &["legal.read", "legal.hold.release"]),
                &independent,
                &envelope,
                &command,
                "legal-command-key-0002"
            )
            .unwrap()
            .result
            .hold_id
            .as_deref(),
            Some("hold-requested")
        );
        let early = json!({"action":"hold.release","id":"item","expectedVersion":1,"holdId":"hold-active","reason":"Independent release"});
        assert_eq!(
            parse_command(&early, &parse_envelope(&early).unwrap())
                .and_then(|command| {
                    plan_command(
                        &actor("legal_manager", &["legal.read", "legal.hold.release"]),
                        &independent,
                        &envelope,
                        &command,
                        "legal-command-key-0002",
                    )
                })
                .unwrap_err()
                .code,
            "LEGAL_HOLD_STATE"
        );
    }

    #[test]
    fn archive_preserves_contract_transitions_and_requester_scope() {
        let raw = json!({"action":"transition","id":"item","expectedVersion":1,"state":"archived","reason":"Expired retention"});
        let envelope = parse_envelope(&raw).unwrap();
        let command = parse_command(&raw, &envelope).unwrap();
        assert_eq!(
            plan_command(
                &actor("legal_manager", &["legal.read", "legal.edit"]),
                &source("expired"),
                &envelope,
                &command,
                "legal-command-key-0003"
            )
            .unwrap()
            .result
            .state,
            "archived"
        );
        assert_eq!(
            plan_command(
                &actor("legal_manager", &["legal.read", "legal.edit"]),
                &source("approved"),
                &envelope,
                &command,
                "legal-command-key-0003"
            )
            .unwrap_err()
            .code,
            "LEGAL_TRANSITION"
        );
        assert_eq!(
            plan_command(
                &actor("legal_requester", &["legal.read", "legal.edit"]),
                &source("expired"),
                &envelope,
                &command,
                "legal-command-key-0003"
            )
            .unwrap_err()
            .code,
            "LEGAL_REQUESTER_SCOPE"
        );
    }

    #[test]
    fn command_rejects_stale_versions_and_frozen_content() {
        let raw = json!({"action":"hold.create","id":"item","expectedVersion":2,"reason":"Preserve evidence"});
        let envelope = parse_envelope(&raw).unwrap();
        let command = parse_command(&raw, &envelope).unwrap();
        assert_eq!(
            plan_command(
                &actor("legal_manager", &["legal.read", "legal.hold"]),
                &source("active"),
                &envelope,
                &command,
                "legal-command-key-0004"
            )
            .unwrap_err()
            .code,
            "LEGAL_VERSION_CONFLICT"
        );
        let mut frozen = source("active");
        frozen.frozen = true;
        let envelope = LegalCommandEnvelope {
            expected_version: 1,
            ..envelope
        };
        assert_eq!(
            plan_command(
                &actor("legal_manager", &["legal.read", "legal.hold"]),
                &frozen,
                &envelope,
                &command,
                "legal-command-key-0004"
            )
            .unwrap_err()
            .code,
            "LEGAL_CONTENT_FROZEN"
        );
    }
}
