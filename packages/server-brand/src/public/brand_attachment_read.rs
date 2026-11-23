//! Native Brand attachment-source image contract.
//!
//! Consumers use these named exports; no SQL relation or private adapter type
//! escapes the crate. The image pair is bounded to the same transaction that
//! authorizes the Files attachment read.
pub use crate::adapters::brand_attachment_read_postgres::read_brand_attachment_image;
pub use crate::domain::brand_attachment_read::{
    BrandAttachmentKind, BrandAttachmentRead, validate_read,
};
