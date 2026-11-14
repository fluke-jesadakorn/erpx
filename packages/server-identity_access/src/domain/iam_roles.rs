//! Portable Identity Access role, independent-review and access-review policy.
//!
//! A role save appends an immutable revision and opens an independent review;
//! publishing is a separate decision. Access-review campaigns certify the
//! current assignment snapshot. Nothing here reads a database, HTTP request,
//! clock or permission catalogue schema version; callers supply those facts.
use crate::domain::assignment::AssignmentActor;
use crate::domain::permission_catalog::{
    PermissionAvailability, PermissionCatalog, PermissionNamespace,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use serde::Deserialize;
use serde_json::{Value, json};

pub const MAX_ROLE_NAME: usize = 100;
pub const MAX_ROLE_ID: usize = 180;
pub const MAX_ROLE_PERMISSIONS: usize = 1_000;
pub const MAX_REASON: usize = 1_000;
pub const MAX_CAMPAIGN_NAME: usize = 120;
pub const MAX_CAMPAIGN_ASSIGNMENTS: usize = 1_000;
pub const MAX_REVISION: i64 = 1_000_000_000;
pub const REVIEW_DUE_WINDOW_MS: i64 = 366 * 86_400_000;

pub const ROLE_SAVE_OPERATION: &str = "iam.role.save";
pub const REVIEW_DECIDE_OPERATION: &str = "iam.review.decide";
pub const ACCESS_REVIEW_START_OPERATION: &str = "iam.access_review.start";
pub const ACCESS_REVIEW_DECIDE_OPERATION: &str = "iam.access_review.decide";
pub const ACCESS_REVIEW_CANCEL_OPERATION: &str = "iam.access_review.cancel";

pub const REVIEW_KIND_ROLE_PUBLISH: &str = "role.publish";
pub const REVIEW_KIND_ASSIGNMENT_GRANT: &str = "assignment.grant";
pub const REVIEW_KIND_MAPPING_ACTIVATE: &str = "mapping.activate";
pub const REVIEW_KIND_POLICY_GRANT: &str = "policy.grant";

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
fn missing(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::NotFound, code, message)
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "IAM_ROLES_REVIEWS_REQUEST_INVALID",
        "Review the identity request and try again.",
    )
}
pub fn permission_required() -> BusinessError {
    forbidden(
        "PERMISSION_REQUIRED",
        "Your selected role does not grant this operation.",
    )
}
pub fn access_changed() -> BusinessError {
    conflict(
        "IAM_ACCESS_CHANGED",
        "Your current identity or selected role changed. Reload before retrying.",
    )
}
pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "IDEMPOTENCY_CONFLICT",
        "This Idempotency-Key was already used for a different identity command.",
    )
}
pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "IAM_ROLES_REVIEWS_SOURCE_INVALID",
        "The identity source is not available.",
    )
}
pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "IAM_ROLES_REVIEWS_READBACK_INVALID",
        "The identity command was not persisted as submitted.",
    )
}
pub fn delegation_exceeded() -> BusinessError {
    forbidden(
        "DELEGATION_EXCEEDED",
        "The role or revision exceeds your delegated permission set.",
    )
}
pub fn role_scope_invalid() -> BusinessError {
    BusinessError::invalid(
        "ROLE_SCOPE_INVALID",
        "Role scopes must remain inside their namespace.",
    )
}
pub fn protected_role() -> BusinessError {
    forbidden(
        "PROTECTED_ROLE",
        "Clone a built-in role instead of replacing it.",
    )
}
pub fn role_changed() -> BusinessError {
    conflict(
        "ROLE_CHANGED",
        "The role changed. Review the latest revision.",
    )
}
pub fn review_not_found() -> BusinessError {
    missing("REVIEW_NOT_FOUND", "Review not found in this Company.")
}
pub fn review_changed() -> BusinessError {
    conflict(
        "REVIEW_CHANGED",
        "This review changed. Reload before deciding.",
    )
}
pub fn independent_review_required() -> BusinessError {
    forbidden(
        "INDEPENDENT_REVIEW_REQUIRED",
        "A different person must review this change.",
    )
}
pub fn review_outside_scope() -> BusinessError {
    forbidden(
        "REVIEW_OUTSIDE_SCOPE",
        "This review is outside your delegated scope.",
    )
}
pub fn proposer_authority_revoked() -> BusinessError {
    conflict(
        "PROPOSER_AUTHORITY_REVOKED",
        "The proposer no longer has active authority. Create a new proposal.",
    )
}
pub fn review_unsupported() -> BusinessError {
    conflict(
        "REVIEW_UNSUPPORTED",
        "This review requires its registered workflow handler.",
    )
}
pub fn holding_review_required() -> BusinessError {
    BusinessError::invalid(
        "HOLDING_REVIEW_REQUIRED",
        "Run access certification inside a company workspace.",
    )
}
pub fn review_due_date_invalid() -> BusinessError {
    BusinessError::invalid(
        "REVIEW_DUE_DATE_INVALID",
        "Choose a review due date within the next year.",
    )
}
pub fn no_active_assignments() -> BusinessError {
    conflict(
        "NO_ACTIVE_ASSIGNMENTS",
        "No active assignments are available for review.",
    )
}
pub fn access_review_item_not_found() -> BusinessError {
    missing(
        "ACCESS_REVIEW_ITEM_NOT_FOUND",
        "Access review item not found.",
    )
}
pub fn access_review_item_changed() -> BusinessError {
    conflict(
        "ACCESS_REVIEW_ITEM_CHANGED",
        "This access review item is no longer pending.",
    )
}
pub fn access_review_campaign_changed() -> BusinessError {
    conflict(
        "ACCESS_REVIEW_CAMPAIGN_CHANGED",
        "The campaign changed. Reload before deciding.",
    )
}
pub fn access_review_campaign_not_found() -> BusinessError {
    missing(
        "ACCESS_REVIEW_CAMPAIGN_NOT_FOUND",
        "Access review campaign not found.",
    )
}
pub fn campaign_owner_required() -> BusinessError {
    forbidden(
        "CAMPAIGN_OWNER_REQUIRED",
        "Only the campaign creator can cancel it.",
    )
}
pub fn campaign_decisions_exist() -> BusinessError {
    conflict(
        "CAMPAIGN_DECISIONS_EXIST",
        "A campaign with recorded decisions cannot be cancelled.",
    )
}
pub fn self_access_review() -> BusinessError {
    forbidden(
        "SELF_ACCESS_REVIEW",
        "Another administrator must review your access.",
    )
}
pub fn review_source_changed() -> BusinessError {
    conflict(
        "REVIEW_SOURCE_CHANGED",
        "The assignment changed. Start a new access review for current evidence.",
    )
}
pub fn self_access_change() -> BusinessError {
    forbidden(
        "SELF_ACCESS_CHANGE",
        "Another administrator must change your access.",
    )
}
pub fn last_administrator() -> BusinessError {
    conflict(
        "LAST_ADMINISTRATOR",
        "The last recovery administrator must remain available.",
    )
}
pub fn last_recovery_administrator() -> BusinessError {
    conflict(
        "LAST_RECOVERY_ADMINISTRATOR",
        "Designate another verified local recovery administrator first.",
    )
}
pub fn scope_forbidden() -> BusinessError {
    forbidden(
        "SCOPE_FORBIDDEN",
        "This assignment is outside your delegated scope.",
    )
}

