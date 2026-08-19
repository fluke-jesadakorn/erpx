//! Cross-cutting holding-workspace and platform-administration rules.
//!
//! These types are deliberately transport and persistence neutral. An entered
//! platform administrator remains the real platform principal; the
//! administrative context is additional, short-lived authorization evidence,
//! never an impersonated holding identity.

use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    commercial::{BillingState, ModuleEntitlement},
    contracts::AuthorityScope,
    data_plane::BusinessUnitId,
};

pub const PLATFORM_HOLDING_ADMINISTRATOR_ROLE: &str = "platform_holding_administrator";
pub const PLATFORM_HOLDING_ENTER_PERMISSION: &str = "platform.holding.enter";
pub const HOLDING_ADMIN_CONTEXT_TTL: Duration = Duration::minutes(30);
pub const MAX_HOLDING_ADMIN_REASON_BYTES: usize = 512;

/// Canonical business-unit allow-list accepted by entered-workspace routes and
/// administrative contexts. Support remains a capability, not a business unit.
pub type HoldingWorkspaceModule = BusinessUnitId;

/// Immutable one-customer/one-holding/one-root-holding association.
///
/// Persistence additionally enforces uniqueness for all three identifiers.
/// Rows that cannot be proven exactly are intentionally not represented by
/// this type and remain `configuration_required` in the holding registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldingRegistryBinding {
    pub customer_id: Uuid,
    pub holding_id: String,
    pub canonical_holding_id: Uuid,
    pub data_profile_key: String,
}

