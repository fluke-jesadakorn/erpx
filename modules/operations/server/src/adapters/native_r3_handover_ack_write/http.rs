//! Owner-local axum ingress for the native acknowledgement POST. Composition
//! mounts this router on the private service. A request reaches only the pool
//! registered for its exact `(Host, holding_slug)` pair and the injected work
//! context must agree with that target's Company. The retained Next transport
//! and its re-exports are untouched by this module.

use crate::adapters::native_r3_handover_ack::PgHandoverAckSource;
use crate::adapters::native_r3_handover_ack_write::postgres::PgAckWriteStore;
use crate::application::native_r3_handover_ack::{
    HandoverAckRead, HandoverPostSource,
};
use crate::application::native_r3_handover_ack_write::{
    AckCommandLedger, AckWriteHandler,
};
use crate::domain::native_r3_handover_ack_write::{origin_forbidden, validate_key};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");
const VERSION_HEADER: HeaderName = HeaderName::from_static("x-erpx-handover-version");
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AckWriteTarget {
    pub host: String,
    pub holding_slug: String,
    pub company_id: String,
    /// Exact browser origin; a missing or different Origin is refused.
    pub origin: String,
}

struct WriteState {
    company_id: String,
    origin: String,
    handler: AckWriteHandler,
}

struct AckWriteRouterState {
    targets: BTreeMap<(String, String), WriteState>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_HANDOVER_ACK_INVALID",
        "Use one bounded handover acknowledgement write.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_HANDOVER_ACK_CONFIG_INVALID",
        "Use exact distinct registered Holding Company targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn required(headers: &HeaderMap, name: HeaderName) -> Result<String, BusinessError> {
    unique_header(headers, name)?
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(invalid)
}

fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn error_response(error: BusinessError) -> Response {
    (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

/// `canonicalDestinationLinks` is identity on both response shapes: the
/// acknowledgement view and the replay receipt contain no href/destination
/// field, so the retained `{data}` envelope is exact.
fn envelope(value: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"data": value})
}

