//! Native CRM activity command HTTP ingress.
//!
//! Transport facts only: a registered Host/Holding target, the exact browser
//! Origin, one `Idempotency-Key`, a bounded typed activity body and
//! business-error mapping. Current Account session resolution, selected scope,
//! Sales-recipient eligibility, purchase admission, Workflow replay, owner CAS
//! effects and Collaboration notification remain composition responsibilities
//! behind [`CrmActivityCommandHttpBackend`].
//!
//! `create`, `complete` and `cancel` are the owner-complete actions. `reassign`
//! is exposed through a separate backend method because it additionally needs
//! the current Sales recipient witness, Workflow replay and the Collaboration
//! notification in the same retained transaction; composition must not route it
//! through the plain change path.
use crate::adapters::commands_http::{
    CommandHttpFuture, bounded, configure, error_response, parse_command, target,
};
use crate::domain::activities_native::{ActivityChangeInput, ActivityCreateInput};
use erpx_server_runtime::public::http_support::{
    HeaderMap, Path, Request, Response, Router, State, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey};
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct CrmActivityCommandHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a CommandKey,
    /// Present for `complete`, `cancel` and `reassign`; absent on create.
    pub activity_id: Option<&'a str>,
    pub body: &'a serde_json::Value,
}

/// Composition-owned Account session, scope, authority, purchase, Workflow,
/// Sales recipient, notification and owner transaction adapter. The router
/// never mints a session, actor, Company, permission, recipient or entitlement.
pub trait CrmActivityCommandHttpBackend: Send + Sync {
    fn create_activity<'a>(
        &'a self,
        request: CrmActivityCommandHttpRequest<'a>,
    ) -> CommandHttpFuture<'a>;
    fn change_activity<'a>(
        &'a self,
        request: CrmActivityCommandHttpRequest<'a>,
    ) -> CommandHttpFuture<'a>;
    /// Separate owner transaction: composition must prepare the recipient
    /// witness and Workflow replay, then retain the Collaboration notification
    /// with the owner after-image.
    fn reassign_activity<'a>(
        &'a self,
        request: CrmActivityCommandHttpRequest<'a>,
    ) -> CommandHttpFuture<'a>;
}

pub struct CrmActivityCommandHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn CrmActivityCommandHttpBackend>,
}

struct CrmActivityCommandHttpState {
    targets: BTreeMap<(String, String), CrmActivityCommandHttpTarget>,
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "CRM_ACTIVITY_REQUEST_INVALID",
        "Use one bounded CRM activity command.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "CRM_ACTIVITY_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn action_invalid() -> BusinessError {
    BusinessError::invalid(
        "VALIDATION_FAILED",
        "Choose a complete, cancel or reassign activity action.",
    )
}

pub fn crm_activity_command_http_router(
    targets: Vec<CrmActivityCommandHttpTarget>,
) -> Result<Router, BusinessError> {
    let configured = configure(targets, "CRM_ACTIVITY_CONFIGURATION_INVALID", |target| {
        (
            target.host.as_str(),
            target.holding_slug.as_str(),
            target.origin.as_str(),
        )
    })?;
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/crm/activities",
            post(create_activity),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/crm/activities/{activity_id}/changes",
            post(change_activity),
        )
        .with_state(Arc::new(CrmActivityCommandHttpState {
            targets: configured,
        })))
}

async fn create_activity(
    State(state): State<Arc<CrmActivityCommandHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state.targets, request.headers(), &slug, malformed)?;
        let parsed = parse_command(request, &target.origin, malformed).await?;
        let _: ActivityCreateInput =
            serde_json::from_value(parsed.body.clone()).map_err(|_| malformed())?;
        target
            .backend
            .create_activity(CrmActivityCommandHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &parsed.headers,
                key: &parsed.key,
                activity_id: None,
                body: &parsed.body,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.into_value()),
        Err(error) => error_response(error),
    }
}

async fn change_activity(
    State(state): State<Arc<CrmActivityCommandHttpState>>,
    Path((slug, activity_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state.targets, request.headers(), &slug, malformed)?;
        let parsed = parse_command(request, &target.origin, malformed).await?;
        let input: ActivityChangeInput =
            serde_json::from_value(parsed.body.clone()).map_err(|_| malformed())?;
        let request = CrmActivityCommandHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: &parsed.headers,
            key: &parsed.key,
            activity_id: Some(&activity_id),
            body: &parsed.body,
        };
        match input.action.as_str() {
            "complete" | "cancel" => target.backend.change_activity(request).await,
            "reassign" => target.backend.reassign_activity(request).await,
            _ => Err(action_invalid()),
        }
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.into_value()),
        Err(error) => error_response(error),
    }
}
