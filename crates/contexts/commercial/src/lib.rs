#![doc = "Commercial bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    ApplyBillingEvent, ApplyEntitlementChange, CommercialApplicationError,
    CommercialCommandHandler, CommercialMutationResult, CommercialQueryHandler, CreateCatalogPlan,
    CreateEntitlement, CreateSubscription, GetCatalogPlan, GetCatalogPlanResult, GetEntitlement,
    GetEntitlementResult, GetSubscription, GetSubscriptionResult, ListCatalogPlans,
    ListEntitlements, ListSubscriptions, PublishCatalogPlan,
};
pub use domain::{
    BillingSignal, CatalogPlan, CatalogPlanId, CatalogPlanStatus, CommercialAggregate,
    CommercialDomainError, Entitlement, EntitlementCapacity, EntitlementId, Subscription,
    SubscriptionId, SubscriptionStatus,
};
pub use ports::{
    AuthorizationPort, AuthorizationRequest, BillingEventEvidence, CatalogPlanView,
    CommandFingerprint, CommandIdentity, CommandUnitOfWork, CommercialAction, CommercialCommit,
    CommercialRecordView, CommercialStoredCommit, EntitlementChangeEvidence, EntitlementView, Page,
    PageFilter, PortError, ProviderVerificationPort, ReadProjection, ReadRepository,
    ScopeAuthorityPort, SubscriptionView, VerifiedBillingEvent, VerifiedEntitlementChange,
    WriteRepository,
};
