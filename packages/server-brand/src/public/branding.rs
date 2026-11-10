//! Native Branding owner facade. Consumers import only these named exports;
//! no SQL relation or internal adapter path escapes the crate.
pub use crate::adapters::branding::{BrandingError, PgBranding};
pub use crate::adapters::branding_use_cases::{
    BrandingMutation, BrandingMutationAction, BrandingMutationCommon, BrandingPorts,
    attach_brand_media, read_branding_workspace, update_brand_profile, update_storefront,
};
pub use crate::adapters::identity_use_cases::BrandActor;
pub use crate::domain::branding::{
    BrandMediaInput, BrandScopeKind, BrandVariant, BrandingAttachment, BrandingBranch,
    BrandingProfile, BrandingPublication, BrandingStorefront, BrandingWorkspace, DocumentMode,
    UpdateProfileInput, UpdateStorefrontInput,
};
