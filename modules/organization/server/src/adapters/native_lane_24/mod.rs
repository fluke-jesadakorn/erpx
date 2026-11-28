//! Native migration lane 24 for organization: the evaluation-reference read
//! family. The axum router owns the SQL, the Company-scoped bind, the typed
//! projection and the response shape; TypeScript is a strict, fail-closed
//! transport. No SQL fallback exists in the TypeScript consumer.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{EvaluationFactsReader, FactsQuery, ReadFuture};
pub use domain::{
    AccessNode, BranchOption, EVAL_LEVEL_ACTIVE, EVAL_MARKET_LEVELS, EVAL_POSITION_BY_LEVEL,
    EVAL_WORKSPACE_LEVELS, EVAL_WORKSPACE_POSITIONS, FactsEnvelope, KINDS, LevelRef, MarketLevel,
    ORG_ACCESS_NODES, ORG_BRANCH_OPTIONS_ALL, ORG_BRANCH_OPTIONS_POS, ORG_BRANCH_OPTIONS_SCOPED,
    PositionRef, ReadResult, WorkspacePosition, valid_company, valid_id,
};
pub use http::{LaneTarget, router, router_for_holdings};
pub use postgres::PgEvaluationFacts;
