//! Native expense claim policy tests.
//!
//! These are owner unit facts: exact idempotency replay, claimant/version
//! checks, the retained v2 stage machine, accounting/posting gates, Treasury
//! payment preparation/confirmation/cancellation, reversal and the fail-closed
//! port defaults. PostgreSQL transaction behaviour and the composed
//! Organization/Workflow/Files/Finance ports are proven by the sibling
//! integration target.
use erpx_server_people_payroll::public::expense::{
    ExpenseAccountingInput, ExpenseAccountingWrite, ExpenseAction, ExpenseApprovalDecision,
    ExpenseApprovalInstance, ExpenseApprovalPort, ExpenseApprovalView, ExpenseCancelWrite,
    ExpenseClaimant, ExpenseCommandFuture, ExpenseCommandRepository, ExpenseDecisionInput,
    ExpenseDecisionInsert, ExpenseDraftInput, ExpenseEngagementScope, ExpenseEvidenceFacts,
    ExpenseEvidenceOwner, ExpenseEvidencePort, ExpenseEvidenceRemoval, ExpenseFileBinding,
    ExpenseInsert, ExpenseLedgerEntry, ExpensePayable, ExpensePaymentCancelWrite,
    ExpensePaymentCancellationInput, ExpensePaymentConfirmedWrite, ExpensePaymentDraft,
    ExpensePaymentDraftInsert, ExpensePaymentDraftWrite, ExpensePaymentInput,
    ExpensePaymentSettlement, ExpensePaymentSettlementRequest, ExpensePostWrite,
    ExpensePostingRequest, ExpensePostingSettlement, ExpenseReversalRequest,
    ExpenseReversalSettlement, ExpenseReverseWrite, ExpenseReviewWrite, ExpenseRoute,
    ExpenseRoutePort, ExpenseRouteStep, ExpenseRow, ExpenseSettlementPort, ExpenseSettlementWrite,
    ExpenseStaffFacts, ExpenseStaffPort, ExpenseSubmitWrite, ExpenseTransitionInput, ExpenseUpdate,
    ExpenseVersionBump, NoExpenseApprovalPort, NoExpenseEvidencePort, NoExpenseRoutePort,
    NoExpenseSettlementPort, NoExpenseStaffPort, StoredExpenseReceipt, cancel_expense,
    cancel_expense_payment, confirm_expense_payment, decide_expense, post_expense,
    prepare_expense_payment, remove_expense_attachment, reverse_expense, save_expense,
    save_expense_accounting, save_expense_payment, submit_expense,
};
use erpx_server_people_payroll::public::workforce::WorkforceCommandActor;
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, TimestampMillis,
};
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::collections::BTreeMap;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> WorkforceCommandActor {
    WorkforceCommandActor {
        user_id: "owner".into(),
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: "assignment".into(),
        role: "employee".into(),
        permissions: vec![
            "expense.prepare".into(),
            "expense.read".into(),
            "expense.approve".into(),
            "expense.pay".into(),
        ],
        work_context: "a".repeat(64),
        expires_at: NOW.get() + 100_000,
        beta: true,
    }
}

fn reviewer() -> WorkforceCommandActor {
    WorkforceCommandActor {
        user_id: "reviewer".into(),
        assignment_id: "reviewer-assignment".into(),
        permissions: vec!["expense.approve".into(), "expense.read".into()],
        ..actor()
    }
}

