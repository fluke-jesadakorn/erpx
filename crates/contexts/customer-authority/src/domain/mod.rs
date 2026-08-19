use chrono::{DateTime, Utc};
use erpx_integration_contracts::{
    StableRecordRef,
    catalog::ContextId,
    reference::{RecordTypeId, ReferenceValidationError},
};
use erpx_kernel::{AuthorityScope, RecordId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

mod authority_record;
pub use authority_record::{
    CustomerAuthorityRecord, CustomerAuthorityRecordKind, CustomerAuthorityRecordStatus,
};

mod portal_binding;

pub use portal_binding::{
    CustomerPortalBindingId, CustomerPortalBindingStatus, CustomerPortalPrincipalBinding,
    PortalPrincipalKind, PortalUseCase, VerifiedIdentityEvidence,
};

pub const MAX_CUSTOMER_CODE_BYTES: usize = 128;
pub const MAX_CUSTOMER_NAME_BYTES: usize = 256;
pub const MAX_CUSTOMER_COUNTRY_BYTES: usize = 2;
pub const MAX_CUSTOMER_IDENTIFIER_BYTES: usize = 256;
pub const MAX_CUSTOMER_AUTHORITY_LINKS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CustomerId(Uuid);

impl CustomerId {
    pub fn try_from_uuid(value: Uuid) -> Result<Self, CustomerAuthorityDomainError> {
        require_id(value, "customer_id")?;
        Ok(Self(value))
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl TryFrom<Uuid> for CustomerId {
    type Error = CustomerAuthorityDomainError;

    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        Self::try_from_uuid(value)
    }
}

impl From<CustomerId> for Uuid {
    fn from(value: CustomerId) -> Self {
        value.as_uuid()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CustomerCode(String);

impl CustomerCode {
    pub fn try_new(value: impl Into<String>) -> Result<Self, CustomerAuthorityDomainError> {
        let normalized = value.into().trim().to_ascii_uppercase();
        validate_text(&normalized, "customer_code", MAX_CUSTOMER_CODE_BYTES)?;
        if !normalized
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        {
            return Err(CustomerAuthorityDomainError::InvalidCustomerCode);
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerIdentity {
    trading_name: String,
    legal_name: Option<String>,
    registration_number: Option<String>,
    tax_identifier: Option<String>,
    country: String,
}

impl CustomerIdentity {
    pub fn try_new(
        trading_name: impl Into<String>,
        legal_name: Option<String>,
        registration_number: Option<String>,
        tax_identifier: Option<String>,
        country: impl Into<String>,
    ) -> Result<Self, CustomerAuthorityDomainError> {
        let value = Self {
            trading_name: trading_name.into().trim().to_owned(),
            legal_name: normalize_optional(legal_name),
            registration_number: normalize_optional(registration_number),
            tax_identifier: normalize_optional(tax_identifier),
            country: country.into().trim().to_ascii_uppercase(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), CustomerAuthorityDomainError> {
        validate_text(&self.trading_name, "trading_name", MAX_CUSTOMER_NAME_BYTES)?;
        validate_optional_text(
            self.legal_name.as_deref(),
            "legal_name",
            MAX_CUSTOMER_NAME_BYTES,
        )?;
        validate_optional_text(
            self.registration_number.as_deref(),
            "registration_number",
            MAX_CUSTOMER_IDENTIFIER_BYTES,
        )?;
        validate_optional_text(
            self.tax_identifier.as_deref(),
            "tax_identifier",
            MAX_CUSTOMER_IDENTIFIER_BYTES,
        )?;
        if self.country.len() != MAX_CUSTOMER_COUNTRY_BYTES
            || !self
                .country
                .chars()
                .all(|character| character.is_ascii_alphabetic())
        {
            return Err(CustomerAuthorityDomainError::InvalidCountry);
        }
        Ok(())
    }

    pub fn trading_name(&self) -> &str {
        &self.trading_name
    }

    pub fn legal_name(&self) -> Option<&str> {
        self.legal_name.as_deref()
    }

    pub fn registration_number(&self) -> Option<&str> {
        self.registration_number.as_deref()
    }

    pub fn tax_identifier(&self) -> Option<&str> {
        self.tax_identifier.as_deref()
    }

    pub fn country(&self) -> &str {
        &self.country
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomerStatus {
    Prospect,
    Onboarding,
    Active,
    Suspended,
    Churned,
}

impl CustomerStatus {
    const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Prospect,
                Self::Onboarding | Self::Active | Self::Suspended | Self::Churned
            ) | (
                Self::Onboarding,
                Self::Active | Self::Suspended | Self::Churned
            ) | (Self::Active, Self::Suspended | Self::Churned)
                | (Self::Suspended, Self::Active | Self::Churned)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CustomerAuthorityDomainError {
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds {maximum} bytes")]
    TooLong { field: &'static str, maximum: usize },
    #[error("{field} contains an unsupported control character")]
    ControlCharacter { field: &'static str },
    #[error("customer code has an invalid format")]
    InvalidCustomerCode,
    #[error("customer country must be a two-letter code")]
    InvalidCountry,
    #[error("a customer requires an exact company authority as its primary owner")]
    InvalidPrimaryAuthority,
    #[error("customer authority links must be exact holding, company, or branch scopes")]
    InvalidAuthorityLink,
    #[error("customer authority link already exists")]
    DuplicateAuthorityLink,
    #[error("customer authority link was not found")]
    AuthorityLinkNotFound,
    #[error("the primary customer authority cannot be removed")]
    PrimaryAuthorityCannotBeRemoved,
    #[error("customer authority link capacity was reached")]
    AuthorityLinkCapacity,
    #[error("customer lifecycle transition is not allowed: {from:?} -> {to:?}")]
    InvalidStatusTransition {
        from: CustomerStatus,
        to: CustomerStatus,
    },
    #[error("a merged customer cannot be changed")]
    CustomerMerged,
    #[error("a customer cannot be merged into itself")]
    SelfMerge,
    #[error("customers can be merged only inside the same primary authority")]
    IncompatibleMergeAuthority,
    #[error("customer timestamp moved backwards")]
    InvalidTimestamp,
    #[error("customer portal principal binding is invalid")]
    InvalidPortalBinding,
    #[error("customer portal principal binding transition is not allowed")]
    InvalidPortalBindingTransition,
    #[error("customer authority or support-access record is invalid")]
    InvalidAuthorityRecord,
    #[error("customer version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerAccount {
    id: CustomerId,
    version: u64,
    code: CustomerCode,
    identity: CustomerIdentity,
    status: CustomerStatus,
    primary_authority: AuthorityScope,
    authority_links: Vec<AuthorityScope>,
    merged_into: Option<CustomerId>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl CustomerAccount {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: CustomerId,
        version: u64,
        code: CustomerCode,
        identity: CustomerIdentity,
        status: CustomerStatus,
        primary_authority: AuthorityScope,
        authority_links: Vec<AuthorityScope>,
        merged_into: Option<CustomerId>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, CustomerAuthorityDomainError> {
        let value = Self {
            id,
            version,
            code,
            identity,
            status,
            primary_authority,
            authority_links,
            merged_into,
            created_at,
            updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn create(
        id: CustomerId,
        code: CustomerCode,
        identity: CustomerIdentity,
        primary_authority: AuthorityScope,
        at: DateTime<Utc>,
    ) -> Result<Self, CustomerAuthorityDomainError> {
        validate_primary_authority(&primary_authority)?;
        identity.validate()?;
        let mut value = Self {
            id,
            version: 1,
            code,
            identity,
            status: CustomerStatus::Prospect,
            primary_authority: primary_authority.clone(),
            authority_links: vec![primary_authority],
            merged_into: None,
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        value.emit_profile_changed(at);
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), CustomerAuthorityDomainError> {
        require_id(self.id.as_uuid(), "customer_id")?;
        if self.version == 0 {
            return Err(CustomerAuthorityDomainError::Required { field: "version" });
        }
        CustomerCode::try_new(self.code.as_str())?;
        self.identity.validate()?;
        validate_primary_authority(&self.primary_authority)?;
        if self.authority_links.is_empty()
            || self.authority_links.len() > MAX_CUSTOMER_AUTHORITY_LINKS
            || !self.authority_links.contains(&self.primary_authority)
            || self
                .authority_links
                .iter()
                .any(|scope| validate_authority_link(scope).is_err())
        {
            return Err(CustomerAuthorityDomainError::InvalidAuthorityLink);
        }
        for (index, scope) in self.authority_links.iter().enumerate() {
            if self.authority_links[index + 1..].contains(scope) {
                return Err(CustomerAuthorityDomainError::DuplicateAuthorityLink);
            }
        }
        if self.merged_into == Some(self.id) {
            return Err(CustomerAuthorityDomainError::SelfMerge);
        }
        if self.updated_at < self.created_at {
            return Err(CustomerAuthorityDomainError::InvalidTimestamp);
        }
        Ok(())
    }

    pub fn update_identity(
        &mut self,
        identity: CustomerIdentity,
        at: DateTime<Utc>,
    ) -> Result<(), CustomerAuthorityDomainError> {
        self.ensure_mutable(at)?;
        identity.validate()?;
        self.version = next_version(self.version)?;
        self.identity = identity;
        self.updated_at = at;
        self.emit_profile_changed(at);
        Ok(())
    }

    pub fn change_status(
        &mut self,
        status: CustomerStatus,
        at: DateTime<Utc>,
    ) -> Result<(), CustomerAuthorityDomainError> {
        self.ensure_mutable(at)?;
        if !self.status.can_transition_to(status) {
            return Err(CustomerAuthorityDomainError::InvalidStatusTransition {
                from: self.status,
                to: status,
            });
        }
        self.version = next_version(self.version)?;
        self.status = status;
        self.updated_at = at;
        self.emit_profile_changed(at);
        Ok(())
    }

    pub fn link_authority(
        &mut self,
        scope: AuthorityScope,
        at: DateTime<Utc>,
    ) -> Result<(), CustomerAuthorityDomainError> {
        self.ensure_mutable(at)?;
        validate_authority_link(&scope)?;
        if self.authority_links.contains(&scope) {
            return Err(CustomerAuthorityDomainError::DuplicateAuthorityLink);
        }
        if self.authority_links.len() == MAX_CUSTOMER_AUTHORITY_LINKS {
            return Err(CustomerAuthorityDomainError::AuthorityLinkCapacity);
        }
        self.version = next_version(self.version)?;
        self.authority_links.push(scope.clone());
        self.updated_at = at;
        self.uncommitted_events.push(DomainEvent::AuthorityLinked {
            customer_id: self.id,
            scope,
            at,
        });
        Ok(())
    }

    pub fn unlink_authority(
        &mut self,
        scope: &AuthorityScope,
        at: DateTime<Utc>,
    ) -> Result<(), CustomerAuthorityDomainError> {
        self.ensure_mutable(at)?;
        if scope == &self.primary_authority {
            return Err(CustomerAuthorityDomainError::PrimaryAuthorityCannotBeRemoved);
        }
        let Some(index) = self.authority_links.iter().position(|link| link == scope) else {
            return Err(CustomerAuthorityDomainError::AuthorityLinkNotFound);
        };
        self.version = next_version(self.version)?;
        self.authority_links.remove(index);
        self.updated_at = at;
        self.uncommitted_events
            .push(DomainEvent::AuthorityUnlinked {
                customer_id: self.id,
                scope: scope.clone(),
                at,
            });
        Ok(())
    }

    pub(crate) fn merge_into(
        &mut self,
        survivor: CustomerId,
        at: DateTime<Utc>,
    ) -> Result<(), CustomerAuthorityDomainError> {
        self.ensure_mutable(at)?;
        if survivor == self.id {
            return Err(CustomerAuthorityDomainError::SelfMerge);
        }
        self.version = next_version(self.version)?;
        self.status = CustomerStatus::Churned;
        self.merged_into = Some(survivor);
        self.updated_at = at;
        self.uncommitted_events.push(DomainEvent::Merged {
            retired_customer_id: self.id,
            surviving_customer_id: survivor,
            at,
        });
        Ok(())
    }

    /// Merge while enforcing the authority boundary of the surviving record.
    /// A holding- or platform-scoped caller may administer both customers, but
    /// that broader caller authority does not make cross-company identities
    /// compatible merge targets.
    pub fn merge_into_compatible(
        &mut self,
        survivor: CustomerId,
        survivor_primary_authority: &AuthorityScope,
        at: DateTime<Utc>,
    ) -> Result<(), CustomerAuthorityDomainError> {
        if &self.primary_authority != survivor_primary_authority {
            return Err(CustomerAuthorityDomainError::IncompatibleMergeAuthority);
        }
        self.merge_into(survivor, at)
    }

    fn ensure_mutable(&self, at: DateTime<Utc>) -> Result<(), CustomerAuthorityDomainError> {
        if self.merged_into.is_some() {
            return Err(CustomerAuthorityDomainError::CustomerMerged);
        }
        if at < self.updated_at {
            return Err(CustomerAuthorityDomainError::InvalidTimestamp);
        }
        Ok(())
    }

    fn emit_profile_changed(&mut self, at: DateTime<Utc>) {
        self.uncommitted_events.push(DomainEvent::ProfileChanged {
            profile: self.published_profile(),
            at,
        });
    }

    pub const fn id(&self) -> CustomerId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn code(&self) -> &CustomerCode {
        &self.code
    }
    pub fn identity(&self) -> &CustomerIdentity {
        &self.identity
    }
    pub const fn status(&self) -> CustomerStatus {
        self.status
    }
    pub const fn primary_authority(&self) -> &AuthorityScope {
        &self.primary_authority
    }
    pub fn authority_links(&self) -> &[AuthorityScope] {
        &self.authority_links
    }
    pub const fn merged_into(&self) -> Option<CustomerId> {
        self.merged_into
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Stable, route-independent identity used by Collaboration and the Item
    /// Directory. Descriptor publication remains an adapter responsibility
    /// because the source outbox position is assigned at commit time.
    pub fn record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        let record_id = RecordId::try_from_uuid(self.id.as_uuid())
            .expect("validated customer identifier is non-nil");
        StableRecordRef::try_new(
            record_id,
            ContextId::CustomerAuthority,
            RecordTypeId::try_new("customer.account")?,
        )
    }

    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }

    fn published_profile(&self) -> PublishedCustomerProfile {
        PublishedCustomerProfile {
            customer_id: self.id,
            code: self.code.clone(),
            display_name: self.identity.trading_name.clone(),
            owning_company_id: primary_company_id(&self.primary_authority)
                .expect("validated primary authority always has a company"),
            status: self.status,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublishedCustomerProfile {
    pub customer_id: CustomerId,
    pub code: CustomerCode,
    pub display_name: String,
    pub owning_company_id: Uuid,
    pub status: CustomerStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    ProfileChanged {
        profile: PublishedCustomerProfile,
        at: DateTime<Utc>,
    },
    AuthorityLinked {
        customer_id: CustomerId,
        scope: AuthorityScope,
        at: DateTime<Utc>,
    },
    AuthorityUnlinked {
        customer_id: CustomerId,
        scope: AuthorityScope,
        at: DateTime<Utc>,
    },
    Merged {
        retired_customer_id: CustomerId,
        surviving_customer_id: CustomerId,
        at: DateTime<Utc>,
    },
}

fn primary_company_id(scope: &AuthorityScope) -> Option<Uuid> {
    match scope {
        AuthorityScope::Company { company_id, .. } => Some(*company_id),
        _ => None,
    }
}

fn validate_primary_authority(scope: &AuthorityScope) -> Result<(), CustomerAuthorityDomainError> {
    scope
        .validate()
        .map_err(|_| CustomerAuthorityDomainError::InvalidPrimaryAuthority)?;
    if !matches!(scope, AuthorityScope::Company { .. }) {
        return Err(CustomerAuthorityDomainError::InvalidPrimaryAuthority);
    }
    Ok(())
}

fn validate_authority_link(scope: &AuthorityScope) -> Result<(), CustomerAuthorityDomainError> {
    scope
        .validate()
        .map_err(|_| CustomerAuthorityDomainError::InvalidAuthorityLink)?;
    if matches!(
        scope,
        AuthorityScope::Platform | AuthorityScope::Subtree { .. }
    ) {
        return Err(CustomerAuthorityDomainError::InvalidAuthorityLink);
    }
    Ok(())
}

fn require_id(value: Uuid, field: &'static str) -> Result<(), CustomerAuthorityDomainError> {
    if value.is_nil() {
        return Err(CustomerAuthorityDomainError::NilIdentifier { field });
    }
    Ok(())
}

fn validate_text(
    value: &str,
    field: &'static str,
    maximum: usize,
) -> Result<(), CustomerAuthorityDomainError> {
    if value.trim().is_empty() {
        return Err(CustomerAuthorityDomainError::Required { field });
    }
    if value.len() > maximum {
        return Err(CustomerAuthorityDomainError::TooLong { field, maximum });
    }
    if value.chars().any(char::is_control) {
        return Err(CustomerAuthorityDomainError::ControlCharacter { field });
    }
    Ok(())
}

fn validate_optional_text(
    value: Option<&str>,
    field: &'static str,
    maximum: usize,
) -> Result<(), CustomerAuthorityDomainError> {
    if let Some(value) = value {
        validate_text(value, field, maximum)?;
    }
    Ok(())
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}

fn next_version(version: u64) -> Result<u64, CustomerAuthorityDomainError> {
    version
        .checked_add(1)
        .ok_or(CustomerAuthorityDomainError::VersionOverflow)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 5, hour, 0, 0)
            .single()
            .unwrap()
    }

    fn company() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: id(1),
            company_id: id(2),
        }
    }

    fn account() -> CustomerAccount {
        CustomerAccount::create(
            CustomerId::try_from_uuid(id(10)).unwrap(),
            CustomerCode::try_new("c-100").unwrap(),
            CustomerIdentity::try_new("Acme", Some("Acme Ltd".into()), None, None, "th").unwrap(),
            company(),
            at(9),
        )
        .unwrap()
    }

    #[test]
    fn creation_normalizes_identity_and_emits_private_profile_event() {
        let mut account = account();
        assert_eq!(account.code().as_str(), "C-100");
        assert_eq!(account.identity().country(), "TH");
        assert_eq!(account.version(), 1);
        assert_eq!(account.take_uncommitted_events().len(), 1);
    }

    #[test]
    fn identity_exposes_validated_transport_fields_without_serialization() {
        let identity = CustomerIdentity::try_new(
            "Acme",
            Some("Acme Ltd".into()),
            Some("REG-42".into()),
            Some("TAX-42".into()),
            "th",
        )
        .unwrap();
        assert_eq!(identity.registration_number(), Some("REG-42"));
        assert_eq!(identity.tax_identifier(), Some("TAX-42"));
    }

    #[test]
    fn primary_authority_must_be_an_exact_company() {
        let result = CustomerAccount::create(
            CustomerId::try_from_uuid(id(10)).unwrap(),
            CustomerCode::try_new("C-100").unwrap(),
            CustomerIdentity::try_new("Acme", None, None, None, "TH").unwrap(),
            AuthorityScope::Holding { holding_id: id(1) },
            at(9),
        );
        assert_eq!(
            result.unwrap_err(),
            CustomerAuthorityDomainError::InvalidPrimaryAuthority
        );
    }

    #[test]
    fn platform_and_subtree_cannot_be_customer_authority_links() {
        let mut account = account();
        assert_eq!(
            account.link_authority(AuthorityScope::Platform, at(10)),
            Err(CustomerAuthorityDomainError::InvalidAuthorityLink)
        );
    }

    #[test]
    fn links_are_exact_unique_and_primary_link_cannot_be_removed() {
        let mut account = account();
        let branch = AuthorityScope::Branch {
            holding_id: id(1),
            company_id: id(2),
            branch_id: id(3),
        };
        account.link_authority(branch.clone(), at(10)).unwrap();
        assert_eq!(
            account.link_authority(branch, at(11)),
            Err(CustomerAuthorityDomainError::DuplicateAuthorityLink)
        );
        assert_eq!(
            account.unlink_authority(&company(), at(11)),
            Err(CustomerAuthorityDomainError::PrimaryAuthorityCannotBeRemoved)
        );
    }

    #[test]
    fn lifecycle_disallows_reopening_churned_customer() {
        let mut account = account();
        account
            .change_status(CustomerStatus::Churned, at(10))
            .unwrap();
        assert!(matches!(
            account.change_status(CustomerStatus::Active, at(11)),
            Err(CustomerAuthorityDomainError::InvalidStatusTransition { .. })
        ));
    }

    #[test]
    fn merge_is_terminal_and_rejects_self_merge() {
        let mut account = account();
        assert_eq!(
            account.merge_into(account.id(), at(10)),
            Err(CustomerAuthorityDomainError::SelfMerge)
        );
        account
            .merge_into(CustomerId::try_from_uuid(id(11)).unwrap(), at(10))
            .unwrap();
        assert_eq!(
            account.update_identity(
                CustomerIdentity::try_new("New", None, None, None, "TH").unwrap(),
                at(11)
            ),
            Err(CustomerAuthorityDomainError::CustomerMerged)
        );
    }

    #[test]
    fn aggregate_rejects_time_travel() {
        let mut account = account();
        assert_eq!(
            account.change_status(CustomerStatus::Active, at(8)),
            Err(CustomerAuthorityDomainError::InvalidTimestamp)
        );
    }
}
