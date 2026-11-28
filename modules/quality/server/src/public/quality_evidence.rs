//! Native Executive Quality inspection-evidence search contract.
//!
//! The owner executes SQL, owns the row projection and the HTTP ingress;
//! consumers only transport the bounded request and the seven-field rows.
pub use crate::adapters::quality_evidence::{
    Bind, PgQualityEvidenceReader, render,
};
pub use crate::adapters::quality_evidence_http::{
    QualityEvidenceTarget, router, router_for_holdings, router_with_reader, router_with_readers,
};
pub use crate::application::quality_evidence::{
    QualityEvidenceFuture, QualityEvidenceReader, read_inspection_evidence,
};
pub use crate::domain::quality_evidence::{
    EvidenceCursor, InspectionEvidenceQuery, InspectionEvidenceRow, MAX_LIMIT, MIN_LIMIT,
    METRIC_INSPECTION_PASS_RATE, ROW_KEYS, date_millis, decode_cursor, invalid, valid_row, validate,
};
