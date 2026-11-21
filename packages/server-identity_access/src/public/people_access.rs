//! Native People relationship metadata commands.
//!
//! Organization, team and external relationship metadata only. Privilege
//! commands (account preparation, roles, grants, invitations, revocation) stay
//! on the separate IAM owner command.
pub use crate::adapters::people_access_postgres::PgPeopleRelationships;
pub use crate::application::people_access::{
    PeopleAccessFuture, PeopleRelationshipRepository, apply_relationship, preview_relationship,
};
pub use crate::domain::people_access::{
    MAX_NAME, MAX_REASON, PREVIEW_LIFETIME_MS, PeopleAccessActor, RELATIONSHIP_APPLY_OPERATION,
    RELATIONSHIP_MANAGE_PERMISSION, RELATIONSHIP_READ_PERMISSION, RelationshipChange,
    RelationshipSnapshot, access_object_unavailable, preview_summary, validate_change,
};
