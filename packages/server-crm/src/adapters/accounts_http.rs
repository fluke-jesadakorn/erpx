//! Native CRM account identity command HTTP ingress.
//!
//! Transport facts only: a registered Host/Holding target, the exact browser
//! Origin, one `Idempotency-Key`, a bounded typed identity body and
//! business-error mapping. Current Account session resolution, selected
//! Company/branch/department scope, current IAM authority, package purchase
//! admission, the Finance customer witness, owner CAS effects and Workflow
//! receipts/audit/outbox remain composition responsibilities behind
//! [`CrmAccountCommandHttpBackend`].
//!
//! Only the owner-complete actions are accepted: `create-account` and
//! `link-finance`. `create-contact`, `link-guest` and `guest-sharing` have no
//! native owner domain/application/adapter yet, so they fail closed here with
//! `CRM_IDENTITY_ACTION_NOT_NATIVE` and stay on the retained TypeScript
//! identity command until their slices land.
use crate::adapters::commands_http::{
    CommandHttpFuture, CrmCommandHttpReply, bounded, configure, error_response, parse_command,
    target,
};
use crate::domain::accounts::{AccountCreateInput, AccountLinkFinanceInput};
use erpx_server_runtime::public::http_support::{
    HeaderMap, Path, Request, Response, Router, State, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct CrmAccountCommandHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a CommandKey,
    pub body: &'a serde_json::Value,
}

/// Composition-owned Account session, scope, authority, purchase, Finance
/// witness and owner transaction adapter. The router never mints a session,
/// actor, Company, permission, customer or entitlement.
pub trait CrmAccountCommandHttpBackend: Send + Sync {
    fn create_account<'a>(
        &'a self,
        request: CrmAccountCommandHttpRequest<'a>,
    ) -> CommandHttpFuture<'a>;
    fn link_finance<'a>(
        &'a self,
        request: CrmAccountCommandHttpRequest<'a>,
    ) -> CommandHttpFuture<'a>;
}

pub struct CrmAccountCommandHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn CrmAccountCommandHttpBackend>,
}

struct CrmAccountCommandHttpState {
    targets: BTreeMap<(String, String), CrmAccountCommandHttpTarget>,
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "CRM_IDENTITY_REQUEST_INVALID",
        "Use one bounded CRM identity command.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "CRM_IDENTITY_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn not_native() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CRM_IDENTITY_ACTION_NOT_NATIVE",
        "The CRM identity command is not native yet.",
    )
}

pub fn crm_account_command_http_router(
    targets: Vec<CrmAccountCommandHttpTarget>,
) -> Result<Router, BusinessError> {
    let configured = configure(targets, "CRM_IDENTITY_CONFIGURATION_INVALID", |target| {
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
            "/api/v1/holdings/{holding_slug}/crm/identity/changes",
            post(change_identity),
        )
        .with_state(Arc::new(CrmAccountCommandHttpState {
            targets: configured,
        })))
}

async fn change_identity(
    State(state): State<Arc<CrmAccountCommandHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state.targets, request.headers(), &slug, malformed)?;
        let parsed = parse_command(request, &target.origin, malformed).await?;
        let action = parsed
            .body
            .get("action")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(malformed)?;
        let reply = match action {
            "create-account" => {
                let _: AccountCreateInput =
                    serde_json::from_value(parsed.body.clone()).map_err(|_| malformed())?;
                target
                    .backend
                    .create_account(CrmAccountCommandHttpRequest {
                        host: &target.host,
                        holding_slug: &target.holding_slug,
                        headers: &parsed.headers,
                        key: &parsed.key,
                        body: &parsed.body,
                    })
                    .await?
            }
            "link-finance" => {
                let _: AccountLinkFinanceInput =
                    serde_json::from_value(parsed.body.clone()).map_err(|_| malformed())?;
                target
                    .backend
                    .link_finance(CrmAccountCommandHttpRequest {
                        host: &target.host,
                        holding_slug: &target.holding_slug,
                        headers: &parsed.headers,
                        key: &parsed.key,
                        body: &parsed.body,
                    })
                    .await?
            }
            _ => return Err(not_native()),
        };
        Ok::<CrmCommandHttpReply, BusinessError>(reply)
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.into_value()),
        Err(error) => error_response(error),
    }
}