fn payer() -> WorkforceCommandActor {
    WorkforceCommandActor {
        user_id: "payer".into(),
        assignment_id: "payer-assignment".into(),
        permissions: vec!["expense.pay".into(), "expense.read".into()],
        ..actor()
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn row() -> ExpenseRow {
    ExpenseRow {
        id: "expense-1".into(),
        company_id: "company".into(),
        request_kind: "expense".into(),
        payee_party_id: None,
        employee_id: Some("person".into()),
        engagement_id: Some("engagement".into()),
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        claimant_user_id: Some("owner".into()),
        claimant_name: Some("Somchai Jaidee".into()),
        legal_name: Some("Somchai Jaidee".into()),
        expense_date: Some("2026-02-01".into()),
        merchant: Some("Bookshop".into()),
        currency: Some("THB".into()),
        amount_minor: Some(120_000),
        tax_minor: 0,
        category_account_id: Some("account".into()),
        receipt_reference: Some("RCPT-1".into()),
        business_purpose: "Team reference books".into(),
        prepared_by: Some("owner".into()),
        approved_by: None,
        status: "draft".into(),
        workflow_version: 2,
        workflow_stage: "draft".into(),
        accounting_json: json!({}),
        invoice_id: None,
        journal_id: None,
        rejection_reason: None,
        reversal_journal_id: None,
        cancelled_at: None,
        submitted_at: None,
        posted_at: None,
        reimbursed_at: None,
        version: 1,
        created_by: "owner".into(),
        created_at: NOW.get(),
        updated_at: NOW.get(),
    }
}

fn accounting_row(stage: &str, prepared_by: &str) -> ExpenseRow {
    ExpenseRow {
        status: "in_review".into(),
        workflow_stage: stage.into(),
        accounting_json: json!({
            "categoryAccountId": "account",
            "periodId": "period",
            "entryDate": "2026-02-03",
            "preparedBy": prepared_by,
            "note": ""
        }),
        ..row()
    }
}

fn posted_row() -> ExpenseRow {
    ExpenseRow {
        status: "posted".into(),
        workflow_stage: "payment".into(),
        invoice_id: Some("invoice".into()),
        journal_id: Some("journal".into()),
        version: 3,
        submitted_at: Some(1),
        posted_at: Some(2),
        ..row()
    }
}

fn staff() -> ExpenseStaffFacts {
    ExpenseStaffFacts {
        person_id: "person".into(),
        engagement_id: "engagement".into(),
        engagement_version: 1,
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        legal_name: "Somchai Jaidee".into(),
        worker_type: "employee".into(),
        level_active: true,
        start_date: "2026-01-01".into(),
        end_date: None,
    }
}

#[derive(Default)]
struct FakeRepo {
    receipts: BTreeMap<String, StoredExpenseReceipt>,
    ledger: Vec<String>,
    row: Option<ExpenseRow>,
    rows: Vec<ExpenseRow>,
    claimant: Option<ExpenseClaimant>,
    engagement: Option<ExpenseEngagementScope>,
    drafts: BTreeMap<String, ExpensePaymentDraft>,
    prepared_drafts: Vec<(String, String)>,
    create_calls: usize,
    update_calls: usize,
    submit_calls: usize,
    review_calls: usize,
    cancel_calls: usize,
    accounting_calls: usize,
    post_calls: usize,
    settle_calls: usize,
    reverse_calls: usize,
    decision_calls: usize,
    insert_draft_calls: usize,
    save_draft_calls: usize,
    confirm_draft_calls: usize,
    cancel_draft_calls: usize,
    bump_expense_calls: usize,
    bump_payment_calls: usize,
    decisions: Vec<(String, String)>,
}

impl FakeRepo {
    fn with_row(row: ExpenseRow) -> Self {
        Self {
            rows: vec![row.clone()],
            row: Some(row),
            ..Self::default()
        }
    }

    fn with_draft(mut self, draft: ExpensePaymentDraft) -> Self {
        self.drafts.insert(draft.id.clone(), draft);
        self
    }
}

impl ExpenseCommandRepository for FakeRepo {
    type Error = BusinessError;

    fn connection(&mut self) -> Option<&mut PgConnection> {
        None
    }
    fn lock_command<'a>(&'a mut self, _: &'a str) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async { Ok(()) })
    }
    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> ExpenseCommandFuture<'a, String, Self::Error> {
        Box::pin(async move {
            let mut hash = 0xcbf29ce484222325_u64;
            for byte in identity.bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
            Ok(format!("{hash:064x}"))
        })
    }
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<StoredExpenseReceipt>, Self::Error> {
        Box::pin(async move { Ok(self.receipts.get(key).cloned()) })
    }
    fn ledger<'a>(
        &'a mut self,
        entry: &'a ExpenseLedgerEntry<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ledger.push(entry.operation.to_owned());
            self.receipts.insert(
                entry.key.to_owned(),
                StoredExpenseReceipt {
                    request_hash: entry.request_hash.to_owned(),
                    result: Some(entry.result.clone()),
                },
            );
            Ok(())
        })
    }
    fn expense_rows<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: bool,
        _: Option<&'a str>,
        _: Option<&'a str>,
    ) -> ExpenseCommandFuture<'a, Vec<ExpenseRow>, Self::Error> {
        Box::pin(async move { Ok(self.rows.clone()) })
    }
    fn expense_row<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: bool,
        _: Option<&'a str>,
        _: Option<&'a str>,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseRow>, Self::Error> {
        Box::pin(async move { Ok(self.row.clone()) })
    }
    fn expense_row_for_update<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseRow>, Self::Error> {
        Box::pin(async move { Ok(self.row.clone()) })
    }
    fn claimant<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseClaimant>, Self::Error> {
        Box::pin(async move { Ok(self.claimant.clone()) })
    }
    fn latest_engagement<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseEngagementScope>, Self::Error> {
        Box::pin(async move { Ok(self.engagement.clone()) })
    }
    fn create_expense<'a>(
        &'a mut self,
        _: &'a ExpenseInsert<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.create_calls += 1;
            Ok(())
        })
    }
    fn update_expense<'a>(
        &'a mut self,
        _: &'a ExpenseUpdate<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.update_calls += 1;
            Ok(())
        })
    }
    fn submit_expense<'a>(
        &'a mut self,
        _: &'a ExpenseSubmitWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.submit_calls += 1;
            Ok(())
        })
    }
    fn review_expense<'a>(
        &'a mut self,
        _: &'a ExpenseReviewWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.review_calls += 1;
            Ok(())
        })
    }
    fn cancel_expense<'a>(
        &'a mut self,
        _: &'a ExpenseCancelWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.cancel_calls += 1;
            Ok(())
        })
    }
    fn save_accounting<'a>(
        &'a mut self,
        _: &'a ExpenseAccountingWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.accounting_calls += 1;
            Ok(())
        })
    }
    fn post_expense<'a>(
        &'a mut self,
        _: &'a ExpensePostWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.post_calls += 1;
            Ok(())
        })
    }
    fn settle_expense<'a>(
        &'a mut self,
        _: &'a ExpenseSettlementWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.settle_calls += 1;
            Ok(())
        })
    }
    fn reverse_expense<'a>(
        &'a mut self,
        _: &'a ExpenseReverseWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.reverse_calls += 1;
            Ok(())
        })
    }
    fn insert_decision<'a>(
        &'a mut self,
        command: &'a ExpenseDecisionInsert<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.decision_calls += 1;
            self.decisions
                .push((command.action.to_owned(), command.command_id.to_owned()));
            Ok(())
        })
    }
    fn payment_draft<'a>(
        &'a mut self,
        _: &'a CompanyId,
        id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpensePaymentDraft>, Self::Error> {
        Box::pin(async move { Ok(self.drafts.get(id).cloned()) })
    }
    fn payment_draft_for_update<'a>(
        &'a mut self,
        _: &'a CompanyId,
        id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpensePaymentDraft>, Self::Error> {
        Box::pin(async move { Ok(self.drafts.get(id).cloned()) })
    }
    fn payment_drafts<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> ExpenseCommandFuture<'a, Vec<ExpensePaymentDraft>, Self::Error> {
        Box::pin(async move { Ok(self.drafts.values().cloned().collect()) })
    }
    fn native_payment_prepared<'a>(
        &'a mut self,
        _: &'a CompanyId,
        expense_id: &'a str,
        draft_id: &'a str,
    ) -> ExpenseCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            Ok(self
                .prepared_drafts
                .iter()
                .any(|(expense, draft)| expense == expense_id && draft == draft_id))
        })
    }
    fn insert_payment_draft<'a>(
        &'a mut self,
        _: &'a ExpensePaymentDraftInsert<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.insert_draft_calls += 1;
            Ok(())
        })
    }
    fn save_payment_draft<'a>(
        &'a mut self,
        _: &'a ExpensePaymentDraftWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.save_draft_calls += 1;
            Ok(())
        })
    }
    fn confirm_payment_draft<'a>(
        &'a mut self,
        _: &'a ExpensePaymentConfirmedWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.confirm_draft_calls += 1;
            Ok(())
        })
    }
    fn cancel_payment_draft<'a>(
        &'a mut self,
        _: &'a ExpensePaymentCancelWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.cancel_draft_calls += 1;
            Ok(())
        })
    }
    fn bump_expense_version<'a>(
        &'a mut self,
        _: &'a ExpenseVersionBump<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.bump_expense_calls += 1;
            Ok(())
        })
    }
    fn bump_payment_version<'a>(
        &'a mut self,
        _: &'a ExpenseVersionBump<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.bump_payment_calls += 1;
            Ok(())
        })
    }
}

