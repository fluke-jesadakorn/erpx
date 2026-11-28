//! Public Finance attachment version-guard port.
//!
//! The composition mounts these functions on the caller's serializable unit;
//! this crate never opens a connection or commits. The allow-list rules are
//! exported for contract checks and future deletion assertions.
pub use crate::adapters::attachment_version::{
    AttachmentVersionActor, AttachmentVersionStep, VERSION_STEP_OPERATION,
    assert_attachment_version, bump_attachment_version,
};
pub use crate::domain::attachment_version::{
    AttachmentVersionFamily, AttachmentVersionTarget, attachment_version_target,
    invalid_expected_version, scope_forbidden, target_changed, target_not_found,
    unsupported_record, unsupported_target,
};
