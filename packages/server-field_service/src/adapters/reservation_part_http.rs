//! Native Field Service part-reservation axum ingress.
//!
//! Transport facts only: an exact registered Host/Holding pair, a closed
//! order/reservation identifier pair, the current Company work-context header
//! and error mapping. The backend is the application port whose only
//! implementation executes real SQL in the owner adapter. No SQL, plan or
//! TypeScript fallback is accepted from the wire.
use crate::application::reservation_part::{
    FieldPartReservationQuery, FieldPartReservationStore, project,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

pub struct FieldPartReservationTarget {
    pub host: String,
    pub holding_slug: String,
    pub holding_id: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
}

struct PartReservationState {
    targets: BTreeMap<(String, String), FieldPartReservationTarget>,
    store: Arc<dyn FieldPartReservationStore>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "FIELD_SERVICE_PART_RESERVATION_INVALID",
        "Use one bounded Field part reservation read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "FIELD_SERVICE_PART_RESERVATION_CONFIG_INVALID",
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

fn bounded(value: serde_json::Value) -> Response {
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

fn unique_header<'a>(
    headers: &'a HeaderMap,
    name: HeaderName,
) -> Result<Option<&'a str>, BusinessError> {
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
    state: &'a PartReservationState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a FieldPartReservationTarget, BusinessError> {
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

/// Build the exact Holding-target part-reservation router. Every target must
/// be a distinct registered host/slug pair with a valid browser Origin and a
/// non-empty Holding identifier. The store is the owner SQL port.
pub fn field_part_reservation_router(
    targets: Vec<FieldPartReservationTarget>,
    store: Arc<dyn FieldPartReservationStore>,
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
            "/api/v1/holdings/{holding_slug}/field-service/{order_id}/parts-reservations/{reservation_id}",
            get(read_reservation),
        )
        .with_state(Arc::new(PartReservationState { targets: configured, store })))
}

async fn read_reservation(
    State(state): State<Arc<PartReservationState>>,
    Path((slug, order, reservation)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        if request.uri().query().is_some() {
            return Err(malformed());
        }
        let headers = request.headers().clone();
        let company = unique_header(&headers, HeaderName::from_static("x-erpx-context-company"))?
            .filter(|value| !value.is_empty() && value.len() <= 1024)
            .ok_or_else(malformed)?
            .to_owned();
        let _ = to_bytes(request.into_body(), 0)
            .await
            .map_err(|_| malformed())?;
        let query = FieldPartReservationQuery {
            holding_id: target.holding_id.clone(),
            company_id: company,
            order_id: order,
            reservation_id: reservation,
        };
        query.validate()?;
        let row = state.store.read(&query).await?.ok_or_else(missing)?;
        project(&row, &query)
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
    use crate::{
        application::reservation_part::FieldPartReservationFuture,
        domain::reservation_part::FieldPartReservation,
    };

    struct Noop;
    impl FieldPartReservationStore for Noop {
        fn read<'a>(
            &'a self,
            _: &'a FieldPartReservationQuery,
        ) -> FieldPartReservationFuture<'a, Option<FieldPartReservation>> {
            Box::pin(async { Ok(None) })
        }
    }

    fn target() -> FieldPartReservationTarget {
        FieldPartReservationTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
            holding_id: "holding".into(),
            origin: "https://acme.jesadakorn.com".into(),
        }
    }

    #[test]
    fn origin_and_slug_admission_is_closed() {
        assert!(valid_origin("https://acme.jesadakorn.com"));
        assert!(!valid_origin("http://acme.jesadakorn.com"));
        assert!(!valid_origin("https://acme.jesadakorn.com/path"));
        assert!(valid_slug("acme"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug("a/b"));
    }

    #[test]
    fn configured_targets_must_be_distinct() {
        assert!(field_part_reservation_router(vec![target()], Arc::new(Noop)).is_ok());
        assert!(field_part_reservation_router(vec![target(), target()], Arc::new(Noop)).is_err());
        let mut bad = target();
        bad.holding_id.clear();
        assert!(field_part_reservation_router(vec![bad], Arc::new(Noop)).is_err());
    }
}
