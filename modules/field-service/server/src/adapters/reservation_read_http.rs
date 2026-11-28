//! Native Field Service reservation readback axum ingress.
//!
//! Transport facts only: an exact registered Host/Holding pair, a closed
//! order/reservation identifier pair, no query string, and error mapping. The
//! backend is the composition-owned, SQL-executing owner read; it resolves the
//! Account session, current Company scope and the serializable unit. No
//! TypeScript proxy, SQL string or plan is accepted from the wire.
use crate::application::reservation_read_http::{FieldReservationReadbackQuery, project};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type FieldReservationReadbackFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct FieldReservationReadbackRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub holding_id: &'a str,
    pub order_id: &'a str,
    pub reservation_id: &'a str,
    pub headers: &'a HeaderMap,
}

/// Composition executes the owner read on its own current unit and returns the
/// complete six-field witness. The router never executes or plans SQL.
pub trait FieldReservationReadbackBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: FieldReservationReadbackRequest<'a>,
    ) -> FieldReservationReadbackFuture<'a, Value>;
}

pub struct FieldReservationReadbackTarget {
    pub host: String,
    pub holding_slug: String,
    pub holding_id: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn FieldReservationReadbackBackend>,
}

struct ReservationReadbackState {
    targets: BTreeMap<(String, String), FieldReservationReadbackTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid(
        "FIELD_SERVICE_RESERVATION_READ_INVALID",
        "Use one bounded Field reservation read.",
    )
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "FIELD_SERVICE_RESERVATION_READ_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
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
fn bounded(value: Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
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
fn unique_header<'a>(headers: &'a HeaderMap, name: HeaderName) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed());
    }
    first
        .map(|value| value.to_str().map_err(|_| malformed()))
        .transpose()
}
fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len())
                    && port.bytes().all(|byte| byte.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|value| value > 0)
            })
    })
}
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}
fn target<'a>(
    state: &'a ReservationReadbackState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a FieldReservationReadbackTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    let target = state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)?;
    if unique_header(headers, header::ORIGIN)?.is_some_and(|origin| origin != target.origin) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        ));
    }
    Ok(target)
}

/// Build the exact Holding-target reservation readback router. Every target
/// must be a distinct registered host/slug pair with a valid browser Origin
/// and a non-empty Holding identifier.
pub fn field_reservation_readback_router(
    targets: Vec<FieldReservationReadbackTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || target.holding_id.is_empty()
            || target.holding_id.len() > 200
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/field-service/{order_id}/reservations/{reservation_id}",
            get(read_reservation),
        )
        .with_state(Arc::new(ReservationReadbackState {
            targets: configured,
        })))
}

async fn read_reservation(
    State(state): State<Arc<ReservationReadbackState>>,
    Path((slug, order, reservation)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        if request.uri().query().is_some() {
            return Err(malformed());
        }
        let query = FieldReservationReadbackQuery {
            holding_slug: target.holding_slug.clone(),
            order_id: order,
            reservation_id: reservation,
        };
        query.validate()?;
        let headers = request.headers().clone();
        let _ = to_bytes(request.into_body(), 0)
            .await
            .map_err(|_| malformed())?;
        let value = target
            .backend
            .read(FieldReservationReadbackRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                holding_id: &target.holding_id,
                order_id: &query.order_id,
                reservation_id: &query.reservation_id,
                headers: &headers,
            })
            .await?;
        project(&value, &query, &target.holding_id)
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_and_slug_admission_is_closed() {
        assert!(valid_origin("https://acme.jesadakorn.com"));
        assert!(!valid_origin("http://acme.jesadakorn.com"));
        assert!(!valid_origin("https://acme.jesadakorn.com/path"));
        assert!(valid_slug("acme"));
        assert!(valid_slug("north-star_1"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug("a/b"));
    }

    #[test]
    fn configured_targets_must_be_distinct() {
        let target = || FieldReservationReadbackTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
            holding_id: "holding".into(),
            origin: "https://acme.jesadakorn.com".into(),
            backend: Arc::new(Noop),
        };
        assert!(field_reservation_readback_router(vec![target()]).is_ok());
        assert!(field_reservation_readback_router(vec![target(), target()]).is_err());
    }

    struct Noop;
    impl FieldReservationReadbackBackend for Noop {
        fn read<'a>(
            &'a self,
            _: FieldReservationReadbackRequest<'a>,
        ) -> FieldReservationReadbackFuture<'a, Value> {
            Box::pin(async { Err(BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")) })
        }
    }
}
