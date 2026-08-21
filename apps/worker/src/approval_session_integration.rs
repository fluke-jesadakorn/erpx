//! Typed, idempotent handoff between owner contexts and Operations approvals.
//!
//! Approval requests are converted into Operations commands. Completed
//! sessions are converted back into owner commands; neither path writes a
//! foreign bounded-context table.

use std::collections::BTreeMap;

use erpx_finance::{AccountingPlanId, PostingPacketId, SalesPostingCommand};
use erpx_integration_contracts::{
    IntegrationEventEnvelope, RecordRef, finance, operations as contract, sales,
};
use erpx_kernel::{ActorAttribution, ActorKind, AuthorityScope, EventId};
use erpx_operations::{
    ApprovalGateKind, ApprovalGateSpec, ApprovalGateStatus, ApprovalSession,
    ApprovalSessionCommand, ApprovalSessionCommandRequest, ApprovalSessionId,
    ApprovalSessionStatus,
};
use erpx_sales::{
    QuoteApprovalEvidence, QuoteApprovalRejectionEvidence, QuoteId, QuoteWorkflowCommand,
};
use thiserror::Error;
use uuid::Uuid;

const OWNER_HANDOFF_NAMESPACE: Uuid = Uuid::from_u128(0x4552_5058_4150_5052_4f56_414c_0000_0001);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalIngressOutcome {
    Staged(ApprovalSessionCommandRequest),
    Replay(ApprovalSessionCommandRequest),
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApprovalSessionIngressState {
    checkpoint: u64,
    staged: BTreeMap<EventId, ApprovalSessionCommandRequest>,
}

impl ApprovalSessionIngressState {
    pub fn consume_sales(
        &mut self,
        global_sequence: u64,
        envelope: &IntegrationEventEnvelope<sales::Event>,
    ) -> Result<ApprovalIngressOutcome, ApprovalIntegrationError> {
        envelope
            .validate()
            .map_err(|_| ApprovalIntegrationError::InvalidEnvelope)?;
        self.ensure_sequence(global_sequence, envelope.event_id())?;
        let request = match envelope.payload() {
            sales::Event::QuoteApprovalRequested(fact) => Some(request_from_fact(
                envelope.event_id(),
                envelope.active_scope().clone(),
                &fact.approval,
            )?),
            sales::Event::SalesOrderChanged(_)
            | sales::Event::RevenueArrangementActivated(_)
            | sales::Event::FulfillmentAccepted(_)
            | sales::Event::BillingEligible(_) => None,
        };
        self.stage(global_sequence, envelope.event_id(), request)
    }

    pub fn consume_finance(
        &mut self,
        global_sequence: u64,
        envelope: &IntegrationEventEnvelope<finance::Event>,
    ) -> Result<ApprovalIngressOutcome, ApprovalIntegrationError> {
        envelope
            .validate()
            .map_err(|_| ApprovalIntegrationError::InvalidEnvelope)?;
        self.ensure_sequence(global_sequence, envelope.event_id())?;
        let request = match envelope.payload() {
            finance::Event::AccountingPlanApprovalRequested(fact) => Some(request_from_fact(
                envelope.event_id(),
                envelope.active_scope().clone(),
                &fact.approval,
            )?),
            finance::Event::PostingApprovalRequested(fact) => Some(request_from_fact(
                envelope.event_id(),
                envelope.active_scope().clone(),
                &fact.approval,
            )?),
            finance::Event::JournalPosted(_)
            | finance::Event::ReceivableChanged(_)
            | finance::Event::CustomerInvoicePosted(_)
            | finance::Event::VendorPayablePosted(_)
            | finance::Event::PaymentReleased(_)
            | finance::Event::RevenueRecognized(_)
            | finance::Event::CashApplied(_)
            | finance::Event::MarketingSpendPosted(_) => None,
        };
        self.stage(global_sequence, envelope.event_id(), request)
    }

    fn ensure_sequence(
        &self,
        global_sequence: u64,
        event_id: EventId,
    ) -> Result<(), ApprovalIntegrationError> {
        if global_sequence == 0 {
            return Err(ApprovalIntegrationError::InvalidSequence);
        }
        if self.staged.contains_key(&event_id) {
            return Ok(());
        }
        if global_sequence <= self.checkpoint {
            return Err(ApprovalIntegrationError::CheckpointRegression);
        }
        Ok(())
    }

    fn stage(
        &mut self,
        global_sequence: u64,
        event_id: EventId,
        request: Option<ApprovalSessionCommandRequest>,
    ) -> Result<ApprovalIngressOutcome, ApprovalIntegrationError> {
        if let Some(staged) = self.staged.get(&event_id) {
            return Ok(ApprovalIngressOutcome::Replay(staged.clone()));
        }
        self.checkpoint = global_sequence;
        let Some(request) = request else {
            return Ok(ApprovalIngressOutcome::Ignored);
        };
        self.staged.insert(event_id, request.clone());
        Ok(ApprovalIngressOutcome::Staged(request))
    }

    pub const fn checkpoint(&self) -> u64 {
        self.checkpoint
    }
}

fn request_from_fact(
    event_id: EventId,
    scope: AuthorityScope,
    fact: &contract::ApprovalSessionRequestFact,
) -> Result<ApprovalSessionCommandRequest, ApprovalIntegrationError> {
    fact.validate()
        .map_err(|_| ApprovalIntegrationError::InvalidApprovalRequest)?;
    let session_id = ApprovalSessionId::try_from_uuid(fact.session.as_uuid())
        .map_err(|_| ApprovalIntegrationError::InvalidApprovalRequest)?;
    Ok(ApprovalSessionCommandRequest {
        command_id: event_id.as_uuid(),
        idempotency_key: format!("operations.approval-session:{}", event_id.as_uuid()),
        scope,
        actor: ActorAttribution {
            principal_ref: "system-worker:approval-session-ingress".into(),
            kind: ActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        expected_version: 0,
        command: ApprovalSessionCommand::Create {
            session_id,
            subject: fact.subject.clone(),
            policy_version: fact.policy_version,
            requested_by: fact.requested_by.as_str().into(),
            gates: fact
                .required_gates
                .iter()
                .copied()
                .map(|kind| ApprovalGateSpec {
                    kind: map_gate(kind),
                    assigned_to: None,
                })
                .collect(),
        },
        at: fact.requested_at,
    })
}

const fn map_gate(kind: contract::ApprovalGateKind) -> ApprovalGateKind {
    match kind {
        contract::ApprovalGateKind::SalesCommercial => ApprovalGateKind::SalesCommercial,
        contract::ApprovalGateKind::FinanceAccountingPlan => {
            ApprovalGateKind::FinanceAccountingPlan
        }
        contract::ApprovalGateKind::FinancePosting => ApprovalGateKind::FinancePosting,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalOwnerCommand {
    Sales(QuoteWorkflowCommand),
    Finance(SalesPostingCommand),
    FinancePostingApproval { packet_id: PostingPacketId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalOwnerHandoff {
    pub handoff_id: Uuid,
    pub approval_session_id: ApprovalSessionId,
    pub approval_session_version: u64,
    pub scope: AuthorityScope,
    pub subject: RecordRef,
    pub command: ApprovalOwnerCommand,
}

pub fn owner_handoff(
    session: &ApprovalSession,
) -> Result<Option<ApprovalOwnerHandoff>, ApprovalIntegrationError> {
    let command = match session.status() {
        ApprovalSessionStatus::Pending
        | ApprovalSessionStatus::Cancelled
        | ApprovalSessionStatus::Superseded => return Ok(None),
        ApprovalSessionStatus::Approved | ApprovalSessionStatus::Rejected => {
            owner_command(session)?
        }
    };
    let mut name = session.id().as_uuid().as_bytes().to_vec();
    name.extend_from_slice(&session.version().to_be_bytes());
    Ok(Some(ApprovalOwnerHandoff {
        handoff_id: Uuid::new_v5(&OWNER_HANDOFF_NAMESPACE, &name),
        approval_session_id: session.id(),
        approval_session_version: session.version(),
        scope: session.scope().clone(),
        subject: session.subject().clone(),
        command,
    }))
}

fn owner_command(
    session: &ApprovalSession,
) -> Result<ApprovalOwnerCommand, ApprovalIntegrationError> {
    match (
        session.subject().context,
        session.subject().aggregate_kind.as_str(),
    ) {
        (erpx_integration_contracts::BoundedContext::Sales, "quote") => {
            quote_owner_command(session).map(ApprovalOwnerCommand::Sales)
        }
        (erpx_integration_contracts::BoundedContext::Finance, "accounting_plan") => {
            let plan_id = AccountingPlanId::try_from_uuid(session.subject().aggregate_id)
                .map_err(|_| ApprovalIntegrationError::InvalidSubject)?;
            Ok(ApprovalOwnerCommand::Finance(match session.status() {
                ApprovalSessionStatus::Approved => SalesPostingCommand::ApprovePlan { plan_id },
                ApprovalSessionStatus::Rejected => SalesPostingCommand::RejectPlan { plan_id },
                _ => return Err(ApprovalIntegrationError::IncompleteDecision),
            }))
        }
        (erpx_integration_contracts::BoundedContext::Finance, "posting_packet") => {
            let packet_id = PostingPacketId::try_from_uuid(session.subject().aggregate_id)
                .map_err(|_| ApprovalIntegrationError::InvalidSubject)?;
            match session.status() {
                // The Finance command service reloads current authority data and
                // supplies the validation snapshot before executing this intent.
                ApprovalSessionStatus::Approved => {
                    Ok(ApprovalOwnerCommand::FinancePostingApproval { packet_id })
                }
                ApprovalSessionStatus::Rejected => Ok(ApprovalOwnerCommand::Finance(
                    SalesPostingCommand::RejectPacket { packet_id },
                )),
                _ => Err(ApprovalIntegrationError::IncompleteDecision),
            }
        }
        _ => Err(ApprovalIntegrationError::InvalidSubject),
    }
}

fn quote_owner_command(
    session: &ApprovalSession,
) -> Result<QuoteWorkflowCommand, ApprovalIntegrationError> {
    let quote_id = QuoteId::try_from_uuid(session.subject().aggregate_id)
        .map_err(|_| ApprovalIntegrationError::InvalidSubject)?;
    match session.status() {
        ApprovalSessionStatus::Approved => {
            let sales_gate = session
                .gates()
                .get(&ApprovalGateKind::SalesCommercial)
                .ok_or(ApprovalIntegrationError::IncompleteDecision)?;
            let finance_gate = session
                .gates()
                .get(&ApprovalGateKind::FinanceAccountingPlan)
                .ok_or(ApprovalIntegrationError::IncompleteDecision)?;
            let sales_approver = sales_gate
                .decided_by()
                .ok_or(ApprovalIntegrationError::IncompleteDecision)?;
            let finance_approver = finance_gate
                .decided_by()
                .ok_or(ApprovalIntegrationError::IncompleteDecision)?;
            let approved_at = sales_gate
                .decided_at()
                .into_iter()
                .chain(finance_gate.decided_at())
                .max()
                .ok_or(ApprovalIntegrationError::IncompleteDecision)?;
            Ok(QuoteWorkflowCommand::RecordApproval {
                quote_id,
                evidence: QuoteApprovalEvidence {
                    approval_session_id: session.id().as_uuid(),
                    quote_version: session.subject().aggregate_version,
                    policy_version: session.policy_version(),
                    sales_approver: sales_approver.into(),
                    finance_approver: finance_approver.into(),
                    approved_at,
                },
            })
        }
        ApprovalSessionStatus::Rejected => {
            let (kind, gate) = session
                .gates()
                .iter()
                .find(|(_, gate)| gate.status() == ApprovalGateStatus::Rejected)
                .ok_or(ApprovalIntegrationError::IncompleteDecision)?;
            Ok(QuoteWorkflowCommand::RejectApproval {
                quote_id,
                evidence: QuoteApprovalRejectionEvidence {
                    approval_session_id: session.id().as_uuid(),
                    quote_version: session.subject().aggregate_version,
                    policy_version: session.policy_version(),
                    rejected_gate: format!("{kind:?}").to_ascii_lowercase(),
                    rejected_by: gate
                        .decided_by()
                        .ok_or(ApprovalIntegrationError::IncompleteDecision)?
                        .into(),
                    reason_code: gate
                        .reason_code()
                        .ok_or(ApprovalIntegrationError::IncompleteDecision)?
                        .into(),
                    rejected_at: gate
                        .decided_at()
                        .ok_or(ApprovalIntegrationError::IncompleteDecision)?,
                },
            })
        }
        _ => Err(ApprovalIntegrationError::IncompleteDecision),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApprovalIntegrationError {
    #[error("approval integration envelope is invalid")]
    InvalidEnvelope,
    #[error("approval integration sequence is invalid")]
    InvalidSequence,
    #[error("approval integration checkpoint moved backwards")]
    CheckpointRegression,
    #[error("approval request fact is invalid")]
    InvalidApprovalRequest,
    #[error("approval subject is invalid")]
    InvalidSubject,
    #[error("approval decision is incomplete")]
    IncompleteDecision,
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use erpx_integration_contracts::{AggregateKind, ReferenceCode};
    use erpx_kernel::{AggregateId, CorrelationId, HumanDecisionProof, SessionId};
    use erpx_operations::approval_session_decision_ref;

    use super::*;

    fn uuid(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: uuid(1),
            company_id: uuid(2),
        }
    }

    fn at(second: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 18, 10, 0, second).unwrap()
    }

    fn human_decision(
        principal_ref: &str,
        command_id: Uuid,
        session_id: ApprovalSessionId,
        subject_version: u64,
        gate: ApprovalGateKind,
        outcome: ApprovalGateStatus,
        decided_at: chrono::DateTime<Utc>,
    ) -> HumanDecisionProof {
        HumanDecisionProof::try_new(
            ActorAttribution {
                principal_ref: principal_ref.into(),
                kind: ActorKind::HoldingUser,
                session_id: Some(
                    SessionId::try_from_uuid(uuid(99)).expect("non-nil human session"),
                ),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope(),
            approval_session_decision_ref(command_id, session_id, subject_version, gate, outcome),
            decided_at,
        )
        .expect("valid human decision proof")
    }

    fn quote_subject() -> RecordRef {
        RecordRef::try_new(
            erpx_integration_contracts::BoundedContext::Sales,
            "quote",
            uuid(10),
            4,
        )
        .unwrap()
    }

    fn quote_request_envelope() -> IntegrationEventEnvelope<sales::Event> {
        let request = contract::ApprovalSessionRequestFact {
            session: contract::ApprovalSessionRef::try_from_uuid(uuid(11)).unwrap(),
            subject: quote_subject(),
            policy_version: 3,
            required_gates: vec![
                contract::ApprovalGateKind::SalesCommercial,
                contract::ApprovalGateKind::FinanceAccountingPlan,
            ],
            requested_by: ReferenceCode::try_new("sales-maker").unwrap(),
            requested_at: at(0),
        };
        IntegrationEventEnvelope::try_new(
            EventId::try_from_uuid(uuid(12)).unwrap(),
            scope(),
            AggregateKind::try_new("quote").unwrap(),
            AggregateId::try_from_uuid(uuid(10)).unwrap(),
            7,
            CorrelationId::try_from_uuid(uuid(13)).unwrap(),
            None,
            at(0),
            sales::Event::QuoteApprovalRequested(sales::QuoteApprovalRequestedFact {
                quote: sales::QuoteRef::try_from_uuid(uuid(10)).unwrap(),
                approval: request,
            }),
        )
        .unwrap()
    }

    #[test]
    fn request_ingress_preserves_the_business_maker_and_replays_by_event_id() {
        let envelope = quote_request_envelope();
        let mut state = ApprovalSessionIngressState::default();
        let ApprovalIngressOutcome::Staged(first) = state.consume_sales(20, &envelope).unwrap()
        else {
            panic!("approval request must be staged")
        };
        let ApprovalSessionCommand::Create {
            requested_by,
            subject,
            gates,
            ..
        } = &first.command
        else {
            panic!("ingress must create a session")
        };
        assert_eq!(
            first.actor.principal_ref,
            "system-worker:approval-session-ingress"
        );
        assert_eq!(first.actor.kind, ActorKind::SystemWorker);
        assert_eq!(requested_by, "sales-maker");
        assert_eq!(subject, &quote_subject());
        assert_eq!(gates.len(), 2);
        assert_eq!(state.checkpoint(), 20);
        assert_eq!(
            state.consume_sales(20, &envelope).unwrap(),
            ApprovalIngressOutcome::Replay(first)
        );
    }

    #[test]
    fn completed_quote_session_returns_typed_owner_commands_for_approval_and_rejection() {
        let mut approved = ApprovalSession::create(
            ApprovalSessionId::try_from_uuid(uuid(11)).unwrap(),
            scope(),
            quote_subject(),
            3,
            "sales-maker".into(),
            vec![
                ApprovalGateSpec {
                    kind: ApprovalGateKind::SalesCommercial,
                    assigned_to: None,
                },
                ApprovalGateSpec {
                    kind: ApprovalGateKind::FinanceAccountingPlan,
                    assigned_to: None,
                },
            ],
            at(0),
        )
        .unwrap();
        approved
            .decide(
                uuid(12),
                ApprovalGateKind::SalesCommercial,
                4,
                ApprovalGateStatus::Approved,
                human_decision(
                    "sales-reviewer",
                    uuid(12),
                    ApprovalSessionId::try_from_uuid(uuid(11)).unwrap(),
                    4,
                    ApprovalGateKind::SalesCommercial,
                    ApprovalGateStatus::Approved,
                    at(1),
                ),
                "terms_ok".into(),
            )
            .unwrap();
        approved
            .decide(
                uuid(13),
                ApprovalGateKind::FinanceAccountingPlan,
                4,
                ApprovalGateStatus::Approved,
                human_decision(
                    "finance-reviewer",
                    uuid(13),
                    ApprovalSessionId::try_from_uuid(uuid(11)).unwrap(),
                    4,
                    ApprovalGateKind::FinanceAccountingPlan,
                    ApprovalGateStatus::Approved,
                    at(2),
                ),
                "plan_ok".into(),
            )
            .unwrap();
        let handoff = owner_handoff(&approved).unwrap().unwrap();
        let ApprovalOwnerCommand::Sales(QuoteWorkflowCommand::RecordApproval { evidence, .. }) =
            handoff.command
        else {
            panic!("approved quote must return a typed Sales approval command")
        };
        assert_eq!(evidence.quote_version, 4);
        assert_eq!(evidence.sales_approver, "sales-reviewer");
        assert_eq!(evidence.finance_approver, "finance-reviewer");

        let mut rejected = ApprovalSession::create(
            ApprovalSessionId::try_from_uuid(uuid(21)).unwrap(),
            scope(),
            quote_subject(),
            3,
            "sales-maker".into(),
            vec![ApprovalGateSpec {
                kind: ApprovalGateKind::SalesCommercial,
                assigned_to: None,
            }],
            at(0),
        )
        .unwrap();
        rejected
            .decide(
                uuid(22),
                ApprovalGateKind::SalesCommercial,
                4,
                ApprovalGateStatus::Rejected,
                human_decision(
                    "sales-reviewer",
                    uuid(22),
                    ApprovalSessionId::try_from_uuid(uuid(21)).unwrap(),
                    4,
                    ApprovalGateKind::SalesCommercial,
                    ApprovalGateStatus::Rejected,
                    at(1),
                ),
                "discount_out_of_policy".into(),
            )
            .unwrap();
        let handoff = owner_handoff(&rejected).unwrap().unwrap();
        let ApprovalOwnerCommand::Sales(QuoteWorkflowCommand::RejectApproval { evidence, .. }) =
            handoff.command
        else {
            panic!("rejected quote must return a typed Sales rejection command")
        };
        assert_eq!(evidence.quote_version, 4);
        assert_eq!(evidence.rejected_by, "sales-reviewer");
        assert_eq!(evidence.reason_code, "discount_out_of_policy");
    }

    #[test]
    fn posting_approval_returns_an_intent_that_requires_fresh_finance_validation() {
        let subject = RecordRef::try_new(
            erpx_integration_contracts::BoundedContext::Finance,
            "posting_packet",
            uuid(30),
            2,
        )
        .unwrap();
        let mut session = ApprovalSession::create(
            ApprovalSessionId::try_from_uuid(uuid(31)).unwrap(),
            scope(),
            subject,
            1,
            "finance-preparer".into(),
            vec![ApprovalGateSpec {
                kind: ApprovalGateKind::FinancePosting,
                assigned_to: None,
            }],
            at(0),
        )
        .unwrap();
        session
            .decide(
                uuid(32),
                ApprovalGateKind::FinancePosting,
                2,
                ApprovalGateStatus::Approved,
                human_decision(
                    "finance-reviewer",
                    uuid(32),
                    ApprovalSessionId::try_from_uuid(uuid(31)).unwrap(),
                    2,
                    ApprovalGateKind::FinancePosting,
                    ApprovalGateStatus::Approved,
                    at(1),
                ),
                "actuals_match".into(),
            )
            .unwrap();
        assert!(matches!(
            owner_handoff(&session).unwrap().unwrap().command,
            ApprovalOwnerCommand::FinancePostingApproval { packet_id }
                if packet_id.as_uuid() == uuid(30)
        ));
    }
}
