//! Public Talent caller-unit read-plan facade. The `talent/command-plan`
//! router (mounted by composition) resolves these kinds; the TS bridge sends
//! only a kind and positional values, never SQL. This facade exposes the read
//! kinds and their resolver so composition can mount or verify the plan route
//! and PG parity checks can assert the exact owner statement without importing
//! adapter internals. No TypeScript consumer is removed by this facade.

pub use crate::adapters::native_talent_read_sql::{
    NativeTalentReadStatement, RD_CANDIDATE_EVENTS, RD_CANDIDATE_SOURCE_IMAGE,
    RD_CANDIDATE_SOURCE_IMAGE_FILE, RD_OFFER_EVENTS, RD_OFFER_SOURCE_IMAGE,
    RD_OFFER_SOURCE_IMAGE_FILE, RD_HANDOFF_SOURCE_IMAGE, RD_HANDOFF_SOURCE_IMAGE_FILE,
    RD_REQUISITION_SOURCE_IMAGE, RD_REQUISITION_SOURCE_IMAGE_FILE,
    RD_WORKSPACE_RECORD_SOURCE, read_statement,
};
