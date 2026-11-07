//! Agent governance audit-read facts.
//!
//! The projection exposes immutable invocation evidence and private-session
//! retention controls for one Company. It is a read fact, not a command
//! receipt, assignment grant or provider capability. Session rows are private
//! to their user unless the current role grants `ai.manage`.
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::Serialize;

pub const INVOCATION_LIMIT: usize = 50;
pub const SESSION_LIMIT: usize = 100;
/// Response-path identifier bound. The retained TypeScript transport validator
/// (`native-agent-evidence.ts` `identifier`) allows at most 200 UTF-8 bytes, so
/// this matches `RecordId` and the client render check instead of a looser 300.
const ID_LIMIT: usize = 200;
/// Response-path conversation-key bound (`text(value, 400)` on the client).
const CONVERSATION_KEY_LIMIT: usize = 400;
const DIGEST_LIMIT: usize = 64;
const ERROR_LIMIT: usize = 2_000;

#[derive(Clone, Debug)]
pub struct AgentEvidenceActor {
    pub user_id: String,
    pub company_id: String,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    /// Epoch milliseconds at which the selected work context expires.
    pub expires_at: i64,
}

impl AgentEvidenceActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }
    pub fn can_manage(&self) -> bool {
        self.has("ai.manage")
    }
    pub fn require_read(&self, now: i64) -> Result<(), BusinessError> {
        if !self.has("records.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission.",
            ));
        }
        if !self.modules.iter().any(|module| module == "agent") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MODULE_FORBIDDEN",
                "Agent governance is unavailable to the selected role.",
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
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct AgentInvocationEvidence {
    pub id: String,
    pub session_id: String,
    pub turn_id: String,
    pub company_id: String,
    pub user_id: String,
    pub agent_version_id: Option<String>,
    pub question_digest: String,
    pub sources_digest: String,
    pub outcome: String,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub error: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct AgentPrivateSession {
    pub id: String,
    pub company_id: String,
    pub user_id: String,
    pub assignment_id: String,
    pub conversation_key: String,
    pub generation: i64,
    pub created_at: i64,
    pub status: String,
    pub expires_at: i64,
    pub legal_hold: bool,
    pub hold_reason: Option<String>,
    pub version: i64,
    pub last_used_at: i64,
    pub updated_at: i64,
    pub effective_status: String,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct AgentEvidenceWorkspace {
    pub now: i64,
    pub invocations: Vec<AgentInvocationEvidence>,
    pub sessions: Vec<AgentPrivateSession>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentEvidenceKind {
    Invocation,
    Session,
}

impl AgentEvidenceKind {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "invocation" => Ok(Self::Invocation),
            "session" => Ok(Self::Session),
            _ => Err(request_invalid()),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Invocation => "invocation",
            Self::Session => "session",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentEvidenceQuery {
    pub kind: AgentEvidenceKind,
    pub id: String,
}

impl AgentEvidenceQuery {
    pub fn parse(kind: &str, id: &str) -> Result<Self, BusinessError> {
        Ok(Self {
            kind: AgentEvidenceKind::parse(kind)?,
            id: RecordId::new(id.to_owned())
                .map(String::from)
                .map_err(|_| request_invalid())?,
        })
    }
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvidenceRecord {
    Invocation { record: AgentInvocationEvidence },
    Session { record: AgentPrivateSession },
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "AGENT_EVIDENCE_REQUEST_INVALID",
        "Use one bounded Agent evidence request.",
    )
}

pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "AGENT_EVIDENCE_SOURCE_INVALID",
        "The current Agent evidence source is invalid.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "AGENT_RECORD_NOT_FOUND",
        "Agent record not found in this work scope.",
    )
}

/// Retained TypeScript transport `text()`: UTF-8 byte length plus rejection of
/// `[\u0000-\u001f\u007f]` (and only that set; C1 controls U+0080-U+009F pass
/// on the client, unlike Rust `char::is_control`). Byte length is deliberate:
/// unlike the Zod command schemas, this response boundary uses `TextEncoder`
/// byte counts, so no UTF-16 conversion applies here.
fn transport_text(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && !value
            .chars()
            .any(|character| matches!(character, '\u{0000}'..='\u{001f}' | '\u{007f}'))
}

fn identifier_text(value: &str) -> bool {
    transport_text(value, ID_LIMIT)
}

fn optional_identifier(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(identifier_text)
}

/// Retained TypeScript transport `messageText()`: UTF-8 byte length plus
/// rejection of `[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]`. Tab, LF and CR
/// stay allowed so multi-line hold reasons and provider errors are not lost.
fn message_text(value: &str) -> bool {
    value.len() <= ERROR_LIMIT
        && !value.chars().any(|character| {
            matches!(
                character,
                '\u{0000}'..='\u{0008}'
                    | '\u{000b}'
                    | '\u{000c}'
                    | '\u{000e}'..='\u{001f}'
                    | '\u{007f}'
            )
        })
}

fn optional_message_text(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(message_text)
}

fn digest(value: &str) -> bool {
    value.len() == DIGEST_LIMIT
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn timestamp(value: i64) -> bool {
    value >= 0
}

fn optional_timestamp(value: Option<i64>) -> bool {
    value.is_none_or(timestamp)
}

pub fn validate_invocation(
    actor: &AgentEvidenceActor,
    row: AgentInvocationEvidence,
) -> Result<AgentInvocationEvidence, BusinessError> {
    if row.company_id != actor.company_id
        || !identifier_text(&row.id)
        || !identifier_text(&row.session_id)
        || !identifier_text(&row.turn_id)
        || !identifier_text(&row.user_id)
        || !optional_identifier(&row.agent_version_id)
        || !digest(&row.question_digest)
        || !digest(&row.sources_digest)
        || !matches!(row.outcome.as_str(), "succeeded" | "failed")
        || !optional_timestamp(row.input_tokens)
        || !optional_timestamp(row.output_tokens)
        || !optional_message_text(&row.error)
        || !timestamp(row.created_at)
    {
        return Err(source_invalid());
    }
    Ok(row)
}

fn effective_status(status: &str, expires_at: i64, now: i64) -> String {
    if status == "active" && expires_at <= now {
        "expired".to_owned()
    } else {
        status.to_owned()
    }
}

pub fn validate_session(
    actor: &AgentEvidenceActor,
    mut row: AgentPrivateSession,
    now: i64,
    manage: bool,
) -> Result<AgentPrivateSession, BusinessError> {
    if row.company_id != actor.company_id
        || (!manage && row.user_id != actor.user_id)
        || !identifier_text(&row.id)
        || !identifier_text(&row.user_id)
        || !identifier_text(&row.assignment_id)
        || !transport_text(&row.conversation_key, CONVERSATION_KEY_LIMIT)
        || row.generation < 1
        || row.version < 1
        || !matches!(row.status.as_str(), "active" | "expired")
        || !timestamp(row.created_at)
        || !timestamp(row.expires_at)
        || !timestamp(row.last_used_at)
        || !timestamp(row.updated_at)
        || row.legal_hold != row.hold_reason.is_some()
        || !optional_message_text(&row.hold_reason)
    {
        return Err(source_invalid());
    }
    row.effective_status = effective_status(&row.status, row.expires_at, now);
    Ok(row)
}

fn unique<T: AsRef<str>>(values: impl Iterator<Item = T>) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    values
        .into_iter()
        .all(|value| seen.insert(value.as_ref().to_owned()))
}

pub fn validate_workspace(
    actor: &AgentEvidenceActor,
    now: i64,
    mut invocations: Vec<AgentInvocationEvidence>,
    mut sessions: Vec<AgentPrivateSession>,
) -> Result<AgentEvidenceWorkspace, BusinessError> {
    // The TypeScript workspace check requires `now` as an integer of at least
    // 1, so a non-positive caller clock fails closed instead of emitting an
    // envelope the retained client would reject.
    if now < 1 || invocations.len() > INVOCATION_LIMIT || sessions.len() > SESSION_LIMIT {
        return Err(source_invalid());
    }
    invocations = invocations
        .into_iter()
        .map(|row| validate_invocation(actor, row))
        .collect::<Result<_, _>>()?;
    sessions = sessions
        .into_iter()
        .map(|row| validate_session(actor, row, now, actor.can_manage()))
        .collect::<Result<_, _>>()?;
    if !unique(invocations.iter().map(|row| row.id.as_str()))
        || !unique(sessions.iter().map(|row| row.id.as_str()))
    {
        return Err(source_invalid());
    }
    invocations.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    sessions.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(AgentEvidenceWorkspace {
        now,
        invocations,
        sessions,
    })
}

pub fn validate_record(
    actor: &AgentEvidenceActor,
    kind: AgentEvidenceKind,
    row: AgentEvidenceRecord,
    now: i64,
) -> Result<AgentEvidenceRecord, BusinessError> {
    match (kind, row) {
        (AgentEvidenceKind::Invocation, AgentEvidenceRecord::Invocation { record }) => {
            Ok(AgentEvidenceRecord::Invocation {
                record: validate_invocation(actor, record)?,
            })
        }
        (AgentEvidenceKind::Session, AgentEvidenceRecord::Session { record }) => {
            Ok(AgentEvidenceRecord::Session {
                record: validate_session(actor, record, now, actor.can_manage())?,
            })
        }
        _ => Err(source_invalid()),
    }
}
