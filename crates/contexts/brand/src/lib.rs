#![doc = "Brand bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    BrandApplicationError, BrandCommandHandler, BrandMutationResult, BrandQueryHandler,
    CreateBrandDraft, CreateBrandDraftResult, GetBrandDraft, GetBrandDraftResult, GetResolvedBrand,
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
    AuthorizationPort, AuthorizationRequest, BrandAction, BrandCommit, BrandHistory,
    BrandHistoryEntry, BrandProfileView, BrandStoredCommit, CommandFingerprint, CommandIdentity,
    CommandUnitOfWork, EntitlementPort, ParentAuthorityPort, PortError, ReadProjection,
    ReadRepository, ResolvedBrandView, WriteRepository,
};
