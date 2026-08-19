use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use erpx_integration_contracts::{RecordVersionRef, catalog::ContextId};
use erpx_kernel::{AuthorityScope, PrincipalId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::CustomerAuthorityDomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CustomerPortalBindingId(Uuid);

impl CustomerPortalBindingId {
    pub fn try_from_uuid(value: Uuid) -> Result<Self, CustomerAuthorityDomainError> {
        if value.is_nil() || value.get_version_num() != 4 {
            return Err(CustomerAuthorityDomainError::NilIdentifier {
                field: "customer_portal_binding_id",
            });
        }
        Ok(Self(value))
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortalPrincipalKind {
    Human,
    Service,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortalUseCase {
    FieldServiceRecordAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomerPortalBindingStatus {
    Proposed,
    Active,
    Rejected,
    Revoked,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedIdentityEvidence {
    digest: String,
    version: u64,
    verified_at: DateTime<Utc>,
}

impl VerifiedIdentityEvidence {
    pub fn try_new(
        digest: impl Into<String>,
        version: u64,
        verified_at: DateTime<Utc>,
    ) -> Result<Self, CustomerAuthorityDomainError> {
        let value = Self {
            digest: digest.into(),
            version,
            verified_at,
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), CustomerAuthorityDomainError> {
        if self.version == 0
            || self.digest.len() != 64
            || !self.digest.bytes().all(|value| value.is_ascii_hexdigit())
        {
            return Err(CustomerAuthorityDomainError::InvalidPortalBinding);
        }
        Ok(())
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub const fn version(&self) -> u64 {
        self.version
    }

    pub const fn verified_at(&self) -> DateTime<Utc> {
        self.verified_at
    }
}

/// Customer Authority owns this exact, governed link. It is authorization
/// evidence only and is never a discoverable directory record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerPortalPrincipalBinding {
    id: CustomerPortalBindingId,
    version: u64,
    customer: RecordVersionRef,
    portal_principal_id: PrincipalId,
    principal_kind: PortalPrincipalKind,
    scope: AuthorityScope,
    verified_identity: VerifiedIdentityEvidence,
    permitted_use_cases: BTreeSet<PortalUseCase>,
    valid_from: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    proposed_by: PrincipalId,
    approved_by: Option<PrincipalId>,
    rejected_by: Option<PrincipalId>,
    revoked_by: Option<PrincipalId>,
    ended_at: Option<DateTime<Utc>>,
    decision_evidence_digest: Option<String>,
    status: CustomerPortalBindingStatus,
    changed_at: DateTime<Utc>,
}

impl CustomerPortalPrincipalBinding {
    #[allow(clippy::too_many_arguments)]
    pub fn propose(
        id: CustomerPortalBindingId,
        customer: RecordVersionRef,
        portal_principal_id: PrincipalId,
        principal_kind: PortalPrincipalKind,
        scope: AuthorityScope,
        verified_identity: VerifiedIdentityEvidence,
        permitted_use_cases: BTreeSet<PortalUseCase>,
        valid_from: DateTime<Utc>,
        valid_until: DateTime<Utc>,
        proposed_by: PrincipalId,
        at: DateTime<Utc>,
    ) -> Result<Self, CustomerAuthorityDomainError> {
        let value = Self {
            id,
            version: 1,
            customer,
            portal_principal_id,
            principal_kind,
            scope,
            verified_identity,
            permitted_use_cases,
            valid_from,
            valid_until,
            proposed_by,
            approved_by: None,
            rejected_by: None,
            revoked_by: None,
            ended_at: None,
            decision_evidence_digest: None,
            status: CustomerPortalBindingStatus::Proposed,
            changed_at: at,
        };
        value.validate_rehydrated()?;
        Ok(value)
    }

    pub fn approve(
        &mut self,
        checker: PrincipalId,
        decision_evidence_digest: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<(), CustomerAuthorityDomainError> {
        if self.status != CustomerPortalBindingStatus::Proposed
            || checker == self.proposed_by
            || at >= self.valid_until
        {
            return Err(CustomerAuthorityDomainError::InvalidPortalBindingTransition);
        }
        self.approved_by = Some(checker);
        self.decision_evidence_digest = Some(decision_evidence_digest.into());
        self.status = CustomerPortalBindingStatus::Active;
        self.advance(at)
    }

    pub fn reject(
        &mut self,
        checker: PrincipalId,
        decision_evidence_digest: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<(), CustomerAuthorityDomainError> {
        if self.status != CustomerPortalBindingStatus::Proposed || checker == self.proposed_by {
            return Err(CustomerAuthorityDomainError::InvalidPortalBindingTransition);
        }
        self.rejected_by = Some(checker);
        self.ended_at = Some(at);
        self.decision_evidence_digest = Some(decision_evidence_digest.into());
        self.status = CustomerPortalBindingStatus::Rejected;
        self.advance(at)
    }

    pub fn revoke(
        &mut self,
        actor: PrincipalId,
        decision_evidence_digest: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<(), CustomerAuthorityDomainError> {
        if self.status != CustomerPortalBindingStatus::Active {
            return Err(CustomerAuthorityDomainError::InvalidPortalBindingTransition);
        }
        self.revoked_by = Some(actor);
        self.ended_at = Some(at);
        self.decision_evidence_digest = Some(decision_evidence_digest.into());
        self.status = CustomerPortalBindingStatus::Revoked;
        self.advance(at)
    }

    pub fn expire(&mut self, at: DateTime<Utc>) -> Result<(), CustomerAuthorityDomainError> {
        if !matches!(
            self.status,
            CustomerPortalBindingStatus::Proposed | CustomerPortalBindingStatus::Active
        ) || at < self.valid_until
        {
            return Err(CustomerAuthorityDomainError::InvalidPortalBindingTransition);
        }
        self.ended_at = Some(at);
        self.status = CustomerPortalBindingStatus::Expired;
        self.advance(at)
    }

    pub fn is_effective_at(&self, at: DateTime<Utc>) -> bool {
        self.status == CustomerPortalBindingStatus::Active
            && self.valid_from <= at
            && at < self.valid_until
            && self.ended_at.is_none()
    }

    pub fn validate_rehydrated(&self) -> Result<(), CustomerAuthorityDomainError> {
        self.customer
            .validate()
            .map_err(|_| CustomerAuthorityDomainError::InvalidPortalBinding)?;
        self.scope
            .validate()
            .map_err(|_| CustomerAuthorityDomainError::InvalidPortalBinding)?;
        self.verified_identity.validate()?;
        let record = self.customer.record();
        if self.version == 0
            || record.owner_context() != ContextId::CustomerAuthority
            || record.record_type().as_str() != "customer.account"
            || self.permitted_use_cases.is_empty()
            || self.valid_until <= self.valid_from
            || self.changed_at < self.verified_identity.verified_at
            || self
                .decision_evidence_digest
                .as_ref()
                .is_some_and(|digest| !valid_digest(digest))
        {
            return Err(CustomerAuthorityDomainError::InvalidPortalBinding);
        }
        match self.status {
            CustomerPortalBindingStatus::Proposed => {
                if self.approved_by.is_some()
                    || self.rejected_by.is_some()
                    || self.revoked_by.is_some()
                    || self.ended_at.is_some()
                {
                    return Err(CustomerAuthorityDomainError::InvalidPortalBinding);
                }
            }
            CustomerPortalBindingStatus::Active => {
                if self.approved_by.is_none()
                    || self.approved_by == Some(self.proposed_by)
                    || self.rejected_by.is_some()
                    || self.revoked_by.is_some()
                    || self.ended_at.is_some()
                    || self.decision_evidence_digest.is_none()
                {
                    return Err(CustomerAuthorityDomainError::InvalidPortalBinding);
                }
            }
            CustomerPortalBindingStatus::Rejected => {
                if self.rejected_by.is_none()
                    || self.rejected_by == Some(self.proposed_by)
                    || self.ended_at.is_none()
                    || self.decision_evidence_digest.is_none()
                {
                    return Err(CustomerAuthorityDomainError::InvalidPortalBinding);
                }
            }
            CustomerPortalBindingStatus::Revoked => {
                if self.approved_by.is_none()
                    || self.revoked_by.is_none()
                    || self.ended_at.is_none()
                    || self.decision_evidence_digest.is_none()
                {
                    return Err(CustomerAuthorityDomainError::InvalidPortalBinding);
                }
            }
            CustomerPortalBindingStatus::Expired => {
                if self.ended_at.is_none() {
                    return Err(CustomerAuthorityDomainError::InvalidPortalBinding);
                }
            }
        }
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), CustomerAuthorityDomainError> {
        if at < self.changed_at {
            return Err(CustomerAuthorityDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(CustomerAuthorityDomainError::VersionOverflow)?;
        self.changed_at = at;
        self.validate_rehydrated()
    }

    pub const fn id(&self) -> CustomerPortalBindingId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn customer(&self) -> &RecordVersionRef {
        &self.customer
    }
    pub const fn portal_principal_id(&self) -> PrincipalId {
        self.portal_principal_id
    }
    pub const fn principal_kind(&self) -> PortalPrincipalKind {
        self.principal_kind
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn verified_identity(&self) -> &VerifiedIdentityEvidence {
        &self.verified_identity
    }
    pub const fn permitted_use_cases(&self) -> &BTreeSet<PortalUseCase> {
        &self.permitted_use_cases
    }
    pub const fn valid_from(&self) -> DateTime<Utc> {
        self.valid_from
    }
    pub const fn valid_until(&self) -> DateTime<Utc> {
        self.valid_until
    }
    pub const fn proposed_by(&self) -> PrincipalId {
        self.proposed_by
    }
    pub const fn approved_by(&self) -> Option<PrincipalId> {
        self.approved_by
    }
    pub const fn rejected_by(&self) -> Option<PrincipalId> {
        self.rejected_by
    }
    pub const fn revoked_by(&self) -> Option<PrincipalId> {
        self.revoked_by
    }
    pub const fn ended_at(&self) -> Option<DateTime<Utc>> {
        self.ended_at
    }
    pub fn decision_evidence_digest(&self) -> Option<&str> {
        self.decision_evidence_digest.as_deref()
    }
    pub const fn status(&self) -> CustomerPortalBindingStatus {
        self.status
    }
    pub const fn changed_at(&self) -> DateTime<Utc> {
        self.changed_at
    }
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use erpx_integration_contracts::{RecordTypeId, StableRecordRef};
    use erpx_kernel::RecordId;

    use super::*;

    fn principal() -> PrincipalId {
        PrincipalId::try_from_uuid(Uuid::new_v4()).unwrap()
    }

    fn customer() -> RecordVersionRef {
        RecordVersionRef::try_new(
            StableRecordRef::try_new(
                RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
                ContextId::CustomerAuthority,
                RecordTypeId::try_new("customer.account").unwrap(),
            )
            .unwrap(),
            7,
        )
        .unwrap()
    }

    fn proposed() -> CustomerPortalPrincipalBinding {
        let now = Utc::now();
        CustomerPortalPrincipalBinding::propose(
            CustomerPortalBindingId::try_from_uuid(Uuid::new_v4()).unwrap(),
            customer(),
            principal(),
            PortalPrincipalKind::Human,
            AuthorityScope::Company {
                holding_id: Uuid::new_v4(),
                company_id: Uuid::new_v4(),
            },
            VerifiedIdentityEvidence::try_new("a".repeat(64), 2, now).unwrap(),
            BTreeSet::from([PortalUseCase::FieldServiceRecordAccess]),
            now,
            now + Duration::days(30),
            principal(),
            now,
        )
        .unwrap()
    }

    #[test]
    fn maker_cannot_approve_and_revoke_is_terminal() {
        let mut value = proposed();
        assert_eq!(
            value.approve(value.proposed_by(), "b".repeat(64), Utc::now()),
            Err(CustomerAuthorityDomainError::InvalidPortalBindingTransition)
        );
        value
            .approve(principal(), "b".repeat(64), Utc::now())
            .unwrap();
        assert!(value.is_effective_at(Utc::now()));
        value
            .revoke(principal(), "c".repeat(64), Utc::now())
            .unwrap();
        assert!(!value.is_effective_at(Utc::now()));
        assert_eq!(
            value.revoke(principal(), "d".repeat(64), Utc::now()),
            Err(CustomerAuthorityDomainError::InvalidPortalBindingTransition)
        );
    }

    #[test]
    fn customer_reference_must_be_exact_owner_and_version() {
        let mut wrong = proposed();
        wrong.customer = RecordVersionRef::try_new(
            StableRecordRef::try_new(
                RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
                ContextId::Support,
                RecordTypeId::try_new("support.customer-case").unwrap(),
            )
            .unwrap(),
            1,
        )
        .unwrap();
        assert_eq!(
            wrong.validate_rehydrated(),
            Err(CustomerAuthorityDomainError::InvalidPortalBinding)
        );
    }
}
