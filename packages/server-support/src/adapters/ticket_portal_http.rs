//! Native Support customer ticket-portal HTTP ingress.
//!
//! Transport facts only: exact registered Host/Holding targets, origin,
//! idempotency, bounded JSON and business-error mapping. Normal Account session
//! resolution, current Company/branch/department scope, Commercial purchase
//! admission and the serializable owner unit belong to the composition backend,
//! which must implement [`TicketPortalHttpBackend`]. This router performs no SQL
//! and imports no composition module.
use crate::adapters::support_http::{
    OriginPolicy, SupportHttpFuture, SupportHttpTargetSpec, configure_targets, created,
    error_response, missing, no_query, ok, parse_command, selected_target, ticket_selection_query,
    validation_failed,
};
use crate::domain::ticket_portal::{
    AcknowledgementInput, AcknowledgementResult, DecisionInput, DecisionResult, PortalProjection,
    PortalWorkspace, PrepareInput, PrepareResult, ResponseInput, ResponseResult,
    parse_acknowledgement, parse_decision, parse_prepare, parse_response,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, Path, Request, Response, Router, State,
    routing::{get, post},
    serde_json::Value,
};
use erpx_server_shared::public::{BusinessError, CommandKey};
use std::{collections::BTreeMap, sync::Arc};

const PORTAL_BODY_LIMIT: usize = 50_000;

pub struct TicketPortalWorkspaceHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub ticket_id: Option<&'a str>,
    pub binding_id: Option<&'a str>,
}

pub struct TicketPortalCustomerHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

pub struct TicketPortalPrepareHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub input: &'a PrepareInput,
    pub body: &'a Value,
    pub key: &'a CommandKey,
}

pub struct TicketPortalDecisionHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub binding_id: &'a str,
    pub input: &'a DecisionInput,
    pub body: &'a Value,
    pub key: &'a CommandKey,
}

pub struct TicketPortalResponseHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub binding_id: &'a str,
    pub input: &'a ResponseInput,
    pub body: &'a Value,
    pub key: &'a CommandKey,
}

pub struct TicketPortalAcknowledgementHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub binding_id: &'a str,
    pub input: &'a AcknowledgementInput,
    pub body: &'a Value,
    pub key: &'a CommandKey,
}

/// Composition-owned Account proof, scope, purchase and command-unit adapter.
/// The router never mints a session, actor, Company, customer or permission.
pub trait TicketPortalHttpBackend: Send + Sync {
    fn staff_workspace<'a>(
        &'a self,
        request: TicketPortalWorkspaceHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, PortalWorkspace>;

    fn customer_projection<'a>(
        &'a self,
        request: TicketPortalCustomerHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, PortalProjection>;

    fn prepare<'a>(
        &'a self,
        request: TicketPortalPrepareHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, PrepareResult>;

    fn decide<'a>(
        &'a self,
        request: TicketPortalDecisionHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, DecisionResult>;

    fn respond<'a>(
        &'a self,
        request: TicketPortalResponseHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, ResponseResult>;

    fn acknowledge<'a>(
        &'a self,
        request: TicketPortalAcknowledgementHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, AcknowledgementResult>;
}

pub struct TicketPortalHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding/portal host.
    pub origin: String,
    pub backend: Arc<dyn TicketPortalHttpBackend>,
}

impl SupportHttpTargetSpec for TicketPortalHttpTarget {
    fn host(&self) -> &str {
        &self.host
    }
    fn holding_slug(&self) -> &str {
        &self.holding_slug
    }
    fn origin(&self) -> &str {
        &self.origin
    }
}

struct TicketPortalHttpState {
    targets: BTreeMap<(String, String), TicketPortalHttpTarget>,
}

