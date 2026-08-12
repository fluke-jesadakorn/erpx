#![doc = "Legal bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    AddContractParty, AddDocumentVersion, ArchiveDocument, CloseMatter, CreateContract,
    CreateDocument, CreateMatter, GetContract, GetDocument, GetMatter, LegalApplicationError,
    LegalCommand, LegalCommandHandler, LegalMutationResult, LegalQuery, LegalQueryHandler,
    LegalQueryResponse, LinkMatterRecord, ListAccessHistory, ListContracts, ListDocuments,
    PlaceLegalHold, RecordDocumentAccess, ReleaseLegalHold, TransitionContract,
};
pub use domain::{
    AccessEntry, AccessEntryId, ContractParty, ContractStatus, ContractTransition, DocumentVersion,
    LegalAggregate, LegalContract, LegalContractId, LegalDocument, LegalDocumentId,
    LegalDomainError, LegalHold, LegalMatter, LegalMatterId, MatterStatus, RecordLink,
    RecordRelation,
};
pub use ports::{
    AuthorizationPort, CommandUnitOfWork, EntitlementPort, LegalAction, LegalCommit, MatterView,
    PortError, ReadRepository, StoredCommit, WriteRepository,
};