/// Current render and command facts captured by the composition root. This is
/// not a grant; the repository rechecks the same facts inside its unit.
#[derive(Clone, Debug)]
pub struct IamRoleActor {
    pub user_id: UserId,
    pub session_id: RecordId,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub assignment_id: AssignmentId,
    pub scope_kind: String,
    pub scope_node_id: Option<RecordId>,
    pub role: Option<String>,
    pub role_revision: Option<i64>,
    pub assignment_version: Option<i64>,
    pub identity_version: Option<i64>,
    pub authority_revision: Option<i64>,
    pub permissions: Vec<String>,
    pub grantable_permissions: Vec<String>,
    pub person_user_ids: Vec<UserId>,
    pub person_id: Option<RecordId>,
    pub platform: bool,
    pub emergency_until: Option<i64>,
    pub expires_at: i64,
}

impl IamRoleActor {
    /// Adapt the already verified Work context. The extra facts are current
    /// command metadata only; none of them is a fresh authorization proof.
    pub fn from_assignment(
        actor: AssignmentActor,
        grantable_permissions: Vec<String>,
        person_id: Option<RecordId>,
        platform: bool,
    ) -> Self {
        Self {
            user_id: actor.user_id,
            session_id: actor.session_id,
            holding_id: actor.holding_id,
            company_id: actor.company_id,
            branch_id: actor.branch_id,
            department_id: actor.department_id,
            assignment_id: actor.assignment_id,
            scope_kind: actor.scope_kind,
            scope_node_id: actor.scope_node_id,
            role: actor.role,
            role_revision: actor.role_revision,
            assignment_version: actor.assignment_version,
            identity_version: actor.identity_version,
            authority_revision: actor.authority_revision,
            permissions: actor.permissions,
            grantable_permissions,
            person_user_ids: actor.person_user_ids,
            person_id,
            platform,
            emergency_until: actor.emergency_until,
            expires_at: actor.expires_at,
        }
    }

    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn owns_grantable(&self, permission: &str) -> bool {
        self.grantable_permissions
            .iter()
            .any(|value| value == permission)
    }

    fn require_context(&self, now: i64) -> Result<(), BusinessError> {
        if now >= self.expires_at {
            return Err(conflict(
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Choose a current role again.",
            ));
        }
        Ok(())
    }

    pub fn require(&self, permission: &str, now: i64) -> Result<(), BusinessError> {
        self.require_context(now)?;
        if !self.has(permission) {
            return Err(permission_required());
        }
        Ok(())
    }

    /// Role editing and review decisions run only from a Holding company
    /// workspace or the platform namespace.
    pub fn require_management_scope(&self) -> Result<(), BusinessError> {
        if self.platform || self.scope_kind == "holding" {
            Ok(())
        } else {
            Err(scope_forbidden())
        }
    }

    /// Access certification never runs on the platform namespace.
    pub fn require_holding_review(&self, now: i64) -> Result<(), BusinessError> {
        if self.platform {
            return Err(holding_review_required());
        }
        self.require(&self.scoped_permission("iam.review"), now)?;
        if self.scope_kind != "holding" {
            return Err(scope_forbidden());
        }
        Ok(())
    }

