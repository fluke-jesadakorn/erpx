//! Native migration lane 39 for brand.
//!
//! The owner-executed workspace-choice publication read: Rust owns the SQL,
//! the Company scope, the projection and the response shape. TypeScript is a
//! strict, fail-closed transport with no SQL and no fallback reader.

mod application;
mod domain;
mod evidence_application;
mod evidence_domain;
mod evidence_http;
mod evidence_postgres;
mod http;
mod postgres;

pub use application::{WorkspaceChoiceFuture, WorkspaceChoiceRead, read_workspace_choices};
pub use domain::{
    MAX_COMPANY_IDS, ScopeKind, WorkspaceChoice, valid_id, validate_company_ids,
};
pub use evidence_application::{
    BrandEvidenceFuture, BrandEvidenceRead, read_brand_evidence,
};
pub use evidence_domain::{
    BrandProfileRow, EvidenceCursor, EvidenceSearchRequest, project as project_evidence,
    validate as validate_evidence,
};
pub use evidence_http::{
    EvidenceTarget, evidence_router, evidence_router_for_holdings,
};
pub use evidence_postgres::{PgBrandEvidenceRead, rendered_sql};
pub use http::{LaneTarget, WorkspaceChoiceTarget, router, router_for_holdings};
pub use postgres::{
    PgWorkspaceChoiceRead, WORKSPACE_CHOICE_SQL_HEAD, WORKSPACE_CHOICE_SQL_TAIL,
    workspace_choice_sql,
};
