//! Pure typed-kind registry for every native Agent write. No IO, no SQL and no
//! authority resolution here: this is the owner allow list and arity fence.
use erpx_platform_shared_server::public::BusinessError;

pub const AG_PRIVATE_SESSION_INSERT: &str = "ag_private_session_insert";
pub const AG_SESSION_CONTROLS_INSERT: &str = "ag_session_controls_insert";
pub const AG_INVOCATION_EVIDENCE_INSERT: &str = "ag_invocation_evidence_insert";
pub const AG_PROVIDER_USAGE_INSERT: &str = "ag_provider_usage_insert";
pub const AG_WORKSPACE_TURN_INSERT: &str = "ag_workspace_turn_insert";
pub const AG_WORKSPACE_TURN_READY: &str = "ag_workspace_turn_ready";
pub const AG_WORKSPACE_TURN_FAILED: &str = "ag_workspace_turn_failed";
pub const AG_WORKSPACE_TURN_ENGLISH: &str = "ag_workspace_turn_english";
pub const AG_WORKSPACE_TURN_TRANSLATION_READY: &str = "ag_workspace_turn_translation_ready";
pub const AG_WORKSPACE_TURN_TRANSLATION_FAILED: &str = "ag_workspace_turn_translation_failed";

const KINDS: &[(&str, usize)] = &[
    (AG_PRIVATE_SESSION_INSERT, 10),
    (AG_SESSION_CONTROLS_INSERT, 4),
    (AG_INVOCATION_EVIDENCE_INSERT, 16),
    (AG_PROVIDER_USAGE_INSERT, 12),
    (AG_WORKSPACE_TURN_INSERT, 11),
    (AG_WORKSPACE_TURN_READY, 4),
    (AG_WORKSPACE_TURN_FAILED, 3),
    (AG_WORKSPACE_TURN_ENGLISH, 4),
    (AG_WORKSPACE_TURN_TRANSLATION_READY, 3),
    (AG_WORKSPACE_TURN_TRANSLATION_FAILED, 2),
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
            BusinessError::invalid("AGENT_OWNER_WRITE_INVALID", "Unsupported owner write kind.")
        })?;
    if value_count != expected {
        return Err(BusinessError::invalid(
            "AGENT_OWNER_WRITE_INVALID",
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
        assert!(supported(AG_INVOCATION_EVIDENCE_INSERT));
        assert!(!supported("unknown"));
        assert!(validate(AG_INVOCATION_EVIDENCE_INSERT, 16).is_ok());
        assert!(validate(AG_INVOCATION_EVIDENCE_INSERT, 15).is_err());
    }
}
