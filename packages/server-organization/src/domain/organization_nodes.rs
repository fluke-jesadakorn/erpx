//! Organization node lifecycle, business-unit scope and sharing rules.
//!
//! These are the current rules from the retained TypeScript owner:
//! `organizationNodeChangeSchema` plus `validateNodeChange`, the node-change
//! state machine, the relocation fences and the department channel audience.
//! The module performs no I/O; adapters supply current sources.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use serde::Serialize;
use serde_json::Value;

pub const MAX_NODES: usize = 4_000;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// Static portion of `reservedWorkspaceSlugs` from the Holding address
/// contract. A unit code may never shadow an application, system route or
/// reserved public root.
const RESERVED_SLUGS: &[&str] = &[
    "apps",
    "pos",
    "pms",
    "finance",
    "people",
    "sales",
    "supply",
    "assets",
    "projects",
    "recruit",
    "learn",
    "support",
    "service",
    "legal",
    "manufacturing",
    "admin",
    "booking",
    "portal",
    "legal-portal",
    "forms",
    "book",
    "customer-portal",
    "departments",
    "sso",
    "api",
    "_next",
    "__erpx",
    "login",
    "logout",
    "launch",
    "choose-work-role",
    "applications",
    "account",
    "provider",
    "platform",
    "robots.txt",
    "manifest.webmanifest",
    "brand",
    "fonts",
    "vendor",
];

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

pub fn permission_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PERMISSION_REQUIRED",
        "Your selected role does not grant this permission.",
    )
}

pub fn company_authority_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMPANY_AUTHORITY_REQUIRED",
        "Open the Company workspace with Company HR authority to manage branches and departments.",
    )
}

pub fn workspace_missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "WORKSPACE_NOT_FOUND",
        "Workspace not found.",
    )
}

pub fn node_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ORGANIZATION_NODE_NOT_FOUND",
        "Organization unit not found.",
    )
}

pub fn change_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ORGANIZATION_CHANGE_NOT_FOUND",
        "Organization change not found.",
    )
}

fn js_len(value: &str) -> usize {
    value.encode_utf16().count()
}

fn integer(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64().or_else(|| {
            let value = number.as_f64()?;
            (value.is_finite() && value.fract() == 0.0 && value.abs() <= MAX_SAFE_INTEGER as f64)
                .then_some(value as i64)
        }),
        _ => None,
    }
}

