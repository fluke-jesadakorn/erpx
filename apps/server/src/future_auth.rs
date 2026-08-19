//! Shared authenticated holding routing for future-context target transports.
//!
//! Authentication is deliberately completed before holding-registry or data
//! plane lookup. Request scopes are treated only as presented claims and are
//! rebound to the active role and authoritative control-plane holding route.

use erpx_infrastructure::RoutedDatabaseSide;
use erpx_kernel::{ActorAttribution, ActorKind, AuthorityScope, HoldingId, PrincipalId, SessionId};
use erpx_proto::common as wire;
use sqlx::PgPool;
use tonic::{Request, Status, metadata::MetadataMap};
use uuid::Uuid;

use crate::holding_workspace::{ResolvedHoldingWorkspace, resolve_holding_workspace};
use crate::wave4_composition::authenticate_context_for_holding;
use crate::{AppState, boundary};

#[derive(Debug, Clone)]
pub(crate) struct FutureInvocation {
    pub principal_id: PrincipalId,
    pub holding_id: HoldingId,
    pub scope: AuthorityScope,
    pub workspace: ResolvedHoldingWorkspace,
    pub realm: erpx_identity::AuthenticatedRealm,
}

pub(crate) async fn command<T>(
    state: &AppState,
    request: &Request<T>,
    context: Option<&wire::CommandContext>,
    product: &'static str,
    permission: &'static str,
) -> Result<FutureInvocation, Status> {
    // The token/session boundary runs before any protected routing lookup.
    let claims = state.auth.authorize(request).await?;
    let domain = boundary::command_domain(request, context)?;
    let scope = crate::legacy_kernel_scope(&domain.active_scope);
    authorize_presented_context(
        state,
        request.metadata(),
        &claims,
        &domain.actor.principal_ref,
        scope,
        product,
        permission,
        true,
        (!context
            .expect("validated command context")
            .mfa_code
            .is_empty())
        .then_some(
            context
                .expect("validated command context")
                .mfa_code
                .as_str(),
        ),
    )
    .await
}

