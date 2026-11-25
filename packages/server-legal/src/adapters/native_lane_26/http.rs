//! Owner-local axum router for the Legal executed-copy read.
//!
//! This follows the same target/header/Company isolation and error mapping as
//! the Quality native read: an exact registered `(host, holding_slug)` pair is
//! required, the Company and user identities come from unique private headers,
//! and every failure maps to the shared owner error envelope. No SQL is
//! executed here and no TypeScript business fallback exists.

use std::{collections::BTreeMap, sync::Arc};

use erpx_server_runtime::public::{
    http_support::{
        HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router,
        State, StatusCode, header, routing::get, serde_json,
    },
    postgres::DatabasePool,
};
use erpx_server_shared::public::{BusinessError, FailureKind};

use super::{
    application::{ExecutedCopySource, TrustMaterialSource, read_executed_copies},
    domain::{LaneTarget, TrustMaterial, TrustMaterialDetail, valid_id, valid_slug},
    postgres::{PgExecutedCopySource, PgTrustMaterialSource},
};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn ExecutedCopySource>>,
    trust: BTreeMap<(String, String), Arc<dyn TrustMaterialSource>>,
}

/// Reader-injection fallback for constructors that only inject the copy source.
/// Trust reads stay closed rather than silently using another Holding's pool.
struct UnavailableTrustSource;

impl TrustMaterialSource for UnavailableTrustSource {
    fn approved<'a>(&'a self, _company: &'a str) -> super::application::LaneFuture<'a, Vec<TrustMaterial>> {
        Box::pin(async { Err(unavailable()) })
    }
    fn material<'a>(
        &'a self,
        _company: &'a str,
        _id: &'a str,
    ) -> super::application::LaneFuture<'a, Option<TrustMaterialDetail>> {
        Box::pin(async { Err(unavailable()) })
    }
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_LEGAL_COPY_READ_UNAVAILABLE",
        "The native Legal executed-copy service is unavailable.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_LEGAL_COPY_READ_REQUEST_INVALID",
        "Use one bounded Legal executed-copy read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_LEGAL_COPY_READ_CONFIG_INVALID",
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

/// Shared reader-injection path. Every public constructor funnels through this
/// map, so the `(host, holding_slug)` key and the exact-target validation have
/// one owner. Each key holds one reader, which may be a distinct Pg reader for
/// a distinct Holding pool.
fn configure_targets<Source: ?Sized>(
    targets: Vec<(LaneTarget, Arc<Source>)>,
) -> Result<BTreeMap<(String, String), Arc<Source>>, BusinessError> {
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
    targets: BTreeMap<(String, String), Arc<dyn ExecutedCopySource>>,
    trust: BTreeMap<(String, String), Arc<dyn TrustMaterialSource>>,
) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/legal/native-lane-26/executed-copies/{item_id}",
            get(current),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/legal/native-lane-26/trust-materials/approved",
            get(trust_approved),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/legal/native-lane-26/trust-materials/{id}",
            get(trust_material),
        )
        .with_state(Arc::new(LaneState { targets, trust }))
}

fn sources_for_one_pool(
    pool: DatabasePool,
) -> (Arc<dyn ExecutedCopySource>, Arc<dyn TrustMaterialSource>) {
    (
        Arc::new(PgExecutedCopySource::new(pool.clone())),
        Arc::new(PgTrustMaterialSource::new(pool)),
    )
}

/// Build the native read router with one pool shared by all registered targets.
/// The lead mounts it alongside the other Legal owner routers; composition owns
/// the pool and the exact registered targets.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let (reader, trust) = sources_for_one_pool(pool);
    let copies = configure_targets(
        targets.iter().map(|target| (target.clone(), reader.clone())).collect(),
    )?;
    let trust = configure_targets(
        targets.into_iter().map(|target| (target, trust.clone())).collect(),
    )?;
    Ok(into_router(copies, trust))
}

/// Reader-injection constructor used by owner tests. It installs the supplied
/// copy reader for every exact target; trust reads stay closed.
pub fn router_with_reader(
    reader: Arc<dyn ExecutedCopySource>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_with_sources(reader, Arc::new(UnavailableTrustSource), targets)
}

/// Owner-test injection path for both read families.
fn router_with_sources(
    reader: Arc<dyn ExecutedCopySource>,
    trust: Arc<dyn TrustMaterialSource>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let copies = configure_targets(
        targets.iter().map(|target| (target.clone(), reader.clone())).collect(),
    )?;
    let trust = configure_targets(
        targets.into_iter().map(|target| (target, trust.clone())).collect(),
    )?;
    Ok(into_router(copies, trust))
}