fn string<'a>(value: &'a Value, message: &'static str) -> Result<&'a str, BusinessError> {
    value
        .as_str()
        .ok_or_else(|| invalid("VALIDATION_FAILED", message))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeChangeAction {
    Create,
    Rename,
    Suspend,
    Reactivate,
    Archive,
    Relocate,
}

impl NodeChangeAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "create" => Some(Self::Create),
            "rename" => Some(Self::Rename),
            "suspend" => Some(Self::Suspend),
            "reactivate" => Some(Self::Reactivate),
            "archive" => Some(Self::Archive),
            "relocate" => Some(Self::Relocate),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Rename => "rename",
            Self::Suspend => "suspend",
            Self::Reactivate => "reactivate",
            Self::Archive => "archive",
            Self::Relocate => "relocate",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeChangeState {
    Draft,
    Submitted,
    Applied,
    Rejected,
    Cancelled,
}

impl NodeChangeState {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "submitted" => Some(Self::Submitted),
            "applied" => Some(Self::Applied),
            "rejected" => Some(Self::Rejected),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Submitted => "submitted",
            Self::Applied => "applied",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn editable(self) -> bool {
        matches!(self, Self::Draft | Self::Rejected)
    }

    pub fn cancellable(self) -> bool {
        matches!(self, Self::Draft | Self::Submitted | Self::Rejected)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeLifecycleKind {
    Branch,
    Department,
}

impl NodeLifecycleKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "branch" => Some(Self::Branch),
            "department" => Some(Self::Department),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Branch => "branch",
            Self::Department => "department",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeChangeInput {
    pub action: NodeChangeAction,
    pub target_node_id: Option<RecordId>,
    pub kind: NodeLifecycleKind,
    pub slug: Option<String>,
    pub display_name: String,
    pub branch_id: Option<RecordId>,
    pub expected_node_version: Option<i64>,
    pub expected_version: i64,
    pub reason: String,
    pub preview_snapshot: Option<String>,
}

impl NodeChangeInput {
    /// Strict current wire input. Unknown keys are ignored exactly as the
    /// retained Zod object does; every accepted field keeps its bound.
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let object = value.as_object().ok_or_else(|| {
            invalid(
                "VALIDATION_FAILED",
                "Provide a complete organization change.",
            )
        })?;
        let action = object
            .get("action")
            .and_then(Value::as_str)
            .and_then(NodeChangeAction::parse)
            .ok_or_else(|| {
                invalid(
                    "VALIDATION_FAILED",
                    "Choose a supported organization change.",
                )
            })?;
        let kind = object
            .get("kind")
            .and_then(Value::as_str)
            .and_then(NodeLifecycleKind::parse)
            .ok_or_else(|| invalid("VALIDATION_FAILED", "Choose a branch or department."))?;
        let target_node_id = optional_id(object.get("targetNodeId"))?;
        let slug = optional_trimmed(
            object.get("slug"),
            80,
            "Choose a stable URL code for the new unit.",
        )?;
        if let Some(slug) = &slug
            && !valid_slug(slug)
        {
            return Err(invalid(
                "VALIDATION_FAILED",
                "Use lower-case letters, digits and single hyphens for the URL code.",
            ));
        }
        let display_name = trimmed(
            object.get("displayName"),
            2,
            120,
            "Provide a unit name of 2 to 120 characters.",
        )?;
        let branch_id = optional_id(object.get("branchId"))?;
        let expected_node_version = optional_integer(
            object.get("expectedNodeVersion"),
            1,
            "Provide the current organization unit version.",
        )?;
        let expected_version = integer(
            object
                .get("expectedVersion")
                .ok_or_else(|| invalid("VALIDATION_FAILED", "Provide the change version."))?,
        )
        .filter(|value| *value >= 0)
        .ok_or_else(|| invalid("VALIDATION_FAILED", "Provide the change version."))?;
        let reason = trimmed(
            object.get("reason"),
            3,
            1_000,
            "Provide a review reason of 3 to 1000 characters.",
        )?;
        let preview_snapshot = match object.get("previewSnapshot") {
            None | Some(Value::Null) => None,
            Some(Value::String(value))
                if value.len() == 64
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) =>
            {
                Some(value.clone())
            }
            Some(_) => {
                return Err(invalid(
                    "VALIDATION_FAILED",
                    "Preview the relocation again before submitting.",
                ));
            }
        };
        if action == NodeChangeAction::Create
            && slug
                .as_deref()
                .is_some_and(|slug| RESERVED_SLUGS.contains(&slug))
        {
            return Err(invalid(
                "VALIDATION_FAILED",
                "Choose a workspace slug distinct from applications and system routes.",
            ));
        }
        Ok(Self {
            action,
            target_node_id,
            kind,
            slug,
            display_name,
            branch_id,
            expected_node_version,
            expected_version,
            reason,
            preview_snapshot,
        })
    }

    pub fn is_relocation(&self) -> bool {
        self.action == NodeChangeAction::Relocate
    }

    /// Structural checks that need no database facts.
    pub fn validate_structure(&self) -> Result<(), BusinessError> {
        if self.action == NodeChangeAction::Create {
            if self.target_node_id.is_some() || self.expected_node_version.is_some() {
                return Err(invalid(
                    "NEW_NODE_INVALID",
                    "A new unit cannot reference an existing node version.",
                ));
            }
            if self.slug.is_none() {
                return Err(invalid(
                    "SLUG_REQUIRED",
                    "Choose a stable URL code for the new unit.",
                ));
            }
        } else if self.target_node_id.is_none() || self.expected_node_version.is_none() {
            return Err(invalid(
                "NODE_VERSION_REQUIRED",
                "Reload and choose the current organization unit.",
            ));
        }
        if self.kind == NodeLifecycleKind::Branch && self.branch_id.is_some() {
            return Err(invalid(
                "BRANCH_PARENT_INVALID",
                "A branch belongs directly to the company.",
            ));
        }
        if self.kind == NodeLifecycleKind::Department && self.branch_id.is_none() {
            return Err(invalid(
                "DEPARTMENT_BRANCH_REQUIRED",
                "Choose the department's branch.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeReviewDecision {
    Approve,
    Reject,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeDecisionInput {
    pub expected_version: i64,
    pub decision: NodeReviewDecision,
    pub reason: String,
    pub attachment_set_ids: Vec<String>,
}

impl NodeDecisionInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let object = value
            .as_object()
            .ok_or_else(|| invalid("VALIDATION_FAILED", "Provide a complete review decision."))?;
        let expected_version = integer(
            object
                .get("expectedVersion")
                .ok_or_else(|| invalid("VALIDATION_FAILED", "Provide the change version."))?,
        )
        .filter(|value| *value >= 1)
        .ok_or_else(|| invalid("VALIDATION_FAILED", "Provide the change version."))?;
        let decision = match object.get("decision").and_then(Value::as_str) {
            Some("approve") => NodeReviewDecision::Approve,
            Some("reject") => NodeReviewDecision::Reject,
            _ => {
                return Err(invalid("VALIDATION_FAILED", "Choose approve or reject."));
            }
        };
        let reason = trimmed(
            object.get("reason"),
            3,
            1_000,
            "Provide a review reason of 3 to 1000 characters.",
        )?;
        let attachment_set_ids = match object.get("attachmentSetIds") {
            None => Vec::new(),
            Some(Value::Array(values)) if values.len() <= 20 => values
                .iter()
                .map(|value| {
                    let text = string(value, "Choose only organization-change proof.")?.trim();
                    if text.is_empty() || js_len(text) > 200 {
                        return Err(invalid(
                            "VALIDATION_FAILED",
                            "Choose only organization-change proof.",
                        ));
                    }
                    Ok(text.to_owned())
                })
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => {
                return Err(invalid(
                    "VALIDATION_FAILED",
                    "Choose at most 20 organization-change proofs.",
                ));
            }
        };
        Ok(Self {
            expected_version,
            decision,
            reason,
            attachment_set_ids,
        })
    }

    /// Node lifecycle does not bind attachment proof in this native slice.
    pub fn require_unbound_proof(&self) -> Result<(), BusinessError> {
        if self.attachment_set_ids.is_empty() {
            Ok(())
        } else {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "ORGANIZATION_ATTACHMENT_BINDING_UNAVAILABLE",
                "Attach organization-change proof through the retained review until native proof binding is available.",
            ))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeFilters {
    pub change_id: Option<String>,
    pub state: ChangeStateFilter,
    pub query: String,
    pub page: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeStateFilter {
    Open,
    All,
    One(NodeChangeState),
}

impl Default for ChangeFilters {
    fn default() -> Self {
        Self {
            change_id: None,
            state: ChangeStateFilter::Open,
            query: String::new(),
            page: 1,
        }
    }
}

impl ChangeFilters {
    pub fn parse(query: &std::collections::BTreeMap<String, String>) -> Self {
        let change_id = query
            .get("changeId")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty() && js_len(value) <= 200)
            .map(str::to_owned);
        let state = match query.get("changeState").map(String::as_str) {
            Some("all") => ChangeStateFilter::All,
            Some("draft") => ChangeStateFilter::One(NodeChangeState::Draft),
            Some("submitted") => ChangeStateFilter::One(NodeChangeState::Submitted),
            Some("rejected") => ChangeStateFilter::One(NodeChangeState::Rejected),
            Some("applied") => ChangeStateFilter::One(NodeChangeState::Applied),
            Some("cancelled") => ChangeStateFilter::One(NodeChangeState::Cancelled),
            _ => ChangeStateFilter::Open,
        };
        let mut query_text = query
            .get("changeQuery")
            .map(|value| value.trim().chars().take(120).collect::<String>())
            .unwrap_or_default();
        if change_id.is_some() {
            // The retained overview pins an exact record and clears free text.
            query_text.clear();
        }
        let page = query
            .get("changePage")
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| (1..=100_000).contains(value))
            .unwrap_or(1);
        let state = if change_id.is_some() {
            ChangeStateFilter::All
        } else {
            state
        };
        Self {
            change_id,
            state,
            query: query_text,
            page,
        }
    }

    pub fn sql_state(&self) -> &'static str {
        match self.state {
            ChangeStateFilter::Open => "state IN ('draft','submitted','rejected')",
            ChangeStateFilter::All => "TRUE",
            ChangeStateFilter::One(NodeChangeState::Draft) => "state='draft'",
            ChangeStateFilter::One(NodeChangeState::Submitted) => "state='submitted'",
            ChangeStateFilter::One(NodeChangeState::Rejected) => "state='rejected'",
            ChangeStateFilter::One(NodeChangeState::Applied) => "state='applied'",
            ChangeStateFilter::One(NodeChangeState::Cancelled) => "state='cancelled'",
        }
    }

    pub fn rendered_state(&self) -> &'static str {
        match self.state {
            ChangeStateFilter::Open => "open",
            ChangeStateFilter::All => "all",
            ChangeStateFilter::One(state) => state.as_str(),
        }
    }

    /// ILIKE pattern with the retained `\ % _` escaping.
    pub fn like_pattern(&self) -> String {
        let mut escaped = String::with_capacity(self.query.len() + 2);
        escaped.push('%');
        for character in self.query.chars() {
            if matches!(character, '\\' | '%' | '_') {
                escaped.push('\\');
            }
            escaped.push(character);
        }
        escaped.push('%');
        escaped
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationScopeKind {
    Holding,
    Branch,
    Department,
}

impl OrganizationScopeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Holding => "holding",
            Self::Branch => "branch",
            Self::Department => "department",
        }
    }
}

/// Current authority projection. It is not request-deserializable and grants
/// no work by itself; the adapter re-checks the source rows on the transaction.
#[derive(Clone, Debug)]
pub struct OrganizationActor {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub user_id: UserId,
    pub assignment_id: AssignmentId,
    pub role: String,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub scope_kind: OrganizationScopeKind,
    pub assignment_scope_kind: OrganizationScopeKind,
    pub permissions: Vec<String>,
    pub authority: Option<OrganizationAuthority>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationAuthority {
    pub session_id: String,
    pub revision: i64,
    pub assignment_version: i64,
}

impl OrganizationActor {
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn can_manage(&self) -> bool {
        self.has_permission("organization.manage")
    }

    pub fn can_manage_units(&self) -> bool {
        self.company_authority().is_ok()
    }

    pub fn company_authority(&self) -> Result<(), BusinessError> {
        if !self.can_manage()
            || self.assignment_scope_kind != OrganizationScopeKind::Holding
            || self.scope_kind != OrganizationScopeKind::Holding
        {
            return Err(company_authority_required());
        }
        Ok(())
    }

    pub fn same_person_sql_company(&self) -> &CompanyId {
        &self.company_id
    }
}

/// Structural business-unit row with no roster or IAM authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeFacts {
    pub id_kind: NodeLifecycleKind,
    pub branch_id: Option<RecordId>,
    pub status: NodeStatus,
    pub hierarchy_revision: i64,
}

impl NodeFacts {
    pub fn active(&self) -> bool {
        self.status == NodeStatus::Active
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    Active,
    Suspended,
    Archived,
}

impl NodeStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "suspended" => Some(Self::Suspended),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Archived => "archived",
        }
    }
}

