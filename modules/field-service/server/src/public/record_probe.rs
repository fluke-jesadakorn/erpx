//! Native Field record-probe contract consumed by the retained Records
//! partition. The caller supplies only a closed kind; no SQL or relation name
//! crosses this boundary, and no TypeScript SQL duplicate remains.
pub use crate::adapters::record_probe::{
    FIELD_RECORD_PROBE_SOURCES_JSON, field_record_probe,
};
