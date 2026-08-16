#![doc = "Legal bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    AddContractParty, AddDocumentVersion, ArchiveDocument, CloseMatter, CreateContract,
    CreateDocument, CreateMatter, GetContract, GetDocument, GetMatter, LegalApplicationError,
    LegalCaseCommandHandler, LegalCommand, LegalCommandHandler, LegalMutationResult, LegalQuery,
    LegalQueryHandler, LegalQueryResponse, LinkMatterRecord, ListAccessHistory, ListContracts,
    ListDocuments, PlaceLegalHold, RecordDocumentAccess, ReleaseLegalHold, TransitionContract,
};
pub use domain::{
    AccessEntry, AccessEntryId, CallbackOutcome, ContractParty, ContractStatus, ContractTransition,
    DocumentVersion, HoldEvidence, LegalAggregate, LegalCase, LegalCaseStatus, LegalContract,
    LegalContractId, LegalDocument, LegalDocumentId, LegalDomainError, LegalHold, LegalMatter,
    LegalMatterId, MatterStatus, RecordLink, RecordRelation, RetentionDecision,
    RetentionPolicySnapshot, SignatureCallbackKind, SignatureEnvelope, SignatureEnvelopeId,
    SignatureOrder, SignatureRecipient, SignatureRecipientId, SignatureStatus, SignatureSubject,
    VerifiedSignatureCallback, legal_hold_record_ref_for,
};
pub use ports::{
    AuthorizationPort, CommandUnitOfWork, ESignatureProviderPort, EntitlementPort, LegalAction,
    LegalCaseAuthorizationPort, LegalCaseCommand, LegalCaseQueryPort, LegalCaseReceipt,
    LegalCaseUnitOfWork, LegalCaseView, LegalCommit, MatterView, PortError, ReadRepository,
    SignatureCallbackVerifierPort, SignatureEnvelopeQueryPort, SignatureEnvelopeView,
    SignatureEvidencePackage, SignatureProviderEnvelope, SignatureProviderSubmission, StoredCommit,
    WriteRepository,
};
