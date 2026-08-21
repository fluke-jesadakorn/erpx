use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use erpx_finance::{
    AccountId, AccountingObligation, AccountingPlan, AccountingPlanId, FinanceTaxClassification,
    FinanceTaxProfileSnapshot, PostingActuals, PostingPacket, PostingPacketId, PostingReceipt,
    PostingValidationContext, SalesAccountRoles,
};
use erpx_integration_contracts::{BoundedContext, DimensionSet, RecordRef, organization};
use erpx_kernel::{ActorAttribution, ActorKind, AuthorityScope, HumanDecisionProof, SessionId};
use erpx_operations::{
    ApprovalGateKind, ApprovalGateSpec, ApprovalGateStatus, ApprovalSession, ApprovalSessionId,
    ApprovalSessionStatus, approval_session_decision_ref,
};
use erpx_sales::{
    AuthoritativeTaxProfile, ExternalQuoteDecision, ExternalQuoteDecisionKind,
    QuoteAccountingPlanEvidence, QuoteApprovalEvidence, QuoteDocumentLanguage, QuoteDraftLine,
    QuoteDraftLineSource, QuoteDraftPayload, QuoteId, QuotePdfArtifact, QuoteWorkflow,
    QuoteWorkflowStatus, TaxClassification,
};
use uuid::Uuid;

use crate::{FinalPostingCoordinator, FinalPostingError, FinancePostingOwnerPort};

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_800_000_000 + seconds, 0).unwrap()
}

