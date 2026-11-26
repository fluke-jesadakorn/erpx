//! Native HTTP ingress for the Asset register read family.
//!
//! Composition registers exact `(Host, holding_slug)` targets and supplies the
//! Account actor authority plus the owner's PostgreSQL reader. A request can
//! only reach the reader bound to its exact target; the handler never builds a
//! predicate, opens a connection or calls a TypeScript reader.
use super::{
    application::{
        AssetRead, AssetReadActorRequest, AssetReadActorSource, load_detail, load_workspace,
    },
    domain,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Query, Response, Router, State,
    StatusCode, header, json, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::Arc};

/// One registered Holding target. Both the Host header and the path slug must
/// match; no suffix or fallback target match is made.
pub struct NativeAssetReadTarget {
    pub host: String,
    pub holding_slug: String,
    pub actor: Arc<dyn AssetReadActorSource>,
    pub reader: Arc<dyn AssetRead>,
}

struct LaneState {
    targets: BTreeMap<(String, String), NativeAssetReadTarget>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetReadQuery {
    id: Option<String>,
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
        Json(json!({"error":{"code":error.code,"message":error.message}})),
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

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(domain::invalid_request());
    }
    first
        .map(|value| value.to_str().map_err(|_| domain::invalid_request()))
        .transpose()
}

/// Build the exact Holding-target router. Empty or duplicate targets are
/// rejected at construction; no connection is opened here.
pub fn native_asset_read_router(
    targets: Vec<NativeAssetReadTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if !domain::valid_host(&target.host)
            || !domain::valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(domain::invalid_request());
        }
    }
    if configured.is_empty() {
        return Err(domain::invalid_request());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/assets/native-read",
            get(read),
        )
        .with_state(Arc::new(LaneState {
            targets: configured,
        })))
}

async fn read(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    Query(query): Query<AssetReadQuery>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| domain::valid_host(host))
            .ok_or_else(domain::missing)?
            .to_owned();
        let target = state
            .targets
            .get(&(host, slug.clone()))
            .ok_or_else(domain::missing)?;
        let id = match query.id.as_deref() {
            Some(id) => {
                domain::validate_id(id)?;
                Some(id)
            }
            None => None,
        };
        let actor = target
            .actor
            .actor(AssetReadActorRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
            })
            .await?;
        let body = match id {
            Some(id) => json!({"data": load_detail(target.reader.as_ref(), actor, id).await?}),
            None => json!({"data": load_workspace(target.reader.as_ref(), actor).await?}),
        };
        Ok::<_, BusinessError>(body)
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
    use crate::{
        adapters::native_asset_read::application::{AssetReadActorFuture, AssetReadFuture},
        public::asset_register::{AssetActor, AssetDetail, AssetWorkspace},
    };
    use erpx_server_runtime::public::http_support::body::to_bytes;
    use erpx_server_shared::public::{CompanyId, RecordId, UserId};

    struct FixedActor;
    impl AssetReadActorSource for FixedActor {
        fn actor<'a>(&'a self, _: AssetReadActorRequest<'a>) -> AssetReadActorFuture<'a> {
            Box::pin(async {
                Ok(AssetActor {
                    user_id: UserId::new("user-1").unwrap(),
                    company_id: CompanyId::new("acme:co").unwrap(),
                    branch_id: Some(RecordId::new("branch-1").unwrap()),
                    department_id: None,
                    permissions: vec!["records.read".into()],
                    modules: vec!["assets".into()],
                })
            })
        }
    }

    struct FixedReader;
    impl AssetRead for FixedReader {
        fn workspace<'a>(&'a self, actor: AssetActor) -> AssetReadFuture<'a, AssetWorkspace> {
            Box::pin(async move {
                Ok(AssetWorkspace {
                    items: Vec::new(),
                    people: Vec::new(),
                    nodes: Vec::new(),
                    finance_assets: Vec::new(),
                    can_link_finance: false,
                    can_create: actor.has("records.write"),
                    actor_id: actor.user_id.as_str().to_owned(),
                })
            })
        }
        fn detail<'a>(&'a self, _: AssetActor, _: &'a str) -> AssetReadFuture<'a, AssetDetail> {
            Box::pin(async {
                Err(BusinessError::new(
                    FailureKind::NotFound,
                    "ASSET_NOT_FOUND",
                    "Asset not found in this scope.",
                ))
            })
        }
    }

    fn target(host: &str, slug: &str) -> NativeAssetReadTarget {
        NativeAssetReadTarget {
            host: host.to_owned(),
            holding_slug: slug.to_owned(),
            actor: Arc::new(FixedActor),
            reader: Arc::new(FixedReader),
        }
    }

    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        serde_json::from_slice(&bytes).expect("json body")
    }

    #[tokio::test]
    async fn workspace_payload_matches_the_retained_assets_route_shape() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("acme.example.test"));
        let response = read(
            State(Arc::new(LaneState {
                targets: BTreeMap::from([(
                    ("acme.example.test".to_owned(), "acme".to_owned()),
                    target("acme.example.test", "acme"),
                )]),
            })),
            Path("acme".to_owned()),
            Query(AssetReadQuery { id: None }),
            headers,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .unwrap()
                .to_str()
                .unwrap(),
            "private, no-store"
        );
        assert_eq!(
            body_json(response).await,
            json!({"data": {
                "items": [], "people": [], "nodes": [], "financeAssets": [],
                "canLinkFinance": false, "canCreate": false, "actorId": "user-1"
            }})
        );
    }

    #[tokio::test]
    async fn unmatched_target_and_unbounded_id_fail_closed() {
        let state = Arc::new(LaneState {
            targets: BTreeMap::from([(
                ("acme.example.test".to_owned(), "acme".to_owned()),
                target("acme.example.test", "acme"),
            )]),
        });
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("other.example.test"));
        let response = read(
            State(state.clone()),
            Path("acme".to_owned()),
            Query(AssetReadQuery { id: None }),
            headers,
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            body_json(response).await,
            json!({"error":{"code":"NOT_FOUND","message":"Not found."}})
        );

        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("acme.example.test"));
        let response = read(
            State(state),
            Path("acme".to_owned()),
            Query(AssetReadQuery {
                id: Some("a".repeat(201)),
            }),
            headers,
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            body_json(response).await["error"]["code"],
            json!("ASSETS_NATIVE_READ_INVALID")
        );
    }

    #[test]
    fn router_requires_distinct_registered_targets() {
        assert!(native_asset_read_router(vec![target("acme.example.test", "acme")]).is_ok());
        assert!(native_asset_read_router(Vec::new()).is_err());
        assert!(
            native_asset_read_router(vec![
                target("acme.example.test", "acme"),
                target("acme.example.test", "acme"),
            ])
            .is_err()
        );
        assert!(native_asset_read_router(vec![target("acme.example.test", "ACME")]).is_err());
    }
}
