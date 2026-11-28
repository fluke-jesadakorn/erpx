//! Public surface for the native handover acknowledgement read. Composition
//! mounts the router; TypeScript consumers keep a strict transport, never the
//! retired `handoverAcknowledgement` SQL.

pub use crate::adapters::native_r3_handover_ack::{
    configured, router, router_for_holdings, AckTarget, PgHandoverAckSource,
};
pub use crate::application::native_r3_handover_ack::{
    AckFuture, HandoverAckHandler, HandoverAckRead, HandoverPostSource,
};
pub use crate::domain::native_r3_handover_ack::{
    not_found, project, render_acknowledgements_sql, validate_post_id, validate_user_id,
    ACKNOWLEDGEMENTS_HEAD, RECIPIENTS_SQL, HandoverAcknowledgementView, HandoverPostFacts,
};
