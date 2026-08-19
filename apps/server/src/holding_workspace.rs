//! Server-owned holding workspace resolution.
//!
//! Browser transports present only the authenticated role scope. Routing,
//! customer identity, and write-entitlement bindings remain control-plane
//! concerns and never need to be copied into business-module forms.

use erpx_domain::holding_administration::HoldingRegistryBinding;
use erpx_infrastructure::SqlxHoldingAdministrationStore;
use erpx_kernel::AuthorityScope;
use tonic::Status;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PresentedHoldingRoute<'a> {
    pub holding_id: &'a str,
    pub data_profile_key: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedHoldingWorkspace {
    pub customer_id: Uuid,
    pub holding_id: String,
    pub data_profile_key: String,
    pub canonical_holding_id: Uuid,
    pub subscription_id: Option<Uuid>,
}

impl ResolvedHoldingWorkspace {
    fn from_binding(binding: HoldingRegistryBinding, subscription_id: Option<Uuid>) -> Self {
        Self {
            customer_id: binding.customer_id,
            holding_id: binding.holding_id,
            data_profile_key: binding.data_profile_key,
            canonical_holding_id: binding.canonical_holding_id,
            subscription_id,
        }
    }
}

pub(crate) async fn resolve_holding_workspace(
    state: &AppState,
    scope: &AuthorityScope,
    presented_route: Option<PresentedHoldingRoute<'_>>,
    presented_subscription_id: Option<&str>,
    write: bool,
) -> Result<ResolvedHoldingWorkspace, Status> {
    scope
        .validate()
        .map_err(|_| Status::permission_denied("active holding scope is invalid"))?;
    let canonical_holding_id = scope_holding_id(scope)
        .ok_or_else(|| Status::permission_denied("a holding workspace scope is required"))?;
    let administration = SqlxHoldingAdministrationStore::new(state.store.clone());
    let holding_id = administration
        .holding_id_for_root_holding(canonical_holding_id)
        .await
        .map_err(|_| Status::unavailable("holding workspace binding is unavailable"))?
        .ok_or_else(|| Status::failed_precondition("holding workspace route is not configured"))?;
    let binding = administration
        .customer_holding_binding(&holding_id)
        .await
        .map_err(|_| Status::unavailable("holding workspace binding is unavailable"))?
        .ok_or_else(|| {
            Status::failed_precondition("holding workspace customer binding is not configured")
        })?;
    if binding.canonical_holding_id != canonical_holding_id {
        return Err(Status::permission_denied(
            "holding workspace holding does not match the active role",
        ));
    }
    validate_presented_route(&binding, presented_route)?;

    let subscription_id = if write || presented_subscription_id.is_some() {
        let subscriptions = state
            .commercial
            .customer_subscription_ids(binding.customer_id)
            .await
            .map_err(|_| Status::unavailable("holding subscription binding is unavailable"))?;
        Some(resolve_subscription_id(
            &subscriptions,
            presented_subscription_id,
        )?)
    } else {
        None
    };

    Ok(ResolvedHoldingWorkspace::from_binding(
        binding,
        subscription_id,
    ))
}

fn validate_presented_route(
    binding: &HoldingRegistryBinding,
    presented: Option<PresentedHoldingRoute<'_>>,
) -> Result<(), Status> {
    if let Some(route) = presented
        && (route.holding_id != binding.holding_id
            || route.data_profile_key != binding.data_profile_key)
    {
        return Err(Status::permission_denied(
            "presented holding route does not match the active workspace",
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

const fn scope_holding_id(scope: &AuthorityScope) -> Option<Uuid> {
    match scope {
        AuthorityScope::Holding { holding_id }
        | AuthorityScope::Company { holding_id, .. }
        | AuthorityScope::Branch { holding_id, .. }
        | AuthorityScope::Subtree { holding_id, .. } => Some(*holding_id),
        AuthorityScope::Platform => None,
    }
}

#[cfg(test)]
mod tests {
    use erpx_domain::holding_administration::HoldingRegistryBinding;

    use super::*;

    #[test]
    fn every_holding_scope_exposes_its_root_holding() {
        let holding_id = Uuid::from_u128(1);
        let company_id = Uuid::from_u128(2);
        let branch_id = Uuid::from_u128(3);
        for scope in [
            AuthorityScope::Holding { holding_id },
            AuthorityScope::Company {
                holding_id,
                company_id,
            },
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            },
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id: branch_id,
            },
        ] {
            assert_eq!(scope_holding_id(&scope), Some(holding_id));
        }
        assert_eq!(scope_holding_id(&AuthorityScope::Platform), None);
    }

    #[test]
    fn legacy_route_and_subscription_proofs_must_match_server_resolution() {
        let binding = HoldingRegistryBinding {
            customer_id: Uuid::from_u128(10),
            holding_id: "holding-001".into(),
            data_profile_key: "holding-ref-001".into(),
            canonical_holding_id: Uuid::from_u128(1),
        };
        assert!(
            validate_presented_route(
                &binding,
                Some(PresentedHoldingRoute {
                    holding_id: "holding-001",
                    data_profile_key: "holding-ref-001",
                }),
            )
            .is_ok()
        );
        assert_eq!(
            validate_presented_route(
                &binding,
                Some(PresentedHoldingRoute {
                    holding_id: "another-holding",
                    data_profile_key: "holding-ref-001",
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
