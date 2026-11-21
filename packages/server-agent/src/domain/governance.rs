//! Agent governance rows, wire inputs and retained decision rules.
//!
//! The retained TypeScript owner accepted these commands through strict Zod
//! objects: strings are trimmed before their bounds are checked, versions are
//! positive integers, a version carries one or more source paths, an
//! assignment names exactly one pool or one user, and lifecycle actions are
//! valid only for their record kind. This module preserves those accepted
//! values and their canonical JSON field order; it performs no I/O.
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const MIN_REASON_UNITS: usize = 3;
pub const MAX_REASON_UNITS: usize = 3000;
pub const MIN_NAME_UNITS: usize = 2;
pub const MAX_NAME_UNITS: usize = 120;
pub const MAX_SOURCE_PATH_UNITS: usize = 300;
pub const MAX_SOURCES: usize = 30;
pub const MAX_POOL_MEMBERS: usize = 100;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

fn validation(message: &'static str) -> BusinessError {
    BusinessError::invalid("VALIDATION_FAILED", message)
}

/// ECMAScript string length used by the retained Zod bounds.
pub fn js_len(value: &str) -> usize {
    value.encode_utf16().count()
}

fn integer(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number.as_i64().or_else(|| {
            let value = number.as_f64()?;
            (value.is_finite() && value.fract() == 0.0 && value.abs() <= MAX_SAFE_INTEGER as f64)
                .then_some(value as i64)
        }),
        _ => None,
    }
}

fn positive(value: Option<&Value>, message: &'static str) -> Result<i64, BusinessError> {
    let value = integer(value).ok_or_else(|| validation(message))?;
    if value < 1 {
        return Err(validation(message));
    }
    Ok(value)
}

fn text<'a>(value: Option<&'a Value>, message: &'static str) -> Result<&'a str, BusinessError> {
    value
        .and_then(Value::as_str)
        .ok_or_else(|| validation(message))
}

fn object<'a>(
    value: &'a Value,
    message: &'static str,
) -> Result<&'a Map<String, Value>, BusinessError> {
    value.as_object().ok_or_else(|| validation(message))
}

fn optional_text(
    value: Option<&Value>,
    message: &'static str,
) -> Result<Option<String>, BusinessError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value.clone())),
        _ => Err(validation(message)),
    }
}

/// The retained `reason` value: trimmed and bounded.
pub fn parse_reason(value: Option<&Value>, message: &'static str) -> Result<String, BusinessError> {
    let value = text(value, message)?.trim();
    let units = js_len(value);
    if !(MIN_REASON_UNITS..=MAX_REASON_UNITS).contains(&units) {
        return Err(validation(message));
    }
    Ok(value.to_owned())
}

/// A bounded assistant page path. The retained owner normalised page paths
/// through the workspace projection; this owner keeps only the canonical
/// absolute-path fact it stores, rejecting query strings for assignment scope.
fn parse_source_path(value: Option<&Value>, allow_query: bool) -> Result<String, BusinessError> {
    let value = text(value, "Choose an available assistant source path.")?;
    let units = js_len(value);
    if units == 0
        || units > MAX_SOURCE_PATH_UNITS
        || !value.starts_with('/')
        || value.chars().any(char::is_control)
        || value.chars().any(char::is_whitespace)
        || (!allow_query && value.contains('?'))
    {
        return Err(validation("Choose an available assistant source path."));
    }
    Ok(value.to_owned())
}

