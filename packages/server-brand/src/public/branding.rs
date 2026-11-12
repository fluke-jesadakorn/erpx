//! Native Branding owner facade. Consumers import only these named exports;
//! no SQL relation or internal adapter path escapes the crate.
pub use crate::adapters::branding::{BrandingError, PgBranding};
pub use crate::adapters::branding_use_cases::{
    BrandingMutation, BrandingMutationAction, BrandingMutationCommon, BrandingPorts,
    BrandingPresentationPorts, attach_brand_media, list_brand_profiles, read_brand_profile_evidence,
    read_branding_workspace, read_storefront_app_brand, resolve_document_brand,
    resolve_workspace_brand, update_brand_profile, update_storefront,
};
pub use crate::adapters::identity_use_cases::BrandActor;
pub use crate::domain::branding::{
    BrandMediaInput, BrandMediaUrls, BrandPresentationContext, BrandProfileEvidence, BrandScopeKind,
    BrandVariant, BrandingAttachment, BrandingBranch, BrandingProfile, BrandingProfiles,
    BrandingPublication, BrandingStorefront, BrandingWorkspace, DocumentBrand, DocumentMode,
    StorefrontAppBrand, UpdateProfileInput, UpdateStorefrontInput, WorkspaceBrand,
    brand_presentation, storefront_presentation,
};
