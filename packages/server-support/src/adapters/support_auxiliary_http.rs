//! Native Support auxiliary-read HTTP ingress for Sales and Collaboration.
//!
//! Transport facts only: registered Host/Holding targets, bounded identifiers,
//! no-selector GETs and business-error mapping. Account proof, selected scope,
//! purchase gates and the owner transaction belong to the composition backend,
//! which must implement [`SupportAuxiliaryHttpBackend`]. No SQL or composition
//! import lives here.
use crate::adapters::support_auxiliary::{
    SupportAlertSourceSnapshot, SupportIncidentSourceSnapshot, SupportPolicySourceSnapshot,
    SupportSalesWorkspace, SupportSessionSourceSnapshot, SupportTicketSourceSnapshot,
};
use crate::adapters::support_http::{
    SupportHttpFuture, SupportHttpTargetSpec, configure_targets, error_response, no_query,
    not_found_source, ok, selected_target, validation_failed,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, Path, Request, Response, Router, State, routing::get, serde_json::Value,
};
use erpx_server_shared::public::BusinessError;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupportAuxiliarySourceKind {
    Ticket,
    Policy,
    Alert,
    Incident,
    Session,
}

pub struct SupportAuxiliaryReadHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub id: &'a str,
    pub kind: SupportAuxiliarySourceKind,
}

pub struct SupportAuxiliaryWorkspaceHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

pub enum SupportAuxiliarySourceReply {
    Ticket(SupportTicketSourceSnapshot),
    Policy(SupportPolicySourceSnapshot),
    Alert(SupportAlertSourceSnapshot),
    Incident(SupportIncidentSourceSnapshot),
    Session(SupportSessionSourceSnapshot),
}

impl SupportAuxiliarySourceReply {
    fn value(self) -> Result<Value, BusinessError> {
        let serialized = match self {
            Self::Ticket(value) => {
                erpx_server_runtime::public::http_support::serde_json::to_value(value)
            }
            Self::Policy(value) => {
                erpx_server_runtime::public::http_support::serde_json::to_value(value)
            }
            Self::Alert(value) => {
                erpx_server_runtime::public::http_support::serde_json::to_value(value)
            }
            Self::Incident(value) => {
                erpx_server_runtime::public::http_support::serde_json::to_value(value)
            }
            Self::Session(value) => {
                erpx_server_runtime::public::http_support::serde_json::to_value(value)
            }
        };
        serialized.map_err(|_| {
            BusinessError::invalid("SUPPORT_RESPONSE_INVALID", "Invalid Support response.")
        })
    }
}

/// Composition-owned session/scope/purchase adapter. Optional sources are
/// returned only when the current Support scope reveals them.
pub trait SupportAuxiliaryHttpBackend: Send + Sync {
    fn workspace<'a>(
        &'a self,
        request: SupportAuxiliaryWorkspaceHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, SupportSalesWorkspace>;

    fn source<'a>(
        &'a self,
        request: SupportAuxiliaryReadHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, Option<SupportAuxiliarySourceReply>>;
}

pub struct SupportAuxiliaryHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn SupportAuxiliaryHttpBackend>,
}

