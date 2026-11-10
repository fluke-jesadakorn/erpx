//! Native Legal trust material HTTP contract.
//!
//! This is the missing owner-side transport for the already-native
//! `execute_legal_trust_command`. Composition mounts it, resolves the current
//! Account session to `LegalTrustActor`, calls `authorize_trust_actor` and then
//! `execute_legal_trust_command` on the same serializable Holding transaction.
//! Offline PDF trust/revocation/timestamp/modification and the three-state
//! verifier are unchanged; provider-dependent signing stays fail-closed.
pub use crate::adapters::legal_trust_http::{
    LegalTrustCommandReply, LegalTrustCommandWire, LegalTrustHttpBackend,
    LegalTrustHttpCommandRequest, LegalTrustHttpService, LegalTrustHttpTarget,
    authorize_trust_actor, legal_trust_router, parse_trust_command,
};
