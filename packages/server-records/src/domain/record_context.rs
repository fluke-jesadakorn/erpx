//! Portable Records context-read policy.
//!
//! One authorized record context consists of the scoped workspace record plus
//! its outgoing references, visible backlinks, principal mentions, suggestion
//! metadata and transition timeline. Cross-owner source/admission checks are
//! supplied by the composition-owned guard port; this owner never reads another
//! owner's SQL. Nothing here opens a transaction, reads HTTP or grants access.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Map, Value, json};

pub const CONTRACT_VERSION: &str = "records.record-context.v1";
pub const MAX_RECORD_ID: usize = 180;
pub const MAX_MODULE_ID: usize = 64;
pub const MAX_TITLE: usize = 160;
pub const MAX_DESCRIPTION: usize = 2_000;
pub const MAX_FIELDS_JSON: usize = 64 * 1024;
pub const MAX_FIELD_COUNT: usize = 64;
pub const MAX_REFERENCES: usize = 50;
pub const MAX_BACKLINKS: usize = 50;
pub const MAX_MENTIONS: usize = 200;
pub const MAX_SUGGESTIONS: usize = 20;
pub const MAX_HISTORY: usize = 20;
pub const MAX_SUGGESTION_JSON: usize = 16 * 1024;
pub const MAX_NOTE: usize = 1_000;
pub const EDGE_KINDS: [&str; 5] = [
    "dependsOn",
    "blocks",
    "billedBy",
    "fulfills",
    "authoredWith",
];
pub const STATUSES: [&str; 5] = ["draft", "in_review", "approved", "completed", "rejected"];
pub const SUGGESTION_STATES: [&str; 4] = ["ready", "applied", "rejected", "conflicted"];
pub const MENTION_KINDS: [&str; 3] = ["person", "group", "agent_pool"];

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}
fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}
pub fn request_invalid() -> BusinessError {
    invalid(
        "RECORD_CONTEXT_REQUEST_INVALID",
        "Choose a valid record in this workspace.",
    )
}
pub fn source_invalid() -> BusinessError {
    unavailable(
        "RECORD_CONTEXT_SOURCE_INVALID",
        "The record context is not available.",
    )
}
pub fn context_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your selected work context changed. Choose a current role again.",
    )
}
pub fn permission_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PERMISSION_REQUIRED",
        "Your selected role does not grant this operation.",
    )
}
pub fn module_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MODULE_FORBIDDEN",
        "Change to a role with access to this module.",
    )
}
pub fn record_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "NOT_FOUND",
        "Record not found in this workspace.",
    )
}

/// Current render/authority facts captured by the composition root. This is a
/// snapshot for policy checks, never a grant: the adapter rechecks current IAM.
#[derive(Clone, Debug)]
pub struct RecordContextActor {
    pub user_id: String,
    pub holding_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub assignment_id: String,
    pub assignment_version: Option<i64>,
    pub authority_revision: Option<i64>,
    pub role: String,
    pub scope_kind: String,
    pub modules: Vec<String>,
    pub permissions: Vec<String>,
    pub expires_at: i64,
}

