//! Native Records context read and owner-local HTTP ingress.
pub use crate::adapters::record_context::PgRecordContext;
pub use crate::application::record_context::{
    RecordContextFuture, RecordContextGuard, RecordContextRepository, record_context,
};
pub use crate::domain::record_context::{
    BacklinkEdge, BacklinkView, CONTRACT_VERSION, HistoryRow, LinkTarget, MentionRow,
    RecordContextActor, RecordContextDocument, RecordRow, ReferenceEdge, ReferenceView,
    SuggestionRow, context_changed, context_envelope, module_forbidden, permission_required,
    record_not_found, request_invalid, source_invalid, validate_request,
};
