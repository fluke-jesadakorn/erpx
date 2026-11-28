//! Optional engineering example. This has no business data or database access.
use axum::{Json, Router, routing::get};
pub(crate) fn router() -> Router {
    Router::new().route("/api/modules/example", get(example))
}
async fn example() -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"data":{"moduleId":"erpx-example","contract":"erpx.example.http","contractVersion":"1.0.0","eventSchema":"erpx.example.observed","eventSchemaVersion":"1.0.0"},"acceptance":false}),
    )
}
