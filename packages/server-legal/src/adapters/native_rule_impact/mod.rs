//! Native migration slice for the Legal rule-impact read.
//!
//! `GET .../legal/rule-impact/{id}` was the last plan-only read left in the
//! Legal auxiliary tail: TypeScript held the SQL contract and prepared/executed
//! it. This module moves the whole operation into the owner: pure types and
//! invariants in `domain`, the query boundary and authority gate in
//! `application`, the real Company-scoped sqlx statements in `postgres`, and the
//! Axum ingress in `http`. No TypeScript statement or fallback is involved.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    RuleImpactAuthority, RuleImpactSource, read_rule_impact,
};
pub use domain::{
    IMPACT_LIMIT, NOTICE, RULE_IMPACT_OPERATION, RuleHead, RuleImpactItem, RuleImpactRead,
    RuleImpactTarget, valid_id, valid_slug,
};
pub use http::{router, router_for_holdings, router_with_reader};
pub use postgres::{PgRuleImpactSource, SQL_ENABLED, SQL_IMPACTS, SQL_RULE};
