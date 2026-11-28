//! Owner-local axum router for the native stock-transfer list read.
//!
//! Target, header and Company/branch isolation follow the owner-read reference:
//! a request is served only on an exact registered Holding host/slug pair, with
//! one Company context header, one optional bounded branch header and an empty
//! body. No TypeScript SQL, no fallback reader and no caller-supplied predicate
//! exists behind this route.

use crate::adapters::native_transfer_list::application::InventoryTransferListRead;
use crate::adapters::native_transfer_list::domain::{
    valid_id, valid_slug, READ_PATH, TransferListRequest,
};
use erpx_platform_runtime_server::public::http_support::{
    body::to_bytes, header, routing::post, HeaderMap, HeaderName, HeaderValue, IntoResponse, Json,
    Path, Request, Response, Router, State, StatusCode, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::adapters::native_transfer_list::postgres::PgInventoryTransferListRead as PgReader;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-workspace-branch");
const MAX_BODY: usize = 1024;

/// Exact registered Holding target injected by composition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferTarget {
    pub host: String,
    pub holding_slug: String,
}

struct ListState {
    readers: BTreeMap<(String, String), Arc<dyn InventoryTransferListRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid(code: &'static str) -> BusinessError {
    BusinessError::invalid(code, "Use one bounded native Inventory transfer read.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "INVENTORY_TRANSFER_LIST_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid("INVENTORY_TRANSFER_LIST_REQUEST_INVALID"));
    }
    first
        .map(|value| {
            value
                .to_str()
                .map_err(|_| invalid("INVENTORY_TRANSFER_LIST_REQUEST_INVALID"))
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
pub fn router(pool: DatabasePool, targets: Vec<TransferTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn InventoryTransferListRead> = Arc::new(PgReader::new(pool));
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
        .route(READ_PATH, post(read))
        .with_state(Arc::new(ListState { readers })))
}

/// Build the router with one independent owner reader per Holding target.
/// Each `(TransferTarget, DatabasePool)` pair is keyed by the exact
/// `(host, holding_slug)` tuple so a Holding can only reach its own pool.
pub fn router_for_holdings(
    targets: Vec<(TransferTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut readers = BTreeMap::new();
    for (target, pool) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || readers
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    Arc::new(PgReader::new(pool)) as Arc<dyn InventoryTransferListRead>,
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
        .route(READ_PATH, post(read))
        .with_state(Arc::new(ListState { readers })))
}

/// The wire body carries no data. An empty body or `{}` is admitted; every
/// other shape, key and non-scalar is rejected before the reader is invoked.
fn parse_body(bytes: &[u8]) -> Result<(), BusinessError> {
    if bytes.is_empty() {
        return Ok(());
    }
    let body: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| invalid("INVENTORY_TRANSFER_LIST_REQUEST_INVALID"))?;
    match body.as_object() {
        Some(object) if object.is_empty() => Ok(()),
        _ => Err(invalid("INVENTORY_TRANSFER_LIST_REQUEST_INVALID")),
    }
}

async fn read(
    State(state): State<Arc<ListState>>,
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
            .ok_or_else(|| invalid("INVENTORY_TRANSFER_LIST_COMPANY_INVALID"))?
            .to_owned();
        let branch = unique_header(request.headers(), CONTEXT_BRANCH)?
            .filter(|value| !value.is_empty())
            .map(|value| {
                if valid_id(value) {
                    Ok(value.to_owned())
                } else {
                    Err(invalid("INVENTORY_TRANSFER_LIST_BRANCH_INVALID"))
                }
            })
            .transpose()?;
        let bytes = to_bytes(request.into_body(), MAX_BODY)
            .await
            .map_err(|_| invalid("INVENTORY_TRANSFER_LIST_REQUEST_INVALID"))?;
        parse_body(&bytes)?;
        let request = TransferListRequest { company, branch };
        request.validate()?;
        let rows = reader.list(&request).await?;
        Ok::<_, BusinessError>(serde_json::json!({"rows": rows}))
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
    fn body_parse_admits_only_empty_or_empty_object() {
        assert!(parse_body(b"").is_ok());
        assert!(parse_body(b"{}").is_ok());
        assert!(parse_body(br#"{"company":"company-1"}"#).is_err());
        assert!(parse_body(b"[]").is_err());
        assert!(parse_body(b"not json").is_err());
    }

    #[test]
    fn router_requires_registered_distinct_targets() {
        assert!(router_for_holdings(vec![]).is_err());
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
