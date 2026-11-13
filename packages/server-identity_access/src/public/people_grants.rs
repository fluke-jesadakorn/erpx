//! Native People access grant, custom-role and invitation commands.
//!
//! Privilege commands only. Relationship metadata (organizations, teams and
//! external relationships) stays on [`crate::public::people_access`].
pub use crate::adapters::people_grants_postgres::{
    DirectInvitationDelivery, PgPeopleGrants, read_direct_invitation,
    stage_direct_invitation_delivery,
};
pub use crate::adapters::permission_catalog::permission_catalog;
pub use crate::application::people_grants::{
    PeopleGrantFuture, PeopleGrantRepository, apply_grant_change, preview_grant_change,
};
pub use crate::domain::people_grants::{
    ACCESS_APPLY_PERMISSION, GRANT_APPLY_OPERATION, GrantApplyPlan, GrantSnapshot, PeopleGrantActor,
    PeopleGrantChange, ROLE_MANAGE_PERMISSION, access_object_unavailable, preview_summary,
    validate_change,
};
