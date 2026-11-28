//! Public owner port for the native public Marketing form source read.
//!
//! Stable re-exports for composition and the Next transport. No TypeScript
//! implementation backs this operation: the pool-backed owner source is the
//! authority and the router is mounted by composition.

pub use crate::adapters::native_public_form::postgres::SOURCE_SQL;
pub use crate::adapters::native_public_form::{
    FormQuery, FormRow, PgPublicFormSource, PublicFormFuture, PublicFormSource, PublicFormTarget,
    SOURCE_KEYS, missing, read, rendered_sql, router, router_for_holdings, router_with_reader,
    router_with_sources, valid_id, valid_row, validate,
};
