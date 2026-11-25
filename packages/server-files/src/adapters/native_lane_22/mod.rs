//! Native migration lane 22 for files.
//!
//! Owner-local successor of the two evidence SELECTs in
//! `adapters/work-read-facts.ts`. The axum router owns the SQL and the complete
//! row projection; the retained TypeScript consumer is a strict transport that
//! fails closed with 503 when the native service is unavailable. No TypeScript
//! business fallback and no caller-supplied SQL exist in this slice.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{WorkEvidenceFuture, WorkEvidenceRead};
pub use domain::{WorkEvidenceKind, WorkEvidenceResponse, WorkEvidenceRow, parse_read_only, valid_id};
pub use http::{
    LaneTarget, WorkEvidenceTarget, router, router_for_holdings, router_with_pool,
};
pub use postgres::{
    EVIDENCE_SETS_READ_ONLY_SQL, EVIDENCE_SETS_SQL, LEGACY_EVIDENCE_SQL, PgWorkEvidenceRead,
};