impl SupportHttpTargetSpec for SupportAuxiliaryHttpTarget {
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

struct SupportAuxiliaryHttpState {
    targets: BTreeMap<(String, String), SupportAuxiliaryHttpTarget>,
}

pub fn support_auxiliary_http_router(
    targets: Vec<SupportAuxiliaryHttpTarget>,
) -> Result<Router, BusinessError> {
    let configured = configure_targets(targets, "SUPPORT_AUXILIARY_HTTP_CONFIGURATION_INVALID")?;
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/support/auxiliary/workspace",
            get(workspace),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/support/auxiliary/tickets/{id}",
            get(ticket),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/support/auxiliary/policies/{id}",
            get(policy),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/support/auxiliary/alerts/{id}",
            get(alert),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/support/auxiliary/incidents/{id}",
            get(incident),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/support/auxiliary/sessions/{id}",
            get(session),
        )
        .with_state(Arc::new(SupportAuxiliaryHttpState {
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

async fn workspace(
    State(state): State<Arc<SupportAuxiliaryHttpState>>,
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
    match target
        .backend
        .workspace(SupportAuxiliaryWorkspaceHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: request.headers(),
        })
        .await
        .and_then(response_value)
    {
        Ok(value) => ok(value),
        Err(error) => error_response(error),
    }
}

async fn ticket(
    State(state): State<Arc<SupportAuxiliaryHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    source(state, slug, id, SupportAuxiliarySourceKind::Ticket, request).await
}

async fn policy(
    State(state): State<Arc<SupportAuxiliaryHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    source(state, slug, id, SupportAuxiliarySourceKind::Policy, request).await
}

async fn alert(
    State(state): State<Arc<SupportAuxiliaryHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    source(state, slug, id, SupportAuxiliarySourceKind::Alert, request).await
}

async fn incident(
    State(state): State<Arc<SupportAuxiliaryHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    source(
        state,
        slug,
        id,
        SupportAuxiliarySourceKind::Incident,
        request,
    )
    .await
}

async fn session(
    State(state): State<Arc<SupportAuxiliaryHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    source(
        state,
        slug,
        id,
        SupportAuxiliarySourceKind::Session,
        request,
    )
    .await
}

async fn source(
    state: Arc<SupportAuxiliaryHttpState>,
    slug: String,
    id: String,
    kind: SupportAuxiliarySourceKind,
    request: Request,
) -> Response {
    let target = match selected_target(&state.targets, request.headers(), &slug) {
        Ok(target) => target,
        Err(error) => return error_response(error),
    };
    if let Err(error) = no_query(request.uri().query()) {
        return error_response(error);
    }
    if !valid_id(&id) {
        return error_response(validation_failed());
    }
    let result = target
        .backend
        .source(SupportAuxiliaryReadHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: request.headers(),
            id: &id,
            kind,
        })
        .await;
    match result {
        Ok(Some(reply)) => match reply.value() {
            Ok(value) => ok(value),
            Err(error) => error_response(error),
        },
        Ok(None) => error_response(not_found_source()),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::support_http::block_on;
    use erpx_server_runtime::public::http_support::body::Body;
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Default)]
    struct Backend {
        workspaces: AtomicUsize,
        sources: AtomicUsize,
        seen_kind: Mutex<Option<SupportAuxiliarySourceKind>>,
        has_ticket: AtomicUsize,
    }

    impl SupportAuxiliaryHttpBackend for Backend {
        fn workspace<'a>(
            &'a self,
            _request: SupportAuxiliaryWorkspaceHttpRequest<'a>,
        ) -> SupportHttpFuture<'a, SupportSalesWorkspace> {
            self.workspaces.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Ok(SupportSalesWorkspace {
                    items: vec![],
                    sources: vec![],
                    can_create: true,
                    can_create_order: true,
                    own_reports_only: false,
                })
            })
        }

        fn source<'a>(
            &'a self,
            request: SupportAuxiliaryReadHttpRequest<'a>,
        ) -> SupportHttpFuture<'a, Option<SupportAuxiliarySourceReply>> {
            self.sources.fetch_add(1, Ordering::SeqCst);
            *self.seen_kind.lock().unwrap() = Some(request.kind);
            let ticket = self.has_ticket.load(Ordering::SeqCst) == 1;
            let id = request.id.to_owned();
            Box::pin(async move {
                if !ticket {
                    return Ok(None);
                }
                Ok(Some(SupportAuxiliarySourceReply::Ticket(
                    SupportTicketSourceSnapshot {
                        id,
                        version: 2,
                        title: "Printer offline".into(),
                        status: "open".into(),
                        source_kind: "general".into(),
                        owner_id: Some("manager".into()),
                        order_id: None,
                        due_date: Some("2026-11-10".into()),
                    },
                )))
            })
        }
    }

    fn target(backend: Arc<Backend>) -> SupportAuxiliaryHttpTarget {
        SupportAuxiliaryHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
        }
    }

    fn state(backend: Arc<Backend>) -> Arc<SupportAuxiliaryHttpState> {
        let target = target(backend);
        Arc::new(SupportAuxiliaryHttpState {
            targets: BTreeMap::from([((target.host.clone(), target.holding_slug.clone()), target)]),
        })
    }

    fn get(uri: &str) -> Request {
        Request::builder()
            .uri(uri)
            .header("host", "acme.example.test")
            .body(Body::empty())
            .unwrap()
    }

    #[test]
    fn workspace_is_a_no_selector_read() {
        let backend = Arc::new(Backend::default());
        let state = state(backend.clone());
        let response = block_on(workspace(
            State(state.clone()),
            Path("acme".into()),
            get("/support/auxiliary/workspace?ticketId=ticket-1"),
        ));
        assert_eq!(response.status(), 422);
        let response = block_on(workspace(
            State(state),
            Path("acme".into()),
            get("/support/auxiliary/workspace"),
        ));
        assert_eq!(response.status(), 200);
        assert_eq!(backend.workspaces.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn notification_sources_map_hidden_records_to_not_found() {
        let backend = Arc::new(Backend::default());
        let state = state(backend.clone());
        let response = block_on(ticket(
            State(state.clone()),
            Path(("acme".into(), "ticket-1".into())),
            get("/support/auxiliary/tickets/ticket-1"),
        ));
        assert_eq!(response.status(), 404);
        assert_eq!(
            *backend.seen_kind.lock().unwrap(),
            Some(SupportAuxiliarySourceKind::Ticket)
        );
        backend.has_ticket.store(1, Ordering::SeqCst);
        let response = block_on(ticket(
            State(state.clone()),
            Path(("acme".into(), "ticket-1".into())),
            get("/support/auxiliary/tickets/ticket-1"),
        ));
        assert_eq!(response.status(), 200);
        let response = block_on(ticket(
            State(state),
            Path(("acme".into(), "ticket-1".into())),
            get("/support/auxiliary/tickets/ticket-1?expand=all"),
        ));
        assert_eq!(response.status(), 422);
    }

    #[test]
    fn source_kinds_are_passed_to_the_backend() {
        let backend = Arc::new(Backend::default());
        let state = state(backend.clone());
        for (kind, path) in [
            (
                SupportAuxiliarySourceKind::Policy,
                ("acme".to_owned(), "policy-1".to_owned()),
            ),
            (
                SupportAuxiliarySourceKind::Alert,
                ("acme".to_owned(), "alert-1".to_owned()),
            ),
            (
                SupportAuxiliarySourceKind::Incident,
                ("acme".to_owned(), "incident-1".to_owned()),
            ),
            (
                SupportAuxiliarySourceKind::Session,
                ("acme".to_owned(), "session-1".to_owned()),
            ),
        ] {
            let response = match kind {
                SupportAuxiliarySourceKind::Policy => block_on(policy(
                    State(state.clone()),
                    Path(path),
                    get("/support/auxiliary/policies/policy-1"),
                )),
                SupportAuxiliarySourceKind::Alert => block_on(alert(
                    State(state.clone()),
                    Path(path),
                    get("/support/auxiliary/alerts/alert-1"),
                )),
                SupportAuxiliarySourceKind::Incident => block_on(incident(
                    State(state.clone()),
                    Path(path),
                    get("/support/auxiliary/incidents/incident-1"),
                )),
                SupportAuxiliarySourceKind::Session => block_on(session(
                    State(state.clone()),
                    Path(path),
                    get("/support/auxiliary/sessions/session-1"),
                )),
                SupportAuxiliarySourceKind::Ticket => unreachable!(),
            };
            assert_eq!(response.status(), 404);
            assert_eq!(*backend.seen_kind.lock().unwrap(), Some(kind));
        }
        assert_eq!(backend.sources.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn duplicate_targets_are_rejected() {
        let backend = Arc::new(Backend::default());
        let error =
            match support_auxiliary_http_router(vec![target(backend.clone()), target(backend)]) {
                Ok(_) => panic!("duplicate target accepted"),
                Err(error) => error,
            };
        assert_eq!(error.code, "SUPPORT_AUXILIARY_HTTP_CONFIGURATION_INVALID");
    }
}