    pub fn same_person_user(&self, user_id: &str) -> bool {
        self.user_id.as_str() == user_id
            || self
                .person_user_ids
                .iter()
                .any(|alias| alias.as_str() == user_id)
    }

    /// Platform commands are gated by the Platform permission namespace; the
    /// Holding namespace keeps its unprefixed customer permission names.
    pub fn scoped_permission(&self, permission: &str) -> String {
        if self.platform && !permission.starts_with("platform.") {
            format!("platform.{permission}")
        } else {
            permission.to_owned()
        }
    }

    pub fn authority_json(&self) -> String {
        json!({
            "assignmentId": self.assignment_id.as_str(),
            "companyId": self.company_id.as_str(),
            "branchId": self.branch_id.as_ref().map(RecordId::as_str),
            "departmentId": self.department_id.as_ref().map(RecordId::as_str),
            "scopeKind": self.scope_kind,
            "scopeNodeId": self.scope_node_id.as_ref().map(RecordId::as_str),
            "role": self.role,
            "roleRevision": self.role_revision,
            "assignmentVersion": self.assignment_version,
            "identityVersion": self.identity_version,
            "revision": self.authority_revision,
            "permissions": self.permissions,
            "grantablePermissions": self.grantable_permissions,
            "sessionId": self.session_id.as_str(),
            "platform": self.platform,
            "emergencyUntil": self.emergency_until,
            "expiresAt": self.expires_at,
        })
        .to_string()
    }
}

