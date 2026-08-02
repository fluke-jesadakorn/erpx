//! Cross-cutting tenant-workspace and platform-administration rules.
//!
//! These types are deliberately transport and persistence neutral. An entered
//! platform administrator remains the real platform principal; the
//! administrative context is additional, short-lived authorization evidence,
//! never an impersonated tenant identity.

use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    access::Scope,
    commercial::{BillingState, ModuleEntitlement},
    data_plane::BusinessUnitId,
};

pub const PLATFORM_TENANT_ADMINISTRATOR_ROLE: &str = "platform_tenant_administrator";
pub const PLATFORM_TENANT_ENTER_PERMISSION: &str = "platform.tenant.enter";
pub const TENANT_ADMIN_CONTEXT_TTL: Duration = Duration::minutes(30);
pub const MAX_TENANT_ADMIN_REASON_BYTES: usize = 512;

/// Canonical business-unit allow-list accepted by entered-workspace routes and
/// administrative contexts. Support remains a capability, not a business unit.
pub type TenantWorkspaceModule = BusinessUnitId;

/// Immutable one-customer/one-tenant/one-root-holding association.
///
/// Persistence additionally enforces uniqueness for all three identifiers.
/// Rows that cannot be proven exactly are intentionally not represented by
/// this type and remain `configuration_required` in the tenant registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerTenantBinding {
    pub customer_id: Uuid,
    pub tenant_id: String,
    pub root_holding_id: Uuid,
    pub holding_ref: String,
}

impl CustomerTenantBinding {
    pub fn new(
        customer_id: Uuid,
        tenant_id: impl Into<String>,
        root_holding_id: Uuid,
        holding_ref: impl Into<String>,
    ) -> Result<Self, TenantAdministrationError> {
        let binding = Self {
            customer_id,
            tenant_id: tenant_id.into(),
            root_holding_id,
            holding_ref: holding_ref.into(),
        };
        binding.validate()?;
        Ok(binding)
    }

