//! Native Legal record reads, document/contract lifecycle, holds and archive.
pub use crate::adapters::legal_http::{
    LegalCommandReply, LegalDetailReply, LegalExportReply, LegalHttpBackend,
    LegalHttpCommandRequest, LegalHttpFuture, LegalHttpRead, LegalHttpReadReply,
    LegalHttpReadRequest, LegalHttpService, LegalHttpTarget, LegalRegisterReply,
    LegalRenderedContext, legal_router,
};
pub use crate::adapters::postgres_legal_records::{
    LegalHoldNotice, LegalHoldNotifier, LegalIdentity, PgLegalFuture, PgLegalRecords,
    execute_legal_command, legal_records_business_error, read_legal_detail, read_legal_export,
    read_legal_register,
};
pub use crate::adapters::legal_records_use_cases::{
    LegalRecordsError, LegalRecordsFuture, LegalRecordsPorts, legal_command, legal_detail,
    legal_export, legal_register,
};
pub use crate::domain::legal_records::{
    DETAIL_LIMIT, DOCUMENT_BODY_LIMIT, DOCUMENT_LIMIT, DOCUMENT_NAME_LIMIT, HOLD_LIMIT,
    LegalAccessEvent, LegalCommand, LegalCommandEffect, LegalCommandEnvelope, LegalCommandPlan,
    LegalCommandResult, LegalCommandSource, LegalCommentView, LegalDetail, LegalDocumentVersion,
    LegalEnvelopeView, LegalEvent, LegalExport, LegalHoldSource, LegalHoldView, LegalLinkView,
    LegalMember, LegalRecord, LegalRecordsActor, LegalRegister, LegalRegisterQuery, LegalShareView,
    LegalSignerView, LegalStateCount, LegalSubmissionView, REASON_MAX, REASON_MIN,
    REGISTER_MAX_PAGE, REGISTER_PAGE_SIZE, allowed_transitions, authorize_detail,
    authorize_register, derived_id, parse_command, parse_detail_id, parse_envelope, plan_command,
    sha256_hex,
};