impl HoldingRegistryBinding {
    pub fn new(
        customer_id: Uuid,
        holding_id: impl Into<String>,
        canonical_holding_id: Uuid,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, HoldingAdministrationError> {
        let binding = Self {
            customer_id,
            holding_id: holding_id.into(),
            canonical_holding_id,
            data_profile_key: data_profile_key.into(),
        };
        binding.validate()?;
        Ok(binding)
    }

    pub fn validate(&self) -> Result<(), HoldingAdministrationError> {
        if self.customer_id.is_nil() || self.canonical_holding_id.is_nil() {
            return Err(HoldingAdministrationError::NilIdentifier);
        }
        validate_bounded_reference(&self.holding_id, 128)?;
        validate_bounded_reference(&self.data_profile_key, 255)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HoldingOrganizationScope {
    Holding {
        holding_id: Uuid,
    },
    Company {
        holding_id: Uuid,
        company_id: Uuid,
    },
    Branch {
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Uuid,
    },
}

impl HoldingOrganizationScope {
    pub const fn holding_id(&self) -> Uuid {
        match self {
            Self::Holding { holding_id }
            | Self::Company { holding_id, .. }
            | Self::Branch { holding_id, .. } => *holding_id,
        }
    }

    pub fn as_access_scope(&self) -> AuthorityScope {
        match self {
            Self::Holding { holding_id } => AuthorityScope::Holding {
                holding_id: *holding_id,
            },
            Self::Company {
                holding_id,
                company_id,
            } => AuthorityScope::Company {
                holding_id: *holding_id,
                company_id: *company_id,
            },
            Self::Branch {
                holding_id,
                company_id,
                branch_id,
            } => AuthorityScope::Branch {
                holding_id: *holding_id,
                company_id: *company_id,
                branch_id: *branch_id,
            },
        }
    }

    fn validate(&self) -> Result<(), HoldingAdministrationError> {
        let ids = match self {
            Self::Holding { holding_id } => vec![*holding_id],
            Self::Company {
                holding_id,
                company_id,
            } => vec![*holding_id, *company_id],
            Self::Branch {
                holding_id,
                company_id,
                branch_id,
            } => vec![*holding_id, *company_id, *branch_id],
        };
        if ids.into_iter().any(|id| id.is_nil()) {
            Err(HoldingAdministrationError::NilIdentifier)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HoldingWorkspaceViewerKind {
    HoldingUser,
    PlatformHoldingAdministrator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HoldingWorkspaceBillingMode {
    Writable,
    Grace { until: DateTime<Utc> },
    ReadOnly,
}

impl HoldingWorkspaceBillingMode {
    pub const fn permits_business_mutation(self) -> bool {
        matches!(self, Self::Writable | Self::Grace { .. })
    }

    pub fn from_billing_state(state: BillingState, grace_until: Option<DateTime<Utc>>) -> Self {
        match state {
            BillingState::ReadOnly | BillingState::Canceled => Self::ReadOnly,
            BillingState::PastDue | BillingState::Grace => {
                grace_until.map_or(Self::ReadOnly, |until| Self::Grace { until })
            }
            BillingState::Trialing | BillingState::Active => Self::Writable,
        }
    }
}

/// Shared context consumed by every holding module surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldingWorkspaceContext {
    pub holding_id: String,
    pub customer_id: Uuid,
    pub canonical_holding_id: Uuid,
    pub scope: HoldingOrganizationScope,
    pub viewer_kind: HoldingWorkspaceViewerKind,
    pub billing_mode: HoldingWorkspaceBillingMode,
    pub entitlements: BTreeSet<HoldingWorkspaceModule>,
    /// Present only for an entered platform administrator. It is opaque and
    /// must be redacted from logs and UI diagnostics.
    pub administrative_context_id: Option<Uuid>,
}

impl HoldingWorkspaceContext {
    pub fn validate(&self) -> Result<(), HoldingAdministrationError> {
        validate_bounded_reference(&self.holding_id, 128)?;
        if self.customer_id.is_nil() || self.canonical_holding_id.is_nil() {
            return Err(HoldingAdministrationError::NilIdentifier);
        }
        self.scope.validate()?;
        if self.scope.holding_id() != self.canonical_holding_id {
            return Err(HoldingAdministrationError::ScopeOutsideHolding);
        }
        match (self.viewer_kind, self.administrative_context_id) {
            (HoldingWorkspaceViewerKind::HoldingUser, None)
            | (HoldingWorkspaceViewerKind::PlatformHoldingAdministrator, Some(_)) => Ok(()),
            _ => Err(HoldingAdministrationError::AdministrativeContextRequired),
        }
    }

    pub fn permits_module(&self, module: HoldingWorkspaceModule) -> bool {
        self.entitlements.contains(&module)
    }

    pub fn permits_business_mutation(&self, module: HoldingWorkspaceModule) -> bool {
        self.permits_module(module) && self.billing_mode.permits_business_mutation()
    }
}

/// Server-side evidence created only after a fresh MFA ceremony and an active
/// `platform.holding.enter` role check.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldingAdminContext {
    pub id: Uuid,
    pub principal_ref: String,
    pub device_id: Uuid,
    pub session_id: Uuid,
    pub holding_id: String,
    pub customer_id: Uuid,
    pub canonical_holding_id: Uuid,
    pub reason: String,
    pub correlation_id: Uuid,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl std::fmt::Debug for HoldingAdminContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HoldingAdminContext")
            .field("id", &"<redacted>")
            .field("principal_ref", &self.principal_ref)
            .field("device_id", &self.device_id)
            .field("session_id", &self.session_id)
            .field("holding_id", &self.holding_id)
            .field("customer_id", &self.customer_id)
            .field("canonical_holding_id", &self.canonical_holding_id)
            .field("reason", &self.reason)
            .field("correlation_id", &self.correlation_id)
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .field("revoked_at", &self.revoked_at)
            .finish()
    }
}

impl HoldingAdminContext {
    pub fn issue(
        binding: &HoldingRegistryBinding,
        principal_ref: impl Into<String>,
        device_id: Uuid,
        session_id: Uuid,
        reason: impl Into<String>,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Self, HoldingAdministrationError> {
        binding.validate()?;
        let context = Self {
            id: Uuid::new_v4(),
            principal_ref: principal_ref.into(),
            device_id,
            session_id,
            holding_id: binding.holding_id.clone(),
            customer_id: binding.customer_id,
            canonical_holding_id: binding.canonical_holding_id,
            reason: reason.into(),
            correlation_id,
            issued_at: at,
            expires_at: at
                .checked_add_signed(HOLDING_ADMIN_CONTEXT_TTL)
                .ok_or(HoldingAdministrationError::TimestampOverflow)?,
            revoked_at: None,
        };
        context.validate_shape()?;
        Ok(context)
    }

    pub fn validate_shape(&self) -> Result<(), HoldingAdministrationError> {
        if self.id.is_nil()
            || self.device_id.is_nil()
            || self.session_id.is_nil()
            || self.customer_id.is_nil()
            || self.canonical_holding_id.is_nil()
            || self.correlation_id.is_nil()
        {
            return Err(HoldingAdministrationError::NilIdentifier);
        }
        validate_bounded_reference(&self.principal_ref, 255)?;
        validate_bounded_reference(&self.holding_id, 128)?;
        validate_bounded_reference(&self.reason, MAX_HOLDING_ADMIN_REASON_BYTES)?;
        if self.expires_at != self.issued_at + HOLDING_ADMIN_CONTEXT_TTL {
            return Err(HoldingAdministrationError::InvalidExpiry);
        }
        Ok(())
    }

    pub fn authorize(
        &self,
        request: &HoldingAdminAuthorizationRequest<'_>,
    ) -> Result<(), HoldingAdministrationError> {
        self.validate_shape()?;
        if self.revoked_at.is_some() {
            return Err(HoldingAdministrationError::Revoked);
        }
        if request.at < self.issued_at || request.at >= self.expires_at {
            return Err(HoldingAdministrationError::Expired);
        }
        if self.principal_ref != request.principal_ref {
            return Err(HoldingAdministrationError::PrincipalMismatch);
        }
        if self.device_id != request.device_id {
            return Err(HoldingAdministrationError::DeviceMismatch);
        }
        if self.session_id != request.session_id {
            return Err(HoldingAdministrationError::SessionMismatch);
        }
        if self.holding_id != request.holding_id {
            return Err(HoldingAdministrationError::HoldingMismatch);
        }
        if !request.identity_active || !request.session_active || !request.directory_allows {
            return Err(HoldingAdministrationError::IdentityOrSessionInactive);
        }
        if !request.platform_role_active {
            return Err(HoldingAdministrationError::RoleInactive);
        }
        if request.requested_scope.holding_id() != self.canonical_holding_id {
            return Err(HoldingAdministrationError::ScopeOutsideHolding);
        }
        if let Some(module) = request.module
            && !request
                .entitlements
                .iter()
                .any(|entitlement| entitlement.module_key == module.as_str() && entitlement.active)
        {
            return Err(HoldingAdministrationError::ModuleNotEntitled);
        }
        Ok(())
    }
}

pub struct HoldingAdminAuthorizationRequest<'a> {
    pub principal_ref: &'a str,
    pub device_id: Uuid,
    pub session_id: Uuid,
    pub holding_id: &'a str,
    pub requested_scope: &'a HoldingOrganizationScope,
    pub module: Option<HoldingWorkspaceModule>,
    pub entitlements: &'a [ModuleEntitlement],
    pub identity_active: bool,
    pub session_active: bool,
    /// Directory deny-wins is evaluated before this value reaches the domain.
    pub directory_allows: bool,
    pub platform_role_active: bool,
    pub at: DateTime<Utc>,
}

impl std::fmt::Debug for HoldingAdminAuthorizationRequest<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HoldingAdminAuthorizationRequest")
            .field("principal_ref", &self.principal_ref)
            .field("device_id", &self.device_id)
            .field("session_id", &self.session_id)
            .field("holding_id", &self.holding_id)
            .field("requested_scope", &self.requested_scope)
            .field("module", &self.module)
            .field("identity_active", &self.identity_active)
            .field("session_active", &self.session_active)
            .field("directory_allows", &self.directory_allows)
            .field("platform_role_active", &self.platform_role_active)
            .field("at", &self.at)
            .finish()
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HoldingAdministrationError {
    #[error("holding administration identifier cannot be nil")]
    NilIdentifier,
    #[error("holding administration reference is missing or invalid")]
    InvalidReference,
    #[error("holding administration timestamp overflow")]
    TimestampOverflow,
    #[error("administrative context must have the fixed 30-minute expiry")]
    InvalidExpiry,
    #[error("administrative context is required for a platform viewer")]
    AdministrativeContextRequired,
    #[error("administrative context was revoked")]
    Revoked,
    #[error("administrative context is expired or not yet active")]
    Expired,
    #[error("administrative context principal does not match")]
    PrincipalMismatch,
    #[error("administrative context device does not match")]
    DeviceMismatch,
    #[error("administrative context session does not match")]
    SessionMismatch,
    #[error("administrative context holding does not match")]
    HoldingMismatch,
    #[error("identity, session, or directory authorization is inactive")]
    IdentityOrSessionInactive,
    #[error("platform holding administrator role is inactive")]
    RoleInactive,
    #[error("requested organization scope is outside the target holding")]
    ScopeOutsideHolding,
    #[error("requested module is not entitled")]
    ModuleNotEntitled,
    #[error("workspace module is unsupported")]
    UnsupportedModule,
}

fn validate_bounded_reference(
    value: &str,
    maximum: usize,
) -> Result<(), HoldingAdministrationError> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        Err(HoldingAdministrationError::InvalidReference)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (HoldingRegistryBinding, DateTime<Utc>, Uuid, Uuid) {
        let at = DateTime::parse_from_rfc3339("2026-08-01T00:00:00Z")
            .expect("time")
            .with_timezone(&Utc);
        (
            HoldingRegistryBinding::new(Uuid::new_v4(), "holding-a", Uuid::new_v4(), "holding-a")
                .expect("binding"),
            at,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
    }

    #[test]
    fn admin_context_is_exactly_thirty_minutes_and_debug_redacts_id() {
        let (binding, at, device_id, session_id) = fixture();
        let context = HoldingAdminContext::issue(
            &binding,
            "platform-admin",
            device_id,
            session_id,
            "assist HQ with finance close",
            Uuid::new_v4(),
            at,
        )
        .expect("context");
        assert_eq!(context.expires_at, at + Duration::minutes(30));
        let debug = format!("{context:?}");
        assert!(!debug.contains(&context.id.to_string()));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn context_revalidates_actor_device_session_holding_scope_role_and_entitlement() {
        let (binding, at, device_id, session_id) = fixture();
        let context = HoldingAdminContext::issue(
            &binding,
            "platform-admin",
            device_id,
            session_id,
            "investigate invoice-posting failure",
            Uuid::new_v4(),
            at,
        )
        .expect("context");
        let scope = HoldingOrganizationScope::Holding {
            holding_id: binding.canonical_holding_id,
        };
        let entitlements = vec![ModuleEntitlement {
            module_key: "finance".into(),
            active: true,
            seats: 5,
            company_limit: 2,
            branch_limit: 4,
            active_until: None,
        }];
        let mut request = HoldingAdminAuthorizationRequest {
            principal_ref: "platform-admin",
            device_id,
            session_id,
            holding_id: "holding-a",
            requested_scope: &scope,
            module: Some(HoldingWorkspaceModule::Finance),
            entitlements: &entitlements,
            identity_active: true,
            session_active: true,
            directory_allows: true,
            platform_role_active: true,
            at: at + Duration::minutes(29),
        };
        assert_eq!(context.authorize(&request), Ok(()));
        request.device_id = Uuid::new_v4();
        assert_eq!(
            context.authorize(&request),
            Err(HoldingAdministrationError::DeviceMismatch)
        );
        request.device_id = device_id;
        request.at = at + Duration::minutes(30);
        assert_eq!(
            context.authorize(&request),
            Err(HoldingAdministrationError::Expired)
        );
    }

    #[test]
    fn conflicting_identifiers_never_cross_holding_contexts() {
        let (binding, at, device_id, session_id) = fixture();
        let context = HoldingAdminContext::issue(
            &binding,
            "platform-admin",
            device_id,
            session_id,
            "compare isolated records",
            Uuid::new_v4(),
            at,
        )
        .expect("context");
        let scope = HoldingOrganizationScope::Holding {
            holding_id: binding.canonical_holding_id,
        };
        let request = HoldingAdminAuthorizationRequest {
            principal_ref: "platform-admin",
            device_id,
            session_id,
            holding_id: "holding-b",
            requested_scope: &scope,
            module: None,
            entitlements: &[],
            identity_active: true,
            session_active: true,
            directory_allows: true,
            platform_role_active: true,
            at: at + Duration::minutes(1),
        };
        assert_eq!(
            context.authorize(&request),
            Err(HoldingAdministrationError::HoldingMismatch)
        );
    }

    #[test]
    fn platform_workspace_requires_admin_context_and_entitled_module() {
        let (binding, _, _, _) = fixture();
        let context = HoldingWorkspaceContext {
            holding_id: binding.holding_id,
            customer_id: binding.customer_id,
            canonical_holding_id: binding.canonical_holding_id,
            scope: HoldingOrganizationScope::Holding {
                holding_id: binding.canonical_holding_id,
            },
            viewer_kind: HoldingWorkspaceViewerKind::PlatformHoldingAdministrator,
            billing_mode: HoldingWorkspaceBillingMode::ReadOnly,
            entitlements: BTreeSet::from([HoldingWorkspaceModule::Finance]),
            administrative_context_id: Some(Uuid::new_v4()),
        };
        assert_eq!(context.validate(), Ok(()));
        assert!(context.permits_module(HoldingWorkspaceModule::Finance));
        assert!(!context.permits_business_mutation(HoldingWorkspaceModule::Finance));
        assert!(!context.permits_module(HoldingWorkspaceModule::Sales));
    }
}
