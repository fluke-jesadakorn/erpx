use chrono::{DateTime, Utc};
use erpx_integration_contracts::{
    StableRecordRef,
    catalog::ContextId,
    reference::{RecordTypeId, ReferenceValidationError},
};
use erpx_kernel::{AuthorityScope, RecordId};
use serde::{Deserialize, Serialize};

use super::{CustomerAuthorityDomainError, CustomerId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomerAuthorityRecordKind {
    AuthorityGrant,
    SupportAccess,
}

impl CustomerAuthorityRecordKind {
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::AuthorityGrant => "customer.authority-grant",
            Self::SupportAccess => "customer.support-access",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomerAuthorityRecordStatus {
    Proposed,
    Active,
    Revoked,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerAuthorityRecord {
    id: RecordId,
    version: u64,
    kind: CustomerAuthorityRecordKind,
    customer_id: CustomerId,
    customer_account: StableRecordRef,
    support_case: Option<StableRecordRef>,
    scope: AuthorityScope,
    evidence_sha256: String,
    status: CustomerAuthorityRecordStatus,
    valid_until: DateTime<Utc>,
    created_by: String,
    revoked_by: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl CustomerAuthorityRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn propose(
        id: RecordId,
        kind: CustomerAuthorityRecordKind,
        customer_id: CustomerId,
        customer_account: StableRecordRef,
        support_case: Option<StableRecordRef>,
        scope: AuthorityScope,
        evidence_sha256: String,
        valid_until: DateTime<Utc>,
        created_by: String,
        at: DateTime<Utc>,
    ) -> Result<Self, CustomerAuthorityDomainError> {
        let value = Self {
            id,
            version: 1,
            kind,
            customer_id,
            customer_account,
            support_case,
            scope,
            evidence_sha256,
            status: CustomerAuthorityRecordStatus::Proposed,
            valid_until,
            created_by,
            revoked_by: None,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn activate(&mut self, at: DateTime<Utc>) -> Result<(), CustomerAuthorityDomainError> {
        if self.status != CustomerAuthorityRecordStatus::Proposed || at >= self.valid_until {
            return Err(CustomerAuthorityDomainError::InvalidAuthorityRecord);
        }
        self.advance(at)?;
        self.status = CustomerAuthorityRecordStatus::Active;
        Ok(())
    }

    pub fn revoke(
        &mut self,
        actor: String,
        at: DateTime<Utc>,
    ) -> Result<(), CustomerAuthorityDomainError> {
        if !matches!(
            self.status,
            CustomerAuthorityRecordStatus::Proposed | CustomerAuthorityRecordStatus::Active
        ) || actor.trim().is_empty()
        {
            return Err(CustomerAuthorityDomainError::InvalidAuthorityRecord);
        }
        self.advance(at)?;
        self.status = CustomerAuthorityRecordStatus::Revoked;
        self.revoked_by = Some(actor);
        Ok(())
    }

    pub fn expire(&mut self, at: DateTime<Utc>) -> Result<(), CustomerAuthorityDomainError> {
        if self.status != CustomerAuthorityRecordStatus::Active || at < self.valid_until {
            return Err(CustomerAuthorityDomainError::InvalidAuthorityRecord);
        }
        self.advance(at)?;
        self.status = CustomerAuthorityRecordStatus::Expired;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), CustomerAuthorityDomainError> {
        self.customer_account
            .validate()
            .map_err(|_| CustomerAuthorityDomainError::InvalidAuthorityRecord)?;
        if self.customer_account.owner_context() != ContextId::CustomerAuthority
            || self.customer_account.record_type().as_str() != "customer.account"
            || self.scope.validate().is_err()
            || !matches!(
                self.scope,
                AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
            )
            || self.version == 0
            || self.created_by.trim().is_empty()
            || self.valid_until <= self.created_at
            || self.updated_at < self.created_at
            || !valid_digest(&self.evidence_sha256)
        {
            return Err(CustomerAuthorityDomainError::InvalidAuthorityRecord);
        }
        match (self.kind, &self.support_case) {
            (CustomerAuthorityRecordKind::AuthorityGrant, None) => {}
            (CustomerAuthorityRecordKind::SupportAccess, Some(record))
                if record.owner_context() == ContextId::Support
                    && record.record_type().as_str() == "support.customer-case" => {}
            _ => return Err(CustomerAuthorityDomainError::InvalidAuthorityRecord),
        }
        if self.status == CustomerAuthorityRecordStatus::Revoked && self.revoked_by.is_none() {
            return Err(CustomerAuthorityDomainError::InvalidAuthorityRecord);
        }
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), CustomerAuthorityDomainError> {
        if at < self.updated_at {
            return Err(CustomerAuthorityDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(CustomerAuthorityDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub fn record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        StableRecordRef::try_new(
            self.id,
            ContextId::CustomerAuthority,
            RecordTypeId::try_new(self.kind.record_type())?,
        )
    }

    pub const fn id(&self) -> RecordId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn kind(&self) -> CustomerAuthorityRecordKind {
        self.kind
    }
    pub const fn customer_id(&self) -> CustomerId {
        self.customer_id
    }
    pub const fn customer_account(&self) -> &StableRecordRef {
        &self.customer_account
    }
    pub const fn support_case(&self) -> Option<&StableRecordRef> {
        self.support_case.as_ref()
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn evidence_sha256(&self) -> &str {
        &self.evidence_sha256
    }
    pub const fn status(&self) -> CustomerAuthorityRecordStatus {
        self.status
    }
    pub const fn valid_until(&self) -> DateTime<Utc> {
        self.valid_until
    }
    pub fn created_by(&self) -> &str {
        &self.created_by
    }
    pub fn revoked_by(&self) -> Option<&str> {
        self.revoked_by.as_deref()
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

fn valid_digest(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};
    use uuid::Uuid;

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 16, 9, 0, 0).single().unwrap()
    }
    fn scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }
    fn reference(owner: ContextId, record_type: &str, _id: u128) -> StableRecordRef {
        StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            owner,
            RecordTypeId::try_new(record_type).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn authority_records_have_distinct_identity_and_closed_lifecycle() {
        let mut record = CustomerAuthorityRecord::propose(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            CustomerAuthorityRecordKind::AuthorityGrant,
            CustomerId::try_from_uuid(Uuid::from_u128(11)).unwrap(),
            reference(ContextId::CustomerAuthority, "customer.account", 12),
            None,
            scope(),
            format!("sha256:{}", "a".repeat(64)),
            at() + Duration::hours(1),
            "maker".into(),
            at(),
        )
        .unwrap();
        assert_eq!(
            record.record_ref().unwrap().record_type().as_str(),
            "customer.authority-grant"
        );
        record.activate(at() + Duration::minutes(1)).unwrap();
        record
            .revoke("checker".into(), at() + Duration::minutes(2))
            .unwrap();
        assert_eq!(record.version(), 3);
        assert!(record.activate(at() + Duration::minutes(3)).is_err());
    }

    #[test]
    fn support_access_requires_exact_support_case_handoff() {
        let result = CustomerAuthorityRecord::propose(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            CustomerAuthorityRecordKind::SupportAccess,
            CustomerId::try_from_uuid(Uuid::from_u128(21)).unwrap(),
            reference(ContextId::CustomerAuthority, "customer.account", 22),
            None,
            scope(),
            format!("sha256:{}", "b".repeat(64)),
            at() + Duration::hours(1),
            "maker".into(),
            at(),
        );
        assert_eq!(
            result,
            Err(CustomerAuthorityDomainError::InvalidAuthorityRecord)
        );
    }
}
