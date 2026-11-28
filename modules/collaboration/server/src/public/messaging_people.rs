//! Stable owner exports for the native Messaging people picker.
//!
//! Composition binds one [`PgMessagingPeopleRead`] per registered Holding and
//! returns the projection from its `MessagingHttpBackend::people` override.

pub use crate::adapters::messaging_people_pg::{PgMessagingPeopleRead, fetch_active_people, sql_active_people};
pub use crate::application::messaging_people::{
    project, project_people, MessagingPeopleFuture, MessagingPeopleRead, PeopleService,
};
pub use crate::domain::messaging_people::{PeopleQuery, Person, PersonRow};

pub const CONTRACT_VERSION: &str = "collaboration.messaging.people.v1";
