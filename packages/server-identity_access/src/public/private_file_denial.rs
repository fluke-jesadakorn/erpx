//! Native private-file denial authority witness owner port.
//!
//! The caller opens the Holding unit; the adapter reads only the closed IAM,
//! Organization and People facts and returns an opaque equality token. No
//! caller SQL, permission grant or file byte crosses this boundary.
pub use crate::adapters::private_file_denial_postgres::{PgPrivateFileDenial, SQL_PRIVATE_FILE_DENIAL_IMAGE, TS_ORACLE_SQL};
pub use crate::application::private_file_denial::{
    PrivateFileDenialFuture, PrivateFileDenialRepository, private_file_denial_witness,
};
pub use crate::domain::private_file_denial::{
    PrivateFileDenialActor, MAX_ID, witness_hash, witness_image_required, witness_request_invalid,
};
