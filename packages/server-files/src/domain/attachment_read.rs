//! Pure policy for the native Files attachment metadata read.
//!
//! These types never confer a Person's read grant. The composition-owned
//! target authority port authorizes the owning record; this module validates
//! the bounded Files ledger facts, preserves draft visibility, and derives
//! presentation values such as `bindable`. No database identity, transaction
//! proof or HTTP claim can be constructed from a request.
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde::{Deserialize, Serialize};

pub const MAX_SETS: usize = 100;
pub const MAX_SET_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_STATUS_RESPONSE_BYTES: usize = 64 * 1024;
pub const MAX_TARGET_TYPE_BYTES: usize = 80;
pub const MAX_TARGET_ID_BYTES: usize = 180;
pub const MAX_PURPOSE_BYTES: usize = 80;
pub const MAX_IDENTIFIER_BYTES: usize = 200;
pub const MAX_FILENAME_BYTES: usize = 255;
pub const MAX_MEDIA_TYPE_BYTES: usize = 100;

const COMPONENT_KINDS: [&str; 6] = [
    "record",
    "action",
    "line",
    "event",
    "message",
    "publication",
];
const SET_STATES: [&str; 4] = ["draft", "bound", "withdrawn", "expired"];
const LINK_STATES: [&str; 4] = ["draft", "bound", "withdrawn", "expired"];
const SOURCE_STATES: [&str; 7] = [
    "legacy_unscanned",
    "quarantined",
    "scanning",
    "available",
    "scan_error",
    "malware_rejected",
    "destroyed",
];
const CLASSIFICATIONS: [&str; 4] = ["public", "internal", "confidential", "restricted"];
/// Same reviewed targets as the retained sanitized-system-evidence policy.
const SANITIZATION_TARGETS: [&str; 5] = [
    "iam_review",
    "iam_migration",
    "company_release_acceptance",
    "people_provision_operation",
    "company_provisioning",
];

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}
fn missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_LINK_NOT_FOUND",
        "File link not found.",
    )
}
fn cross_company() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ATTACHMENT_CROSS_COMPANY",
        "Files cannot cross Companies.",
    )
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_READ_CHANGED",
        "The file metadata changed while it was being read.",
    )
}
/// ECMAScript `String.prototype.trim` whitespace: WhiteSpace plus
/// LineTerminator. Rust's `str::trim` additionally strips U+0085 (White_Space
/// in Unicode but not in JavaScript `trim`) and keeps U+FEFF, so the exact
/// set is spelled out to match the retained TypeScript `.trim()` boundary.
pub(crate) fn js_trim(value: &str) -> &str {
    value.trim_matches(|character| {
        matches!(
            character,
            '\u{0009}'..='\u{000D}'
                | '\u{0020}'
                | '\u{00A0}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
        )
    })
}

fn bounded(value: &str, maximum: usize, _code: &'static str) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}
fn optional_bounded(value: Option<&str>, maximum: usize) -> bool {
    value.is_none_or(|value| bounded(value, maximum, "ATTACHMENT_READ_INVALID"))
}
fn known(value: &str, allowed: &[&str]) -> bool {
    allowed.contains(&value)
}
fn timestamp(value: i64) -> bool {
    value >= 0
}

/// Selected Person/Company scope captured by composition, never by the request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentReader {
    user_id: String,
    company_id: CompanyId,
    branch_id: Option<String>,
    department_id: Option<String>,
    expires_at: i64,
}
impl AttachmentReader {
    pub fn new(
        user_id: impl Into<String>,
        company_id: CompanyId,
        branch_id: Option<String>,
        department_id: Option<String>,
        expires_at: i64,
    ) -> Result<Self, BusinessError> {
        let user_id = user_id.into();
        if !bounded(&user_id, MAX_IDENTIFIER_BYTES, "ATTACHMENT_READ_INVALID")
            || !optional_bounded(branch_id.as_deref(), MAX_IDENTIFIER_BYTES)
            || !optional_bounded(department_id.as_deref(), MAX_IDENTIFIER_BYTES)
            || expires_at <= 0
        {
            return Err(invalid(
                "ATTACHMENT_READ_INVALID",
                "Use a current selected Company reader.",
            ));
        }
        Ok(Self {
            user_id,
            company_id,
            branch_id,
            department_id,
            expires_at,
        })
    }
    pub fn user_id(&self) -> &str {
        &self.user_id
    }
    pub fn company_id(&self) -> &CompanyId {
        &self.company_id
    }
    pub fn branch_id(&self) -> Option<&str> {
        self.branch_id.as_deref()
    }
    pub fn department_id(&self) -> Option<&str> {
        self.department_id.as_deref()
    }
    pub fn expires_at(&self) -> i64 {
        self.expires_at
    }
}

