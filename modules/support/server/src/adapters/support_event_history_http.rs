//! Native Support ticket/incident event-history HTTP ingress.
//!
//! Transport facts only: registered Host/Holding targets, bounded event
//! selectors and no-selector GETs. Account proof, selected Support scope, the
//! serializable owner transaction and the complete event payloads belong to the
//! composition backend. No SQL lives here.
use crate::adapters::support_http::{
    SupportHttpFuture, SupportHttpTargetSpec, configure_targets, error_response, no_query, ok,
    selected_target, validation_failed,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, Path, Request, Response, Router, State, routing::get, serde_json::Value,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{collections::BTreeMap, sync::Arc};

pub struct TicketHistoryHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub id: &'a str,
}

pub struct IncidentHistoryHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub id: &'a str,
}

/// Composition-owned session/scope/transaction adapter. Both values are full
/// event arrays produced by the owner SQL adapter; the router cannot shape them.
pub trait SupportEventHistoryHttpBackend: Send + Sync {
    fn ticket_history<'a>(
        &'a self,
        request: TicketHistoryHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, Value>;

    fn incident_history<'a>(
        &'a self,
        request: IncidentHistoryHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, Value>;
}

pub struct SupportEventHistoryHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn SupportEventHistoryHttpBackend>,
}

impl SupportHttpTargetSpec for SupportEventHistoryHttpTarget {
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

struct SupportEventHistoryHttpState {
    targets: BTreeMap<(String, String), SupportEventHistoryHttpTarget>,
}

/// Router for the Support ticket/incident event-history reads. Composition
/// mounts this on the private native listener; the TypeScript plan/execute
/// authority is retired only after the browser reaches these handlers directly.
pub fn support_event_history_http_router(
    targets: Vec<SupportEventHistoryHttpTarget>,
) -> Result<Router, BusinessError> {
    let configured = configure_targets(targets, "SUPPORT_EVENT_HISTORY_HTTP_CONFIG_INVALID")?;
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/support/tickets/{id}/events",
            get(ticket_history),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/support/incidents/{id}/events",
            get(incident_history),
        )
        .with_state(Arc::new(SupportEventHistoryHttpState {
            targets: configured,
        })))
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 200 && !value.chars().any(char::is_control)
}

async fn ticket_history(
    State(state): State<Arc<SupportEventHistoryHttpState>>,
    Path((slug, id)): Path<(String, String)>,
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
    match target
        .backend
        .ticket_history(TicketHistoryHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: request.headers(),
            id: &id,
        })
        .await
    {
        Ok(value) => ok(value),
        Err(error) => error_response(error),
    }
}

async fn incident_history(
    State(state): State<Arc<SupportEventHistoryHttpState>>,
    Path((slug, id)): Path<(String, String)>,
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
    match target
        .backend
        .incident_history(IncidentHistoryHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: request.headers(),
            id: &id,
        })
        .await
    {
        Ok(value) => ok(value),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::support_http::block_on;
    use erpx_platform_runtime_server::public::http_support::body::Body;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Backend {
        tickets: AtomicUsize,
        incidents: AtomicUsize,
    }

    impl SupportEventHistoryHttpBackend for Backend {
        fn ticket_history<'a>(
            &'a self,
            _request: TicketHistoryHttpRequest<'a>,
        ) -> SupportHttpFuture<'a, Value> {
            self.tickets.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                Ok(serde_json::json!([{"id":"ticket-1:1","ticket_id":"ticket-1","version":1}]))
            })
        }

        fn incident_history<'a>(
            &'a self,
            _request: IncidentHistoryHttpRequest<'a>,
        ) -> SupportHttpFuture<'a, Value> {
            self.incidents.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                Ok(serde_json::json!([{"id":"incident-1:1","incident_id":"incident-1","version":1}]))
            })
        }
    }

    fn target(backend: Arc<Backend>) -> SupportEventHistoryHttpTarget {
        SupportEventHistoryHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
        }
    }

    fn state(backend: Arc<Backend>) -> Arc<SupportEventHistoryHttpState> {
        Arc::new(SupportEventHistoryHttpState {
            targets: BTreeMap::from([(
                ("acme.example.test".to_owned(), "acme".to_owned()),
                target(backend),
            )]),
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
    fn history_reads_are_bounded_selector_gets() {
        let backend = Arc::new(Backend::default());
        let state = state(backend.clone());
        let response = block_on(ticket_history(
            State(state.clone()),
            Path(("acme".into(), "ticket-1".into())),
            get("/support/tickets/ticket-1/events?version=1"),
        ));
        assert_eq!(response.status(), 422);
        assert_eq!(backend.tickets.load(Ordering::SeqCst), 0);

        let response = block_on(ticket_history(
            State(state.clone()),
            Path(("acme".into(), "ticket-1".into())),
            get("/support/tickets/ticket-1/events"),
        ));
        assert_eq!(response.status(), 200);
        assert_eq!(backend.tickets.load(Ordering::SeqCst), 1);

        let response = block_on(incident_history(
            State(state.clone()),
            Path(("acme".into(), "incident-1".into())),
            get("/support/incidents/incident-1/events"),
        ));
        assert_eq!(response.status(), 200);
        assert_eq!(backend.incidents.load(Ordering::SeqCst), 1);

        let response = block_on(incident_history(
            State(state),
            Path(("acme".into(), "".into())),
            get("/support/incidents//events"),
        ));
        assert_eq!(response.status(), 422);
        assert_eq!(backend.incidents.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn duplicate_targets_are_rejected() {
        let backend = Arc::new(Backend::default());
        let error = match support_event_history_http_router(vec![
            target(backend.clone()),
            target(backend),
        ]) {
            Ok(_) => panic!("duplicate target accepted"),
            Err(error) => error,
        };
        assert_eq!(error.code, "SUPPORT_EVENT_HISTORY_HTTP_CONFIG_INVALID");
    }
}
