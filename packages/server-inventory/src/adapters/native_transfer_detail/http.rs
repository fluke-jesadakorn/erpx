//! Owner-local axum router for the native stock-transfer detail read.
//!
//! Target, header and Company/branch isolation follow the transfer-list
//! reference: a request is served only on an exact registered Holding host/slug
//! pair, with one Company context header, one optional bounded branch header and
//! a body carrying only the transfer id. No TypeScript SQL, no fallback reader
//! and no caller-supplied predicate exists behind this route.

use crate::adapters::native_transfer_detail::application::{
    not_found, read_transfer_detail, InventoryTransferDetailRead,
};
use crate::adapters::native_transfer_detail::domain::{
    valid_id, valid_slug, TransferDetailRequest,
};
use crate::adapters::native_transfer_detail::postgres::PgInventoryTransferDetailRead as PgReader;
use erpx_server_runtime::public::http_support::{
    body::to_bytes, header, routing::post, HeaderMap, HeaderName, HeaderValue, IntoResponse, Json,
    Path, Request, Response, Router, State, StatusCode, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-workspace-branch");
const MAX_BODY: usize = 1024;

/// Exact registered Holding target injected by composition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferDetailTarget {
    pub host: String,
    pub holding_slug: String,
}

struct DetailState {
    readers: BTreeMap<(String, String), Arc<dyn InventoryTransferDetailRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid(code: &'static str) -> BusinessError {
    BusinessError::invalid(code, "Use one bounded native Inventory transfer read.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "INVENTORY_TRANSFER_DETAIL_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid("INVENTORY_TRANSFER_DETAIL_REQUEST_INVALID"));
    }
    first
        .map(|value| {
            value
                .to_str()
                .map_err(|_| invalid("INVENTORY_TRANSFER_DETAIL_REQUEST_INVALID"))
        })
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

/// Build the owner-local router. Composition mounts it; this module never
/// edits composition and never accepts a reader from the request.
pub fn router(
    pool: DatabasePool,
    targets: Vec<TransferDetailTarget>,
) -> Result<Router, BusinessError> {
    let reader: Arc<dyn InventoryTransferDetailRead> = Arc::new(PgReader::new(pool));
    let mut readers = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || readers
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    reader.clone(),
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if readers.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(crate::adapters::native_transfer_detail::domain::READ_PATH, post(read))
        .with_state(Arc::new(DetailState { readers })))
}

/// Build the router with one independent owner reader per Holding target.
/// Each `(TransferDetailTarget, DatabasePool)` pair is keyed by the exact
/// `(host, holding_slug)` tuple so a Holding can only reach its own pool.
pub fn router_for_holdings(
    targets: Vec<(TransferDetailTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut readers = BTreeMap::new();
    for (target, pool) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || readers
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    Arc::new(PgReader::new(pool)) as Arc<dyn InventoryTransferDetailRead>,
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if readers.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(crate::adapters::native_transfer_detail::domain::READ_PATH, post(read))
        .with_state(Arc::new(DetailState { readers })))
}

/// The wire body carries only the bounded transfer id. Every other key, a
/// non-string id and any non-scalar value is rejected before the reader runs.
fn parse_body(bytes: &[u8]) -> Result<String, BusinessError> {
    let body: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| invalid("INVENTORY_TRANSFER_DETAIL_REQUEST_INVALID"))?;
    let object = body
        .as_object()
        .ok_or_else(|| invalid("INVENTORY_TRANSFER_DETAIL_REQUEST_INVALID"))?;
    if object.len() != 1 {
        return Err(invalid("INVENTORY_TRANSFER_DETAIL_REQUEST_INVALID"));
    }
    let id = object
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid("INVENTORY_TRANSFER_DETAIL_REQUEST_INVALID"))?;
    if !valid_id(id) {
        return Err(invalid("INVENTORY_TRANSFER_DETAIL_ID_INVALID"));
    }
    Ok(id.to_owned())
}

async fn read(
    State(state): State<Arc<DetailState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .readers
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(|| invalid("INVENTORY_TRANSFER_DETAIL_COMPANY_INVALID"))?
            .to_owned();
        let branch = unique_header(request.headers(), CONTEXT_BRANCH)?
            .filter(|value| !value.is_empty())
            .map(|value| {
                if valid_id(value) {
                    Ok(value.to_owned())
                } else {
                    Err(invalid("INVENTORY_TRANSFER_DETAIL_BRANCH_INVALID"))
                }
            })
            .transpose()?;
        let bytes = to_bytes(request.into_body(), MAX_BODY)
            .await
            .map_err(|_| invalid("INVENTORY_TRANSFER_DETAIL_REQUEST_INVALID"))?;
        let id = parse_body(&bytes)?;
        let request = TransferDetailRequest {
            company,
            branch,
            id,
        };
        let response = read_transfer_detail(reader.as_ref(), &request).await?;
        response.row.ok_or_else(not_found)
    }
    .await;
    match result {
        Ok(row) => bounded(row),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_parse_admits_only_one_bounded_transfer_id() {
        assert_eq!(parse_body(br#"{"id":"transfer-1"}"#).unwrap(), "transfer-1");
        assert!(parse_body(b"").is_err());
        assert!(parse_body(b"{}").is_err());
        assert!(parse_body(br#"{"id":"transfer-1","extra":1}"#).is_err());
        assert!(parse_body(br#"{"transferId":"transfer-1"}"#).is_err());
        assert!(parse_body(br#"{"id":"bad id"}"#).is_err());
    }

    #[test]
    fn router_requires_registered_distinct_targets() {
        assert!(router_for_holdings(vec![]).is_err());
    }

    #[test]
    fn missing_row_is_a_closed_not_found_error() {
        assert_eq!(not_found().code, "TRANSFER_NOT_FOUND");
        assert_eq!(error_response(not_found()).status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn error_status_mapping_is_closed() {
        assert_eq!(
            status(FailureKind::InvalidInput),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
    }
}
