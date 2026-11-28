//! Owner-local axum router for the Legal rule-impact read.
//!
//! This follows the same target/header/Company isolation and error mapping as
//! the other native Legal reads: an exact registered `(host, holding_slug)` pair
//! is required, the Company/user/scope identities and the purchase authority
//! arrive from composition as unique private headers, and the owner executes
//! all SQL. No TypeScript business fallback exists for this operation.

use std::{collections::BTreeMap, sync::Arc};

use erpx_platform_runtime_server::public::{
    http_support::{
        HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router,
        State, StatusCode, header, routing::get, serde_json,
    },
    postgres::DatabasePool,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};

use super::{
    application::{RuleImpactAuthority, RuleImpactSource, read_rule_impact},
    domain::{RuleImpactTarget, valid_id, valid_slug},
    postgres::PgRuleImpactSource,
};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-context-branch");
const CONTEXT_DEPARTMENT: HeaderName = HeaderName::from_static("x-erpx-context-department");
const AUTHORITY_ENABLED: HeaderName = HeaderName::from_static("x-erpx-legal-rule-impact-enabled");
const AUTHORITY_PURCHASED: HeaderName = HeaderName::from_static("x-erpx-legal-rule-impact-purchased");

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn RuleImpactSource>>,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_LEGAL_RULE_IMPACT_UNAVAILABLE",
        "The native Legal rule-impact service is unavailable.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_LEGAL_RULE_IMPACT_REQUEST_INVALID",
        "Use one bounded Legal rule-impact read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_LEGAL_RULE_IMPACT_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
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

fn scope(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    match unique_header(headers, name)? {
        None | Some("") => Ok(None),
        Some(value) if valid_id(value) => Ok(Some(value)),
        Some(_) => Err(invalid()),
    }
}

