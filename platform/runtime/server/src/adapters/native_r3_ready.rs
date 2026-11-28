//! Native R3 read parity for `/api/ready`.
//!
//! Retained Next route: `packages/workplace/src/routes/api/ready/route.ts`.
//! Additive only: this module never deletes or rewrites the Next route and does
//! not mount anything; the lead owns the composition mount and cutover.

use crate::adapters::postgres::pool::DatabasePool;
use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use sqlx::query_scalar;
use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

/// Freshness window retained verbatim from the Next route (milliseconds).
pub const FRESH_MILLIS: i64 = 45_000;

/// Verbatim statement: `runtimeHealthSql('heartbeat_at', " WHERE service='jobs'")`
/// in `read-tail-sources.json`, as executed by `readyHeartbeat()`.
pub const READY_HEARTBEAT_SQL: &str =
    "SELECT heartbeat_at FROM runtime_health WHERE service='jobs'";

/// Pure application decision retained from the Next route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadyStatus {
    Ready,
    Unavailable,
}

impl ReadyStatus {
    /// Maintenance blocks first. A missing heartbeat or one older than 45s is
    /// unavailable; a future heartbeat stays fresh, matching the retained
    /// `Date.now() - heartbeat_at > 45_000` comparison exactly.
    pub fn assess(maintenance: bool, heartbeat_at: Option<i64>, now_ms: i64) -> Self {
        let stale = heartbeat_at.is_none_or(|at| now_ms.saturating_sub(at) > FRESH_MILLIS);
        if maintenance || stale {
            Self::Unavailable
        } else {
            Self::Ready
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Unavailable => "unavailable",
        }
    }
}

/// The retained `canonicalDestinationLinks({status})` projection is identity
/// here: the object has no href/destination field, so the body is exactly
/// `{"status":"ready"}` or `{"status":"unavailable"}`.
#[derive(Serialize)]
struct ReadyBody<'a> {
    status: &'a str,
}

fn render(status: ReadyStatus) -> Response {
    let code = match status {
        ReadyStatus::Ready => StatusCode::OK,
        ReadyStatus::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    (
        code,
        [(header::CACHE_CONTROL, "no-store")],
        Json(ReadyBody {
            status: status.as_str(),
        }),
    )
        .into_response()
}

/// Inputs to the pure decision, read from owner-owned sources.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadySnapshot {
    pub maintenance: bool,
    pub heartbeat_at: Option<i64>,
    pub now_ms: i64,
}

impl ReadySnapshot {
    fn assess(self) -> ReadyStatus {
        ReadyStatus::assess(self.maintenance, self.heartbeat_at, self.now_ms)
    }
}

/// Port implemented by the control-plane source in production and by a fixed
/// snapshot in unit tests.
pub trait ReadySource: Clone + Send + Sync + 'static {
    fn snapshot(&self) -> impl Future<Output = ReadySnapshot> + Send;
}

/// Control-plane implementation.
#[derive(Clone)]
pub struct ControlReadySource {
    pool: DatabasePool,
    data_dir: PathBuf,
}

impl ControlReadySource {
    pub fn new(pool: DatabasePool, data_dir: impl Into<PathBuf>) -> Self {
        Self {
            pool,
            data_dir: data_dir.into(),
        }
    }