struct FakeStaff(Option<ExpenseStaffFacts>);

impl ExpenseStaffPort for FakeStaff {
    fn active_staff<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseStaffFacts>, BusinessError> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

struct FakeRoute(ExpenseRoute);

impl Default for FakeRoute {
    fn default() -> Self {
        Self(ExpenseRoute {
            steps: vec![ExpenseRouteStep {
                stage: "department_approval".into(),
            }],
            blocked_reason: None,
        })
    }
}

impl ExpenseRoutePort for FakeRoute {
    fn route<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a [String],
        _: &'a str,
    ) -> ExpenseCommandFuture<'a, ExpenseRoute, BusinessError> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

struct FakeApproval {
    final_decision: bool,
    next_stage: Option<String>,
    can_decide: bool,
    can_override: bool,
    payers: Vec<String>,
    opens: usize,
    decisions: usize,
    closes: usize,
}

impl Default for FakeApproval {
    fn default() -> Self {
        Self {
            final_decision: true,
            next_stage: Some("accounts_review".into()),
            can_decide: true,
            can_override: false,
            payers: vec!["payer".into()],
            opens: 0,
            decisions: 0,
            closes: 0,
        }
    }
}

impl ExpenseApprovalPort for FakeApproval {
    fn open<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a ExpenseRoute,
        _: i64,
    ) -> ExpenseCommandFuture<'a, ExpenseApprovalInstance, BusinessError> {
        Box::pin(async move {
            self.opens += 1;
            Ok(ExpenseApprovalInstance {
                instance_id: "approval".into(),
            })
        })
    }
    fn prepare_decision<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: bool,
        _: &'a str,
        _: i64,
        _: bool,
    ) -> ExpenseCommandFuture<'a, ExpenseApprovalDecision, BusinessError> {
        Box::pin(async move {
            Ok(ExpenseApprovalDecision {
                instance_id: "approval".into(),
                final_decision: self.final_decision,
                next_stage: self.next_stage.clone(),
            })
        })
    }
    fn apply_decision<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: bool,
        _: &'a str,
        _: i64,
        _: bool,
    ) -> ExpenseCommandFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.decisions += 1;
            Ok(())
        })
    }
    fn close<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: i64,
    ) -> ExpenseCommandFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.closes += 1;
            Ok(())
        })
    }
    fn view<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a WorkforceCommandActor,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseApprovalView>, BusinessError> {
        Box::pin(async move {
            Ok(Some(ExpenseApprovalView {
                can_view: self.can_decide,
                can_decide: self.can_decide,
                can_override: self.can_override,
                blocked_reason: None,
            }))
        })
    }
    fn payers<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
    ) -> ExpenseCommandFuture<'a, Vec<String>, BusinessError> {
        Box::pin(async move { Ok(self.payers.clone()) })
    }
}

struct FakeEvidence {
    facts: ExpenseEvidenceFacts,
    bindings: BTreeMap<(String, String), ExpenseFileBinding>,
    removals: usize,
}

impl Default for FakeEvidence {
    fn default() -> Self {
        Self {
            facts: ExpenseEvidenceFacts {
                claim_files: 1,
                claim_pending: 0,
                payment_slips: 1,
                payment_pending: 0,
            },
            bindings: BTreeMap::new(),
            removals: 0,
        }
    }
}

impl ExpenseEvidencePort for FakeEvidence {
    fn facts<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
    ) -> ExpenseCommandFuture<'a, ExpenseEvidenceFacts, BusinessError> {
        Box::pin(async move { Ok(self.facts.clone()) })
    }
    fn binding<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        owner: ExpenseEvidenceOwner<'a>,
        attachment_id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseFileBinding>, BusinessError> {
        Box::pin(async move {
            let owner = match owner {
                ExpenseEvidenceOwner::Claim(id) => format!("claim:{id}"),
                ExpenseEvidenceOwner::Payment(id) => format!("payment:{id}"),
            };
            Ok(self
                .bindings
                .get(&(owner, attachment_id.to_owned()))
                .cloned())
        })
    }
    fn remove<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a ExpenseEvidenceRemoval<'a>,
    ) -> ExpenseCommandFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.removals += 1;
            Ok(())
        })
    }
}