fn authority(headers: &HeaderMap) -> Result<RuleImpactAuthority, BusinessError> {
    let flag = |name: HeaderName| -> Result<bool, BusinessError> {
        match unique_header(headers, name)? {
            Some("true") => Ok(true),
            Some("false") => Ok(false),
            _ => Err(invalid()),
        }
    };
    Ok(RuleImpactAuthority {
        company_enabled: flag(AUTHORITY_ENABLED)?,
        rule_purchased: flag(AUTHORITY_PURCHASED)?,
    })
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

fn bounded(body: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

/// Shared target configuration. Every public constructor funnels through this
/// map, so the `(host, holding_slug)` key and exact-target validation have one
/// owner. Each key holds one reader, which may be a distinct Pg reader for a
/// distinct Holding pool.
fn configure_targets(
    targets: Vec<(RuleImpactTarget, Arc<dyn RuleImpactSource>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn RuleImpactSource>>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(configured)
}

fn into_router(
    targets: BTreeMap<(String, String), Arc<dyn RuleImpactSource>>,
) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/legal/rule-impact/{id}",
            get(current),
        )
        .with_state(Arc::new(LaneState { targets }))
}

/// Build the native read router with one pool shared by all registered targets.
pub fn router(pool: DatabasePool, targets: Vec<RuleImpactTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn RuleImpactSource> = Arc::new(PgRuleImpactSource::new(pool));
    let targets = targets.into_iter().map(|target| (target, reader.clone())).collect();
    Ok(into_router(configure_targets(targets)?))
}

/// Per-Holding constructor. Each `(RuleImpactTarget, DatabasePool)` pair gets
/// its own reader, so every Holding reads only from its own database pool while
/// the same `host`/`holding_slug` isolation applies to all of them.
pub fn router_for_holdings(
    targets: Vec<(RuleImpactTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let readers = targets
        .into_iter()
        .map(|(target, pool)| {
            let reader: Arc<dyn RuleImpactSource> = Arc::new(PgRuleImpactSource::new(pool));
            (target, reader)
        })
        .collect();
    Ok(into_router(configure_targets(readers)?))
}

/// Reader-injection constructor used by owner tests.
pub fn router_with_reader(
    reader: Arc<dyn RuleImpactSource>,
    targets: Vec<RuleImpactTarget>,
) -> Result<Router, BusinessError> {
    let targets = targets.into_iter().map(|target| (target, reader.clone())).collect();
    Ok(into_router(configure_targets(targets)?))
}

async fn current(
    State(state): State<Arc<LaneState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = scope(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| !value.is_empty())
            .ok_or_else(invalid)?;
        let user = scope(request.headers(), CONTEXT_USER)?
            .filter(|value| !value.is_empty())
            .ok_or_else(invalid)?;
        let branch = scope(request.headers(), CONTEXT_BRANCH)?;
        let department = scope(request.headers(), CONTEXT_DEPARTMENT)?;
        if !valid_id(&id) {
            return Err(invalid());
        }
        let authority = authority(request.headers())?;
        let read = read_rule_impact(
            reader.as_ref(),
            authority,
            company,
            branch,
            department,
            user,
            &id,
        )
        .await?;
        let body = serde_json::to_value(read).map_err(|_| unavailable())?;
        Ok::<_, BusinessError>(serde_json::json!({"data": body}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use erpx_platform_runtime_server::public::http_support::body::Body;
    use super::super::domain::{RuleHead, RuleImpactItem};
    use super::super::application::RuleImpactFuture;

    struct TaggedReader {
        tag: &'static str,
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl RuleImpactSource for TaggedReader {
        fn enabled<'a>(&'a self, _company: &'a str) -> RuleImpactFuture<'a, bool> {
            Box::pin(async { Ok(true) })
        }
        fn rule<'a>(
            &'a self,
            _company: &'a str,
            _id: &'a str,
        ) -> RuleImpactFuture<'a, Option<RuleHead>> {
            Box::pin(async move {
                Ok(Some(RuleHead {
                    kind: "rule".to_owned(),
                    version: 3,
                }))
            })
        }
        fn impacts<'a>(
            &'a self,
            company: &'a str,
            _branch: Option<&'a str>,
            _department: Option<&'a str>,
            _user: &'a str,
            id: &'a str,
        ) -> RuleImpactFuture<'a, Vec<RuleImpactItem>> {
            let tag = self.tag;
            let seen = self.seen.clone();
            let company = company.to_owned();
            let id = id.to_owned();
            Box::pin(async move {
                seen.lock().unwrap().push(format!("{tag}:{company}:{id}"));
                Ok(vec![RuleImpactItem {
                    id: "item-1".to_owned(),
                    kind: "contract".to_owned(),
                    title: "Agreement".to_owned(),
                    state: "draft".to_owned(),
                    due_date: None,
                    version: 2,
                }])
            })
        }
    }

    fn request(host: &str, enabled: &str, purchased: &str) -> Request {
        Request::builder()
            .header(header::HOST, host)
            .header(CONTEXT_COMPANY, "company-1")
            .header(CONTEXT_USER, "user-1")
            .header(AUTHORITY_ENABLED, enabled)
            .header(AUTHORITY_PURCHASED, purchased)
            .body(Body::empty())
            .expect("request")
    }

    #[test]
    fn error_mapping_and_configuration() {
        assert_eq!(status(FailureKind::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(invalid().code, "NATIVE_LEGAL_RULE_IMPACT_REQUEST_INVALID");
        assert_eq!(unavailable().kind, FailureKind::Unavailable);
        let reader: Arc<dyn RuleImpactSource> = Arc::new(TaggedReader {
            tag: "reader",
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let target = RuleImpactTarget {
            host: "acme.jesadakorn.com".to_owned(),
            holding_slug: "acme".to_owned(),
        };
        assert!(router_with_reader(reader.clone(), vec![target.clone()]).is_ok());
        assert_eq!(
            router_with_reader(reader, vec![target.clone(), target])
                .err()
                .map(|error| error.code),
            Some("NATIVE_LEGAL_RULE_IMPACT_CONFIG_INVALID")
        );
    }

    #[tokio::test]
    async fn two_holdings_route_to_two_distinct_readers() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let acme: Arc<dyn RuleImpactSource> = Arc::new(TaggedReader {
            tag: "reader-acme",
            seen: seen.clone(),
        });
        let north: Arc<dyn RuleImpactSource> = Arc::new(TaggedReader {
            tag: "reader-north",
            seen: seen.clone(),
        });
        let state = Arc::new(LaneState {
            targets: configure_targets(vec![
                (
                    RuleImpactTarget {
                        host: "acme.jesadakorn.com".to_owned(),
                        holding_slug: "acme".to_owned(),
                    },
                    acme,
                ),
                (
                    RuleImpactTarget {
                        host: "north.jesadakorn.com".to_owned(),
                        holding_slug: "northstar".to_owned(),
                    },
                    north,
                ),
            ])
            .expect("targets"),
        });
        for (slug, host) in [
            ("acme", "acme.jesadakorn.com"),
            ("northstar", "north.jesadakorn.com"),
        ] {
            let response = current(
                State(state.clone()),
                Path((slug.to_owned(), "rule-1".to_owned())),
                request(host, "true", "true"),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
        }
        assert_eq!(*seen.lock().unwrap(), vec![
            "reader-acme:company-1:rule-1".to_owned(),
            "reader-north:company-1:rule-1".to_owned(),
        ]);
    }

    #[tokio::test]
    async fn handlers_fail_closed_for_unregistered_or_unbounded_requests() {
        let reader: Arc<dyn RuleImpactSource> = Arc::new(TaggedReader {
            tag: "reader",
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let state = Arc::new(LaneState {
            targets: configure_targets(vec![(
                RuleImpactTarget {
                    host: "acme.jesadakorn.com".to_owned(),
                    holding_slug: "acme".to_owned(),
                },
                reader,
            )])
            .expect("targets"),
        });
        for (host, id, enabled, purchased, status_code) in [
            ("other.jesadakorn.com", "rule-1", "true", "true", StatusCode::NOT_FOUND),
            ("acme.jesadakorn.com", "bad id!", "true", "true", StatusCode::UNPROCESSABLE_ENTITY),
            ("acme.jesadakorn.com", "rule-1", "true", "false", StatusCode::FORBIDDEN),
            ("acme.jesadakorn.com", "rule-1", "maybe", "true", StatusCode::UNPROCESSABLE_ENTITY),
            ("acme.jesadakorn.com", "rule-1", "true", "", StatusCode::UNPROCESSABLE_ENTITY),
        ] {
            let response = current(
                State(state.clone()),
                Path(("acme".to_owned(), id.to_owned())),
                request(host, enabled, purchased),
            )
            .await;
            assert_eq!(response.status(), status_code);
        }
    }
}