/// Per-Holding constructor. Each `(LaneTarget, DatabasePool)` pair gets its own
/// reader pair, so every Holding reads only from its own database pool while
/// the same `host`/`holding_slug` isolation applies to all of them.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut copies_targets = Vec::with_capacity(targets.len());
    let mut trust_targets = Vec::with_capacity(targets.len());
    for (target, pool) in targets {
        let (reader, trust) = sources_for_one_pool(pool);
        copies_targets.push((target.clone(), reader));
        trust_targets.push((target, trust));
    }
    Ok(into_router(
        configure_targets(copies_targets)?,
        configure_targets(trust_targets)?,
    ))
}

async fn current(
    State(state): State<Arc<LaneState>>,
    Path((slug, item)): Path<(String, String)>,
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
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let user = unique_header(request.headers(), CONTEXT_USER)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        if !valid_id(&item) {
            return Err(invalid());
        }
        let read = read_executed_copies(reader.as_ref(), company, user, &item).await?;
        let body = serde_json::to_value(read).map_err(|_| unavailable())?;
        Ok::<_, BusinessError>(body)
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

fn trust_scope<'a>(
    state: &'a LaneState,
    request: &Request,
    slug: &str,
) -> Result<(String, &'a dyn TrustMaterialSource), BusinessError> {
    let host = unique_header(request.headers(), header::HOST)?
        .filter(|host| !host.is_empty())
        .ok_or_else(missing)?;
    let source = state
        .trust
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)?;
    let company = unique_header(request.headers(), CONTEXT_COMPANY)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?;
    Ok((company.to_owned(), source.as_ref()))
}