pub fn ticket_portal_http_router(
    targets: Vec<TicketPortalHttpTarget>,
) -> Result<Router, BusinessError> {
    let configured = configure_targets(targets, "SUPPORT_PORTAL_HTTP_CONFIGURATION_INVALID")?;
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/support/ticket-portal",
            get(staff_workspace).post(prepare),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/support/ticket-portal/{binding_id}",
            get(staff_binding_workspace),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/support/ticket-portal/{binding_id}/decisions",
            post(decide),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/support/ticket-portal/{binding_id}/responses",
            post(respond),
        )
        .route(
            "/api/customer-portal/{holding_slug}/tickets",
            get(customer_projection),
        )
        .route(
            "/api/customer-portal/{holding_slug}/tickets/{binding_id}/acknowledgements",
            post(acknowledge),
        )
        .with_state(Arc::new(TicketPortalHttpState {
            targets: configured,
        })))
}

fn response_value<T: serde::Serialize>(value: T) -> Result<Value, BusinessError> {
    erpx_server_runtime::public::http_support::serde_json::to_value(value).map_err(|_| {
        BusinessError::invalid("SUPPORT_RESPONSE_INVALID", "Invalid Support response.")
    })
}

fn valid_path_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 200 && !value.chars().any(char::is_control)
}

async fn staff_workspace(
    State(state): State<Arc<TicketPortalHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let target = match selected_target(&state.targets, request.headers(), &slug) {
        Ok(target) => target,
        Err(error) => return error_response(error),
    };
    let ticket_id = match ticket_selection_query(request.uri().query()) {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    let result = target
        .backend
        .staff_workspace(TicketPortalWorkspaceHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: request.headers(),
            ticket_id: ticket_id.as_deref(),
            binding_id: None,
        })
        .await;
    match result.and_then(response_value) {
        Ok(value) => ok(value),
        Err(error) => error_response(error),
    }
}

async fn staff_binding_workspace(
    State(state): State<Arc<TicketPortalHttpState>>,
    Path((slug, binding_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let target = match selected_target(&state.targets, request.headers(), &slug) {
        Ok(target) => target,
        Err(error) => return error_response(error),
    };
    if request.uri().query().is_some_and(|query| !query.is_empty()) {
        return error_response(missing());
    }
    if !valid_path_id(&binding_id) {
        return error_response(validation_failed());
    }
    let result = target
        .backend
        .staff_workspace(TicketPortalWorkspaceHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: request.headers(),
            ticket_id: None,
            binding_id: Some(&binding_id),
        })
        .await;
    match result.and_then(response_value) {
        Ok(value) => ok(value),
        Err(error) => error_response(error),
    }
}

async fn prepare(
    State(state): State<Arc<TicketPortalHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let target = match selected_target(&state.targets, request.headers(), &slug) {
        Ok(target) => target,
        Err(error) => return error_response(error),
    };
    if let Err(error) = no_query(request.uri().query()) {
        return error_response(error);
    }
    let parsed = match parse_command(
        request,
        &target.origin,
        OriginPolicy::Required,
        PORTAL_BODY_LIMIT,
    )
    .await
    {
        Ok(parsed) => parsed,
        Err(error) => return error_response(error),
    };
    let input = match parse_prepare(&parsed.body) {
        Ok(input) => input,
        Err(error) => return error_response(error),
    };
    let result = target
        .backend
        .prepare(TicketPortalPrepareHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: &parsed.headers,
            input: &input,
            body: &parsed.body,
            key: &parsed.key,
        })
        .await;
    match result.and_then(response_value) {
        Ok(value) => created(value),
        Err(error) => error_response(error),
    }
}

