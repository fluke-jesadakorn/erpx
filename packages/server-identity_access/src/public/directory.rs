//! Directory observations/credential ports are not an Actor or a Work grant.
pub use crate::application::directory::{
    DirectoryAccountIssuance, DirectoryAdmissionError, DirectoryAttemptCounter,
    DirectoryAuthentication, DirectoryIssueFuture, DirectoryPassword, DirectoryProviderRead,
    DirectoryRoutes, DirectorySynchronization, SynchronizationOutcome, VerifiedDirectorySubject,
};
pub use crate::domain::directory::{
    ApprovedDirectoryIdentity, CompleteDirectorySnapshot, DIRECTORY_FRESH_MS,
    DirectoryConnectionConfig, DirectoryConnectionDescriptor, DirectoryGroupEdge,
    DirectoryHierarchy, DirectoryMode, DirectoryPlacement, DirectoryProvider, ObservedContainer,
    ObservedDirectoryGroup, ObservedDirectoryUser,
};
pub use crate::domain::directory_provider_values::{
    InvalidDirectoryValue, LdapAccountControls, ad_guid, canonical_external_id,
};

#[cfg(feature = "directory-ldaps")]
pub mod framing {
    pub use crate::adapters::directory_ber::{
        BerFailure, BerFrameLength, MAX_BER_DEPTH, MAX_BER_ELEMENTS, MAX_BER_FRAME_BYTES,
        MAX_WIRE_BYTES, MAX_WIRE_FRAMES, WireBudget, frame_length, validate_complete_frame,
    };
}

#[cfg(feature = "directory-ldaps")]
pub use crate::adapters::directory_profiles::DirectoryProfileStore;

#[cfg(feature = "directory-ldaps")]
pub use crate::adapters::directory_ldap::LdapDirectoryProvider;

/// Standing mapping policy facts; validation alone creates no Assignment.
pub mod grants {
    pub use crate::domain::directory_grants::{
        DirectoryGrantFacts, DirectoryMemberFact, DirectoryParentFact, DirectoryRoleFact,
        DirectoryScopeFact, validate_mapping_grant,
    };
}

#[cfg(feature = "directory-ldaps")]
pub use crate::adapters::directory_postgres::{
    DirectoryDatabaseBindings, DirectoryHoldingDatabase, PostgresDirectory,
};

pub use crate::adapters::directory_http::directory_account_router;
#[cfg(feature = "contracts")]
pub use crate::adapters::directory_http::directory_openapi;

#[cfg(feature = "directory-ldaps")]
pub use crate::adapters::directory_work::DirectoryRefreshRuntime;

/// Preflight denial/observation rules only. These facts never create an Actor or
/// AuthorizedWorkContext; actual session/selection/IAM reads remain mandatory.
#[cfg(feature = "directory-ldaps")]
pub mod refresh_rules {
    pub use crate::domain::directory_refresh::{
        DirectoryRefreshDisposition, DirectoryRefreshMemberFacts, DirectoryRefreshSelectionFacts,
        classify_member, selection_fence,
    };
}
