use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use erpx_kernel::AuthorityScope;

use crate::commercial::{
    BillingState, EntitlementAllocationBook, ModuleEntitlement, authorize_module,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskTier {
    PlatformRoot,
    Administrator,
    High,
    Standard,
    Contractor,
    SupportDiagnostic,
    SupportMutation,
    Emergency,
}

impl RiskTier {
    pub const fn maximum_duration(self) -> Duration {
        match self {
            Self::PlatformRoot | Self::Administrator | Self::High => Duration::days(90),
            Self::Standard => Duration::days(365),
            Self::Contractor => Duration::days(30),
            Self::SupportDiagnostic => Duration::hours(8),
            Self::SupportMutation => Duration::hours(2),
            Self::Emergency => Duration::hours(1),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AssignmentError {
    #[error("assignment window must end after it starts")]
    InvalidWindow,
    #[error("role assignments must have a finite expiry")]
    PermanentAssignment,
    #[error("assignment window exceeds the {maximum:?} risk-tier maximum")]
    ExceedsMaximum { maximum: Duration },
    #[error("child assignment starts before its parent")]
    ChildStartsBeforeParent,
    #[error("parent assignment is required for a delegated role")]
    ParentAssignmentRequired,
    #[error("delegated role does not reference the supplied parent")]
    ParentIdentityMismatch,
    #[error("child assignment scope is outside its parent")]
    ScopeExceedsParent,
    #[error("child assignment outlives its parent")]
    ChildOutlivesParent,
    #[error("child assignment permission is not contained by its parent: {permission}")]
    PermissionExceedsParent { permission: String },
    #[error("child assignment authority exceeds its parent")]
    AuthorityExceedsParent,
    #[error("high-risk assignments require an approver")]
    MissingApprover,
    #[error("high-risk assignments require an approver other than the grantor")]
    ApproverMustDifferFromGrantor,
    #[error("assignment is not active at the requested time")]
    NotActive,
    #[error("assignment version cannot be incremented")]
    VersionOverflow,
    #[error("renewal reviewer is required for this operation")]
    MissingRenewalReviewer,
    #[error("renewal reviewer must differ from the grantor")]
    RenewalReviewerMustDifferFromGrantor,
    #[error("renewal reviewer must differ from the approver")]
    RenewalReviewerMustDifferFromApprover,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveAccessRequest {
    /// The identity that the transport resolved from the active session. The
    /// evaluator binds it to the assignment so a caller cannot supply a role
    /// row for a different principal.
    pub principal_ref: String,
    pub at: DateTime<Utc>,
    pub requested_scope: AuthorityScope,
    pub module_key: String,
    pub required_permission: String,
    pub requested_approval_minor: u64,
    pub approval_limit_minor: u64,
    pub identity_active: bool,
    pub session_active: bool,
    pub billing_state: BillingState,
    pub entitlement: Option<ModuleEntitlement>,
    /// Authoritative allocation state for company/branch requests. Keeping
    /// this beside the purchased entitlement makes the shared evaluator the
    /// single intersection for subscription, activation, and RBAC checks.
    pub entitlement_book: Option<EntitlementAllocationBook>,
    pub is_mutation: bool,
    pub segregation_of_duties_passed: bool,
    pub support_capability_required: bool,
    pub support_capability_active: bool,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EffectiveAccessDenial {
    #[error("role assignment belongs to a different principal")]
    PrincipalMismatch,
    #[error("identity is inactive")]
    IdentityInactive,
    #[error("session is inactive")]
    SessionInactive,
    #[error("role assignment is outside its validity window")]
    AssignmentInactive,
    #[error("parent role assignment is missing or inactive")]
    ParentInactive,
    #[error("requested scope is outside the role assignment")]
    ScopeDenied,
    #[error("required permission is not assigned")]
    PermissionDenied,
    #[error("requested approval exceeds the authority limit")]
    ApprovalLimitExceeded,
    #[error("segregation of duties check failed")]
    SegregationOfDutiesFailed,
    #[error("support capability is missing or inactive")]
    SupportCapabilityInactive,
    #[error("{0}")]
    CommercialDenied(String),
}

/// Evaluate every universal access condition at one instant. Role windows
/// are half-open (`valid_from <= at < valid_until`) and parent revocation is
/// cascaded by requiring the parent to be effective at the same instant.
pub fn evaluate_effective_access(
    assignment: &RoleAssignment,
    parent: Option<&RoleAssignment>,
    request: &EffectiveAccessRequest,
) -> Result<(), EffectiveAccessDenial> {
    let parents = parent.map_or(&[][..], std::slice::from_ref);
    evaluate_effective_access_chain(assignment, parents, request)
}

/// Evaluate an assignment against its complete immediate-to-root parent
/// chain. Every edge is rechecked at the same instant, so revoking or
/// expiring any ancestor immediately invalidates all descendants.
pub fn evaluate_effective_access_chain(
    assignment: &RoleAssignment,
    parents: &[RoleAssignment],
    request: &EffectiveAccessRequest,
) -> Result<(), EffectiveAccessDenial> {
    if assignment.principal_ref != request.principal_ref {
        return Err(EffectiveAccessDenial::PrincipalMismatch);
    }
    if !request.identity_active {
        return Err(EffectiveAccessDenial::IdentityInactive);
    }
    if !request.session_active {
        return Err(EffectiveAccessDenial::SessionInactive);
    }
    if !assignment.is_active_at(request.at) {
        return Err(EffectiveAccessDenial::AssignmentInactive);
    }
    let mut child_scope = &assignment.scope;
    let mut child_authority = assignment.authority_level;
    let mut child_permissions = assignment.permissions.as_slice();
    let mut expected_parent_id = assignment.parent_assignment_id;
    let mut seen = Vec::with_capacity(parents.len().saturating_add(1));
    for parent in parents {
        let Some(parent_id) = expected_parent_id else {
            return Err(EffectiveAccessDenial::ParentInactive);
        };
        if parent.id != parent_id || seen.contains(&parent.id) || !parent.is_active_at(request.at) {
            return Err(EffectiveAccessDenial::ParentInactive);
        }
        if !parent.scope.contains(child_scope)
            || child_authority > parent.authority_level
            || child_permissions
                .iter()
                .any(|permission| !parent.permissions.contains(permission))
        {
            return Err(EffectiveAccessDenial::ParentInactive);
        }
        seen.push(parent.id);
        child_scope = &parent.scope;
        child_authority = parent.authority_level;
        child_permissions = parent.permissions.as_slice();
        expected_parent_id = parent.parent_assignment_id;
    }
    if expected_parent_id.is_some() {
        return Err(EffectiveAccessDenial::ParentInactive);
    }
    if !assignment.scope.contains(&request.requested_scope) {
        return Err(EffectiveAccessDenial::ScopeDenied);
    }
    if !assignment
        .permissions
        .iter()
        .any(|permission| permission == &request.required_permission)
    {
        return Err(EffectiveAccessDenial::PermissionDenied);
    }
    if request.requested_approval_minor > request.approval_limit_minor {
        return Err(EffectiveAccessDenial::ApprovalLimitExceeded);
    }
    if !request.segregation_of_duties_passed {
        return Err(EffectiveAccessDenial::SegregationOfDutiesFailed);
    }
    if request.support_capability_required && !request.support_capability_active {
        return Err(EffectiveAccessDenial::SupportCapabilityInactive);
    }
    if request
        .entitlement
        .as_ref()
        .is_some_and(|entitlement| entitlement.module_key != request.module_key)
        || request
            .entitlement_book
            .as_ref()
            .is_some_and(|book| book.module_key != request.module_key)
    {
        return Err(EffectiveAccessDenial::CommercialDenied(
            "module_mismatch".into(),
        ));
    }
    let entitlement = request
        .entitlement_book
        .as_ref()
        .map(EntitlementAllocationBook::module_entitlement)
        .or_else(|| request.entitlement.clone());
    match authorize_module(
        request.billing_state,
        entitlement.as_ref(),
        true,
        request.at,
        request.is_mutation,
    ) {
        crate::commercial::EntitlementDecision::Allowed => {}
        crate::commercial::EntitlementDecision::Denied(reason) => {
            return Err(EffectiveAccessDenial::CommercialDenied(reason.into()));
        }
    }

    let Some(book) = request.entitlement_book.as_ref() else {
        if matches!(
            request.requested_scope,
            AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
        ) {
            return Err(EffectiveAccessDenial::CommercialDenied(
                "allocation_context_missing".into(),
            ));
        }
        return Ok(());
    };

    let allocation_decision = match request.requested_scope {
        AuthorityScope::Company { company_id, .. } => book.authorize(
            request.billing_state,
            company_id,
            None,
            true,
            request.at,
            request.is_mutation,
        ),
        AuthorityScope::Branch {
            company_id,
            branch_id,
            ..
        } => book.authorize(
            request.billing_state,
            company_id,
            Some(branch_id),
            true,
            request.at,
            request.is_mutation,
        ),
        AuthorityScope::Platform
        | AuthorityScope::Holding { .. }
        | AuthorityScope::Subtree { .. } => crate::commercial::EntitlementDecision::Allowed,
    };
    match allocation_decision {
        crate::commercial::EntitlementDecision::Allowed => Ok(()),
        crate::commercial::EntitlementDecision::Denied(reason) => {
            Err(EffectiveAccessDenial::CommercialDenied(reason.into()))
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ApproverScopeDenial {
    #[error("assignment does not name an approver")]
    ApproverRequired,
    #[error("approver identity does not match the assignment")]
    ApproverIdentityMismatch,
    #[error("approver assignment is inactive")]
    ApproverInactive,
    #[error("approver parent assignment is missing or inactive")]
    ApproverParentInactive,
    #[error("approver scope does not contain the assignment scope")]
    ApproverScopeDenied,
    #[error("approver lacks role.assignment.approve")]
    ApproverPermissionDenied,
    #[error("approver authority is below the assignment authority")]
    ApproverAuthorityDenied,
}

/// Evaluate the domain-side intersection required before an approver can
/// authorize a role assignment. The approver must be the immutable identity
/// recorded on the assignment, must have an effective role at `at`, and that
/// role's scope, authority, and approval permission must contain the target
/// assignment. Parent roles are rechecked in order so a revoked or narrowed
/// ancestor cannot leave an approver effective through a stale child row.
pub fn evaluate_approver_scope_intersection(
    assignment: &RoleAssignment,
    approver: &RoleAssignment,
    approver_parents: &[RoleAssignment],
    at: DateTime<Utc>,
) -> Result<(), ApproverScopeDenial> {
    let Some(expected_approver) = assignment.approver.as_deref() else {
        return Err(ApproverScopeDenial::ApproverRequired);
    };
    if expected_approver.trim().is_empty() {
        return Err(ApproverScopeDenial::ApproverRequired);
    }
    if expected_approver != approver.principal_ref {
        return Err(ApproverScopeDenial::ApproverIdentityMismatch);
    }
    if !approver.is_active_at(at) {
        return Err(ApproverScopeDenial::ApproverInactive);
    }

    let mut child_scope = &approver.scope;
    let mut child_authority = approver.authority_level;
    let mut child_permissions = approver.permissions.as_slice();
    let mut expected_parent_id = approver.parent_assignment_id;
    let mut seen = Vec::with_capacity(approver_parents.len().saturating_add(1));
    for parent in approver_parents {
        let Some(parent_id) = expected_parent_id else {
            return Err(ApproverScopeDenial::ApproverParentInactive);
        };
        if parent.id != parent_id || seen.contains(&parent.id) || !parent.is_active_at(at) {
            return Err(ApproverScopeDenial::ApproverParentInactive);
        }
        if !parent.scope.contains(child_scope)
            || child_authority > parent.authority_level
            || child_permissions
                .iter()
                .any(|permission| !parent.permissions.contains(permission))
        {
            return Err(ApproverScopeDenial::ApproverParentInactive);
        }
        seen.push(parent.id);
        child_scope = &parent.scope;
        child_authority = parent.authority_level;
        child_permissions = parent.permissions.as_slice();
        expected_parent_id = parent.parent_assignment_id;
    }
    if expected_parent_id.is_some() {
        return Err(ApproverScopeDenial::ApproverParentInactive);
    }
    if !approver.scope.contains(&assignment.scope) {
        return Err(ApproverScopeDenial::ApproverScopeDenied);
    }
    if !approver
        .permissions
        .iter()
        .any(|permission| permission == "role.assignment.approve")
    {
        return Err(ApproverScopeDenial::ApproverPermissionDenied);
    }
    if approver.authority_level < assignment.authority_level {
        return Err(ApproverScopeDenial::ApproverAuthorityDenied);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleAssignment {
    pub id: Uuid,
    pub principal_ref: String,
    pub role_key: String,
    pub permissions: Vec<String>,
    pub functional_domain: String,
    pub authority_level: u8,
    /// Maximum approval amount in minor currency units for this assignment.
    /// Zero is an explicit no-approval limit, including legacy assignments
    /// deserialized without this field.
    #[serde(default)]
    pub approval_limit_minor: u64,
    pub scope: AuthorityScope,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub timezone: String,
    pub risk_tier: RiskTier,
    pub grant_reason: String,
    pub ticket_reference: Option<String>,
    pub grantor: String,
    pub approver: Option<String>,
    pub renewal_reviewer: Option<String>,
    pub parent_assignment_id: Option<Uuid>,
    pub assignment_version: u32,
}

impl RoleAssignment {
    pub fn validate_window(
        valid_from: DateTime<Utc>,
        valid_until: DateTime<Utc>,
        risk_tier: RiskTier,
    ) -> Result<(), AssignmentError> {
        if valid_until == DateTime::<Utc>::MAX_UTC {
            return Err(AssignmentError::PermanentAssignment);
        }
        if valid_until <= valid_from {
            return Err(AssignmentError::InvalidWindow);
        }
        let duration = valid_until - valid_from;
        let maximum = risk_tier.maximum_duration();
        if duration > maximum {
            return Err(AssignmentError::ExceedsMaximum { maximum });
        }
        Ok(())
    }

    pub fn is_active_at(&self, at: DateTime<Utc>) -> bool {
        self.valid_from <= at && at < self.valid_until
    }

    pub fn effective_at(&self, at: DateTime<Utc>, parent: Option<&Self>) -> bool {
        if !self.is_active_at(at) {
            return false;
        }
        match (self.parent_assignment_id, parent) {
            (None, None) => true,
            (Some(parent_id), Some(assignment)) => {
                parent_id == assignment.id
                    && assignment.is_active_at(at)
                    && assignment.scope.contains(&self.scope)
            }
            _ => false,
        }
    }

    pub fn validate_delegation(&self, parent: &Self) -> Result<(), AssignmentError> {
        if self.parent_assignment_id != Some(parent.id) {
            return Err(AssignmentError::ParentIdentityMismatch);
        }
        if self.valid_from < parent.valid_from {
            return Err(AssignmentError::ChildStartsBeforeParent);
        }
        if !parent.scope.contains(&self.scope) {
            return Err(AssignmentError::ScopeExceedsParent);
        }
        if self.valid_until > parent.valid_until {
            return Err(AssignmentError::ChildOutlivesParent);
        }
        if self.authority_level > parent.authority_level {
            return Err(AssignmentError::AuthorityExceedsParent);
        }
        if let Some(permission) = self
            .permissions
            .iter()
            .find(|permission| !parent.permissions.contains(permission))
        {
            return Err(AssignmentError::PermissionExceedsParent {
                permission: permission.clone(),
            });
        }
        if !parent.is_active_at(self.valid_from) {
            return Err(AssignmentError::NotActive);
        }
        Ok(())
    }

    pub fn validate_high_risk_approval(&self) -> Result<(), AssignmentError> {
        if !matches!(
            self.risk_tier,
            RiskTier::PlatformRoot | RiskTier::Administrator | RiskTier::High
        ) {
            return Ok(());
        }
        let Some(approver) = self.approver.as_deref() else {
            return Err(AssignmentError::MissingApprover);
        };
        if approver.trim().is_empty() {
            return Err(AssignmentError::MissingApprover);
        }
        if approver == self.grantor {
            return Err(AssignmentError::ApproverMustDifferFromGrantor);
        }
        Ok(())
    }

    pub fn validate_for_grant(&self, parent: Option<&Self>) -> Result<(), AssignmentError> {
        Self::validate_window(self.valid_from, self.valid_until, self.risk_tier)?;
        self.validate_high_risk_approval()?;
        match (self.parent_assignment_id, parent) {
            (Some(_), Some(parent)) => self.validate_delegation(parent)?,
            (Some(_), None) => return Err(AssignmentError::ParentAssignmentRequired),
            (None, Some(_)) => return Err(AssignmentError::ParentIdentityMismatch),
            (None, None) => {}
        }
        Ok(())
    }

    pub fn renew(
        &self,
        valid_from: DateTime<Utc>,
        valid_until: DateTime<Utc>,
        grantor: impl Into<String>,
        approver: Option<String>,
    ) -> Result<Self, AssignmentError> {
        Self::validate_window(valid_from, valid_until, self.risk_tier)?;
        if valid_from < self.valid_from {
            return Err(AssignmentError::ChildStartsBeforeParent);
        }
        let renewed = Self {
            id: Uuid::new_v4(),
            valid_from,
            valid_until,
            grantor: grantor.into(),
            approver,
            assignment_version: self
                .assignment_version
                .checked_add(1)
                .ok_or(AssignmentError::VersionOverflow)?,
            ..self.clone()
        };
        renewed.validate_high_risk_approval()?;
        Ok(renewed)
    }

    /// Renew an assignment while recording an explicit independent reviewer.
    /// The ordinary `renew` method remains available for lower-risk role
    /// renewals; this path is used when a review workflow has explicitly
    /// collected a reviewer decision.
    pub fn renew_with_reviewer(
        &self,
        valid_from: DateTime<Utc>,
        valid_until: DateTime<Utc>,
        grantor: impl Into<String>,
        approver: Option<String>,
        renewal_reviewer: impl Into<String>,
    ) -> Result<Self, AssignmentError> {
        let reviewer = renewal_reviewer.into();
        if reviewer.trim().is_empty() {
            return Err(AssignmentError::MissingRenewalReviewer);
        }
        let grantor = grantor.into();
        if reviewer == grantor {
            return Err(AssignmentError::RenewalReviewerMustDifferFromGrantor);
        }
        if approver.as_deref() == Some(reviewer.as_str()) {
            return Err(AssignmentError::RenewalReviewerMustDifferFromApprover);
        }
        let mut renewed = self.renew(valid_from, valid_until, grantor, approver)?;
        renewed.renewal_reviewer = Some(reviewer);
        Ok(renewed)
    }

    /// Return a canonical copy suitable for hashing or persistence replay.
    /// Permission order is not semantically meaningful, so it is normalized
    /// before serialization while every other assignment field is retained.
    pub fn canonicalized(&self) -> Self {
        let mut value = self.clone();
        value.permissions.sort_unstable();
        value
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.canonicalized())
    }
}

pub const PLATFORM_ROOT_ROLE: &str = "platform_root";
pub const HOLDING_OWNER_ROLE: &str = "holding_owner";
pub const CUSTOMER_SECURITY_HEAD_ROLE: &str = "customer_security_head";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoleReviewClassification {
    Expired,
    ExpiringSoon,
    Orphaned,
    Excessive,
    Conflicting,
    Inactive,
}

impl RoleReviewClassification {
    fn sort_key(self) -> u8 {
        match self {
            Self::Expired => 0,
            Self::ExpiringSoon => 1,
            Self::Orphaned => 2,
            Self::Excessive => 3,
            Self::Conflicting => 4,
            Self::Inactive => 5,
        }
    }

    pub const fn code(self) -> &'static str {
        match self {
            Self::Expired => "expired",
            Self::ExpiringSoon => "expiring_soon",
            Self::Orphaned => "orphaned",
            Self::Excessive => "excessive",
            Self::Conflicting => "conflicting",
            Self::Inactive => "inactive",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoleExpiryNotificationKind {
    ExpiringSoon,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleExpiryNotification {
    pub assignment_id: Uuid,
    pub principal_ref: String,
    pub valid_until: DateTime<Utc>,
    pub kind: RoleExpiryNotificationKind,
}

impl RoleExpiryNotification {
    pub fn stable_key(&self) -> String {
        format!(
            "role-expiry:{}:{}:{}",
            self.assignment_id,
            self.valid_until
                .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            match self.kind {
                RoleExpiryNotificationKind::ExpiringSoon => "expiring_soon",
                RoleExpiryNotificationKind::Expired => "expired",
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleReviewItem {
    pub assignment_id: Uuid,
    pub principal_ref: String,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub parent_assignment_id: Option<Uuid>,
    pub classifications: Vec<RoleReviewClassification>,
    pub notification: Option<RoleExpiryNotification>,
    pub assignment: RoleAssignment,
}

impl RoleReviewItem {
    pub fn canonicalized(&self) -> Self {
        let mut value = self.clone();
        value
            .classifications
            .sort_by_key(|classification| classification.sort_key());
        value.assignment = value.assignment.canonicalized();
        value
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.canonicalized())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleReviewQueue {
    pub evaluated_at: DateTime<Utc>,
    pub expiry_horizon_until: DateTime<Utc>,
    pub items: Vec<RoleReviewItem>,
}

impl RoleReviewQueue {
    pub fn canonicalized(&self) -> Self {
        let mut value = self.clone();
        value.items = value
            .items
            .into_iter()
            .map(|item| item.canonicalized())
            .collect();
        value.items.sort_by_key(|item| item.assignment_id);
        value
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.canonicalized())
    }

    pub fn notifications(&self) -> Vec<RoleExpiryNotification> {
        self.items
            .iter()
            .filter_map(|item| item.notification.clone())
            .collect()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RoleReviewError {
    #[error("expiry review horizon must be positive")]
    InvalidHorizon,
    #[error("recovery requester is required")]
    RecoveryRequesterRequired,
    #[error("recovery approver is required")]
    RecoveryApproverRequired,
    #[error("recovery approver must differ from requester")]
    RecoveryApproverMustDiffer,
    #[error("recovery reviewer must differ from requester")]
    RecoveryReviewerMustDifferFromRequester,
    #[error("recovery reviewer must differ from approver")]
    RecoveryReviewerMustDifferFromApprover,
    #[error("recovery target already has an active assignment")]
    RecoveryTargetAlreadyActive,
    #[error("recovery must start at the evaluation instant")]
    RecoveryMustStartNow,
    #[error("recovery principal is required")]
    RecoveryPrincipalRequired,
    #[error("recovery reason is required")]
    RecoveryReasonRequired,
    #[error("recovery functional domain is required")]
    RecoveryFunctionalDomainRequired,
    #[error("recovery timezone is required")]
    RecoveryTimezoneRequired,
    #[error("recovery permissions are required")]
    RecoveryPermissionsRequired,
    #[error("recovery assignment cannot have a parent")]
    RecoveryParentNotAllowed,
    #[error("recovery request does not match its target")]
    RecoveryTargetMismatch,
    #[error(transparent)]
    Assignment(#[from] AssignmentError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoleRecoveryTarget {
    PlatformRoot,
    HoldingOwner { holding_id: Uuid },
}

impl RoleRecoveryTarget {
    fn role_key(&self) -> &'static str {
        match self {
            Self::PlatformRoot => PLATFORM_ROOT_ROLE,
            Self::HoldingOwner { .. } => HOLDING_OWNER_ROLE,
        }
    }

    fn risk_tier(&self) -> RiskTier {
        match self {
            Self::PlatformRoot => RiskTier::PlatformRoot,
            Self::HoldingOwner { .. } => RiskTier::Administrator,
        }
    }

    fn scope(&self) -> AuthorityScope {
        match self {
            Self::PlatformRoot => AuthorityScope::Platform,
            Self::HoldingOwner { holding_id } => AuthorityScope::Holding {
                holding_id: *holding_id,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleRecoveryRequest {
    pub request_id: Uuid,
    pub target: RoleRecoveryTarget,
    pub principal_ref: String,
    pub permissions: Vec<String>,
    pub functional_domain: String,
    pub authority_level: u8,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub timezone: String,
    pub grant_reason: String,
    pub ticket_reference: Option<String>,
    pub requester_ref: String,
    pub approver_ref: String,
    pub reviewer_ref: Option<String>,
}

impl RoleRecoveryRequest {
    /// Validate and materialize a final-root/final-owner recovery. Existing
    /// active target assignments are checked across all principals, so a
    /// recovery cannot silently create a second emergency authority.
    pub fn materialize(
        &self,
        assignments: &[RoleAssignment],
        at: DateTime<Utc>,
    ) -> Result<RoleAssignment, RoleReviewError> {
        if self.requester_ref.trim().is_empty() {
            return Err(RoleReviewError::RecoveryRequesterRequired);
        }
        if self.approver_ref.trim().is_empty() {
            return Err(RoleReviewError::RecoveryApproverRequired);
        }
        if self.requester_ref == self.approver_ref {
            return Err(RoleReviewError::RecoveryApproverMustDiffer);
        }
        if let Some(reviewer) = self.reviewer_ref.as_deref() {
            if reviewer.trim().is_empty() || reviewer == self.requester_ref {
                return Err(RoleReviewError::RecoveryReviewerMustDifferFromRequester);
            }
            if reviewer == self.approver_ref {
                return Err(RoleReviewError::RecoveryReviewerMustDifferFromApprover);
            }
        }
        if self.principal_ref.trim().is_empty() {
            return Err(RoleReviewError::RecoveryPrincipalRequired);
        }
        if self.grant_reason.trim().is_empty() {
            return Err(RoleReviewError::RecoveryReasonRequired);
        }
        if self.functional_domain.trim().is_empty() {
            return Err(RoleReviewError::RecoveryFunctionalDomainRequired);
        }
        if self.timezone.trim().is_empty() {
            return Err(RoleReviewError::RecoveryTimezoneRequired);
        }
        if self.permissions.is_empty()
            || self.permissions.iter().any(|value| value.trim().is_empty())
        {
            return Err(RoleReviewError::RecoveryPermissionsRequired);
        }
        if self.valid_from != at {
            return Err(RoleReviewError::RecoveryMustStartNow);
        }
        if assignments.iter().any(|assignment| {
            assignment.role_key == self.target.role_key()
                && assignment.scope == self.target.scope()
                && assignment.is_active_at(at)
        }) {
            return Err(RoleReviewError::RecoveryTargetAlreadyActive);
        }

        let mut permissions = self.permissions.clone();
        permissions.sort_unstable();
        permissions.dedup();
        let assignment = RoleAssignment {
            id: self.request_id,
            principal_ref: self.principal_ref.clone(),
            role_key: self.target.role_key().into(),
            permissions,
            functional_domain: self.functional_domain.clone(),
            authority_level: self.authority_level,
            approval_limit_minor: 0,
            scope: self.target.scope(),
            valid_from: self.valid_from,
            valid_until: self.valid_until,
            timezone: self.timezone.clone(),
            risk_tier: self.target.risk_tier(),
            grant_reason: self.grant_reason.clone(),
            ticket_reference: self.ticket_reference.clone(),
            grantor: self.requester_ref.clone(),
            approver: Some(self.approver_ref.clone()),
            renewal_reviewer: self.reviewer_ref.clone(),
            parent_assignment_id: None,
            assignment_version: 1,
        };
        assignment.validate_for_grant(None)?;
        Ok(assignment)
    }

    pub fn canonicalized(&self) -> Self {
        let mut value = self.clone();
        value.permissions.sort_unstable();
        value
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.canonicalized())
    }
}

fn scopes_overlap(left: &AuthorityScope, right: &AuthorityScope) -> bool {
    left.contains(right) || right.contains(left)
}

fn windows_overlap(left: &RoleAssignment, right: &RoleAssignment) -> bool {
    left.valid_from < right.valid_until && right.valid_from < left.valid_until
}

fn permission_is_maker(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "create", "write", "submit", "request", "prepare", "process", "execute", "post", "adjust",
    ]
    .iter()
    .any(|marker| value.ends_with(marker) || value.contains(&format!(".{marker}")))
}

fn permission_is_checker(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    ["approve", "authorize", "review", "reconcile"]
        .iter()
        .any(|marker| value.ends_with(marker) || value.contains(&format!(".{marker}")))
}

fn roles_conflict(left: &RoleAssignment, right: &RoleAssignment) -> bool {
    left.principal_ref == right.principal_ref
        && windows_overlap(left, right)
        && scopes_overlap(&left.scope, &right.scope)
        && ((left
            .permissions
            .iter()
            .any(|permission| permission_is_maker(permission))
            && right
                .permissions
                .iter()
                .any(|permission| permission_is_checker(permission)))
            || (right
                .permissions
                .iter()
                .any(|permission| permission_is_maker(permission))
                && left
                    .permissions
                    .iter()
                    .any(|permission| permission_is_checker(permission))))
}

fn review_classifications(
    assignment: &RoleAssignment,
    parent: Option<&RoleAssignment>,
    conflicts: bool,
    at: DateTime<Utc>,
    expiry_horizon_until: DateTime<Utc>,
) -> Vec<RoleReviewClassification> {
    let mut classifications = Vec::new();
    if RoleAssignment::validate_window(
        assignment.valid_from,
        assignment.valid_until,
        assignment.risk_tier,
    )
    .is_err()
    {
        classifications.push(RoleReviewClassification::Excessive);
    }
    if assignment.parent_assignment_id.is_some()
        && !parent.is_some_and(|parent| assignment.effective_at(at, Some(parent)))
    {
        classifications.push(RoleReviewClassification::Orphaned);
    }
    if conflicts {
        classifications.push(RoleReviewClassification::Conflicting);
    }
    if assignment.valid_until <= at {
        classifications.push(RoleReviewClassification::Expired);
    } else if assignment.valid_until <= expiry_horizon_until {
        classifications.push(RoleReviewClassification::ExpiringSoon);
    }
    if assignment.valid_from > at {
        classifications.push(RoleReviewClassification::Inactive);
    }
    classifications.sort_by_key(|classification| classification.sort_key());
    classifications.dedup();
    classifications
}

/// Build a deterministic review queue. A queue contains only assignments
/// needing review or notification; active assignments outside the horizon are
/// omitted. The horizon is inclusive at its upper boundary and assignment
/// windows remain half-open at `evaluated_at`.
pub fn build_role_review_queue(
    assignments: &[RoleAssignment],
    evaluated_at: DateTime<Utc>,
    expiry_horizon: Duration,
) -> Result<RoleReviewQueue, RoleReviewError> {
    if expiry_horizon <= Duration::zero() {
        return Err(RoleReviewError::InvalidHorizon);
    }
    let expiry_horizon_until = evaluated_at
        .checked_add_signed(expiry_horizon)
        .ok_or(RoleReviewError::InvalidHorizon)?;
    let mut ordered = assignments.to_vec();
    ordered.sort_by_key(|assignment| assignment.id);
    let mut items = Vec::new();
    for assignment in &ordered {
        let parent = assignment
            .parent_assignment_id
            .and_then(|parent_id| ordered.iter().find(|candidate| candidate.id == parent_id));
        let conflicts = ordered.iter().any(|candidate| {
            candidate.id != assignment.id && roles_conflict(assignment, candidate)
        });
        let classifications = review_classifications(
            assignment,
            parent,
            conflicts,
            evaluated_at,
            expiry_horizon_until,
        );
        if classifications.is_empty() {
            continue;
        }
        let notification = if classifications.contains(&RoleReviewClassification::Expired) {
            Some(RoleExpiryNotification {
                assignment_id: assignment.id,
                principal_ref: assignment.principal_ref.clone(),
                valid_until: assignment.valid_until,
                kind: RoleExpiryNotificationKind::Expired,
            })
        } else if classifications.contains(&RoleReviewClassification::ExpiringSoon) {
            Some(RoleExpiryNotification {
                assignment_id: assignment.id,
                principal_ref: assignment.principal_ref.clone(),
                valid_until: assignment.valid_until,
                kind: RoleExpiryNotificationKind::ExpiringSoon,
            })
        } else {
            None
        };
        items.push(RoleReviewItem {
            assignment_id: assignment.id,
            principal_ref: assignment.principal_ref.clone(),
            valid_from: assignment.valid_from,
            valid_until: assignment.valid_until,
            parent_assignment_id: assignment.parent_assignment_id,
            classifications,
            notification,
            assignment: assignment.clone(),
        });
    }
    items.sort_by_key(|item| item.assignment_id);
    Ok(RoleReviewQueue {
        evaluated_at,
        expiry_horizon_until,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(days: i64) -> (DateTime<Utc>, DateTime<Utc>) {
        let from = Utc::now();
        (from, from + Duration::days(days))
    }

    #[test]
    fn role_windows_use_risk_tier_defaults() {
        let (from, until) = window(91);
        assert_eq!(
            RoleAssignment::validate_window(from, until, RiskTier::Administrator),
            Err(AssignmentError::ExceedsMaximum {
                maximum: Duration::days(90)
            })
        );
        let (from, until) = window(30);
        assert!(RoleAssignment::validate_window(from, until, RiskTier::Contractor).is_ok());
    }

    #[test]
    fn child_scope_cannot_escape_company_or_parent_window() {
        let holding = Uuid::new_v4();
        let company = Uuid::new_v4();
        let branch = Uuid::new_v4();
        let (from, until) = window(30);
        let parent = RoleAssignment {
            id: Uuid::new_v4(),
            principal_ref: "owner".into(),
            role_key: "holding_owner".into(),
            permissions: vec![],
            functional_domain: "organization".into(),
            authority_level: 1,
            approval_limit_minor: 0,
            scope: AuthorityScope::Company {
                holding_id: holding,
                company_id: company,
            },
            valid_from: from,
            valid_until: until,
            timezone: "Asia/Bangkok".into(),
            risk_tier: RiskTier::Administrator,
            grant_reason: "test".into(),
            ticket_reference: None,
            grantor: "root".into(),
            approver: Some("root".into()),
            renewal_reviewer: None,
            parent_assignment_id: None,
            assignment_version: 1,
        };
        let child = RoleAssignment {
            scope: AuthorityScope::Branch {
                holding_id: holding,
                company_id: company,
                branch_id: branch,
            },
            valid_until: until,
            parent_assignment_id: Some(parent.id),
            ..parent.clone()
        };
        assert!(child.validate_delegation(&parent).is_ok());
        let outside = RoleAssignment {
            scope: AuthorityScope::Holding {
                holding_id: holding,
            },
            ..child
        };
        assert_eq!(
            outside.validate_delegation(&parent),
            Err(AssignmentError::ScopeExceedsParent)
        );
    }

    #[test]
    fn subtree_delegation_is_exact_and_never_aliases_a_branch_uuid() {
        let holding = Uuid::new_v4();
        let company = Uuid::new_v4();
        let selected_branch = Uuid::new_v4();
        let other_branch = Uuid::new_v4();
        let parent = AuthorityScope::Subtree {
            holding_id: holding,
            organization_node_id: selected_branch,
        };
        assert!(!parent.contains(&AuthorityScope::Branch {
            holding_id: holding,
            company_id: company,
            branch_id: selected_branch,
        }));
        assert!(!parent.contains(&AuthorityScope::Branch {
            holding_id: holding,
            company_id: company,
            branch_id: other_branch,
        }));
        assert!(!parent.contains(&AuthorityScope::Company {
            holding_id: holding,
            company_id: company,
        }));
        assert!(parent.contains(&AuthorityScope::Subtree {
            holding_id: holding,
            organization_node_id: selected_branch,
        }));
        assert!(!parent.contains(&AuthorityScope::Subtree {
            holding_id: holding,
            organization_node_id: other_branch,
        }));
    }

    #[test]
    fn renewal_creates_a_new_immutable_version() {
        let (from, until) = window(30);
        let original = RoleAssignment {
            id: Uuid::new_v4(),
            principal_ref: "u".into(),
            role_key: "role".into(),
            permissions: vec![],
            functional_domain: "finance".into(),
            authority_level: 5,
            approval_limit_minor: 0,
            scope: AuthorityScope::Platform,
            valid_from: from,
            valid_until: until,
            timezone: "UTC".into(),
            risk_tier: RiskTier::Contractor,
            grant_reason: "contract".into(),
            ticket_reference: None,
            grantor: "g".into(),
            approver: None,
            renewal_reviewer: None,
            parent_assignment_id: None,
            assignment_version: 1,
        };
        let renewed = original
            .renew(from, until, "new-grantor", None)
            .expect("valid renewal");
        assert_ne!(original.id, renewed.id);
        assert_eq!(original.valid_from, from);
        assert_eq!(original.valid_until, until);
        assert_eq!(original.grantor, "g");
        assert_eq!(original.approver, None);
        assert_eq!(original.assignment_version, 1);
        assert_eq!(renewed.assignment_version, 2);
    }

    #[test]
    fn assignment_windows_are_finite_and_half_open() {
        let from = Utc::now();
        assert_eq!(
            RoleAssignment::validate_window(from, from, RiskTier::Standard),
            Err(AssignmentError::InvalidWindow)
        );
        assert_eq!(
            RoleAssignment::validate_window(from, DateTime::<Utc>::MAX_UTC, RiskTier::Standard),
            Err(AssignmentError::PermanentAssignment)
        );
        assert!(from < from + Duration::seconds(1));
    }

    #[test]
    fn delegation_contains_duration_authority_and_permissions() {
        let (from, until) = window(30);
        let parent = RoleAssignment {
            id: Uuid::new_v4(),
            principal_ref: "owner".into(),
            role_key: "finance_owner".into(),
            permissions: vec!["finance.read".into()],
            functional_domain: "finance".into(),
            authority_level: 2,
            approval_limit_minor: 0,
            scope: AuthorityScope::Platform,
            valid_from: from,
            valid_until: until,
            timezone: "UTC".into(),
            risk_tier: RiskTier::Standard,
            grant_reason: "delegation".into(),
            ticket_reference: None,
            grantor: "root".into(),
            approver: None,
            renewal_reviewer: None,
            parent_assignment_id: None,
            assignment_version: 1,
        };
        let mut child = RoleAssignment {
            id: Uuid::new_v4(),
            principal_ref: "delegate".into(),
            role_key: "finance_reader".into(),
            permissions: vec!["finance.read".into()],
            functional_domain: "finance".into(),
            authority_level: 2,
            approval_limit_minor: 0,
            scope: AuthorityScope::Platform,
            valid_from: from,
            valid_until: until,
            timezone: "UTC".into(),
            risk_tier: RiskTier::Standard,
            grant_reason: "delegation".into(),
            ticket_reference: None,
            grantor: "owner".into(),
            approver: None,
            renewal_reviewer: None,
            parent_assignment_id: Some(parent.id),
            assignment_version: 1,
        };
        assert!(child.validate_for_grant(Some(&parent)).is_ok());

        child.permissions.push("finance.post".into());
        assert_eq!(
            child.validate_for_grant(Some(&parent)),
            Err(AssignmentError::PermissionExceedsParent {
                permission: "finance.post".into()
            })
        );
        child.permissions.pop();
        child.authority_level = parent.authority_level + 1;
        assert_eq!(
            child.validate_for_grant(Some(&parent)),
            Err(AssignmentError::AuthorityExceedsParent)
        );
        child.authority_level = parent.authority_level;
        child.valid_until = until + Duration::seconds(1);
        assert_eq!(
            child.validate_for_grant(Some(&parent)),
            Err(AssignmentError::ChildOutlivesParent)
        );
    }

    #[test]
    fn high_risk_changes_require_two_distinct_people() {
        let (from, until) = window(1);
        let mut assignment = RoleAssignment {
            id: Uuid::new_v4(),
            principal_ref: "administrator".into(),
            role_key: "platform_admin".into(),
            permissions: vec!["platform.manage".into()],
            functional_domain: "platform".into(),
            authority_level: 10,
            approval_limit_minor: 0,
            scope: AuthorityScope::Platform,
            valid_from: from,
            valid_until: until,
            timezone: "UTC".into(),
            risk_tier: RiskTier::High,
            grant_reason: "break-glass review".into(),
            ticket_reference: None,
            grantor: "alice".into(),
            approver: None,
            renewal_reviewer: None,
            parent_assignment_id: None,
            assignment_version: 1,
        };
        assert_eq!(
            assignment.validate_for_grant(None),
            Err(AssignmentError::MissingApprover)
        );
        assignment.approver = Some("alice".into());
        assert_eq!(
            assignment.validate_for_grant(None),
            Err(AssignmentError::ApproverMustDifferFromGrantor)
        );
        assignment.approver = Some("bob".into());
        assert!(assignment.validate_for_grant(None).is_ok());
        assert_eq!(
            assignment.renew(from, until, "alice", None),
            Err(AssignmentError::MissingApprover)
        );
        let renewed = assignment
            .renew(from, until, "alice", Some("bob".into()))
            .expect("distinct approver permits renewal");
        assert_eq!(renewed.assignment_version, 2);
    }

    fn evaluated_assignment(
        id: Uuid,
        scope: AuthorityScope,
        from: DateTime<Utc>,
        until: DateTime<Utc>,
        parent_assignment_id: Option<Uuid>,
    ) -> RoleAssignment {
        RoleAssignment {
            id,
            principal_ref: "user".into(),
            role_key: "finance_operator".into(),
            permissions: vec!["finance.post".into()],
            functional_domain: "finance".into(),
            authority_level: 3,
            approval_limit_minor: 1_000,
            scope,
            valid_from: from,
            valid_until: until,
            timezone: "Asia/Bangkok".into(),
            risk_tier: RiskTier::Standard,
            grant_reason: "access review".into(),
            ticket_reference: None,
            grantor: "owner".into(),
            approver: None,
            renewal_reviewer: None,
            parent_assignment_id,
            assignment_version: 1,
        }
    }

    fn access_request(at: DateTime<Utc>, scope: AuthorityScope) -> EffectiveAccessRequest {
        EffectiveAccessRequest {
            principal_ref: "user".into(),
            at,
            requested_scope: scope,
            module_key: "finance".into(),
            required_permission: "finance.post".into(),
            requested_approval_minor: 500,
            approval_limit_minor: 1_000,
            identity_active: true,
            session_active: true,
            billing_state: BillingState::Active,
            entitlement: Some(ModuleEntitlement {
                module_key: "finance".into(),
                active: true,
                seats: 5,
                company_limit: 2,
                branch_limit: 4,
                active_until: None,
            }),
            entitlement_book: None,
            is_mutation: true,
            segregation_of_duties_passed: true,
            support_capability_required: false,
            support_capability_active: false,
        }
    }

    #[test]
    fn effective_access_uses_exact_half_open_boundaries() {
        let from = Utc::now();
        let until = from + Duration::hours(1);
        let scope = AuthorityScope::Platform;
        let assignment = evaluated_assignment(Uuid::new_v4(), scope.clone(), from, until, None);
        assert!(
            evaluate_effective_access(&assignment, None, &access_request(from, scope.clone()))
                .is_ok()
        );
        assert_eq!(
            evaluate_effective_access(&assignment, None, &access_request(until, scope)),
            Err(EffectiveAccessDenial::AssignmentInactive)
        );
    }

    #[test]
    fn effective_access_cascades_parent_and_checks_scope_and_support() {
        let holding = Uuid::new_v4();
        let company = Uuid::new_v4();
        let branch = Uuid::new_v4();
        let from = Utc::now();
        let parent_until = from + Duration::hours(1);
        let child_until = from + Duration::hours(2);
        let parent = evaluated_assignment(
            Uuid::new_v4(),
            AuthorityScope::Company {
                holding_id: holding,
                company_id: company,
            },
            from,
            parent_until,
            None,
        );
        let child = evaluated_assignment(
            Uuid::new_v4(),
            AuthorityScope::Branch {
                holding_id: holding,
                company_id: company,
                branch_id: branch,
            },
            from,
            child_until,
            Some(parent.id),
        );
        let mut request = access_request(from, child.scope.clone());
        let mut book = EntitlementAllocationBook::new(
            "finance",
            crate::commercial::EntitlementCapacity {
                seats: 1,
                companies: 1,
                branches: 1,
            },
            None,
        );
        book.set_active(true, None);
        book.allocate_company(company, 1, 1)
            .expect("company allocation");
        book.activate_company(company).expect("company activation");
        book.activate_branch(company, branch)
            .expect("branch activation");
        request.entitlement_book = Some(book);
        assert!(evaluate_effective_access(&child, Some(&parent), &request).is_ok());
        request.support_capability_required = true;
        assert_eq!(
            evaluate_effective_access(&child, Some(&parent), &request),
            Err(EffectiveAccessDenial::SupportCapabilityInactive)
        );
        assert_eq!(
            evaluate_effective_access(
                &child,
                Some(&parent),
                &access_request(parent_until, child.scope.clone())
            ),
            Err(EffectiveAccessDenial::ParentInactive)
        );
    }

    #[test]
    fn effective_access_checks_entitlement_billing_and_approval_limit() {
        let from = Utc::now();
        let until = from + Duration::hours(1);
        let scope = AuthorityScope::Platform;
        let assignment = evaluated_assignment(Uuid::new_v4(), scope.clone(), from, until, None);
        let mut request = access_request(from, scope);
        request.requested_approval_minor = 1_001;
        assert_eq!(
            evaluate_effective_access(&assignment, None, &request),
            Err(EffectiveAccessDenial::ApprovalLimitExceeded)
        );
        request.requested_approval_minor = 500;
        request.billing_state = BillingState::ReadOnly;
        assert_eq!(
            evaluate_effective_access(&assignment, None, &request),
            Err(EffectiveAccessDenial::CommercialDenied(
                "payment_failure_read_only".into()
            ))
        );
    }

    #[test]
    fn effective_access_binds_principal_and_company_branch_activation() {
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let branch_id = Uuid::new_v4();
        let from = Utc::now();
        let until = from + Duration::hours(1);
        let company_scope = AuthorityScope::Company {
            holding_id,
            company_id,
        };
        let branch_scope = AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        };
        let assignment =
            evaluated_assignment(Uuid::new_v4(), company_scope.clone(), from, until, None);
        let mut book = EntitlementAllocationBook::new(
            "finance",
            crate::commercial::EntitlementCapacity {
                seats: 10,
                companies: 1,
                branches: 1,
            },
            None,
        );
        book.set_active(true, None);
        book.allocate_company(company_id, 1, 1)
            .expect("company allocation");
        book.activate_company(company_id)
            .expect("company activation");

        let mut company_request = access_request(from, company_scope);
        company_request.entitlement_book = Some(book.clone());
        assert!(evaluate_effective_access(&assignment, None, &company_request).is_ok());

        let mut branch_request = access_request(from, branch_scope.clone());
        branch_request.entitlement_book = Some(book.clone());
        assert_eq!(
            evaluate_effective_access(&assignment, None, &branch_request),
            Err(EffectiveAccessDenial::CommercialDenied(
                "branch_not_activated".into()
            ))
        );
        book.activate_branch(company_id, branch_id)
            .expect("branch activation");
        branch_request.entitlement_book = Some(book);
        assert!(evaluate_effective_access(&assignment, None, &branch_request).is_ok());

        let mut wrong_principal = company_request;
        wrong_principal.principal_ref = "different-user".into();
        assert_eq!(
            evaluate_effective_access(&assignment, None, &wrong_principal),
            Err(EffectiveAccessDenial::PrincipalMismatch)
        );
    }

    #[test]
    fn effective_access_chain_rechecks_every_ancestor() {
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let branch_id = Uuid::new_v4();
        let from = Utc::now();
        let grandparent_until = from + Duration::hours(1);
        let parent_until = from + Duration::hours(2);
        let child_until = from + Duration::hours(3);
        let grandparent = evaluated_assignment(
            Uuid::new_v4(),
            AuthorityScope::Holding { holding_id },
            from,
            grandparent_until,
            None,
        );
        let parent = evaluated_assignment(
            Uuid::new_v4(),
            AuthorityScope::Company {
                holding_id,
                company_id,
            },
            from,
            parent_until,
            Some(grandparent.id),
        );
        let child = evaluated_assignment(
            Uuid::new_v4(),
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            },
            from,
            child_until,
            Some(parent.id),
        );
        let mut book = EntitlementAllocationBook::new(
            "finance",
            crate::commercial::EntitlementCapacity {
                seats: 1,
                companies: 1,
                branches: 1,
            },
            None,
        );
        book.set_active(true, None);
        book.allocate_company(company_id, 1, 1)
            .expect("company allocation");
        book.activate_company(company_id)
            .expect("company activation");
        book.activate_branch(company_id, branch_id)
            .expect("branch activation");
        let mut request = access_request(from, child.scope.clone());
        request.entitlement_book = Some(book.clone());
        assert!(
            evaluate_effective_access_chain(
                &child,
                &[parent.clone(), grandparent.clone()],
                &request
            )
            .is_ok()
        );
        assert_eq!(
            evaluate_effective_access_chain(&child, std::slice::from_ref(&parent), &request),
            Err(EffectiveAccessDenial::ParentInactive)
        );
        request.at = grandparent_until;
        assert_eq!(
            evaluate_effective_access_chain(&child, &[parent.clone(), grandparent], &request),
            Err(EffectiveAccessDenial::ParentInactive)
        );
    }

    #[test]
    fn company_access_fails_closed_without_authoritative_allocation_book() {
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let from = Utc::now();
        let scope = AuthorityScope::Company {
            holding_id,
            company_id,
        };
        let assignment = evaluated_assignment(
            Uuid::new_v4(),
            scope.clone(),
            from,
            from + Duration::hours(1),
            None,
        );
        assert_eq!(
            evaluate_effective_access(&assignment, None, &access_request(from, scope)),
            Err(EffectiveAccessDenial::CommercialDenied(
                "allocation_context_missing".into()
            ))
        );
    }

    #[test]
    fn canonical_scope_keys_are_stable_and_delegated_roles_fail_closed_without_parent() {
        let holding_id = Uuid::from_u128(1);
        let branch_a = Uuid::from_u128(4);
        let first = AuthorityScope::Subtree {
            holding_id,
            organization_node_id: branch_a,
        };
        let second = AuthorityScope::Subtree {
            holding_id,
            organization_node_id: branch_a,
        };
        assert_eq!(first.canonical_key(), second.canonical_key());

        let from = DateTime::from_timestamp(1_700_000_000, 0).expect("fixed timestamp");
        let until = from + Duration::hours(1);
        let parent = evaluated_assignment(
            Uuid::from_u128(10),
            AuthorityScope::Platform,
            from,
            until,
            None,
        );
        let child = evaluated_assignment(
            Uuid::from_u128(11),
            AuthorityScope::Platform,
            from,
            until,
            Some(parent.id),
        );
        assert!(!child.effective_at(from, None));
        assert!(child.effective_at(from, Some(&parent)));
        assert!(!child.effective_at(until, Some(&parent)));
    }

    #[allow(clippy::too_many_arguments)]
    fn review_assignment(
        id: u128,
        principal_ref: &str,
        role_key: &str,
        permissions: &[&str],
        from: DateTime<Utc>,
        until: DateTime<Utc>,
        parent_assignment_id: Option<Uuid>,
        scope: AuthorityScope,
    ) -> RoleAssignment {
        RoleAssignment {
            id: Uuid::from_u128(id),
            principal_ref: principal_ref.into(),
            role_key: role_key.into(),
            permissions: permissions.iter().map(|value| (*value).into()).collect(),
            functional_domain: "finance".into(),
            authority_level: 3,
            approval_limit_minor: 0,
            scope,
            valid_from: from,
            valid_until: until,
            timezone: "UTC".into(),
            risk_tier: RiskTier::Standard,
            grant_reason: "review fixture".into(),
            ticket_reference: None,
            grantor: "owner".into(),
            approver: None,
            renewal_reviewer: None,
            parent_assignment_id,
            assignment_version: 1,
        }
    }

    #[test]
    fn review_queue_classifies_expiry_boundary_and_inactive_roles() {
        let at = DateTime::from_timestamp(1_700_000_000, 0).expect("fixed timestamp");
        let horizon = Duration::days(7);
        let expired = review_assignment(
            1,
            "expired-user",
            "finance_reader",
            &["finance.read"],
            at - Duration::days(2),
            at,
            None,
            AuthorityScope::Platform,
        );
        let expiring = review_assignment(
            2,
            "expiring-user",
            "finance_reader",
            &["finance.read"],
            at,
            at + horizon,
            None,
            AuthorityScope::Platform,
        );
        let outside_horizon = review_assignment(
            3,
            "stable-user",
            "finance_reader",
            &["finance.read"],
            at,
            at + horizon + Duration::seconds(1),
            None,
            AuthorityScope::Platform,
        );
        let inactive = review_assignment(
            4,
            "future-user",
            "finance_reader",
            &["finance.read"],
            at + Duration::seconds(1),
            at + Duration::days(2),
            None,
            AuthorityScope::Platform,
        );

        let queue = build_role_review_queue(
            &[
                inactive.clone(),
                outside_horizon,
                expiring.clone(),
                expired.clone(),
            ],
            at,
            horizon,
        )
        .expect("review queue");
        assert_eq!(
            queue
                .items
                .iter()
                .map(|item| item.assignment_id)
                .collect::<Vec<_>>(),
            vec![expired.id, expiring.id, inactive.id]
        );
        assert!(
            queue.items[0]
                .classifications
                .contains(&RoleReviewClassification::Expired)
        );
        assert_eq!(
            queue.items[0].notification.as_ref().map(|value| value.kind),
            Some(RoleExpiryNotificationKind::Expired)
        );
        assert!(
            queue.items[1]
                .classifications
                .contains(&RoleReviewClassification::ExpiringSoon)
        );
        assert_eq!(
            queue.items[1].notification.as_ref().map(|value| value.kind),
            Some(RoleExpiryNotificationKind::ExpiringSoon)
        );
        assert!(
            queue.items[2]
                .classifications
                .contains(&RoleReviewClassification::Inactive)
        );
    }

    #[test]
    fn review_queue_classifies_orphaned_excessive_and_conflicting_roles() {
        let at = DateTime::from_timestamp(1_700_000_000, 0).expect("fixed timestamp");
        let orphaned = review_assignment(
            10,
            "orphan",
            "finance_reader",
            &["finance.read"],
            at,
            at + Duration::days(1),
            Some(Uuid::from_u128(999)),
            AuthorityScope::Platform,
        );
        let excessive = review_assignment(
            11,
            "excessive",
            "finance_reader",
            &["finance.read"],
            at,
            at + Duration::days(366),
            None,
            AuthorityScope::Platform,
        );
        let maker = review_assignment(
            12,
            "same-user",
            "finance_maker",
            &["finance.write"],
            at,
            at + Duration::days(1),
            None,
            AuthorityScope::Platform,
        );
        let checker = review_assignment(
            13,
            "same-user",
            "finance_checker",
            &["finance.approve"],
            at,
            at + Duration::days(1),
            None,
            AuthorityScope::Platform,
        );

        let queue = build_role_review_queue(
            &[checker, excessive.clone(), orphaned.clone(), maker],
            at,
            Duration::hours(1),
        )
        .expect("review queue");
        let item = |id| queue.items.iter().find(|item| item.assignment_id == id);
        assert!(
            item(orphaned.id)
                .expect("orphan item")
                .classifications
                .contains(&RoleReviewClassification::Orphaned)
        );
        assert!(
            item(excessive.id)
                .expect("excessive item")
                .classifications
                .contains(&RoleReviewClassification::Excessive)
        );
        assert_eq!(
            queue
                .items
                .iter()
                .filter(|item| item
                    .classifications
                    .contains(&RoleReviewClassification::Conflicting))
                .count(),
            2
        );
    }

    fn recovery_request(
        request_id: u128,
        target: RoleRecoveryTarget,
        at: DateTime<Utc>,
        valid_until: DateTime<Utc>,
    ) -> RoleRecoveryRequest {
        RoleRecoveryRequest {
            request_id: Uuid::from_u128(request_id),
            target,
            principal_ref: "recovered-principal".into(),
            permissions: vec!["organization.admin".into(), "organization.read".into()],
            functional_domain: "organization".into(),
            authority_level: 100,
            valid_from: at,
            valid_until,
            timezone: "UTC".into(),
            grant_reason: "final authority recovery".into(),
            ticket_reference: Some("recovery-ticket".into()),
            requester_ref: "requester".into(),
            approver_ref: "approver".into(),
            reviewer_ref: None,
        }
    }

    #[test]
    fn final_root_recovery_requires_two_people_and_is_finite() {
        let at = DateTime::from_timestamp(1_700_000_000, 0).expect("fixed timestamp");
        let request = recovery_request(
            20,
            RoleRecoveryTarget::PlatformRoot,
            at,
            at + Duration::days(30),
        );
        let recovered = request.materialize(&[], at).expect("final root recovery");
        assert_eq!(recovered.role_key, PLATFORM_ROOT_ROLE);
        assert_eq!(recovered.scope, AuthorityScope::Platform);
        assert_eq!(recovered.risk_tier, RiskTier::PlatformRoot);
        assert_eq!(recovered.assignment_version, 1);
        assert!(recovered.valid_until < DateTime::<Utc>::MAX_UTC);

        let mut self_approved = request.clone();
        self_approved.approver_ref = self_approved.requester_ref.clone();
        assert_eq!(
            self_approved.materialize(&[], at),
            Err(RoleReviewError::RecoveryApproverMustDiffer)
        );
        let mut permanent = request;
        permanent.valid_until = DateTime::<Utc>::MAX_UTC;
        assert_eq!(
            permanent.materialize(&[], at),
            Err(RoleReviewError::Assignment(
                AssignmentError::PermanentAssignment
            ))
        );

        let mut reviewer_conflict = recovery_request(
            21,
            RoleRecoveryTarget::PlatformRoot,
            at,
            at + Duration::days(30),
        );
        reviewer_conflict.reviewer_ref = Some("approver".into());
        assert_eq!(
            reviewer_conflict.materialize(&[], at),
            Err(RoleReviewError::RecoveryReviewerMustDifferFromApprover)
        );
    }

    #[test]
    fn final_owner_recovery_requires_no_active_owner_and_matching_scope() {
        let at = DateTime::from_timestamp(1_700_000_000, 0).expect("fixed timestamp");
        let holding_id = Uuid::from_u128(30);
        let request = recovery_request(
            31,
            RoleRecoveryTarget::HoldingOwner { holding_id },
            at,
            at + Duration::days(30),
        );
        let active_owner = RoleAssignment {
            id: Uuid::from_u128(32),
            principal_ref: "existing-owner".into(),
            role_key: HOLDING_OWNER_ROLE.into(),
            permissions: vec!["organization.admin".into()],
            functional_domain: "organization".into(),
            authority_level: 100,
            approval_limit_minor: 0,
            scope: AuthorityScope::Holding { holding_id },
            valid_from: at,
            valid_until: at + Duration::days(1),
            timezone: "UTC".into(),
            risk_tier: RiskTier::Administrator,
            grant_reason: "existing owner".into(),
            ticket_reference: None,
            grantor: "grantor".into(),
            approver: Some("approver".into()),
            renewal_reviewer: None,
            parent_assignment_id: None,
            assignment_version: 1,
        };
        assert_eq!(
            request.materialize(std::slice::from_ref(&active_owner), at),
            Err(RoleReviewError::RecoveryTargetAlreadyActive)
        );
        let recovered = request
            .materialize(
                &[RoleAssignment {
                    valid_until: at,
                    ..active_owner
                }],
                at,
            )
            .expect("expired owner permits final recovery");
        assert_eq!(recovered.scope, AuthorityScope::Holding { holding_id });
        assert_eq!(recovered.role_key, HOLDING_OWNER_ROLE);
    }

    #[test]
    fn recovery_and_review_serialization_are_canonical() {
        let at = DateTime::from_timestamp(1_700_000_000, 0).expect("fixed timestamp");
        let mut first = recovery_request(
            40,
            RoleRecoveryTarget::PlatformRoot,
            at,
            at + Duration::days(1),
        );
        let mut second = first.clone();
        first.permissions = vec!["z.permission".into(), "a.permission".into()];
        second.permissions = vec!["a.permission".into(), "z.permission".into()];
        assert_eq!(
            first.canonical_bytes().expect("canonical recovery"),
            second.canonical_bytes().expect("canonical recovery")
        );

        let assignment = review_assignment(
            41,
            "stable",
            "finance_reader",
            &["finance.read"],
            at,
            at + Duration::days(1),
            None,
            AuthorityScope::Platform,
        );
        let left =
            build_role_review_queue(std::slice::from_ref(&assignment), at, Duration::days(2))
                .expect("left queue");
        let right =
            build_role_review_queue(&[assignment], at, Duration::days(2)).expect("right queue");
        assert_eq!(
            left.canonical_bytes().expect("canonical queue"),
            right.canonical_bytes().expect("canonical queue")
        );
        assert_eq!(
            left.notifications()[0].stable_key(),
            right.notifications()[0].stable_key()
        );

        let base = review_assignment(
            42,
            "reviewed",
            "finance_reader",
            &["finance.read"],
            at,
            at + Duration::days(1),
            None,
            AuthorityScope::Platform,
        );
        assert_eq!(
            base.renew_with_reviewer(
                at,
                at + Duration::days(1),
                "grantor",
                Some("approver".into()),
                "approver",
            ),
            Err(AssignmentError::RenewalReviewerMustDifferFromApprover)
        );
        assert_eq!(
            base.renew_with_reviewer(
                at,
                at + Duration::days(1),
                "grantor",
                Some("approver".into()),
                "grantor",
            ),
            Err(AssignmentError::RenewalReviewerMustDifferFromGrantor)
        );
    }

    #[test]
    fn approver_scope_intersection_requires_identity_scope_permission_and_authority() {
        let at = DateTime::from_timestamp(1_700_000_000, 0).expect("fixed timestamp");
        let until = at + Duration::days(1);
        let holding_id = Uuid::from_u128(50);
        let company_id = Uuid::from_u128(51);
        let target_scope = AuthorityScope::Company {
            holding_id,
            company_id,
        };
        let mut assignment = review_assignment(
            52,
            "target-principal",
            "finance_operator",
            &["finance.post"],
            at,
            until,
            None,
            target_scope,
        );
        assignment.approver = Some("approver-principal".into());
        assignment.authority_level = 5;

        let mut approver = review_assignment(
            53,
            "approver-principal",
            "security_approver",
            &["role.assignment.approve"],
            at,
            until,
            None,
            AuthorityScope::Holding { holding_id },
        );
        approver.authority_level = 5;
        assert!(
            evaluate_approver_scope_intersection(&assignment, &approver, &[], at).is_ok(),
            "a same-or-higher authority approver with containing scope is allowed"
        );

        let mut wrong_scope = approver.clone();
        wrong_scope.scope = AuthorityScope::Company {
            holding_id,
            company_id: Uuid::from_u128(54),
        };
        assert_eq!(
            evaluate_approver_scope_intersection(&assignment, &wrong_scope, &[], at),
            Err(ApproverScopeDenial::ApproverScopeDenied)
        );

        let mut missing_permission = approver.clone();
        missing_permission.permissions = vec!["role.assignment.review".into()];
        assert_eq!(
            evaluate_approver_scope_intersection(&assignment, &missing_permission, &[], at),
            Err(ApproverScopeDenial::ApproverPermissionDenied)
        );

        let mut insufficient_authority = approver;
        insufficient_authority.authority_level = 4;
        assert_eq!(
            evaluate_approver_scope_intersection(&assignment, &insufficient_authority, &[], at),
            Err(ApproverScopeDenial::ApproverAuthorityDenied)
        );
    }

    #[test]
    fn approver_scope_intersection_rechecks_the_complete_parent_chain() {
        let at = DateTime::from_timestamp(1_700_000_000, 0).expect("fixed timestamp");
        let until = at + Duration::days(1);
        let holding_id = Uuid::from_u128(60);
        let company_id = Uuid::from_u128(61);
        let mut assignment = review_assignment(
            62,
            "target-principal",
            "finance_operator",
            &["finance.post"],
            at,
            until,
            None,
            AuthorityScope::Company {
                holding_id,
                company_id,
            },
        );
        assignment.approver = Some("delegated-approver".into());
        assignment.authority_level = 4;

        let mut parent = review_assignment(
            63,
            "approver-owner",
            "security_owner",
            &["role.assignment.approve"],
            at,
            until,
            None,
            AuthorityScope::Holding { holding_id },
        );
        parent.authority_level = 4;
        let mut approver = review_assignment(
            64,
            "delegated-approver",
            "security_approver",
            &["role.assignment.approve"],
            at,
            until,
            Some(parent.id),
            AuthorityScope::Company {
                holding_id,
                company_id,
            },
        );
        approver.authority_level = 4;
        assert!(
            evaluate_approver_scope_intersection(
                &assignment,
                &approver,
                std::slice::from_ref(&parent),
                at,
            )
            .is_ok()
        );

        let expired_parent = RoleAssignment {
            valid_until: at,
            ..parent.clone()
        };
        assert_eq!(
            evaluate_approver_scope_intersection(
                &assignment,
                &approver,
                std::slice::from_ref(&expired_parent),
                at,
            ),
            Err(ApproverScopeDenial::ApproverParentInactive)
        );

        let wrong_parent = RoleAssignment {
            id: Uuid::from_u128(65),
            ..parent
        };
        assert_eq!(
            evaluate_approver_scope_intersection(
                &assignment,
                &approver,
                std::slice::from_ref(&wrong_parent),
                at,
            ),
            Err(ApproverScopeDenial::ApproverParentInactive)
        );
    }

    #[test]
    fn approver_scope_intersection_fails_closed_for_missing_or_inactive_approvers() {
        let at = DateTime::from_timestamp(1_700_000_000, 0).expect("fixed timestamp");
        let until = at + Duration::days(1);
        let mut assignment = review_assignment(
            70,
            "target-principal",
            "finance_operator",
            &["finance.post"],
            at,
            until,
            None,
            AuthorityScope::Platform,
        );
        let approver = review_assignment(
            71,
            "approver-principal",
            "security_approver",
            &["role.assignment.approve"],
            at,
            until,
            None,
            AuthorityScope::Platform,
        );
        assert_eq!(
            evaluate_approver_scope_intersection(&assignment, &approver, &[], at),
            Err(ApproverScopeDenial::ApproverRequired)
        );

        assignment.approver = Some("different-principal".into());
        assert_eq!(
            evaluate_approver_scope_intersection(&assignment, &approver, &[], at),
            Err(ApproverScopeDenial::ApproverIdentityMismatch)
        );

        assignment.approver = Some(approver.principal_ref.clone());
        let inactive = RoleAssignment {
            valid_from: at + Duration::seconds(1),
            ..approver
        };
        assert_eq!(
            evaluate_approver_scope_intersection(&assignment, &inactive, &[], at),
            Err(ApproverScopeDenial::ApproverInactive)
        );
    }
}
