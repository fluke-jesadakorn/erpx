//! Owner-local axum ingress for the native handover acknowledgement read.
//!
//! Composition mounts this router on the private service. A request is routed
//! only to the Postgres pool registered for its exact `(Host, holding_slug)`
//! pair, and the injected work context must agree with that target's Company.
//! The retained Next transport and its re-exports are untouched by this module.

use crate::adapters::native_r3_handover_ack::postgres::PgHandoverAckSource;
use crate::application::native_r3_handover_ack::{
    HandoverAckHandler, HandoverAckRead, HandoverPostSource,
};
use crate::domain::native_r3_handover_ack::HandoverAcknowledgementView;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");

/// One exact registered Holding target. The Company is part of the target so a
/// caller cannot select another Company's handover by changing a header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AckTarget {
    pub host: String,
    pub holding_slug: String,
    pub company_id: String,
}

struct AckState {
    company_id: String,
    handler: HandoverAckHandler,
}

struct AckRouterState {
    targets: BTreeMap<(String, String), AckState>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_HANDOVER_ACK_INVALID",
        "Use one bounded handover acknowledgement read.",
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
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
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

/// `canonicalDestinationLinks` is identity on this payload: the six fields
/// contain no href/destination key, so the retained `{data}` envelope is exact.
fn envelope(view: &HandoverAcknowledgementView) -> serde_json::Value {
    serde_json::json!({"data": view})
}

fn bounded(body: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

pub fn configured(
    targets: Vec<(AckTarget, Arc<dyn HandoverAckRead>, Arc<dyn HandoverPostSource>)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader, posts) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || target.company_id.is_empty()
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    AckState {
                        company_id: target.company_id,
                        handler: HandoverAckHandler::new(reader, posts),
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
            get(acknowledgement),
        )
        .with_state(Arc::new(AckRouterState { targets: configured })))
}

async fn acknowledgement(
    State(state): State<Arc<AckRouterState>>,
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
        let view = target.handler.read(&company, &user, &post_id).await?;
        Ok::<_, BusinessError>(envelope(&view))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

/// One owner reader per Holding pool; a request can only reach the pool
/// registered for its exact target.
pub fn router_for_holdings(
    targets: Vec<(AckTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let owners = targets
        .into_iter()
        .map(|(target, pool)| {
            let reader: Arc<dyn HandoverAckRead> = Arc::new(PgHandoverAckSource::new(pool.clone()));
            let posts: Arc<dyn HandoverPostSource> = Arc::new(PgHandoverAckSource::new(pool));
            (target, reader, posts)
        })
        .collect();
    configured(owners)
}

/// Single-pool router retained for owner tests and composition variants that
/// use one shared pool across registered targets.
pub fn router(pool: DatabasePool, targets: Vec<AckTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::native_r3_handover_ack::AckFuture;
    use crate::domain::native_r3_handover_ack::HandoverPostFacts;

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
            Box::pin(async move { Ok(vec!["u-2".into()]) })
        }
    }

    fn target() -> AckTarget {
        AckTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            company_id: "acme:001".into(),
        }
    }

    #[test]
    fn targets_must_be_exact_and_distinct() {
        let owner = |target| (target, Arc::new(Probe) as Arc<dyn HandoverAckRead>, Arc::new(Probe) as Arc<dyn HandoverPostSource>);
        assert!(configured(vec![owner(target())]).is_ok());
        assert!(configured(vec![owner(target()), owner(target())]).is_err());
        assert!(configured(vec![owner(AckTarget {
            host: String::new(),
            ..target()
        })]).is_err());
    }

    #[test]
    fn envelope_matches_the_retained_next_payload() {
        let view = HandoverAcknowledgementView {
            post_id: "post-1".into(),
            post_version: 3,
            can_acknowledge: true,
            acknowledged: true,
            recipient_count: 2,
            acknowledged_count: 1,
        };
        assert_eq!(
            envelope(&view),
            serde_json::json!({"data": {
                "postId": "post-1",
                "postVersion": 3,
                "canAcknowledge": true,
                "acknowledged": true,
                "recipientCount": 2,
                "acknowledgedCount": 1
            }})
        );
    }

    #[test]
    fn context_headers_are_single_and_required() {
        let mut headers = HeaderMap::new();
        assert!(required(&headers, CONTEXT_COMPANY).is_err());
        headers.insert(CONTEXT_COMPANY, HeaderValue::from_static("acme:001"));
        assert_eq!(required(&headers, CONTEXT_COMPANY).unwrap(), "acme:001");
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("acme:001"));
        assert!(required(&headers, CONTEXT_COMPANY).is_err());
    }

    #[test]
    fn business_failures_keep_the_next_status_codes() {
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
    }
}
