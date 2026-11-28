//! Native migration lane 33 for platform_administration.
//!
//! Owner-local successor of the release child-image `jsonb_build_object` read
//! in `adapters/company-release-attachment-read-sources.ts`. The axum owner
//! executes the acceptance row and the reviews/events/revision aggregates; the
//! retained TypeScript adapter is a strict, fail-closed transport.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    ReleaseChildImageFuture, ReleaseChildImageRead, SharedReleaseChildImageRead, unavailable,
};
pub use domain::{
    MAX_COMPANY, MAX_ID, MAX_SLUG, ReleaseChildImage, SQL_SHA256, valid_company, valid_id,
    valid_slug,
};
pub use http::{LaneTarget, router, router_for_holdings, router_with_reader};
pub use postgres::{IMAGE_SQL, PgReleaseChildImageRead, rendered_image_sql};
