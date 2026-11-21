//! Files owns payment proof sets/manifests. Finance supplies its actual retained
//! target, current authority and before/after transaction proofs.
pub use crate::adapters::payment_proof_binding::{
    PaymentProofBindingFacts, PgPaymentProofOwner, PreparedPaymentProofBinding,
    RetainedPaymentProofBinding, lock_payment_proof_binding_sources, prepare_payment_proof_binding,
};
pub use crate::domain::payment_proof_binding::{
    PaymentProofBindingFeatures, PaymentProofBindingLink, PaymentProofBindingSet,
    PaymentProofManifestItem, prepare_payment_proof_binding as prepare_payment_proof_policy,
};
