//! Native Benefits settlement event-history read: owner-local axum router,
//! sqlx reader and strict TS client. No TypeScript fallback is retained.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{ReadSettlementEvents, SettlementEventRead, SettlementEventsFuture};
pub use domain::{
    LaneTarget, SettlementEventFact, SettlementEventsRequest, SettlementEventsResponse,
};
pub use http::{router_for_holdings, router_with_reader};
pub use postgres::{PgSettlementEventRead, SOURCE_SQL};
