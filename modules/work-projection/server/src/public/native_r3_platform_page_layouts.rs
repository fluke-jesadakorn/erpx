//! R3 native platform page-layout read contract for composition. The lead
//! registers the `adapters` and this `public` module and mounts
//! [`router`] with the exact registered Platform hosts, one Control pool and an
//! authority implementation that resolves the current platform session,
//! Company scope, widget allow-list and publish flag.

pub use crate::adapters::native_r3_platform_page_layouts::{
    project, router, router_with_reader, validate, ContextFuture, LayoutFuture,
    PlatformLayoutAuthority, PlatformLayoutContext, PlatformLayoutQuery, PlatformLayoutRead,
    PgPlatformLayoutRead,
};
