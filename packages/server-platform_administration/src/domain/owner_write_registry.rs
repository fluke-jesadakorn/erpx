//! Pure typed-kind registry for every native Platform Administration write.
//! No IO, no SQL and no authority resolution here: this is the owner allow
//! list and exact arity fence. The owner adapter holds all SQL.
use erpx_server_shared::public::BusinessError;

pub const PL_RELEASE_REVISION_INIT: &str = "pl_release_revision_init";
pub const PL_LEGAL_RELEASE_REVISION_INIT: &str = "pl_legal_release_revision_init";
pub const PL_RELEASE_ACCEPTANCE_INSERT: &str = "pl_release_acceptance_insert";
pub const PL_RELEASE_GATE_REVIEW_UPSERT: &str = "pl_release_gate_review_upsert";
pub const PL_RELEASE_VERSION_STEP: &str = "pl_release_version_step";
pub const PL_RELEASE_REVOKE: &str = "pl_release_revoke";
pub const PL_ATTACHMENT_VERSION_STEP: &str = "pl_attachment_version_step";

const KINDS: &[(&str, usize)] = &[
    (PL_RELEASE_REVISION_INIT, 1),
    (PL_LEGAL_RELEASE_REVISION_INIT, 1),
    (PL_RELEASE_ACCEPTANCE_INSERT, 12),
    (PL_RELEASE_GATE_REVIEW_UPSERT, 9),
    (PL_RELEASE_VERSION_STEP, 4),
    (PL_RELEASE_REVOKE, 5),
    (PL_ATTACHMENT_VERSION_STEP, 3),
];

pub fn supported(kind: &str) -> bool {
    KINDS.iter().any(|(name, _)| *name == kind)
}

pub fn validate(kind: &str, value_count: usize) -> Result<(), BusinessError> {
    let expected = KINDS
        .iter()
        .find(|(name, _)| *name == kind)
        .map(|(_, arity)| *arity)
        .ok_or_else(|| {
            BusinessError::invalid(
                "PLATFORM_OWNER_WRITE_INVALID",
                "Unsupported owner write kind.",
            )
        })?;
    if value_count != expected {
        return Err(BusinessError::invalid(
            "PLATFORM_OWNER_WRITE_INVALID",
            "Unexpected owner write value count.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_list_is_exact_and_arity_fenced() {
        assert!(supported(PL_RELEASE_ACCEPTANCE_INSERT));
        assert!(!supported("unknown"));
        assert!(validate(PL_RELEASE_ACCEPTANCE_INSERT, 12).is_ok());
        assert!(validate(PL_RELEASE_ACCEPTANCE_INSERT, 11).is_err());
    }
}