struct FakeSettlement {
    payable: Option<ExpensePayable>,
    outstanding: i64,
    posts: usize,
    confirms: usize,
    reverses: usize,
}

impl Default for FakeSettlement {
    fn default() -> Self {
        Self {
            payable: Some(ExpensePayable {
                outstanding_minor: 120_000,
                total_minor: 120_000,
            }),
            outstanding: 0,
            posts: 0,
            confirms: 0,
            reverses: 0,
        }
    }
}

impl ExpenseSettlementPort for FakeSettlement {
    fn payable<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpensePayable>, BusinessError> {
        Box::pin(async move { Ok(self.payable.clone()) })
    }
    fn post<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a ExpensePostingRequest<'a>,
    ) -> ExpenseCommandFuture<'a, ExpensePostingSettlement, BusinessError> {
        Box::pin(async move {
            self.posts += 1;
            Ok(ExpensePostingSettlement {
                journal_id: "journal-posted".into(),
                invoice_id: "invoice-posted".into(),
            })
        })
    }
    fn confirm_payment<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a ExpensePaymentSettlementRequest<'a>,
    ) -> ExpenseCommandFuture<'a, ExpensePaymentSettlement, BusinessError> {
        Box::pin(async move {
            self.confirms += 1;
            Ok(ExpensePaymentSettlement {
                payment_id: "payment-1".into(),
                outstanding_minor: self.outstanding,
            })
        })
    }
    fn reverse<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a ExpenseReversalRequest<'a>,
    ) -> ExpenseCommandFuture<'a, ExpenseReversalSettlement, BusinessError> {
        Box::pin(async move {
            self.reverses += 1;
            Ok(ExpenseReversalSettlement {
                reversal_journal_id: "journal-reversal".into(),
            })
        })
    }
}

fn draft_body() -> Value {
    json!({
        "expenseDate": "2026-02-01",
        "merchant": "Bookshop",
        "receiptReference": "RCPT-1",
        "businessPurpose": "Team reference books",
        "currency": "THB",
        "amount": "1200.00",
        "tax": "0.00",
        "categoryAccountId": "account"
    })
}

fn submit_body() -> Value {
    json!({"action": "submit", "expectedVersion": 1})
}

fn decision_body(reason: &str) -> Value {
    json!({"expectedVersion": 1, "reason": reason})
}

fn payment_draft(id: &str, version: i64) -> ExpensePaymentDraft {
    ExpensePaymentDraft {
        id: id.into(),
        expense_id: "expense-1".into(),
        company_id: "company".into(),
        created_by: "payer".into(),
        payment_date: None,
        amount_minor: None,
        reference: None,
        status: "draft".into(),
        payment_id: None,
        version,
        created_at: 0,
        updated_at: 0,
    }
}