fn bounded(body: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

pub fn configured(
    targets: Vec<(
        AckWriteTarget,
        Arc<dyn HandoverAckRead>,
        Arc<dyn HandoverPostSource>,
        Arc<dyn crate::application::native_r3_handover_ack_write::AckWriteStore>,
        Arc<dyn AckCommandLedger>,
    )>,
) -> Result<Router, BusinessError> {
    let owners = targets
        .into_iter()
        .map(|(target, read, posts, store, ledger)| {
            (
                target,
                AckWriteHandler::new(posts.clone(), read, posts, store, ledger),
            )
        })
        .collect();
    configured_with_handlers(owners)
}

async fn acknowledge(
    State(state): State<Arc<AckWriteRouterState>>,
    Path((slug, post_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let company = required(&headers, CONTEXT_COMPANY)?;
        let user = required(&headers, CONTEXT_USER)?;
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        if target.company_id != company {
            return Err(invalid());
        }
        if unique_header(&headers, ORIGIN_HEADER)? != Some(target.origin.as_str()) {
            return Err(origin_forbidden());
        }
        let key = required(&headers, KEY_HEADER)?;
        validate_key(&key)?;
        let value = target
            .handler
            .acknowledge(
                &company,
                &user,
                &post_id,
                &key,
                unique_header(&headers, VERSION_HEADER)?,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| invalid())?
                    .as_millis()
                    .min(i64::MAX as u128) as i64,
            )
            .await?;
        Ok::<_, BusinessError>(envelope(value))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

/// One owner reader/writer per Holding pool; a request can only reach the pool
/// registered for its exact target.
pub fn router_for_holdings(
    targets: Vec<(AckWriteTarget, DatabasePool)>,
    ledger: Arc<dyn AckCommandLedger>,
) -> Result<Router, BusinessError> {
    let owners = targets
        .into_iter()
        .map(|(target, pool)| {
            let read: Arc<dyn HandoverAckRead> = Arc::new(PgHandoverAckSource::new(pool.clone()));
            let posts: Arc<dyn HandoverPostSource> = Arc::new(PgHandoverAckSource::new(pool.clone()));
            let store: Arc<dyn crate::application::native_r3_handover_ack_write::AckWriteStore> =
                Arc::new(PgAckWriteStore::new(pool));
            let handler = AckWriteHandler::new(posts.clone(), read, posts, store, ledger.clone());
            (target, handler)
        })
        .collect::<Vec<_>>();
    configured_with_handlers(owners)
}

fn configured_with_handlers(
    owners: Vec<(AckWriteTarget, AckWriteHandler)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, handler) in owners {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || target.company_id.is_empty()
            || target.origin.is_empty()
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    WriteState {
                        company_id: target.company_id,
                        origin: target.origin,
                        handler,
                    },
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/handover/{post_id}/acknowledgement",
            post(acknowledge),
        )
        .with_state(Arc::new(AckWriteRouterState {
            targets: configured,
        })))
}

/// Single-pool router retained for owner tests and composition variants that
/// use one shared pool across registered targets.
pub fn router(
    pool: DatabasePool,
    targets: Vec<AckWriteTarget>,
    ledger: Arc<dyn AckCommandLedger>,
) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
        ledger,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::native_r3_handover_ack::AckFuture;
    use crate::domain::native_r3_handover_ack::HandoverPostFacts;
    use crate::application::native_r3_handover_ack_write::{
        AckCommit, AckReceipt, AckWriteStore,
    };
    use crate::domain::native_r3_handover_ack_write::{invalid_key, invalid_version};

    #[derive(Clone)]
    struct Probe;

    impl HandoverPostSource for Probe {
        fn published_handover<'a>(
            &'a self,
            _company: &'a str,
            _post_id: &'a str,
            _user: &'a str,
        ) -> AckFuture<'a, Option<HandoverPostFacts>> {
            Box::pin(async move {
                Ok(Some(HandoverPostFacts {
                    id: "post-1".into(),
                    author_id: "author-1".into(),
                    version: 3,
                    purpose: "handover".into(),
                    audience_kind: "participants".into(),
                }))
            })
        }
    }

    impl HandoverAckRead for Probe {
        fn recipients<'a>(
            &'a self,
            _post_id: &'a str,
            _author: &'a str,
        ) -> AckFuture<'a, Vec<String>> {
            Box::pin(async move { Ok(vec!["u-1".into(), "u-2".into()]) })
        }

        fn acknowledgements<'a>(
            &'a self,
            _post_id: &'a str,
            _version: i64,
            _ids: &'a [String],
        ) -> AckFuture<'a, Vec<String>> {
            Box::pin(async move { Ok(vec![]) })
        }
    }

    impl AckWriteStore for Probe {
        fn receipt<'a>(&'a self, _key: &'a str) -> AckFuture<'a, Option<AckReceipt>> {
            Box::pin(async { Ok(None) })
        }

        fn assert_recipient<'a>(
            &'a self,
            _post_id: &'a str,
            _user: &'a str,
            _version: i64,
        ) -> AckFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }

        fn insert<'a>(
            &'a self,
            _post_id: &'a str,
            _user: &'a str,
            _version: i64,
            _now: i64,
            _key: &'a str,
        ) -> AckFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }
    }

    #[derive(Clone)]
    struct Ledger;

    impl AckCommandLedger for Ledger {
        fn commit<'a>(&'a self, _command: AckCommit<'a>) -> AckFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }
    }

    fn target() -> AckWriteTarget {
        AckWriteTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            company_id: "acme:001".into(),
            origin: "https://acme.example.test".into(),
        }
    }

    #[test]
    fn targets_must_be_exact_and_distinct() {
        let owner = || {
            (
                target(),
                Arc::new(Probe) as Arc<dyn HandoverAckRead>,
                Arc::new(Probe) as Arc<dyn HandoverPostSource>,
                Arc::new(Probe) as Arc<dyn AckWriteStore>,
                Arc::new(Ledger) as Arc<dyn AckCommandLedger>,
            )
        };
        assert!(configured(vec![owner()]).is_ok());
        assert!(configured(vec![owner(), owner()]).is_err());
        assert!(configured(vec![(
            AckWriteTarget {
                origin: String::new(),
                ..target()
            },
            Arc::new(Probe) as Arc<dyn HandoverAckRead>,
            Arc::new(Probe) as Arc<dyn HandoverPostSource>,
            Arc::new(Probe) as Arc<dyn AckWriteStore>,
            Arc::new(Ledger) as Arc<dyn AckCommandLedger>,
        )])
        .is_err());
    }

    #[test]
    fn envelope_matches_the_retained_next_payload() {
        assert_eq!(
            envelope(serde_json::json!({
                "postId": "post-1", "postVersion": 3, "canAcknowledge": true,
                "acknowledged": true, "recipientCount": 2, "acknowledgedCount": 1
            })),
            serde_json::json!({"data": {
                "postId": "post-1", "postVersion": 3, "canAcknowledge": true,
                "acknowledged": true, "recipientCount": 2, "acknowledgedCount": 1
            }})
        );
        assert_eq!(
            envelope(serde_json::json!({"postId":"post-1","postVersion":3,"acknowledged":true})),
            serde_json::json!({"data":{"postId":"post-1","postVersion":3,"acknowledged":true}})
        );
    }

    #[test]
    fn transport_validation_and_status_codes_are_exact() {
        let mut headers = HeaderMap::new();
        assert!(required(&headers, CONTEXT_COMPANY).is_err());
        headers.insert(CONTEXT_COMPANY, HeaderValue::from_static("acme:001"));
        assert_eq!(required(&headers, CONTEXT_COMPANY).unwrap(), "acme:001");
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("acme:001"));
        assert!(required(&headers, CONTEXT_COMPANY).is_err());
        assert_eq!(invalid_key().code, "COMMAND_KEY_REQUIRED");
        assert_eq!(invalid_version().code, "HANDOVER_VERSION_INVALID");
        assert_eq!(status(FailureKind::Conflict), StatusCode::CONFLICT);
        assert_eq!(status(FailureKind::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
    }
}