fn trim_unicode(value: &str) -> &str {
    value.trim_matches(|character: char| {
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

fn identifier(value: &str, limit: usize) -> bool {
    !value.is_empty()
        && value.encode_utf16().count() <= limit
        && !value.chars().any(char::is_control)
}

fn reason(value: &str) -> Option<String> {
    let value = trim_unicode(value).to_owned();
    let count = value.encode_utf16().count();
    if (3..=MAX_REASON).contains(&count) && !value.chars().any(char::is_control) {
        Some(value)
    } else {
        None
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoleSaveInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub expected_version: i64,
    pub permissions: Vec<String>,
    #[serde(default)]
    pub grantable_permissions: Vec<String>,
    pub scopes: Vec<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRoleSave {
    pub id: Option<String>,
    pub name: String,
    pub expected_version: i64,
    pub permissions: Vec<String>,
    pub grantable_permissions: Vec<String>,
    pub scopes: Vec<String>,
    pub reason: String,
}

/// The catalogue is mandatory and current. A live permission outside the
/// actor's namespace, a preview permission, or one the actor cannot delegate is
/// never accepted, regardless of the proposed role name.
pub fn permission_in_namespace(
    catalog: &PermissionCatalog,
    permission: &str,
    platform: bool,
) -> bool {
    catalog.definition(permission).is_some_and(|definition| {
        definition.availability == PermissionAvailability::Live
            && matches!(
                (&definition.namespace, platform),
                (PermissionNamespace::Platform, true) | (PermissionNamespace::Customer, false)
            )
    })
}

pub fn validate_role_save(
    input: RoleSaveInput,
    actor: &IamRoleActor,
    catalog: &PermissionCatalog,
) -> Result<ValidatedRoleSave, BusinessError> {
    actor.require_management_scope()?;
    let name = trim_unicode(&input.name).to_owned();
    let name_count = name.encode_utf16().count();
    if !(2..=MAX_ROLE_NAME).contains(&name_count) || name.chars().any(char::is_control) {
        return Err(request_invalid());
    }
    if !(0..=MAX_REVISION).contains(&input.expected_version) {
        return Err(request_invalid());
    }
    if input.permissions.len() > MAX_ROLE_PERMISSIONS
        || input.grantable_permissions.len() > MAX_ROLE_PERMISSIONS
        || input.permissions.is_empty()
        || input.scopes.is_empty()
    {
        return Err(request_invalid());
    }
    let permissions = input
        .permissions
        .iter()
        .map(|value| {
            if identifier(value, MAX_ROLE_ID) {
                Ok(value.clone())
            } else {
                Err(request_invalid())
            }
        })
        .collect::<Result<std::collections::BTreeSet<_>, _>>()?
        .into_iter()
        .collect::<Vec<_>>();
    let grantable_permissions = input
        .grantable_permissions
        .iter()
        .map(|value| {
            if identifier(value, MAX_ROLE_ID) {
                Ok(value.clone())
            } else {
                Err(request_invalid())
            }
        })
        .collect::<Result<std::collections::BTreeSet<_>, _>>()?
        .into_iter()
        .collect::<Vec<_>>();
    let mut scopes = input.scopes.clone();
    scopes.sort();
    scopes.dedup();
    let prefix = if actor.platform {
        "platform_custom_"
    } else {
        "custom_"
    };
    if let Some(id) = input.id.as_deref()
        && (!identifier(id, MAX_ROLE_ID) || !id.starts_with(prefix))
    {
        return Err(protected_role());
    }
    if scopes.iter().any(|scope| {
        !matches!(
            scope.as_str(),
            "holding" | "branch" | "department" | "platform"
        ) || (actor.platform && scope != "platform")
            || (!actor.platform && scope == "platform")
    }) {
        return Err(role_scope_invalid());
    }
    if permissions
        .iter()
        .chain(grantable_permissions.iter())
        .any(|p| !permission_in_namespace(catalog, p, actor.platform) || !actor.owns_grantable(p))
    {
        return Err(delegation_exceeded());
    }
    let reason = reason(&input.reason).ok_or_else(request_invalid)?;
    Ok(ValidatedRoleSave {
        id: input.id,
        name,
        expected_version: input.expected_version,
        permissions,
        grantable_permissions,
        scopes,
        reason,
    })
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewDecisionInput {
    pub expected_version: i64,
    pub decision: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedReviewDecision {
    pub expected_version: i64,
    pub decision: String,
    pub reason: String,
}

impl ReviewDecisionInput {
    pub fn validate(self) -> Result<ValidatedReviewDecision, BusinessError> {
        if !(1..=MAX_REVISION).contains(&self.expected_version)
            || !matches!(self.decision.as_str(), "approve" | "reject")
        {
            return Err(request_invalid());
        }
        let reason = reason(&self.reason).ok_or_else(request_invalid)?;
        Ok(ValidatedReviewDecision {
            expected_version: self.expected_version,
            decision: self.decision,
            reason,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessReviewCampaignInput {
    pub name: String,
    pub due_at: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedCampaign {
    pub name: String,
    pub due_at: i64,
    pub reason: String,
}

impl AccessReviewCampaignInput {
    pub fn validate(self, now: i64) -> Result<ValidatedCampaign, BusinessError> {
        let name = trim_unicode(&self.name).to_owned();
        let count = name.encode_utf16().count();
        if !(3..=MAX_CAMPAIGN_NAME).contains(&count)
            || name.chars().any(char::is_control)
            || self.due_at <= now
            || self.due_at > now.saturating_add(REVIEW_DUE_WINDOW_MS)
        {
            return Err(review_due_date_invalid());
        }
        let reason = reason(&self.reason).ok_or_else(request_invalid)?;
        Ok(ValidatedCampaign {
            name,
            due_at: self.due_at,
            reason,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessReviewDecisionInput {
    pub expected_campaign_version: i64,
    pub decision: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedCampaignDecision {
    pub expected_campaign_version: i64,
    pub decision: String,
    pub reason: String,
}

impl AccessReviewDecisionInput {
    pub fn validate(self) -> Result<ValidatedCampaignDecision, BusinessError> {
        if !(1..=MAX_REVISION).contains(&self.expected_campaign_version)
            || !matches!(self.decision.as_str(), "keep" | "revoke")
        {
            return Err(request_invalid());
        }
        let reason = reason(&self.reason).ok_or_else(request_invalid)?;
        Ok(ValidatedCampaignDecision {
            expected_campaign_version: self.expected_campaign_version,
            decision: self.decision,
            reason,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessReviewCancelInput {
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedCancel {
    pub expected_version: i64,
    pub reason: String,
}

impl AccessReviewCancelInput {
    pub fn validate(self) -> Result<ValidatedCancel, BusinessError> {
        if !(1..=MAX_REVISION).contains(&self.expected_version) {
            return Err(request_invalid());
        }
        let reason = reason(&self.reason).ok_or_else(request_invalid)?;
        Ok(ValidatedCancel {
            expected_version: self.expected_version,
            reason,
        })
    }
}

#[derive(Clone, Debug)]
pub struct RoleRow {
    pub company_id: String,
    pub id: String,
    pub name: String,
    pub built_in: bool,
    pub published_revision: i64,
    pub status: String,
}

impl RoleRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if !identifier(&self.id, MAX_ROLE_ID)
            || self.name.trim().is_empty()
            || self.name.encode_utf16().count() > 300
            || !(0..=MAX_REVISION).contains(&self.published_revision)
            || !matches!(self.status.as_str(), "active" | "archived")
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ReviewRow {
    pub id: String,
    pub kind: String,
    pub payload: Value,
    pub created_by: String,
    pub created_person: String,
    pub status: String,
    pub version: i64,
    pub reason: String,
}

impl ReviewRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if !identifier(&self.id, 200)
            || self.kind.trim().is_empty()
            || !self.payload.is_object()
            || !matches!(
                self.status.as_str(),
                "pending" | "approved" | "rejected" | "failed"
            )
            || !(1..=MAX_REVISION).contains(&self.version)
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ProposerAuthority {
    pub role: String,
    pub grantable_permissions: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ReviewAssignment {
    pub id: String,
    pub user_id: String,
    pub role: String,
    pub scope_kind: String,
    pub scope_node_id: Option<String>,
    pub source: String,
    pub valid_until: i64,
    pub version: i64,
    pub revoked_at: Option<i64>,
}

impl ReviewAssignment {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if !identifier(&self.id, 200)
            || !identifier(&self.user_id, 200)
            || self.role.trim().is_empty()
            || !matches!(
                self.scope_kind.as_str(),
                "holding" | "branch" | "department"
            )
            || !matches!(
                self.source.as_str(),
                "legacy"
                    | "manual"
                    | "directory"
                    | "emergency"
                    | "policy"
                    | "lifecycle"
                    | "provisioning"
            )
            || !(1..=MAX_REVISION).contains(&self.version)
            || self.valid_until <= 0
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ReviewItemRow {
    pub id: String,
    pub campaign_id: String,
    pub company_id: String,
    pub assignment_id: String,
    pub assignment_version: i64,
    pub user_id: String,
    pub role: String,
    pub scope_kind: String,
    pub scope_node_id: Option<String>,
    pub source: String,
    pub valid_until: i64,
    pub decision: String,
    pub campaign_status: String,
    pub campaign_version: i64,
    pub created_by: String,
}

impl ReviewItemRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if !identifier(&self.id, 200)
            || !identifier(&self.campaign_id, 200)
            || !identifier(&self.assignment_id, 200)
            || !(1..=MAX_REVISION).contains(&self.assignment_version)
            || !(1..=MAX_REVISION).contains(&self.campaign_version)
            || !matches!(self.decision.as_str(), "pending" | "keep" | "revoke")
            || !matches!(
                self.campaign_status.as_str(),
                "open" | "completed" | "cancelled"
            )
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct CampaignRow {
    pub id: String,
    pub status: String,
    pub version: i64,
    pub created_by: String,
}

impl CampaignRow {
    pub fn validate_source(&self) -> Result<(), BusinessError> {
        if !identifier(&self.id, 200)
            || !matches!(self.status.as_str(), "open" | "completed" | "cancelled")
            || !(1..=MAX_REVISION).contains(&self.version)
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct RevocationGuardFacts {
    pub same_person: bool,
    pub recovery_member: bool,
    pub other_recovery_administrator: bool,
    pub other_administrator: bool,
}

#[derive(Clone, Debug)]
pub struct RolePublishPayload {
    pub id: String,
    pub name: String,
    pub revision: i64,
    pub expected_version: i64,
    pub permissions: Vec<String>,
    pub grantable_permissions: Vec<String>,
    pub affected: Vec<String>,
    pub scope_kind: String,
    pub scope_node_id: Option<String>,
    pub authorizing_assignment_id: String,
}

impl RolePublishPayload {
    pub fn parse(
        value: &Value,
        catalog: &PermissionCatalog,
        platform: bool,
    ) -> Result<Self, BusinessError> {
        let payload: RolePublishPayloadWire =
            serde_json::from_value(value.clone()).map_err(|_| source_invalid())?;
        let mut permissions = payload.permissions;
        let mut grantable = payload.grantable_permissions;
        permissions.sort();
        permissions.dedup();
        grantable.sort();
        grantable.dedup();
        if !identifier(&payload.id, MAX_ROLE_ID)
            || payload.name.trim().is_empty()
            || payload.name.encode_utf16().count() > MAX_ROLE_NAME
            || !(1..=MAX_REVISION).contains(&payload.revision)
            || !(0..=MAX_REVISION).contains(&payload.expected_version)
            || !identifier(&payload.authorizing_assignment_id, 200)
            || !matches!(
                payload.scope_kind.as_str(),
                "holding" | "branch" | "department" | "platform"
            )
            || payload.affected.len() > MAX_ROLE_PERMISSIONS
            || payload.affected.iter().any(|value| !identifier(value, 200))
            || permissions
                .iter()
                .chain(grantable.iter())
                .any(|value| !permission_in_namespace(catalog, value, platform))
        {
            return Err(source_invalid());
        }
        if matches!(payload.scope_kind.as_str(), "branch" | "department")
            && payload.scope_node_id.as_deref().is_none_or(str::is_empty)
        {
            return Err(source_invalid());
        }
        Ok(Self {
            id: payload.id,
            name: payload.name,
            revision: payload.revision,
            expected_version: payload.expected_version,
            permissions,
            grantable_permissions: grantable,
            affected: payload.affected,
            scope_kind: payload.scope_kind,
            scope_node_id: payload.scope_node_id,
            authorizing_assignment_id: payload.authorizing_assignment_id,
        })
    }

    pub fn to_payload_value(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "revision": self.revision,
            "expectedVersion": self.expected_version,
            "permissions": self.permissions,
            "grantablePermissions": self.grantable_permissions,
            "affected": self.affected,
            "scopeKind": self.scope_kind,
            "scopeNodeId": self.scope_node_id,
            "authorizingAssignmentId": self.authorizing_assignment_id,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RolePublishPayloadWire {
    id: String,
    name: String,
    revision: i64,
    expected_version: i64,
    permissions: Vec<String>,
    grantable_permissions: Vec<String>,
    affected: Vec<String>,
    scope_kind: String,
    scope_node_id: Option<String>,
    authorizing_assignment_id: String,
}

#[derive(Clone, Debug)]
pub struct RolePublication {
    pub id: String,
    pub name: String,
    pub revision: i64,
    pub expected_version: i64,
    pub permissions: Vec<String>,
    pub grantable_permissions: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct RoleSavePlan {
    pub id: String,
    pub name: String,
    pub expected_version: i64,
    pub revision: i64,
    pub permissions: Vec<String>,
    pub grantable_permissions: Vec<String>,
    pub scopes: Vec<String>,
    pub reason: String,
    pub created: bool,
    pub affected: Vec<String>,
    pub review_id: String,
    pub review_payload: Value,
    pub now: i64,
}

impl RoleSavePlan {
    pub fn publication(&self) -> RolePublication {
        RolePublication {
            id: self.id.clone(),
            name: self.name.clone(),
            revision: self.revision,
            expected_version: self.expected_version,
            permissions: self.permissions.clone(),
            grantable_permissions: self.grantable_permissions.clone(),
        }
    }

    pub fn receipt_json(&self) -> String {
        json!({
            "id": self.review_id,
            "status": "pending",
            "message": "Submitted for independent review.",
            "roleId": self.id,
            "revision": self.revision,
            "affected": self.affected,
        })
        .to_string()
    }

    pub fn audit_evidence(&self) -> Value {
        json!({
            "input": {
                "id": self.id,
                "name": self.name,
                "expectedVersion": self.expected_version,
                "permissions": self.permissions,
                "grantablePermissions": self.grantable_permissions,
                "scopes": self.scopes,
                "reason": self.reason,
            },
            "result": {
                "id": self.review_id,
                "status": "pending",
                "roleId": self.id,
                "revision": self.revision,
            },
            "affected": self.affected,
        })
    }

    pub fn outbox_payload(&self, authority: Value) -> Value {
        json!({
            "authority": authority,
            "id": self.review_id,
            "roleId": self.id,
            "name": self.name,
            "revision": self.revision,
            "expectedVersion": self.expected_version,
            "permissions": self.permissions,
            "grantablePermissions": self.grantable_permissions,
            "scopes": self.scopes,
            "affected": self.affected,
            "status": "pending",
            "createdAt": self.now,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ReviewDecisionPlan {
    pub review_id: String,
    pub expected_version: i64,
    pub decision: String,
    pub reason: String,
    pub publication: Option<RolePublication>,
    pub now: i64,
}

impl ReviewDecisionPlan {
    pub fn status(&self) -> &'static str {
        if self.decision == "approve" {
            "approved"
        } else {
            "rejected"
        }
    }

    pub fn receipt_json(&self) -> String {
        json!({
            "id": self.review_id,
            "status": self.status(),
        })
        .to_string()
    }

    pub fn audit_evidence(&self) -> Value {
        json!({
            "input": {
                "id": self.review_id,
                "expectedVersion": self.expected_version,
                "decision": self.decision,
                "reason": self.reason,
            },
            "result": {
                "id": self.review_id,
                "status": self.status(),
                "publication": self.publication.as_ref().map(|p| json!({
                    "roleId": p.id,
                    "revision": p.revision,
                    "name": p.name,
                })),
            },
        })
    }

    pub fn outbox_payload(&self, authority: Value) -> Value {
        json!({
            "authority": authority,
            "id": self.review_id,
            "status": self.status(),
            "decision": self.decision,
            "publication": self.publication.as_ref().map(|p| json!({
                "roleId": p.id,
                "revision": p.revision,
                "name": p.name,
            })),
            "decidedAt": self.now,
        })
    }
}

#[derive(Clone, Debug)]
pub struct AccessReviewStartPlan {
    pub campaign_id: String,
    pub name: String,
    pub due_at: i64,
    pub reason: String,
    pub assignments: Vec<ReviewAssignment>,
    pub started_event_id: String,
    pub now: i64,
}

impl AccessReviewStartPlan {
    pub fn receipt_json(&self) -> String {
        json!({
            "id": self.campaign_id,
            "status": "open",
            "version": 1,
            "itemCount": self.assignments.len(),
        })
        .to_string()
    }

    pub fn audit_evidence(&self) -> Value {
        json!({
            "input": {
                "id": self.campaign_id,
                "name": self.name,
                "dueAt": self.due_at,
                "reason": self.reason,
            },
            "result": {
                "id": self.campaign_id,
                "status": "open",
                "version": 1,
                "itemCount": self.assignments.len(),
            },
            "assignmentIds": self.assignments.iter().map(|a| &a.id).collect::<Vec<_>>(),
        })
    }

    pub fn outbox_payload(&self, authority: Value) -> Value {
        json!({
            "authority": authority,
            "id": self.campaign_id,
            "name": self.name,
            "dueAt": self.due_at,
            "status": "open",
            "version": 1,
            "assignmentIds": self.assignments.iter().map(|a| &a.id).collect::<Vec<_>>(),
            "createdAt": self.now,
        })
    }
}

#[derive(Clone, Debug)]
pub struct AccessRevocation {
    pub assignment_id: String,
    pub assignment_version: i64,
    pub user_id: String,
    pub role: String,
    pub company_id: String,
    pub scope_kind: String,
    pub scope_node_id: Option<String>,
    pub reason: String,
}

#[derive(Clone, Debug)]
pub struct AccessReviewDecisionPlan {
    pub item_id: String,
    pub campaign_id: String,
    pub decision: String,
    pub reason: String,
    pub expected_campaign_version: i64,
    pub resulting_campaign_version: i64,
    pub revocation: Option<AccessRevocation>,
    pub decision_event_id: String,
    pub completed_event_id: Option<String>,
    pub now: i64,
}

impl AccessReviewDecisionPlan {
    pub fn status(&self) -> &'static str {
        if self.revocation.is_some() {
            "revoked"
        } else {
            "kept"
        }
    }

    pub fn receipt_json(&self, campaign_status: &str) -> String {
        json!({
            "id": self.item_id,
            "campaignId": self.campaign_id,
            "decision": self.decision,
            "campaignStatus": campaign_status,
            "campaignVersion": self.resulting_campaign_version,
        })
        .to_string()
    }

    pub fn audit_evidence(&self, campaign_status: &str) -> Value {
        json!({
            "input": {
                "id": self.item_id,
                "expectedCampaignVersion": self.expected_campaign_version,
                "decision": self.decision,
                "reason": self.reason,
            },
            "result": {
                "id": self.item_id,
                "campaignId": self.campaign_id,
                "campaignStatus": campaign_status,
                "campaignVersion": self.resulting_campaign_version,
            },
            "revocation": self.revocation.as_ref().map(|r| json!({
                "assignmentId": r.assignment_id,
                "assignmentVersion": r.assignment_version + 1,
            })),
        })
    }

    pub fn outbox_payload(&self, authority: Value, campaign_status: &str) -> Value {
        json!({
            "authority": authority,
            "id": self.item_id,
            "campaignId": self.campaign_id,
            "decision": self.decision,
            "campaignStatus": campaign_status,
            "campaignVersion": self.resulting_campaign_version,
            "revocation": self.revocation.as_ref().map(|r| json!({
                "assignmentId": r.assignment_id,
                "userId": r.user_id,
                "role": r.role,
            })),
            "decidedAt": self.now,
        })
    }
}

#[derive(Clone, Debug)]
pub struct AccessReviewCancelPlan {
    pub campaign_id: String,
    pub expected_version: i64,
    pub resulting_version: i64,
    pub reason: String,
    pub cancelled_event_id: String,
    pub now: i64,
}

impl AccessReviewCancelPlan {
    pub fn receipt_json(&self) -> String {
        json!({
            "id": self.campaign_id,
            "status": "cancelled",
            "version": self.resulting_version,
        })
        .to_string()
    }

    pub fn audit_evidence(&self) -> Value {
        json!({
            "input": {
                "id": self.campaign_id,
                "expectedVersion": self.expected_version,
                "reason": self.reason,
            },
            "result": {
                "id": self.campaign_id,
                "status": "cancelled",
                "version": self.resulting_version,
            },
        })
    }

    pub fn outbox_payload(&self, authority: Value) -> Value {
        json!({
            "authority": authority,
            "id": self.campaign_id,
            "status": "cancelled",
            "version": self.resulting_version,
            "cancelledAt": self.now,
        })
    }
}

#[derive(Clone, Debug)]
pub struct IamReceiptSnapshot {
    pub request_hash: String,
    pub result_json: String,
    pub operation: String,
    pub actor_id: String,
    pub record_id: String,
    pub authority_json: String,
}

pub fn role_save_identity(actor: &IamRoleActor, key: &str, input: &ValidatedRoleSave) -> String {
    json!({
        "actor": actor.user_id.as_str(),
        "assignment": actor.assignment_id.as_str(),
        "company": actor.company_id.as_str(),
        "operation": ROLE_SAVE_OPERATION,
        "key": key,
        "payload": {
            "id": input.id,
            "name": input.name,
            "expectedVersion": input.expected_version,
            "permissions": input.permissions,
            "grantablePermissions": input.grantable_permissions,
            "scopes": input.scopes,
            "reason": input.reason,
        }
    })
    .to_string()
}

pub fn review_decision_identity(
    actor: &IamRoleActor,
    key: &str,
    review_id: &str,
    input: &ValidatedReviewDecision,
) -> String {
    json!({
        "actor": actor.user_id.as_str(),
        "assignment": actor.assignment_id.as_str(),
        "company": actor.company_id.as_str(),
        "operation": REVIEW_DECIDE_OPERATION,
        "key": key,
        "payload": {
            "id": review_id,
            "expectedVersion": input.expected_version,
            "decision": input.decision,
            "reason": input.reason,
        }
    })
    .to_string()
}

pub fn campaign_start_identity(
    actor: &IamRoleActor,
    key: &str,
    input: &ValidatedCampaign,
) -> String {
    json!({
        "actor": actor.user_id.as_str(),
        "assignment": actor.assignment_id.as_str(),
        "company": actor.company_id.as_str(),
        "operation": ACCESS_REVIEW_START_OPERATION,
        "key": key,
        "payload": {
            "name": input.name,
            "dueAt": input.due_at,
            "reason": input.reason,
        }
    })
    .to_string()
}

pub fn campaign_decision_identity(
    actor: &IamRoleActor,
    key: &str,
    item_id: &str,
    input: &ValidatedCampaignDecision,
) -> String {
    json!({
        "actor": actor.user_id.as_str(),
        "assignment": actor.assignment_id.as_str(),
        "company": actor.company_id.as_str(),
        "operation": ACCESS_REVIEW_DECIDE_OPERATION,
        "key": key,
        "payload": {
            "itemId": item_id,
            "expectedCampaignVersion": input.expected_campaign_version,
            "decision": input.decision,
            "reason": input.reason,
        }
    })
    .to_string()
}

pub fn campaign_cancel_identity(
    actor: &IamRoleActor,
    key: &str,
    campaign_id: &str,
    input: &ValidatedCancel,
) -> String {
    json!({
        "actor": actor.user_id.as_str(),
        "assignment": actor.assignment_id.as_str(),
        "company": actor.company_id.as_str(),
        "operation": ACCESS_REVIEW_CANCEL_OPERATION,
        "key": key,
        "payload": {
            "id": campaign_id,
            "expectedVersion": input.expected_version,
            "reason": input.reason,
        }
    })
    .to_string()
}

/// The kind determines the proposer permission rechecked on approval.
pub fn required_permission(kind: &str) -> &'static str {
    if kind == REVIEW_KIND_ROLE_PUBLISH {
        "iam.roles.manage"
    } else if kind == "people.provision.grant" {
        "iam.directory.apply"
    } else if matches!(kind, "connection.activate" | "directory.operation") {
        "iam.directory.manage"
    } else if kind.starts_with("emergency.") {
        "iam.recovery"
    } else {
        "iam.manage"
    }
}

pub fn review_scope_covered(actor: &IamRoleActor, kind: &str, payload: &Value) -> bool {
    if actor.platform || actor.scope_kind == "holding" {
        return true;
    }
    if !matches!(
        kind,
        REVIEW_KIND_ASSIGNMENT_GRANT | REVIEW_KIND_MAPPING_ACTIVATE | REVIEW_KIND_POLICY_GRANT
    ) {
        return false;
    }
    let scope_kind = payload["scopeKind"].as_str().unwrap_or_default();
    let scope_node = payload["scopeNodeId"].as_str();
    scope_kind != "holding"
        && scope_node.is_some_and(|node| {
            actor
                .scope_node_id
                .as_ref()
                .is_some_and(|current| current.as_str() == node)
        })
}

pub fn ensure_independent_review(
    actor: &IamRoleActor,
    review: &ReviewRow,
    same_person: bool,
) -> Result<(), BusinessError> {
    let actor_person = actor
        .person_id
        .as_ref()
        .map(RecordId::as_str)
        .unwrap_or_else(|| actor.user_id.as_str());
    if review.created_by == actor.user_id.as_str()
        || review.created_person == actor_person
        || review.created_person == actor.user_id.as_str()
        || same_person
    {
        return Err(independent_review_required());
    }
    Ok(())
}

pub fn ensure_campaign_independent(
    actor: &IamRoleActor,
    item: &ReviewItemRow,
    same_person: bool,
) -> Result<(), BusinessError> {
    if item.created_by == actor.user_id.as_str() {
        return Err(independent_review_required());
    }
    if item.user_id == actor.user_id.as_str()
        || actor.same_person_user(&item.user_id)
        || same_person
    {
        return Err(self_access_review());
    }
    Ok(())
}

pub fn ensure_campaign_source(
    item: &ReviewItemRow,
    assignment: &ReviewAssignment,
    active: bool,
) -> Result<(), BusinessError> {
    if assignment.id != item.assignment_id
        || assignment.version != item.assignment_version
        || assignment.revoked_at.is_some()
        || !active
    {
        return Err(review_source_changed());
    }
    Ok(())
}

/// The proposed revision is revalidated against the proposer's still-current
/// delegation and the reviewer's own delegation before publication.
pub fn ensure_publication_delegation(
    actor: &IamRoleActor,
    payload: &RolePublishPayload,
    proposer: &ProposerAuthority,
) -> Result<(), BusinessError> {
    if payload
        .permissions
        .iter()
        .chain(payload.grantable_permissions.iter())
        .any(|permission| {
            !proposer
                .grantable_permissions
                .iter()
                .any(|value| value == permission)
        })
    {
        return Err(delegation_exceeded());
    }
    if payload
        .permissions
        .iter()
        .chain(payload.grantable_permissions.iter())
        .any(|permission| !actor.owns_grantable(permission))
    {
        return Err(delegation_exceeded());
    }
    Ok(())
}

/// Per-assignment guards for a revoke decision. Recovery membership requires
/// `iam.recovery`; caller checks that before invoking this function.
pub fn ensure_revocation_allowed(
    actor: &IamRoleActor,
    assignment: &ReviewAssignment,
    facts: &RevocationGuardFacts,
) -> Result<(), BusinessError> {
    if actor.same_person_user(&assignment.user_id) || facts.same_person {
        return Err(self_access_change());
    }
    if actor.scope_kind != "holding"
        && !actor.platform
        && actor.scope_node_id.as_ref().map(RecordId::as_str) != assignment.scope_node_id.as_deref()
    {
        return Err(scope_forbidden());
    }
    if facts.recovery_member && !facts.other_recovery_administrator {
        return Err(last_recovery_administrator());
    }
    if matches!(assignment.role.as_str(), "holding_owner" | "platform_admin")
        && !facts.other_administrator
    {
        return Err(last_administrator());
    }
    Ok(())
}

pub fn campaign_id(key: &str) -> String {
    format!("ar-campaign-{key}")
}
pub fn campaign_item_id(key: &str, index: usize) -> String {
    format!("ar-item-{key}-{index}")
}
pub fn campaign_started_event_id(key: &str) -> String {
    format!("ar-event-{key}")
}
pub fn campaign_decision_event_id(key: &str) -> String {
    format!("ar-event-{key}-d")
}
pub fn campaign_completed_event_id(key: &str) -> String {
    format!("ar-event-{key}-c")
}
pub fn campaign_cancelled_event_id(key: &str) -> String {
    format!("ar-event-{key}-x")
}
pub fn role_review_id(key: &str) -> String {
    format!("iam-review-{key}")
}
