//! Organization structural facts are not authorization evidence.
pub use crate::adapters::business_acceptance::{
    CompanyBusinessAcceptance, PgCompanyBusinessAcceptanceReader, lock_business_acceptance_sources,
};
pub use crate::adapters::postgres_authority_seats::{
    CurrentOrganizationSalesAuthorityBoundary, PgOrganizationAuthorityReader,
    inspect_organization_sales_authority_boundary, lock_organization_authority_sources,
    require_organization_authority_sources_held,
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

pub mod commercial_companies;
pub mod field_service_workspace;

pub mod pms_source_branch;

pub mod organization_nodes;

pub mod organization_setup;
pub mod organization_setup_http;

pub mod member_profile;
pub mod organization_owner_write;
pub mod organization_access;
pub mod pos_issuer;
pub mod position;
pub mod position_moves;
pub mod records_request_department_head;
pub mod rfq_request_scope;
pub mod staff_department_route;

/// Organization-owned placement port composed into native People commands.
pub mod people_staff_placement {
    pub use crate::adapters::people_staff_placement::{
        StaffPlacementRequest, apply_staff_placement, into_business,
    };
}
