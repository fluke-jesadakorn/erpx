use chrono::{DateTime, Duration, Utc};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    IdentityAccessDomainError, PrincipalId, RoleAssignmentId, next_version, validate_scope,
    validate_text,
};

pub const FRESH_MFA_WINDOW: Duration = Duration::minutes(10);
pub const MAX_BREAK_GLASS_DURATION: Duration = Duration::hours(1);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    #[default]
    Human,
    Service,
    Agent,
    HumanGroup,
    AgentPool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalRealm {
    #[default]
    InternalWorkforce,
    PlatformWorkforce,
    CustomerPortal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonHumanPrincipalGovernance {
    owner_ref: String,
    purpose: String,
    workload_binding: String,
    credential_reference: String,
}

impl NonHumanPrincipalGovernance {
    pub fn new(
        owner_ref: String,
        purpose: String,
        workload_binding: String,
        credential_reference: String,
    ) -> Result<Self, IdentityAccessDomainError> {
        validate_text(&owner_ref, "owner_ref")?;
        validate_text(&purpose, "purpose")?;
        validate_text(&workload_binding, "workload_binding")?;
        validate_text(&credential_reference, "credential_reference")?;
        Ok(Self {
            owner_ref,
            purpose,
            workload_binding,
            credential_reference,
        })
    }

    pub fn owner_ref(&self) -> &str {
        &self.owner_ref
    }

    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    pub fn workload_binding(&self) -> &str {
        &self.workload_binding
    }

    pub fn credential_reference(&self) -> &str {
        &self.credential_reference
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighRiskAuthority<'a> {
    pub requester_ref: &'a str,
    pub approver_ref: &'a str,
    pub requester_authenticated_at: DateTime<Utc>,
    pub requester_mfa_at: DateTime<Utc>,
    pub approver_authenticated_at: DateTime<Utc>,
    pub approver_mfa_at: DateTime<Utc>,
    pub evaluated_at: DateTime<Utc>,
}

impl HighRiskAuthority<'_> {
    pub fn validate(&self) -> Result<(), IdentityAccessDomainError> {
        validate_text(self.requester_ref, "requester_ref")?;
        validate_text(self.approver_ref, "approver_ref")?;
        if self.requester_ref == self.approver_ref {
            return Err(IdentityAccessDomainError::IndependentApproverRequired);
        }
        for timestamp in [
            self.requester_authenticated_at,
            self.requester_mfa_at,
            self.approver_authenticated_at,
            self.approver_mfa_at,
        ] {
            if timestamp > self.evaluated_at || self.evaluated_at - timestamp > FRESH_MFA_WINDOW {
                return Err(IdentityAccessDomainError::FreshMfaRequired);
            }
        }
        if self.requester_mfa_at < self.requester_authenticated_at
            || self.approver_mfa_at < self.approver_authenticated_at
        {
            return Err(IdentityAccessDomainError::FreshMfaRequired);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakGlassStatus {
    Requested,
    Active,
    Closed,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreakGlassSession {
    id: Uuid,
    version: u64,
    principal_id: PrincipalId,
    scope: AuthorityScope,
    reason: String,
    requester_ref: String,
    approver_ref: Option<String>,
    requested_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    activated_at: Option<DateTime<Utc>>,
    closed_at: Option<DateTime<Utc>>,
    after_reviewed_at: Option<DateTime<Utc>>,
    status: BreakGlassStatus,
}

impl BreakGlassSession {
    pub fn request(
        id: Uuid,
        principal_id: PrincipalId,
        scope: AuthorityScope,
        reason: String,
        requester_ref: String,
        requested_at: DateTime<Utc>,
        valid_until: DateTime<Utc>,
    ) -> Result<Self, IdentityAccessDomainError> {
        if id.is_nil() {
            return Err(IdentityAccessDomainError::NilIdentifier {
                field: "break_glass_session_id",
            });
        }
        validate_scope(&scope)?;
        validate_text(&reason, "break_glass_reason")?;
        validate_text(&requester_ref, "requester_ref")?;
        if valid_until <= requested_at || valid_until - requested_at > MAX_BREAK_GLASS_DURATION {
            return Err(IdentityAccessDomainError::InvalidBreakGlassWindow);
        }
        Ok(Self {
            id,
            version: 1,
            principal_id,
            scope,
            reason,
            requester_ref,
            approver_ref: None,
            requested_at,
            valid_until,
            activated_at: None,
            closed_at: None,
            after_reviewed_at: None,
            status: BreakGlassStatus::Requested,
        })
    }

    pub fn activate(
        &mut self,
        expected_version: u64,
        authority: &HighRiskAuthority<'_>,
        monitoring_available: bool,
    ) -> Result<(), IdentityAccessDomainError> {
        self.expect_version(expected_version)?;
        if self.status != BreakGlassStatus::Requested
            || authority.requester_ref != self.requester_ref
        {
            return Err(IdentityAccessDomainError::InvalidBreakGlassTransition);
        }
        authority.validate()?;
        if !monitoring_available {
            return Err(IdentityAccessDomainError::BreakGlassMonitoringRequired);
        }
        if authority.evaluated_at >= self.valid_until {
            return Err(IdentityAccessDomainError::InvalidBreakGlassWindow);
        }
        self.version = next_version(self.version)?;
        self.approver_ref = Some(authority.approver_ref.to_owned());
        self.activated_at = Some(authority.evaluated_at);
        self.status = BreakGlassStatus::Active;
        Ok(())
    }

    pub fn terminate(
        &mut self,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<(), IdentityAccessDomainError> {
        self.expect_version(expected_version)?;
        if self.status != BreakGlassStatus::Active
            || at
                < self
                    .activated_at
                    .expect("active sessions have activation time")
        {
            return Err(IdentityAccessDomainError::InvalidBreakGlassTransition);
        }
        self.version = next_version(self.version)?;
        self.closed_at = Some(at);
        self.status = BreakGlassStatus::Closed;
        Ok(())
    }

    pub fn expire(
        &mut self,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<(), IdentityAccessDomainError> {
        self.expect_version(expected_version)?;
        if !matches!(
            self.status,
            BreakGlassStatus::Requested | BreakGlassStatus::Active
        ) || at < self.valid_until
        {
            return Err(IdentityAccessDomainError::InvalidBreakGlassTransition);
        }
        self.version = next_version(self.version)?;
        self.closed_at = Some(at);
        self.status = BreakGlassStatus::Expired;
        Ok(())
    }

    pub fn record_after_review(
        &mut self,
        expected_version: u64,
        reviewer_ref: &str,
        at: DateTime<Utc>,
    ) -> Result<(), IdentityAccessDomainError> {
        self.expect_version(expected_version)?;
        validate_text(reviewer_ref, "reviewer_ref")?;
        if !matches!(
            self.status,
            BreakGlassStatus::Closed | BreakGlassStatus::Expired
        ) || reviewer_ref == self.requester_ref
            || self.approver_ref.as_deref() == Some(reviewer_ref)
            || self.closed_at.is_some_and(|closed_at| at < closed_at)
        {
            return Err(IdentityAccessDomainError::IndependentAfterReviewRequired);
        }
        if self.after_reviewed_at.is_some() {
            return Ok(());
        }
        self.version = next_version(self.version)?;
        self.after_reviewed_at = Some(at);
        Ok(())
    }

    pub fn is_effective_at(&self, at: DateTime<Utc>) -> bool {
        self.status == BreakGlassStatus::Active
            && self.activated_at.is_some_and(|activated| activated <= at)
            && at < self.valid_until
    }

    pub fn validate_rehydrated(&self) -> Result<(), IdentityAccessDomainError> {
        if self.id.is_nil() || self.version == 0 {
            return Err(IdentityAccessDomainError::NilIdentifier {
                field: "break_glass_session_id",
            });
        }
        validate_scope(&self.scope)?;
        validate_text(&self.reason, "break_glass_reason")?;
        validate_text(&self.requester_ref, "requester_ref")?;
        if self.valid_until <= self.requested_at
            || self.valid_until - self.requested_at > MAX_BREAK_GLASS_DURATION
        {
            return Err(IdentityAccessDomainError::InvalidBreakGlassWindow);
        }
        let coherent = match self.status {
            BreakGlassStatus::Requested => {
                self.approver_ref.is_none()
                    && self.activated_at.is_none()
                    && self.closed_at.is_none()
                    && self.after_reviewed_at.is_none()
            }
            BreakGlassStatus::Active => {
                self.approver_ref.is_some()
                    && self.activated_at.is_some()
                    && self.closed_at.is_none()
                    && self.after_reviewed_at.is_none()
            }
            BreakGlassStatus::Closed | BreakGlassStatus::Expired => {
                self.closed_at.is_some()
                    && self
                        .after_reviewed_at
                        .is_none_or(|reviewed| reviewed >= self.closed_at.expect("checked"))
            }
        };
        if !coherent {
            return Err(IdentityAccessDomainError::InvalidBreakGlassTransition);
        }
        Ok(())
    }

    fn expect_version(&self, expected: u64) -> Result<(), IdentityAccessDomainError> {
        if self.version != expected {
            return Err(IdentityAccessDomainError::AuthorityVersionConflict {
                expected,
                actual: self.version,
            });
        }
        Ok(())
    }

    pub const fn id(&self) -> Uuid {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn principal_id(&self) -> PrincipalId {
        self.principal_id
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
    pub const fn requested_at(&self) -> DateTime<Utc> {
        self.requested_at
    }
    pub const fn valid_until(&self) -> DateTime<Utc> {
        self.valid_until
    }
    pub const fn status(&self) -> BreakGlassStatus {
        self.status
    }
    pub fn requester_ref(&self) -> &str {
        &self.requester_ref
    }
    pub fn approver_ref(&self) -> Option<&str> {
        self.approver_ref.as_deref()
    }
    pub const fn activated_at(&self) -> Option<DateTime<Utc>> {
        self.activated_at
    }
    pub const fn closed_at(&self) -> Option<DateTime<Utc>> {
        self.closed_at
    }
    pub const fn after_reviewed_at(&self) -> Option<DateTime<Utc>> {
        self.after_reviewed_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CertificationDecision {
    Retain,
    Revoke,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertificationItem {
    assignment_id: RoleAssignmentId,
    principal_id: PrincipalId,
    principal_kind: PrincipalKind,
    owner_ref: String,
    grantor_ref: String,
    decision: Option<CertificationDecision>,
    decided_at: Option<DateTime<Utc>>,
}

impl CertificationItem {
    pub fn new(
        assignment_id: RoleAssignmentId,
        principal_id: PrincipalId,
        principal_kind: PrincipalKind,
        owner_ref: String,
        grantor_ref: String,
    ) -> Result<Self, IdentityAccessDomainError> {
        validate_text(&owner_ref, "owner_ref")?;
        validate_text(&grantor_ref, "grantor_ref")?;
        Ok(Self {
            assignment_id,
            principal_id,
            principal_kind,
            owner_ref,
            grantor_ref,
            decision: None,
            decided_at: None,
        })
    }
    pub const fn assignment_id(&self) -> RoleAssignmentId {
        self.assignment_id
    }
    pub const fn principal_id(&self) -> PrincipalId {
        self.principal_id
    }
    pub const fn principal_kind(&self) -> PrincipalKind {
        self.principal_kind
    }
    pub fn owner_ref(&self) -> &str {
        &self.owner_ref
    }
    pub fn grantor_ref(&self) -> &str {
        &self.grantor_ref
    }
    pub const fn decision(&self) -> Option<CertificationDecision> {
        self.decision
    }
    pub const fn decided_at(&self) -> Option<DateTime<Utc>> {
        self.decided_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessCertification {
    id: Uuid,
    version: u64,
    scope: AuthorityScope,
    reviewer_ref: String,
    due_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    items: Vec<CertificationItem>,
}

impl AccessCertification {
    pub fn start(
        id: Uuid,
        scope: AuthorityScope,
        reviewer_ref: String,
        due_at: DateTime<Utc>,
        items: Vec<CertificationItem>,
    ) -> Result<Self, IdentityAccessDomainError> {
        if id.is_nil() {
            return Err(IdentityAccessDomainError::NilIdentifier {
                field: "certification_id",
            });
        }
        validate_scope(&scope)?;
        validate_text(&reviewer_ref, "reviewer_ref")?;
        if items.is_empty() {
            return Err(IdentityAccessDomainError::EmptyCertification);
        }
        let mut ids = items
            .iter()
            .map(|item| item.assignment_id)
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        if ids.len() != items.len() {
            return Err(IdentityAccessDomainError::DuplicateCertificationItem);
        }
        Ok(Self {
            id,
            version: 1,
            scope,
            reviewer_ref,
            due_at,
            completed_at: None,
            items,
        })
    }

    pub fn decide(
        &mut self,
        expected_version: u64,
        assignment_id: RoleAssignmentId,
        decision: CertificationDecision,
        at: DateTime<Utc>,
    ) -> Result<(), IdentityAccessDomainError> {
        self.expect_version(expected_version)?;
        if self.completed_at.is_some() || at > self.due_at {
            return Err(IdentityAccessDomainError::CertificationClosed);
        }
        let item = self
            .items
            .iter_mut()
            .find(|item| item.assignment_id == assignment_id)
            .ok_or(IdentityAccessDomainError::AssignmentNotFound)?;
        if self.reviewer_ref == item.owner_ref || self.reviewer_ref == item.grantor_ref {
            return Err(IdentityAccessDomainError::ConflictedCertificationReviewer);
        }
        if item.decision == Some(decision) {
            return Ok(());
        }
        if item.decision.is_some() {
            return Err(IdentityAccessDomainError::CertificationDecisionFinal);
        }
        item.decision = Some(decision);
        item.decided_at = Some(at);
        self.version = next_version(self.version)?;
        if self.items.iter().all(|item| item.decision.is_some()) {
            self.completed_at = Some(at);
        }
        Ok(())
    }

    fn expect_version(&self, expected: u64) -> Result<(), IdentityAccessDomainError> {
        if self.version != expected {
            return Err(IdentityAccessDomainError::AuthorityVersionConflict {
                expected,
                actual: self.version,
            });
        }
        Ok(())
    }

    pub fn validate_rehydrated(&self) -> Result<(), IdentityAccessDomainError> {
        if self.id.is_nil() || self.version == 0 {
            return Err(IdentityAccessDomainError::NilIdentifier {
                field: "certification_id",
            });
        }
        validate_scope(&self.scope)?;
        validate_text(&self.reviewer_ref, "reviewer_ref")?;
        if self.items.is_empty() {
            return Err(IdentityAccessDomainError::EmptyCertification);
        }
        let mut ids = self
            .items
            .iter()
            .map(|item| item.assignment_id)
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        if ids.len() != self.items.len() {
            return Err(IdentityAccessDomainError::DuplicateCertificationItem);
        }
        for item in &self.items {
            validate_text(&item.owner_ref, "owner_ref")?;
            validate_text(&item.grantor_ref, "grantor_ref")?;
            if item.decision.is_some() != item.decided_at.is_some() {
                return Err(IdentityAccessDomainError::CertificationDecisionFinal);
            }
        }
        if self.completed_at.is_some() != self.items.iter().all(|item| item.decision.is_some()) {
            return Err(IdentityAccessDomainError::CertificationClosed);
        }
        Ok(())
    }

    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn id(&self) -> Uuid {
        self.id
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn reviewer_ref(&self) -> &str {
        &self.reviewer_ref
    }
    pub const fn due_at(&self) -> DateTime<Utc> {
        self.due_at
    }
    pub const fn completed_at(&self) -> Option<DateTime<Utc>> {
        self.completed_at
    }
    pub fn items(&self) -> &[CertificationItem] {
        &self.items
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }
    fn at(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 15, 9, minute, 0)
            .single()
            .unwrap()
    }
    fn scope() -> AuthorityScope {
        AuthorityScope::Holding { holding_id: id(1) }
    }

    #[test]
    fn high_risk_authority_requires_independent_fresh_mfa() {
        let stale = HighRiskAuthority {
            requester_ref: "maker",
            approver_ref: "checker",
            requester_authenticated_at: at(0),
            requester_mfa_at: at(0),
            approver_authenticated_at: at(19),
            approver_mfa_at: at(19),
            evaluated_at: at(20),
        };
        assert_eq!(
            stale.validate(),
            Err(IdentityAccessDomainError::FreshMfaRequired)
        );
        let self_approved = HighRiskAuthority {
            requester_ref: "maker",
            approver_ref: "maker",
            requester_authenticated_at: at(19),
            requester_mfa_at: at(19),
            ..stale
        };
        assert_eq!(
            self_approved.validate(),
            Err(IdentityAccessDomainError::IndependentApproverRequired)
        );
    }

    #[test]
    fn break_glass_is_bounded_versioned_revocable_and_reviewed() {
        let principal_id = PrincipalId::try_from_uuid(id(2)).unwrap();
        let mut session = BreakGlassSession::request(
            id(3),
            principal_id,
            scope(),
            "restore holding access".into(),
            "maker".into(),
            at(10),
            at(40),
        )
        .unwrap();
        let authority = HighRiskAuthority {
            requester_ref: "maker",
            approver_ref: "checker",
            requester_authenticated_at: at(11),
            requester_mfa_at: at(11),
            approver_authenticated_at: at(11),
            approver_mfa_at: at(11),
            evaluated_at: at(12),
        };
        session.activate(1, &authority, true).unwrap();
        assert!(session.is_effective_at(at(20)));
        assert_eq!(
            session.terminate(1, at(21)),
            Err(IdentityAccessDomainError::AuthorityVersionConflict {
                expected: 1,
                actual: 2
            })
        );
        session.terminate(2, at(21)).unwrap();
        assert!(!session.is_effective_at(at(22)));
        assert_eq!(
            session.record_after_review(3, "checker", at(23)),
            Err(IdentityAccessDomainError::IndependentAfterReviewRequired)
        );
        session.record_after_review(3, "auditor", at(23)).unwrap();
        session.record_after_review(4, "auditor", at(24)).unwrap();
        assert_eq!(session.version(), 4);
    }

    #[test]
    fn certification_covers_non_human_access_and_is_idempotent() {
        let agent_item = CertificationItem::new(
            RoleAssignmentId::try_from_uuid(id(4)).unwrap(),
            PrincipalId::try_from_uuid(id(5)).unwrap(),
            PrincipalKind::Agent,
            "agent-owner".into(),
            "grantor".into(),
        )
        .unwrap();
        let service_item = CertificationItem::new(
            RoleAssignmentId::try_from_uuid(id(7)).unwrap(),
            PrincipalId::try_from_uuid(id(8)).unwrap(),
            PrincipalKind::Service,
            "service-owner".into(),
            "grantor".into(),
        )
        .unwrap();
        let mut certification = AccessCertification::start(
            id(6),
            scope(),
            "reviewer".into(),
            at(50),
            vec![agent_item, service_item],
        )
        .unwrap();
        certification
            .decide(
                1,
                RoleAssignmentId::try_from_uuid(id(4)).unwrap(),
                CertificationDecision::Revoke,
                at(20),
            )
            .unwrap();
        assert_eq!(certification.version(), 2);
        assert!(certification.completed_at().is_none());
        certification
            .decide(
                2,
                RoleAssignmentId::try_from_uuid(id(4)).unwrap(),
                CertificationDecision::Revoke,
                at(21),
            )
            .unwrap();
        assert_eq!(certification.version(), 2);
        certification
            .decide(
                2,
                RoleAssignmentId::try_from_uuid(id(7)).unwrap(),
                CertificationDecision::Retain,
                at(22),
            )
            .unwrap();
        assert!(certification.completed_at().is_some());
        assert_eq!(
            certification.decide(
                3,
                RoleAssignmentId::try_from_uuid(id(4)).unwrap(),
                CertificationDecision::Revoke,
                at(23)
            ),
            Err(IdentityAccessDomainError::CertificationClosed)
        );
    }
}
