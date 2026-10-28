//! Unmounted production assembly: only genuine Identity and owner administration.
pub use crate::field_service_http_wire::{
    OtherPostBranch, SharedPostWire, SharedPostWireFailure, classify_shared_post,
};
use crate::{
    business_http::BusinessScopeSlugs,
    field_service_http_core::{
        PendingIngress, PostExecutor, PostResponse, TrustedTarget, WireError, router_for_executor,
    },
    field_service_parts::{FieldServicePartsAdministration, HttpIngressError, OtherPost},
};
use axum::{Router, http::HeaderMap};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{CapturedHoldingMutation, parse_inbound_account_request},
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use erpx_server_workflow::public::commands::CommandJson;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};
type WorkspaceInput = Box<dyn FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send>;
type NativeResult =
    Result<Result<CommandJson, HttpIngressError>, PgTransactionError<BusinessError>>;
trait NativeAdministration: Send + Sync {
    fn execute<'a>(
        &'a self,
        host: String,
        proof: DeviceProof,
        workspace: WorkspaceInput,
        ingress: Arc<PendingIngress>,
        key: String,
        other: Arc<dyn OtherPost>,
    ) -> Pin<Box<dyn Future<Output = NativeResult> + Send + 'a>>;
}
impl<W: Clock + Send + Sync + 'static, G: Clock + Send + Sync + 'static> NativeAdministration
    for FieldServicePartsAdministration<W, G>
{
    fn execute<'a>(
        &'a self,
        host: String,
        proof: DeviceProof,
        workspace: WorkspaceInput,
        ingress: Arc<PendingIngress>,
        key: String,
        other: Arc<dyn OtherPost>,
    ) -> Pin<Box<dyn Future<Output = NativeResult> + Send + 'a>> {
        Box::pin(async move {
            self.execute_http(&host, proof, workspace, ingress, key, other)
                .await
        })
    }
}
/// Constructed from the genuine owner administration, never an inbound actor.
pub struct FieldServiceHttpTarget {
    host: String,
    holding_slug: String,
    service: Arc<dyn NativeAdministration>,
    other: Arc<dyn OtherPost>,
}
impl FieldServiceHttpTarget {
    pub fn new<W: Clock + Send + Sync + 'static, G: Clock + Send + Sync + 'static>(
        host: String,
        holding_slug: String,
        service: Arc<FieldServicePartsAdministration<W, G>>,
        other: Arc<dyn OtherPost>,
    ) -> Self {
        Self {
            host,
            holding_slug,
            service,
            other,
        }
    }
}
struct SealedNativeExecutor {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String), FieldServiceHttpTarget>,
}
/// Non-generic production factory requires actual configured Holding targets.
pub fn field_service_http_router(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<FieldServiceHttpTarget>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let mut registered = BTreeMap::new();
    let mut trusted = Vec::new();
    for target in targets {
        let matching = policy
            .audience(&target.host)
            .and_then(|a| registry.expected_for_audience(&a).ok())
            .is_some_and(|h| h.slug == target.holding_slug);
        let origin = policy
            .holdings
            .iter()
            .find(|h| h.host == target.host)
            .map(|h| h.origin.clone());
        let Some(origin) = origin else {
            return Err(BusinessError::invalid(
                "FIELD_SERVICE_HTTP_CONFIGURATION_INVALID",
                "Configure distinct actual Holding hosts and slugs.",
            ));
        };
        let identity = (target.host.clone(), target.holding_slug.clone());
        if !matching || registered.insert(identity.clone(), target).is_some() {
            return Err(BusinessError::invalid(
                "FIELD_SERVICE_HTTP_CONFIGURATION_INVALID",
                "Configure distinct actual Holding hosts and slugs.",
            ));
        }
        trusted.push(TrustedTarget {
            host: identity.0,
            slug: identity.1,
            origin,
        });
    }
    Ok(router_for_executor(
        trusted,
        SealedNativeExecutor {
            policy,
            scopes,
            targets: registered,
        },
    ))
}
impl PostExecutor for SealedNativeExecutor {
    fn execute<'a>(
        &'a self,
        host: String,
        slug: String,
        headers: HeaderMap,
        key: String,
        ingress: Arc<PendingIngress>,
    ) -> Pin<Box<dyn Future<Output = PostResponse> + Send + 'a>> {
        Box::pin(async move {
            let Some(target) = self.targets.get(&(host.clone(), slug)) else {
                return PostResponse::error(
                    axum::http::StatusCode::NOT_FOUND,
                    "NOT_FOUND",
                    "Not found.",
                );
            };
            let inbound = match parse_inbound_account_request(&self.policy, &headers) {
                Ok(value) => value,
                Err(error) => return PostResponse::business(error),
            };
            if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
                return PostResponse::error(
                    axum::http::StatusCode::NOT_FOUND,
                    "NOT_FOUND",
                    "Not found.",
                );
            }
            let scope = self.scopes.slugs(&headers);
            let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
            let captured = CapturedHoldingMutation::capture(&headers, branch, department);
            let input: WorkspaceInput = Box::new(move || {
                scope?;
                captured.resolve()
            });
            match target
                .service
                .execute(
                    host,
                    inbound.into_proof(),
                    input,
                    ingress,
                    key,
                    target.other.clone(),
                )
                .await
            {
                Ok(Ok(data)) => PostResponse::Success(data),
                Ok(Err(HttpIngressError::Validation(message))) => {
                    PostResponse::wire(WireError::Validation(message))
                }
                Ok(Err(HttpIngressError::Scope)) => PostResponse::business(BusinessError::new(
                    FailureKind::Forbidden,
                    "FIELD_SERVICE_CONFIGURATION_SCOPE_CHANGED",
                    "The configured Field Service scope changed.",
                )),
                Ok(Err(
                    HttpIngressError::InitialOwnerReadDenied(_)
                    | HttpIngressError::InitialOwnerWriteDenied(_),
                )) => PostResponse::error(
                    axum::http::StatusCode::FORBIDDEN,
                    "WORK_ROLE_UNAVAILABLE",
                    "The original Service assignment is no longer current.",
                ),
                Ok(Err(HttpIngressError::InitialOwnerModuleDenied)) => PostResponse::error(
                    axum::http::StatusCode::FORBIDDEN,
                    "MODULE_FORBIDDEN",
                    "The current assignment no longer selects Field Service.",
                ),
                Ok(Err(HttpIngressError::OtherAdmissionDenied(branch, error))) => {
                    PostResponse::business(target.other.admission_error(branch, error))
                }
                Ok(Err(HttpIngressError::InvalidJson)) => {
                    PostResponse::wire(WireError::InvalidJson)
                }
                Err(PgTransactionError::Persistence(_)) => PostResponse::internal(),
                Err(PgTransactionError::Business(e)) => PostResponse::business(e),
            }
        })
    }
}
