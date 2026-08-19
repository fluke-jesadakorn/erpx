//! Application boundary for the human-controlled Quote workflow.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_kernel::AuthorityScope;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    AuthoritativeTaxProfile, ExternalQuoteDecision, QuoteAccountingPlanEvidence,
    QuoteApprovalEvidence, QuoteApprovalRejectionEvidence, QuoteDraftPayload, QuoteId,
    QuotePdfArtifact, QuoteWorkflow, QuoteWorkflowError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteWorkflowPermission {
    Read,
    Draft,
    ApplySuggestion,
    SubmitApproval,
    RecordApproval,
    RecordExternalDecision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuoteWorkflowCommand {
    Create {
        quote_id: QuoteId,
        quote_code: String,
        payload: QuoteDraftPayload,
        authoritative_tax_profiles: Vec<AuthoritativeTaxProfile>,
    },
    UpdateDraft {
        quote_id: QuoteId,
        expected_quote_version: u64,
        payload: QuoteDraftPayload,
        authoritative_tax_profiles: Vec<AuthoritativeTaxProfile>,
    },
    ApplySuggestion {
        quote_id: QuoteId,
        expected_quote_version: u64,
        payload: QuoteDraftPayload,
        authoritative_tax_profiles: Vec<AuthoritativeTaxProfile>,
        suggestion_id: Uuid,
        payload_digest: String,
        output_digest: String,
        provider_label: String,
    },
    SubmitApproval {
        quote_id: QuoteId,
        expected_quote_version: u64,
    },
    AttachAccountingPlan {
        quote_id: QuoteId,
        evidence: QuoteAccountingPlanEvidence,
    },
    RecordApproval {
        quote_id: QuoteId,
        evidence: QuoteApprovalEvidence,
    },
    RejectApproval {
        quote_id: QuoteId,
        evidence: QuoteApprovalRejectionEvidence,
    },
    AttachPdf {
        quote_id: QuoteId,
        artifact: QuotePdfArtifact,
    },
    RecordExternalDecision {
        quote_id: QuoteId,
        decision: ExternalQuoteDecision,
    },
}

impl QuoteWorkflowCommand {
    pub const fn quote_id(&self) -> QuoteId {
        match self {
            Self::Create { quote_id, .. }
            | Self::UpdateDraft { quote_id, .. }
            | Self::ApplySuggestion { quote_id, .. }
            | Self::SubmitApproval { quote_id, .. }
            | Self::AttachAccountingPlan { quote_id, .. }
            | Self::RecordApproval { quote_id, .. }
            | Self::RejectApproval { quote_id, .. }
            | Self::AttachPdf { quote_id, .. }
            | Self::RecordExternalDecision { quote_id, .. } => *quote_id,
        }
    }

    pub const fn permission(&self) -> QuoteWorkflowPermission {
        match self {
            Self::Create { .. } | Self::UpdateDraft { .. } => QuoteWorkflowPermission::Draft,
            Self::ApplySuggestion { .. } => QuoteWorkflowPermission::ApplySuggestion,
            Self::SubmitApproval { .. } => QuoteWorkflowPermission::SubmitApproval,
            Self::AttachAccountingPlan { .. }
            | Self::RecordApproval { .. }
            | Self::RejectApproval { .. }
            | Self::AttachPdf { .. } => QuoteWorkflowPermission::RecordApproval,
            Self::RecordExternalDecision { .. } => QuoteWorkflowPermission::RecordExternalDecision,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteWorkflowCommandRequest {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub payload_digest: String,
    pub scope: AuthorityScope,
    pub actor_ref: String,
    pub expected_aggregate_version: u64,
    pub command: QuoteWorkflowCommand,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteWorkflowCommit {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub payload_digest: String,
    pub expected_aggregate_version: u64,
    pub workflow: QuoteWorkflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteWorkflowReceipt {
    pub workflow: QuoteWorkflow,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteWorkflowPage {
    pub rows: Vec<QuoteWorkflow>,
    pub next_cursor: Option<String>,
    pub projection_lag_millis: Option<u64>,
}

#[async_trait]
pub trait QuoteWorkflowAuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        scope: &AuthorityScope,
        actor_ref: &str,
        permission: QuoteWorkflowPermission,
    ) -> Result<bool, QuoteWorkflowApplicationError>;
}

#[async_trait]
pub trait QuoteWorkflowRepository: Send + Sync {
    async fn load(
        &self,
        scope: &AuthorityScope,
        quote_id: QuoteId,
    ) -> Result<Option<QuoteWorkflow>, QuoteWorkflowApplicationError>;

    async fn commit(
        &self,
        scope: &AuthorityScope,
        commit: QuoteWorkflowCommit,
    ) -> Result<QuoteWorkflowReceipt, QuoteWorkflowApplicationError>;

    async fn list(
        &self,
        scope: &AuthorityScope,
        cursor: Option<&str>,
        page_size: u16,
    ) -> Result<QuoteWorkflowPage, QuoteWorkflowApplicationError>;
}

#[derive(Debug)]
pub struct QuoteWorkflowService<R, A> {
    repository: R,
    authorization: A,
}

impl<R, A> QuoteWorkflowService<R, A>
where
    R: QuoteWorkflowRepository,
    A: QuoteWorkflowAuthorizationPort,
{
    pub fn new(repository: R, authorization: A) -> Self {
        Self {
            repository,
            authorization,
        }
    }

    pub async fn execute(
        &self,
        request: QuoteWorkflowCommandRequest,
    ) -> Result<QuoteWorkflowReceipt, QuoteWorkflowApplicationError> {
        validate_request(&request)?;
        if !self
            .authorization
            .authorize(
                &request.scope,
                &request.actor_ref,
                request.command.permission(),
            )
            .await?
        {
            return Err(QuoteWorkflowApplicationError::Denied);
        }
        let mut workflow = match &request.command {
            QuoteWorkflowCommand::Create {
                quote_id,
                quote_code,
                payload,
                authoritative_tax_profiles,
            } => {
                if request.expected_aggregate_version != 0 {
                    return Err(QuoteWorkflowApplicationError::VersionConflict);
                }
                QuoteWorkflow::create(
                    *quote_id,
                    request.scope.clone(),
                    quote_code.clone(),
                    request.actor_ref.clone(),
                    payload.clone(),
                    authoritative_tax_profiles,
                    request.at,
                )?
            }
            _ => {
                let value = self
                    .repository
                    .load(&request.scope, request.command.quote_id())
                    .await?
                    .ok_or(QuoteWorkflowApplicationError::NotFound)?;
                if value.aggregate_version() != request.expected_aggregate_version {
                    return Err(QuoteWorkflowApplicationError::VersionConflict);
                }
                value
            }
        };
        match request.command {
            QuoteWorkflowCommand::Create { .. } => {}
            QuoteWorkflowCommand::UpdateDraft {
                expected_quote_version,
                payload,
                authoritative_tax_profiles,
                ..
            } => workflow.update_draft(
                expected_quote_version,
                payload,
                &authoritative_tax_profiles,
                request.at,
            )?,
            QuoteWorkflowCommand::ApplySuggestion {
                expected_quote_version,
                payload,
                authoritative_tax_profiles,
                suggestion_id,
                payload_digest,
                output_digest,
                provider_label,
                ..
            } => workflow.apply_suggestion(
                expected_quote_version,
                payload,
                &authoritative_tax_profiles,
                suggestion_id,
                payload_digest,
                output_digest,
                provider_label,
                request.actor_ref,
                request.at,
            )?,
            QuoteWorkflowCommand::SubmitApproval {
                expected_quote_version,
                ..
            } => workflow.submit_for_approval(expected_quote_version, request.at)?,
            QuoteWorkflowCommand::AttachAccountingPlan { evidence, .. } => {
                workflow.attach_accounting_plan(evidence, request.at)?
            }
            QuoteWorkflowCommand::RecordApproval { evidence, .. } => {
                workflow.approve(evidence, request.at)?
            }
            QuoteWorkflowCommand::RejectApproval { evidence, .. } => {
                workflow.reject_approval(evidence, request.at)?
            }
            QuoteWorkflowCommand::AttachPdf { artifact, .. } => {
                workflow.attach_pdf(artifact, request.at)?
            }
            QuoteWorkflowCommand::RecordExternalDecision { decision, .. } => {
                workflow.record_external_decision(decision, request.at)?
            }
        }
        self.repository
            .commit(
                &request.scope,
                QuoteWorkflowCommit {
                    command_id: request.command_id,
                    idempotency_key: request.idempotency_key,
                    payload_digest: request.payload_digest,
                    expected_aggregate_version: request.expected_aggregate_version,
                    workflow,
                },
            )
            .await
    }

    pub async fn get(
        &self,
        scope: &AuthorityScope,
        actor_ref: &str,
        quote_id: QuoteId,
    ) -> Result<Option<QuoteWorkflow>, QuoteWorkflowApplicationError> {
        if !self
            .authorization
            .authorize(scope, actor_ref, QuoteWorkflowPermission::Read)
            .await?
        {
            return Err(QuoteWorkflowApplicationError::Denied);
        }
        self.repository.load(scope, quote_id).await
    }

    pub async fn list(
        &self,
        scope: &AuthorityScope,
        actor_ref: &str,
        cursor: Option<&str>,
        page_size: u16,
    ) -> Result<QuoteWorkflowPage, QuoteWorkflowApplicationError> {
        if page_size == 0 || page_size > 100 {
            return Err(QuoteWorkflowApplicationError::InvalidRequest);
        }
        if !self
            .authorization
            .authorize(scope, actor_ref, QuoteWorkflowPermission::Read)
            .await?
        {
            return Err(QuoteWorkflowApplicationError::Denied);
        }
        self.repository.list(scope, cursor, page_size).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QuoteWorkflowApplicationError {
    #[error("quote workflow request is invalid")]
    InvalidRequest,
    #[error("quote workflow access denied")]
    Denied,
    #[error("quote workflow not found")]
    NotFound,
    #[error("quote workflow version conflict")]
    VersionConflict,
    #[error("quote workflow storage is unavailable")]
    Unavailable,
    #[error(transparent)]
    Domain(#[from] QuoteWorkflowError),
}

fn validate_request(
    request: &QuoteWorkflowCommandRequest,
) -> Result<(), QuoteWorkflowApplicationError> {
    request
        .scope
        .validate()
        .map_err(|_| QuoteWorkflowApplicationError::InvalidRequest)?;
    if request.command_id.is_nil()
        || request.idempotency_key.trim().is_empty()
        || request.idempotency_key.len() > 256
        || request.payload_digest.len() != 64
        || !request
            .payload_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || request.actor_ref.trim().is_empty()
        || request.actor_ref.len() > 256
    {
        return Err(QuoteWorkflowApplicationError::InvalidRequest);
    }
    Ok(())
}
