//! Native Support service-team HTTP ingress.
//!
//! Transport facts only: registered Host/Holding targets, optional exact origin,
//! idempotency, bounded JSON and business-error mapping. The composition backend
//! resolves the current Account proof, Holding-owner authority, Organization
//! membership/scope, People category/member facts and the command unit. The
//! router performs no SQL and imports no composition module.
use crate::adapters::support_http::{
    OriginPolicy, SupportHttpFuture, SupportHttpTargetSpec, configure_targets, error_response,
    ok, parse_command, parse_json_body, selected_target, validation_failed,
};
use crate::application::service_teams::{ServiceTeamPreview, ServiceTeamResult};
use erpx_server_runtime::public::http_support::{
    HeaderMap, Path, Request, Response, Router, State,
    routing::{get, post},
    serde_json::Value,
};
use erpx_server_shared::public::{BusinessError, CommandKey};
use std::{collections::BTreeMap, sync::Arc};

const SERVICE_TEAM_BODY_LIMIT: usize = 192_000;

pub struct ServiceTeamOptionsHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

pub struct ServiceTeamListHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

pub struct ServiceTeamPreviewHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub id: Option<&'a str>,
    pub body: &'a Value,
}

pub struct ServiceTeamSaveHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub id: Option<&'a str>,
    pub body: &'a Value,
    pub key: &'a CommandKey,
}

/// Composition-owned owner/session/scope/command adapter. The router never
/// mints a session, actor, Company, Holding owner or category catalogue.
pub trait ServiceTeamHttpBackend: Send + Sync {
    fn options<'a>(
        &'a self,
        request: ServiceTeamOptionsHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, Value>;

    fn list<'a>(&'a self, request: ServiceTeamListHttpRequest<'a>) -> SupportHttpFuture<'a, Value>;

    fn preview<'a>(
        &'a self,
        request: ServiceTeamPreviewHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, ServiceTeamPreview>;

    fn save<'a>(
        &'a self,
        request: ServiceTeamSaveHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, ServiceTeamResult>;
}

pub struct ServiceTeamHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn ServiceTeamHttpBackend>,
}

impl SupportHttpTargetSpec for ServiceTeamHttpTarget {
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

struct ServiceTeamHttpState {
    targets: BTreeMap<(String, String), ServiceTeamHttpTarget>,
}

pub fn service_teams_http_router(
    targets: Vec<ServiceTeamHttpTarget>,
) -> Result<Router, BusinessError> {
    let configured = configure_targets(targets, "SERVICE_TEAMS_HTTP_CONFIGURATION_INVALID")?;
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/service-teams",
            get(options).post(save_create),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/service-teams/list",
            get(list),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/service-teams/preview",
            post(preview_create),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/service-teams/{id}",
            post(save_update),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/service-teams/{id}/preview",
            post(preview_update),
        )
        .with_state(Arc::new(ServiceTeamHttpState {
            targets: configured,
        })))
}

fn response_value<T: serde::Serialize>(value: T) -> Result<Value, BusinessError> {
    erpx_server_runtime::public::http_support::serde_json::to_value(value).map_err(|_| {
        BusinessError::invalid("SUPPORT_RESPONSE_INVALID", "Invalid Support response.")
    })
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 200 && !value.chars().any(char::is_control)
}

async fn options(
    State(state): State<Arc<ServiceTeamHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let target = match selected_target(&state.targets, request.headers(), &slug) {
        Ok(target) => target,
        Err(error) => return error_response(error),
    };
    match target
        .backend
        .options(ServiceTeamOptionsHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: request.headers(),
        })
        .await
    {
        Ok(value) => ok(value),
        Err(error) => error_response(error),
    }
}

async fn list(
    State(state): State<Arc<ServiceTeamHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let target = match selected_target(&state.targets, request.headers(), &slug) {
        Ok(target) => target,
        Err(error) => return error_response(error),
    };
    match target
        .backend
        .list(ServiceTeamListHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: request.headers(),
        })
        .await
    {
        Ok(value) => ok(value),
        Err(error) => error_response(error),
    }
}

async fn save_create(
    State(state): State<Arc<ServiceTeamHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    save(state, slug, None, request).await
}

async fn save_update(
    State(state): State<Arc<ServiceTeamHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    if !valid_id(&id) {
        return error_response(validation_failed());
    }
    save(state, slug, Some(id), request).await
}

