//! Pure typed-kind registry for every remaining native Manage ERPX write.
//! No IO, no SQL execution and no authority resolution happen here: this module
//! is the owner allow-list and arity fence. The owner adapter owns the SQL.
use erpx_server_shared::public::BusinessError;

pub const ME_ATTACHMENT_VERSION_STEP: &str = "me_attachment_version_step";
pub const ME_BETA_FEEDBACK_INSERT: &str = "me_beta_feedback_insert";
pub const ME_BETA_MEMBERSHIP_DELETE: &str = "me_beta_membership_delete";
pub const ME_BETA_MEMBERSHIP_INSERT: &str = "me_beta_membership_insert";
pub const ME_BETA_INVITATION_INSERT: &str = "me_beta_invitation_insert";
pub const ME_BETA_INVITATION_ACCEPT: &str = "me_beta_invitation_accept";

/// (kind, exact positional arity). A request with any other kind or a
/// different value count is rejected before it reaches the adapter.
const KINDS: &[(&str, usize)] = &[
    (ME_ATTACHMENT_VERSION_STEP, 3),
    (ME_BETA_FEEDBACK_INSERT, 5),
    (ME_BETA_MEMBERSHIP_DELETE, 1),
    (ME_BETA_MEMBERSHIP_INSERT, 3),
    (ME_BETA_INVITATION_INSERT, 8),
    (ME_BETA_INVITATION_ACCEPT, 3),
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
                "MANAGE_ERPX_OWNER_WRITE_INVALID",
                "Unsupported owner write kind.",
            )
        })?;
    if value_count != expected {
        return Err(BusinessError::invalid(
            "MANAGE_ERPX_OWNER_WRITE_INVALID",
            "Unexpected owner write value count.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_list_is_exact() {
        assert!(supported(ME_BETA_FEEDBACK_INSERT));
        assert!(!supported("unknown"));
        assert!(validate(ME_BETA_FEEDBACK_INSERT, 5).is_ok());
        assert!(validate(ME_BETA_FEEDBACK_INSERT, 4).is_err());
        assert!(validate("unknown", 5).is_err());
    }
}
