//! Public surface for the native Executive evidence-access command stack.
pub use crate::adapters::executive_evidence_http::{
    ExecutiveEvidenceHttpBackend, ExecutiveEvidenceHttpFuture, ExecutiveEvidenceHttpKind,
    ExecutiveEvidenceHttpRequest, ExecutiveEvidenceHttpTarget, executive_evidence_http_router,
};
pub use crate::adapters::executive_evidence_postgres::PgEvidenceAccessCommand;
pub use crate::application::executive_evidence::{
    EvidenceAccessRepository, EvidenceActor, EvidenceInsert, EvidenceLedgerEntry,
    StoredEvidenceReceipt, record_evidence_access,
};
