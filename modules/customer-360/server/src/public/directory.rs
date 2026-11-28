//! Native Customer 360 customer directory read ports. Composition supplies the
//! current Account session, selected Company/branch/department scope and the
//! serializable Holding transaction; this owner only validates and pages the
//! bounded read.
pub use crate::adapters::directory::PgCustomerDirectoryRead;
pub use crate::application::directory::{
    CustomerDirectoryFuture, CustomerDirectoryRepository, customer_directory,
    customer_directory_page,
};
pub use crate::domain::directory::{
    CustomerDirectoryCursor, CustomerDirectoryEntry, CustomerDirectoryPage, CustomerDirectoryQuery,
    CustomerDirectoryView, MAX_DIRECTORY,
};
pub use crate::domain::timeline::CustomerTimelineActor;
