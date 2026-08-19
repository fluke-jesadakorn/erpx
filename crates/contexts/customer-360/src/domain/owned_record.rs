use chrono::{DateTime, Utc};
use erpx_integration_contracts::{
    StableRecordRef,
    catalog::ContextId,
    reference::{RecordTypeId, ReferenceValidationError},
};
use erpx_kernel::{AuthorityScope, RecordId};
use serde::{Deserialize, Serialize};

use super::Customer360DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Customer360RecordKind {
    Profile,
    Note,
    Merge,
}

impl Customer360RecordKind {
    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Profile => "customer-360.profile",
            Self::Note => "customer-360.note",
            Self::Merge => "customer-360.merge",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Customer360RecordStatus {
    Draft,
    Active,
    Closed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Customer360OwnedRecord {
    id: RecordId,
    version: u64,
    kind: Customer360RecordKind,
    scope: AuthorityScope,
    customer_account: StableRecordRef,
    related_customer_account: Option<StableRecordRef>,
    source_version: u64,
    title: String,
    content_object_ref: Option<String>,
    content_sha256: String,
    status: Customer360RecordStatus,
    created_by: String,
    closed_by: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Customer360OwnedRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: RecordId,
        kind: Customer360RecordKind,
        scope: AuthorityScope,
        customer_account: StableRecordRef,
        related_customer_account: Option<StableRecordRef>,
        source_version: u64,
        title: String,
        content_object_ref: Option<String>,
        content_sha256: String,
        created_by: String,
        at: DateTime<Utc>,
    ) -> Result<Self, Customer360DomainError> {
        let value = Self {
            id,
            version: 1,
            kind,
            scope,
            customer_account,
            related_customer_account,
            source_version,
            title,
            content_object_ref,
            content_sha256,
            status: Customer360RecordStatus::Draft,
            created_by,
            closed_by: None,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn activate(&mut self, at: DateTime<Utc>) -> Result<(), Customer360DomainError> {
        if self.status != Customer360RecordStatus::Draft {
            return Err(Customer360DomainError::InvalidOwnedRecord);
        }
        self.advance(at)?;
        self.status = Customer360RecordStatus::Active;
        Ok(())
    }

    pub fn close(
        &mut self,
        actor: String,
        at: DateTime<Utc>,
    ) -> Result<(), Customer360DomainError> {
        if self.status != Customer360RecordStatus::Active || actor.trim().is_empty() {
            return Err(Customer360DomainError::InvalidOwnedRecord);
        }
        self.advance(at)?;
        self.status = Customer360RecordStatus::Closed;
        self.closed_by = Some(actor);
        Ok(())
    }

    pub fn cancel(
        &mut self,
        actor: String,
        at: DateTime<Utc>,
    ) -> Result<(), Customer360DomainError> {
        if self.status != Customer360RecordStatus::Draft || actor.trim().is_empty() {
            return Err(Customer360DomainError::InvalidOwnedRecord);
        }
        self.advance(at)?;
        self.status = Customer360RecordStatus::Cancelled;
        self.closed_by = Some(actor);
        Ok(())
    }

    pub fn validate(&self) -> Result<(), Customer360DomainError> {
        let customer_ok = |record: &StableRecordRef| {
            record.owner_context() == ContextId::CustomerAuthority
                && record.record_type().as_str() == "customer.account"
                && record.validate().is_ok()
        };
        if self.version == 0
            || self.source_version == 0
            || self.scope.validate().is_err()
            || !matches!(
                self.scope,
                AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
            )
            || !customer_ok(&self.customer_account)
            || self.title.trim().is_empty()
            || self.title.len() > 255
            || self.created_by.trim().is_empty()
            || !valid_digest(&self.content_sha256)
            || self.updated_at < self.created_at
            || self
                .content_object_ref
                .as_ref()
                .is_some_and(|v| v.trim().is_empty() || v.len() > 512)
        {
            return Err(Customer360DomainError::InvalidOwnedRecord);
        }
        match (self.kind, &self.related_customer_account) {
            (Customer360RecordKind::Merge, Some(related))
                if customer_ok(related) && related != &self.customer_account => {}
            (Customer360RecordKind::Profile | Customer360RecordKind::Note, None) => {}
            _ => return Err(Customer360DomainError::InvalidOwnedRecord),
        }
        if matches!(
            self.status,
            Customer360RecordStatus::Closed | Customer360RecordStatus::Cancelled
        ) != self.closed_by.is_some()
        {
            return Err(Customer360DomainError::InvalidOwnedRecord);
        }
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), Customer360DomainError> {
        if at < self.updated_at {
            return Err(Customer360DomainError::InvalidOwnedRecord);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(Customer360DomainError::InvalidOwnedRecord)?;
        self.updated_at = at;
        Ok(())
    }

    pub fn record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        StableRecordRef::try_new(
            self.id,
            ContextId::Customer360,
            RecordTypeId::try_new(self.kind.record_type())?,
        )
    }
    pub const fn id(&self) -> RecordId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn kind(&self) -> Customer360RecordKind {
        self.kind
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn customer_account(&self) -> &StableRecordRef {
        &self.customer_account
    }
    pub const fn related_customer_account(&self) -> Option<&StableRecordRef> {
        self.related_customer_account.as_ref()
    }
    pub const fn source_version(&self) -> u64 {
        self.source_version
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn content_object_ref(&self) -> Option<&str> {
        self.content_object_ref.as_deref()
    }
    pub fn content_sha256(&self) -> &str {
        &self.content_sha256
    }
    pub const fn status(&self) -> Customer360RecordStatus {
        self.status
    }
    pub fn created_by(&self) -> &str {
        &self.created_by
    }
    pub fn closed_by(&self) -> Option<&str> {
        self.closed_by.as_deref()
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
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
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
    fn customer(_id: u128) -> StableRecordRef {
        StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            ContextId::CustomerAuthority,
            RecordTypeId::try_new("customer.account").unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn merge_is_a_distinct_record_and_requires_two_customers() {
        let mut record = Customer360OwnedRecord::create(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            Customer360RecordKind::Merge,
            scope(),
            customer(11),
            Some(customer(12)),
            4,
            "Merge CUS-11 into CUS-12".into(),
            None,
            format!("sha256:{}", "a".repeat(64)),
            "maker".into(),
            at(),
        )
        .unwrap();
        assert_eq!(
            record.record_ref().unwrap().record_type().as_str(),
            "customer-360.merge"
        );
        record.activate(at() + Duration::minutes(1)).unwrap();
        record
            .close("checker".into(), at() + Duration::minutes(2))
            .unwrap();
        assert_eq!(record.version(), 3);
    }
    #[test]
    fn profile_cannot_alias_a_related_customer() {
        assert!(
            Customer360OwnedRecord::create(
                RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
                Customer360RecordKind::Profile,
                scope(),
                customer(21),
                Some(customer(22)),
                1,
                "Profile".into(),
                None,
                format!("sha256:{}", "b".repeat(64)),
                "maker".into(),
                at(),
            )
            .is_err()
        );
    }
}
