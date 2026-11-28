//! Native migration slice for the Legal processing-status read.
//!
//! `GET .../legal/processing/{id}` is still a TypeScript point read: the TS
//! adapter prepares `legalPointByIdAndKeySql` against `legal_processing_jobs`,
//! then shapes `jobId`/`result` in the application layer. This module moves the
//! whole operation into the owner: pure types and invariants in `domain`, the
//! query boundary and item-access gate in `application`, the real Company-scoped
//! sqlx statement in `postgres`, and the Axum ingress in `http`. No TypeScript
//! statement or fallback is involved.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    ProcessingStatusAuthority, ProcessingStatusSource, read_processing_status,
};
pub use domain::{
    PROCESSING_STATUS_OPERATION, ProcessingStatusRead, ProcessingStatusTarget, valid_id, valid_slug,
};
pub use http::{router, router_for_holdings, router_with_reader};
pub use postgres::{PgProcessingStatusSource, SQL_STATUS};
