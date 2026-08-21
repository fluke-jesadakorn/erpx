use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use erpx_integration_contracts::{
    BoundedContext, DimensionSet, MoneyAmount, RecordRef, marketing, operations,
};
use erpx_kernel::{AuthorityScope, CommandId, HumanDecisionProof};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_MARKETING_TEXT_BYTES: usize = 255;

macro_rules! marketing_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, MarketingDomainError> {
                if value.is_nil() {
                    return Err(MarketingDomainError::NilIdentifier { field: $field });
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = MarketingDomainError;
            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                Self::try_from_uuid(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

marketing_id!(LeadCandidateId, "lead_candidate_id");
marketing_id!(CampaignId, "campaign_id");
marketing_id!(AudienceMemberId, "audience_member_id");
marketing_id!(CampaignBudgetId, "campaign_budget_id");
marketing_id!(BudgetAllocationId, "budget_allocation_id");
marketing_id!(SpendRequestId, "spend_request_id");

pub const MAX_BUDGET_ALLOCATIONS: usize = 256;
pub const MAX_SPEND_RESERVATIONS: usize = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeadCandidateStatus {
    Captured,
    Qualified,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignBudgetStatus {
    Draft,
    Review,
    Approved,
    Running,
    Completed,
    Cancelled,
}

impl CampaignBudgetStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Review => "review",
            Self::Approved => "approved",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpendReservationStatus {
    Active,
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MarketingDomainError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds {maximum} bytes")]
    TooLong { field: &'static str, maximum: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("Marketing records require an exact company or branch scope")]
    InvalidScope,
    #[error("qualification score must be between 1 and 10,000 basis points")]
    InvalidQualificationScore,
    #[error("lead candidate lifecycle transition is not allowed")]
    InvalidTransition,
    #[error("campaign budget lifecycle transition is not allowed")]
    InvalidBudgetTransition,
    #[error("Marketing record lifecycle transition is not allowed")]
    InvalidRecordTransition,
    #[error("Marketing record version conflict (expected {expected}, actual {actual})")]
    RecordVersionConflict { expected: u64, actual: u64 },
    #[error(
        "campaign budget dimensions must bind the exact legal entity, branch, campaign, business unit, and cost center"
    )]
    InvalidBudgetDimensions,
    #[error(
        "campaign budget allocations must be unique, positive, currency-consistent, and equal the budget amount"
    )]
    InvalidBudgetAllocations,
    #[error("campaign budget approval evidence is invalid or violates maker-checker")]
    InvalidBudgetApproval,
    #[error("campaign spend reservation is invalid, duplicated, or exceeds the allocation")]
    InvalidSpendReservation,
    #[error("Marketing amount arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("Marketing timestamp cannot move backwards")]
    InvalidTimestamp,
    #[error("Marketing aggregate version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeadCandidate {
    id: LeadCandidateId,
    version: u64,
    scope: AuthorityScope,
    campaign_id: CampaignId,
    audience_member_id: AudienceMemberId,
    source: String,
    existing_customer_id: Option<Uuid>,
    qualification_score_basis_points: Option<u16>,
    status: LeadCandidateStatus,
    engagement_count: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

/// Typed relational persistence state for a lead candidate. This is an
/// invariant-checked boundary, not a serialized aggregate snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeadCandidateState {
    pub id: LeadCandidateId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub campaign_id: CampaignId,
    pub audience_member_id: AudienceMemberId,
    pub source: String,
    pub existing_customer_id: Option<Uuid>,
    pub qualification_score_basis_points: Option<u16>,
    pub status: LeadCandidateStatus,
    pub engagement_count: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LeadCandidate {
    pub fn try_rehydrate(state: LeadCandidateState) -> Result<Self, MarketingDomainError> {
        let value = Self {
            id: state.id,
            version: state.version,
            scope: state.scope,
            campaign_id: state.campaign_id,
            audience_member_id: state.audience_member_id,
            source: state.source,
            existing_customer_id: state.existing_customer_id,
            qualification_score_basis_points: state.qualification_score_basis_points,
            status: state.status,
            engagement_count: state.engagement_count,
            created_at: state.created_at,
            updated_at: state.updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: LeadCandidateId,
        scope: AuthorityScope,
        campaign_id: CampaignId,
        audience_member_id: AudienceMemberId,
        source: String,
        existing_customer_id: Option<Uuid>,
        at: DateTime<Utc>,
    ) -> Result<Self, MarketingDomainError> {
        exact_scope(&scope)?;
        validate_text(&source, "source")?;
        if existing_customer_id.is_some_and(|id| id.is_nil()) {
            return Err(MarketingDomainError::NilIdentifier {
                field: "existing_customer_id",
            });
        }
        let value = Self {
            id,
            version: 1,
            scope,
            campaign_id,
            audience_member_id,
            source,
            existing_customer_id,
            qualification_score_basis_points: None,
            status: LeadCandidateStatus::Captured,
            engagement_count: 0,
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn capture_engagement(
        &mut self,
        channel: String,
        action: String,
        at: DateTime<Utc>,
    ) -> Result<(), MarketingDomainError> {
        if self.status != LeadCandidateStatus::Captured {
            return Err(MarketingDomainError::InvalidTransition);
        }
        validate_text(&channel, "channel")?;
        validate_text(&action, "action")?;
        self.advance(at)?;
        self.engagement_count = self
            .engagement_count
            .checked_add(1)
            .ok_or(MarketingDomainError::VersionOverflow)?;
        self.uncommitted_events
            .push(DomainEvent::EngagementCaptured {
                campaign_id: self.campaign_id,
                audience_member_id: self.audience_member_id,
                channel,
                action,
            });
        Ok(())
    }

    pub fn qualify(
        &mut self,
        score_basis_points: u16,
        at: DateTime<Utc>,
    ) -> Result<(), MarketingDomainError> {
        if self.status != LeadCandidateStatus::Captured {
            return Err(MarketingDomainError::InvalidTransition);
        }
        if score_basis_points == 0 || score_basis_points > 10_000 {
            return Err(MarketingDomainError::InvalidQualificationScore);
        }
        self.advance(at)?;
        self.qualification_score_basis_points = Some(score_basis_points);
        self.status = LeadCandidateStatus::Qualified;
        self.uncommitted_events.push(DomainEvent::LeadQualified {
            candidate_id: self.id,
            campaign_id: self.campaign_id,
            audience_member_id: self.audience_member_id,
            existing_customer_id: self.existing_customer_id,
            source: self.source.clone(),
            qualification_score_basis_points: score_basis_points,
        });
        Ok(())
    }

    pub fn reject(
        &mut self,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<(), MarketingDomainError> {
        if decision.authority_scope() != &self.scope
            || decision.decision_ref()
                != lead_rejection_decision_ref(command_id, self.id, expected_version)
            || decision.decided_at() != at
        {
            return Err(MarketingDomainError::InvalidTransition);
        }
        if self.status != LeadCandidateStatus::Captured {
            return Err(MarketingDomainError::InvalidTransition);
        }
        self.advance(at)?;
        self.status = LeadCandidateStatus::Rejected;
        Ok(())
    }

    #[cfg(test)]
    fn reject_for_test(&mut self, at: DateTime<Utc>) -> Result<(), MarketingDomainError> {
        use erpx_kernel::{ActorAttribution, ActorKind, SessionId};

        let command_id = CommandId::try_from_uuid(Uuid::new_v4()).expect("test command id");
        let decision = HumanDecisionProof::try_new(
            ActorAttribution {
                principal_ref: "marketing-test-user".into(),
                kind: ActorKind::HoldingUser,
                session_id: Some(
                    SessionId::try_from_uuid(Uuid::new_v4()).expect("test session id"),
                ),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            self.scope.clone(),
            lead_rejection_decision_ref(command_id, self.id, self.version),
            at,
        )
        .expect("test human decision");
        self.reject(&decision, command_id, self.version, at)
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), MarketingDomainError> {
        if at < self.updated_at {
            return Err(MarketingDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(MarketingDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), MarketingDomainError> {
        exact_scope(&self.scope)?;
        validate_text(&self.source, "source")?;
        if self.version == 0 {
            return Err(MarketingDomainError::VersionOverflow);
        }
        if self.updated_at < self.created_at {
            return Err(MarketingDomainError::InvalidTimestamp);
        }
        if self
            .qualification_score_basis_points
            .is_some_and(|score| score == 0 || score > 10_000)
        {
            return Err(MarketingDomainError::InvalidQualificationScore);
        }
        if (self.status == LeadCandidateStatus::Qualified)
            != self.qualification_score_basis_points.is_some()
        {
            return Err(MarketingDomainError::InvalidTransition);
        }
        Ok(())
    }

    pub const fn id(&self) -> LeadCandidateId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn campaign_id(&self) -> CampaignId {
        self.campaign_id
    }
    pub const fn audience_member_id(&self) -> AudienceMemberId {
        self.audience_member_id
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub const fn existing_customer_id(&self) -> Option<Uuid> {
        self.existing_customer_id
    }
    pub const fn score(&self) -> Option<u16> {
        self.qualification_score_basis_points
    }
    pub const fn status(&self) -> LeadCandidateStatus {
        self.status
    }
    pub const fn engagement_count(&self) -> u64 {
        self.engagement_count
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

pub fn lead_rejection_decision_ref(
    command_id: CommandId,
    lead_id: LeadCandidateId,
    expected_version: u64,
) -> String {
    format!(
        "marketing:reject_lead_candidate:{command_id}:{}:{expected_version}",
        lead_id.as_uuid()
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    LeadQualified {
        candidate_id: LeadCandidateId,
        campaign_id: CampaignId,
        audience_member_id: AudienceMemberId,
        existing_customer_id: Option<Uuid>,
        source: String,
        qualification_score_basis_points: u16,
    },
    EngagementCaptured {
        campaign_id: CampaignId,
        audience_member_id: AudienceMemberId,
        channel: String,
        action: String,
    },
    CampaignBudgetChanged,
    CampaignBudgetApprovalRequested {
        budget_id: CampaignBudgetId,
        campaign_id: CampaignId,
        approval: operations::ApprovalRequestFact,
    },
    CampaignBudgetApproved {
        budget_id: CampaignBudgetId,
        campaign_id: CampaignId,
        period: marketing::CampaignBudgetPeriod,
        approved_amount: MoneyAmount,
        dimensions: DimensionSet,
        approved_at: DateTime<Utc>,
    },
    SpendRequested {
        budget_id: CampaignBudgetId,
        request_id: SpendRequestId,
        campaign_id: CampaignId,
        allocation_id: BudgetAllocationId,
        tactic: marketing::TacticRef,
        channel: marketing::ChannelRef,
        amount: MoneyAmount,
        dimensions: DimensionSet,
        expected_outcome: String,
        requested_at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetAllocation {
    pub allocation_id: BudgetAllocationId,
    pub tactic: marketing::TacticRef,
    pub channel: marketing::ChannelRef,
    pub planned_amount: MoneyAmount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpendReservation {
    pub request_id: SpendRequestId,
    pub allocation_id: BudgetAllocationId,
    pub amount: MoneyAmount,
    pub expected_outcome: String,
    pub status: SpendReservationStatus,
    pub requested_at: DateTime<Utc>,
    pub released_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignBudget {
    id: CampaignBudgetId,
    version: u64,
    scope: AuthorityScope,
    campaign_id: CampaignId,
    code: String,
    name: String,
    period: marketing::CampaignBudgetPeriod,
    requested_amount: MoneyAmount,
    approved_amount: Option<MoneyAmount>,
    dimensions: DimensionSet,
    allocations: Vec<BudgetAllocation>,
    reservations: BTreeMap<SpendRequestId, SpendReservation>,
    approval_request: Option<operations::ApprovalRequestRef>,
    approval_subject: Option<RecordRef>,
    approval_requested_by: Option<erpx_integration_contracts::ReferenceCode>,
    approval_requested_at: Option<DateTime<Utc>>,
    status: CampaignBudgetStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

/// Typed relational persistence state for a campaign budget and its owned
/// allocation/reservation rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CampaignBudgetState {
    pub id: CampaignBudgetId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub campaign_id: CampaignId,
    pub code: String,
    pub name: String,
    pub period: marketing::CampaignBudgetPeriod,
    pub requested_amount: MoneyAmount,
    pub approved_amount: Option<MoneyAmount>,
    pub dimensions: DimensionSet,
    pub allocations: Vec<BudgetAllocation>,
    pub reservations: BTreeMap<SpendRequestId, SpendReservation>,
    pub approval_request: Option<operations::ApprovalRequestRef>,
    pub approval_subject: Option<RecordRef>,
    pub approval_requested_by: Option<erpx_integration_contracts::ReferenceCode>,
    pub approval_requested_at: Option<DateTime<Utc>>,
    pub status: CampaignBudgetStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CampaignBudget {
    pub fn try_rehydrate(state: CampaignBudgetState) -> Result<Self, MarketingDomainError> {
        let value = Self {
            id: state.id,
            version: state.version,
            scope: state.scope,
            campaign_id: state.campaign_id,
            code: state.code,
            name: state.name,
            period: state.period,
            requested_amount: state.requested_amount,
            approved_amount: state.approved_amount,
            dimensions: state.dimensions,
            allocations: state.allocations,
            reservations: state.reservations,
            approval_request: state.approval_request,
            approval_subject: state.approval_subject,
            approval_requested_by: state.approval_requested_by,
            approval_requested_at: state.approval_requested_at,
            status: state.status,
            created_at: state.created_at,
            updated_at: state.updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: CampaignBudgetId,
        scope: AuthorityScope,
        campaign_id: CampaignId,
        code: String,
        name: String,
        period: marketing::CampaignBudgetPeriod,
        requested_amount: MoneyAmount,
        dimensions: DimensionSet,
        allocations: Vec<BudgetAllocation>,
        at: DateTime<Utc>,
    ) -> Result<Self, MarketingDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            campaign_id,
            code,
            name,
            period,
            requested_amount,
            approved_amount: None,
            dimensions,
            allocations,
            reservations: BTreeMap::new(),
            approval_request: None,
            approval_subject: None,
            approval_requested_by: None,
            approval_requested_at: None,
            status: CampaignBudgetStatus::Draft,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::CampaignBudgetChanged],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn submit_for_review(
        &mut self,
        approval: operations::ApprovalRequestFact,
        at: DateTime<Utc>,
    ) -> Result<(), MarketingDomainError> {
        if self.status != CampaignBudgetStatus::Draft
            || approval.subject.context != BoundedContext::Marketing
            || approval.subject.aggregate_kind.as_str() != "campaign_budget"
            || approval.subject.aggregate_id != self.id.as_uuid()
            || approval.subject.aggregate_version != self.version
            || approval.amount != Some(self.requested_amount)
            || approval.requested_at < self.updated_at
            || approval.requested_at > at
        {
            return Err(MarketingDomainError::InvalidBudgetApproval);
        }
        self.bump(at)?;
        self.approval_request = Some(approval.request);
        self.approval_subject = Some(approval.subject.clone());
        self.approval_requested_by = Some(approval.requested_by.clone());
        self.approval_requested_at = Some(approval.requested_at);
        self.status = CampaignBudgetStatus::Review;
        self.uncommitted_events
            .push(DomainEvent::CampaignBudgetApprovalRequested {
                budget_id: self.id,
                campaign_id: self.campaign_id,
                approval,
            });
        Ok(())
    }

    pub fn apply_approval_decision(
        &mut self,
        decision: operations::ApprovalDecisionFact,
        at: DateTime<Utc>,
    ) -> Result<(), MarketingDomainError> {
        let requested_at = self
            .approval_requested_at
            .ok_or(MarketingDomainError::InvalidBudgetApproval)?;
        let requested_by = self
            .approval_requested_by
            .as_ref()
            .ok_or(MarketingDomainError::InvalidBudgetApproval)?;
        if self.status != CampaignBudgetStatus::Review
            || self.approval_request != Some(decision.request)
            || self.approval_subject.as_ref() != Some(&decision.subject)
            || requested_by == &decision.decided_by
            || decision.decided_at < requested_at
            || decision.decided_at > at
        {
            return Err(MarketingDomainError::InvalidBudgetApproval);
        }
        self.bump(at)?;
        match decision.decision {
            operations::ApprovalDecision::Approved => {
                self.approved_amount = Some(self.requested_amount);
                self.status = CampaignBudgetStatus::Approved;
                self.uncommitted_events
                    .push(DomainEvent::CampaignBudgetApproved {
                        budget_id: self.id,
                        campaign_id: self.campaign_id,
                        period: self.period,
                        approved_amount: self.requested_amount,
                        dimensions: self.dimensions.clone(),
                        approved_at: decision.decided_at,
                    });
            }
            operations::ApprovalDecision::Rejected | operations::ApprovalDecision::Cancelled => {
                self.status = CampaignBudgetStatus::Cancelled;
                self.uncommitted_events
                    .push(DomainEvent::CampaignBudgetChanged);
            }
        }
        Ok(())
    }

    pub fn start(&mut self, at: DateTime<Utc>) -> Result<(), MarketingDomainError> {
        if self.status != CampaignBudgetStatus::Approved {
            return Err(MarketingDomainError::InvalidBudgetTransition);
        }
        self.bump(at)?;
        self.status = CampaignBudgetStatus::Running;
        self.uncommitted_events
            .push(DomainEvent::CampaignBudgetChanged);
        Ok(())
    }

    pub fn complete(&mut self, at: DateTime<Utc>) -> Result<(), MarketingDomainError> {
        if self.status != CampaignBudgetStatus::Running
            || self
                .reservations
                .values()
                .any(|reservation| reservation.status == SpendReservationStatus::Active)
        {
            return Err(MarketingDomainError::InvalidBudgetTransition);
        }
        self.bump(at)?;
        self.status = CampaignBudgetStatus::Completed;
        self.uncommitted_events
            .push(DomainEvent::CampaignBudgetChanged);
        Ok(())
    }

    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), MarketingDomainError> {
        if matches!(
            self.status,
            CampaignBudgetStatus::Completed | CampaignBudgetStatus::Cancelled
        ) || self
            .reservations
            .values()
            .any(|value| value.status == SpendReservationStatus::Active)
        {
            return Err(MarketingDomainError::InvalidBudgetTransition);
        }
        self.bump(at)?;
        self.status = CampaignBudgetStatus::Cancelled;
        self.uncommitted_events
            .push(DomainEvent::CampaignBudgetChanged);
        Ok(())
    }

    pub fn reserve_spend(
        &mut self,
        request_id: SpendRequestId,
        allocation_id: BudgetAllocationId,
        amount: MoneyAmount,
        expected_outcome: String,
        at: DateTime<Utc>,
    ) -> Result<(), MarketingDomainError> {
        if !matches!(
            self.status,
            CampaignBudgetStatus::Approved | CampaignBudgetStatus::Running
        ) || amount.minor_units() <= 0
            || amount.currency() != self.requested_amount.currency()
            || self.reservations.contains_key(&request_id)
            || self.reservations.len() >= MAX_SPEND_RESERVATIONS
        {
            return Err(MarketingDomainError::InvalidSpendReservation);
        }
        validate_text(&expected_outcome, "expected_outcome")?;
        let allocation = self
            .allocations
            .iter()
            .find(|value| value.allocation_id == allocation_id)
            .ok_or(MarketingDomainError::InvalidSpendReservation)?;
        let reserved = self.reservations.values().try_fold(0_i64, |total, value| {
            if value.allocation_id == allocation_id
                && value.status == SpendReservationStatus::Active
            {
                total.checked_add(value.amount.minor_units())
            } else {
                Some(total)
            }
        });
        let next = reserved.and_then(|value| value.checked_add(amount.minor_units()));
        if next.is_none_or(|value| value > allocation.planned_amount.minor_units()) {
            return Err(MarketingDomainError::InvalidSpendReservation);
        }
        let tactic = allocation.tactic;
        let channel = allocation.channel;
        self.bump(at)?;
        self.reservations.insert(
            request_id,
            SpendReservation {
                request_id,
                allocation_id,
                amount,
                expected_outcome: expected_outcome.clone(),
                status: SpendReservationStatus::Active,
                requested_at: at,
                released_at: None,
            },
        );
        self.uncommitted_events.push(DomainEvent::SpendRequested {
            budget_id: self.id,
            request_id,
            campaign_id: self.campaign_id,
            allocation_id,
            tactic,
            channel,
            amount,
            dimensions: self.dimensions.clone(),
            expected_outcome,
            requested_at: at,
        });
        Ok(())
    }

    pub fn release_spend(
        &mut self,
        request_id: SpendRequestId,
        at: DateTime<Utc>,
    ) -> Result<(), MarketingDomainError> {
        let reservation = self
            .reservations
            .get(&request_id)
            .ok_or(MarketingDomainError::InvalidSpendReservation)?;
        if reservation.status != SpendReservationStatus::Active {
            return Err(MarketingDomainError::InvalidSpendReservation);
        }
        self.bump(at)?;
        let reservation = self
            .reservations
            .get_mut(&request_id)
            .ok_or(MarketingDomainError::InvalidSpendReservation)?;
        reservation.status = SpendReservationStatus::Released;
        reservation.released_at = Some(at);
        self.uncommitted_events
            .push(DomainEvent::CampaignBudgetChanged);
        Ok(())
    }

    pub fn validate(&self) -> Result<(), MarketingDomainError> {
        exact_scope(&self.scope)?;
        validate_text(&self.code, "budget_code")?;
        validate_text(&self.name, "budget_name")?;
        validate_budget_dimensions(&self.scope, self.campaign_id, &self.dimensions)?;
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(MarketingDomainError::InvalidTimestamp);
        }
        if self.requested_amount.minor_units() <= 0
            || self.allocations.is_empty()
            || self.allocations.len() > MAX_BUDGET_ALLOCATIONS
        {
            return Err(MarketingDomainError::InvalidBudgetAllocations);
        }
        let mut ids = BTreeSet::new();
        let allocated = self.allocations.iter().try_fold(0_i64, |total, value| {
            if !ids.insert(value.allocation_id)
                || value.planned_amount.minor_units() <= 0
                || value.planned_amount.currency() != self.requested_amount.currency()
            {
                return None;
            }
            total.checked_add(value.planned_amount.minor_units())
        });
        if allocated != Some(self.requested_amount.minor_units()) {
            return Err(MarketingDomainError::InvalidBudgetAllocations);
        }
        let approval_shape = self.approval_request.is_some()
            && self.approval_subject.is_some()
            && self.approval_requested_by.is_some()
            && self.approval_requested_at.is_some();
        if matches!(self.status, CampaignBudgetStatus::Draft) && approval_shape
            || !matches!(self.status, CampaignBudgetStatus::Draft) && !approval_shape
            || matches!(
                self.status,
                CampaignBudgetStatus::Approved
                    | CampaignBudgetStatus::Running
                    | CampaignBudgetStatus::Completed
            ) && self.approved_amount != Some(self.requested_amount)
            || matches!(
                self.status,
                CampaignBudgetStatus::Draft | CampaignBudgetStatus::Review
            ) && self.approved_amount.is_some()
        {
            return Err(MarketingDomainError::InvalidBudgetApproval);
        }
        for reservation in self.reservations.values() {
            validate_text(&reservation.expected_outcome, "expected_outcome")?;
            if reservation.amount.minor_units() <= 0
                || reservation.amount.currency() != self.requested_amount.currency()
                || !self
                    .allocations
                    .iter()
                    .any(|value| value.allocation_id == reservation.allocation_id)
                || reservation.requested_at < self.created_at
                || reservation.requested_at > self.updated_at
                || (reservation.status == SpendReservationStatus::Active
                    && reservation.released_at.is_some())
                || (reservation.status == SpendReservationStatus::Released
                    && reservation
                        .released_at
                        .is_none_or(|released| released < reservation.requested_at))
            {
                return Err(MarketingDomainError::InvalidSpendReservation);
            }
        }
        if self.status == CampaignBudgetStatus::Completed
            && self
                .reservations
                .values()
                .any(|reservation| reservation.status == SpendReservationStatus::Active)
        {
            return Err(MarketingDomainError::InvalidBudgetTransition);
        }
        for allocation in &self.allocations {
            let active = self.reservations.values().try_fold(0_i64, |total, value| {
                if value.allocation_id == allocation.allocation_id
                    && value.status == SpendReservationStatus::Active
                {
                    total.checked_add(value.amount.minor_units())
                } else {
                    Some(total)
                }
            });
            if active.is_none_or(|value| value > allocation.planned_amount.minor_units()) {
                return Err(MarketingDomainError::InvalidSpendReservation);
            }
        }
        Ok(())
    }

    fn bump(&mut self, at: DateTime<Utc>) -> Result<(), MarketingDomainError> {
        if at < self.updated_at {
            return Err(MarketingDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(MarketingDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> CampaignBudgetId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn campaign_id(&self) -> CampaignId {
        self.campaign_id
    }
    pub fn code(&self) -> &str {
        &self.code
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub const fn period(&self) -> marketing::CampaignBudgetPeriod {
        self.period
    }
    pub const fn requested_amount(&self) -> MoneyAmount {
        self.requested_amount
    }
    pub const fn approved_amount(&self) -> Option<MoneyAmount> {
        self.approved_amount
    }
    pub const fn dimensions(&self) -> &DimensionSet {
        &self.dimensions
    }
    pub fn allocations(&self) -> &[BudgetAllocation] {
        &self.allocations
    }
    pub fn reservations(&self) -> &BTreeMap<SpendRequestId, SpendReservation> {
        &self.reservations
    }
    pub const fn approval_request(&self) -> Option<operations::ApprovalRequestRef> {
        self.approval_request
    }
    pub const fn approval_subject(&self) -> Option<&RecordRef> {
        self.approval_subject.as_ref()
    }
    pub const fn approval_requested_by(
        &self,
    ) -> Option<&erpx_integration_contracts::ReferenceCode> {
        self.approval_requested_by.as_ref()
    }
    pub const fn approval_requested_at(&self) -> Option<DateTime<Utc>> {
        self.approval_requested_at
    }
    pub const fn status(&self) -> CampaignBudgetStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub fn reserved_minor(&self) -> Result<i64, MarketingDomainError> {
        self.reservations.values().try_fold(0_i64, |total, value| {
            if value.status == SpendReservationStatus::Active {
                total
                    .checked_add(value.amount.minor_units())
                    .ok_or(MarketingDomainError::ArithmeticOverflow)
            } else {
                Ok(total)
            }
        })
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "state", rename_all = "snake_case")]
pub enum MarketingAggregate {
    LeadCandidate(Box<LeadCandidate>),
    CampaignBudget(Box<CampaignBudget>),
}

impl MarketingAggregate {
    pub const fn id_uuid(&self) -> Uuid {
        match self {
            Self::LeadCandidate(value) => value.id().as_uuid(),
            Self::CampaignBudget(value) => value.id().as_uuid(),
        }
    }
    pub const fn version(&self) -> u64 {
        match self {
            Self::LeadCandidate(value) => value.version(),
            Self::CampaignBudget(value) => value.version(),
        }
    }
    pub const fn scope(&self) -> &AuthorityScope {
        match self {
            Self::LeadCandidate(value) => value.scope(),
            Self::CampaignBudget(value) => value.scope(),
        }
    }
    pub const fn aggregate_type(&self) -> &'static str {
        match self {
            Self::LeadCandidate(_) => "lead_candidate",
            Self::CampaignBudget(_) => "campaign_budget",
        }
    }
    pub fn validate(&self) -> Result<(), MarketingDomainError> {
        match self {
            Self::LeadCandidate(value) => value.validate(),
            Self::CampaignBudget(value) => value.validate(),
        }
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        match self {
            Self::LeadCandidate(value) => value.take_uncommitted_events(),
            Self::CampaignBudget(value) => value.take_uncommitted_events(),
        }
    }
}

fn validate_budget_dimensions(
    scope: &AuthorityScope,
    campaign_id: CampaignId,
    dimensions: &DimensionSet,
) -> Result<(), MarketingDomainError> {
    let (company_id, branch_id) = match scope {
        AuthorityScope::Company { company_id, .. } => (*company_id, None),
        AuthorityScope::Branch {
            company_id,
            branch_id,
            ..
        } => (*company_id, Some(*branch_id)),
        _ => return Err(MarketingDomainError::InvalidScope),
    };
    let branch_matches = match (branch_id, dimensions.branch) {
        (None, None) => true,
        (Some(expected), Some(actual)) => expected == actual.as_uuid(),
        _ => false,
    };
    if dimensions.legal_entity.as_uuid() != company_id
        || !branch_matches
        || dimensions.business_unit.is_none()
        || dimensions.cost_center.is_none()
        || dimensions.campaign.map(|value| value.as_uuid()) != Some(campaign_id.as_uuid())
        || dimensions.channel.is_some()
    {
        Err(MarketingDomainError::InvalidBudgetDimensions)
    } else {
        Ok(())
    }
}

fn exact_scope(scope: &AuthorityScope) -> Result<(), MarketingDomainError> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } if !holding_id.is_nil() && !company_id.is_nil() => Ok(()),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } if !holding_id.is_nil() && !company_id.is_nil() && !branch_id.is_nil() => Ok(()),
        _ => Err(MarketingDomainError::InvalidScope),
    }
}

fn validate_text(value: &str, field: &'static str) -> Result<(), MarketingDomainError> {
    if value.trim().is_empty() {
        return Err(MarketingDomainError::Required { field });
    }
    if value.len() > MAX_MARKETING_TEXT_BYTES {
        return Err(MarketingDomainError::TooLong {
            field,
            maximum: MAX_MARKETING_TEXT_BYTES,
        });
    }
    if value.chars().any(char::is_control) {
        return Err(MarketingDomainError::ControlCharacter { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use erpx_integration_contracts::{CostCenterRef, CurrencyCode, ReferenceCode, organization};

    fn scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }
    fn lead() -> LeadCandidate {
        LeadCandidate::create(
            LeadCandidateId::try_from_uuid(Uuid::from_u128(3)).unwrap(),
            scope(),
            CampaignId::try_from_uuid(Uuid::from_u128(4)).unwrap(),
            AudienceMemberId::try_from_uuid(Uuid::from_u128(5)).unwrap(),
            "organic".into(),
            None,
            Utc::now(),
        )
        .unwrap()
    }

    fn campaign_id() -> CampaignId {
        CampaignId::try_from_uuid(Uuid::from_u128(40)).unwrap()
    }

    fn money(minor_units: i64) -> MoneyAmount {
        MoneyAmount::new(minor_units, CurrencyCode::try_new("THB").unwrap())
    }

    fn budget_dimensions() -> DimensionSet {
        DimensionSet {
            legal_entity: organization::CompanyRef::try_from_uuid(Uuid::from_u128(2)).unwrap(),
            branch: None,
            business_unit: Some(
                organization::BusinessUnitRef::try_from_uuid(Uuid::from_u128(41)).unwrap(),
            ),
            cost_center: Some(CostCenterRef::try_from_uuid(Uuid::from_u128(42)).unwrap()),
            location: None,
            project: None,
            campaign: Some(marketing::CampaignRef::try_from_uuid(campaign_id().as_uuid()).unwrap()),
            channel: None,
            party: None,
            employee: None,
            asset: None,
            legal_matter: None,
            intercompany_partner: None,
        }
    }

    fn allocation(id: u128, amount: i64) -> BudgetAllocation {
        BudgetAllocation {
            allocation_id: BudgetAllocationId::try_from_uuid(Uuid::from_u128(id)).unwrap(),
            tactic: marketing::TacticRef::try_from_uuid(Uuid::from_u128(id + 100)).unwrap(),
            channel: marketing::ChannelRef::try_from_uuid(Uuid::from_u128(id + 200)).unwrap(),
            planned_amount: money(amount),
        }
    }

    fn budget() -> CampaignBudget {
        CampaignBudget::create(
            CampaignBudgetId::try_from_uuid(Uuid::from_u128(43)).unwrap(),
            scope(),
            campaign_id(),
            "FY27-GROWTH".into(),
            "FY27 growth plan".into(),
            marketing::CampaignBudgetPeriod::Annual,
            money(100_000),
            budget_dimensions(),
            vec![allocation(44, 60_000), allocation(45, 40_000)],
            Utc::now(),
        )
        .unwrap()
    }

    fn approval_request(budget: &CampaignBudget) -> operations::ApprovalRequestFact {
        operations::ApprovalRequestFact {
            request: operations::ApprovalRequestRef::try_from_uuid(Uuid::from_u128(46)).unwrap(),
            subject: RecordRef::try_new(
                BoundedContext::Marketing,
                "campaign_budget",
                budget.id().as_uuid(),
                budget.version(),
            )
            .unwrap(),
            policy_code: ReferenceCode::try_new("marketing-budget").unwrap(),
            amount: Some(budget.requested_amount()),
            requested_by: ReferenceCode::try_new("budget-owner").unwrap(),
            requested_at: budget.updated_at(),
        }
    }

    #[test]
    fn qualification_is_terminal_and_bounded() {
        let mut lead = lead();
        let at = lead.created_at();
        assert_eq!(
            lead.qualify(0, at),
            Err(MarketingDomainError::InvalidQualificationScore)
        );
        lead.qualify(7_500, at).unwrap();
        assert_eq!(lead.status(), LeadCandidateStatus::Qualified);
        assert_eq!(
            lead.reject_for_test(at),
            Err(MarketingDomainError::InvalidTransition)
        );
    }

    #[test]
    fn engagement_is_only_owned_before_handoff() {
        let mut lead = lead();
        let at = lead.created_at();
        lead.capture_engagement("email".into(), "opened".into(), at)
            .unwrap();
        lead.qualify(8_000, at).unwrap();
        assert_eq!(
            lead.capture_engagement("web".into(), "visit".into(), at),
            Err(MarketingDomainError::InvalidTransition)
        );
    }

    #[test]
    fn timestamps_and_scopes_are_authoritative() {
        let mut lead = lead();
        assert_eq!(
            lead.reject_for_test(lead.created_at() - Duration::seconds(1)),
            Err(MarketingDomainError::InvalidTimestamp)
        );
        assert!(matches!(
            LeadCandidate::create(
                LeadCandidateId::try_from_uuid(Uuid::from_u128(9)).unwrap(),
                AuthorityScope::Platform,
                CampaignId::try_from_uuid(Uuid::from_u128(4)).unwrap(),
                AudienceMemberId::try_from_uuid(Uuid::from_u128(5)).unwrap(),
                "x".into(),
                None,
                Utc::now()
            ),
            Err(MarketingDomainError::InvalidScope)
        ));
    }

    #[test]
    fn budget_requires_complete_dimensions_and_conserved_allocations() {
        let mut dimensions = budget_dimensions();
        dimensions.cost_center = None;
        assert_eq!(
            CampaignBudget::create(
                CampaignBudgetId::try_from_uuid(Uuid::from_u128(50)).unwrap(),
                scope(),
                campaign_id(),
                "INVALID-DIMS".into(),
                "Invalid dimensions".into(),
                marketing::CampaignBudgetPeriod::Campaign,
                money(100),
                dimensions,
                vec![allocation(51, 100)],
                Utc::now(),
            )
            .unwrap_err(),
            MarketingDomainError::InvalidBudgetDimensions
        );

        assert_eq!(
            CampaignBudget::create(
                CampaignBudgetId::try_from_uuid(Uuid::from_u128(52)).unwrap(),
                scope(),
                campaign_id(),
                "INVALID-TOTAL".into(),
                "Invalid allocation total".into(),
                marketing::CampaignBudgetPeriod::Campaign,
                money(100),
                budget_dimensions(),
                vec![allocation(53, 99)],
                Utc::now(),
            )
            .unwrap_err(),
            MarketingDomainError::InvalidBudgetAllocations
        );
    }

    #[test]
    fn budget_approval_is_version_bound_and_maker_checker() {
        let mut budget = budget();
        let request = approval_request(&budget);
        let subject = request.subject.clone();
        let requested_at = request.requested_at;
        budget
            .submit_for_review(request.clone(), requested_at)
            .unwrap();
        assert_eq!(budget.status(), CampaignBudgetStatus::Review);

        let self_approval = operations::ApprovalDecisionFact {
            request: request.request,
            subject: subject.clone(),
            decision: operations::ApprovalDecision::Approved,
            decided_by: request.requested_by.clone(),
            reason_code: ReferenceCode::try_new("within-ceiling").unwrap(),
            decided_at: requested_at,
        };
        assert_eq!(
            budget.apply_approval_decision(self_approval, requested_at),
            Err(MarketingDomainError::InvalidBudgetApproval)
        );

        budget
            .apply_approval_decision(
                operations::ApprovalDecisionFact {
                    request: request.request,
                    subject,
                    decision: operations::ApprovalDecision::Approved,
                    decided_by: ReferenceCode::try_new("finance-controller").unwrap(),
                    reason_code: ReferenceCode::try_new("within-ceiling").unwrap(),
                    decided_at: requested_at,
                },
                requested_at,
            )
            .unwrap();
        assert_eq!(budget.status(), CampaignBudgetStatus::Approved);
        assert_eq!(budget.approved_amount(), Some(money(100_000)));

        let events =
            crate::integration::published_events(&budget.take_uncommitted_events()).unwrap();
        assert!(matches!(
            events.as_slice(),
            [
                marketing::Event::CampaignBudgetApprovalRequested(_),
                marketing::Event::CampaignBudgetApproved(_)
            ]
        ));
    }

    #[test]
    fn spend_reservation_cannot_exceed_its_approved_allocation() {
        let mut budget = budget();
        let request = approval_request(&budget);
        let subject = request.subject.clone();
        let at = request.requested_at;
        budget.submit_for_review(request.clone(), at).unwrap();
        budget
            .apply_approval_decision(
                operations::ApprovalDecisionFact {
                    request: request.request,
                    subject,
                    decision: operations::ApprovalDecision::Approved,
                    decided_by: ReferenceCode::try_new("finance-controller").unwrap(),
                    reason_code: ReferenceCode::try_new("approved").unwrap(),
                    decided_at: at,
                },
                at,
            )
            .unwrap();

        let allocation_id = BudgetAllocationId::try_from_uuid(Uuid::from_u128(44)).unwrap();
        budget
            .reserve_spend(
                SpendRequestId::try_from_uuid(Uuid::from_u128(60)).unwrap(),
                allocation_id,
                money(50_000),
                "Agency production".into(),
                at,
            )
            .unwrap();
        assert_eq!(
            budget.reserve_spend(
                SpendRequestId::try_from_uuid(Uuid::from_u128(61)).unwrap(),
                allocation_id,
                money(10_001),
                "Media placement".into(),
                at,
            ),
            Err(MarketingDomainError::InvalidSpendReservation)
        );
        assert_eq!(budget.reserved_minor().unwrap(), 50_000);
    }

    #[test]
    fn budget_completion_requires_every_spend_reservation_to_reconcile() {
        let mut budget = budget();
        let request = approval_request(&budget);
        let subject = request.subject.clone();
        let at = request.requested_at;
        budget.submit_for_review(request.clone(), at).unwrap();
        budget
            .apply_approval_decision(
                operations::ApprovalDecisionFact {
                    request: request.request,
                    subject,
                    decision: operations::ApprovalDecision::Approved,
                    decided_by: ReferenceCode::try_new("finance-controller").unwrap(),
                    reason_code: ReferenceCode::try_new("approved").unwrap(),
                    decided_at: at,
                },
                at,
            )
            .unwrap();
        budget.start(at).unwrap();
        let spend_id = SpendRequestId::try_from_uuid(Uuid::from_u128(62)).unwrap();
        budget
            .reserve_spend(
                spend_id,
                BudgetAllocationId::try_from_uuid(Uuid::from_u128(44)).unwrap(),
                money(10_000),
                "Provider delivery".into(),
                at,
            )
            .unwrap();
        assert_eq!(
            budget.complete(at),
            Err(MarketingDomainError::InvalidBudgetTransition)
        );
        budget.release_spend(spend_id, at).unwrap();
        budget.complete(at).unwrap();
        assert_eq!(budget.status(), CampaignBudgetStatus::Completed);
    }
}
mod records;

pub use records::{MarketingRecord, MarketingRecordKind, MarketingRecordStatus};