impl RecordContextActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn permits_module(&self, module_id: &str) -> bool {
        self.modules.iter().any(|value| value == module_id)
    }

    pub fn require_read(&self, now: i64) -> Result<(), BusinessError> {
        if now >= self.expires_at {
            return Err(context_changed());
        }
        if !self.has("records.read") {
            return Err(permission_required());
        }
        Ok(())
    }

    /// Exact retained scope selection: a department actor sees rows with the
    /// selected department, a branch actor rows with the selected branch, and a
    /// holding actor every row in the Company.
    pub fn permits_scope(
        &self,
        company_id: &str,
        branch_id: Option<&str>,
        department_id: Option<&str>,
    ) -> bool {
        if company_id != self.company_id {
            return false;
        }
        if let Some(department) = self.department_id.as_deref() {
            return department_id == Some(department);
        }
        if let Some(branch) = self.branch_id.as_deref() {
            return branch_id == Some(branch);
        }
        true
    }

    pub fn context_json(&self) -> Value {
        json!({
            "userId": self.user_id,
            "holdingId": self.holding_id,
            "companyId": self.company_id,
            "branchId": self.branch_id,
            "departmentId": self.department_id,
            "assignmentId": self.assignment_id,
            "assignmentVersion": self.assignment_version,
            "revision": self.authority_revision,
            "role": self.role,
            "scopeKind": self.scope_kind,
            "modules": self.modules,
            "permissions": self.permissions,
            "expiresAt": self.expires_at,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordRow {
    pub module_id: String,
    pub id: String,
    pub section_id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub fields_json: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub version: i64,
    pub created_by: String,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceEdge {
    pub target_module: String,
    pub target_id: String,
    pub kind: String,
    pub ordinal: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BacklinkEdge {
    pub host_module: String,
    pub host_id: String,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionRow {
    pub target_kind: String,
    pub target_id: String,
    pub target_label: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestionRow {
    pub id: String,
    pub field_key: String,
    pub proposed_value_json: String,
    pub source_versions_json: String,
    pub provenance_json: String,
    pub state: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryRow {
    pub from_status: String,
    pub to_status: String,
    pub action: String,
    pub note: String,
    pub actor_id: String,
    pub record_version: i64,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkTarget {
    pub title: String,
    pub version: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordView {
    pub module_id: String,
    pub id: String,
    pub section_id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub fields: Value,
    pub version: i64,
    pub created_by: String,
    pub updated_at: i64,
}

impl RecordView {
    pub fn to_wire(&self) -> Value {
        json!({
            "moduleId": self.module_id,
            "id": self.id,
            "sectionId": self.section_id,
            "title": self.title,
            "description": self.description,
            "status": self.status,
            "fields": self.fields,
            "version": self.version,
            "createdBy": self.created_by,
            "updatedAt": self.updated_at,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceView {
    pub target_module: String,
    pub target_id: String,
    pub kind: String,
    pub ordinal: i64,
    pub title: String,
    pub version: i64,
    pub access: &'static str,
}

impl ReferenceView {
    pub fn to_wire(&self) -> Value {
        json!({
            "target_module": self.target_module,
            "target_id": self.target_id,
            "kind": self.kind,
            "ordinal": self.ordinal,
            "title": self.title,
            "version": self.version,
            "access": self.access,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BacklinkView {
    pub host_module: String,
    pub host_id: String,
    pub kind: String,
    pub title: String,
    pub version: i64,
}

impl BacklinkView {
    pub fn to_wire(&self) -> Value {
        json!({
            "host_module": self.host_module,
            "host_id": self.host_id,
            "kind": self.kind,
            "title": self.title,
            "version": self.version,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordContextDocument {
    pub record: RecordView,
    pub references: Vec<ReferenceView>,
    pub backlinks: Vec<BacklinkView>,
    pub mentions: Vec<MentionRow>,
    pub suggestions: Vec<SuggestionRow>,
    pub history: Vec<HistoryRow>,
}

impl RecordContextDocument {
    pub fn to_wire(&self) -> Value {
        json!({
            "record": self.record.to_wire(),
            "references": self.references.iter().map(ReferenceView::to_wire).collect::<Vec<_>>(),
            "backlinks": self.backlinks.iter().map(BacklinkView::to_wire).collect::<Vec<_>>(),
            "mentions": self.mentions.iter().map(mention_wire).collect::<Vec<_>>(),
            "suggestions": self.suggestions.iter().map(suggestion_wire).collect::<Vec<_>>(),
            "history": self.history.iter().map(history_wire).collect::<Vec<_>>(),
        })
    }
}

fn mention_wire(row: &MentionRow) -> Value {
    json!({
        "target_kind": row.target_kind,
        "target_id": row.target_id,
        "target_label": row.target_label,
        "created_at": row.created_at,
    })
}

fn suggestion_wire(row: &SuggestionRow) -> Value {
    json!({
        "id": row.id,
        "field_key": row.field_key,
        "proposed_value_json": row.proposed_value_json,
        "source_versions_json": row.source_versions_json,
        "provenance_json": row.provenance_json,
        "state": row.state,
        "created_at": row.created_at,
    })
}

fn history_wire(row: &HistoryRow) -> Value {
    json!({
        "from_status": row.from_status,
        "to_status": row.to_status,
        "action": row.action,
        "note": row.note,
        "actor_id": row.actor_id,
        "record_version": row.record_version,
        "created_at": row.created_at,
    })
}

pub(crate) fn valid_module(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_MODULE_ID
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

pub(crate) fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_RECORD_ID
        && !value.contains('\0')
        && !value.bytes().any(|byte| byte.is_ascii_control())
}

pub(crate) fn valid_text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.contains('\0')
}

pub fn validate_request(module_id: &str, id: &str) -> Result<(), BusinessError> {
    if !valid_module(module_id) || !valid_id(id) {
        return Err(request_invalid());
    }
    Ok(())
}

fn validate_fields(fields_json: &str) -> Result<Value, BusinessError> {
    if fields_json.len() > MAX_FIELDS_JSON {
        return Err(source_invalid());
    }
    let fields: Value = serde_json::from_str(fields_json).map_err(|_| source_invalid())?;
    let Some(entries) = fields.as_array() else {
        return Err(source_invalid());
    };
    if entries.is_empty() || entries.len() > MAX_FIELD_COUNT {
        return Err(source_invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    for entry in entries {
        let Some(object) = entry.as_object() else {
            return Err(source_invalid());
        };
        let (Some(key), Some(label), Some(kind)) = (
            object.get("key").and_then(Value::as_str),
            object.get("label").and_then(Value::as_str),
            object.get("type").and_then(Value::as_str),
        ) else {
            return Err(source_invalid());
        };
        if !valid_text(key, 80)
            || key.trim() != key
            || !seen.insert(key.to_owned())
            || !valid_text(label, 120)
            || !valid_text(kind, 30)
        {
            return Err(source_invalid());
        }
        let Some(value) = object.get("value") else {
            return Err(source_invalid());
        };
        if !(value.is_string() || value.is_number() || value.is_boolean()) {
            return Err(source_invalid());
        }
        if let Some(options) = object.get("options") {
            let Some(options) = options.as_array() else {
                return Err(source_invalid());
            };
            if options.len() > 50
                || options.iter().any(|option| {
                    option
                        .as_str()
                        .is_none_or(|value| value.len() > 120 || !valid_text(value, 120))
                })
            {
                return Err(source_invalid());
            }
        }
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "key" | "label" | "type" | "value" | "options"))
        {
            return Err(source_invalid());
        }
    }
    Ok(fields)
}

pub fn validate_record(
    row: &RecordRow,
    actor: &RecordContextActor,
    module_id: &str,
    id: &str,
) -> Result<RecordView, BusinessError> {
    if row.module_id != module_id
        || row.id != id
        || !actor.permits_scope(
            &row.company_id,
            row.branch_id.as_deref(),
            row.department_id.as_deref(),
        )
        || !STATUSES.contains(&row.status.as_str())
        || !valid_text(&row.section_id, 128)
        || !valid_text(&row.title, MAX_TITLE)
        || row.description.len() > MAX_DESCRIPTION
        || row.description.contains('\0')
        || !valid_text(&row.created_by, 200)
        || row.version <= 0
        || row.updated_at < 0
    {
        return Err(source_invalid());
    }
    Ok(RecordView {
        module_id: row.module_id.clone(),
        id: row.id.clone(),
        section_id: row.section_id.clone(),
        title: row.title.clone(),
        description: row.description.clone(),
        status: row.status.clone(),
        fields: validate_fields(&row.fields_json)?,
        version: row.version,
        created_by: row.created_by.clone(),
        updated_at: row.updated_at,
    })
}

pub fn validate_reference_edges(
    edges: Vec<ReferenceEdge>,
) -> Result<Vec<ReferenceEdge>, BusinessError> {
    if edges.len() > MAX_REFERENCES {
        return Err(source_invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut previous = -1;
    for edge in &edges {
        if !valid_module(&edge.target_module)
            || !valid_id(&edge.target_id)
            || !EDGE_KINDS.contains(&edge.kind.as_str())
            || !(0..=49).contains(&edge.ordinal)
            || edge.ordinal < previous
            || !seen.insert((edge.target_module.clone(), edge.target_id.clone()))
        {
            return Err(source_invalid());
        }
        previous = edge.ordinal;
    }
    Ok(edges)
}

pub fn validate_backlinks(edges: Vec<BacklinkEdge>) -> Result<Vec<BacklinkEdge>, BusinessError> {
    if edges.len() > MAX_BACKLINKS {
        return Err(source_invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    for edge in &edges {
        if !valid_module(&edge.host_module)
            || !valid_id(&edge.host_id)
            || !EDGE_KINDS.contains(&edge.kind.as_str())
            || !seen.insert((edge.host_module.clone(), edge.host_id.clone()))
        {
            return Err(source_invalid());
        }
    }
    Ok(edges)
}

pub fn validate_link_target(target: LinkTarget) -> Result<LinkTarget, BusinessError> {
    if !valid_text(&target.title, MAX_TITLE) || target.version <= 0 {
        return Err(source_invalid());
    }
    Ok(target)
}

pub fn validate_mentions(rows: Vec<MentionRow>) -> Result<Vec<MentionRow>, BusinessError> {
    if rows.len() > MAX_MENTIONS {
        return Err(source_invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    for row in &rows {
        if !MENTION_KINDS.contains(&row.target_kind.as_str())
            || !valid_id(&row.target_id)
            || !valid_text(&row.target_label, MAX_TITLE)
            || row.created_at < 0
            || !seen.insert((row.target_kind.clone(), row.target_id.clone()))
        {
            return Err(source_invalid());
        }
    }
    Ok(rows)
}

fn valid_json(value: &str, expected: fn(&Value) -> bool) -> bool {
    value.len() <= MAX_SUGGESTION_JSON
        && serde_json::from_str::<Value>(value)
            .ok()
            .is_some_and(|value| expected(&value))
}

pub fn validate_suggestions(rows: Vec<SuggestionRow>) -> Result<Vec<SuggestionRow>, BusinessError> {
    if rows.len() > MAX_SUGGESTIONS {
        return Err(source_invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    for row in &rows {
        if !valid_id(&row.id)
            || !valid_text(&row.field_key, 80)
            || row.field_key.trim() != row.field_key
            || !SUGGESTION_STATES.contains(&row.state.as_str())
            || row.created_at < 0
            || !seen.insert(row.id.clone())
            || !valid_json(&row.proposed_value_json, |_| true)
            || !valid_json(&row.source_versions_json, Value::is_array)
            || !valid_json(&row.provenance_json, Value::is_object)
        {
            return Err(source_invalid());
        }
    }
    Ok(rows)
}

pub fn validate_history(rows: Vec<HistoryRow>) -> Result<Vec<HistoryRow>, BusinessError> {
    if rows.len() > MAX_HISTORY {
        return Err(source_invalid());
    }
    for row in &rows {
        if !STATUSES.contains(&row.from_status.as_str())
            || !STATUSES.contains(&row.to_status.as_str())
            || !matches!(
                row.action.as_str(),
                "submit" | "approve" | "reject" | "complete" | "reopen"
            )
            || row.note.len() > MAX_NOTE
            || row.note.contains('\0')
            || !valid_text(&row.actor_id, 200)
            || row.record_version <= 0
            || row.created_at < 0
        {
            return Err(source_invalid());
        }
    }
    Ok(rows)
}

pub fn validate_document(
    document: RecordContextDocument,
    module_id: &str,
    id: &str,
) -> Result<Value, BusinessError> {
    if document.record.module_id != module_id
        || document.record.id != id
        || document.references.len() > MAX_REFERENCES
        || document.backlinks.len() > MAX_BACKLINKS
        || document.mentions.len() > MAX_MENTIONS
        || document.suggestions.len() > MAX_SUGGESTIONS
        || document.history.len() > MAX_HISTORY
    {
        return Err(source_invalid());
    }
    Ok(document.to_wire())
}

/// Serialize an exact owner context. Kept public so the HTTP adapter can build
/// the `{data, context}` envelope without duplicating field names.
pub fn context_envelope(data: Value, context: Value) -> Value {
    let mut envelope = Map::new();
    envelope.insert("data".into(), data);
    envelope.insert("context".into(), context);
    Value::Object(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> RecordContextActor {
        RecordContextActor {
            user_id: "user".into(),
            holding_id: "h001".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            assignment_id: "assignment".into(),
            assignment_version: Some(1),
            authority_revision: Some(1),
            role: "records".into(),
            scope_kind: "holding".into(),
            modules: vec!["finance".into()],
            permissions: vec!["records.read".into()],
            expires_at: 1_000,
        }
    }

    #[test]
    fn request_and_scope_validation_preserve_retained_selection() {
        assert!(validate_request("finance", "rec-1").is_ok());
        assert!(validate_request("../finance", "rec-1").is_err());
        assert!(validate_request("finance", "").is_err());
        let mut scoped = actor();
        scoped.department_id = Some("dept".into());
        assert!(scoped.permits_scope("company", Some("branch"), Some("dept")));
        assert!(!scoped.permits_scope("company", Some("branch"), Some("other")));
        scoped.department_id = None;
        scoped.branch_id = Some("branch".into());
        assert!(scoped.permits_scope("company", Some("branch"), Some("dept")));
        assert!(!scoped.permits_scope("company", Some("other"), Some("dept")));
        assert!(!scoped.permits_scope("foreign", None, None));
    }

    #[test]
    fn permission_and_expiry_are_current_render_facts() {
        let mut scoped = actor();
        assert!(scoped.require_read(999).is_ok());
        assert_eq!(
            scoped.require_read(1_000).unwrap_err().code,
            "WORK_CONTEXT_CHANGED"
        );
        scoped.expires_at = 2_000;
        scoped.permissions.clear();
        assert_eq!(
            scoped.require_read(999).unwrap_err().code,
            "PERMISSION_REQUIRED"
        );
    }
}
