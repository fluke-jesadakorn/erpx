#![doc = "Customer Authority bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    ChangeCustomerStatus, CreateCustomer, CreateCustomerResult, CustomerAuthorityApplicationError,
    CustomerAuthorityCommandHandler, CustomerAuthorityMutationResult,
    CustomerAuthorityQueryHandler, CustomerAuthorityRecordCommandHandler, GetCustomer,
    GetCustomerResult, LinkCustomerAuthority, ListCustomers, MergeCustomer,
    PortalBindingApplicationError, PortalBindingCommandHandler, UnlinkCustomerAuthority,
    UpdateCustomerIdentity,
};
pub use domain::{
    CustomerAccount, CustomerAuthorityDomainError, CustomerAuthorityRecord,
    CustomerAuthorityRecordKind, CustomerAuthorityRecordStatus, CustomerCode, CustomerId,
    CustomerIdentity, CustomerPortalBindingId, CustomerPortalBindingStatus,
    CustomerPortalPrincipalBinding, CustomerStatus, PortalPrincipalKind, PortalUseCase,
    VerifiedIdentityEvidence,
};
pub use ports::{
    AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
    CommandUnitOfWork, CustomerAuthorityAction, CustomerAuthorityCommit,
    CustomerAuthorityRecordAuthorizationPort, CustomerAuthorityRecordCommand,
    CustomerAuthorityRecordQueryPort, CustomerAuthorityRecordReceipt,
    CustomerAuthorityRecordUnitOfWork, CustomerAuthorityRecordView, CustomerAuthorityView,
    CustomerList, CustomerListFilter, CustomerRecordAuthorityPort, EntitlementPort,
    MergeTargetVersion, PortError, PortalBindingAction, PortalBindingAuthorizationPort,
    PortalBindingCommandIdentity, PortalBindingCommandPayload, PortalBindingCommandRequest,
    PortalBindingCommandUnitOfWork, PortalBindingCommit, PortalBindingProposal,
    PortalBindingWriteRepository, ReadProjection, ReadRepository, StoredCommit,
    StoredPortalBindingCommit, WriteRepository,
};
