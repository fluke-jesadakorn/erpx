//! Native Records drawer-detail (supplement) read.
pub use crate::adapters::record_context::PgRecordContext;
pub use crate::application::record_details::{RecordDetailsRepository, record_details};
pub use crate::domain::record_details::{
    CONTRACT_VERSION, MAX_PAGE, PAGE_SIZE, RecordDetailsQuery, details_wire, history_page,
};
