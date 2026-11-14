use crate::{
    adapters::{
        composition::compose_business_owners,
        migration_inventory::{EmbeddedMigrationInventory, EmbeddedOwnerRegistry},
    },
    application::inspection::{
        MigrationInventoryReader, OwnerCompositionCoverage, inspect_migration,
    },
};
use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use serde_json::json;
use std::sync::Arc;
#[derive(Clone)]
struct FoundationState {
    inventory: Arc<dyn MigrationInventoryReader + Send + Sync>,
    coverage: Arc<Vec<OwnerCompositionCoverage>>,
}
pub fn foundation_router() -> Router {
    foundation_router_with_inventory(EmbeddedMigrationInventory)
}
/// Only metadata is injectable. Actual composition coverage is source-owned.
pub fn foundation_router_with_inventory(
    reader: impl MigrationInventoryReader + Send + Sync + 'static,
) -> Router {
    let composition = compose_business_owners();
    Router::new()
        .route("/api/health", get(health))
        .route("/api/ready", get(ready))
        .fallback(not_found)
        .with_state(FoundationState {
            inventory: Arc::new(reader),
            coverage: Arc::new(composition.coverage),
        })
        .merge(composition.router)
}
async fn health() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(
            json!({"data":{"service":"erpx-rust","status":"ok","phase":"foundation","acceptance":false}}),
        ),
    )
}
async fn ready(State(state): State<FoundationState>) -> impl IntoResponse {
    let inspection = inspect_migration(
        state.inventory.as_ref(),
        &EmbeddedOwnerRegistry,
        &state.coverage,
    );
    let (is_ready, pending, readiness) = match inspection {
        Ok((_, readiness)) => (
            readiness.ready,
            readiness.pending_owners.clone(),
            Some(readiness),
        ),
        Err(_) => (false, Vec::new(), None),
    };
    let status = if is_ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let body = if is_ready {
        json!({"data":{"status":"ready","readiness":readiness},"acceptance":false})
    } else {
        json!({"error":{"code":"RUST_MIGRATION_INCOMPLETE","message":"The Rust foundation is not a migrated business backend."},"pendingOwners":pending,"readiness":readiness,"acceptance":false})
    };
    (
        status,
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(body),
    )
}
async fn not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        [
            (header::CACHE_CONTROL, "private, no-store"),
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
        ],
        "Not Found",
    )
}
