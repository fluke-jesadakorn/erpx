use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::access::Scope;

fn default_customer_status() -> CustomerStatus {
    CustomerStatus::Prospect
}

fn default_billing_state() -> super::commercial::BillingState {
    super::commercial::BillingState::Trialing
}

fn default_health_state() -> HealthState {
    HealthState::Unknown
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CustomerStatus {
    Prospect,
    Onboarding,
    Active,
    Suspended,
    Churned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthState {
    Healthy,
    Degraded,
    Down,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerAccount {
    pub id: Uuid,
    pub account_code: String,
    pub trading_name: String,
    pub legal_name: Option<String>,
    pub industry: Option<String>,
    pub country: String,
    pub status: CustomerStatus,
    pub billing_state: super::commercial::BillingState,
    pub renewal_at: Option<DateTime<Utc>>,
    pub lifetime_value_minor: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerHealth {
    pub customer_account_id: Uuid,
    pub state: HealthState,
    pub projection_lag_seconds: u32,
    pub worker_failures: u32,
    pub backup_status: String,
    pub last_activity_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Customer360 {
    pub account: CustomerAccount,
    pub health: CustomerHealth,
    pub holding_count: u32,
    pub company_count: u32,
    pub branch_count: u32,
    pub active_user_count: u32,
    pub expiring_role_count: u32,
    pub open_ticket_count: u32,
    pub module_count: u32,
}

/// The enriched account projection used by Customer 360. The original
/// `CustomerAccount` remains unchanged so existing persisted payloads and
/// constructors remain compatible while the richer projection is introduced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerAccountProjection {
    #[serde(default)]
    pub id: Uuid,
    #[serde(default)]
    pub account_code: String,
    #[serde(default)]
    pub identity: TradingLegalIdentity,
    #[serde(default)]
    pub industry: Option<String>,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub locale: String,
    #[serde(default)]
    pub billing_address: Option<BillingAddress>,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub logos: Vec<LogoReference>,
    #[serde(default = "default_customer_status")]
    pub account_state: CustomerStatus,
    #[serde(default = "default_billing_state")]
    pub subscription_state: super::commercial::BillingState,
    #[serde(default)]
    pub renewal_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub lifetime_value_minor: u64,
    #[serde(default)]
    pub responsibility_contacts: Vec<ResponsibilityContact>,
    #[serde(default)]
    pub customer_success_owner: Option<CustomerSuccessOwner>,
}

impl Default for CustomerAccountProjection {
    fn default() -> Self {
        Self {
            id: Uuid::nil(),
            account_code: String::new(),
            identity: TradingLegalIdentity::default(),
            industry: None,
            country: String::new(),
            locale: String::new(),
            billing_address: None,
            domains: Vec::new(),
            logos: Vec::new(),
            account_state: CustomerStatus::Prospect,
            subscription_state: super::commercial::BillingState::Trialing,
            renewal_at: None,
            lifetime_value_minor: 0,
            responsibility_contacts: Vec::new(),
            customer_success_owner: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TradingLegalIdentity {
    #[serde(default)]
    pub trading_name: String,
    #[serde(default)]
    pub legal_name: Option<String>,
    #[serde(default)]
    pub registration_number: Option<String>,
    #[serde(default)]
    pub tax_identifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BillingAddress {
    #[serde(default)]
    pub line_1: String,
    #[serde(default)]
    pub line_2: Option<String>,
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub postal_code: String,
    #[serde(default)]
    pub country: String,
}

/// A MinIO object reference. It intentionally contains no signed URL or
/// credential; those are issued at the authorized application boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LogoReference {
    #[serde(default)]
    pub variant: String,
    #[serde(default)]
    pub object_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ResponsibilityContact {
    #[serde(default)]
    pub id: Uuid,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub responsibilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CustomerSuccessOwner {
    #[serde(default)]
    pub principal_ref: String,
    #[serde(default)]
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OrganizationUnit {
    #[serde(default)]
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub parent_id: Option<Uuid>,
    #[serde(default)]
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModuleAllocation {
    #[serde(default)]
    pub module_key: String,
    #[serde(default)]
    pub purchased_quantity: u32,
    #[serde(default)]
    pub allocated_quantity: u32,
    #[serde(default)]
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CustomerOrganizationProjection {
    #[serde(default)]
    pub headquarters: Option<OrganizationUnit>,
    #[serde(default)]
    pub holdings: Vec<OrganizationUnit>,
    #[serde(default)]
    pub companies: Vec<OrganizationUnit>,
    #[serde(default)]
    pub branches: Vec<OrganizationUnit>,
    #[serde(default)]
    pub departments: Vec<OrganizationUnit>,
    #[serde(default)]
    pub active_user_count: u32,
    #[serde(default)]
    pub expiring_role_count: u32,
    #[serde(default)]
    pub open_ticket_count: u32,
    #[serde(default)]
    pub holding_count: u32,
    #[serde(default)]
    pub company_count: u32,
    #[serde(default)]
    pub branch_count: u32,
    #[serde(default)]
    pub module_count: u32,
    #[serde(default)]
    pub module_allocations: Vec<ModuleAllocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CommercialInvoiceSummary {
    #[serde(default)]
    pub invoice_ref: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub amount_minor: u64,
    #[serde(default)]
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CommercialQuoteSummary {
    #[serde(default)]
    pub quote_ref: String,
    #[serde(default)]
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerCommercialProjection {
    #[serde(default)]
    pub stripe_customer_ref: Option<String>,
    #[serde(default)]
    pub stripe_subscription_ref: Option<String>,
    #[serde(default)]
    pub invoices: Vec<CommercialInvoiceSummary>,
    #[serde(default = "default_billing_state")]
    pub payment_state: super::commercial::BillingState,
    #[serde(default)]
    pub trial_ends_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub renewal_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub discounts: Vec<String>,
    #[serde(default)]
    pub quotes: Vec<CommercialQuoteSummary>,
    #[serde(default)]
    pub lifetime_value_minor: u64,
    #[serde(default)]
    pub module_allocations: Vec<ModuleAllocation>,
}

impl Default for CustomerCommercialProjection {
    fn default() -> Self {
        Self {
            stripe_customer_ref: None,
            stripe_subscription_ref: None,
            invoices: Vec::new(),
            payment_state: super::commercial::BillingState::Trialing,
            trial_ends_at: None,
            renewal_at: None,
            discounts: Vec::new(),
            quotes: Vec::new(),
            lifetime_value_minor: 0,
            module_allocations: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OnboardingState {
    #[default]
    NotStarted,
    InProgress,
    ReadyForGoLive,
    Live,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CustomerOnboardingProjection {
    #[serde(default)]
    pub state: OnboardingState,
    #[serde(default)]
    pub imports_completed: u32,
    #[serde(default)]
    pub training_completed: bool,
    #[serde(default)]
    pub go_live_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub owner: Option<CustomerSuccessOwner>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerHealthProjection {
    #[serde(default = "default_health_state")]
    pub state: HealthState,
    #[serde(default)]
    pub database_schema_version: String,
    #[serde(default)]
    pub projection_lag_seconds: u32,
    #[serde(default)]
    pub worker_failures: u32,
    #[serde(default)]
    pub storage_status: String,
    #[serde(default)]
    pub backup_status: String,
    #[serde(default)]
    pub point_in_time_recovery_status: String,
    #[serde(default)]
    pub integration_failure_count: u32,
    #[serde(default)]
    pub api_usage_count: u64,
    #[serde(default)]
    pub last_activity_at: Option<DateTime<Utc>>,
}

impl Default for CustomerHealthProjection {
    fn default() -> Self {
        Self {
            state: HealthState::Unknown,
            database_schema_version: String::new(),
            projection_lag_seconds: 0,
            worker_failures: 0,
            storage_status: String::new(),
            backup_status: String::new(),
            point_in_time_recovery_status: String::new(),
            integration_failure_count: 0,
            api_usage_count: 0,
            last_activity_at: None,
        }
    }
}

/// The complete control-plane projection. New fields are independently
/// defaultable so old projection documents can be read during migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Customer360Projection {
    #[serde(default)]
    pub account: CustomerAccountProjection,
    #[serde(default)]
    pub organization: CustomerOrganizationProjection,
    #[serde(default)]
    pub commercial: CustomerCommercialProjection,
    #[serde(default)]
    pub onboarding: CustomerOnboardingProjection,
    #[serde(default)]
    pub health: CustomerHealthProjection,
}

/// A rebuildable projection record used to populate the write-side customer
/// scope authority. It contains only organization identifiers and no tenant
/// credentials, storage keys, or secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerScopeBindingProjection {
    pub customer_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Option<Uuid>,
    pub branch_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Customer360Summary {
    pub id: Uuid,
    pub account_code: String,
    pub trading_name: String,
    pub country: String,
    pub account_state: CustomerStatus,
    pub subscription_state: super::commercial::BillingState,
    pub holding_count: u32,
    pub company_count: u32,
    pub branch_count: u32,
    pub active_user_count: u32,
    pub module_count: u32,
    pub health_state: HealthState,
}

/// A read projection older than this threshold is still usable for display,
/// but consumers must surface that it may not reflect the latest write-side
/// state. The threshold is intentionally a domain constant so every client
/// applies the same freshness semantics.
pub const CUSTOMER_READ_STALE_AFTER_SECONDS: i64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CustomerReadFreshness {
    Fresh,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerReadMetadata {
    pub customer_id: Uuid,
    pub projection_updated_at: Option<DateTime<Utc>>,
    pub freshness: CustomerReadFreshness,
    pub read_only: bool,
}

impl CustomerReadMetadata {
    pub fn present(
        customer_id: Uuid,
        projection_updated_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Self {
        let age = now.signed_duration_since(projection_updated_at);
        let freshness = if age < chrono::Duration::zero()
            || age > chrono::Duration::seconds(CUSTOMER_READ_STALE_AFTER_SECONDS)
        {
            CustomerReadFreshness::Stale
        } else {
            CustomerReadFreshness::Fresh
        };
        Self {
            customer_id,
            projection_updated_at: Some(projection_updated_at),
            freshness,
            read_only: true,
        }
    }

    pub fn absent(customer_id: Uuid) -> Self {
        Self {
            customer_id,
            projection_updated_at: None,
            freshness: CustomerReadFreshness::Unknown,
            read_only: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CustomerReadView<T> {
    Present {
        data: T,
        metadata: CustomerReadMetadata,
    },
    Absent {
        metadata: CustomerReadMetadata,
    },
    Unsupported {
        metadata: CustomerReadMetadata,
        reason: String,
    },
}

impl<T> CustomerReadView<T> {
    pub fn present(
        data: T,
        customer_id: Uuid,
        projection_updated_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Self {
        Self::Present {
            data,
            metadata: CustomerReadMetadata::present(customer_id, projection_updated_at, now),
        }
    }

    pub fn absent(customer_id: Uuid) -> Self {
        Self::Absent {
            metadata: CustomerReadMetadata::absent(customer_id),
        }
    }

    pub fn map<U>(self, transform: impl FnOnce(T) -> U) -> CustomerReadView<U> {
        match self {
            Self::Present { data, metadata } => CustomerReadView::Present {
                data: transform(data),
                metadata,
            },
            Self::Absent { metadata } => CustomerReadView::Absent { metadata },
            Self::Unsupported { metadata, reason } => {
                CustomerReadView::Unsupported { metadata, reason }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerOrganizationReadDto {
    pub organization: CustomerOrganizationProjection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerContactsReadDto {
    pub contacts: Vec<ResponsibilityContact>,
    pub customer_success_owner: Option<CustomerSuccessOwner>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerCommercialReadDto {
    pub commercial: CustomerCommercialProjection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerEntitlementsReadDto {
    pub subscription_allocations: Vec<ModuleAllocation>,
    pub organization_allocations: Vec<ModuleAllocation>,
    pub active_module_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerOnboardingReadDto {
    pub onboarding: CustomerOnboardingProjection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerHealthReadDto {
    pub health: CustomerHealthProjection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerSupportSummaryReadDto {
    pub open_ticket_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CustomerAccessAuditSummaryReadDto {
    pub read_count: u64,
    pub last_access_at: Option<DateTime<Utc>>,
    pub customer_visible: bool,
}

/// Customer-security-facing detail for one global read. This intentionally
/// excludes the persisted audit payload; the structured attribution is the
/// complete customer-visible record needed by the security audit view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerGlobalReadAuditDetailReadDto {
    pub audit_id: Uuid,
    pub actor: GlobalReadActor,
    pub customer_id: Uuid,
    pub module: String,
    pub scope: GlobalReadScope,
    pub purpose: String,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerNoteReadDto {
    pub id: Uuid,
    pub author_ref: String,
    pub created_at: DateTime<Utc>,
    pub body: String,
}

pub const CUSTOMER_NOTE_MAX_BODY_BYTES: usize = 16 * 1024;

/// The authoritative append-only Customer 360 note aggregate. Notes are
/// persisted on the control write side and copied to the control read side by
/// the Customer 360 projector; a read-model row is never a note authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerNote {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub author_ref: String,
    pub scope: Scope,
    pub created_at: DateTime<Utc>,
    pub body: String,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CustomerNoteError {
    #[error("customer note ID is required")]
    MissingId,
    #[error("customer note customer ID is required")]
    MissingCustomerId,
    #[error("customer note author is required")]
    MissingAuthor,
    #[error("customer note body is required")]
    MissingBody,
    #[error("customer note body exceeds the bounded maximum")]
    BodyTooLarge,
    #[error("customer note body contains unsupported control characters")]
    InvalidBody,
    #[error("customer note scope is invalid")]
    InvalidScope,
}

impl CustomerNote {
    pub fn new(
        id: Uuid,
        customer_id: Uuid,
        author_ref: impl Into<String>,
        scope: Scope,
        created_at: DateTime<Utc>,
        body: impl Into<String>,
    ) -> Result<Self, CustomerNoteError> {
        let author_ref = author_ref.into();
        let body = body.into();
        if id.is_nil() {
            return Err(CustomerNoteError::MissingId);
        }
        if customer_id.is_nil() {
            return Err(CustomerNoteError::MissingCustomerId);
        }
        if author_ref.trim().is_empty() || author_ref.chars().any(char::is_control) {
            return Err(CustomerNoteError::MissingAuthor);
        }
        if body.trim().is_empty() {
            return Err(CustomerNoteError::MissingBody);
        }
        if body.len() > CUSTOMER_NOTE_MAX_BODY_BYTES {
            return Err(CustomerNoteError::BodyTooLarge);
        }
        if body
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        {
            return Err(CustomerNoteError::InvalidBody);
        }
        if !valid_customer_note_scope(&scope) {
            return Err(CustomerNoteError::InvalidScope);
        }
        Ok(Self {
            id,
            customer_id,
            author_ref,
            scope,
            created_at,
            body,
        })
    }

    pub fn read_dto(&self) -> CustomerNoteReadDto {
        CustomerNoteReadDto {
            id: self.id,
            author_ref: self.author_ref.clone(),
            created_at: self.created_at,
            body: self.body.clone(),
        }
    }

    /// Re-validate a deserialized or persisted note before it is treated as
    /// an authoritative aggregate or projected into a read model.
    pub fn validate(&self) -> Result<(), CustomerNoteError> {
        Self::new(
            self.id,
            self.customer_id,
            self.author_ref.clone(),
            self.scope.clone(),
            self.created_at,
            self.body.clone(),
        )
        .map(|_| ())
    }
}

fn valid_customer_note_scope(scope: &Scope) -> bool {
    match scope {
        Scope::Platform => true,
        Scope::Holding { holding_id } => !holding_id.is_nil(),
        Scope::Company {
            holding_id,
            company_id,
        } => !holding_id.is_nil() && !company_id.is_nil(),
        Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => !holding_id.is_nil() && !company_id.is_nil() && !branch_id.is_nil(),
        Scope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => {
            !holding_id.is_nil()
                && (!company_ids.is_empty() || !branch_ids.is_empty())
                && company_ids.iter().all(|id| !id.is_nil())
                && branch_ids.iter().all(|id| !id.is_nil())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CustomerNotesReadDto {
    pub notes: Vec<CustomerNoteReadDto>,
}

pub const CUSTOMER_DETAIL_MAX_PAGE_SIZE: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct CustomerNoteCursor {
    pub created_at: DateTime<Utc>,
    pub note_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CustomerNotesPageReadDto {
    pub notes: Vec<CustomerNoteReadDto>,
    pub next_before: Option<CustomerNoteCursor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Customer360DetailSection {
    Organization,
    Contacts,
    Commercial,
    Entitlements,
    Onboarding,
    Health,
    Support,
    AccessAudit,
    Notes,
}

impl Customer360DetailSection {
    pub const ALL: [Self; 9] = [
        Self::Organization,
        Self::Contacts,
        Self::Commercial,
        Self::Entitlements,
        Self::Onboarding,
        Self::Health,
        Self::Support,
        Self::AccessAudit,
        Self::Notes,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerGlobalReadContext {
    pub actor: GlobalReadActor,
    pub module: String,
    pub purpose: String,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Customer360DetailReadContext {
    pub customer_id: Uuid,
    pub scope: Scope,
    pub global_read: CustomerGlobalReadContext,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CustomerDetailReadError {
    #[error("customer ID is required")]
    MissingCustomerId,
    #[error("customer scope is not bound to the requested customer")]
    ScopeNotBound,
    #[error("customer scope contains an invalid identifier")]
    InvalidScope,
    #[error("global-read actor is required")]
    MissingGlobalReadActor,
    #[error("global-read module is required")]
    MissingGlobalReadModule,
    #[error("global-read purpose is required")]
    MissingGlobalReadPurpose,
    #[error("global-read correlation ID is required")]
    MissingGlobalReadCorrelation,
    #[error("customer detail section belongs to a different customer")]
    SectionCustomerMismatch,
    #[error("customer detail page size must be greater than zero")]
    InvalidPageSize,
}

impl CustomerGlobalReadContext {
    pub fn validate(&self) -> Result<(), CustomerDetailReadError> {
        if self.actor.principal_ref.trim().is_empty() {
            return Err(CustomerDetailReadError::MissingGlobalReadActor);
        }
        if self.module.trim().is_empty() {
            return Err(CustomerDetailReadError::MissingGlobalReadModule);
        }
        if self.purpose.trim().is_empty() {
            return Err(CustomerDetailReadError::MissingGlobalReadPurpose);
        }
        if self.correlation_id.is_nil() {
            return Err(CustomerDetailReadError::MissingGlobalReadCorrelation);
        }
        Ok(())
    }
}

impl Customer360DetailReadContext {
    pub fn validate(&self) -> Result<(), CustomerDetailReadError> {
        if self.customer_id.is_nil() {
            return Err(CustomerDetailReadError::MissingCustomerId);
        }
        self.global_read.validate()
    }

    /// Check the requested read scope against only authoritative customer
    /// bindings. A platform read is intentionally explicit; every other
    /// scope must be covered by a binding, including every member of a
    /// subtree. An empty or malformed binding list fails closed.
    pub fn authorize_against(
        &self,
        bindings: &[CustomerScopeBindingProjection],
    ) -> Result<(), CustomerDetailReadError> {
        self.validate()?;
        if matches!(self.scope, Scope::Platform) {
            return Ok(());
        }
        let bindings: Vec<_> = bindings
            .iter()
            .filter(|binding| binding.customer_id == self.customer_id)
            .collect();
        let authorized = match &self.scope {
            Scope::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => {
                if company_ids.is_empty() && branch_ids.is_empty() {
                    false
                } else if branch_ids.is_empty()
                    && bindings.iter().any(|binding| {
                        binding
                            .as_scope()
                            .is_some_and(|scope| scope.contains(&self.scope))
                    })
                {
                    true
                } else {
                    company_ids.iter().all(|company_id| {
                        bindings.iter().any(|binding| {
                            binding.as_scope().is_some_and(|scope| {
                                scope.contains(&Scope::Company {
                                    holding_id: *holding_id,
                                    company_id: *company_id,
                                })
                            })
                        })
                    }) && branch_ids.iter().all(|branch_id| {
                        bindings.iter().any(|binding| {
                            binding.company_id.is_some()
                                && binding.branch_id.is_some()
                                && (company_ids.is_empty()
                                    || company_ids
                                        .contains(&binding.company_id.unwrap_or_default()))
                                && binding.as_scope().is_some_and(|scope| {
                                    scope.contains(&Scope::Branch {
                                        holding_id: *holding_id,
                                        company_id: binding.company_id.unwrap_or_default(),
                                        branch_id: *branch_id,
                                    })
                                })
                        })
                    })
                }
            }
            _ => bindings
                .iter()
                .any(|binding| binding.allows_scope(&self.scope)),
        };
        if authorized {
            Ok(())
        } else {
            Err(CustomerDetailReadError::ScopeNotBound)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Customer360DetailReadModel {
    pub account: CustomerReadView<CustomerAccountProjection>,
    pub organization: CustomerReadView<CustomerOrganizationReadDto>,
    pub contacts: CustomerReadView<CustomerContactsReadDto>,
    pub commercial: CustomerReadView<CustomerCommercialReadDto>,
    pub entitlements: CustomerReadView<CustomerEntitlementsReadDto>,
    pub onboarding: CustomerReadView<CustomerOnboardingReadDto>,
    pub health: CustomerReadView<CustomerHealthReadDto>,
    pub support: CustomerReadView<CustomerSupportSummaryReadDto>,
    pub access_audit: CustomerReadView<CustomerAccessAuditSummaryReadDto>,
    pub notes: CustomerReadView<CustomerNotesPageReadDto>,
}

impl Customer360DetailReadModel {
    pub fn from_projection(
        projection: &Customer360Projection,
        projection_metadata: &CustomerReadMetadata,
        access_audit: CustomerReadView<CustomerAccessAuditSummaryReadDto>,
        notes: CustomerReadView<CustomerNotesPageReadDto>,
        context: &Customer360DetailReadContext,
        bindings: &[CustomerScopeBindingProjection],
    ) -> Result<Self, CustomerDetailReadError> {
        context.authorize_against(bindings)?;
        Self::from_authorized_projection(
            projection,
            projection_metadata,
            access_audit,
            notes,
            context,
        )
    }

    /// Assemble a complete detail view after an authoritative repository has
    /// checked the customer binding. This keeps the aggregate reusable for a
    /// SQLx read pool without allowing a read payload to become an authority
    /// substitute.
    pub fn from_authorized_projection(
        projection: &Customer360Projection,
        projection_metadata: &CustomerReadMetadata,
        access_audit: CustomerReadView<CustomerAccessAuditSummaryReadDto>,
        notes: CustomerReadView<CustomerNotesPageReadDto>,
        context: &Customer360DetailReadContext,
    ) -> Result<Self, CustomerDetailReadError> {
        context.validate()?;
        if projection.account.id != context.customer_id
            || projection_metadata.customer_id != context.customer_id
        {
            return Err(CustomerDetailReadError::SectionCustomerMismatch);
        }
        let metadata = projection_metadata.clone();
        let model = Self {
            account: present_detail_section(
                projection.account.redacted_for_detail_read(),
                &metadata,
            ),
            organization: present_detail_section(
                CustomerOrganizationReadDto::from(projection),
                &metadata,
            ),
            contacts: present_detail_section(CustomerContactsReadDto::from(projection), &metadata),
            commercial: present_detail_section(
                CustomerCommercialReadDto::from(projection),
                &metadata,
            ),
            entitlements: present_detail_section(
                CustomerEntitlementsReadDto::from(projection),
                &metadata,
            ),
            onboarding: present_detail_section(
                CustomerOnboardingReadDto::from(projection),
                &metadata,
            ),
            health: present_detail_section(CustomerHealthReadDto::from(projection), &metadata),
            support: present_detail_section(
                CustomerSupportSummaryReadDto::from(projection),
                &metadata,
            ),
            access_audit: access_audit.map(|data| data.redacted_for_detail_read()),
            notes: notes.map(|data| data.redacted_for_detail_read()),
        };
        model.validate_customer_ids(context.customer_id)?;
        Ok(model)
    }

    pub fn validate_customer_ids(&self, customer_id: Uuid) -> Result<(), CustomerDetailReadError> {
        let metadata = [
            self.account.metadata(),
            self.organization.metadata(),
            self.contacts.metadata(),
            self.commercial.metadata(),
            self.entitlements.metadata(),
            self.onboarding.metadata(),
            self.health.metadata(),
            self.support.metadata(),
            self.access_audit.metadata(),
            self.notes.metadata(),
        ];
        if metadata
            .iter()
            .any(|metadata| metadata.customer_id != customer_id)
        {
            return Err(CustomerDetailReadError::SectionCustomerMismatch);
        }
        Ok(())
    }

    pub const fn section_count(&self) -> usize {
        Customer360DetailSection::ALL.len()
    }
}

impl<T> CustomerReadView<T> {
    pub fn metadata(&self) -> &CustomerReadMetadata {
        match self {
            Self::Present { metadata, .. }
            | Self::Absent { metadata }
            | Self::Unsupported { metadata, .. } => metadata,
        }
    }
}

fn present_detail_section<T>(data: T, metadata: &CustomerReadMetadata) -> CustomerReadView<T> {
    CustomerReadView::Present {
        data,
        metadata: metadata.clone(),
    }
}

impl CustomerScopeBindingProjection {
    fn allows_scope(&self, requested: &Scope) -> bool {
        let Some(bound_scope) = self.as_scope() else {
            return false;
        };
        match requested {
            Scope::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => {
                if branch_ids.is_empty() && bound_scope.contains(requested) {
                    return true;
                }
                if company_ids.is_empty() && branch_ids.is_empty() {
                    return false;
                }
                (company_ids.is_empty()
                    || company_ids.iter().all(|company_id| {
                        bound_scope.contains(&Scope::Company {
                            holding_id: *holding_id,
                            company_id: *company_id,
                        })
                    }))
                    && (branch_ids.is_empty()
                        || branch_ids.iter().all(|branch_id| {
                            self.company_id.is_some()
                                && self.branch_id.is_some()
                                && (company_ids.is_empty()
                                    || company_ids.contains(&self.company_id.unwrap_or_default()))
                                && bound_scope.contains(&Scope::Branch {
                                    holding_id: *holding_id,
                                    company_id: self.company_id.unwrap_or_default(),
                                    branch_id: *branch_id,
                                })
                        }))
            }
            _ => bound_scope.contains(requested),
        }
    }

    fn as_scope(&self) -> Option<Scope> {
        if self.customer_id.is_nil()
            || self.holding_id.is_nil()
            || self.company_id.is_some_and(|id| id.is_nil())
            || self.branch_id.is_some_and(|id| id.is_nil())
            || (self.branch_id.is_some() && self.company_id.is_none())
        {
            return None;
        }
        Some(match (self.company_id, self.branch_id) {
            (None, None) => Scope::Holding {
                holding_id: self.holding_id,
            },
            (Some(company_id), None) => Scope::Company {
                holding_id: self.holding_id,
                company_id,
            },
            (Some(company_id), Some(branch_id)) => Scope::Branch {
                holding_id: self.holding_id,
                company_id,
                branch_id,
            },
            (None, Some(_)) => return None,
        })
    }
}

impl CustomerNotesPageReadDto {
    pub fn from_items(
        mut notes: Vec<CustomerNoteReadDto>,
        before: Option<CustomerNoteCursor>,
        limit: u32,
    ) -> Result<Self, CustomerDetailReadError> {
        if limit == 0 {
            return Err(CustomerDetailReadError::InvalidPageSize);
        }
        let limit = limit.min(CUSTOMER_DETAIL_MAX_PAGE_SIZE) as usize;
        notes.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        if let Some(before) = before {
            notes.retain(|note| (note.created_at, note.id) < (before.created_at, before.note_id));
        }
        let has_more = notes.len() > limit;
        if has_more {
            notes.truncate(limit);
        }
        let next_before = has_more
            .then(|| {
                notes.last().map(|note| CustomerNoteCursor {
                    created_at: note.created_at,
                    note_id: note.id,
                })
            })
            .flatten();
        Ok(Self { notes, next_before })
    }
}

fn redact_secret_text(value: &str) -> String {
    const PREFIXES: [&str; 12] = [
        "Bearer ",
        "Basic ",
        "sk_live_",
        "rk_live_",
        "whsec_",
        "access_token=",
        "refresh_token=",
        "client_secret=",
        "api_key=",
        "apikey=",
        "password=",
        "token=",
    ];
    let mut redacted = value.to_owned();
    for prefix in PREFIXES {
        let mut search_from = 0;
        while let Some(relative_start) = redacted[search_from..].find(prefix) {
            let start = search_from + relative_start;
            let value_start = if prefix.ends_with('=') || prefix.ends_with(' ') {
                start + prefix.len()
            } else {
                start
            };
            let value_end = redacted[value_start..]
                .find(|character: char| character.is_whitespace() || ",;)]}".contains(character))
                .map(|offset| value_start + offset)
                .unwrap_or(redacted.len());
            let replacement = "[REDACTED]".to_owned();
            redacted.replace_range(start..value_end, &replacement);
            search_from = start + replacement.len();
        }
    }
    redacted
}

impl CustomerAccountProjection {
    pub fn redacted_for_detail_read(&self) -> Self {
        let mut redacted = self.clone();
        redacted.identity.trading_name = redact_secret_text(&redacted.identity.trading_name);
        redacted.identity.legal_name = redacted
            .identity
            .legal_name
            .as_deref()
            .map(redact_secret_text);
        redacted.identity.registration_number = redacted
            .identity
            .registration_number
            .as_deref()
            .map(redact_secret_text);
        redacted.identity.tax_identifier = redacted
            .identity
            .tax_identifier
            .as_deref()
            .map(redact_secret_text);
        redacted.industry = redacted.industry.as_deref().map(redact_secret_text);
        redacted.domains = redacted
            .domains
            .iter()
            .map(|domain| redact_secret_text(domain))
            .collect();
        redacted.responsibility_contacts = redacted
            .responsibility_contacts
            .iter()
            .map(|contact| contact.redacted_for_detail_read())
            .collect();
        redacted.customer_success_owner = redacted
            .customer_success_owner
            .as_ref()
            .map(CustomerSuccessOwner::redacted_for_detail_read);
        redacted
    }
}

impl ResponsibilityContact {
    fn redacted_for_detail_read(&self) -> Self {
        let mut redacted = self.clone();
        redacted.display_name = redact_secret_text(&redacted.display_name);
        redacted.email = redacted.email.as_deref().map(redact_secret_text);
        redacted.phone = redacted.phone.as_deref().map(redact_secret_text);
        redacted.responsibilities = redacted
            .responsibilities
            .iter()
            .map(|value| redact_secret_text(value))
            .collect();
        redacted
    }
}

impl CustomerSuccessOwner {
    fn redacted_for_detail_read(&self) -> Self {
        Self {
            principal_ref: redact_secret_text(&self.principal_ref),
            display_name: redact_secret_text(&self.display_name),
        }
    }
}

impl CustomerNoteReadDto {
    pub fn redacted_for_detail_read(&self) -> Self {
        Self {
            body: redact_secret_text(&self.body),
            ..self.clone()
        }
    }
}

impl CustomerNotesReadDto {
    pub fn redacted_for_detail_read(&self) -> Self {
        Self {
            notes: self
                .notes
                .iter()
                .map(CustomerNoteReadDto::redacted_for_detail_read)
                .collect(),
        }
    }
}

impl CustomerNotesPageReadDto {
    pub fn redacted_for_detail_read(&self) -> Self {
        Self {
            notes: self
                .notes
                .iter()
                .map(CustomerNoteReadDto::redacted_for_detail_read)
                .collect(),
            next_before: self.next_before,
        }
    }
}

impl CustomerAccessAuditSummaryReadDto {
    pub const fn redacted_for_detail_read(self) -> Self {
        self
    }
}

impl From<&Customer360Projection> for CustomerOrganizationReadDto {
    fn from(projection: &Customer360Projection) -> Self {
        Self {
            organization: projection.organization.clone(),
        }
    }
}

impl From<&Customer360Projection> for CustomerContactsReadDto {
    fn from(projection: &Customer360Projection) -> Self {
        Self {
            contacts: projection.account.responsibility_contacts.clone(),
            customer_success_owner: projection.account.customer_success_owner.clone(),
        }
    }
}

impl From<&Customer360Projection> for CustomerCommercialReadDto {
    fn from(projection: &Customer360Projection) -> Self {
        Self {
            commercial: projection.commercial.clone(),
        }
    }
}

impl From<&Customer360Projection> for CustomerEntitlementsReadDto {
    fn from(projection: &Customer360Projection) -> Self {
        Self {
            subscription_allocations: projection.commercial.module_allocations.clone(),
            organization_allocations: projection.organization.module_allocations.clone(),
            active_module_count: projection
                .organization
                .module_allocations
                .iter()
                .filter(|allocation| allocation.active)
                .count() as u32,
        }
    }
}

impl From<&Customer360Projection> for CustomerOnboardingReadDto {
    fn from(projection: &Customer360Projection) -> Self {
        Self {
            onboarding: projection.onboarding.clone(),
        }
    }
}

impl From<&Customer360Projection> for CustomerHealthReadDto {
    fn from(projection: &Customer360Projection) -> Self {
        Self {
            health: projection.health.clone(),
        }
    }
}

impl From<&Customer360Projection> for CustomerSupportSummaryReadDto {
    fn from(projection: &Customer360Projection) -> Self {
        Self {
            open_ticket_count: projection.organization.open_ticket_count,
        }
    }
}

impl Customer360Projection {
    /// Convert the legacy event shape into the enriched projection without
    /// fabricating organization identifiers. Counts remain available for
    /// legacy Customer 360 consumers, while scope bindings remain empty until
    /// an organization event supplies real IDs.
    pub fn from_legacy(customer: &Customer360) -> Self {
        let onboarding = match customer.account.status {
            CustomerStatus::Prospect => OnboardingState::NotStarted,
            CustomerStatus::Onboarding => OnboardingState::InProgress,
            CustomerStatus::Active => OnboardingState::Live,
            CustomerStatus::Suspended | CustomerStatus::Churned => OnboardingState::Blocked,
        };
        Self {
            account: CustomerAccountProjection {
                id: customer.account.id,
                account_code: customer.account.account_code.clone(),
                identity: TradingLegalIdentity {
                    trading_name: customer.account.trading_name.clone(),
                    legal_name: customer.account.legal_name.clone(),
                    ..Default::default()
                },
                industry: customer.account.industry.clone(),
                country: customer.account.country.clone(),
                account_state: customer.account.status,
                subscription_state: customer.account.billing_state,
                renewal_at: customer.account.renewal_at,
                lifetime_value_minor: customer.account.lifetime_value_minor,
                ..Default::default()
            },
            organization: CustomerOrganizationProjection {
                active_user_count: customer.active_user_count,
                expiring_role_count: customer.expiring_role_count,
                open_ticket_count: customer.open_ticket_count,
                holding_count: customer.holding_count,
                company_count: customer.company_count,
                branch_count: customer.branch_count,
                module_count: customer.module_count,
                ..Default::default()
            },
            commercial: CustomerCommercialProjection {
                payment_state: customer.account.billing_state,
                renewal_at: customer.account.renewal_at,
                lifetime_value_minor: customer.account.lifetime_value_minor,
                ..Default::default()
            },
            onboarding: CustomerOnboardingProjection {
                state: onboarding,
                ..Default::default()
            },
            health: CustomerHealthProjection {
                state: customer.health.state,
                projection_lag_seconds: customer.health.projection_lag_seconds,
                worker_failures: customer.health.worker_failures,
                backup_status: customer.health.backup_status.clone(),
                last_activity_at: customer.health.last_activity_at,
                ..Default::default()
            },
        }
    }

    /// Convert the enriched read projection back to the compatibility shape
    /// used by existing platform APIs and list/detail stores.
    pub fn to_legacy(&self) -> Customer360 {
        Customer360 {
            account: CustomerAccount {
                id: self.account.id,
                account_code: self.account.account_code.clone(),
                trading_name: self.account.identity.trading_name.clone(),
                legal_name: self.account.identity.legal_name.clone(),
                industry: self.account.industry.clone(),
                country: self.account.country.clone(),
                status: self.account.account_state,
                billing_state: self.account.subscription_state,
                renewal_at: self.account.renewal_at,
                lifetime_value_minor: self.account.lifetime_value_minor,
            },
            health: CustomerHealth {
                customer_account_id: self.account.id,
                state: self.health.state,
                projection_lag_seconds: self.health.projection_lag_seconds,
                worker_failures: self.health.worker_failures,
                backup_status: self.health.backup_status.clone(),
                last_activity_at: self.health.last_activity_at,
            },
            holding_count: self
                .organization
                .holding_count
                .max(self.organization.holdings.len() as u32),
            company_count: self
                .organization
                .company_count
                .max(self.organization.companies.len() as u32),
            branch_count: self
                .organization
                .branch_count
                .max(self.organization.branches.len() as u32),
            active_user_count: self.organization.active_user_count,
            expiring_role_count: self.organization.expiring_role_count,
            open_ticket_count: self.organization.open_ticket_count,
            module_count: self
                .organization
                .module_count
                .max(self.organization.module_allocations.len() as u32),
        }
    }

    /// Canonicalize unordered source collections before persistence. This
    /// makes replay and rebuild payloads byte-stable for the same source
    /// state, regardless of event collection order.
    pub fn canonicalize(&mut self) {
        self.account.domains.sort();
        self.account.domains.dedup();
        self.account.logos.sort_by(|left, right| {
            left.variant
                .cmp(&right.variant)
                .then_with(|| left.object_key.cmp(&right.object_key))
        });
        self.account.responsibility_contacts.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.display_name.cmp(&right.display_name))
                .then_with(|| left.email.cmp(&right.email))
                .then_with(|| left.phone.cmp(&right.phone))
                .then_with(|| left.responsibilities.cmp(&right.responsibilities))
        });
        self.account
            .responsibility_contacts
            .iter_mut()
            .for_each(|contact| {
                contact.responsibilities.sort();
                contact.responsibilities.dedup();
            });
        self.organization.holdings.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.parent_id.cmp(&right.parent_id))
                .then_with(|| left.kind.cmp(&right.kind))
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.active.cmp(&right.active))
        });
        self.organization.companies.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.parent_id.cmp(&right.parent_id))
                .then_with(|| left.kind.cmp(&right.kind))
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.active.cmp(&right.active))
        });
        self.organization.branches.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.parent_id.cmp(&right.parent_id))
                .then_with(|| left.kind.cmp(&right.kind))
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.active.cmp(&right.active))
        });
        self.organization.departments.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.parent_id.cmp(&right.parent_id))
                .then_with(|| left.kind.cmp(&right.kind))
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.active.cmp(&right.active))
        });
        self.organization.module_allocations.sort_by(|left, right| {
            left.module_key
                .cmp(&right.module_key)
                .then_with(|| left.purchased_quantity.cmp(&right.purchased_quantity))
                .then_with(|| left.allocated_quantity.cmp(&right.allocated_quantity))
                .then_with(|| left.active.cmp(&right.active))
        });
        self.commercial.invoices.sort_by(|left, right| {
            left.invoice_ref
                .cmp(&right.invoice_ref)
                .then_with(|| left.state.cmp(&right.state))
                .then_with(|| left.amount_minor.cmp(&right.amount_minor))
                .then_with(|| left.currency.cmp(&right.currency))
        });
        self.commercial.quotes.sort_by(|left, right| {
            left.quote_ref
                .cmp(&right.quote_ref)
                .then_with(|| left.state.cmp(&right.state))
        });
        self.commercial.discounts.sort();
        self.commercial.discounts.dedup();
        self.commercial.module_allocations.sort_by(|left, right| {
            left.module_key
                .cmp(&right.module_key)
                .then_with(|| left.purchased_quantity.cmp(&right.purchased_quantity))
                .then_with(|| left.allocated_quantity.cmp(&right.allocated_quantity))
                .then_with(|| left.active.cmp(&right.active))
        });
    }

    pub fn canonicalized(mut self) -> Self {
        self.canonicalize();
        self
    }

    /// Derive only active, structurally connected organization scopes. A
    /// company or branch with a missing/foreign parent is deliberately omitted
    /// rather than creating an over-broad authority binding.
    pub fn scope_bindings(&self) -> Vec<CustomerScopeBindingProjection> {
        if self.account.id.is_nil() {
            return Vec::new();
        }
        let holding_ids: std::collections::BTreeSet<Uuid> = self
            .organization
            .holdings
            .iter()
            .filter(|unit| unit.active && !unit.id.is_nil())
            .map(|unit| unit.id)
            .collect();
        let mut bindings: Vec<_> = holding_ids
            .iter()
            .copied()
            .map(|holding_id| CustomerScopeBindingProjection {
                customer_id: self.account.id,
                holding_id,
                company_id: None,
                branch_id: None,
            })
            .collect();
        let company_holding: std::collections::BTreeMap<Uuid, Uuid> = self
            .organization
            .companies
            .iter()
            .filter_map(|unit| {
                let holding_id = unit.parent_id?;
                (unit.active && holding_ids.contains(&holding_id) && !unit.id.is_nil())
                    .then_some((unit.id, holding_id))
            })
            .collect();
        bindings.extend(company_holding.iter().map(|(company_id, holding_id)| {
            CustomerScopeBindingProjection {
                customer_id: self.account.id,
                holding_id: *holding_id,
                company_id: Some(*company_id),
                branch_id: None,
            }
        }));
        bindings.extend(self.organization.branches.iter().filter_map(|unit| {
            let company_id = unit.parent_id?;
            let holding_id = *company_holding.get(&company_id)?;
            (unit.active && !unit.id.is_nil()).then_some(CustomerScopeBindingProjection {
                customer_id: self.account.id,
                holding_id,
                company_id: Some(company_id),
                branch_id: Some(unit.id),
            })
        }));
        bindings.sort_by_key(|binding| {
            (
                binding.holding_id,
                binding.company_id.unwrap_or(Uuid::nil()),
                binding.branch_id.unwrap_or(Uuid::nil()),
            )
        });
        bindings.dedup();
        bindings
    }

    /// Return only fields suitable for list views and search results. Tax and
    /// registration identifiers, billing addresses, domains, logos, contacts,
    /// payment-provider references, and financial totals are omitted.
    pub fn summary(&self) -> Customer360Summary {
        Customer360Summary {
            id: self.account.id,
            account_code: self.account.account_code.clone(),
            trading_name: self.account.identity.trading_name.clone(),
            country: self.account.country.clone(),
            account_state: self.account.account_state,
            subscription_state: self.account.subscription_state,
            holding_count: self
                .organization
                .holding_count
                .max(self.organization.holdings.len() as u32),
            company_count: self
                .organization
                .company_count
                .max(self.organization.companies.len() as u32),
            branch_count: self
                .organization
                .branch_count
                .max(self.organization.branches.len() as u32),
            active_user_count: self.organization.active_user_count,
            module_count: self
                .organization
                .module_count
                .max(self.organization.module_allocations.len() as u32),
            health_state: self.health.state,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalReadActor {
    pub principal_ref: String,
    pub session_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalReadScope {
    pub label: String,
    pub holding_id: Option<Uuid>,
    pub company_id: Option<Uuid>,
    pub branch_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalReadAuditRecord {
    pub actor: GlobalReadActor,
    pub customer_id: Uuid,
    pub module: String,
    pub scope: GlobalReadScope,
    pub purpose: String,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Uuid,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GlobalReadAuditError {
    #[error("global-read actor is required")]
    MissingActor,
    #[error("global-read customer ID is required")]
    MissingCustomerId,
    #[error("global-read module is required")]
    MissingModule,
    #[error("global-read scope is required")]
    MissingScope,
    #[error("global-read scope contains an invalid identifier")]
    InvalidScope,
    #[error("global-read purpose is required")]
    MissingPurpose,
    #[error("global-read correlation ID is required")]
    MissingCorrelation,
}

impl GlobalReadAuditRecord {
    pub fn new(
        actor: GlobalReadActor,
        customer_id: Uuid,
        module: impl Into<String>,
        scope: GlobalReadScope,
        purpose: impl Into<String>,
        occurred_at: DateTime<Utc>,
        correlation_id: Uuid,
    ) -> Result<Self, GlobalReadAuditError> {
        let record = Self {
            actor,
            customer_id,
            module: module.into(),
            scope,
            purpose: purpose.into(),
            occurred_at,
            correlation_id,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> Result<(), GlobalReadAuditError> {
        if self.actor.principal_ref.trim().is_empty() {
            return Err(GlobalReadAuditError::MissingActor);
        }
        if self.customer_id.is_nil() {
            return Err(GlobalReadAuditError::MissingCustomerId);
        }
        if self.module.trim().is_empty() {
            return Err(GlobalReadAuditError::MissingModule);
        }
        if self.scope.label.trim().is_empty() {
            return Err(GlobalReadAuditError::MissingScope);
        }
        if self.scope.holding_id.is_some_and(|id| id.is_nil())
            || self.scope.company_id.is_some_and(|id| id.is_nil())
            || self.scope.branch_id.is_some_and(|id| id.is_nil())
            || (self.scope.company_id.is_some() && self.scope.holding_id.is_none())
            || (self.scope.branch_id.is_some() && self.scope.company_id.is_none())
        {
            return Err(GlobalReadAuditError::InvalidScope);
        }
        if self.purpose.trim().is_empty() {
            return Err(GlobalReadAuditError::MissingPurpose);
        }
        if self.correlation_id.is_nil() {
            return Err(GlobalReadAuditError::MissingCorrelation);
        }
        Ok(())
    }
}

#[cfg(test)]
mod enriched_projection_tests {
    use super::*;
    use serde_json::Value;

    fn projection() -> Customer360Projection {
        Customer360Projection {
            account: CustomerAccountProjection {
                id: Uuid::new_v4(),
                account_code: "ACME-001".into(),
                identity: TradingLegalIdentity {
                    trading_name: "Acme".into(),
                    legal_name: Some("Acme Co., Ltd.".into()),
                    registration_number: Some("0105550000000".into()),
                    tax_identifier: Some("TAX-SECRET".into()),
                },
                industry: Some("manufacturing".into()),
                country: "TH".into(),
                locale: "th-TH".into(),
                billing_address: Some(BillingAddress {
                    line_1: "1 Private Road".into(),
                    line_2: None,
                    city: "Bangkok".into(),
                    region: None,
                    postal_code: "10110".into(),
                    country: "TH".into(),
                }),
                domains: vec!["acme.example".into()],
                logos: vec![LogoReference {
                    variant: "primary".into(),
                    object_key: "customers/acme/logo".into(),
                }],
                account_state: CustomerStatus::Active,
                subscription_state: super::super::commercial::BillingState::Active,
                renewal_at: None,
                lifetime_value_minor: 10_000,
                responsibility_contacts: vec![ResponsibilityContact {
                    id: Uuid::new_v4(),
                    display_name: "Security Head".into(),
                    email: Some("security@acme.example".into()),
                    phone: None,
                    responsibilities: vec!["security".into()],
                }],
                customer_success_owner: Some(CustomerSuccessOwner {
                    principal_ref: "csm-1".into(),
                    display_name: "Customer Success".into(),
                }),
            },
            organization: CustomerOrganizationProjection {
                holdings: vec![OrganizationUnit::default()],
                companies: vec![OrganizationUnit::default()],
                branches: vec![OrganizationUnit::default()],
                module_allocations: vec![ModuleAllocation {
                    module_key: "operations".into(),
                    purchased_quantity: 1,
                    allocated_quantity: 1,
                    active: true,
                }],
                ..Default::default()
            },
            health: CustomerHealthProjection {
                state: HealthState::Healthy,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn summaries_exclude_sensitive_account_and_commercial_fields() {
        let value = serde_json::to_value(projection().summary()).expect("summary JSON");
        let object = value.as_object().expect("summary object");
        for field in [
            "registration_number",
            "tax_identifier",
            "billing_address",
            "domains",
            "logos",
            "responsibility_contacts",
            "stripe_customer_ref",
            "stripe_subscription_ref",
            "lifetime_value_minor",
        ] {
            assert!(
                !object.contains_key(field),
                "sensitive field leaked: {field}"
            );
        }
        assert_eq!(
            object.get("trading_name"),
            Some(&Value::String("Acme".into()))
        );
    }

    #[test]
    fn global_read_audit_contains_every_required_attribution() {
        let customer_id = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();
        let record = GlobalReadAuditRecord::new(
            GlobalReadActor {
                principal_ref: "platform-agent".into(),
                session_ref: Some("session-1".into()),
            },
            customer_id,
            "customer_360",
            GlobalReadScope {
                label: "holding-and-descendants".into(),
                holding_id: Some(Uuid::new_v4()),
                company_id: None,
                branch_id: None,
            },
            "customer_support_review",
            Utc::now(),
            correlation_id,
        )
        .expect("complete audit record");

        assert_eq!(record.customer_id, customer_id);
        assert_eq!(record.correlation_id, correlation_id);
        assert!(record.validate().is_ok());
        let value = serde_json::to_value(record).expect("audit JSON");
        for field in [
            "actor",
            "customer_id",
            "module",
            "scope",
            "purpose",
            "occurred_at",
            "correlation_id",
        ] {
            assert!(value.get(field).is_some(), "missing audit field: {field}");
        }
    }

    #[test]
    fn global_read_audit_rejects_incomplete_attribution() {
        let result = GlobalReadAuditRecord::new(
            GlobalReadActor {
                principal_ref: " ".into(),
                session_ref: None,
            },
            Uuid::new_v4(),
            "customer_360",
            GlobalReadScope {
                label: "platform".into(),
                holding_id: None,
                company_id: None,
                branch_id: None,
            },
            "review",
            Utc::now(),
            Uuid::new_v4(),
        );
        assert_eq!(result, Err(GlobalReadAuditError::MissingActor));
    }

    #[test]
    fn global_read_audit_rejects_nil_identity_and_malformed_scope() {
        let actor = GlobalReadActor {
            principal_ref: "platform-agent".into(),
            session_ref: None,
        };
        let scope = GlobalReadScope {
            label: "company".into(),
            holding_id: Some(Uuid::from_u128(0x8010)),
            company_id: Some(Uuid::from_u128(0x8020)),
            branch_id: None,
        };
        let missing_customer = GlobalReadAuditRecord::new(
            actor.clone(),
            Uuid::nil(),
            "customer_360",
            scope.clone(),
            "review",
            Utc::now(),
            Uuid::from_u128(0x8030),
        );
        assert_eq!(
            missing_customer,
            Err(GlobalReadAuditError::MissingCustomerId)
        );

        let missing_correlation = GlobalReadAuditRecord::new(
            actor.clone(),
            Uuid::from_u128(0x8040),
            "customer_360",
            scope.clone(),
            "review",
            Utc::now(),
            Uuid::nil(),
        );
        assert_eq!(
            missing_correlation,
            Err(GlobalReadAuditError::MissingCorrelation)
        );

        let malformed_scope = GlobalReadAuditRecord::new(
            actor,
            Uuid::from_u128(0x8050),
            "customer_360",
            GlobalReadScope {
                label: "branch".into(),
                holding_id: None,
                company_id: None,
                branch_id: Some(Uuid::from_u128(0x8060)),
            },
            "review",
            Utc::now(),
            Uuid::from_u128(0x8070),
        );
        assert_eq!(malformed_scope, Err(GlobalReadAuditError::InvalidScope));
    }

    #[test]
    fn legacy_customer_conversion_is_deterministic_and_preserves_compatibility_counts() {
        let customer_id = Uuid::from_u128(0x3001);
        let customer = Customer360 {
            account: CustomerAccount {
                id: customer_id,
                account_code: "FIX-CUST".into(),
                trading_name: "Fixture Customer".into(),
                legal_name: Some("Fixture Customer Co., Ltd.".into()),
                industry: Some("services".into()),
                country: "TH".into(),
                status: CustomerStatus::Onboarding,
                billing_state: super::super::commercial::BillingState::Trialing,
                renewal_at: None,
                lifetime_value_minor: 125_000,
            },
            health: CustomerHealth {
                customer_account_id: customer_id,
                state: HealthState::Degraded,
                projection_lag_seconds: 4,
                worker_failures: 1,
                backup_status: "verified".into(),
                last_activity_at: None,
            },
            holding_count: 2,
            company_count: 3,
            branch_count: 4,
            active_user_count: 5,
            expiring_role_count: 1,
            open_ticket_count: 2,
            module_count: 3,
        };
        let projection = Customer360Projection::from_legacy(&customer);
        assert_eq!(projection.onboarding.state, OnboardingState::InProgress);
        assert_eq!(projection.scope_bindings(), Vec::new());
        assert_eq!(projection.to_legacy(), customer);
        assert_eq!(
            serde_json::to_vec(&projection).expect("projection JSON"),
            serde_json::to_vec(&projection.clone().canonicalized()).expect("canonical JSON")
        );
    }

    #[test]
    fn scope_bindings_are_sorted_structurally_connected_and_isolated() {
        let customer_id = Uuid::from_u128(0x3002);
        let holding_a = Uuid::from_u128(0x1001);
        let holding_b = Uuid::from_u128(0x1002);
        let company_a = Uuid::from_u128(0x2001);
        let company_b = Uuid::from_u128(0x2002);
        let branch_a = Uuid::from_u128(0x4001);
        let branch_b = Uuid::from_u128(0x4002);
        let mut projection = Customer360Projection {
            account: CustomerAccountProjection {
                id: customer_id,
                ..Default::default()
            },
            organization: CustomerOrganizationProjection {
                holdings: vec![
                    OrganizationUnit {
                        id: holding_b,
                        kind: "holding".into(),
                        active: true,
                        ..Default::default()
                    },
                    OrganizationUnit {
                        id: holding_a,
                        kind: "holding".into(),
                        active: true,
                        ..Default::default()
                    },
                ],
                companies: vec![
                    OrganizationUnit {
                        id: company_b,
                        kind: "company".into(),
                        parent_id: Some(holding_b),
                        active: true,
                        ..Default::default()
                    },
                    OrganizationUnit {
                        id: company_a,
                        kind: "company".into(),
                        parent_id: Some(holding_a),
                        active: true,
                        ..Default::default()
                    },
                ],
                branches: vec![
                    OrganizationUnit {
                        id: branch_b,
                        kind: "branch".into(),
                        parent_id: Some(company_b),
                        active: true,
                        ..Default::default()
                    },
                    OrganizationUnit {
                        id: branch_a,
                        kind: "branch".into(),
                        parent_id: Some(company_a),
                        active: true,
                        ..Default::default()
                    },
                    OrganizationUnit {
                        id: Uuid::from_u128(0x4003),
                        kind: "branch".into(),
                        parent_id: Some(company_a),
                        active: false,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            ..Default::default()
        };
        let first = projection.scope_bindings();
        projection.organization.holdings.reverse();
        projection.organization.companies.reverse();
        projection.organization.branches.reverse();
        let second = projection.scope_bindings();
        assert_eq!(first, second);
        assert_eq!(first.len(), 6);
        assert_eq!(first[0].holding_id, holding_a);
        assert_eq!(
            first[0].company_id, None,
            "holding binding precedes descendants"
        );
        assert!(
            first
                .iter()
                .all(|binding| binding.customer_id == customer_id)
        );
        assert!(
            first
                .iter()
                .all(|binding| binding.branch_id != Some(Uuid::from_u128(0x4003)))
        );
    }

    #[test]
    fn canonicalization_makes_unordered_customer_collections_byte_stable() {
        let mut left = projection();
        let mut right = left.clone();
        left.account.domains = vec!["z.example".into(), "a.example".into(), "a.example".into()];
        right.account.domains = vec!["a.example".into(), "z.example".into()];
        left.commercial.discounts = vec!["later".into(), "early".into(), "early".into()];
        right.commercial.discounts = vec!["early".into(), "later".into()];
        left.canonicalize();
        right.canonicalize();
        assert_eq!(left.account.domains, right.account.domains);
        assert_eq!(left.commercial.discounts, right.commercial.discounts);
        assert_eq!(
            serde_json::to_vec(&left).expect("left JSON"),
            serde_json::to_vec(&right).expect("right JSON")
        );
    }

    #[test]
    fn secondary_read_dtos_are_projection_derived_and_read_metadata_is_explicit() {
        let projection = projection();
        let contacts = CustomerContactsReadDto::from(&projection);
        assert_eq!(contacts.contacts.len(), 1);
        assert_eq!(
            contacts
                .customer_success_owner
                .as_ref()
                .map(|owner| owner.principal_ref.as_str()),
            Some("csm-1")
        );

        let entitlements = CustomerEntitlementsReadDto::from(&projection);
        assert_eq!(entitlements.organization_allocations.len(), 1);
        assert_eq!(entitlements.active_module_count, 1);

        let updated_at = Utc::now();
        let customer_id = projection.account.id;
        let fresh = CustomerReadMetadata::present(customer_id, updated_at, updated_at);
        assert_eq!(fresh.freshness, CustomerReadFreshness::Fresh);
        assert!(fresh.read_only);

        let stale = CustomerReadMetadata::present(
            customer_id,
            updated_at,
            updated_at + chrono::Duration::seconds(CUSTOMER_READ_STALE_AFTER_SECONDS + 1),
        );
        assert_eq!(stale.freshness, CustomerReadFreshness::Stale);

        let exact_boundary = CustomerReadMetadata::present(
            customer_id,
            updated_at,
            updated_at + chrono::Duration::seconds(CUSTOMER_READ_STALE_AFTER_SECONDS),
        );
        assert_eq!(exact_boundary.freshness, CustomerReadFreshness::Fresh);

        let future = CustomerReadMetadata::present(
            customer_id,
            updated_at + chrono::Duration::seconds(1),
            updated_at,
        );
        assert_eq!(future.freshness, CustomerReadFreshness::Stale);

        let absent = CustomerReadMetadata::absent(customer_id);
        assert_eq!(absent.freshness, CustomerReadFreshness::Unknown);
        assert!(absent.read_only);
    }

    #[test]
    fn unsupported_read_view_serializes_only_metadata_and_reason() {
        let view: CustomerReadView<CustomerNotesReadDto> = CustomerReadView::Unsupported {
            metadata: CustomerReadMetadata::absent(Uuid::from_u128(0x4444)),
            reason: "notes read model is not available".into(),
        };
        let value = serde_json::to_value(view).expect("unsupported view JSON");
        let unsupported = value
            .get("Unsupported")
            .and_then(Value::as_object)
            .expect("unsupported envelope");
        assert!(unsupported.get("metadata").is_some());
        assert_eq!(
            unsupported.get("reason"),
            Some(&Value::String("notes read model is not available".into()))
        );
        assert!(unsupported.get("notes").is_none());
    }

    #[test]
    fn access_audit_summary_is_customer_visible_without_actor_scope_disclosure() {
        let summary = CustomerAccessAuditSummaryReadDto {
            read_count: 3,
            last_access_at: Some(Utc::now()),
            customer_visible: true,
        };
        let value = serde_json::to_value(summary).expect("access audit summary JSON");
        assert_eq!(value.get("customer_visible"), Some(&Value::Bool(true)));
        assert!(value.get("read_count").is_some());
        for forbidden in ["actor_ref", "scope", "purpose", "correlation_id", "payload"] {
            assert!(
                !value
                    .as_object()
                    .expect("summary object")
                    .contains_key(forbidden)
            );
        }

        let notes = CustomerNotesReadDto {
            notes: vec![CustomerNoteReadDto {
                id: Uuid::from_u128(0x5005),
                author_ref: "platform-agent".into(),
                created_at: Utc::now(),
                body: "approved operational note".into(),
            }],
        };
        assert_eq!(notes.notes.len(), 1);
        assert_eq!(notes.notes[0].author_ref, "platform-agent");
    }

    #[test]
    fn customer_detail_context_fails_closed_without_customer_scope_or_audit_context() {
        let customer_id = Uuid::from_u128(0x7001);
        let holding_id = Uuid::from_u128(0x7101);
        let company_id = Uuid::from_u128(0x7201);
        let context = Customer360DetailReadContext {
            customer_id,
            scope: Scope::Company {
                holding_id,
                company_id,
            },
            global_read: CustomerGlobalReadContext {
                actor: GlobalReadActor {
                    principal_ref: "security-admin".into(),
                    session_ref: Some("session-1".into()),
                },
                module: "customer_360".into(),
                purpose: "security-review".into(),
                correlation_id: Uuid::from_u128(0x7301),
            },
        };
        let binding = CustomerScopeBindingProjection {
            customer_id,
            holding_id,
            company_id: Some(company_id),
            branch_id: None,
        };
        assert!(
            context
                .authorize_against(std::slice::from_ref(&binding))
                .is_ok()
        );
        assert_eq!(
            context.authorize_against(&[]),
            Err(CustomerDetailReadError::ScopeNotBound)
        );

        let mut invalid = context.clone();
        invalid.global_read.actor.principal_ref.clear();
        assert_eq!(
            invalid.authorize_against(std::slice::from_ref(&binding)),
            Err(CustomerDetailReadError::MissingGlobalReadActor)
        );
        invalid.global_read.actor.principal_ref = "security-admin".into();
        invalid.customer_id = Uuid::from_u128(0x7002);
        assert_eq!(
            invalid.authorize_against(std::slice::from_ref(&binding)),
            Err(CustomerDetailReadError::ScopeNotBound)
        );
    }

    #[test]
    fn customer_subtree_read_cannot_mix_a_company_with_a_foreign_branch() {
        let customer_id = Uuid::from_u128(0x7a01);
        let holding_id = Uuid::from_u128(0x7a02);
        let requested_company = Uuid::from_u128(0x7a03);
        let foreign_company = Uuid::from_u128(0x7a04);
        let foreign_branch = Uuid::from_u128(0x7a05);
        let context = Customer360DetailReadContext {
            customer_id,
            scope: Scope::Subtree {
                holding_id,
                company_ids: vec![requested_company],
                branch_ids: vec![foreign_branch],
            },
            global_read: CustomerGlobalReadContext {
                actor: GlobalReadActor {
                    principal_ref: "security-admin".into(),
                    session_ref: None,
                },
                module: "customer_360".into(),
                purpose: "security-review".into(),
                correlation_id: Uuid::from_u128(0x7a06),
            },
        };
        let bindings = [
            CustomerScopeBindingProjection {
                customer_id,
                holding_id,
                company_id: Some(requested_company),
                branch_id: None,
            },
            CustomerScopeBindingProjection {
                customer_id,
                holding_id,
                company_id: Some(foreign_company),
                branch_id: Some(foreign_branch),
            },
        ];

        assert_eq!(
            context.authorize_against(&bindings),
            Err(CustomerDetailReadError::ScopeNotBound)
        );
    }

    #[test]
    fn customer_detail_model_contains_all_sections_and_preserves_missing_states() {
        let mut projection = projection();
        let customer_id = projection.account.id;
        let now = Utc::now();
        let context = Customer360DetailReadContext {
            customer_id,
            scope: Scope::Platform,
            global_read: CustomerGlobalReadContext {
                actor: GlobalReadActor {
                    principal_ref: "platform-root".into(),
                    session_ref: None,
                },
                module: "customer_360".into(),
                purpose: "onboarding-review".into(),
                correlation_id: Uuid::from_u128(0x7401),
            },
        };
        projection.account.identity.legal_name = Some("Acme Co.".into());
        let metadata = CustomerReadMetadata::present(customer_id, now, now);
        let model = Customer360DetailReadModel::from_projection(
            &projection,
            &metadata,
            CustomerReadView::Unsupported {
                metadata: metadata.clone(),
                reason: "audit summary has not been projected".into(),
            },
            CustomerReadView::Absent {
                metadata: metadata.clone(),
            },
            &context,
            &[],
        )
        .expect("platform scope may assemble the detail model");
        assert_eq!(model.section_count(), 9);
        assert_eq!(Customer360DetailSection::ALL.len(), 9);
        assert!(matches!(
            model.access_audit,
            CustomerReadView::Unsupported { .. }
        ));
        assert!(matches!(model.notes, CustomerReadView::Absent { .. }));
        model
            .validate_customer_ids(customer_id)
            .expect("all section metadata remains customer-bound");
    }

    #[test]
    fn notes_paging_is_stable_for_equal_timestamps_and_rejects_zero_limit() {
        let timestamp = Utc::now();
        let notes: Vec<CustomerNoteReadDto> = (1..=3)
            .map(|id| CustomerNoteReadDto {
                id: Uuid::from_u128(id),
                author_ref: "agent".into(),
                created_at: timestamp,
                body: format!("note-{id}"),
            })
            .collect();
        assert_eq!(
            CustomerNotesPageReadDto::from_items(notes.clone(), None, 0),
            Err(CustomerDetailReadError::InvalidPageSize)
        );
        let first =
            CustomerNotesPageReadDto::from_items(notes, None, 2).expect("first deterministic page");
        assert_eq!(
            first.notes.iter().map(|note| note.id).collect::<Vec<_>>(),
            vec![Uuid::from_u128(3), Uuid::from_u128(2)]
        );
        let cursor = first.next_before.expect("next page cursor");
        let second = CustomerNotesPageReadDto::from_items(first.notes.clone(), Some(cursor), 2)
            .expect("second deterministic page");
        assert!(second.notes.is_empty());

        let all_notes = vec![
            CustomerNoteReadDto {
                id: Uuid::from_u128(1),
                author_ref: "agent".into(),
                created_at: timestamp,
                body: "note-1".into(),
            },
            CustomerNoteReadDto {
                id: Uuid::from_u128(2),
                author_ref: "agent".into(),
                created_at: timestamp,
                body: "note-2".into(),
            },
            CustomerNoteReadDto {
                id: Uuid::from_u128(3),
                author_ref: "agent".into(),
                created_at: timestamp,
                body: "note-3".into(),
            },
        ];
        let first = CustomerNotesPageReadDto::from_items(all_notes.clone(), None, 2)
            .expect("first page from complete source");
        let second = CustomerNotesPageReadDto::from_items(all_notes, first.next_before, 2)
            .expect("second page from complete source");
        assert_eq!(second.notes.len(), 1);
        assert_eq!(second.notes[0].id, Uuid::from_u128(1));
    }

    #[test]
    fn detail_redaction_removes_common_credentials_from_free_text() {
        let mut projection = projection();
        projection.account.identity.legal_name = Some("Bearer secret-value".into());
        let notes = CustomerNotesPageReadDto {
            notes: vec![CustomerNoteReadDto {
                id: Uuid::from_u128(0x7501),
                author_ref: "agent".into(),
                created_at: Utc::now(),
                body: "token=top-secret sk_live_sensitive-value".into(),
            }],
            next_before: None,
        };
        let redacted = notes.redacted_for_detail_read();
        assert!(!redacted.notes[0].body.contains("top-secret"));
        assert!(!redacted.notes[0].body.contains("sk_live_sensitive-value"));
        assert_eq!(redacted.notes[0].body, "[REDACTED] [REDACTED]");
        let account = projection.account.redacted_for_detail_read();
        assert_eq!(account.identity.legal_name.as_deref(), Some("[REDACTED]"));
    }

    #[test]
    fn persisted_customer_notes_are_revalidated_before_projection() {
        let mut note = CustomerNote::new(
            Uuid::from_u128(0x7601),
            Uuid::from_u128(0x7602),
            "customer-success",
            Scope::Platform,
            Utc::now(),
            "safe note",
        )
        .expect("valid note");
        assert!(note.validate().is_ok());

        note.scope = Scope::Holding {
            holding_id: Uuid::nil(),
        };
        assert_eq!(note.validate(), Err(CustomerNoteError::InvalidScope));
    }
}
