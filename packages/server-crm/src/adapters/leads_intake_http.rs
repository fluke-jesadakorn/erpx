//! Native CRM lead create / qualify command HTTP ingress.
//!
//! Transport facts only: a registered Host/Holding target, the exact browser
//! Origin, one `Idempotency-Key`, a bounded typed lead body and business-error
//! mapping. Current Account session resolution, selected Company/branch/
//! department scope, current IAM authority, package purchase admission,
//! Finance customer status, Commerce currency, the reviewed account link,
//! owner CAS effects and Workflow receipts/audit/outbox remain composition
//! responsibilities behind [`CrmLeadIntakeHttpBackend`].
//!
//! Create and qualify use their own routes so the frozen lead/opportunity
//! change ingress keeps rejecting `qualify` and its existing receipt identity
//! is unchanged.
use crate::adapters::commands_http::{
    CommandHttpFuture, bounded, configure, error_response, parse_command, target,
};
use crate::domain::leads_native::{LeadChangeInput, LeadCreateInput};
use erpx_server_runtime::public::http_support::{
    HeaderMap, Path, Request, Response, Router, State, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey};
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct CrmLeadIntakeHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a CommandKey,
    /// Present for qualify; absent on create.
    pub lead_id: Option<&'a str>,
    pub body: &'a serde_json::Value,
}

/// Composition-owned Account session, scope, authority, purchase, Finance/
/// Commerce witness and owner transaction adapter. The router never mints a
/// session, actor, Company, permission, customer, currency or entitlement.
pub trait CrmLeadIntakeHttpBackend: Send + Sync {
    fn create_lead<'a>(&'a self, request: CrmLeadIntakeHttpRequest<'a>) -> CommandHttpFuture<'a>;
    fn qualify_lead<'a>(&'a self, request: CrmLeadIntakeHttpRequest<'a>) -> CommandHttpFuture<'a>;
}

pub struct CrmLeadIntakeHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn CrmLeadIntakeHttpBackend>,
}

struct CrmLeadIntakeHttpState {
    targets: BTreeMap<(String, String), CrmLeadIntakeHttpTarget>,
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "CRM_LEAD_INTAKE_REQUEST_INVALID",
        "Use one bounded CRM lead command.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "CRM_LEAD_INTAKE_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn qualification_required() -> BusinessError {
    BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "Use a qualification command.")
}

pub fn crm_lead_intake_http_router(
    targets: Vec<CrmLeadIntakeHttpTarget>,
) -> Result<Router, BusinessError> {
    let configured = configure(targets, "CRM_LEAD_INTAKE_CONFIGURATION_INVALID", |target| {
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
            "/api/v1/holdings/{holding_slug}/crm/leads",
            post(create_lead),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/crm/leads/{lead_id}/qualification/changes",
            post(qualify_lead),
        )
        .with_state(Arc::new(CrmLeadIntakeHttpState {
            targets: configured,
        })))
}

async fn create_lead(
    State(state): State<Arc<CrmLeadIntakeHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state.targets, request.headers(), &slug, malformed)?;
        let parsed = parse_command(request, &target.origin, malformed).await?;
        let _: LeadCreateInput =
            serde_json::from_value(parsed.body.clone()).map_err(|_| malformed())?;
        target
            .backend
            .create_lead(CrmLeadIntakeHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &parsed.headers,
                key: &parsed.key,
                lead_id: None,
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

async fn qualify_lead(
    State(state): State<Arc<CrmLeadIntakeHttpState>>,
    Path((slug, lead_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state.targets, request.headers(), &slug, malformed)?;
        let parsed = parse_command(request, &target.origin, malformed).await?;
        let input: LeadChangeInput =
            serde_json::from_value(parsed.body.clone()).map_err(|_| malformed())?;
        if input.action != "qualify" {
            return Err(qualification_required());
        }
        target
            .backend
            .qualify_lead(CrmLeadIntakeHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &parsed.headers,
                key: &parsed.key,
                lead_id: Some(&lead_id),
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