fn human_decision(
    scope: &AuthorityScope,
    principal_ref: &str,
    decision_ref: String,
    decided_at: DateTime<Utc>,
) -> HumanDecisionProof {
    HumanDecisionProof::try_new(
        ActorAttribution {
            principal_ref: principal_ref.into(),
            kind: ActorKind::HoldingUser,
            session_id: Some(SessionId::try_from_uuid(id(99)).expect("non-nil human session")),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope.clone(),
        decision_ref,
        decided_at,
    )
    .expect("valid human decision proof")
}

fn account(value: u128) -> AccountId {
    AccountId::try_from_uuid(id(value)).unwrap()
}

#[derive(Debug)]
struct FinanceOwner;

#[async_trait]
impl FinancePostingOwnerPort for FinanceOwner {
    async fn post_once(
        &self,
        idempotency_key: &str,
        instructions: &erpx_finance::PostingInstructions,
    ) -> Result<PostingReceipt, FinalPostingError> {
        assert!(idempotency_key.starts_with("finance.sales-posting:"));
        instructions.validate_balanced()?;
        Ok(PostingReceipt {
            invoice_id: id(90),
            receivable_id: id(91),
            billing_journal_id: id(92),
            fulfillment_journal_id: id(93),
            revenue_schedule_id: id(94),
            posted_at: at(12),
        })
    }
}

#[tokio::test]
async fn quote_to_final_finance_posting_requires_every_human_gate() {
    let scope = AuthorityScope::Company {
        holding_id: id(1),
        company_id: id(2),
    };
    let tax = AuthoritativeTaxProfile::thailand_vat_7(id(10), 1).unwrap();
    let payload = QuoteDraftPayload {
        customer_id: id(11),
        customer_version: 1,
        currency: "THB".into(),
        payment_terms_days: 30,
        valid_until: at(86_400),
        language: QuoteDocumentLanguage::Thai,
        notes: "Human-editable Quote draft".into(),
        lines: vec![QuoteDraftLine {
            line_id: id(12),
            source: QuoteDraftLineSource::Custom {
                reason: "Customer-specific implementation service".into(),
            },
            item_code: "CUSTOM-IMPLEMENTATION".into(),
            description: "Implementation service".into(),
            quantity: 1,
            unit_price_minor: 100_000,
            discount_basis_points: 0,
            tax_profile: tax.clone(),
        }],
    };
    let mut quote = QuoteWorkflow::create(
        QuoteId::try_from_uuid(id(13)).unwrap(),
        scope.clone(),
        "QT-2026-001".into(),
        "sales-maker".into(),
        payload.clone(),
        std::slice::from_ref(&tax),
        at(0),
    )
    .unwrap();

    // Fixture output remains inert until the Sales user explicitly applies it.
    assert_eq!(quote.quote_version(), 1);
    quote
        .apply_suggestion(
            1,
            payload,
            std::slice::from_ref(&tax),
            id(14),
            "a".repeat(64),
            "b".repeat(64),
            "Fixture AI".into(),
            "sales-maker".into(),
            at(1),
        )
        .unwrap();
    assert_eq!(quote.quote_version(), 2);
    quote.submit_for_approval(2, at(2)).unwrap();

    let quote_ref = RecordRef::try_new(
        BoundedContext::Sales,
        "quote",
        quote.quote_id().as_uuid(),
        quote.quote_version(),
    )
    .unwrap();
    let mut approval = ApprovalSession::create(
        ApprovalSessionId::try_from_uuid(id(15)).unwrap(),
        scope.clone(),
        quote_ref.clone(),
        1,
        "sales-maker".into(),
        vec![
            ApprovalGateSpec {
                kind: ApprovalGateKind::SalesCommercial,
                assigned_to: Some("sales-approver".into()),
            },
            ApprovalGateSpec {
                kind: ApprovalGateKind::FinanceAccountingPlan,
                assigned_to: Some("finance-approver".into()),
            },
        ],
        at(3),
    )
    .unwrap();
    approval
        .decide(
            id(16),
            ApprovalGateKind::SalesCommercial,
            2,
            ApprovalGateStatus::Approved,
            human_decision(
                &scope,
                "sales-approver",
                approval_session_decision_ref(
                    id(16),
                    ApprovalSessionId::try_from_uuid(id(15)).unwrap(),
                    2,
                    ApprovalGateKind::SalesCommercial,
                    ApprovalGateStatus::Approved,
                ),
                at(4),
            ),
            "commercial_terms_ok".into(),
        )
        .unwrap();

    let finance_tax = FinanceTaxProfileSnapshot {
        profile_id: tax.profile_id,
        version: tax.version,
        classification: FinanceTaxClassification::StandardVat,
        rate_basis_points: tax.rate_basis_points,
        authority_ref: tax.authority_ref.clone(),
    };
    let roles = SalesAccountRoles {
        accounts_receivable: account(20),
        output_vat: account(21),
        contract_liability: account(22),
        revenue: account(23),
    };
    let obligation_id = id(24);
    let mut plan = AccountingPlan::draft(
        AccountingPlanId::try_from_uuid(id(25)).unwrap(),
        scope.clone(),
        quote_ref,
        "THB".into(),
        100_000,
        7_000,
        DimensionSet::for_legal_entity(organization::CompanyRef::try_from_uuid(id(2)).unwrap()),
        finance_tax.clone(),
        roles.clone(),
        vec![AccountingObligation {
            obligation_id,
            allocated_minor: 100_000,
            template_code: "BILLING_TH_VAT7_ACCEPTED_FULFILLMENT".into(),
        }],
        "finance-preparer".into(),
        "sales-approver".into(),
        None,
        at(5),
    )
    .unwrap();
    plan.submit(at(6)).unwrap();
    plan.approve("finance-approver".into(), at(7)).unwrap();
    approval
        .decide(
            id(17),
            ApprovalGateKind::FinanceAccountingPlan,
            2,
            ApprovalGateStatus::Approved,
            human_decision(
                &scope,
                "finance-approver",
                approval_session_decision_ref(
                    id(17),
                    ApprovalSessionId::try_from_uuid(id(15)).unwrap(),
                    2,
                    ApprovalGateKind::FinanceAccountingPlan,
                    ApprovalGateStatus::Approved,
                ),
                at(8),
            ),
            "tax_and_plan_ok".into(),
        )
        .unwrap();
    assert_eq!(approval.status(), ApprovalSessionStatus::Approved);

    quote
        .attach_accounting_plan(
            QuoteAccountingPlanEvidence {
                accounting_plan_id: plan.id().as_uuid(),
                accounting_plan_version: plan.version(),
                quote_version: 2,
                approved_by: "finance-approver".into(),
                approved_at: at(7),
            },
            at(9),
        )
        .unwrap();
    quote
        .approve(
            QuoteApprovalEvidence {
                approval_session_id: approval.id().as_uuid(),
                quote_version: 2,
                policy_version: 1,
                sales_approver: "sales-approver".into(),
                finance_approver: "finance-approver".into(),
                approved_at: at(8),
            },
            at(10),
        )
        .unwrap();
    quote
        .attach_pdf(
            QuotePdfArtifact {
                artifact_id: id(30),
                quote_version: 2,
                language: QuoteDocumentLanguage::Thai,
                object_key: "quotes/13/versions/2/digest/quote.pdf".into(),
                sha256_digest: "c".repeat(64),
                byte_length: 1_024,
                generated_at: at(11),
            },
            at(11),
        )
        .unwrap();
    quote
        .record_external_decision(
            ExternalQuoteDecision {
                decision: ExternalQuoteDecisionKind::Accepted,
                quote_version: 2,
                pdf_artifact_id: id(30),
                evidence_reference: "customer acceptance reference AC-001".into(),
                attachment_object_key: None,
                recorded_by: "sales-maker".into(),
                decided_at: at(12),
            },
            at(12),
        )
        .unwrap();
    assert_eq!(quote.status(), QuoteWorkflowStatus::CustomerAccepted);

    let actuals = PostingActuals {
        source_event_id: erpx_kernel::EventId::try_from_uuid(id(39)).unwrap(),
        source_aggregate_version: 6,
        billing_event_id: id(40),
        fulfillment_id: id(41),
        obligation_id,
        entry_date: NaiveDate::from_ymd_opt(2026, 8, 18).unwrap(),
        net_minor: 100_000,
        tax_minor: 7_000,
        fulfilled_minor: 100_000,
        currency: "THB".into(),
        tax_profile: finance_tax.clone(),
    };
    let mut packet = PostingPacket::draft_from_plan(
        PostingPacketId::try_from_uuid(id(42)).unwrap(),
        &plan,
        actuals,
        "finance-preparer".into(),
        at(13),
    )
    .unwrap();
    let active_accounts = BTreeSet::from([
        roles.accounts_receivable,
        roles.output_vat,
        roles.contract_liability,
        roles.revenue,
    ]);
    let validation = PostingValidationContext {
        period_open: true,
        active_accounts: &active_accounts,
        current_tax_profile: &finance_tax,
    };
    packet.submit(&validation, at(14)).unwrap();
    assert!(packet.posting_instructions().is_err());
    packet
        .approve("finance-approver".into(), &validation, at(15))
        .unwrap();

    let coordinator = FinalPostingCoordinator::new(FinanceOwner);
    let receipt = coordinator
        .post_approved_packet(&mut packet, at(16))
        .await
        .unwrap();
    assert_eq!(receipt.invoice_id, id(90));
    assert_eq!(packet.receipt(), Some(&receipt));

    // Exact replay returns the same owner IDs and does not post twice.
    let replay = coordinator
        .post_approved_packet(&mut packet, at(17))
        .await
        .unwrap();
    assert_eq!(replay, receipt);
    assert_eq!(tax.classification, TaxClassification::StandardVat);
}
