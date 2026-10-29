//! Organization structural facts are not authorization evidence.
pub use crate::adapters::business_acceptance::{
    CompanyBusinessAcceptance, PgCompanyBusinessAcceptanceReader, lock_business_acceptance_sources,
};
pub use crate::adapters::postgres_authority_seats::{
    PgOrganizationAuthorityReader, lock_organization_authority_sources,
};
pub use crate::adapters::postgres_hierarchy::PgOrganizationReader;
pub use crate::adapters::postgres_hierarchy::lock_hierarchy_sources;
pub use crate::application::hierarchy::{
    OrganizationReadFuture, OrganizationReadPort, read_company_context, resolve_workspace,
};
pub use crate::domain::authority_seats::HrAuthoritySeat;
pub use crate::domain::hierarchy::{
    CompanyContext, HierarchyLineage, NodeKind, NodeStatus, OrganizationNode, WorkspaceRouteQuery,
    company_context,
};

pub mod journal_flow_department;

pub mod field_service_agreement;

pub mod field_service_workspace;
