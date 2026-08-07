#![doc = "Legal bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    AddContractParty, AddDocumentVersion, ArchiveDocument, CreateContract, CreateDocument,
    GetContract, GetDocument, LegalApplicationError, LegalCommand, LegalCommandHandler,
    LegalMutationResult, LegalQuery, LegalQueryHandler, LegalQueryResponse, ListAccessHistory,
    ListContracts, ListDocuments, RecordDocumentAccess, TransitionContract,
};
pub use domain::{
    AccessEntry, AccessEntryId, ContractParty, ContractStatus, ContractTransition, DocumentVersion,
    LegalAggregate, LegalContract, LegalContractId, LegalDocument, LegalDocumentId,
    LegalDomainError,
};
pub use ports::{
    AuthorizationPort, CommandUnitOfWork, EntitlementPort, LegalAction, LegalCommit, PortError,
    ReadRepository, StoredCommit, WriteRepository,
};