async fn trust_approved(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let (company, source) = trust_scope(&state, &request, &slug)?;
        let materials = source.approved(&company).await?;
        Ok::<_, BusinessError>(serde_json::json!({"materials": materials}))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn trust_material(
    State(state): State<Arc<LaneState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        if !valid_id(&id) {
            return Err(invalid());
        }
        let (company, source) = trust_scope(&state, &request, &slug)?;
        let material = source.material(&company, &id).await?;
        Ok::<_, BusinessError>(serde_json::json!({"material": material}))
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
    use erpx_server_runtime::public::http_support::body::{to_bytes, Body};

    /// Reader-injection fake: records its own tag when the handler reaches it,
    /// so a request proves which Holding reader the state map selected.
    struct TaggedReader {
        tag: &'static str,
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl ExecutedCopySource for TaggedReader {
        fn list<'a>(
            &'a self,
            _company: &'a str,
            _item: &'a str,
        ) -> super::super::application::LaneFuture<'a, Vec<super::super::domain::CopyHead>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn authors<'a>(
            &'a self,
            _company: &'a str,
            _item: &'a str,
            _copy: &'a str,
        ) -> super::super::application::LaneFuture<'a, Option<super::super::domain::CopyAuthors>> {
            Box::pin(async { Ok(None) })
        }
        fn reviews<'a>(
            &'a self,
            _company: &'a str,
            _copy: &'a str,
        ) -> super::super::application::LaneFuture<'a, Vec<(String, super::super::domain::ReviewRow)>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn member<'a>(
            &'a self,
            _company: &'a str,
            _item: &'a str,
            _user: &'a str,
        ) -> super::super::application::LaneFuture<'a, bool> {
            let tag = self.tag;
            let seen = self.seen.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(tag.to_owned());
                Ok(true)
            })
        }
        fn approval<'a>(
            &'a self,
            _company: &'a str,
            _item: &'a str,
        ) -> super::super::application::LaneFuture<'a, Option<i64>> {
            Box::pin(async { Ok(None) })
        }
    }

    fn request(host: &str) -> Request {
        Request::builder()
            .header(header::HOST, host)
            .header(CONTEXT_COMPANY, "company-1")
            .header(CONTEXT_USER, "user-1")
            .body(Body::empty())
            .expect("request")
    }

    #[test]
    fn error_mapping_matches_failure_kinds() {
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(invalid().code, "NATIVE_LEGAL_COPY_READ_REQUEST_INVALID");
        assert_eq!(unavailable().kind, FailureKind::Unavailable);
    }

    #[tokio::test]
    async fn two_holdings_route_to_two_distinct_readers() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let acme: Arc<dyn ExecutedCopySource> = Arc::new(TaggedReader {
            tag: "reader-acme",
            seen: seen.clone(),
        });
        let north: Arc<dyn ExecutedCopySource> = Arc::new(TaggedReader {
            tag: "reader-north",
            seen: seen.clone(),
        });
        let targets = vec![
            (
                LaneTarget {
                    host: "acme.jesadakorn.com".to_owned(),
                    holding_slug: "acme".to_owned(),
                },
                acme.clone(),
            ),
            (
                LaneTarget {
                    host: "north.jesadakorn.com".to_owned(),
                    holding_slug: "northstar".to_owned(),
                },
                north.clone(),
            ),
        ];
        let configured = configure_targets(targets).expect("configured targets");
        assert_eq!(configured.len(), 2);
        let acme_key = ("acme.jesadakorn.com".to_owned(), "acme".to_owned());
        let north_key = ("north.jesadakorn.com".to_owned(), "northstar".to_owned());
        assert!(Arc::ptr_eq(&configured[&acme_key], &acme));
        assert!(Arc::ptr_eq(&configured[&north_key], &north));
        assert!(!Arc::ptr_eq(&configured[&acme_key], &configured[&north_key]));

        // The reader-injection path is also used by router_for_holdings: each
        // Holding key resolves to its own reader when the handler runs.
        let state = Arc::new(LaneState {
            targets: configured,
            trust: BTreeMap::new(),
        });
        let _ = current(
            State(state.clone()),
            Path(("acme".to_owned(), "item-1".to_owned())),
            request("acme.jesadakorn.com"),
        )
        .await;
        let _ = current(
            State(state),
            Path(("northstar".to_owned(), "item-1".to_owned())),
            request("north.jesadakorn.com"),
        )
        .await;
        assert_eq!(*seen.lock().unwrap(), vec!["reader-acme", "reader-north"]);
    }

    struct TaggedTrust {
        tag: &'static str,
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl TrustMaterialSource for TaggedTrust {
        fn approved<'a>(
            &'a self,
            _company: &'a str,
        ) -> super::super::application::LaneFuture<'a, Vec<TrustMaterial>> {
            let tag = self.tag;
            let seen = self.seen.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(tag.to_owned());
                Ok(vec![TrustMaterial {
                    id: tag.to_owned(),
                    kind: "root".to_owned(),
                    content: "base64".to_owned(),
                    checksum: "hash".to_owned(),
                }])
            })
        }
        fn material<'a>(
            &'a self,
            _company: &'a str,
            _id: &'a str,
        ) -> super::super::application::LaneFuture<'a, Option<TrustMaterialDetail>> {
            Box::pin(async { Ok(None) })
        }
    }

    #[tokio::test]
    async fn trust_reads_route_to_each_holdings_own_pool() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let acme: Arc<dyn TrustMaterialSource> = Arc::new(TaggedTrust {
            tag: "trust-acme",
            seen: seen.clone(),
        });
        let north: Arc<dyn TrustMaterialSource> = Arc::new(TaggedTrust {
            tag: "trust-north",
            seen: seen.clone(),
        });
        let targets = vec![
            (
                LaneTarget {
                    host: "acme.jesadakorn.com".to_owned(),
                    holding_slug: "acme".to_owned(),
                },
                acme.clone(),
            ),
            (
                LaneTarget {
                    host: "north.jesadakorn.com".to_owned(),
                    holding_slug: "northstar".to_owned(),
                },
                north.clone(),
            ),
        ];
        let trust = configure_targets(targets).expect("trust targets");
        assert!(Arc::ptr_eq(&trust[&("acme.jesadakorn.com".to_owned(), "acme".to_owned())], &acme));
        assert!(Arc::ptr_eq(&trust[&("north.jesadakorn.com".to_owned(), "northstar".to_owned())], &north));
        let state = Arc::new(LaneState {
            targets: BTreeMap::new(),
            trust,
        });

        for (slug, host) in [
            ("acme", "acme.jesadakorn.com"),
            ("northstar", "north.jesadakorn.com"),
        ] {
            let response = trust_approved(
                State(state.clone()),
                Path(slug.to_owned()),
                request(host),
            )
            .await;
            let body = to_bytes(response.into_body(), usize::MAX).await.expect("body");
            let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
            assert_eq!(value["materials"][0]["id"], slug_to_tag(slug));
        }
        assert_eq!(*seen.lock().unwrap(), vec!["trust-acme", "trust-north"]);
    }

    fn slug_to_tag(slug: &str) -> &'static str {
        match slug {
            "acme" => "trust-acme",
            _ => "trust-north",
        }
    }

    #[test]
    fn reader_injection_rejects_duplicate_targets() {
        let reader: Arc<dyn ExecutedCopySource> = Arc::new(TaggedReader {
            tag: "reader-acme",
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let target = LaneTarget {
            host: "acme.jesadakorn.com".to_owned(),
            holding_slug: "acme".to_owned(),
        };
        assert_eq!(
            router_with_reader(reader, vec![target.clone(), target])
                .err()
                .map(|error| error.code),
            Some("NATIVE_LEGAL_COPY_READ_CONFIG_INVALID".to_owned())
        );
    }
}
