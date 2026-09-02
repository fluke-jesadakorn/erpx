//! Pure domain rules for the universal record reference graph.
//! No transport/DB deps. Validates append-only, deduped, ordered, cap-50.

use thiserror::Error;
use uuid::Uuid;

pub const MAX_REFERENCE_EDGES_PER_HOST: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReferenceGraphError {
    #[error("reference target already exists on host (deduped)")]
    DuplicateTarget,
    #[error("reference graph cap 50 per host exceeded (soft cap)")]
    CapExceeded,
    #[error("ordinal out of range")]
    OrdinalOutOfRange,
    #[error("host and target must differ")]
    SelfReference,
}

pub fn validate_add(
    existing_target_ids: &[Uuid],
    target: Uuid,
    host: Uuid,
) -> Result<u8, ReferenceGraphError> {
    if host == target {
        return Err(ReferenceGraphError::SelfReference);
    }
    if existing_target_ids.contains(&target) {
        return Err(ReferenceGraphError::DuplicateTarget);
    }
    if existing_target_ids.len() >= MAX_REFERENCE_EDGES_PER_HOST {
        return Err(ReferenceGraphError::CapExceeded);
    }
    Ok(existing_target_ids.len() as u8)
}

pub fn validate_reorder(existing: &[Uuid], ordered: &[Uuid]) -> Result<(), ReferenceGraphError> {
    if existing.len() != ordered.len() {
        return Err(ReferenceGraphError::OrdinalOutOfRange);
    }
    let mut s = std::collections::BTreeSet::new();
    for r in ordered {
        if !s.insert(r) {
            return Err(ReferenceGraphError::DuplicateTarget);
        }
    }
    for r in existing {
        if !ordered.contains(r) {
            return Err(ReferenceGraphError::OrdinalOutOfRange);
        }
    }
    Ok(())
}