/// Checks the target unit facts against the proposed action before any write.
pub fn validate_target_facts(
    input: &NodeChangeInput,
    facts: Option<NodeFacts>,
    branch_active: bool,
    branch_id: Option<&RecordId>,
) -> Result<(), BusinessError> {
    if input.kind == NodeLifecycleKind::Department && !branch_active {
        return Err(invalid(
            "BRANCH_INVALID",
            "Choose an active branch in this company.",
        ));
    }
    if input.action == NodeChangeAction::Create {
        return Ok(());
    }
    let Some(facts) = facts else {
        return Err(node_not_found());
    };
    if facts.id_kind != input.kind
        || (!input.is_relocation() && facts.branch_id.as_ref() != branch_id)
    {
        return Err(invalid(
            "NODE_PLACEMENT_CHANGED",
            "Reload the organization unit and review its placement.",
        ));
    }
    if input.is_relocation() {
        if facts.id_kind != NodeLifecycleKind::Department {
            return Err(invalid(
                "RELOCATION_REQUIRES_DEPARTMENT",
                "Only a department can be relocated between branches.",
            ));
        }
        if facts.status != NodeStatus::Active {
            return Err(conflict(
                "NODE_STATE_CHANGED",
                "Only an active department can be relocated.",
            ));
        }
        if facts.hierarchy_revision != input.expected_node_version.unwrap_or_default() {
            return Err(conflict(
                "NODE_VERSION_CHANGED",
                "The department changed. Preview the relocation again.",
            ));
        }
        if facts.branch_id.as_ref() == branch_id {
            return Err(conflict(
                "RELOCATION_UNCHANGED",
                "Choose a different active branch for relocation.",
            ));
        }
    } else {
        match input.action {
            NodeChangeAction::Suspend | NodeChangeAction::Archive
                if facts.status != NodeStatus::Active =>
            {
                return Err(conflict(
                    "NODE_STATE_CHANGED",
                    "The organization unit is already in the requested state.",
                ));
            }
            NodeChangeAction::Reactivate
                if !matches!(facts.status, NodeStatus::Suspended | NodeStatus::Archived) =>
            {
                return Err(conflict(
                    "NODE_STATE_CHANGED",
                    "The organization unit is already in the requested state.",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn validate_relocation_children(has_children: bool) -> Result<(), BusinessError> {
    if has_children {
        return Err(conflict(
            "RELOCATION_HAS_CHILDREN",
            "Move or archive the department's child units before relocating it.",
        ));
    }
    Ok(())
}

pub fn validate_relocation_slug_conflict(conflict_exists: bool) -> Result<(), BusinessError> {
    if conflict_exists {
        return Err(conflict(
            "RELOCATION_SLUG_CONFLICT",
            "A department with this URL code already exists in the destination branch.",
        ));
    }
    Ok(())
}

pub fn validate_reserved_draft(reserved: bool) -> Result<(), BusinessError> {
    if reserved {
        return Err(conflict(
            "ORGANIZATION_DRAFT_RESERVED",
            "This department contains changes reserved by a confirmed organization draft.",
        ));
    }
    Ok(())
}

pub fn validate_preview_snapshot(
    submitted: Option<&String>,
    current: &str,
) -> Result<(), BusinessError> {
    if submitted.is_some_and(|submitted| submitted != current) {
        return Err(conflict(
            "ORGANIZATION_RELOCATION_CHANGED",
            "The department's people, positions, or access assignments changed. Preview the move again.",
        ));
    }
    Ok(())
}

pub fn validate_review_before_apply(
    submitted: Option<&String>,
    current: &str,
) -> Result<(), BusinessError> {
    if submitted.is_some_and(|submitted| submitted != current) {
        return Err(conflict(
            "ORGANIZATION_RELOCATION_CHANGED",
            "The department's people, positions, or access assignments changed. Ask HR to preview and submit the move again.",
        ));
    }
    Ok(())
}

/// Dependencies counted for a unit. Unsafe suspension/archive requires all zero.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct NodeImpact {
    pub child_nodes: i64,
    pub staff: i64,
    pub positions: i64,
    pub assignments: i64,
    pub warehouses: i64,
}

impl NodeImpact {
    pub fn blocking(self) -> bool {
        [
            self.child_nodes,
            self.staff,
            self.positions,
            self.assignments,
            self.warehouses,
        ]
        .iter()
        .any(|count| *count > 0)
    }

    pub fn refusal(self, action: NodeChangeAction) -> BusinessError {
        let names = [
            ("child nodes", self.child_nodes),
            ("staff", self.staff),
            ("positions", self.positions),
            ("assignments", self.assignments),
            ("warehouses", self.warehouses),
        ];
        let detail = names
            .iter()
            .filter(|(_, count)| *count > 0)
            .map(|(label, count)| format!("{label} {count}"))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = action;
        // Codes and statuses stay stable; the active counts are informational
        // and are also exposed by the preview impact read.
        let _ = detail;
        BusinessError::new(
            FailureKind::Conflict,
            "ORGANIZATION_NODE_IN_USE",
            "Move or close active dependencies before changing this unit.",
        )
    }
}

pub fn validate_lifecycle_impact(
    action: NodeChangeAction,
    impact: NodeImpact,
) -> Result<(), BusinessError> {
    if matches!(
        action,
        NodeChangeAction::Suspend | NodeChangeAction::Archive
    ) && impact.blocking()
    {
        return Err(impact.refusal(action));
    }
    Ok(())
}

pub fn validate_review_state(
    requested_by: &UserId,
    reviewed_by: Option<&UserId>,
    reviewer_actor: &OrganizationActor,
    state: NodeChangeState,
    version: i64,
    expected_version: i64,
    independent: bool,
) -> Result<(), BusinessError> {
    if state != NodeChangeState::Submitted {
        return Err(conflict(
            "CHANGE_NOT_SUBMITTED",
            "Only submitted organization changes can be reviewed.",
        ));
    }
    let same_person = requested_by == &reviewer_actor.user_id
        || reviewed_by.is_some_and(|reviewed| reviewed == &reviewer_actor.user_id);
    if same_person || !independent {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "INDEPENDENT_REVIEW_REQUIRED",
            "Another company HR administrator must review this change.",
        ));
    }
    if version != expected_version {
        return Err(conflict(
            "STALE_VERSION",
            "The organization change was updated. Reload and review it again.",
        ));
    }
    Ok(())
}

pub fn validate_change_owner(
    requested_by: &UserId,
    actor: &OrganizationActor,
) -> Result<(), BusinessError> {
    if requested_by != &actor.user_id {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "CHANGE_OWNER_REQUIRED",
            "Only the requester can correct this organization change.",
        ));
    }
    Ok(())
}

pub fn validate_editable(state: NodeChangeState) -> Result<(), BusinessError> {
    if !state.editable() {
        return Err(conflict(
            "CHANGE_LOCKED",
            "Only draft or rejected changes can be edited.",
        ));
    }
    Ok(())
}

pub fn validate_cancellable(
    state: NodeChangeState,
    version: i64,
    expected: i64,
) -> Result<(), BusinessError> {
    if !state.cancellable() {
        return Err(conflict(
            "CHANGE_LOCKED",
            "This organization change can no longer be cancelled.",
        ));
    }
    if version != expected {
        // The retained cancel assertion fails as a constraint conflict.
        return Err(conflict(
            "CONFLICT",
            "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
        ));
    }
    Ok(())
}

/// Visible unit rows for the actor's current assignment scope. Company HR sees
/// the whole Company; a branch scope sees that branch and its departments; a
/// department scope sees exactly that department.
pub fn visible_node_scope(actor: &OrganizationActor) -> VisibleNodeScope {
    match actor.scope_kind {
        OrganizationScopeKind::Holding => VisibleNodeScope::Company,
        OrganizationScopeKind::Branch => VisibleNodeScope::Branch(
            actor
                .branch_id
                .clone()
                .or_else(|| actor.department_id.clone()),
        ),
        OrganizationScopeKind::Department => VisibleNodeScope::Department(
            actor
                .department_id
                .clone()
                .or_else(|| actor.branch_id.clone()),
        ),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VisibleNodeScope {
    Company,
    Branch(Option<RecordId>),
    Department(Option<RecordId>),
}

/// A department is visible to the current engagement exactly when the
/// engagement's department, branch and active account place the person there.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelAudience {
    pub active_person: bool,
    pub active_single_engagement: bool,
    pub active_assignment: bool,
    pub kind_matches: bool,
    pub placement_matches: bool,
}

impl ChannelAudience {
    pub fn visible(self) -> bool {
        self.active_person
            && self.active_single_engagement
            && self.active_assignment
            && self.kind_matches
            && self.placement_matches
    }
}

/// Whether the current node is a permitted target for a sharing channel. The
/// exact source predicate is `organizationChannelEngagementSql`; this mirrors
/// its closed combination for the pure tests.
pub fn channel_visible(
    node_kind: &str,
    node_id: &str,
    engagement_branch: Option<&str>,
    engagement_department: Option<&str>,
) -> bool {
    match node_kind {
        "company" => true,
        "branch" => engagement_branch == Some(node_id),
        "department" => engagement_department == Some(node_id),
        _ => false,
    }
}

/// Model of the retained relocation evidence digest. The adapter serialises
/// the same ordered object; this helper keeps the expected input shape near
/// the rules.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RelocationSnapshot {
    pub engagements: Vec<(String, i64, String)>,
    pub positions: Vec<(String, i64)>,
    pub assignments: Vec<(String, i64)>,
    pub rules: Vec<(String, i64)>,
    pub head: Option<(i64, Option<String>, Option<String>, Option<i64>)>,
    pub legacy_head: Option<(i64, String)>,
    pub flow: Option<(String, i64)>,
}

pub fn relocation_snapshot_json(snapshot: &RelocationSnapshot) -> String {
    fn rows(values: &[(String, i64)]) -> String {
        values
            .iter()
            .map(|(id, version)| format!("{{\"id\":\"{}\",\"version\":{}}}", escape(id), version))
            .collect::<Vec<_>>()
            .join(",")
    }
    let engagements = snapshot
        .engagements
        .iter()
        .map(|(id, version, status)| {
            format!(
                "{{\"id\":\"{}\",\"version\":{},\"status\":\"{}\"}}",
                escape(id),
                version,
                escape(status)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let head = match &snapshot.head {
        None => "null".to_owned(),
        Some((version, position, person, effective)) => format!(
            "{{\"version\":{},\"position_id\":{},\"person_id\":{},\"effective_at\":{}}}",
            version,
            optional_text(position),
            optional_text(person),
            effective.map_or_else(|| "null".to_owned(), |value| value.to_string())
        ),
    };
    let legacy_head = match &snapshot.legacy_head {
        None => "null".to_owned(),
        Some((version, person)) => {
            format!(
                "{{\"version\":{},\"person_id\":\"{}\"}}",
                version,
                escape(person)
            )
        }
    };
    let flow = match &snapshot.flow {
        None => "null".to_owned(),
        Some((id, version)) => {
            format!("{{\"id\":\"{}\",\"version\":{}}}", escape(id), version)
        }
    };
    format!(
        "{{\"engagements\":[{}],\"positions\":[{}],\"assignments\":[{}],\"rules\":[{}],\"head\":{},\"legacyHead\":{},\"flow\":{}}}",
        engagements,
        rows(&snapshot.positions),
        rows(&snapshot.assignments),
        rows(&snapshot.rules),
        head,
        legacy_head,
        flow
    )
}

fn optional_text(value: &Option<String>) -> String {
    value
        .as_ref()
        .map(|value| format!("\"{}\"", escape(value)))
        .unwrap_or_else(|| "null".to_owned())
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped
}

fn optional_id(value: Option<&Value>) -> Result<Option<RecordId>, BusinessError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => {
            let text = text.trim();
            if text.is_empty() {
                return Err(invalid(
                    "VALIDATION_FAILED",
                    "Choose a current organization unit.",
                ));
            }
            RecordId::new(text).map(Some)
        }
        Some(_) => Err(invalid(
            "VALIDATION_FAILED",
            "Choose a current organization unit.",
        )),
    }
}

fn optional_integer(
    value: Option<&Value>,
    minimum: i64,
    message: &'static str,
) -> Result<Option<i64>, BusinessError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => integer(value)
            .filter(|value| *value >= minimum)
            .map(Some)
            .ok_or_else(|| invalid("VALIDATION_FAILED", message)),
    }
}

fn optional_trimmed(
    value: Option<&Value>,
    maximum: usize,
    message: &'static str,
) -> Result<Option<String>, BusinessError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => {
            let text = text.trim();
            if text.is_empty() || js_len(text) > maximum {
                return Err(invalid("VALIDATION_FAILED", message));
            }
            Ok(Some(text.to_owned()))
        }
        Some(_) => Err(invalid("VALIDATION_FAILED", message)),
    }
}

fn trimmed(
    value: Option<&Value>,
    minimum: usize,
    maximum: usize,
    message: &'static str,
) -> Result<String, BusinessError> {
    let value = value.ok_or_else(|| invalid("VALIDATION_FAILED", message))?;
    let value = string(value, message)?.trim();
    let length = js_len(value);
    if length < minimum || length > maximum {
        return Err(invalid("VALIDATION_FAILED", message));
    }
    Ok(value.to_owned())
}

fn valid_slug(value: &str) -> bool {
    if value.len() > 80 || value.starts_with('-') || value.ends_with('-') || value.contains("--") {
        return false;
    }
    value
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}