async fn save(
    state: Arc<ServiceTeamHttpState>,
    slug: String,
    id: Option<String>,
    request: Request,
) -> Response {
    let target = match selected_target(&state.targets, request.headers(), &slug) {
        Ok(target) => target,
        Err(error) => return error_response(error),
    };
    let parsed = match parse_command(
        request,
        &target.origin,
        OriginPolicy::Optional,
        SERVICE_TEAM_BODY_LIMIT,
    )
    .await
    {
        Ok(parsed) => parsed,
        Err(error) => return error_response(error),
    };
    let result = target
        .backend
        .save(ServiceTeamSaveHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: &parsed.headers,
            id: id.as_deref(),
            body: &parsed.body,
            key: &parsed.key,
        })
        .await;
    match result.and_then(response_value) {
        Ok(value) => ok(value),
        Err(error) => error_response(error),
    }
}

async fn preview_create(
    State(state): State<Arc<ServiceTeamHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    preview(state, slug, None, request).await
}

async fn preview_update(
    State(state): State<Arc<ServiceTeamHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    if !valid_id(&id) {
        return error_response(validation_failed());
    }
    preview(state, slug, Some(id), request).await
}

async fn preview(
    state: Arc<ServiceTeamHttpState>,
    slug: String,
    id: Option<String>,
    request: Request,
) -> Response {
    let target = match selected_target(&state.targets, request.headers(), &slug) {
        Ok(target) => target,
        Err(error) => return error_response(error),
    };
    let parsed = match parse_json_body(
        request,
        &target.origin,
        OriginPolicy::Optional,
        SERVICE_TEAM_BODY_LIMIT,
    )
    .await
    {
        Ok(parsed) => parsed,
        Err(error) => return error_response(error),
    };
    let body_id = match parsed.body.get("id").and_then(Value::as_str) {
        Some(value) if valid_id(value) => Some(value),
        _ => None,
    };
    let selected = id.as_deref().or(body_id);
    let result = target
        .backend
        .preview(ServiceTeamPreviewHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: &parsed.headers,
            id: selected,
            body: &parsed.body,
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
    use erpx_server_runtime::public::http_support::{body::Body, json};
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Default)]
    struct Backend {
        options: AtomicUsize,
        list: AtomicUsize,
        previews: AtomicUsize,
        saves: AtomicUsize,
        seen_id: Mutex<Option<String>>,
        seen_key: Mutex<String>,
        seen_body: Mutex<Value>,
    }

    impl ServiceTeamHttpBackend for Backend {
        fn options<'a>(
            &'a self,
            _request: ServiceTeamOptionsHttpRequest<'a>,
        ) -> SupportHttpFuture<'a, Value> {
            self.options.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(json!({"owner": true, "teams": []})) })
        }

        fn list<'a>(
            &'a self,
            _request: ServiceTeamListHttpRequest<'a>,
        ) -> SupportHttpFuture<'a, Value> {
            self.list.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(json!({"teams": []})) })
        }

        fn preview<'a>(
            &'a self,
            request: ServiceTeamPreviewHttpRequest<'a>,
        ) -> SupportHttpFuture<'a, ServiceTeamPreview> {
            self.previews.fetch_add(1, Ordering::SeqCst);
            *self.seen_id.lock().unwrap() = request.id.map(str::to_owned);
            *self.seen_body.lock().unwrap() = request.body.clone();
            Box::pin(async move {
                Ok(ServiceTeamPreview {
                    previous_version: 0,
                    new_version: 1,
                    previous: None,
                    proposed: json!({"name": "Desk team"}),
                    warnings: vec![],
                    effect: "New intake uses current coverage.",
                })
            })
        }

        fn save<'a>(
            &'a self,
            request: ServiceTeamSaveHttpRequest<'a>,
        ) -> SupportHttpFuture<'a, ServiceTeamResult> {
            self.saves.fetch_add(1, Ordering::SeqCst);
            *self.seen_id.lock().unwrap() = request.id.map(str::to_owned);
            *self.seen_key.lock().unwrap() = request.key.as_str().to_owned();
            *self.seen_body.lock().unwrap() = request.body.clone();
            let id = request.id.unwrap_or("team-command-0001").to_owned();
            Box::pin(async move { Ok(ServiceTeamResult { id, version: 1 }) })
        }
    }

    fn target(backend: Arc<Backend>) -> ServiceTeamHttpTarget {
        ServiceTeamHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
        }
    }

    fn state(backend: Arc<Backend>) -> Arc<ServiceTeamHttpState> {
        let target = target(backend);
        Arc::new(ServiceTeamHttpState {
            targets: BTreeMap::from([((target.host.clone(), target.holding_slug.clone()), target)]),
        })
    }

    fn team_body() -> String {
        json!({
            "name":"Desk team","active":true,"expectedVersion":1,
            "members":[{"userId":"lead","responsibility":"lead"}],
            "coverage":[{"category":"it","scopeKind":"holding","scopeNodeId":"company"}]
        })
        .to_string()
    }

    fn post(uri: &str, key: Option<&str>, origin: Option<&str>) -> Request {
        let mut builder = Request::builder()
            .method("POST")
            .uri(uri)
            .header("host", "acme.example.test");
        if let Some(key) = key {
            builder = builder.header("idempotency-key", key);
        }
        if let Some(origin) = origin {
            builder = builder.header("origin", origin);
        }
        builder.body(Body::from(team_body())).unwrap()
    }

    #[test]
    fn options_and_list_are_plain_reads() {
        let backend = Arc::new(Backend::default());
        let state = state(backend.clone());
        let response = block_on(options(
            State(state.clone()),
            Path("acme".into()),
            Request::builder()
                .uri("/service-teams?ticketId=ticket-1")
                .header("host", "acme.example.test")
                .body(Body::empty())
                .unwrap(),
        ));
        assert_eq!(response.status(), 200);
        assert_eq!(backend.options.load(Ordering::SeqCst), 1);
        let response = block_on(options(
            State(state.clone()),
            Path("acme".into()),
            Request::builder()
                .uri("/service-teams")
                .header("host", "acme.example.test")
                .body(Body::empty())
                .unwrap(),
        ));
        assert_eq!(response.status(), 200);
        let response = block_on(list(
            State(state),
            Path("acme".into()),
            Request::builder()
                .uri("/service-teams/list")
                .header("host", "acme.example.test")
                .body(Body::empty())
                .unwrap(),
        ));
        assert_eq!(response.status(), 200);
        assert_eq!(backend.options.load(Ordering::SeqCst), 2);
        assert_eq!(backend.list.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn save_accepts_optional_origin_but_requires_key() {
        let backend = Arc::new(Backend::default());
        let state = state(backend.clone());
        let response = block_on(save_create(
            State(state.clone()),
            Path("acme".into()),
            post("/service-teams", None, None),
        ));
        assert_eq!(response.status(), 422);
        let response = block_on(save_create(
            State(state.clone()),
            Path("acme".into()),
            post(
                "/service-teams",
                Some("service-team-0001"),
                Some("https://other.test"),
            ),
        ));
        assert_eq!(response.status(), 403);
        let response = block_on(save_create(
            State(state.clone()),
            Path("acme".into()),
            post("/service-teams", Some("service-team-0001"), None),
        ));
        assert_eq!(response.status(), 200);
        assert_eq!(backend.saves.load(Ordering::SeqCst), 1);
        assert!(backend.seen_id.lock().unwrap().is_none());
        assert_eq!(
            backend.seen_body.lock().unwrap()["name"].as_str(),
            Some("Desk team")
        );
        let response = block_on(save_update(
            State(state),
            Path(("acme".into(), "team-command-0001".into())),
            post(
                "/service-teams/team-command-0001",
                Some("service-team-0002"),
                None,
            ),
        ));
        assert_eq!(response.status(), 200);
        assert_eq!(
            backend.seen_id.lock().unwrap().as_deref(),
            Some("team-command-0001")
        );
    }

    #[test]
    fn preview_is_available_with_and_without_current_team() {
        let backend = Arc::new(Backend::default());
        let state = state(backend.clone());
        let response = block_on(preview_create(
            State(state.clone()),
            Path("acme".into()),
            post("/service-teams/preview", None, None),
        ));
        assert_eq!(response.status(), 200);
        assert!(backend.seen_id.lock().unwrap().is_none());
        let response = block_on(preview_update(
            State(state),
            Path(("acme".into(), "team-command-0001".into())),
            post("/service-teams/team-command-0001/preview", None, None),
        ));
        assert_eq!(response.status(), 200);
        assert_eq!(
            backend.seen_id.lock().unwrap().as_deref(),
            Some("team-command-0001")
        );
        assert_eq!(backend.previews.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn duplicate_targets_are_rejected() {
        let backend = Arc::new(Backend::default());
        let error = match service_teams_http_router(vec![target(backend.clone()), target(backend)])
        {
            Ok(_) => panic!("duplicate target accepted"),
            Err(error) => error,
        };
        assert_eq!(error.code, "SERVICE_TEAMS_HTTP_CONFIGURATION_INVALID");
    }
}