    /// `maintenance(dataDir)` parity: true when `<dataDir>/maintenance` exists,
    /// false only on NotFound; any other I/O error is reported to the caller,
    /// which fails closed as unavailable.
    pub fn maintenance_active(data_dir: &Path) -> std::io::Result<bool> {
        match std::fs::metadata(data_dir.join("maintenance")) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn now_millis() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

impl ReadySource for ControlReadySource {
    fn snapshot(&self) -> impl Future<Output = ReadySnapshot> + Send {
        let pool = self.pool.clone();
        let data_dir = self.data_dir.clone();
        async move {
            let maintenance = Self::maintenance_active(&data_dir).unwrap_or(true);
            if maintenance {
                return ReadySnapshot {
                    maintenance,
                    heartbeat_at: None,
                    now_ms: Self::now_millis(),
                };
            }
            let heartbeat_at = query_scalar::<_, i64>(READY_HEARTBEAT_SQL)
                .fetch_optional(pool.raw())
                .await
                .ok()
                .flatten();
            ReadySnapshot {
                maintenance,
                heartbeat_at,
                now_ms: Self::now_millis(),
            }
        }
    }
}

struct ReadyState<S> {
    source: S,
}

async fn ready<S: ReadySource>(State(state): State<Arc<ReadyState<S>>>) -> Response {
    render(state.source.snapshot().await.assess())
}

/// Owner-owned router constructor. Composition mounts it under `/api/ready`;
/// this module does not replace the retained foundation route by itself.
pub fn r3_ready_router<S: ReadySource>(source: S) -> Router {
    Router::new()
        .route("/api/ready", get(ready::<S>))
        .with_state(Arc::new(ReadyState { source }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::{Body, to_bytes}, http::Request};
    use tower::ServiceExt;

    #[derive(Clone)]
    struct Fixed(ReadySnapshot);

    impl ReadySource for Fixed {
        fn snapshot(&self) -> impl Future<Output = ReadySnapshot> + Send {
            std::future::ready(self.0)
        }
    }

    fn snapshot(status: ReadyStatus) -> ReadySnapshot {
        match status {
            ReadyStatus::Ready => ReadySnapshot {
                maintenance: false,
                heartbeat_at: Some(1_000_000),
                now_ms: 1_000_000 + FRESH_MILLIS,
            },
            ReadyStatus::Unavailable => ReadySnapshot {
                maintenance: false,
                heartbeat_at: None,
                now_ms: 1_000_000,
            },
        }
    }

    #[test]
    fn decision_matches_the_retained_freshness_rules() {
        assert_eq!(
            ReadyStatus::assess(false, None, 1_000_000),
            ReadyStatus::Unavailable
        );
        assert_eq!(
            ReadyStatus::assess(true, Some(1_000_000), 1_000_000),
            ReadyStatus::Unavailable
        );
        assert_eq!(
            ReadyStatus::assess(false, Some(1_000_000), 1_000_000 + FRESH_MILLIS),
            ReadyStatus::Ready
        );
        assert_eq!(
            ReadyStatus::assess(false, Some(1_000_000), 1_000_001 + FRESH_MILLIS),
            ReadyStatus::Unavailable
        );
        assert_eq!(
            ReadyStatus::assess(false, Some(2_000_000), 1_000_000),
            ReadyStatus::Ready
        );
    }

    #[test]
    fn heartbeat_sql_is_byte_identical_to_the_ts_contract() {
        let contract: serde_json::Value =
            serde_json::from_str(include_str!("read-tail-sources.json")).unwrap();
        let shape = contract["runtimeHealth"].as_str().unwrap();
        let rendered = shape
            .replace("{columns}", "heartbeat_at")
            .replace("{suffix}", " WHERE service='jobs'");
        assert_eq!(READY_HEARTBEAT_SQL, rendered);
    }

    #[test]
    fn maintenance_probe_matches_access_semantics() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!ControlReadySource::maintenance_active(dir.path()).unwrap());
        std::fs::write(dir.path().join("maintenance"), b"").unwrap();
        assert!(ControlReadySource::maintenance_active(dir.path()).unwrap());
    }

    #[tokio::test]
    async fn router_returns_the_exact_next_envelope() {
        for (status, code, body) in [
            (
                ReadyStatus::Ready,
                StatusCode::OK,
                br#"{"status":"ready"}"#.as_slice(),
            ),
            (
                ReadyStatus::Unavailable,
                StatusCode::SERVICE_UNAVAILABLE,
                br#"{"status":"unavailable"}"#.as_slice(),
            ),
        ] {
            let response = r3_ready_router(Fixed(snapshot(status)))
                .oneshot(
                    Request::builder()
                        .uri("/api/ready")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), code);
            assert_eq!(response.headers()["cache-control"], "no-store");
            assert_eq!(
                &to_bytes(response.into_body(), 65_536).await.unwrap()[..],
                body
            );
        }
    }
}
