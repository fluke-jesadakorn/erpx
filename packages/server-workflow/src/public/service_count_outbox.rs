//! Outer-composition ports for the scoped Service Count outbox worker.
pub use crate::adapters::service_count_outbox::{ServiceCountEvent, claim, eligible, complete, fail, project, renew};