/// Record-owner authorization result. The owner port, not Files, decides scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentTargetScope {
    pub company_id: CompanyId,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub version: i64,
}
impl AttachmentTargetScope {
    pub fn new(
        company_id: CompanyId,
        branch_id: Option<String>,
        department_id: Option<String>,
        version: i64,
    ) -> Result<Self, BusinessError> {
        if !optional_bounded(branch_id.as_deref(), MAX_IDENTIFIER_BYTES)
            || !optional_bounded(department_id.as_deref(), MAX_IDENTIFIER_BYTES)
            || version <= 0
        {
            return Err(changed());
        }
        Ok(Self {
            company_id,
            branch_id,
            department_id,
            version,
        })
    }
}

/// One exact owning-record read request for the composition port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentTarget<'a> {
    pub target_type: &'a str,
    pub target_id: &'a str,
    pub purpose_key: &'a str,
    pub component_kind: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentListFilter {
    target_type: String,
    target_id: String,
    purpose_key: Option<String>,
    component_kind: Option<String>,
    expected_target_version: Option<i64>,
}
impl AttachmentListFilter {
    pub fn new(
        target_type: impl Into<String>,
        target_id: impl Into<String>,
        purpose_key: Option<String>,
        component_kind: Option<String>,
        expected_target_version: Option<i64>,
    ) -> Result<Self, BusinessError> {
        let target_type = target_type.into();
        let target_id = target_id.into();
        let purpose_key = purpose_key.map(|value| js_trim(&value).to_owned());
        let component_kind = component_kind.map(|value| js_trim(&value).to_owned());
        if !bounded(
            &target_type,
            MAX_TARGET_TYPE_BYTES,
            "ATTACHMENT_TARGET_REQUIRED",
        ) || !bounded(
            &target_id,
            MAX_TARGET_ID_BYTES,
            "ATTACHMENT_TARGET_REQUIRED",
        ) {
            return Err(invalid(
                "ATTACHMENT_TARGET_REQUIRED",
                "Choose a record before listing files.",
            ));
        }
        if purpose_key
            .as_deref()
            .is_some_and(|value| !bounded(value, MAX_PURPOSE_BYTES, "ATTACHMENT_POLICY_UNKNOWN"))
        {
            return Err(invalid(
                "ATTACHMENT_POLICY_UNKNOWN",
                "This file purpose is not registered for the selected record and context.",
            ));
        }
        if component_kind
            .as_deref()
            .is_some_and(|value| !known(value, &COMPONENT_KINDS))
        {
            return Err(invalid(
                "ATTACHMENT_COMPONENT_KIND",
                "Choose a registered file context.",
            ));
        }
        if expected_target_version.is_some_and(|value| value <= 0) {
            return Err(invalid(
                "ATTACHMENT_TARGET_VERSION",
                "Use the record version shown on this page.",
            ));
        }
        Ok(Self {
            target_type,
            target_id,
            purpose_key,
            component_kind,
            expected_target_version,
        })
    }
    pub fn target_type(&self) -> &str {
        &self.target_type
    }
    pub fn target_id(&self) -> &str {
        &self.target_id
    }
    pub fn purpose_key(&self) -> Option<&str> {
        self.purpose_key.as_deref()
    }
    pub fn component_kind(&self) -> Option<&str> {
        self.component_kind.as_deref()
    }
    pub fn expected_target_version(&self) -> Option<i64> {
        self.expected_target_version
    }
    pub fn leave_review(&self) -> bool {
        self.target_type == "leave_request"
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentLinkFacts {
    pub id: String,
    pub source_id: Option<String>,
    pub filename: String,
    pub media_type: String,
    pub bytes: Option<i64>,
    pub source_state: Option<String>,
    pub link_state: String,
    pub caption: Option<String>,
    pub preview_available: bool,
    pub created_at: i64,
    pub expires_at: i64,
    pub bound_at: Option<i64>,
}
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttachmentSetFacts {
    pub company_id: String,
    pub id: String,
    pub target_type: String,
    pub target_id: String,
    pub component_kind: String,
    pub component_ref: Option<String>,
    pub purpose_key: String,
    pub expected_target_version: i64,
    pub policy_version: i64,
    pub classification: String,
    pub required_count: i64,
    pub max_files: i64,
    pub state: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub sanitization_attested: Option<bool>,
    pub links: Vec<AttachmentLinkFacts>,
}
impl<'de> Deserialize<'de> for AttachmentLinkFacts {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            id: String,
            source_id: Option<String>,
            filename: String,
            media_type: String,
            bytes: Option<i64>,
            source_state: Option<String>,
            link_state: String,
            caption: Option<String>,
            preview_available: bool,
            created_at: i64,
            expires_at: i64,
            bound_at: Option<i64>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Ok(Self {
            id: wire.id,
            source_id: wire.source_id,
            filename: wire.filename,
            media_type: wire.media_type,
            bytes: wire.bytes,
            source_state: wire.source_state,
            link_state: wire.link_state,
            caption: wire.caption,
            preview_available: wire.preview_available,
            created_at: wire.created_at,
            expires_at: wire.expires_at,
            bound_at: wire.bound_at,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentLinkView {
    pub id: String,
    pub source_id: Option<String>,
    pub filename: String,
    pub media_type: String,
    pub bytes: Option<i64>,
    pub source_state: Option<String>,
    pub link_state: String,
    pub caption: Option<String>,
    pub preview_available: bool,
    pub created_at: i64,
    pub expires_at: i64,
    pub bound_at: Option<i64>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentSetView {
    pub id: String,
    pub target_type: String,
    pub target_id: String,
    pub component_kind: String,
    pub component_ref: Option<String>,
    pub purpose_key: String,
    pub expected_target_version: i64,
    pub policy_version: i64,
    pub classification: String,
    pub required_count: i64,
    pub max_files: i64,
    pub state: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sanitization_attested: Option<bool>,
    pub links: Vec<AttachmentLinkView>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttachmentScanFacts {
    pub attempts: i64,
    pub last_result: Option<String>,
    pub last_error: Option<String>,
    pub signature: Option<String>,
    pub definition_updated_at: Option<i64>,
    pub scanned_at: Option<i64>,
}
impl AttachmentScanFacts {
    pub fn empty() -> Self {
        Self {
            attempts: 0,
            last_result: None,
            last_error: None,
            signature: None,
            definition_updated_at: None,
            scanned_at: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttachmentStatusFacts {
    pub company_id: String,
    pub link_id: String,
    pub set_id: String,
    pub creator_id: String,
    pub target_type: String,
    pub target_id: String,
    pub component_kind: String,
    pub purpose_key: String,
    pub expected_target_version: i64,
    pub source_id: Option<String>,
    pub source_state: Option<String>,
    pub link_state: String,
    pub preview_available: bool,
    pub expires_at: i64,
    pub bound_at: Option<i64>,
    pub withdrawn_at: Option<i64>,
    pub scan: AttachmentScanFacts,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentLinkStatusView {
    pub link_id: String,
    pub set_id: String,
    pub source_id: Option<String>,
    pub source_state: Option<String>,
    pub link_state: String,
    pub scan: AttachmentScanFacts,
    pub preview_available: bool,
    pub bindable: bool,
    pub expires_at: i64,
    pub bound_at: Option<i64>,
    pub withdrawn_at: Option<i64>,
}

pub fn requires_sanitization_attestation(target_type: &str) -> bool {
    known(target_type, &SANITIZATION_TARGETS)
}

fn validate_link(
    facts: &AttachmentLinkFacts,
    now: i64,
) -> Result<AttachmentLinkView, BusinessError> {
    if !bounded(&facts.id, MAX_IDENTIFIER_BYTES, "ATTACHMENT_READ_INVALID")
        || !bounded(
            &facts.filename,
            MAX_FILENAME_BYTES,
            "ATTACHMENT_READ_INVALID",
        )
        || !bounded(
            &facts.media_type,
            MAX_MEDIA_TYPE_BYTES,
            "ATTACHMENT_READ_INVALID",
        )
        || !known(&facts.link_state, &LINK_STATES)
        || !timestamp(facts.created_at)
        || !timestamp(facts.expires_at)
        || facts.expires_at < facts.created_at
        || facts.bytes.is_some_and(|value| value < 1)
        || facts
            .source_id
            .as_deref()
            .is_some_and(|value| !bounded(value, MAX_IDENTIFIER_BYTES, "ATTACHMENT_READ_INVALID"))
        || facts
            .source_state
            .as_deref()
            .is_some_and(|value| !known(value, &SOURCE_STATES))
        || facts
            .caption
            .as_deref()
            .is_some_and(|value| value.len() > 1000 || value.chars().any(char::is_control))
        || facts.bound_at.is_some_and(|value| !timestamp(value))
        || (facts.link_state == "bound") != facts.bound_at.is_some()
    {
        return Err(changed());
    }
    // A quarantined draft may be visible without a source. A source identifier
    // must always carry its state so presentation never implies clean bytes.
    if facts.source_id.is_some() && facts.source_state.is_none() {
        return Err(changed());
    }
    if facts.link_state == "bound" && facts.bound_at.is_some_and(|value| value > now) {
        return Err(changed());
    }
    Ok(AttachmentLinkView {
        id: facts.id.clone(),
        source_id: facts.source_id.clone(),
        filename: facts.filename.clone(),
        media_type: facts.media_type.clone(),
        bytes: facts.bytes,
        source_state: facts.source_state.clone(),
        link_state: facts.link_state.clone(),
        caption: facts.caption.clone(),
        preview_available: facts.preview_available,
        created_at: facts.created_at,
        expires_at: facts.expires_at,
        bound_at: facts.bound_at,
    })
}

/// Validate one bounded set projection and every link. The exact Company is
/// required because a caller-selected row can never widen beyond its reader.
pub fn validate_set(
    reader: &AttachmentReader,
    filter: &AttachmentListFilter,
    facts: AttachmentSetFacts,
    now: i64,
) -> Result<AttachmentSetView, BusinessError> {
    if facts.company_id != reader.company_id().as_str()
        || facts.target_type != filter.target_type()
        || facts.target_id != filter.target_id()
        || filter
            .purpose_key()
            .is_some_and(|value| facts.purpose_key != value)
        || filter
            .component_kind()
            .is_some_and(|value| facts.component_kind != value)
        || filter
            .expected_target_version()
            .is_some_and(|value| facts.expected_target_version != value)
        || !bounded(&facts.id, MAX_IDENTIFIER_BYTES, "ATTACHMENT_READ_INVALID")
        || !bounded(
            &facts.target_type,
            MAX_TARGET_TYPE_BYTES,
            "ATTACHMENT_READ_INVALID",
        )
        || !bounded(
            &facts.target_id,
            MAX_TARGET_ID_BYTES,
            "ATTACHMENT_READ_INVALID",
        )
        || !bounded(
            &facts.purpose_key,
            MAX_PURPOSE_BYTES,
            "ATTACHMENT_READ_INVALID",
        )
        || !known(&facts.component_kind, &COMPONENT_KINDS)
        || !known(&facts.state, &SET_STATES)
        || !known(&facts.classification, &CLASSIFICATIONS)
        || !optional_bounded(facts.component_ref.as_deref(), 180)
        || facts.expected_target_version <= 0
        || facts.policy_version <= 0
        || !(0..=100).contains(&facts.required_count)
        || !(1..=100).contains(&facts.max_files)
        || facts.required_count > facts.max_files
        || !timestamp(facts.created_at)
        || !timestamp(facts.updated_at)
        || facts.updated_at < facts.created_at
        || facts.links.len() > facts.max_files as usize
        || requires_sanitization_attestation(&facts.target_type)
            != facts.sanitization_attested.is_some()
    {
        return Err(changed());
    }
    let mut links = Vec::with_capacity(facts.links.len());
    let mut ids = std::collections::BTreeSet::new();
    let mut previous: Option<(i64, String)> = None;
    for link in &facts.links {
        let view = validate_link(link, now)?;
        if !ids.insert(view.id.clone()) {
            return Err(changed());
        }
        // Persisted presentation order is `created_at, id`; reject a rewritten
        // order instead of silently sorting owner evidence on the way out.
        if previous.as_ref().is_some_and(|(created, id)| {
            (view.created_at, view.id.as_str()) < (*created, id.as_str())
        }) {
            return Err(changed());
        }
        previous = Some((view.created_at, view.id.clone()));
        links.push(view);
    }
    Ok(AttachmentSetView {
        id: facts.id,
        target_type: facts.target_type,
        target_id: facts.target_id,
        component_kind: facts.component_kind,
        component_ref: facts.component_ref,
        purpose_key: facts.purpose_key,
        expected_target_version: facts.expected_target_version,
        policy_version: facts.policy_version,
        classification: facts.classification,
        required_count: facts.required_count,
        max_files: facts.max_files,
        state: facts.state,
        created_at: facts.created_at,
        updated_at: facts.updated_at,
        sanitization_attested: facts.sanitization_attested,
        links,
    })
}

/// Draft links are private to their creator. Leave review keeps its existing
/// reviewer exception because the owner authority already checks that grant.
pub fn may_read_status(reader: &AttachmentReader, facts: &AttachmentStatusFacts) -> bool {
    !(facts.link_state == "draft"
        && facts.creator_id != reader.user_id()
        && facts.target_type != "leave_request")
}

pub fn bindable(facts: &AttachmentStatusFacts, scope_version: i64) -> bool {
    if facts.source_state.as_deref() != Some("available") || facts.link_state != "draft" {
        return false;
    }
    if facts.target_type == "leave_request" {
        scope_version >= facts.expected_target_version
    } else {
        scope_version == facts.expected_target_version
    }
}

pub fn validate_status(
    reader: &AttachmentReader,
    facts: AttachmentStatusFacts,
    scope: &AttachmentTargetScope,
    now: i64,
) -> Result<AttachmentLinkStatusView, BusinessError> {
    if facts.company_id != reader.company_id().as_str() {
        return Err(cross_company());
    }
    if scope.company_id != *reader.company_id() {
        return Err(cross_company());
    }
    if !may_read_status(reader, &facts) {
        return Err(missing());
    }
    if !bounded(
        &facts.link_id,
        MAX_IDENTIFIER_BYTES,
        "ATTACHMENT_READ_INVALID",
    ) || !bounded(
        &facts.set_id,
        MAX_IDENTIFIER_BYTES,
        "ATTACHMENT_READ_INVALID",
    ) || !bounded(
        &facts.creator_id,
        MAX_IDENTIFIER_BYTES,
        "ATTACHMENT_READ_INVALID",
    ) || !bounded(
        &facts.target_type,
        MAX_TARGET_TYPE_BYTES,
        "ATTACHMENT_READ_INVALID",
    ) || !bounded(
        &facts.target_id,
        MAX_TARGET_ID_BYTES,
        "ATTACHMENT_READ_INVALID",
    ) || !bounded(
        &facts.purpose_key,
        MAX_PURPOSE_BYTES,
        "ATTACHMENT_READ_INVALID",
    ) || !known(&facts.component_kind, &COMPONENT_KINDS)
        || !known(&facts.link_state, &LINK_STATES)
        || facts
            .source_state
            .as_deref()
            .is_some_and(|value| !known(value, &SOURCE_STATES))
        || facts
            .source_id
            .as_deref()
            .is_some_and(|value| !bounded(value, MAX_IDENTIFIER_BYTES, "ATTACHMENT_READ_INVALID"))
        || facts.expected_target_version <= 0
        || !timestamp(facts.expires_at)
        || facts.bound_at.is_some_and(|value| !timestamp(value))
        || facts.withdrawn_at.is_some_and(|value| !timestamp(value))
        || (facts.link_state == "bound") != facts.bound_at.is_some()
        || (facts.link_state == "withdrawn") != facts.withdrawn_at.is_some()
        || facts.bound_at.is_some_and(|value| value > now)
        || facts.withdrawn_at.is_some_and(|value| value > now)
        || !(0..=12).contains(&facts.scan.attempts)
        || facts
            .scan
            .definition_updated_at
            .is_some_and(|value| !timestamp(value))
        || facts.scan.scanned_at.is_some_and(|value| !timestamp(value))
        || facts.scan.last_result.as_deref().is_some_and(|value| {
            !known(value, &["pending", "running", "clean", "infected", "error"])
        })
        || facts
            .scan
            .last_error
            .as_deref()
            .is_some_and(|value| !bounded(value, 160, "ATTACHMENT_READ_INVALID"))
        || facts
            .scan
            .signature
            .as_deref()
            .is_some_and(|value| !bounded(value, 160, "ATTACHMENT_READ_INVALID"))
    {
        return Err(changed());
    }
    if facts.source_id.is_some() && facts.source_state.is_none() {
        return Err(changed());
    }
    let can_bind = bindable(&facts, scope.version);
    Ok(AttachmentLinkStatusView {
        link_id: facts.link_id,
        set_id: facts.set_id,
        source_id: facts.source_id,
        source_state: facts.source_state,
        link_state: facts.link_state,
        scan: facts.scan,
        preview_available: facts.preview_available,
        bindable: can_bind,
        expires_at: facts.expires_at,
        bound_at: facts.bound_at,
        withdrawn_at: facts.withdrawn_at,
    })
}

pub fn require_current(reader: &AttachmentReader, now: i64) -> Result<(), BusinessError> {
    if now < reader.expires_at() {
        Ok(())
    } else {
        Err(BusinessError::new(
            FailureKind::Conflict,
            "WORK_CONTEXT_CHANGED",
            "Your work role changed. Reload before reading files.",
        ))
    }
}
