//! Owner-local Records reference-search HTTP ingress.
pub use crate::adapters::record_search_http::{
    RecordSearchBackend, router_for_holdings, validate_targets,
};
