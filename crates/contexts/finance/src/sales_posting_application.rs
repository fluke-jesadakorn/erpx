use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_integration_contracts::{DimensionSet, RecordRef};
use erpx_kernel::TenantScope;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    AccountId, AccountingObligation, AccountingPlan, AccountingPlanId,
    AccountingPlanSuggestionProvenance, FinanceTaxProfileSnapshot, PostingActuals,
    PostingInstructions, PostingPacket, PostingPacketId, PostingReceipt, PostingValidationContext,
    SalesAccountRoles, SalesPostingError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostingValidationSnapshot {
    pub period_open: bool,
    pub active_accounts: BTreeSet<AccountId>,
    pub current_tax_profile: FinanceTaxProfileSnapshot,
}

impl PostingValidationSnapshot {
    fn as_context(&self) -> PostingValidationContext<'_> {
        PostingValidationContext {
            period_open: self.period_open,
            active_accounts: &self.active_accounts,
            current_tax_profile: &self.current_tax_profile,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SalesPostingCommand {
    CreatePlan {
        plan_id: AccountingPlanId,
        quote: RecordRef,
        currency: String,
        quote_net_minor: i64,
        quote_tax_minor: i64,
        dimensions: Box<DimensionSet>,
        tax_profile: FinanceTaxProfileSnapshot,
        account_roles: SalesAccountRoles,
        obligations: Vec<AccountingObligation>,
        sales_approver: String,
        suggestion: Option<AccountingPlanSuggestionProvenance>,
    },
    SubmitPlan {
        plan_id: AccountingPlanId,
    },
    ApprovePlan {
        plan_id: AccountingPlanId,
    },
    RejectPlan {
        plan_id: AccountingPlanId,
    },
    CreatePacket {
        packet_id: PostingPacketId,
        plan_id: AccountingPlanId,
        actuals: PostingActuals,
    },
    SubmitPacket {
        packet_id: PostingPacketId,
        validation: PostingValidationSnapshot,
    },
    FinalApprovePacket {
        packet_id: PostingPacketId,
        validation: PostingValidationSnapshot,
    },
    RejectPacket {
        packet_id: PostingPacketId,
    },
    MarkPosted {
        packet_id: PostingPacketId,
        receipt: PostingReceipt,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesPostingCommandRequest {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub scope: TenantScope,
    pub actor_ref: String,
    pub expected_version: u64,
    pub command: SalesPostingCommand,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SalesPostingAggregate {
    Plan(AccountingPlan),
    Packet(PostingPacket),
}

impl SalesPostingAggregate {
    pub const fn version(&self) -> u64 {
        match self {
            Self::Plan(value) => value.version(),
            Self::Packet(value) => value.version(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesPostingCommit {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub expected_version: u64,
    pub aggregate: SalesPostingAggregate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesPostingCommandReceipt {
    pub aggregate: SalesPostingAggregate,
    pub posting_instructions: Option<PostingInstructions>,
    pub replayed: bool,
}

#[async_trait]
pub trait SalesPostingAuthorizationPort: Send + Sync {
    async fn authorize(
        &self,
        scope: &TenantScope,
        actor_ref: &str,
        command: &SalesPostingCommand,
    ) -> Result<bool, SalesPostingApplicationError>;
}

#[async_trait]
pub trait SalesPostingRepository: Send + Sync {
    async fn load_plan(
        &self,
        scope: &TenantScope,
        id: AccountingPlanId,
    ) -> Result<Option<AccountingPlan>, SalesPostingApplicationError>;

    async fn load_packet(
        &self,
        scope: &TenantScope,
        id: PostingPacketId,
    ) -> Result<Option<PostingPacket>, SalesPostingApplicationError>;

    async fn commit(
        &self,
        scope: &TenantScope,
        commit: SalesPostingCommit,
    ) -> Result<SalesPostingCommandReceipt, SalesPostingApplicationError>;
}

#[derive(Debug)]
pub struct SalesPostingService<R, A> {
    repository: R,
    authorization: A,
}

impl<R, A> SalesPostingService<R, A>
where
    R: SalesPostingRepository,
    A: SalesPostingAuthorizationPort,
{
    pub fn new(repository: R, authorization: A) -> Self {
        Self {
            repository,
            authorization,
        }
    }

    pub async fn execute(
        &self,
        request: SalesPostingCommandRequest,
    ) -> Result<SalesPostingCommandReceipt, SalesPostingApplicationError> {
        validate_request(&request)?;
        if !self
            .authorization
            .authorize(&request.scope, &request.actor_ref, &request.command)
            .await?
        {
            return Err(SalesPostingApplicationError::Denied);
        }
        let mut instructions = None;
        let aggregate = match request.command {
            SalesPostingCommand::CreatePlan {
                plan_id,
                quote,
                currency,
                quote_net_minor,
                quote_tax_minor,
                dimensions,
                tax_profile,
                account_roles,
                obligations,
                sales_approver,
                suggestion,
            } => {
                if request.expected_version != 0 {
                    return Err(SalesPostingApplicationError::VersionConflict);
                }
                SalesPostingAggregate::Plan(AccountingPlan::draft(
                    plan_id,
                    request.scope.clone(),
                    quote,
                    currency,
                    quote_net_minor,
                    quote_tax_minor,
                    *dimensions,
                    tax_profile,
                    account_roles,
                    obligations,
                    request.actor_ref.clone(),
                    sales_approver,
                    suggestion,
                    request.at,
                )?)
            }
            SalesPostingCommand::SubmitPlan { plan_id }
            | SalesPostingCommand::ApprovePlan { plan_id }
            | SalesPostingCommand::RejectPlan { plan_id } => {
                let mut plan = self
                    .load_plan(&request.scope, request.expected_version, plan_id)
                    .await?;
                match request.command {
                    SalesPostingCommand::SubmitPlan { .. } => plan.submit(request.at)?,
                    SalesPostingCommand::ApprovePlan { .. } => {
                        plan.approve(request.actor_ref.clone(), request.at)?
                    }
                    SalesPostingCommand::RejectPlan { .. } => plan.reject(request.at)?,
                    _ => unreachable!(),
                }
                SalesPostingAggregate::Plan(plan)
            }
            SalesPostingCommand::CreatePacket {
                packet_id,
                plan_id,
                actuals,
            } => {
                if request.expected_version != 0 {
                    return Err(SalesPostingApplicationError::VersionConflict);
                }
                let plan = self
                    .repository
                    .load_plan(&request.scope, plan_id)
                    .await?
                    .ok_or(SalesPostingApplicationError::NotFound)?;
                SalesPostingAggregate::Packet(PostingPacket::draft_from_plan(
                    packet_id,
                    &plan,
                    actuals,
                    request.actor_ref.clone(),
                    request.at,
                )?)
            }
            SalesPostingCommand::SubmitPacket {
                packet_id,
                validation,
            } => {
                let mut packet = self
                    .load_packet(&request.scope, request.expected_version, packet_id)
                    .await?;
                packet.submit(&validation.as_context(), request.at)?;
                SalesPostingAggregate::Packet(packet)
            }
            SalesPostingCommand::FinalApprovePacket {
                packet_id,
                validation,
            } => {
                let mut packet = self
                    .load_packet(&request.scope, request.expected_version, packet_id)
                    .await?;
                packet.approve(
                    request.actor_ref.clone(),
                    &validation.as_context(),
                    request.at,
                )?;
                instructions = Some(packet.posting_instructions()?);
                SalesPostingAggregate::Packet(packet)
            }
            SalesPostingCommand::RejectPacket { packet_id } => {
                let mut packet = self
                    .load_packet(&request.scope, request.expected_version, packet_id)
                    .await?;
                packet.reject(request.at)?;
                SalesPostingAggregate::Packet(packet)
            }
            SalesPostingCommand::MarkPosted { packet_id, receipt } => {
                let mut packet = self
                    .load_packet(&request.scope, request.expected_version, packet_id)
                    .await?;
                packet.mark_posted(receipt, request.at)?;
                SalesPostingAggregate::Packet(packet)
            }
        };
        let mut receipt = self
            .repository
            .commit(
                &request.scope,
                SalesPostingCommit {
                    command_id: request.command_id,
                    idempotency_key: request.idempotency_key,
                    expected_version: request.expected_version,
                    aggregate,
                },
            )
            .await?;
        if receipt.posting_instructions.is_none() {
            receipt.posting_instructions = instructions;
        }
        Ok(receipt)
    }

    async fn load_plan(
        &self,
        scope: &TenantScope,
        expected_version: u64,
        id: AccountingPlanId,
    ) -> Result<AccountingPlan, SalesPostingApplicationError> {
        let value = self
            .repository
            .load_plan(scope, id)
            .await?
            .ok_or(SalesPostingApplicationError::NotFound)?;
        if value.version() != expected_version {
            return Err(SalesPostingApplicationError::VersionConflict);
        }
        Ok(value)
    }

    async fn load_packet(
        &self,
        scope: &TenantScope,
        expected_version: u64,
        id: PostingPacketId,
    ) -> Result<PostingPacket, SalesPostingApplicationError> {
        let value = self
            .repository
            .load_packet(scope, id)
            .await?
            .ok_or(SalesPostingApplicationError::NotFound)?;
        if value.version() != expected_version {
            return Err(SalesPostingApplicationError::VersionConflict);
        }
        Ok(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SalesPostingApplicationError {
    #[error("Sales posting request is invalid")]
    InvalidRequest,
    #[error("Sales posting access denied")]
    Denied,
    #[error("Sales posting record not found")]
    NotFound,
    #[error("Sales posting version conflict")]
    VersionConflict,
    #[error("Sales posting storage is unavailable")]
    Unavailable,
    #[error(transparent)]
    Domain(#[from] SalesPostingError),
}

fn validate_request(
    request: &SalesPostingCommandRequest,
) -> Result<(), SalesPostingApplicationError> {
    request
        .scope
        .validate()
        .map_err(|_| SalesPostingApplicationError::InvalidRequest)?;
    if request.command_id.is_nil()
        || request.idempotency_key.trim().is_empty()
        || request.idempotency_key.len() > 256
        || request.actor_ref.trim().is_empty()
        || request.actor_ref.len() > 256
    {
        return Err(SalesPostingApplicationError::InvalidRequest);
    }
    Ok(())
}
