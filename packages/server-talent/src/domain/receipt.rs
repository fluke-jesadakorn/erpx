//! Shared owner command receipt snapshot.
//!
//! One immutable receipt row per Idempotency-Key. Each Talent slice keeps its
//! own result JSON shape; the envelope fields are identical so replay can
//! compare the request digest, operation, actor and record before returning the
//! original result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptSnapshot {
    pub request_hash: String,
    pub result_json: String,
    pub operation: String,
    pub actor_id: String,
    pub record_id: String,
    pub authority_json: String,
}
