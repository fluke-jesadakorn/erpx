//! Files-owned Social draft upload cleanup port.
//!
//! Collaboration supplies the canceled draft identity on its retained
//! serializable connection; Files owns the attachment revocation and upload
//! intent expiry. See [`revoke_social_draft_uploads`].
pub use crate::adapters::social_draft_uploads::{
    SocialCommentFilesRequest, SocialDraftUploadRevokeRequest, retain_social_comment_images,
    revoke_social_draft_uploads,
};
