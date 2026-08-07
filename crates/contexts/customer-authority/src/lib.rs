#![doc = "Customer Authority bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    ChangeCustomerStatus, CreateCustomer, CreateCustomerResult, CustomerAuthorityApplicationError,
    CustomerAuthorityCommandHandler, CustomerAuthorityMutationResult,
    CustomerAuthorityQueryHandler, GetCustomer, GetCustomerResult, LinkCustomerAuthority,
    ListCustomers, MergeCustomer, UnlinkCustomerAuthority, UpdateCustomerIdentity,
};
pub use domain::{
    CustomerAccount, CustomerAuthorityDomainError, CustomerCode, CustomerId, CustomerIdentity,
    CustomerStatus,
};
pub use ports::{
    AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
    CommandUnitOfWork, CustomerAuthorityAction, CustomerAuthorityCommit, CustomerAuthorityView,
    CustomerList, CustomerListFilter, EntitlementPort, MergeTargetVersion, PortError,
    ReadProjection, ReadRepository, StoredCommit, WriteRepository,
};
