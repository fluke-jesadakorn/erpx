//! Native Records reference-search read and owner ports.
pub use crate::adapters::record_search::{BRANCH_SQL, DEPARTMENT_SQL, HOLDING_SQL, PgRecordSearch};
pub use crate::application::record_search::{
    RecordSearchHandler, RecordSearchSource, SearchAdmission, SearchFuture,
};
pub use crate::domain::record_search::{
    CONTRACT_VERSION, MAX_HITS, MAX_ID, MAX_QUERY_BYTES, MAX_QUERY_CHARS, MAX_SCAN, PrincipalHit,
    RecordHit, SearchActor, SearchQuery, SearchResponse, SearchTarget, configuration_invalid,
    request_invalid, unavailable, valid_id, valid_slug,
};
