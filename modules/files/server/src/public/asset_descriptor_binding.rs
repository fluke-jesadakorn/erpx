//! Public Files-side native Asset descriptor binding seam.
//!
//! The lead implements [`AssetDescriptorPort`] in the composition root with the
//! Assets owner port and mounts [`bind_asset_attachment_target_scope`] as the
//! `asset` branch of the Files attachment read authority.
pub use crate::adapters::asset_descriptor_binding::{
    AssetDescriptorEvidence, AssetDescriptorFacts, AssetDescriptorPort, AssetReadAccess,
    bind_asset_attachment_target_scope,
};
