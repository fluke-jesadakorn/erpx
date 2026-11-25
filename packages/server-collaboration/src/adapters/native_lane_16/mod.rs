//! Native migration lane 16 for collaboration: sent-notice facts.
//!
//! The TypeScript adapter no longer executes these four SELECTs. It transports
//! the complete native projection and fails closed when the owner route is
//! unavailable. Composition mounts `router` on the private native API.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    LaneReadFuture, PushReadRequest, PushSettingsRead, PushSettingsService, ReadRequest,
    SentNoticeRead, SentNoticeService,
};
pub use domain::{
    PushReadKind, ReadKind, SentNoticeDetail, SentNoticeRow, TaskOrigin, KIND_DELIVERY_COUNT,
    KIND_NOTICE, KIND_NOTICES, KIND_PUSH_ENDPOINT_OWNER, KIND_PUSH_PREFERENCES,
    KIND_PUSH_SESSION_COUNT, KIND_TASK_ORIGIN, valid_event_key, valid_identifier,
};
pub use http::{LaneTarget, router, router_for_holdings, router_with_reader};
pub use postgres::{
    PgPushSettingsRead, PgSentNoticeRead, RESULT_ORACLE_JSON, SQL_DELIVERY_COUNT, SQL_NOTICE,
    SQL_NOTICES, SQL_PUSH_ENDPOINT_OWNER, SQL_PUSH_PREFERENCES, SQL_PUSH_SESSION_COUNT,
    SQL_TASK_ORIGIN, SQL_TASK_ORIGIN_SCOPED, hash, render_sql,
};
