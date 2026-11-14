//! Native auxiliary Legal reads, retention commands and trust HTTP contracts.
//!
//! Composition mounts these owner ports in the exact registered Holding target
//! and supplies the current Account session plus one serializable Holding unit.
//! The retained TypeScript GET surface is replaced only for the reads exported
//! here; unported paths are named in `UNPORTED_READS` and must keep the retained
//! route until their own native slice lands.
pub use crate::adapters::legal_http_auxiliary::{
    LegalAuxiliaryHttpBackend, LegalAuxiliaryHttpFuture, LegalAuxiliaryHttpRequest,
    LegalAuxiliaryHttpService, LegalAuxiliaryHttpTarget, LegalAuxiliaryQueryWire,
    UNPORTED_READS, legal_auxiliary_router,
};
pub use crate::adapters::postgres_auxiliary_reads::{
    PgLegalAuxiliaryReads, legal_auxiliary_business_error, require_auxiliary_transaction,
};
pub use crate::adapters::legal_http_retention::{
    LegalRetentionActorFuture, LegalRetentionCommandReply, LegalRetentionHttpBackend,
    LegalRetentionHttpCommandRequest, LegalRetentionHttpFuture, LegalRetentionHttpService,
    LegalRetentionHttpSessionRequest, LegalRetentionHttpTarget, RETENTION_BODY_LIMIT,
    legal_retention_router,
};
pub use crate::adapters::postgres_retention::{
    PgLegalRetention, execute_retention, legal_retention, legal_retention_business_error,
};
pub use crate::application::auxiliary::{
    LegalAuxiliaryError, LegalAuxiliaryFuture, LegalAuxiliaryReadPorts, parse_auxiliary_path,
    read_auxiliary,
};
pub use crate::application::retention::{
    LegalRetentionPorts, RetentionCommand, RetentionDestructionRow, RetentionError,
    RetentionFuture, RetentionPolicyRow, execute_retention_command, parse_retention_command,
};
pub use crate::domain::auxiliary::{
    AUXILIARY_CHUNK_LIMIT, AUXILIARY_ID_MAX, AUXILIARY_JOB_LIMIT, AUXILIARY_ROW_LIMIT,
    LegalAuxiliaryActor, LegalAuxiliaryRead, LegalRetentionAction, READ_PERMISSION, REASON_MAX,
    REASON_MIN, RETENTION_DAYS_MAX, RETENTION_POLICY_PERMISSION, RETENTION_RECORD_PERMISSION,
    TRUST_VIEW_PERMISSION, auxiliary_command_selected, authorize_read, bangkok_date,
    parse_as_of, parse_identifier, parse_search, provider_gated, provider_unavailable,
    visibility_for, visibility_predicate,
};
