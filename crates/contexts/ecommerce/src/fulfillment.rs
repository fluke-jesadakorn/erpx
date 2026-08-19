use chrono::{DateTime, Utc};
use erpx_integration_contracts::StableRecordRef;
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{CommerceError, CommerceIdentity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FulfillmentStatus {
    Proposed,
    Accepted,
    Dispatched,
    Completed,
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FulfillmentRequest {
    id: Uuid,
    version: u64,
    scope: AuthorityScope,
    owner: CommerceIdentity,
    checkout: StableRecordRef,
    order: StableRecordRef,
    reservation_digest: String,
    status: FulfillmentStatus,
    updated_at: DateTime<Utc>,
}

impl FulfillmentRequest {
    pub fn propose(
        id: Uuid,
        scope: AuthorityScope,
        owner: CommerceIdentity,
        checkout: StableRecordRef,
        order: StableRecordRef,
        reservation_digest: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<Self, CommerceError> {
        let value = Self {
            id,
            version: 1,
            scope,
            owner,
            checkout,
            order,
            reservation_digest: reservation_digest.into(),
            status: FulfillmentStatus::Proposed,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn accept(&mut self, at: DateTime<Utc>) -> Result<(), CommerceError> {
        self.transition(FulfillmentStatus::Proposed, FulfillmentStatus::Accepted, at)
    }
    pub fn dispatch(&mut self, at: DateTime<Utc>) -> Result<(), CommerceError> {
        self.transition(
            FulfillmentStatus::Accepted,
            FulfillmentStatus::Dispatched,
            at,
        )
    }
    pub fn complete(&mut self, at: DateTime<Utc>) -> Result<(), CommerceError> {
        self.transition(
            FulfillmentStatus::Dispatched,
            FulfillmentStatus::Completed,
            at,
        )
    }
    pub fn reject(&mut self, at: DateTime<Utc>) -> Result<(), CommerceError> {
        self.transition(FulfillmentStatus::Proposed, FulfillmentStatus::Rejected, at)
    }
    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), CommerceError> {
        if matches!(
            self.status,
            FulfillmentStatus::Completed
                | FulfillmentStatus::Rejected
                | FulfillmentStatus::Cancelled
        ) {
            return Err(CommerceError::InvalidTransition);
        }
        self.status = FulfillmentStatus::Cancelled;
        self.advance(at)
    }
    pub fn validate(&self) -> Result<(), CommerceError> {
        self.scope
            .validate()
            .map_err(|_| CommerceError::InvalidScope)?;
        self.checkout
            .validate()
            .map_err(|_| CommerceError::InvalidFulfillment)?;
        self.order
            .validate()
            .map_err(|_| CommerceError::InvalidFulfillment)?;
        if self.id.is_nil()
            || self.id.get_version_num() != 4
            || self.version == 0
            || self.reservation_digest.len() != 64
            || !self
                .reservation_digest
                .bytes()
                .all(|v| v.is_ascii_hexdigit())
        {
            return Err(CommerceError::InvalidFulfillment);
        }
        Ok(())
    }
    fn transition(
        &mut self,
        e: FulfillmentStatus,
        n: FulfillmentStatus,
        at: DateTime<Utc>,
    ) -> Result<(), CommerceError> {
        if self.status != e {
            return Err(CommerceError::InvalidTransition);
        }
        self.status = n;
        self.advance(at)
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), CommerceError> {
        if at < self.updated_at {
            return Err(CommerceError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(CommerceError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }
    pub const fn id(&self) -> Uuid {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn owner(&self) -> &CommerceIdentity {
        &self.owner
    }
    pub const fn status(&self) -> FulfillmentStatus {
        self.status
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub const fn checkout(&self) -> &StableRecordRef {
        &self.checkout
    }
    pub const fn order(&self) -> &StableRecordRef {
        &self.order
    }
    pub fn reservation_digest(&self) -> &str {
        &self.reservation_digest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_integration_contracts::{RecordTypeId, catalog::ContextId};
    use erpx_kernel::RecordId;
    fn scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
        }
    }
    fn r(t: &str) -> StableRecordRef {
        StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            ContextId::Ecommerce,
            RecordTypeId::try_new(t).unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn fulfillment_is_one_versioned_reconciliation_root() {
        let now = Utc::now();
        let owner = CommerceIdentity {
            realm: crate::CommerceRealm::Customer,
            principal_id: Uuid::new_v4(),
            verified_customer: None,
        };
        let mut f = FulfillmentRequest::propose(
            Uuid::new_v4(),
            scope(),
            owner,
            r("commerce.order"),
            r("commerce.order"),
            "a".repeat(64),
            now,
        )
        .unwrap();
        f.accept(now).unwrap();
        f.dispatch(now).unwrap();
        f.complete(now).unwrap();
        assert_eq!(f.version(), 4);
        assert!(f.cancel(now).is_err())
    }
}
