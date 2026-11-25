//! Native migration lane 38 for billing.
//!
//! The owner crate now executes the executive snapshot watermark read with
//! sqlx and serves it from an owner-local axum router. The retained TypeScript
//! adapter is a strict, fail-closed transport with no SQL and no fallback.
mod application;
mod domain;
mod http;
mod postgres;

pub use application::{BillingWatermarkRead, Lane38Future, read_watermark_body};
pub use domain::{BillingWatermark, WatermarkRow};
pub use http::{LaneTarget, NativeLane38Target, router, router_for_holdings, router_with_reader};
pub use postgres::{PgBillingWatermarkRead, WATERMARK_SQL};
