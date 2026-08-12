//! Server-owned tenant workspace resolution.
//!
//! Browser transports present only the authenticated role scope. Routing,
//! customer identity, and write-entitlement bindings remain control-plane
//! concerns and never need to be copied into business-module forms.

use erpx_domain::tenant_administration::CustomerTenantBinding;
use erpx_infrastructure::SqlxTenantAdministrationStore;
use erpx_kernel::TenantScope;
use tonic::Status;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PresentedTenantRoute<'a> {
    pub tenant_id: &'a str,
    pub holding_ref: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedTenantWorkspace {
    pub customer_id: Uuid,
    pub tenant_id: String,
    pub holding_ref: String,
    pub root_holding_id: Uuid,
    pub subscription_id: Option<Uuid>,
}

impl ResolvedTenantWorkspace {
    fn from_binding(binding: CustomerTenantBinding, subscription_id: Option<Uuid>) -> Self {
        Self {
            customer_id: binding.customer_id,
            tenant_id: binding.tenant_id,
            holding_ref: binding.holding_ref,
            root_holding_id: binding.root_holding_id,
            subscription_id,
        }
    }
}

pub(crate) async fn resolve_tenant_workspace(
    state: &AppState,
    scope: &TenantScope,
    presented_route: Option<PresentedTenantRoute<'_>>,
    presented_subscription_id: Option<&str>,
    write: bool,
) -> Result<ResolvedTenantWorkspace, Status> {
    scope
        .validate()
        .map_err(|_| Status::permission_denied("active tenant scope is invalid"))?;
    let root_holding_id = scope_holding_id(scope)
        .ok_or_else(|| Status::permission_denied("a tenant workspace scope is required"))?;
    let administration = SqlxTenantAdministrationStore::new(state.store.clone());
    let tenant_id = administration
        .tenant_id_for_root_holding(root_holding_id)
        .await
        .map_err(|_| Status::unavailable("tenant workspace binding is unavailable"))?
        .ok_or_else(|| Status::failed_precondition("tenant workspace route is not configured"))?;
    let binding = administration
        .customer_tenant_binding(&tenant_id)
        .await
        .map_err(|_| Status::unavailable("tenant workspace binding is unavailable"))?
        .ok_or_else(|| {
            Status::failed_precondition("tenant workspace customer binding is not configured")
        })?;
    if binding.root_holding_id != root_holding_id {
        return Err(Status::permission_denied(
            "tenant workspace holding does not match the active role",
        ));
    }
    validate_presented_route(&binding, presented_route)?;

    let subscription_id = if write || presented_subscription_id.is_some() {
        let subscriptions = state
            .commercial
            .customer_subscription_ids(binding.customer_id)
            .await
            .map_err(|_| Status::unavailable("tenant subscription binding is unavailable"))?;
        Some(resolve_subscription_id(
            &subscriptions,
            presented_subscription_id,
        )?)
    } else {
        None
    };

    Ok(ResolvedTenantWorkspace::from_binding(
        binding,
        subscription_id,
    ))
}

fn validate_presented_route(
    binding: &CustomerTenantBinding,
    presented: Option<PresentedTenantRoute<'_>>,
) -> Result<(), Status> {
    if let Some(route) = presented
        && (route.tenant_id != binding.tenant_id || route.holding_ref != binding.holding_ref)
    {
        return Err(Status::permission_denied(
            "presented tenant route does not match the active workspace",
        ));
    }
    Ok(())
}

fn validate_presented_subscription(resolved: Uuid, presented: Option<&str>) -> Result<(), Status> {
    if let Some(presented) = presented {
        let presented = Uuid::parse_str(presented)
            .map_err(|_| Status::permission_denied("write entitlement proof is invalid"))?;
        if presented != resolved {
            return Err(Status::permission_denied(
                "presented subscription does not match the active workspace",
            ));
        }
    }
    Ok(())
}

fn resolve_subscription_id(
    subscriptions: &[Uuid],
    presented: Option<&str>,
) -> Result<Uuid, Status> {
    let [subscription_id] = subscriptions else {
        return Err(Status::failed_precondition(if subscriptions.is_empty() {
            "write entitlement is not configured"
        } else {
            "write entitlement binding is ambiguous"
        }));
    };
    validate_presented_subscription(*subscription_id, presented)?;
    Ok(*subscription_id)
}

const fn scope_holding_id(scope: &TenantScope) -> Option<Uuid> {
    match scope {
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => Some(*holding_id),
        TenantScope::Platform => None,
    }
}

#[cfg(test)]
mod tests {
    use erpx_domain::tenant_administration::CustomerTenantBinding;

    use super::*;

    #[test]
    fn every_tenant_scope_exposes_its_root_holding() {
        let holding_id = Uuid::from_u128(1);
        let company_id = Uuid::from_u128(2);
        let branch_id = Uuid::from_u128(3);
        for scope in [
            TenantScope::Holding { holding_id },
            TenantScope::Company {
                holding_id,
                company_id,
            },
            TenantScope::Branch {
                holding_id,
                company_id,
                branch_id,
            },
            TenantScope::Subtree {
                holding_id,
                company_ids: vec![company_id],
                branch_ids: vec![branch_id],
            },
        ] {
            assert_eq!(scope_holding_id(&scope), Some(holding_id));
        }
        assert_eq!(scope_holding_id(&TenantScope::Platform), None);
    }

    #[test]
    fn legacy_route_and_subscription_proofs_must_match_server_resolution() {
        let binding = CustomerTenantBinding {
            customer_id: Uuid::from_u128(10),
            tenant_id: "tenant-001".into(),
            holding_ref: "holding-ref-001".into(),
            root_holding_id: Uuid::from_u128(1),
        };
        assert!(
            validate_presented_route(
                &binding,
                Some(PresentedTenantRoute {
                    tenant_id: "tenant-001",
                    holding_ref: "holding-ref-001",
                }),
            )
            .is_ok()
        );
        assert_eq!(
            validate_presented_route(
                &binding,
                Some(PresentedTenantRoute {
                    tenant_id: "another-tenant",
                    holding_ref: "holding-ref-001",
                }),
            )
            .unwrap_err()
            .code(),
            tonic::Code::PermissionDenied
        );

        let subscription_id = Uuid::from_u128(20);
        assert!(
            validate_presented_subscription(subscription_id, Some(&subscription_id.to_string()))
                .is_ok()
        );
        assert_eq!(
            validate_presented_subscription(
                subscription_id,
                Some(&Uuid::from_u128(21).to_string())
            )
            .unwrap_err()
            .code(),
            tonic::Code::PermissionDenied
        );

        assert_eq!(
            resolve_subscription_id(&[], None).unwrap_err().code(),
            tonic::Code::FailedPrecondition
        );
        assert_eq!(
            resolve_subscription_id(&[subscription_id, Uuid::from_u128(22)], None)
                .unwrap_err()
                .code(),
            tonic::Code::FailedPrecondition
        );
        assert_eq!(
            resolve_subscription_id(&[subscription_id], None).unwrap(),
            subscription_id
        );
    }
}
