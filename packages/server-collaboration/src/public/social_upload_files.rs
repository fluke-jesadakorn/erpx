//! Public Social upload cleanup port contract.
//!
//! Composition binds the Files owner implementation for canceled Social draft
//! uploads; this package owns the request sequencing and never writes Files
//! tables itself.
pub use crate::adapters::social_upload_files::{
    SocialUploadFilesFuture, SocialUploadFilesPort, UnavailableSocialUploadFiles,
};