#[tokio::test]
async fn create_is_idempotent_and_writes_once() {
    let mut repo = FakeRepo::default();
    let first = save_expense(
        &actor(),
        None,
        &draft_body(),
        &key("expense-create-1"),
        &mut repo,
        &mut FakeStaff(Some(staff())),
        NOW,
    )
    .await
    .unwrap();
    let second = save_expense(
        &actor(),
        None,
        &draft_body(),
        &key("expense-create-1"),
        &mut repo,
        &mut FakeStaff(Some(staff())),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(first.id, "expense-expense-create-1");
    assert_eq!(first.status, "draft");
    assert_eq!(first.version, 1);
    assert_eq!(second.id, first.id);
    assert_eq!(repo.create_calls, 1);
    assert_eq!(repo.ledger, vec!["expense.create"]);
}

#[tokio::test]
async fn update_requires_owner_version_and_editable_claim() {
    let mut repo = FakeRepo::with_row(row());
    let body = json!({
        "expectedVersion": 1,
        "expenseDate": "2026-02-02",
        "merchant": "Bookshop",
        "receiptReference": "RCPT-2",
        "businessPurpose": "Team reference books",
        "currency": "THB",
        "amount": "1000.00",
        "tax": "0.00",
        "categoryAccountId": "account"
    });
    let result = save_expense(
        &actor(),
        Some("expense-1"),
        &body,
        &key("expense-update-1"),
        &mut repo,
        &mut FakeStaff(Some(staff())),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.version, 2);
    assert_eq!(repo.update_calls, 1);

    let stale = json!({"expectedVersion": 3, "amount": "1000.00"});
    let error = save_expense(
        &actor(),
        Some("expense-1"),
        &stale,
        &key("expense-update-2"),
        &mut repo,
        &mut FakeStaff(Some(staff())),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "EXPENSE_CHANGED");

    let foreign = WorkforceCommandActor {
        user_id: "other".into(),
        ..actor()
    };
    let error = save_expense(
        &foreign,
        Some("expense-1"),
        &body,
        &key("expense-update-3"),
        &mut repo,
        &mut FakeStaff(Some(staff())),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "EXPENSE_LOCKED");

    let mut posted = FakeRepo::with_row(posted_row());
    let error = save_expense(
        &actor(),
        Some("expense-1"),
        &body,
        &key("expense-update-4"),
        &mut posted,
        &mut FakeStaff(Some(staff())),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "EXPENSE_LOCKED");
}

#[tokio::test]
async fn submit_requires_owner_setup_receipt_and_route() {
    let mut repo = FakeRepo::with_row(row());
    let mut approval = FakeApproval::default();
    let result = submit_expense(
        &actor(),
        "expense-1",
        &submit_body(),
        &key("expense-submit-1"),
        &mut repo,
        &mut FakeStaff(Some(staff())),
        &mut FakeRoute::default(),
        &mut approval,
        &mut FakeEvidence::default(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.status, "in_review");
    assert_eq!(result.stage.as_deref(), Some("department_approval"));
    assert_eq!(result.version, 2);
    assert_eq!(repo.submit_calls, 1);
    assert_eq!(approval.opens, 1);

    let mut repo = FakeRepo::with_row(row());
    let mut evidence = FakeEvidence::default();
    evidence.facts.claim_files = 0;
    let error = submit_expense(
        &actor(),
        "expense-1",
        &submit_body(),
        &key("expense-submit-2"),
        &mut repo,
        &mut FakeStaff(Some(staff())),
        &mut FakeRoute::default(),
        &mut FakeApproval::default(),
        &mut evidence,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "EXPENSE_SETUP_REQUIRED");

    let mut repo = FakeRepo::with_row(row());
    let blocked = FakeRoute(ExpenseRoute {
        steps: vec![ExpenseRouteStep {
            stage: "department_approval".into(),
        }],
        blocked_reason: Some("No independent reviewer".into()),
    });
    let error = submit_expense(
        &actor(),
        "expense-1",
        &submit_body(),
        &key("expense-submit-3"),
        &mut repo,
        &mut FakeStaff(Some(staff())),
        &mut { blocked },
        &mut FakeApproval::default(),
        &mut FakeEvidence::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "EXPENSE_ROUTE_REQUIRED");

    let mut foreign_row = row();
    foreign_row.claimant_user_id = Some("other".into());
    foreign_row.created_by = "other".into();
    let mut repo = FakeRepo::with_row(foreign_row);
    let error = submit_expense(
        &actor(),
        "expense-1",
        &submit_body(),
        &key("expense-submit-4"),
        &mut repo,
        &mut FakeStaff(Some(staff())),
        &mut FakeRoute::default(),
        &mut FakeApproval::default(),
        &mut FakeEvidence::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "EXPENSE_OWNER_REQUIRED");
}

#[tokio::test]
async fn approve_advances_stages_and_records_each_decision() {
    let mut repo = FakeRepo::with_row(accounting_row("accounts_review", "accountant"));
    let mut approval = FakeApproval {
        next_stage: Some("accounts_approval".into()),
        ..FakeApproval::default()
    };
    let result = decide_expense(
        &reviewer(),
        "expense-1",
        &json!({"expectedVersion": 1, "reason": "Reviewed"}),
        &key("expense-approve-1"),
        true,
        &mut repo,
        &mut approval,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.status, "in_review");
    assert_eq!(result.stage.as_deref(), Some("accounts_approval"));
    assert_eq!(result.version, 2);
    assert_eq!(approval.decisions, 1);
    assert_eq!(repo.review_calls, 1);
    assert_eq!(
        repo.decisions,
        vec![("approve".to_owned(), "expense-approve-1".to_owned())]
    );

    let mut repo = FakeRepo::with_row(accounting_row("accounts_approval", "accountant"));
    let result = decide_expense(
        &reviewer(),
        "expense-1",
        &json!({"expectedVersion": 1, "reason": "Accounts approved"}),
        &key("expense-approve-2"),
        true,
        &mut repo,
        &mut FakeApproval::default(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.status, "approved");
    assert_eq!(result.stage.as_deref(), Some("gl_posting"));

    let mut prepared_by_self = FakeRepo::with_row(accounting_row("accounts_approval", "reviewer"));
    let error = decide_expense(
        &reviewer(),
        "expense-1",
        &json!({"expectedVersion": 1, "reason": "Accounts approved"}),
        &key("expense-approve-3"),
        true,
        &mut prepared_by_self,
        &mut FakeApproval::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "INDEPENDENT_MANAGER_REQUIRED");

    let mut pending = FakeRepo::with_row(ExpenseRow {
        status: "in_review".into(),
        ..row()
    });
    let error = decide_expense(
        &reviewer(),
        "expense-1",
        &decision_body("x"),
        &key("expense-reject-1"),
        false,
        &mut pending,
        &mut FakeApproval::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "REASON_REQUIRED");
}

#[tokio::test]
async fn reject_and_gl_return_move_the_claim_back_to_draft() {
    let mut repo = FakeRepo::with_row(ExpenseRow {
        status: "in_review".into(),
        workflow_stage: "department_approval".into(),
        ..row()
    });
    let mut approval = FakeApproval {
        final_decision: false,
        ..FakeApproval::default()
    };
    let result = decide_expense(
        &reviewer(),
        "expense-1",
        &json!({"expectedVersion": 1, "reason": "Missing insight"}),
        &key("expense-reject-2"),
        false,
        &mut repo,
        &mut approval,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.status, "rejected");
    assert_eq!(result.stage.as_deref(), Some("draft"));
    assert_eq!(repo.decisions[0].0, "reject");

    let mut repo = FakeRepo::with_row(ExpenseRow {
        status: "approved".into(),
        workflow_stage: "gl_posting".into(),
        accounting_json: json!({"periodId": "period", "entryDate": "2026-02-03"}),
        ..row()
    });
    let mut approval = FakeApproval::default();
    let result = decide_expense(
        &reviewer(),
        "expense-1",
        &json!({"expectedVersion": 1, "reason": "Wrong account"}),
        &key("expense-gl-return-1"),
        false,
        &mut repo,
        &mut approval,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.status, "rejected");
    assert_eq!(result.stage.as_deref(), Some("draft"));
    assert_eq!(approval.decisions, 0);
    assert_eq!(repo.review_calls, 1);
    assert_eq!(repo.decision_calls, 0);
}

#[tokio::test]
async fn cancel_is_owner_only_and_closes_the_approval() {
    let mut repo = FakeRepo::with_row(row());
    let mut approval = FakeApproval::default();
    let result = cancel_expense(
        &actor(),
        "expense-1",
        &json!({"action": "cancel", "expectedVersion": 1, "reason": "No longer needed"}),
        &key("expense-cancel-1"),
        &mut repo,
        &mut approval,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.status, "rejected");
    assert_eq!(result.stage.as_deref(), Some("cancelled"));
    assert_eq!(approval.closes, 1);
    assert_eq!(repo.cancel_calls, 1);
    assert_eq!(repo.decisions[0].0, "cancel");

    let mut repo = FakeRepo::with_row(row());
    let intruder = WorkforceCommandActor {
        user_id: "intruder".into(),
        ..actor()
    };
    let error = cancel_expense(
        &intruder,
        "expense-1",
        &json!({"action": "cancel", "expectedVersion": 1, "reason": "No longer needed"}),
        &key("expense-cancel-2"),
        &mut repo,
        &mut FakeApproval::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "EXPENSE_OWNER_REQUIRED");
}

#[tokio::test]
async fn accounting_requires_the_current_queue_and_a_valid_tax() {
    let mut repo = FakeRepo::with_row(accounting_row("accounts_review", "accountant"));
    let body = json!({
        "expectedVersion": 1,
        "categoryAccountId": "account",
        "tax": "1200.00",
        "entryDate": "2026-02-03",
        "periodId": "period",
        "note": ""
    });
    let error = save_expense_accounting(
        &actor(),
        "expense-1",
        &body,
        &key("expense-accounting-1"),
        &mut repo,
        &mut FakeApproval {
            can_decide: false,
            ..FakeApproval::default()
        },
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "ACCOUNTS_REVIEW_REQUIRED");

    let error = save_expense_accounting(
        &actor(),
        "expense-1",
        &body,
        &key("expense-accounting-2"),
        &mut repo,
        &mut FakeApproval::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "TAX_INVALID");

    let valid = json!({
        "expectedVersion": 1,
        "categoryAccountId": "account",
        "tax": "70.00",
        "entryDate": "2026-02-03",
        "periodId": "period",
        "note": ""
    });
    let result = save_expense_accounting(
        &actor(),
        "expense-1",
        &valid,
        &key("expense-accounting-3"),
        &mut repo,
        &mut FakeApproval::default(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.version, 2);
    assert_eq!(repo.accounting_calls, 1);
}

#[tokio::test]
async fn posting_requires_approved_accounting_independence_and_receipt() {
    let mut repo = FakeRepo::with_row(ExpenseRow {
        status: "approved".into(),
        workflow_stage: "gl_posting".into(),
        accounting_json: json!({
            "periodId": "period",
            "entryDate": "2026-02-03",
            "preparedBy": "accountant"
        }),
        ..row()
    });
    let mut settlement = FakeSettlement::default();
    let mut evidence = FakeEvidence::default();
    let result = post_expense(
        &reviewer(),
        "expense-1",
        &json!({"expectedVersion": 1, "periodId": "period", "entryDate": "2026-02-03"}),
        &key("expense-post-1"),
        &mut repo,
        &mut evidence,
        &mut settlement,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.status, "posted");
    assert_eq!(result.stage.as_deref(), Some("payment"));
    assert_eq!(repo.post_calls, 1);
    assert_eq!(settlement.posts, 1);

    let mut repo = FakeRepo::with_row(ExpenseRow {
        status: "approved".into(),
        workflow_stage: "gl_posting".into(),
        accounting_json: json!({
            "periodId": "period",
            "entryDate": "2026-02-03",
            "preparedBy": "accountant"
        }),
        prepared_by: Some("reviewer".into()),
        ..row()
    });
    let error = post_expense(
        &reviewer(),
        "expense-1",
        &json!({"expectedVersion": 1, "periodId": "period", "entryDate": "2026-02-03"}),
        &key("expense-post-2"),
        &mut repo,
        &mut FakeEvidence::default(),
        &mut FakeSettlement::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SELF_POST_FORBIDDEN");

    let mut repo = FakeRepo::with_row(ExpenseRow {
        status: "approved".into(),
        workflow_stage: "gl_posting".into(),
        accounting_json: json!({
            "periodId": "period",
            "entryDate": "2026-02-03",
            "preparedBy": "accountant"
        }),
        ..row()
    });
    let mut evidence = FakeEvidence::default();
    evidence.facts.claim_files = 0;
    let error = post_expense(
        &reviewer(),
        "expense-1",
        &json!({"expectedVersion": 1, "periodId": "period", "entryDate": "2026-02-03"}),
        &key("expense-post-3"),
        &mut repo,
        &mut evidence,
        &mut FakeSettlement::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "EXPENSE_RECEIPT_REQUIRED");
}

#[tokio::test]
async fn payment_prepare_save_confirm_and_cancel_follow_treasury_policy() {
    let mut repo = FakeRepo::with_row(posted_row());
    let mut approval = FakeApproval::default();
    let prepared = prepare_expense_payment(
        &payer(),
        "expense-1",
        &json!({"expectedVersion": 3}),
        &key("expense-payment-1"),
        &mut repo,
        &mut approval,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(prepared.draft_id, "expense-payment-expense-payment-1");
    assert_eq!(repo.insert_draft_calls, 1);

    let draft_id = "expense-payment-expense-payment-1";
    let mut repo = FakeRepo::with_row(posted_row()).with_draft(payment_draft(draft_id, 1));
    let saved = save_expense_payment(
        &payer(),
        "expense-1",
        draft_id,
        &json!({"expectedVersion": 1, "date": "2026-02-04", "amount": "500.00", "reference": "TRX-1"}),
        &key("expense-payment-2"),
        &mut repo,
        &mut FakeApproval::default(),
        &mut FakeEvidence::default(),
        &mut FakeSettlement::default(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(saved.version, 2);
    assert_eq!(repo.save_draft_calls, 1);

    let mut repo = FakeRepo::with_row(posted_row()).with_draft(payment_draft(draft_id, 1));
    let error = save_expense_payment(
        &payer(),
        "expense-1",
        draft_id,
        &json!({"expectedVersion": 1, "date": "2026-02-04", "amount": "1300.00", "reference": "TRX-1"}),
        &key("expense-payment-3"),
        &mut repo,
        &mut FakeApproval::default(),
        &mut FakeEvidence::default(),
        &mut FakeSettlement::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PAYMENT_AMOUNT_INVALID");

    let mut repo = FakeRepo::with_row(posted_row()).with_draft(payment_draft(draft_id, 1));
    let mut settlement = FakeSettlement {
        outstanding: 0,
        ..FakeSettlement::default()
    };
    let confirmed = confirm_expense_payment(
        &payer(),
        draft_id,
        &json!({"expectedVersion": 1, "date": "2026-02-04", "amount": "1200.00", "reference": "TRX-1"}),
        &key("expense-payment-4"),
        &mut repo,
        &mut FakeApproval::default(),
        &mut FakeEvidence::default(),
        &mut settlement,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(confirmed.payment_id.as_deref(), Some("payment-1"));
    assert_eq!(confirmed.outstanding_minor, Some(0));
    assert_eq!(repo.confirm_draft_calls, 1);
    assert_eq!(repo.settle_calls, 1);
    assert_eq!(settlement.confirms, 1);

    let mut repo = FakeRepo::with_row(posted_row()).with_draft(payment_draft(draft_id, 1));
    let mut evidence = FakeEvidence::default();
    evidence.facts.payment_slips = 0;
    let error = confirm_expense_payment(
        &payer(),
        draft_id,
        &json!({"expectedVersion": 1, "date": "2026-02-04", "amount": "1200.00", "reference": "TRX-1"}),
        &key("expense-payment-5"),
        &mut repo,
        &mut FakeApproval::default(),
        &mut evidence,
        &mut FakeSettlement::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PAYMENT_SLIP_REQUIRED");

    let mut repo = FakeRepo::with_row(posted_row()).with_draft(payment_draft(draft_id, 1));
    repo.prepared_drafts
        .push(("expense-1".into(), draft_id.into()));
    let cancelled = cancel_expense_payment(
        &payer(),
        "expense-1",
        draft_id,
        &json!({
            "expectedVersion": 1,
            "expenseVersion": 3,
            "reason": "Bank details invalid",
            "paymentNotExecuted": true
        }),
        &key("expense-payment-6"),
        &mut repo,
        &mut FakeApproval::default(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(cancelled.version, 2);
    assert_eq!(repo.cancel_draft_calls, 1);

    let mut repo = FakeRepo::with_row(posted_row()).with_draft(payment_draft(draft_id, 1));
    let error = cancel_expense_payment(
        &payer(),
        "expense-1",
        draft_id,
        &json!({
            "expectedVersion": 1,
            "expenseVersion": 3,
            "reason": "Bank details invalid",
            "paymentNotExecuted": true
        }),
        &key("expense-payment-7"),
        &mut repo,
        &mut FakeApproval::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "EXPENSE_PAYMENT_NOT_CANCELLABLE");
}

#[tokio::test]
async fn reversal_requires_a_holding_finance_approver_and_no_settlement() {
    let mut repo = FakeRepo::with_row(posted_row());
    let mut settlement = FakeSettlement::default();
    let result = reverse_expense(
        &reviewer(),
        "expense-1",
        &json!({
            "expectedVersion": 3,
            "periodId": "period",
            "entryDate": "2026-02-04",
            "reason": "Duplicate claim"
        }),
        &key("expense-reverse-1"),
        &mut repo,
        &mut settlement,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.status, "rejected");
    assert_eq!(repo.reverse_calls, 1);
    assert_eq!(settlement.reverses, 1);
    assert_eq!(repo.decisions[0].0, "cancel");

    let mut repo = FakeRepo::with_row(posted_row());
    let mut partially_paid = FakeSettlement {
        payable: Some(ExpensePayable {
            outstanding_minor: 60_000,
            total_minor: 120_000,
        }),
        ..FakeSettlement::default()
    };
    let error = reverse_expense(
        &reviewer(),
        "expense-1",
        &json!({
            "expectedVersion": 3,
            "periodId": "period",
            "entryDate": "2026-02-04",
            "reason": "Duplicate claim"
        }),
        &key("expense-reverse-2"),
        &mut repo,
        &mut partially_paid,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "EXPENSE_HAS_SETTLEMENT");

    let mut branch_actor = reviewer();
    branch_actor.branch_id = Some(erpx_server_shared::public::RecordId::new("branch").unwrap());
    let mut repo = FakeRepo::with_row(posted_row());
    let error = reverse_expense(
        &branch_actor,
        "expense-1",
        &json!({
            "expectedVersion": 3,
            "periodId": "period",
            "entryDate": "2026-02-04",
            "reason": "Duplicate claim"
        }),
        &key("expense-reverse-3"),
        &mut repo,
        &mut FakeSettlement::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "FINANCE_APPROVER_REQUIRED");
}

#[tokio::test]
async fn receipt_removal_requires_a_draft_binding_on_an_editable_claim() {
    let mut repo = FakeRepo::with_row(row());
    let mut evidence = FakeEvidence::default();
    evidence.bindings.insert(
        ("claim:expense-1".into(), "attachment-1".into()),
        ExpenseFileBinding {
            link_id: "link-1".into(),
            set_id: "set-1".into(),
            state: "draft".into(),
        },
    );
    let result = remove_expense_attachment(
        &actor(),
        "expense-1",
        &json!({"expectedVersion": 1, "attachmentId": "attachment-1"}),
        &key("expense-receipt-1"),
        &mut repo,
        &mut evidence,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.version, 2);
    assert_eq!(evidence.removals, 1);
    assert_eq!(repo.bump_expense_calls, 1);

    let mut evidence = FakeEvidence::default();
    evidence.bindings.insert(
        ("claim:expense-1".into(), "attachment-1".into()),
        ExpenseFileBinding {
            link_id: "link-1".into(),
            set_id: "set-1".into(),
            state: "bound".into(),
        },
    );
    let error = remove_expense_attachment(
        &actor(),
        "expense-1",
        &json!({"expectedVersion": 1, "attachmentId": "attachment-1"}),
        &key("expense-receipt-2"),
        &mut FakeRepo::with_row(row()),
        &mut evidence,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "BOUND_ATTACHMENT_IMMUTABLE");

    let error = remove_expense_attachment(
        &actor(),
        "expense-1",
        &json!({"expectedVersion": 1, "attachmentId": "missing"}),
        &key("expense-receipt-3"),
        &mut FakeRepo::with_row(row()),
        &mut FakeEvidence::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "ATTACHMENT_NOT_FOUND");
}

#[tokio::test]
async fn default_ports_fail_closed() {
    let mut repo = FakeRepo::with_row(row());
    let error = submit_expense(
        &actor(),
        "expense-1",
        &submit_body(),
        &key("expense-closed-1"),
        &mut repo,
        &mut NoExpenseStaffPort,
        &mut FakeRoute::default(),
        &mut FakeApproval::default(),
        &mut FakeEvidence::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "EXPENSE_STAFF_PORT_UNAVAILABLE");

    let mut repo = FakeRepo::with_row(row());
    let error = submit_expense(
        &actor(),
        "expense-1",
        &submit_body(),
        &key("expense-closed-2"),
        &mut repo,
        &mut FakeStaff(Some(staff())),
        &mut NoExpenseRoutePort,
        &mut FakeApproval::default(),
        &mut FakeEvidence::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "EXPENSE_ROUTE_PORT_UNAVAILABLE");

    let mut repo = FakeRepo::with_row(ExpenseRow {
        status: "in_review".into(),
        ..row()
    });
    let error = decide_expense(
        &reviewer(),
        "expense-1",
        &json!({"expectedVersion": 1, "reason": "Reviewed"}),
        &key("expense-closed-3"),
        true,
        &mut repo,
        &mut NoExpenseApprovalPort,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "EXPENSE_APPROVAL_PORT_UNAVAILABLE");

    let mut repo = FakeRepo::with_row(posted_row());
    let error = prepare_expense_payment(
        &payer(),
        "expense-1",
        &json!({"expectedVersion": 3}),
        &key("expense-closed-4"),
        &mut repo,
        &mut NoExpenseApprovalPort,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "EXPENSE_APPROVAL_PORT_UNAVAILABLE");

    let mut repo = FakeRepo::with_row(ExpenseRow {
        status: "approved".into(),
        workflow_stage: "gl_posting".into(),
        accounting_json: json!({
            "periodId": "period",
            "entryDate": "2026-02-03",
            "preparedBy": "accountant"
        }),
        ..row()
    });
    let error = post_expense(
        &reviewer(),
        "expense-1",
        &json!({"expectedVersion": 1, "periodId": "period", "entryDate": "2026-02-03"}),
        &key("expense-closed-5"),
        &mut repo,
        &mut NoExpenseEvidencePort,
        &mut FakeSettlement::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "EXPENSE_EVIDENCE_PORT_UNAVAILABLE");

    let mut repo = FakeRepo::with_row(ExpenseRow {
        status: "approved".into(),
        workflow_stage: "gl_posting".into(),
        accounting_json: json!({
            "periodId": "period",
            "entryDate": "2026-02-03",
            "preparedBy": "accountant"
        }),
        ..row()
    });
    let error = post_expense(
        &reviewer(),
        "expense-1",
        &json!({"expectedVersion": 1, "periodId": "period", "entryDate": "2026-02-03"}),
        &key("expense-closed-6"),
        &mut repo,
        &mut FakeEvidence::default(),
        &mut NoExpenseSettlementPort,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "EXPENSE_SETTLEMENT_PORT_UNAVAILABLE");
}

#[tokio::test]
async fn input_shapes_are_strict() {
    assert!(ExpenseDraftInput::parse(&json!({"amount": "12.345", "currency": "THB"})).is_err());
    assert!(ExpenseDraftInput::parse(&json!({"amount": "-1.00"})).is_err());
    assert!(ExpenseDraftInput::parse(&json!({"currency": "thb"})).is_err());
    assert!(
        ExpenseDraftInput::parse(&json!({
            "expenseDate": "2026-02-30",
            "currency": "THB"
        }))
        .is_err()
    );
    assert!(
        ExpenseTransitionInput::parse(&json!({"action": "pay", "expectedVersion": 1})).is_err()
    );
    assert!(ExpenseDecisionInput::parse(&json!({"expectedVersion": 0})).is_err());
    assert!(
        ExpensePaymentInput::parse(&json!({
            "expectedVersion": 1,
            "date": "2026-02-04",
            "amount": "10.00",
            "reference": "x"
        }))
        .is_err()
    );
    assert!(
        ExpensePaymentCancellationInput::parse(&json!({
            "expectedVersion": 1,
            "expenseVersion": 1,
            "reason": "Bank details invalid",
            "paymentNotExecuted": false
        }))
        .is_err()
    );
    assert!(ExpenseAccountingInput::parse(&json!({"expectedVersion": 1})).is_err());
    assert_eq!(ExpenseAction::Submit, ExpenseAction::Submit,);
    assert_eq!(
        BusinessError::new(FailureKind::Conflict, "X", "Y").code,
        "X"
    );
}
