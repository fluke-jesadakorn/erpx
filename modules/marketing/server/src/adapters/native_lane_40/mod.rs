//! Native migration lane 40 for marketing.
//!
//! The owner now executes and projects six live owner-local reads itself:
//! candidate list, candidate detail, engagement history, the attribution
//! summary, the active-form source and the form-content row. The retained
//! TypeScript files only transport results; there is no SQL fallback and no
//! cross-owner projection in this lane.
pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{Lane40ReadFuture, Lane40Reader, read};
pub use domain::{
    AttributionRow, KINDS, Lane40ReadInput, Lane40ReadResult, Lane40Request,
    RD_ACTIVE_FORM_SOURCE, RD_CAMPAIGN_SUMMARY, RD_CANDIDATE_DETAIL, RD_CANDIDATE_HISTORY,
    RD_CANDIDATE_LIST, RD_FORM_CONTENT_ROW, RD_PUBLIC_RECEIPT,
};
pub use http::{LaneTarget, router, router_for_holdings};
pub use postgres::PgLane40Reader;
