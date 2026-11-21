//! Supplier Promise evidence; the caller supplies real same-unit owner proofs.
pub use crate::adapters::supplier_promise_binding::{
    GuardedSupplierPromiseEvidence, RetainedSupplierPromiseEvidence, SupplierEvidenceFeatures,
    SupplierPromiseEvidence, SupplierPromiseEvidenceError, SupplierPromiseEvidenceFacts,
    SupplierPromiseEvidenceOwner, SupplierPromiseManifest, apply_supplier_promise_evidence,
    lock_supplier_promise_evidence_sources, prepare_supplier_promise_evidence,
    validate_supplier_promise_evidence_guards,
};
