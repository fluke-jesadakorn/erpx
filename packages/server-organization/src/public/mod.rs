//! Organization structural facts are not authorization evidence.
pub use crate::adapters::business_acceptance::{
    CompanyBusinessAcceptance, PgCompanyBusinessAcceptanceReader, lock_business_acceptance_sources,
};
pub use crate::adapters::postgres_authority_seats::{
    PgOrganizationAuthorityReader, lock_organization_authority_sources, require_organization_authority_sources_held,
    CurrentOrganizationSalesAuthorityBoundary, inspect_organization_sales_authority_boundary,
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
pub mod commercial_companies;

pub mod pms_source_branch;

pub mod organization_nodes;

pub mod organization_setup;
pub mod organization_setup_http;

pub mod pos_issuer;
pub mod records_request_department_head;
pub mod staff_department_route;
pub mod organization_access;
pub mod position_moves;
