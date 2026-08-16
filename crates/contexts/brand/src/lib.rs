#![doc = "Brand bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    BrandApplicationError, BrandCommandHandler, BrandMutationResult, BrandQueryHandler,
    CreateBrandDraft, CreateBrandDraftResult, GetBrandAsset, GetBrandAssetResult, GetBrandDraft,
    GetBrandDraftResult, GetBrandPublication, GetBrandPublicationResult, GetResolvedBrand,
    GetResolvedBrandResult, ListBrandHistory, PublishBrandDraft, RollbackBrand, SaveBrandDraft,
    SetBrandCategoryLocks,
};
pub use domain::{
    ArchivedBrandRevision, BrandAssetKind, BrandAssetManifest, BrandAssetReference, BrandCategory,
    BrandDefinition, BrandDensity, BrandDomainError, BrandElevation, BrandFontPack, BrandLifecycle,
    BrandMotion, BrandNavigation, BrandPalette, BrandProfile, BrandProfileId, BrandRadius,
    BrandRevision, BrandScope, BrandTokenSet, BusinessUnitKey, HexColor, ParentBrandWitness,
    ResolvedBrandDefinition, resolve_published_brand,
};
pub use ports::{
    AuthorizationPort, AuthorizationRequest, BrandAction, BrandAssetView, BrandCommit,
    BrandHistory, BrandHistoryEntry, BrandProfileView, BrandPublicationView, BrandStoredCommit,
    CommandFingerprint, CommandIdentity, CommandUnitOfWork, DerivedRecordReadRepository,
    EntitlementPort, ParentAuthorityPort, PortError, ReadProjection, ReadRepository,
    ResolvedBrandView, WriteRepository,
};
