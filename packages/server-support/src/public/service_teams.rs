//! Native service-team contract.
//!
//! Composition supplies the current Holding-owner authority, Organization
//! membership/scope facts, People member directory and workflow receipt. This
//! port owns the team/revision/member/coverage validation and writes.
pub const CONTRACT_VERSION: &str = "support.service-teams.v1";

pub use crate::adapters::service_teams_pg::PgServiceTeams;

pub use crate::adapters::service_teams::{
    apply_plan, authority_snapshot, company_member, current_team, lock_service_team_sources,
    organization_node, overlapping_teams, replay, service_member, service_team_options,
    validate_service_scope,
};

pub use crate::application::service_teams::{
    OrgNode, RecordScope, ServiceTeamActor, ServiceTeamPlan, ServiceTeamPorts, ServiceTeamPreview,
    ServiceTeamResult, ServiceTeamRow, in_service_workspace, parse_service_team, preview, save,
};

pub use crate::domain::service_teams::{
    Coverage, Member, ServiceTeamInput, normalize_service_team, validate_local_team_rules,
};
