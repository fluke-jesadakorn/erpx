//! Stable owner surface for the native Talent executive-evidence read.
//!
//! Domain facts, the read-only query/handler port, the real sqlx reader and
//! the axum router constructor. Composition mounts the router with an owner
//! pool; nothing here prepares SQL in TypeScript or falls back to a TS reader.

pub use crate::adapters::native_talent_evidence::{
    PgTalentEvidenceRead, TalentEvidenceTarget, router, router_for_holdings, router_with_reader,
};
pub use crate::application::native_talent_evidence::{
    EvidenceFuture, EvidenceHandler, EvidenceQuery, EvidenceRange, TalentEvidenceRead,
};
pub use crate::domain::native_talent_evidence::{
    EVIDENCE_APPLICATION_SQL, EVIDENCE_REQUISITION_SQL, MAX_IDENTIFIER,
    METRIC_APPLICATION_STAGE_COUNTS, METRIC_OPEN_REQUISITIONS, EvidenceMetric, TalentEvidence,
    invalid, range_required, unavailable, validate_id, validate_optional_scope,
};
