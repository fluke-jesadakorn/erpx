#![doc = "ERPX use cases and ports. The application layer owns orchestration, not persistence or transport."]

pub mod approval_governance;
pub mod enterprise_home;
pub mod information_services;
pub mod jurisdiction_policy;
pub mod marketing_privacy;
pub mod marketing_provider_onboarding;
pub mod notifications;
pub mod operational_assurance;
pub mod signature_orchestration;
pub mod support_workflow;

pub use marketing_privacy::{
    AuthorizedMarketingProfilePrivacyPort, EraseMarketingProfileCommand,
    MarketingProfileErasureCorrelation, MarketingProfileErasureReceipt,
    MarketingProfilePrivacyFailure, MarketingProfilePrivacyPort,
    authorize_and_erase_marketing_profile, redacted_marketing_profile_privacy_error,
};
pub use marketing_provider_onboarding::{
    AcknowledgeMarketingProviderVerificationCommand, ConfigureMarketingProviderAccountCommand,
    DEFAULT_MARKETING_PROVIDER_PRIORITY, MARKETING_PROVIDER_CHANNELS,
    MAX_MARKETING_PROVIDER_CHANNELS, MAX_MARKETING_PROVIDER_PRIORITY,
    MAX_MARKETING_PROVIDER_REFERENCE_BYTES, MAX_MARKETING_PROVIDER_ROUTE_BYTES,
    MarketingProviderAccountWriteReceipt, MarketingProviderChannelRoute,
    MarketingProviderOnboardingError, SetMarketingProviderAccountStateCommand,
};
pub use support_workflow::{
    PersistApprovedSupportGrantCommand, RevokeSupportGrantCommand,
    SUPPORT_WORKFLOW_HIGH_RISK_MODES, SupportWorkflowPort, SupportWorkflowReceipt,
    SupportWorkflowService,
};

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
pub use erpx_domain::customer::{
    CustomerAccessAuditSummaryReadDto, CustomerCommercialReadDto, CustomerContactsReadDto,
    CustomerEntitlementsReadDto, CustomerGlobalReadAuditDetailReadDto, CustomerHealthReadDto,
    CustomerNote, CustomerNoteError, CustomerNoteReadDto, CustomerNotesReadDto,
    CustomerOnboardingReadDto, CustomerOrganizationReadDto, CustomerReadFreshness,
    CustomerReadMetadata, CustomerReadView, CustomerSupportSummaryReadDto, GlobalReadActor,
    GlobalReadScope,
};
pub use erpx_domain::support::{
    CustomerVisibleTicket, CustomerVisibleTicketMessage, SupportDomainPolicy,
    SupportIntersectionDenial, SupportIntersectionRequest, SupportMessageAudience,
    SupportTicketAssignment, SupportTicketCommand, SupportTicketEscalation, SupportTicketMessage,
    evaluate_support_intersection,
};
use erpx_domain::{
    access::{
        AssignmentError, RoleAssignment, RoleRecoveryRequest, RoleReviewQueue, Scope,
        build_role_review_queue,
    },
    commercial::{BillingState, EntitlementDecision, ModuleEntitlement, authorize_module},
    contracts::{CommandContext, TenantScope},
    cqrs::OutboxEvent,
    customer::Customer360,
    marketing_content::{
        ContentForm, ContentFormRef, ContentTemplateRef, CustomDomain, CustomDomainBinding,
        MarketingContentError, PreviewGrant,
    },
    support::{SupportError, SupportSession, SupportTicket},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("authorization denied: {0}")]
    Authorization(String),
    #[error(transparent)]
    Assignment(#[from] AssignmentError),
    #[error(transparent)]
    RoleReview(#[from] erpx_domain::access::RoleReviewError),
    #[error(transparent)]
    Support(#[from] SupportError),
    #[error("repository error: {0}")]
    Repository(String),
}

mod legacy;

pub use legacy::{
    access::*, command::*, commercial::*, customer::*, marketing_content::*, support::*,
};
