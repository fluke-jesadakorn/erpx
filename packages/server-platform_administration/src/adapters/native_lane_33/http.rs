//! Owner-local axum router for the lane 33 release child-image read.
//!
//! Target/header/company isolation and error mapping follow the established
//! owner read pattern. The caller transports only a company and a release id.

use crate::adapters::native_lane_33::application::SharedReleaseChildImageRead;
use crate::adapters::native_lane_33::domain::{valid_company, valid_id, valid_slug};
use crate::adapters::native_lane_33::postgres::PgReleaseChildImageRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router,
    State, StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_RELEASE: HeaderName = HeaderName::from_static("x-erpx-context-release");

#[derive(Clone, Debug)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct ReadState {
    targets: BTreeMap<(String, String), SharedReleaseChildImageRead>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PLATFORM_RELEASE_CHILD_IMAGE_REQUEST_INVALID",
        "Use one bounded Platform release child-image read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PLATFORM_RELEASE_CHILD_IMAGE_CONFIG_INVALID",
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

fn build_state(
    entries: impl IntoIterator<Item = (LaneTarget, SharedReleaseChildImageRead)>,
) -> Result<Arc<ReadState>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in entries {
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
    Ok(Arc::new(ReadState { targets: configured }))
}

fn build_router(state: Arc<ReadState>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/platform-administration/release-child-image",
            get(read_image),
        )
        .with_state(state)
}

fn router_for_readers(
    targets: Vec<(LaneTarget, SharedReleaseChildImageRead)>,
) -> Result<Router, BusinessError> {
    Ok(build_router(build_state(targets)?))
}

pub fn router_with_reader(
    reader: SharedReleaseChildImageRead,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_for_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgReleaseChildImageRead::new(pool)), targets)
}

/// One owner reader/pool per registered Holding.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_for_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: SharedReleaseChildImageRead = Arc::new(PgReleaseChildImageRead::new(pool));
                (target, reader)
            })
            .collect(),
    )
}

async fn read_image(
    State(state): State<Arc<ReadState>>,
    Path(slug): Path<String>,
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
            .filter(|value| valid_company(value))
            .ok_or_else(invalid)?;
        let release = unique_header(request.headers(), CONTEXT_RELEASE)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let image = reader.read(company, release).await?;
        Ok::<_, BusinessError>(serde_json::json!({
            "image": image.map(|image| image.image),
        }))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_33::application::{ReleaseChildImageFuture, ReleaseChildImageRead, unavailable};
    use crate::adapters::native_lane_33::domain::ReleaseChildImage;
    use erpx_server_runtime::public::http_support::body;

    struct Labelled {
        id: &'static str,
    }

    impl ReleaseChildImageRead for Labelled {
        fn read<'a>(
            &'a self,
            company: &'a str,
            release: &'a str,
        ) -> ReleaseChildImageFuture<'a, Option<ReleaseChildImage>> {
            let id = self.id.to_owned();
            let company = company.to_owned();
            let release = release.to_owned();
            Box::pin(async move {
                ReleaseChildImage::new(format!(
                    "{{\"acceptance\":{{\"company\":\"{company}\",\"id\":\"{release}\"}},\"revision\":{{\"holding\":\"{id}\"}}}}"
                ))
                .map(Some)
                .map_err(|_| unavailable())
            })
        }
    }

    struct Down;

    impl ReleaseChildImageRead for Down {
        fn read<'a>(
            &'a self,
            _company: &'a str,
            _release: &'a str,
        ) -> ReleaseChildImageFuture<'a, Option<ReleaseChildImage>> {
            Box::pin(async { Err(unavailable()) })
        }
    }

    fn request(host: &'static str) -> Request {
        Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/platform-administration/release-child-image")
            .header("host", host)
            .header("x-erpx-context-company", "company-001")
            .header("x-erpx-context-release", "release:001")
            .body(body::Body::empty())
            .unwrap()
    }

    async fn body(response: Response) -> serde_json::Value {
        let bytes = body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn target_configuration_is_closed() {
        let targets = vec![LaneTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
        }];
        assert!(router_with_reader(Arc::new(Labelled { id: "h001" }), targets.clone()).is_ok());
        assert!(router_with_reader(Arc::new(Labelled { id: "h001" }), vec![]).is_err());
        assert!(router_with_reader(
            Arc::new(Labelled { id: "h001" }),
            vec![LaneTarget { host: "a".into(), holding_slug: "acme".into() }; 2]
        )
        .is_err());
    }

    #[tokio::test]
    async fn two_targets_reach_two_distinct_readers() {
        let state = build_state(vec![
            (
                LaneTarget {
                    host: "acme.jesadakorn.com".into(),
                    holding_slug: "acme".into(),
                },
                Arc::new(Labelled { id: "h001" }) as SharedReleaseChildImageRead,
            ),
            (
                LaneTarget {
                    host: "northstar.jesadakorn.com".into(),
                    holding_slug: "acme".into(),
                },
                Arc::new(Labelled { id: "h002" }) as SharedReleaseChildImageRead,
            ),
        ])
        .unwrap();
        let acme = body(
            read_image(
                State(state.clone()),
                Path("acme".to_owned()),
                request("acme.jesadakorn.com"),
            )
            .await,
        )
        .await;
        let north = body(
            read_image(
                State(state),
                Path("acme".to_owned()),
                request("northstar.jesadakorn.com"),
            )
            .await,
        )
        .await;
        let ok = read_image(
            State(build_state(vec![(
                LaneTarget {
                    host: "acme.jesadakorn.com".into(),
                    holding_slug: "acme".into(),
                },
                Arc::new(Labelled { id: "h001" }) as SharedReleaseChildImageRead,
            )])
            .unwrap()),
            Path("acme".to_owned()),
            request("acme.jesadakorn.com"),
        )
        .await;
        assert_eq!(ok.headers()["cache-control"], "private, no-store");
        assert!(acme["image"].as_str().unwrap().contains("\"holding\":\"h001\""));
        assert!(north["image"].as_str().unwrap().contains("\"holding\":\"h002\""));
        assert_ne!(acme, north);
    }

    #[tokio::test]
    async fn response_is_bounded_and_fail_closed() {
        let state = build_state(vec![(
            LaneTarget {
                host: "acme.jesadakorn.com".into(),
                holding_slug: "acme".into(),
            },
            Arc::new(Down) as SharedReleaseChildImageRead,
        )])
        .unwrap();
        let response = read_image(
            State(state),
            Path("acme".to_owned()),
            request("acme.jesadakorn.com"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let value = body(response).await;
        assert_eq!(
            value["error"]["code"],
            "PLATFORM_RELEASE_CHILD_IMAGE_UNAVAILABLE"
        );
    }

    #[tokio::test]
    async fn malformed_company_scope_fails_closed() {
        let state = build_state(vec![(
            LaneTarget {
                host: "acme.jesadakorn.com".into(),
                holding_slug: "acme".into(),
            },
            Arc::new(Labelled { id: "h001" }) as SharedReleaseChildImageRead,
        )])
        .unwrap();
        let response = read_image(
            State(state),
            Path("acme".to_owned()),
            Request::builder()
                .method("GET")
                .uri("/api/v1/holdings/acme/platform-administration/release-child-image")
                .header("host", "acme.jesadakorn.com")
                .header("x-erpx-context-company", "H001';DROP")
                .header("x-erpx-context-release", "release:001")
                .body(body::Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
