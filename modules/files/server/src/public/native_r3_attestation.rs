//! Public owner exports for the R3 sanitization-attestation native command.
//!
//! The lead mounts `router` in composition with the exact registered Holding
//! targets, work-context resolver and owning-record authority. No TypeScript
//! route, re-export or shared config is changed by this module.
pub use crate::adapters::native_r3_attestation::{
    attestation_event_id, details_json, normalize_note, prepare_attestation, router,
    sensitive_evidence, valid_command_key, valid_set_id, valid_slug, AttestationAuthority,
    AttestationCommand, AttestationContext, AttestationDecision, AttestationExecutor,
    AttestationFuture, AttestationTarget, AttestationTargetConfig, EvidenceLink,
    PgAttestationExecutor, PriorAttestation, WorkActor, MAX_BODY_BYTES,
    MAX_NOTE_UNITS, MAX_SET_ID_BYTES, MIN_NOTE_UNITS,
};

// Exact mount lines for the owner crate:
// `pub mod native_r3_attestation;` in `src/adapters/mod.rs` and
// `pub mod native_r3_attestation;` in `src/public/mod.rs`.
