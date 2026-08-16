#![doc = "Commercial bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    ApplyBillingEvent, ApplyEntitlementChange, CommercialApplicationError,
    CommercialCommandHandler, CommercialLifecycleRecordApplicationError,
    CommercialLifecycleRecordCommandHandler, CommercialMutationResult, CommercialQueryHandler,
    CreateCatalogPlan, CreateEntitlement, CreateSubscription, GetCatalogPlan, GetCatalogPlanResult,
    GetEntitlement, GetEntitlementResult, GetSubscription, GetSubscriptionResult, ListCatalogPlans,
    ListEntitlements, ListSubscriptions, PublishCatalogPlan,
};
pub use domain::{
    BillingSignal, CatalogPlan, CatalogPlanId, CatalogPlanStatus, CommercialAggregate,
    CommercialDomainError, CommercialLifecycleRecord, CommercialLifecycleRecordError,
    CommercialLifecycleRecordKind, CommercialLifecycleRecordStatus, Entitlement,
    EntitlementCapacity, EntitlementId, EntitlementState, Subscription, SubscriptionId,
    SubscriptionState, SubscriptionStatus,
};
pub use ports::{
    AuthorizationPort, AuthorizationRequest, BillingEventEvidence, CatalogPlanView,
    CommandFingerprint, CommandIdentity, CommandUnitOfWork, CommercialAction, CommercialCommit,
    CommercialLifecycleRecordAction, CommercialLifecycleRecordAuthorizationPort,
    CommercialLifecycleRecordAuthorizationRequest, CommercialLifecycleRecordCommand,
    CommercialLifecycleRecordCommandIdentity, CommercialLifecycleRecordCommandRequest,
    CommercialLifecycleRecordCommit, CommercialLifecycleRecordReadRepository,
    CommercialLifecycleRecordUnitOfWork, CommercialLifecycleRecordWriteRepository,
    CommercialRecordView, CommercialStoredCommit, EntitlementChangeEvidence, EntitlementView, Page,
    PageFilter, PortError, ProviderVerificationPort, ReadProjection, ReadRepository,
    ScopeAuthorityPort, StoredCommercialLifecycleRecordCommit, SubscriptionView,
    VerifiedBillingEvent, VerifiedEntitlementChange, WriteRepository,
};
