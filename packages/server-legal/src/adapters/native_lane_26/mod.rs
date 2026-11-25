//! Native migration lane 26 for legal.
//!
//! Owner-local executed-copy reads: the axum router owns the target/Company
//! isolation, the application port owns the query boundary and projection, and
//! the Postgres adapter owns all five SQL statements. TypeScript keeps only a
//! strict transport client and fails closed when this service is unavailable.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    CitationEvidenceSource, ExecutedCopySource, LaneFuture, TrustMaterialSource,
    read_executed_copies,
};
pub use domain::{
    ApprovalCycle, COPY_LIMIT, CitationChunk, CopyAuthors, CopyHead, ExecutedCopyRead,
    ExecutedCopyRow, LaneTarget, ReviewRow, TrustMaterial, TrustMaterialDetail, project, valid_id,
    valid_slug,
};
pub use http::{router, router_for_holdings, router_with_reader};
pub use postgres::{
    PgCitationEvidenceSource, PgExecutedCopySource, PgTrustMaterialSource, SQL_APPROVAL,
    SQL_AUTHORS, SQL_CITATION_CHUNK, SQL_LIST, SQL_MEMBER, SQL_REVIEWS, SQL_TEXT_APPROVED,
    SQL_TRUST_APPROVED, SQL_TRUST_MATERIAL,
};