async fn decide(
    State(state): State<Arc<TicketPortalHttpState>>,
    Path((slug, binding_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let target = match selected_target(&state.targets, request.headers(), &slug) {
        Ok(target) => target,
        Err(error) => return error_response(error),
    };
    if let Err(error) = no_query(request.uri().query()) {
        return error_response(error);
    }
    if !valid_path_id(&binding_id) {
        return error_response(validation_failed());
    }
    let parsed = match parse_command(
        request,
        &target.origin,
        OriginPolicy::Required,
        PORTAL_BODY_LIMIT,
    )
    .await
    {
        Ok(parsed) => parsed,
        Err(error) => return error_response(error),
    };
    let input = match parse_decision(&parsed.body) {
        Ok(input) => input,
        Err(error) => return error_response(error),
    };
    let result = target
        .backend
        .decide(TicketPortalDecisionHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: &parsed.headers,
            binding_id: &binding_id,
            input: &input,
            body: &parsed.body,
            key: &parsed.key,
        })
        .await;
    match result.and_then(response_value) {
        Ok(value) => ok(value),
        Err(error) => error_response(error),
    }
}

async fn respond(
    State(state): State<Arc<TicketPortalHttpState>>,
    Path((slug, binding_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let target = match selected_target(&state.targets, request.headers(), &slug) {
        Ok(target) => target,
        Err(error) => return error_response(error),
    };
    if let Err(error) = no_query(request.uri().query()) {
        return error_response(error);
    }
    if !valid_path_id(&binding_id) {
        return error_response(validation_failed());
    }
    let parsed = match parse_command(
        request,
        &target.origin,
        OriginPolicy::Required,
        PORTAL_BODY_LIMIT,
    )
    .await
    {
        Ok(parsed) => parsed,
        Err(error) => return error_response(error),
    };
    let input = match parse_response(&parsed.body) {
        Ok(input) => input,
        Err(error) => return error_response(error),
    };
    let result = target
        .backend
        .respond(TicketPortalResponseHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: &parsed.headers,
            binding_id: &binding_id,
            input: &input,
            body: &parsed.body,
            key: &parsed.key,
        })
        .await;
    match result.and_then(response_value) {
        Ok(value) => ok(value),
        Err(error) => error_response(error),
    }
}

async fn customer_projection(
    State(state): State<Arc<TicketPortalHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let target = match selected_target(&state.targets, request.headers(), &slug) {
        Ok(target) => target,
        Err(error) => return error_response(error),
    };
    if let Err(error) = no_query(request.uri().query()) {
        return error_response(error);
    }
    let result = target
        .backend
        .customer_projection(TicketPortalCustomerHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: request.headers(),
        })
        .await;
    match result.and_then(response_value) {
        Ok(value) => ok(value),
        Err(error) => error_response(error),
    }
}

async fn acknowledge(
    State(state): State<Arc<TicketPortalHttpState>>,
    Path((slug, binding_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let target = match selected_target(&state.targets, request.headers(), &slug) {
        Ok(target) => target,
        Err(error) => return error_response(error),
    };
    if let Err(error) = no_query(request.uri().query()) {
        return error_response(error);
    }
    if !valid_path_id(&binding_id) {
        return error_response(validation_failed());
    }
    let parsed = match parse_command(
        request,
        &target.origin,
        OriginPolicy::Required,
        PORTAL_BODY_LIMIT,
    )
    .await
    {
        Ok(parsed) => parsed,
        Err(error) => return error_response(error),
    };
    let input = match parse_acknowledgement(&parsed.body) {
        Ok(input) => input,
        Err(error) => return error_response(error),
    };
    let result = target
        .backend
        .acknowledge(TicketPortalAcknowledgementHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: &parsed.headers,
            binding_id: &binding_id,
            input: &input,
            body: &parsed.body,
            key: &parsed.key,
        })
        .await;
    match result.and_then(response_value) {
        Ok(value) => ok(value),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::support_http::block_on;
    use crate::domain::ticket_portal::{BindingStatus, PortalProjection};
    use erpx_server_runtime::public::http_support::{body::Body, json};
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Default)]
    struct Backend {
        workspaces: AtomicUsize,
        projections: AtomicUsize,
        prepares: AtomicUsize,
        seen_ticket: Mutex<Option<String>>,
        seen_binding: Mutex<Option<String>>,
        seen_key: Mutex<String>,
        seen_input: Mutex<Option<PrepareInput>>,
    }

    impl TicketPortalHttpBackend for Backend {
        fn staff_workspace<'a>(
            &'a self,
            request: TicketPortalWorkspaceHttpRequest<'a>,
        ) -> SupportHttpFuture<'a, PortalWorkspace> {
            self.workspaces.fetch_add(1, Ordering::SeqCst);
            *self.seen_ticket.lock().unwrap() = request.ticket_id.map(str::to_owned);
            *self.seen_binding.lock().unwrap() = request.binding_id.map(str::to_owned);
            Box::pin(async move {
                Ok(PortalWorkspace {
                    company_id: "company".into(),
                    schema_available: true,
                    available: true,
                    context_key: Some("context".into()),
                    can_prepare: true,
                    ticket: None,
                    bindings: vec![],
                })
            })
        }

        fn customer_projection<'a>(
            &'a self,
            _request: TicketPortalCustomerHttpRequest<'a>,
        ) -> SupportHttpFuture<'a, PortalProjection> {
            self.projections.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Ok(PortalProjection {
                    schema_available: true,
                    context_key: "context".into(),
                    items: vec![],
                })
            })
        }

        fn prepare<'a>(
            &'a self,
            request: TicketPortalPrepareHttpRequest<'a>,
        ) -> SupportHttpFuture<'a, PrepareResult> {
            self.prepares.fetch_add(1, Ordering::SeqCst);
            *self.seen_key.lock().unwrap() = request.key.as_str().to_owned();
            *self.seen_input.lock().unwrap() = Some(request.input.clone());
            Box::pin(async move {
                Ok(PrepareResult {
                    id: "support-portal-command-0001".into(),
                    status: "draft",
                    version: 1,
                })
            })
        }

        fn decide<'a>(
            &'a self,
            _request: TicketPortalDecisionHttpRequest<'a>,
        ) -> SupportHttpFuture<'a, DecisionResult> {
            Box::pin(async move {
                Ok(DecisionResult {
                    id: "support-portal-command-0001".into(),
                    status: BindingStatus::Approved,
                    version: 2,
                })
            })
        }

        fn respond<'a>(
            &'a self,
            _request: TicketPortalResponseHttpRequest<'a>,
        ) -> SupportHttpFuture<'a, ResponseResult> {
            Box::pin(async move {
                Ok(ResponseResult {
                    id: "response-1".into(),
                    binding_id: "support-portal-command-0001".into(),
                    version: 1,
                    status: "published",
                    message: "Repaired.".into(),
                    published_at: 1,
                })
            })
        }

        fn acknowledge<'a>(
            &'a self,
            _request: TicketPortalAcknowledgementHttpRequest<'a>,
        ) -> SupportHttpFuture<'a, AcknowledgementResult> {
            Box::pin(async move {
                Ok(AcknowledgementResult {
                    id: "ack-1".into(),
                    binding_id: "support-portal-command-0001".into(),
                    response_id: "response-1".into(),
                    response_version: 1,
                    status: "acknowledged",
                    created_at: 1,
                })
            })
        }
    }

    fn target(backend: Arc<Backend>) -> TicketPortalHttpTarget {
        TicketPortalHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
        }
    }

    fn state(backend: Arc<Backend>) -> Arc<TicketPortalHttpState> {
        let target = target(backend);
        Arc::new(TicketPortalHttpState {
            targets: BTreeMap::from([((target.host.clone(), target.holding_slug.clone()), target)]),
        })
    }

    fn request(method: &str, uri: &str, key: Option<&str>, origin: Option<&str>) -> Request {
        let body = if method == "POST" {
            json!({
                "ticketId":"ticket-1","ticketVersion":1,"customerName":"Customer",
                "email":"Customer@Example.Test","subject":"Printer update",
                "summary":"We are reviewing the printer issue.","expiresAt":6000,
                "reason":"Customer asked for an update"
            })
            .to_string()
        } else {
            String::new()
        };
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("host", "acme.example.test");
        if let Some(key) = key {
            builder = builder.header("idempotency-key", key);
        }
        if let Some(origin) = origin {
            builder = builder.header("origin", origin);
        }
        builder.body(Body::from(body)).unwrap()
    }

    #[test]
    fn prepare_requires_exact_origin_key_and_validated_body() {
        let backend = Arc::new(Backend::default());
        let state = state(backend.clone());
        let response = block_on(prepare(
            State(state.clone()),
            Path("acme".into()),
            request("POST", "/prepare", Some("portal-command-0001"), None),
        ));
        assert_eq!(response.status(), 403);
        let response = block_on(prepare(
            State(state.clone()),
            Path("acme".into()),
            request("POST", "/prepare", None, Some("https://acme.example.test")),
        ));
        assert_eq!(response.status(), 422);
        assert_eq!(backend.prepares.load(Ordering::SeqCst), 0);
        let response = block_on(prepare(
            State(state),
            Path("acme".into()),
            request(
                "POST",
                "/prepare",
                Some("portal-command-0001"),
                Some("https://acme.example.test"),
            ),
        ));
        assert_eq!(response.status(), 201);
        assert_eq!(backend.prepares.load(Ordering::SeqCst), 1);
        assert_eq!(
            backend.seen_input.lock().unwrap().as_ref().unwrap().email,
            "customer@example.test"
        );
        assert_eq!(
            backend.seen_key.lock().unwrap().as_str(),
            "portal-command-0001"
        );
    }

    #[test]
    fn workspace_accepts_one_ticket_selector_and_no_binding_query() {
        let backend = Arc::new(Backend::default());
        let state = state(backend.clone());
        let response = block_on(staff_workspace(
            State(state.clone()),
            Path("acme".into()),
            request("GET", "/workspace?ticketId=ticket-1", None, None),
        ));
        assert_eq!(response.status(), 200);
        assert_eq!(
            backend.seen_ticket.lock().unwrap().as_deref(),
            Some("ticket-1")
        );

        let response = block_on(staff_binding_workspace(
            State(state.clone()),
            Path(("acme".into(), "support-portal-command-0001".into())),
            request("GET", "/workspace/binding?ticketId=ticket-1", None, None),
        ));
        assert_eq!(response.status(), 404);

        let response = block_on(staff_binding_workspace(
            State(state),
            Path(("acme".into(), "support-portal-command-0001".into())),
            request("GET", "/workspace/binding", None, None),
        ));
        assert_eq!(response.status(), 200);
        assert_eq!(
            backend.seen_binding.lock().unwrap().as_deref(),
            Some("support-portal-command-0001")
        );
    }

    #[test]
    fn customer_projection_and_acknowledgement_use_exact_origin() {
        let backend = Arc::new(Backend::default());
        let state = state(backend.clone());
        let response = block_on(customer_projection(
            State(state.clone()),
            Path("acme".into()),
            request("GET", "/tickets", None, None),
        ));
        assert_eq!(response.status(), 200);
        assert_eq!(backend.projections.load(Ordering::SeqCst), 1);

        let ack = json!({
            "expectedBindingVersion":2,
            "responseId":"response-1",
            "responseVersion":1,
            "note":"Thank you"
        })
        .to_string();
        let build = |origin: Option<&str>| {
            let mut builder = Request::builder()
                .method("POST")
                .uri("/tickets/support-portal-command-0001/acknowledgements")
                .header("host", "acme.example.test")
                .header("idempotency-key", "ack-key-00000001");
            if let Some(origin) = origin {
                builder = builder.header("origin", origin);
            }
            builder.body(Body::from(ack.clone())).unwrap()
        };
        let response = block_on(acknowledge(
            State(state.clone()),
            Path(("acme".into(), "support-portal-command-0001".into())),
            build(None),
        ));
        assert_eq!(response.status(), 403);
        let response = block_on(acknowledge(
            State(state),
            Path(("acme".into(), "support-portal-command-0001".into())),
            build(Some("https://acme.example.test")),
        ));
        assert_eq!(response.status(), 200);
    }

    #[test]
    fn duplicate_targets_are_rejected() {
        let backend = Arc::new(Backend::default());
        let error = match ticket_portal_http_router(vec![target(backend.clone()), target(backend)])
        {
            Ok(_) => panic!("duplicate target accepted"),
            Err(error) => error,
        };
        assert_eq!(error.code, "SUPPORT_PORTAL_HTTP_CONFIGURATION_INVALID");
    }
}