pub(crate) async fn query<T>(
    state: &AppState,
    request: &Request<T>,
    context: Option<&wire::QueryContext>,
    product: &'static str,
    permission: &'static str,
) -> Result<FutureInvocation, Status> {
    let claims = state.auth.authorize(request).await?;
    let boundary = boundary::query(request, context)?;
    let scope = decode_scope(
        context
            .expect("validated query context")
            .active_scope
            .as_ref(),
    )?;
    authorize_presented_context(
        state,
        request.metadata(),
        &claims,
        &boundary.actor,
        scope,
        product,
        permission,
        false,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn authorize_presented_context(
    state: &AppState,
    metadata: &MetadataMap,
    claims: &erpx_identity::AccessTokenClaims,
    presented_principal: &str,
    scope: AuthorityScope,
    product: &'static str,
    permission: &'static str,
    write: bool,
    mfa_code: Option<&str>,
) -> Result<FutureInvocation, Status> {
    crate::require_actor_match(
        &claims.principal_ref,
        presented_principal,
        "future-context actor must match the authenticated principal",
    )?;
    let workspace = resolve_holding_workspace(state, &scope, None, None, write).await?;
    let session_id = SessionId::try_from_uuid(claims.session_id)
        .map_err(|_| Status::internal("authenticated session attribution is invalid"))?;
    let presented_actor = ActorAttribution {
        principal_ref: claims.principal_ref.clone(),
        kind: match claims.principal_kind {
            erpx_identity::AuthenticatedPrincipalKind::Human => ActorKind::HoldingUser,
            erpx_identity::AuthenticatedPrincipalKind::Agent
            | erpx_identity::AuthenticatedPrincipalKind::Service => ActorKind::Service,
        },
        session_id: Some(session_id),
        represented_principal_ref: None,
        support_capability_id: None,
    };
    let subscription = workspace.subscription_id.map(|value| value.to_string());
    let actor = authenticate_context_for_holding(
        state,
        metadata,
        &presented_actor,
        &scope,
        &workspace.customer_id.to_string(),
        &workspace.holding_id,
        permission,
        product,
        write,
        subscription.as_deref(),
        mfa_code,
    )
    .await?;
    let principal_uuid = Uuid::parse_str(&actor.principal_ref).map_err(|_| {
        Status::permission_denied("future-context principal must be an opaque UUID")
    })?;
    let principal_id = PrincipalId::try_from_uuid(principal_uuid)
        .map_err(|_| Status::permission_denied("future-context principal is invalid"))?;
    let holding_uuid = Uuid::parse_str(&workspace.holding_id)
        .map_err(|_| Status::failed_precondition("holding binding is not an opaque UUID"))?;
    let holding_id = HoldingId::try_from_uuid(holding_uuid)
        .map_err(|_| Status::failed_precondition("holding binding is invalid"))?;
    Ok(FutureInvocation {
        principal_id,
        holding_id,
        scope,
        workspace,
        realm: claims.realm,
    })
}

pub(crate) async fn pool(
    state: &AppState,
    invocation: &FutureInvocation,
    side: RoutedDatabaseSide,
) -> Result<PgPool, Status> {
    let router = state
        .holding_router
        .as_ref()
        .ok_or_else(|| Status::unavailable("holding database routing is unavailable"))?;
    router
        .route(
            &invocation.workspace.holding_id,
            &invocation.workspace.data_profile_key,
            side,
        )
        .await
        .map(|route| route.database_pool().clone())
        .map_err(|_| Status::unavailable("future-context storage is unavailable"))
}

fn decode_scope(scope: Option<&wire::AuthorityScope>) -> Result<AuthorityScope, Status> {
    use wire::authority_scope::Scope;
    let scope = scope
        .and_then(|scope| scope.scope.as_ref())
        .ok_or_else(|| Status::permission_denied("holding scope is required"))?;
    let uuid = |value: &str, field: &'static str| {
        Uuid::parse_str(value).map_err(|_| Status::invalid_argument(format!("{field} is invalid")))
    };
    let decoded = match scope {
        Scope::Platform(_) => AuthorityScope::Platform,
        Scope::Holding(value) => AuthorityScope::Holding {
            holding_id: uuid(&value.holding_id, "holding_id")?,
        },
        Scope::Company(value) => AuthorityScope::Company {
            holding_id: uuid(&value.holding_id, "holding_id")?,
            company_id: uuid(&value.company_id, "company_id")?,
        },
        Scope::Branch(value) => AuthorityScope::Branch {
            holding_id: uuid(&value.holding_id, "holding_id")?,
            company_id: uuid(&value.company_id, "company_id")?,
            branch_id: uuid(&value.branch_id, "branch_id")?,
        },
        Scope::Subtree(value) => AuthorityScope::Subtree {
            holding_id: uuid(&value.holding_id, "holding_id")?,
            organization_node_id: uuid(&value.organization_node_id, "organization_node_id")?,
        },
    };
    decoded
        .validate()
        .map_err(|_| Status::permission_denied("holding scope is invalid"))?;
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holding_scope_decoder_rejects_nil_and_malformed_identifiers() {
        let nil = wire::AuthorityScope {
            scope: Some(wire::authority_scope::Scope::Holding(wire::HoldingScope {
                holding_id: Uuid::nil().to_string(),
            })),
        };
        assert_eq!(
            decode_scope(Some(&nil)).unwrap_err().code(),
            tonic::Code::PermissionDenied
        );
        let malformed = wire::AuthorityScope {
            scope: Some(wire::authority_scope::Scope::Holding(wire::HoldingScope {
                holding_id: "holding-label".into(),
            })),
        };
        assert_eq!(
            decode_scope(Some(&malformed)).unwrap_err().code(),
            tonic::Code::InvalidArgument
        );
    }
}
