//! Raw Quality attachment target facts; authorization remains with the caller.
pub use crate::adapters::attachment_descriptor::{
    CurrentQualityAttachmentDescriptor, QualityAttachmentDescriptor,
    inspect_quality_attachment_descriptor, lock_quality_attachment_descriptor,
};
pub use crate::adapters::attachment_descriptor_read::{
    QualityAttachmentDescriptorRow, QualityAttachmentDescriptorTarget,
    read_quality_attachment_descriptor,
};
