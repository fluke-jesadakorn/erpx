//! Public owner exports for the R3 native profile-media byte read.
//!
//! The lead mounts `router` in composition and adds the gateway entry; no
//! TypeScript route, re-export or shared config is changed by this module.
pub use crate::adapters::native_r3_profile_media::{
    handle, router, AVATAR_SQL, COVER_SQL, MediaBytes, MediaKind, MediaRow, ProfileMediaAuthority,
    ProfileMediaContext, ProfileMediaFileStore, ProfileMediaRowStore, ProfileMediaTarget,
    PgProfileMediaRowStore,
};

// Exact owner mount line: `pub mod native_r3_profile_media;`
