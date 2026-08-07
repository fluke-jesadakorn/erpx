#![doc = "Assets bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    AssetApplicationError, AssetCommandHandler, AssetMutationResult, AssetQueryHandler,
    BeginMaintenance, CreateAsset, CreateAssetResult, GetAsset, GetAssetResult, ListAssets,
    MarkAssetDegraded, PlaceAssetInService, RetireAsset, ReturnAssetToService,
};
pub use domain::{
    Asset, AssetDomainError, AssetId, AssetLifecycle, AssetScope, AssetStatus, DomainEvent,
};
pub use ports::{
    AssetAction, AssetCommit, AssetList, AssetListFilter, AssetStoredCommit, AssetView,
    AuthorizationPort, CommandFingerprint, CommandIdentity, CommandUnitOfWork, EntitlementPort,
    PortError, ReadProjection, ReadRepository, WriteRepository,
};
