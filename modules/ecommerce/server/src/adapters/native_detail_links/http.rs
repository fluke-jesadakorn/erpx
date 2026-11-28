//! Native Ecommerce checkout detail-link owner-local axum ingress.
//!
//! Transport facts only: exact Host/Holding targets with one dedicated pool
//! each and a bounded checkout identity. The owner reader executes both
//! SELECTs, projects the rows and renders the complete retired payload. No SQL
//! or relation name is accepted from the wire and every failure maps to a named
//! error so the TypeScript transport fails closed.

use crate::adapters::native_detail_links::application::{DetailLinkRead, read_detail_links};
use crate::adapters::native_detail_links::domain::{MAX_ID_LEN, valid_id};
use crate::adapters::native_detail_links::postgres::PgDetailLinkRead;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const BODY_LIMIT: usize = 4 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetailLinkTarget {
    pub host: String,
    pub holding_slug: String,
}

struct AppState {
    targets: BTreeMap<(String, String), Arc<dyn DetailLinkRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "ECOMMERCE_DETAIL_LINKS_REQUEST_INVALID",
        "Use one bounded Ecommerce checkout detail read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "ECOMMERCE_DETAIL_LINKS_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
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

/// Build the router with one shared pool.
pub fn router(pool: DatabasePool, targets: Vec<DetailLinkTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
    )
}

/// Build the router with one dedicated reader/pool per Holding, keyed by
/// `(host, holding_slug)`. Duplicate or malformed targets fail closed.
pub fn router_for_holdings(
    targets: Vec<(DetailLinkTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, pool) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    Arc::new(PgDetailLinkRead::new(pool)) as Arc<dyn DetailLinkRead>,
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
            "/api/v1/holdings/{holding_slug}/ecommerce/native-detail-links",
            post(read),
        )
        .with_state(Arc::new(AppState { targets: configured })))
}

/// The wire body carries only the checkout identity. Unknown keys are refused.
fn wire(value: &serde_json::Value) -> Result<&str, BusinessError> {
    let object = value.as_object().ok_or_else(invalid)?;
    if object.len() != 1 {
        return Err(invalid());
    }
    let checkout = object
        .get("checkoutId")
        .and_then(|value| value.as_str())
        .ok_or_else(invalid)?;
    if !valid_id(checkout) || checkout.len() > MAX_ID_LEN {
        return Err(invalid());
    }
    Ok(checkout)
}

async fn read(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    // Own the transport facts before awaiting: a borrow of `Request<Body>`
    // across an await would make the handler future !Send.
    let headers = request.headers().clone();
    let (_, body) = request.into_parts();
    let result = async move {
        let slug = slug;
        if !valid_slug(&slug) {
            return Err(invalid());
        }
        let bytes = erpx_platform_runtime_server::public::http_support::body::to_bytes(body, BODY_LIMIT + 1)
            .await
            .map_err(|_| invalid())?;
        if bytes.len() > BODY_LIMIT {
            return Err(invalid());
        }
        let body: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let checkout = wire(&body)?.to_owned();
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug))
            .ok_or_else(missing)?;
        let value = read_detail_links(reader.as_ref(), &checkout).await?;
        Ok::<_, BusinessError>(value)
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

    #[test]
    fn wire_accepts_only_one_bounded_checkout_identity() {
        assert_eq!(
            wire(&serde_json::json!({"checkoutId": "checkout-1"})).unwrap(),
            "checkout-1"
        );
        for bad in [
            serde_json::json!({}),
            serde_json::json!({"checkoutId": "checkout-1", "extra": 1}),
            serde_json::json!({"checkoutId": "bad id"}),
            serde_json::json!({"checkoutId": ""}),
            serde_json::json!({"checkoutId": 7}),
            serde_json::json!([]),
        ] {
            assert_eq!(wire(&bad).unwrap_err().code, "ECOMMERCE_DETAIL_LINKS_REQUEST_INVALID");
        }
    }

    #[test]
    fn validation_helpers_reject_bad_targets() {
        assert!(valid_slug("acme"));
        assert!(!valid_slug("Acme"));
        assert!(valid_id("checkout-1"));
        assert!(!valid_id("bad id"));
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            configuration_invalid().code,
            "ECOMMERCE_DETAIL_LINKS_CONFIG_INVALID"
        );
    }
}