/// The retained `agentSourceAllowed` prefix rule. No I/O and no authority.
pub fn agent_source_allowed(path: &str, allowed_sources: &[String]) -> bool {
    allowed_sources.iter().any(|scope| {
        scope == "/"
            || path == scope
            || (!scope.contains('?')
                && (path.starts_with(&format!("{scope}/"))
                    || path.starts_with(&format!("{scope}?"))))
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInput {
    pub name: String,
    pub purpose: String,
}

impl AgentInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let root = object(value, "Provide an Agent name and purpose.")?;
        let name = text(root.get("name"), "Provide an Agent name.")?.trim();
        let name_units = js_len(name);
        if !(MIN_NAME_UNITS..=MAX_NAME_UNITS).contains(&name_units) {
            return Err(validation("Use an Agent name of 2 to 120 characters."));
        }
        Ok(Self {
            name: name.to_owned(),
            purpose: parse_reason(root.get("purpose"), "Provide an Agent purpose.")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentVersionInput {
    pub agent_id: String,
    pub prompt: String,
    pub allowed_sources: Vec<String>,
}

impl AgentVersionInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let root = object(value, "Provide an Agent version.")?;
        let agent_id = text(root.get("agentId"), "Choose an Agent.")?;
        if agent_id.is_empty() {
            return Err(validation("Choose an Agent."));
        }
        let prompt = parse_reason(root.get("prompt"), "Provide an Agent prompt.")?;
        let values = root
            .get("allowedSources")
            .and_then(Value::as_array)
            .ok_or_else(|| validation("Choose 1 to 30 assistant source paths."))?;
        if values.is_empty() || values.len() > MAX_SOURCES {
            return Err(validation("Choose 1 to 30 assistant source paths."));
        }
        let allowed_sources = values
            .iter()
            .map(|value| parse_source_path(Some(value), true))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            agent_id: agent_id.to_owned(),
            prompt,
            allowed_sources,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPoolInput {
    pub name: String,
    pub user_ids: Vec<String>,
}

impl AgentPoolInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let root = object(value, "Provide an Agent pool and its members.")?;
        let name = text(root.get("name"), "Provide an Agent pool name.")?.trim();
        let name_units = js_len(name);
        if !(MIN_NAME_UNITS..=MAX_NAME_UNITS).contains(&name_units) {
            return Err(validation("Use an Agent pool name of 2 to 120 characters."));
        }
        let values = root
            .get("userIds")
            .and_then(Value::as_array)
            .ok_or_else(|| validation("Choose 1 to 100 eligible pool members."))?;
        if values.is_empty() || values.len() > MAX_POOL_MEMBERS {
            return Err(validation("Choose 1 to 100 eligible pool members."));
        }
        let mut user_ids = Vec::new();
        for value in values {
            let id = text(Some(value), "Choose an eligible pool member.")?;
            if id.is_empty() {
                return Err(validation("Choose an eligible pool member."));
            }
            user_ids.push(id.to_owned());
        }
        Ok(Self {
            name: name.to_owned(),
            user_ids,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAssignmentInput {
    pub agent_version_id: String,
    pub pool_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    pub scope_path: String,
}

impl AgentAssignmentInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let root = object(value, "Provide an Agent assignment.")?;
        let agent_version_id = text(
            root.get("agentVersionId"),
            "Choose an approved Agent version.",
        )?;
        if agent_version_id.is_empty() {
            return Err(validation("Choose an approved Agent version."));
        }
        let pool_id = optional_text(root.get("poolId"), "Choose one pool or one user.")?;
        let user_id = optional_text(root.get("userId"), "Choose one pool or one user.")?;
        if pool_id.is_some() == user_id.is_some() {
            return Err(validation("Choose one pool or one user."));
        }
        Ok(Self {
            agent_version_id: agent_version_id.to_owned(),
            pool_id,
            user_id,
            scope_path: parse_source_path(root.get("scopePath"), false)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAssignmentDelegateInput {
    pub expected_version: i64,
    pub pool_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    pub scope_path: String,
    pub reason: String,
}

impl AgentAssignmentDelegateInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let root = object(value, "Provide an Agent delegation.")?;
        let pool_id = optional_text(root.get("poolId"), "Choose one pool or one user.")?;
        let user_id = optional_text(root.get("userId"), "Choose one pool or one user.")?;
        if pool_id.is_some() == user_id.is_some() {
            return Err(validation("Choose one pool or one user."));
        }
        Ok(Self {
            expected_version: positive(
                root.get("expectedVersion"),
                "Use the current assignment version.",
            )?,
            pool_id,
            user_id,
            scope_path: parse_source_path(root.get("scopePath"), false)?,
            reason: parse_reason(root.get("reason"), "Provide a delegation reason.")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAssignmentActionInput {
    pub expected_version: i64,
    pub reason: String,
}

impl AgentAssignmentActionInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let root = object(value, "Provide an assignment action and reason.")?;
        Ok(Self {
            expected_version: positive(
                root.get("expectedVersion"),
                "Use the current assignment version.",
            )?,
            reason: parse_reason(root.get("reason"), "Provide a change reason.")?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PoolAction {
    ReplaceMembers,
    Retire,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPoolChange {
    pub action: PoolAction,
    pub expected_version: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub user_ids: Vec<String>,
    pub reason: String,
}

impl AgentPoolChange {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let root = object(value, "Choose a pool action.")?;
        let action = match text(root.get("action"), "Choose a pool action.")? {
            "replace-members" => PoolAction::ReplaceMembers,
            "retire" => PoolAction::Retire,
            _ => return Err(validation("Choose a pool action.")),
        };
        let expected_version =
            positive(root.get("expectedVersion"), "Use the current pool version.")?;
        let reason = parse_reason(root.get("reason"), "Provide a pool change reason.")?;
        let mut user_ids = Vec::new();
        if action == PoolAction::ReplaceMembers {
            let values = root
                .get("userIds")
                .and_then(Value::as_array)
                .ok_or_else(|| validation("Choose 1 to 100 eligible pool members."))?;
            if values.is_empty() || values.len() > MAX_POOL_MEMBERS {
                return Err(validation("Choose 1 to 100 eligible pool members."));
            }
            for value in values {
                let id = text(Some(value), "Choose an eligible pool member.")?;
                if id.is_empty() {
                    return Err(validation("Choose an eligible pool member."));
                }
                user_ids.push(id.to_owned());
            }
        }
        Ok(Self {
            action,
            expected_version,
            user_ids,
            reason,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecordKind {
    Agent,
    Version,
    Assignment,
}

impl RecordKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Version => "version",
            Self::Assignment => "assignment",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleAction {
    Approve,
    Reject,
    Retire,
    Revoke,
}

impl LifecycleAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Retire => "retire",
            Self::Revoke => "revoke",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "approve" => Some(Self::Approve),
            "reject" => Some(Self::Reject),
            "retire" => Some(Self::Retire),
            "revoke" => Some(Self::Revoke),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLifecycleInput {
    pub action: LifecycleAction,
    pub expected_version: i64,
    pub reason: String,
}

impl AgentLifecycleInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let root = object(value, "Choose a lifecycle action and reason.")?;
        Ok(Self {
            action: LifecycleAction::parse(text(root.get("action"), "Choose a lifecycle action.")?)
                .ok_or_else(|| {
                    validation("Choose an available lifecycle action for this record.")
                })?,
            expected_version: positive(
                root.get("expectedVersion"),
                "Use the current record version.",
            )?,
            reason: parse_reason(root.get("reason"), "Provide a change reason.")?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrivateSessionAction {
    Hold,
    ReleaseHold,
    Expire,
}

impl PrivateSessionAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::ReleaseHold => "release-hold",
            Self::Expire => "expire",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "hold" => Some(Self::Hold),
            "release-hold" => Some(Self::ReleaseHold),
            "expire" => Some(Self::Expire),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateSessionInput {
    pub action: PrivateSessionAction,
    pub expected_version: i64,
    pub reason: String,
}

impl PrivateSessionInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let root = object(value, "Choose a private-session action and reason.")?;
        Ok(Self {
            action: PrivateSessionAction::parse(text(
                root.get("action"),
                "Choose a private-session action.",
            )?)
            .ok_or_else(|| validation("Choose a private-session action."))?,
            expected_version: positive(
                root.get("expectedVersion"),
                "Use the current private-session version.",
            )?,
            reason: parse_reason(root.get("reason"), "Provide a session change reason.")?,
        })
    }
}

/// The retained lifecycle transition table. Only assignments can be revoked,
/// only agents/versions can be approved, rejected or retired, and approval or
/// rejection starts from a draft.
pub fn lifecycle_transition(
    kind: RecordKind,
    action: LifecycleAction,
) -> Option<(&'static str, &'static str)> {
    match (kind, action) {
        (RecordKind::Agent | RecordKind::Version, LifecycleAction::Approve) => {
            Some(("draft", "approved"))
        }
        (RecordKind::Agent | RecordKind::Version, LifecycleAction::Reject) => {
            Some(("draft", "rejected"))
        }
        (RecordKind::Agent | RecordKind::Version, LifecycleAction::Retire) => {
            Some(("approved", "retired"))
        }
        (RecordKind::Assignment, LifecycleAction::Revoke) => Some(("active", "revoked")),
        _ => None,
    }
}

/// Approval or rejection needs a different person from the record creator.
pub fn assert_independent_review(
    created_by: &str,
    actor: &str,
    action: LifecycleAction,
) -> Result<(), BusinessError> {
    if created_by == actor && matches!(action, LifecycleAction::Approve | LifecycleAction::Reject) {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "CONFLICT",
            "Independent review is required.",
        ));
    }
    Ok(())
}

/// One effective assistant governance fact, matching the retained shape. The
/// private session identifier is intentionally separate from the approved
/// policy snapshot so staleness can be compared without it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantPolicy {
    pub assignment_id: Option<String>,
    pub assignment_version: Option<i64>,
    pub agent_version_id: Option<String>,
    pub agent_version_revision: Option<i64>,
    pub catalog_version: Option<i64>,
    pub pool_id: Option<String>,
    pub pool_version: Option<i64>,
    pub prompt: Option<String>,
    pub allowed_sources: Vec<String>,
}

impl AssistantPolicy {
    pub fn manual() -> Self {
        Self {
            assignment_id: None,
            assignment_version: None,
            agent_version_id: None,
            agent_version_revision: None,
            catalog_version: None,
            pool_id: None,
            pool_version: None,
            prompt: None,
            allowed_sources: vec!["/".into()],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantGovernance {
    pub session_id: String,
    #[serde(flatten)]
    pub policy: AssistantPolicy,
}

/// A row from the current approved routing candidates, in retained order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyCandidate {
    pub assignment_id: String,
    pub assignment_version: i64,
    pub scope_path: String,
    pub pool_id: Option<String>,
    pub agent_version_id: String,
    pub agent_version_revision: i64,
    pub prompt: String,
    pub allowed_sources: Vec<String>,
    pub agent_version_status: String,
    pub catalog_version: i64,
    pub catalog_status: String,
    pub pool_version: Option<i64>,
    pub pool_status: Option<String>,
}

/// The retained routing decision: the longest approved assignment scope that
/// covers the page, or the manual default when no candidate covers it.
pub fn assistant_policy(
    candidates: &[PolicyCandidate],
    path: &str,
) -> Result<AssistantPolicy, BusinessError> {
    let Some(row) = candidates
        .iter()
        .find(|candidate| agent_source_allowed(path, std::slice::from_ref(&candidate.scope_path)))
    else {
        return Ok(AssistantPolicy::manual());
    };
    if row.agent_version_status != "approved"
        || row.catalog_status != "approved"
        || row.pool_id.is_some() && row.pool_status.as_deref() != Some("active")
    {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "AI_AGENT_UNAVAILABLE",
            "This assigned agent is no longer approved or its pool is unavailable. Continue with the manual workflow guide.",
        ));
    }
    Ok(AssistantPolicy {
        assignment_id: Some(row.assignment_id.clone()),
        assignment_version: Some(row.assignment_version),
        agent_version_id: Some(row.agent_version_id.clone()),
        agent_version_revision: Some(row.agent_version_revision),
        catalog_version: Some(row.catalog_version),
        pool_id: row.pool_id.clone(),
        pool_version: if row.pool_id.is_some() {
            row.pool_version
        } else {
            None
        },
        prompt: Some(row.prompt.clone()),
        allowed_sources: row.allowed_sources.clone(),
    })
}

/// Compare the caller's policy snapshot with the current decision. The
/// retained owner compared the canonical JSON of the same approved facts.
pub fn assert_current_assistant_policy(
    current: &AssistantPolicy,
    supplied: &AssistantPolicy,
) -> Result<(), BusinessError> {
    if current != supplied {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "AI_AGENT_CHANGED",
            "The approved agent or its assignment changed. Continue manually or ask again.",
        ));
    }
    Ok(())
}

pub fn session_expired() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        "AI_SESSION_EXPIRED",
        "This private session expired. Ask again to start a new session; retained evidence stays private.",
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordState {
    pub status: String,
    pub version: i64,
    pub created_by: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssignmentState {
    pub status: String,
    pub version: i64,
    pub agent_version_id: String,
    pub pool_id: Option<String>,
    pub user_id: Option<String>,
    pub scope_path: String,
}

impl AssignmentState {
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }
}

/// Stable member-row identity for one command key and user. The retained owner
/// hashed the user id; row identity only needs to be deterministic and unique
/// within the command, not cryptographic.
pub fn pool_member_id(key: &str, user_id: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in user_id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("agent-pool-member-{key}-{hash:016x}")
}
