//! Ordered Count review detail. This is not an authority grant; the caller must
//! retain the corresponding scoped Commerce master witness through commit.
pub use crate::adapters::service_initial_count_decision::{
    CurrentInitialCountDecisionDetail, InitialCountDecisionDetail,
    read_initial_count_decision_detail,
};
