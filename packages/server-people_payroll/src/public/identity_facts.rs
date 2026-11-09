pub use crate::adapters::identity_facts::{PgPeopleAuthorityReader, lock_people_authority_sources, require_people_authority_sources_held};
pub use crate::adapters::identity_facts::{CurrentPeopleSalesAuthorityBoundary, inspect_people_sales_authority_boundary};
pub use crate::application::identity_facts::PeopleAuthorityReadPort;
pub use crate::domain::identity_facts::{EngagementAuthorityWitness, PersonAuthorityFacts};
