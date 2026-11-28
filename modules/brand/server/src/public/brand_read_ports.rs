//! Explicit Brand owner read-port facade. Consumers plan an owner read by
//! registered kind; the returned exact statement runs on the caller's open unit.

pub use crate::adapters::native_read_sql::{
    plan_brand_read, BrandReadStatement, EvidenceFlags, RD_EVIDENCE_SEARCH,
    RD_METRIC_PROFILES_BRANCH, RD_METRIC_PROFILES_LABEL, RD_METRIC_UPDATES_BRANCH,
    RD_METRIC_UPDATES_MONTH, RD_WORKSPACE_CHOICE,
};