    pub fn validate(&self) -> Result<(), TenantAdministrationError> {
        if self.customer_id.is_nil() || self.root_holding_id.is_nil() {
            return Err(TenantAdministrationError::NilIdentifier);
        }
        validate_bounded_reference(&self.tenant_id, 128)?;
        validate_bounded_reference(&self.holding_ref, 255)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TenantOrganizationScope {
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

impl TenantOrganizationScope {
    pub const fn holding_id(&self) -> Uuid {
        match self {
            Self::Holding { holding_id }
            | Self::Company { holding_id, .. }
            | Self::Branch { holding_id, .. } => *holding_id,
        }
    }

    pub fn as_access_scope(&self) -> Scope {
        match self {
            Self::Holding { holding_id } => Scope::Holding {
                holding_id: *holding_id,
            },
            Self::Company {
                holding_id,
                company_id,
            } => Scope::Company {
                holding_id: *holding_id,
                company_id: *company_id,
            },
            Self::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Scope::Branch {
                holding_id: *holding_id,
                company_id: *company_id,
                branch_id: *branch_id,
            },
        }
    }

    fn validate(&self) -> Result<(), TenantAdministrationError> {
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
            Err(TenantAdministrationError::NilIdentifier)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TenantWorkspaceViewerKind {
    TenantUser,
    PlatformTenantAdministrator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TenantWorkspaceBillingMode {
    Writable,
    Grace { until: DateTime<Utc> },
    ReadOnly,
}

impl TenantWorkspaceBillingMode {
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

/// Shared context consumed by every tenant module surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantWorkspaceContext {
    pub tenant_id: String,
    pub customer_id: Uuid,
    pub root_holding_id: Uuid,
    pub scope: TenantOrganizationScope,
    pub viewer_kind: TenantWorkspaceViewerKind,
    pub billing_mode: TenantWorkspaceBillingMode,
    pub entitlements: BTreeSet<TenantWorkspaceModule>,
    /// Present only for an entered platform administrator. It is opaque and
    /// must be redacted from logs and UI diagnostics.
    pub administrative_context_id: Option<Uuid>,
}

impl TenantWorkspaceContext {
    pub fn validate(&self) -> Result<(), TenantAdministrationError> {
        validate_bounded_reference(&self.tenant_id, 128)?;
        if self.customer_id.is_nil() || self.root_holding_id.is_nil() {
            return Err(TenantAdministrationError::NilIdentifier);
        }
        self.scope.validate()?;
        if self.scope.holding_id() != self.root_holding_id {
            return Err(TenantAdministrationError::ScopeOutsideTenant);
        }
        match (self.viewer_kind, self.administrative_context_id) {
            (TenantWorkspaceViewerKind::TenantUser, None)
            | (TenantWorkspaceViewerKind::PlatformTenantAdministrator, Some(_)) => Ok(()),
            _ => Err(TenantAdministrationError::AdministrativeContextRequired),
        }
    }

    pub fn permits_module(&self, module: TenantWorkspaceModule) -> bool {
        self.entitlements.contains(&module)
    }

    pub fn permits_business_mutation(&self, module: TenantWorkspaceModule) -> bool {
        self.permits_module(module) && self.billing_mode.permits_business_mutation()
    }
}

/// Server-side evidence created only after a fresh MFA ceremony and an active
/// `platform.tenant.enter` role check.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantAdminContext {
    pub id: Uuid,
    pub principal_ref: String,
    pub device_id: Uuid,
    pub session_id: Uuid,
    pub tenant_id: String,
    pub customer_id: Uuid,
    pub root_holding_id: Uuid,
    pub reason: String,
    pub correlation_id: Uuid,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl std::fmt::Debug for TenantAdminContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TenantAdminContext")
            .field("id", &"<redacted>")
            .field("principal_ref", &self.principal_ref)
            .field("device_id", &self.device_id)
            .field("session_id", &self.session_id)
            .field("tenant_id", &self.tenant_id)
            .field("customer_id", &self.customer_id)
            .field("root_holding_id", &self.root_holding_id)
            .field("reason", &self.reason)
            .field("correlation_id", &self.correlation_id)
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .field("revoked_at", &self.revoked_at)
            .finish()
    }
}

impl TenantAdminContext {
    pub fn issue(
        binding: &CustomerTenantBinding,
        principal_ref: impl Into<String>,
        device_id: Uuid,
        session_id: Uuid,
        reason: impl Into<String>,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Self, TenantAdministrationError> {
        binding.validate()?;
        let context = Self {
            id: Uuid::new_v4(),
            principal_ref: principal_ref.into(),
            device_id,
            session_id,
            tenant_id: binding.tenant_id.clone(),
            customer_id: binding.customer_id,
            root_holding_id: binding.root_holding_id,
            reason: reason.into(),
            correlation_id,
            issued_at: at,
            expires_at: at
                .checked_add_signed(TENANT_ADMIN_CONTEXT_TTL)
                .ok_or(TenantAdministrationError::TimestampOverflow)?,
            revoked_at: None,
        };
        context.validate_shape()?;
        Ok(context)
    }

    pub fn validate_shape(&self) -> Result<(), TenantAdministrationError> {
        if self.id.is_nil()
            || self.device_id.is_nil()
            || self.session_id.is_nil()
            || self.customer_id.is_nil()
            || self.root_holding_id.is_nil()
            || self.correlation_id.is_nil()
        {
            return Err(TenantAdministrationError::NilIdentifier);
        }
        validate_bounded_reference(&self.principal_ref, 255)?;
        validate_bounded_reference(&self.tenant_id, 128)?;
        validate_bounded_reference(&self.reason, MAX_TENANT_ADMIN_REASON_BYTES)?;
        if self.expires_at != self.issued_at + TENANT_ADMIN_CONTEXT_TTL {
            return Err(TenantAdministrationError::InvalidExpiry);
        }
        Ok(())
    }

    pub fn authorize(
        &self,
        request: &TenantAdminAuthorizationRequest<'_>,
    ) -> Result<(), TenantAdministrationError> {
        self.validate_shape()?;
        if self.revoked_at.is_some() {
            return Err(TenantAdministrationError::Revoked);
        }
        if request.at < self.issued_at || request.at >= self.expires_at {
            return Err(TenantAdministrationError::Expired);
        }
        if self.principal_ref != request.principal_ref {
            return Err(TenantAdministrationError::PrincipalMismatch);
        }
        if self.device_id != request.device_id {
            return Err(TenantAdministrationError::DeviceMismatch);
        }
        if self.session_id != request.session_id {
            return Err(TenantAdministrationError::SessionMismatch);
        }
        if self.tenant_id != request.tenant_id {
            return Err(TenantAdministrationError::TenantMismatch);
        }
        if !request.identity_active || !request.session_active || !request.directory_allows {
            return Err(TenantAdministrationError::IdentityOrSessionInactive);
        }
        if !request.platform_role_active {
            return Err(TenantAdministrationError::RoleInactive);
        }
        if request.requested_scope.holding_id() != self.root_holding_id {
            return Err(TenantAdministrationError::ScopeOutsideTenant);
        }
        if let Some(module) = request.module
            && !request
                .entitlements
                .iter()
                .any(|entitlement| entitlement.module_key == module.as_str() && entitlement.active)
        {
            return Err(TenantAdministrationError::ModuleNotEntitled);
        }
        Ok(())
    }
}

pub struct TenantAdminAuthorizationRequest<'a> {
    pub principal_ref: &'a str,
    pub device_id: Uuid,
    pub session_id: Uuid,
    pub tenant_id: &'a str,
    pub requested_scope: &'a TenantOrganizationScope,
    pub module: Option<TenantWorkspaceModule>,
    pub entitlements: &'a [ModuleEntitlement],
    pub identity_active: bool,
    pub session_active: bool,
    /// Directory deny-wins is evaluated before this value reaches the domain.
    pub directory_allows: bool,
    pub platform_role_active: bool,
    pub at: DateTime<Utc>,
}

impl std::fmt::Debug for TenantAdminAuthorizationRequest<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TenantAdminAuthorizationRequest")
            .field("principal_ref", &self.principal_ref)
            .field("device_id", &self.device_id)
            .field("session_id", &self.session_id)
            .field("tenant_id", &self.tenant_id)
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
pub enum TenantAdministrationError {
    #[error("tenant administration identifier cannot be nil")]
    NilIdentifier,
    #[error("tenant administration reference is missing or invalid")]
    InvalidReference,
    #[error("tenant administration timestamp overflow")]
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
    #[error("administrative context tenant does not match")]
    TenantMismatch,
    #[error("identity, session, or directory authorization is inactive")]
    IdentityOrSessionInactive,
    #[error("platform tenant administrator role is inactive")]
    RoleInactive,
    #[error("requested organization scope is outside the target tenant")]
    ScopeOutsideTenant,
    #[error("requested module is not entitled")]
    ModuleNotEntitled,
    #[error("workspace module is unsupported")]
    UnsupportedModule,
}

fn validate_bounded_reference(
    value: &str,
    maximum: usize,
) -> Result<(), TenantAdministrationError> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        Err(TenantAdministrationError::InvalidReference)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (CustomerTenantBinding, DateTime<Utc>, Uuid, Uuid) {
        let at = DateTime::parse_from_rfc3339("2026-08-01T00:00:00Z")
            .expect("time")
            .with_timezone(&Utc);
        (
            CustomerTenantBinding::new(Uuid::new_v4(), "tenant-a", Uuid::new_v4(), "holding-a")
                .expect("binding"),
            at,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
    }

    #[test]
    fn admin_context_is_exactly_thirty_minutes_and_debug_redacts_id() {
        let (binding, at, device_id, session_id) = fixture();
        let context = TenantAdminContext::issue(
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
    fn context_revalidates_actor_device_session_tenant_scope_role_and_entitlement() {
        let (binding, at, device_id, session_id) = fixture();
        let context = TenantAdminContext::issue(
            &binding,
            "platform-admin",
            device_id,
            session_id,
            "investigate invoice-posting failure",
            Uuid::new_v4(),
            at,
        )
        .expect("context");
        let scope = TenantOrganizationScope::Holding {
            holding_id: binding.root_holding_id,
        };
        let entitlements = vec![ModuleEntitlement {
            module_key: "finance".into(),
            active: true,
            seats: 5,
            company_limit: 2,
            branch_limit: 4,
            active_until: None,
        }];
        let mut request = TenantAdminAuthorizationRequest {
            principal_ref: "platform-admin",
            device_id,
            session_id,
            tenant_id: "tenant-a",
            requested_scope: &scope,
            module: Some(TenantWorkspaceModule::Finance),
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
            Err(TenantAdministrationError::DeviceMismatch)
        );
        request.device_id = device_id;
        request.at = at + Duration::minutes(30);
        assert_eq!(
            context.authorize(&request),
            Err(TenantAdministrationError::Expired)
        );
    }

    #[test]
    fn conflicting_identifiers_never_cross_tenant_contexts() {
        let (binding, at, device_id, session_id) = fixture();
        let context = TenantAdminContext::issue(
            &binding,
            "platform-admin",
            device_id,
            session_id,
            "compare isolated records",
            Uuid::new_v4(),
            at,
        )
        .expect("context");
        let scope = TenantOrganizationScope::Holding {
            holding_id: binding.root_holding_id,
        };
        let request = TenantAdminAuthorizationRequest {
            principal_ref: "platform-admin",
            device_id,
            session_id,
            tenant_id: "tenant-b",
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
            Err(TenantAdministrationError::TenantMismatch)
        );
    }

    #[test]
    fn platform_workspace_requires_admin_context_and_entitled_module() {
        let (binding, _, _, _) = fixture();
        let context = TenantWorkspaceContext {
            tenant_id: binding.tenant_id,
            customer_id: binding.customer_id,
            root_holding_id: binding.root_holding_id,
            scope: TenantOrganizationScope::Holding {
                holding_id: binding.root_holding_id,
            },
            viewer_kind: TenantWorkspaceViewerKind::PlatformTenantAdministrator,
            billing_mode: TenantWorkspaceBillingMode::ReadOnly,
            entitlements: BTreeSet::from([TenantWorkspaceModule::Finance]),
            administrative_context_id: Some(Uuid::new_v4()),
        };
        assert_eq!(context.validate(), Ok(()));
        assert!(context.permits_module(TenantWorkspaceModule::Finance));
        assert!(!context.permits_business_mutation(TenantWorkspaceModule::Finance));
        assert!(!context.permits_module(TenantWorkspaceModule::Sales));
    }
}
