//! Native migration lane 31 for learning.
//!
//! Owner-local successor of the two single-row evidence reads formerly composed
//! in `adapters/learning.ts` (`nativeLearningExecutiveEvidence`). The axum
//! router owns the SQL, the projection, the company/branch/department scope and
//! the response shape; the retained TypeScript consumer only transports bounded
//! identities and validates the reply. There is no SQL fallback: a missing or
//! rejected native runtime fails closed with an explicit 503.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{EvidenceFuture, EvidenceReader, EvidenceService};
pub use domain::{
    EvidenceCursor, EvidenceKind, EvidenceQuery, EvidenceRow, EvidenceSearchQuery, LaneTarget,
};
pub use http::{router, router_for_holdings, router_with_reader};
pub use postgres::{COMPLETION_SQL, OVERDUE_SQL, PgEvidenceReader};
