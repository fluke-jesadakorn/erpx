//! Native Billing invoice lifecycle policy, CAS, replay, transitions and
//! bounded-input tests. These exercise the owner application with a repository
//! probe; PostgreSQL behavior is covered by the dedicated Postgres target.
use erpx_server_billing::public::cost::{AuditEntry, ReceiptSnapshot};
use erpx_server_billing::public::invoice::BillingInvoiceRepository;
use erpx_server_billing::public::invoice::{
    BillingAccountFacts, BillingCostFacts, BillingInvoiceCommand, BillingInvoiceRecord,
    BillingPeriodFacts, CreateInvoice, FinalizeInvoice, InvoiceLine, UpdateInvoice,
    ValidatedInvoiceWrite, VoidInvoice, execute_invoice_command,
};
use erpx_server_billing::public::workspace::{BillingActor, BillingFuture};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, FailureKind, HoldingId, RecordId, TimestampMillis,
    UserId,
};
use sqlx::test_block_on;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const KEY: &str = "billing-invoice-key-0001";
const PERIOD_START: i64 = 1_778_000_000_000;
const PERIOD_END: i64 = 1_780_000_000_000;
const DUE: i64 = 1_800_604_800_000;

fn target() -> HoldingId {
    HoldingId::new("h001").unwrap()
}

fn admin() -> BillingActor {
    BillingActor {
        user_id: UserId::new("admin").unwrap(),
        assignment_id: AssignmentId::new("assignment").unwrap(),
        holding_id: None,
        platform: true,
        role: "platform_admin".into(),
        expires_at: None,
    }
}

fn owner() -> BillingActor {
    BillingActor {
        user_id: UserId::new("owner").unwrap(),
        assignment_id: AssignmentId::new("assignment").unwrap(),
        holding_id: Some(target()),
        platform: false,
        role: "holding_owner".into(),
        expires_at: None,
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn period() -> BillingPeriodFacts {
    BillingPeriodFacts {
        id: "period-10".into(),
        start_at: PERIOD_START,
        end_at: PERIOD_END,
        plan_name: "Standard Support".into(),
        plan_revision: 1,
        support_satang: 50_000,
        costs_complete: true,
        closed_at: Some(PERIOD_END),
    }
}

fn account() -> BillingAccountFacts {
    BillingAccountFacts {
        version: 4,
        payment_mode: "manual".into(),
        consent: false,
        card: false,
    }
}

fn cost(id: &str, amount: i64) -> BillingCostFacts {
    BillingCostFacts {
        id: id.into(),
        period_id: "period-10".into(),
        category: "hosting".into(),
        amount_satang: amount,
        service_start: PERIOD_START,
        service_end: PERIOD_END,
        note: format!("Provider {id}"),
        simulated: false,
        invoice_id: None,
    }
}

fn costs() -> Vec<BillingCostFacts> {
    vec![cost("cost-1", 12_500), cost("cost-2", 5_000)]
}

fn previous_main() -> BillingInvoiceRecord {
    BillingInvoiceRecord {
        id: "invoice-1".into(),
        period_id: "period-10".into(),
        kind: "main".into(),
        status: "open".into(),
        total_satang: 62_500,
        lines_json: "[]".into(),
        payment_mode: "manual".into(),
        stripe_id: Some("in_test".into()),
        due_at: Some(1_790_000_000_000),
        payment_state: Some("unpaid".into()),
        original_invoice_id: None,
        created_at: 1_779_000_000_000,
        updated_at: 1_779_000_000_000,
    }
}

fn draft_invoice() -> BillingInvoiceRecord {
    BillingInvoiceRecord {
        id: format!("billing-invoice-{KEY}"),
        period_id: "period-10".into(),
        kind: "main".into(),
        status: "draft".into(),
        total_satang: 67_500,
        lines_json: serde_json::to_string(&vec![
            InvoiceLine {
                description: "Sandbox Standard Support · revision 1".into(),
                amount_satang: 50_000,
                service_start: PERIOD_START,
                service_end: PERIOD_END,
                cost_id: None,
                simulated: false,
            },
            InvoiceLine {
                description: "hosting: Provider cost-1".into(),
                amount_satang: 12_500,
                service_start: PERIOD_START,
                service_end: PERIOD_END,
                cost_id: Some("cost-1".into()),
                simulated: false,
            },
            InvoiceLine {
                description: "hosting: Provider cost-2".into(),
                amount_satang: 5_000,
                service_start: PERIOD_START,
                service_end: PERIOD_END,
                cost_id: Some("cost-2".into()),
                simulated: false,
            },
        ])
        .unwrap(),
        payment_mode: "manual".into(),
        stripe_id: None,
        due_at: None,
        payment_state: None,
        original_invoice_id: None,
        created_at: 1_780_000_000_000,
        updated_at: 1_780_000_000_000,
    }
}

fn create(finalize: bool) -> BillingInvoiceCommand {
    BillingInvoiceCommand::Create(CreateInvoice {
        key: key(KEY),
        expected_version: 4,
        reason: "Approve the reviewed October costs".into(),
        period_id: RecordId::new("period-10").unwrap(),
        finalize,
    })
}

fn update() -> BillingInvoiceCommand {
    BillingInvoiceCommand::Update(UpdateInvoice {
        key: key("billing-invoice-key-0002"),
        expected_version: 4,
        reason: "Refresh the reviewed draft".into(),
        invoice_id: RecordId::new(format!("billing-invoice-{KEY}")).unwrap(),
    })
}

fn finalize() -> BillingInvoiceCommand {
    BillingInvoiceCommand::Finalize(FinalizeInvoice {
        key: key("billing-invoice-key-0003"),
        expected_version: 4,
        reason: "Finalize the reviewed invoice".into(),
        invoice_id: RecordId::new(format!("billing-invoice-{KEY}")).unwrap(),
    })
}

fn void() -> BillingInvoiceCommand {
    BillingInvoiceCommand::Void(VoidInvoice {
        key: key("billing-invoice-key-0004"),
        expected_version: 4,
        reason: "Void the unpaid invoice".into(),
        invoice_id: RecordId::new("invoice-1").unwrap(),
    })
}

struct Probe {
    digest: String,
    calls: Vec<&'static str>,
    receipt: Option<ReceiptSnapshot>,
    holding: bool,
    account: Option<BillingAccountFacts>,
    period: Option<BillingPeriodFacts>,
    invoice: Option<BillingInvoiceRecord>,
    previous: Option<BillingInvoiceRecord>,
    pending: Option<String>,
    costs: Vec<BillingCostFacts>,
    inserted: Option<ValidatedInvoiceWrite>,
    attach_ok: bool,
    attached: Vec<String>,
    released: Vec<String>,
    enqueued: Vec<(String, String, String)>,
    attached_count: i64,
    readback: Option<BillingInvoiceRecord>,
    version: Option<i64>,
    audit_ok: bool,
    audit: Option<(String, String)>,
    receipt_insert_ok: bool,
    serialized: Option<ReceiptSnapshot>,
    fail_readback: bool,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            digest: "a".repeat(64),
            calls: Vec::new(),
            receipt: None,
            holding: true,
            account: Some(account()),
            period: Some(period()),
            invoice: None,
            previous: None,
            pending: None,
            costs: costs(),
            inserted: None,
            attach_ok: true,
            attached: Vec::new(),
            released: Vec::new(),
            enqueued: Vec::new(),
            attached_count: 0,
            readback: None,
            version: Some(5),
            audit_ok: true,
            audit: None,
            receipt_insert_ok: true,
            serialized: None,
            fail_readback: false,
        }
    }
}

impl Probe {
    fn apply(&mut self, write: &ValidatedInvoiceWrite, status: &str, due_at: Option<i64>) {
        self.readback = Some(BillingInvoiceRecord {
            id: write.id.clone(),
            period_id: write.period_id.clone(),
            kind: write.kind.clone(),
            status: status.into(),
            total_satang: write.total_satang,
            lines_json: write.lines_json.clone(),
            payment_mode: self
                .account
                .as_ref()
                .map_or("manual".into(), |a| a.payment_mode.clone()),
            stripe_id: None,
            due_at,
            payment_state: None,
            original_invoice_id: write.original_invoice_id.clone(),
            created_at: NOW.get(),
            updated_at: NOW.get(),
        });
    }
}

impl BillingInvoiceRepository for Probe {
    type Error = BusinessError;

    fn digest<'a>(&'a mut self, _: &'a str) -> BillingFuture<'a, String, Self::Error> {
        self.calls.push("digest");
        let digest = self.digest.clone();
        Box::pin(async move { Ok(digest) })
    }

    fn lock_holding<'a>(&'a mut self, _: &'a HoldingId) -> BillingFuture<'a, (), Self::Error> {
        self.calls.push("lock_holding");
        Box::pin(async { Ok(()) })
    }

    fn lock_command<'a>(&'a mut self, _: &'a str) -> BillingFuture<'a, (), Self::Error> {
        self.calls.push("lock_command");
        Box::pin(async { Ok(()) })
    }

    fn receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> BillingFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
        self.calls.push("receipt");
        let receipt = self.receipt.clone();
        Box::pin(async move { Ok(receipt) })
    }

    fn holding_exists<'a>(&'a mut self, _: &'a HoldingId) -> BillingFuture<'a, bool, Self::Error> {
        self.calls.push("holding_exists");
        let holding = self.holding;
        Box::pin(async move { Ok(holding) })
    }

    fn account_for_update<'a>(
        &'a mut self,
        _: &'a HoldingId,
    ) -> BillingFuture<'a, Option<BillingAccountFacts>, Self::Error> {
        self.calls.push("account_for_update");
        let account = self.account.clone();
        Box::pin(async move { Ok(account) })
    }

    fn period_for_update<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a HoldingId,
    ) -> BillingFuture<'a, Option<BillingPeriodFacts>, Self::Error> {
        self.calls.push("period_for_update");
        let period = self.period.clone();
        Box::pin(async move { Ok(period) })
    }

    fn invoice_for_update<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a HoldingId,
    ) -> BillingFuture<'a, Option<BillingInvoiceRecord>, Self::Error> {
        self.calls.push("invoice_for_update");
        let invoice = self.invoice.clone();
        Box::pin(async move { Ok(invoice) })
    }

    fn current_main_invoice<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a HoldingId,
    ) -> BillingFuture<'a, Option<BillingInvoiceRecord>, Self::Error> {
        self.calls.push("current_main_invoice");
        let previous = self.previous.clone();
        Box::pin(async move { Ok(previous) })
    }

    fn pending_invoice<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a HoldingId,
    ) -> BillingFuture<'a, Option<String>, Self::Error> {
        self.calls.push("pending_invoice");
        let pending = self.pending.clone();
        Box::pin(async move { Ok(pending) })
    }

    fn unissued_costs<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a HoldingId,
    ) -> BillingFuture<'a, Vec<BillingCostFacts>, Self::Error> {
        self.calls.push("unissued_costs");
        let costs = self.costs.clone();
        Box::pin(async move { Ok(costs) })
    }

    fn insert_invoice<'a>(
        &'a mut self,
        invoice: &'a ValidatedInvoiceWrite,
        _: &'a HoldingId,
        _: &'a BillingActor,
        _: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        self.calls.push("insert_invoice");
        let write = invoice.clone();
        self.inserted = Some(write.clone());
        if write.attach_costs {
            self.attached_count = write.cost_ids.len() as i64;
        }
        self.apply(&write, &write.status.clone(), write.due_at);
        Box::pin(async { Ok(true) })
    }

    fn update_invoice<'a>(
        &'a mut self,
        _: &'a str,
        invoice: &'a ValidatedInvoiceWrite,
        _: &'a HoldingId,
        _: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        self.calls.push("update_invoice");
        let write = invoice.clone();
        self.inserted = Some(write.clone());
        self.apply(&write, "draft", None);
        Box::pin(async { Ok(true) })
    }

    fn finalize_invoice<'a>(
        &'a mut self,
        _: &'a str,
        invoice: &'a ValidatedInvoiceWrite,
        _: &'a HoldingId,
        _: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        self.calls.push("finalize_invoice");
        let write = invoice.clone();
        self.inserted = Some(write.clone());
        self.attached_count = write.cost_ids.len() as i64;
        self.apply(&write, "queued", write.due_at);
        Box::pin(async { Ok(true) })
    }

    fn void_invoice<'a>(
        &'a mut self,
        _: &'a str,
        invoice: &'a ValidatedInvoiceWrite,
        _: &'a HoldingId,
        _: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        self.calls.push("void_invoice");
        let write = invoice.clone();
        self.inserted = Some(write.clone());
        self.attached_count = 0;
        self.apply(&write, "void", write.due_at);
        Box::pin(async { Ok(true) })
    }

    fn attach_cost<'a>(
        &'a mut self,
        cost_id: &'a str,
        _: &'a str,
        _: &'a HoldingId,
        _: &'a str,
    ) -> BillingFuture<'a, bool, Self::Error> {
        self.calls.push("attach_cost");
        self.attached.push(cost_id.to_owned());
        let ok = self.attach_ok;
        Box::pin(async move { Ok(ok) })
    }

    fn release_costs<'a>(
        &'a mut self,
        invoice_id: &'a str,
        _: &'a HoldingId,
    ) -> BillingFuture<'a, u64, Self::Error> {
        self.calls.push("release_costs");
        self.released.push(invoice_id.to_owned());
        Box::pin(async { Ok(1) })
    }

    fn enqueue_job<'a>(
        &'a mut self,
        id: &'a str,
        invoice_id: &'a str,
        kind: &'a str,
        _: &'a HoldingId,
        _: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        self.calls.push("enqueue_job");
        self.enqueued
            .push((id.to_owned(), invoice_id.to_owned(), kind.to_owned()));
        Box::pin(async { Ok(true) })
    }

    fn attached_cost_count<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a HoldingId,
    ) -> BillingFuture<'a, i64, Self::Error> {
        self.calls.push("attached_cost_count");
        let count = self.attached_count;
        Box::pin(async move { Ok(count) })
    }

    fn invoice_readback<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a HoldingId,
    ) -> BillingFuture<'a, Option<BillingInvoiceRecord>, Self::Error> {
        self.calls.push("invoice_readback");
        let invoice = if self.fail_readback {
            None
        } else {
            self.readback.clone()
        };
        Box::pin(async move { Ok(invoice) })
    }

    fn increment_account_version<'a>(
        &'a mut self,
        _: &'a HoldingId,
        _: i64,
    ) -> BillingFuture<'a, Option<i64>, Self::Error> {
        self.calls.push("increment_account_version");
        let version = self.version;
        Box::pin(async move { Ok(version) })
    }

    fn insert_audit<'a>(
        &'a mut self,
        entry: &'a AuditEntry<'a>,
        _: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        self.calls.push("insert_audit");
        self.audit = Some((entry.action.into(), entry.details_json.into()));
        let ok = self.audit_ok;
        Box::pin(async move { Ok(ok) })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        _: &'a str,
        digest: &'a str,
        result_json: &'a str,
        _: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        self.calls.push("insert_receipt");
        self.serialized = Some(ReceiptSnapshot {
            digest: digest.into(),
            result_json: result_json.into(),
        });
        let ok = self.receipt_insert_ok;
        Box::pin(async move { Ok(ok) })
    }

    fn serialized_receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> BillingFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
        self.calls.push("serialized_receipt");
        let receipt = self.serialized.clone();
        Box::pin(async move { Ok(receipt) })
    }
}

fn run(probe: &mut Probe, command: &BillingInvoiceCommand) -> Result<(), BusinessError> {
    test_block_on(async {
        execute_invoice_command(&admin(), &target(), command, probe, NOW)
            .await
            .map(|_| ())
    })
}

fn code(error: BusinessError) -> (FailureKind, &'static str) {
    (error.kind, error.code)
}

#[test]
fn create_derives_main_lines_and_finalizes_in_one_unit() {
    let mut probe = Probe::default();
    run(&mut probe, &create(true)).unwrap();
    let write = probe.inserted.unwrap();
    assert_eq!(write.kind, "main");
    assert_eq!(write.status, "queued");
    assert_eq!(write.total_satang, 67_500);
    assert_eq!(write.cost_ids, ["cost-1", "cost-2"]);
    assert_eq!(write.due_at, Some(DUE));
    assert_eq!(probe.attached, ["cost-1", "cost-2"]);
    assert_eq!(
        probe.enqueued,
        [(
            "issue-billing-invoice-billing-invoice-key-0001".into(),
            "billing-invoice-billing-invoice-key-0001".into(),
            "issue".into()
        )]
    );
    let lines: Vec<InvoiceLine> = serde_json::from_str(&write.lines_json).unwrap();
    assert_eq!(lines.len(), 3);
    assert_eq!(
        lines[0].description,
        "Sandbox Standard Support · revision 1"
    );
    assert_eq!(lines[0].amount_satang, 50_000);
    assert!(lines[0].cost_id.is_none());
    assert_eq!(probe.calls.last().copied(), Some("serialized_receipt"));
    assert_eq!(probe.audit.unwrap().0, "create-invoice");
}

#[test]
fn create_without_finalize_stays_an_editable_draft() {
    let mut probe = Probe::default();
    run(&mut probe, &create(false)).unwrap();
    let write = probe.inserted.unwrap();
    assert_eq!(write.status, "draft");
    assert_eq!(write.due_at, None);
    assert!(!write.attach_costs);
    assert!(probe.attached.is_empty());
    assert!(probe.enqueued.is_empty());
}

#[test]
fn update_refreshes_a_draft_from_current_reviewed_costs() {
    let mut probe = Probe {
        invoice: Some(draft_invoice()),
        previous: Some(draft_invoice()),
        costs: vec![cost("cost-1", 12_500), cost("cost-2", 7_000)],
        ..Probe::default()
    };
    run(&mut probe, &update()).unwrap();
    let write = probe.inserted.unwrap();
    assert_eq!(write.total_satang, 69_500);
    assert_eq!(write.status, "draft");
    assert!(probe.attached.is_empty());
    assert!(probe.enqueued.is_empty());
    assert_eq!(
        probe
            .calls
            .iter()
            .filter(|c| **c == "update_invoice")
            .count(),
        1
    );
}

#[test]
fn update_is_rejected_after_finalize() {
    let mut invoice = draft_invoice();
    invoice.status = "queued".into();
    let mut probe = Probe {
        invoice: Some(invoice),
        previous: Some(draft_invoice()),
        ..Probe::default()
    };
    let error = run(&mut probe, &update()).unwrap_err();
    assert_eq!(code(error), (FailureKind::Conflict, "INVOICE_NOT_DRAFT"));
    assert!(!probe.calls.contains(&"update_invoice"));
}

#[test]
fn adjustment_create_references_the_previous_main_invoice() {
    let mut probe = Probe {
        previous: Some(previous_main()),
        ..Probe::default()
    };
    run(&mut probe, &create(true)).unwrap();
    let write = probe.inserted.unwrap();
    assert_eq!(write.kind, "adjustment");
    assert_eq!(write.original_invoice_id.as_deref(), Some("invoice-1"));
    assert_eq!(write.total_satang, 17_500);
    let lines: Vec<InvoiceLine> = serde_json::from_str(&write.lines_json).unwrap();
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|line| line.cost_id.is_some()));
}

#[test]
fn create_requires_a_closed_and_complete_period() {
    for (closed, complete, expected) in [
        (None, true, "MONTH_NOT_CLOSED"),
        (Some(PERIOD_END), false, "COSTS_INCOMPLETE"),
    ] {
        let mut probe = Probe {
            period: Some(BillingPeriodFacts {
                closed_at: closed,
                costs_complete: complete,
                ..period()
            }),
            ..Probe::default()
        };
        let error = run(&mut probe, &create(true)).unwrap_err();
        assert_eq!(code(error), (FailureKind::Conflict, expected));
        assert!(!probe.calls.contains(&"insert_invoice"));
    }
}

#[test]
fn create_rejects_a_pending_invoice_and_a_month_without_new_costs() {
    let mut pending = Probe {
        pending: Some("invoice-2".into()),
        ..Probe::default()
    };
    let error = run(&mut pending, &create(true)).unwrap_err();
    assert_eq!(code(error), (FailureKind::Conflict, "INVOICE_PENDING"));

    let mut none = Probe {
        previous: Some(previous_main()),
        costs: Vec::new(),
        ..Probe::default()
    };
    let error = run(&mut none, &create(true)).unwrap_err();
    assert_eq!(code(error), (FailureKind::Conflict, "ALREADY_ISSUED"));
}

#[test]
fn create_rejects_a_negative_net_total() {
    let mut negative = cost("cost-negative", -99_950_000);
    negative.category = "adjustment".into();
    let mut probe = Probe {
        costs: vec![negative],
        ..Probe::default()
    };
    let error = run(&mut probe, &create(true)).unwrap_err();
    assert_eq!(
        code(error),
        (FailureKind::Conflict, "CREDIT_REVIEW_REQUIRED")
    );
    assert!(!probe.calls.contains(&"insert_invoice"));
}

#[test]
fn create_rejects_auto_pay_without_consent() {
    let mut probe = Probe {
        account: Some(BillingAccountFacts {
            payment_mode: "auto".into(),
            ..account()
        }),
        ..Probe::default()
    };
    let error = run(&mut probe, &create(true)).unwrap_err();
    assert_eq!(code(error), (FailureKind::Conflict, "CONSENT_REQUIRED"));
}

#[test]
fn create_bounds_line_count_and_source_amounts() {
    let mut many = Probe {
        costs: (0..251).map(|i| cost(&format!("cost-{i}"), 100)).collect(),
        ..Probe::default()
    };
    let error = run(&mut many, &create(true)).unwrap_err();
    assert_eq!(code(error), (FailureKind::Conflict, "TOO_MANY_LINES"));

    let mut huge = Probe {
        costs: vec![cost("cost-huge", 100_000_001)],
        ..Probe::default()
    };
    let error = run(&mut huge, &create(true)).unwrap_err();
    assert_eq!(
        code(error),
        (FailureKind::Conflict, "INVOICE_SOURCE_CHANGED")
    );

    let mut attached = Probe {
        costs: vec![BillingCostFacts {
            invoice_id: Some("invoice-other".into()),
            ..cost("cost-attached", 100)
        }],
        ..Probe::default()
    };
    let error = run(&mut attached, &create(true)).unwrap_err();
    assert_eq!(code(error), (FailureKind::Conflict, "COST_FROZEN"));
}

#[test]
fn finalize_freezes_a_draft_and_writes_the_issue_outbox() {
    let mut probe = Probe {
        invoice: Some(draft_invoice()),
        ..Probe::default()
    };
    run(&mut probe, &finalize()).unwrap();
    let write = probe.inserted.unwrap();
    assert_eq!(write.status, "queued");
    assert_eq!(write.total_satang, 67_500);
    assert_eq!(write.due_at, Some(DUE));
    assert_eq!(probe.attached, ["cost-1", "cost-2"]);
    assert_eq!(probe.enqueued.len(), 1);
    assert_eq!(probe.enqueued[0].2, "issue");
}

#[test]
fn finalize_rejects_a_changed_source_and_a_second_finalize() {
    let mut tampered = draft_invoice();
    tampered.lines_json = tampered.lines_json.replace("12500", "1");
    let mut probe = Probe {
        invoice: Some(tampered),
        ..Probe::default()
    };
    let error = run(&mut probe, &finalize()).unwrap_err();
    assert_eq!(
        code(error),
        (FailureKind::Conflict, "INVOICE_SOURCE_CHANGED")
    );
    assert!(!probe.calls.contains(&"finalize_invoice"));

    let mut queued = draft_invoice();
    queued.status = "queued".into();
    let mut probe = Probe {
        invoice: Some(queued),
        ..Probe::default()
    };
    let error = run(&mut probe, &finalize()).unwrap_err();
    assert_eq!(code(error), (FailureKind::Conflict, "INVOICE_NOT_DRAFT"));
}

#[test]
fn void_cancels_a_local_draft_without_a_provider_job() {
    let mut probe = Probe {
        invoice: Some(draft_invoice()),
        ..Probe::default()
    };
    run(
        &mut probe,
        &void_for(
            "billing-invoice-key-0004",
            &format!("billing-invoice-{KEY}"),
        ),
    )
    .unwrap();
    assert!(probe.released.is_empty());
    assert!(probe.enqueued.is_empty());
    let write = probe.inserted.unwrap();
    assert_eq!(write.status, "void");
}

#[test]
fn void_releases_costs_and_writes_the_void_outbox_for_an_issued_invoice() {
    let mut probe = Probe {
        invoice: Some(previous_main()),
        ..Probe::default()
    };
    run(&mut probe, &void()).unwrap();
    assert_eq!(probe.released, ["invoice-1"]);
    assert_eq!(probe.enqueued.len(), 1);
    assert_eq!(probe.enqueued[0].1, "invoice-1");
    assert_eq!(probe.enqueued[0].2, "void");
    assert_eq!(probe.inserted.unwrap().status, "void");
}

#[test]
fn void_rejects_queued_paid_and_already_void_invoices() {
    for status in ["queued", "paid", "void"] {
        let mut invoice = previous_main();
        invoice.status = status.into();
        let mut probe = Probe {
            invoice: Some(invoice),
            ..Probe::default()
        };
        let error = run(&mut probe, &void()).unwrap_err();
        assert_eq!(
            code(error),
            (FailureKind::Conflict, "VOID_UNAVAILABLE"),
            "{status}"
        );
        assert!(!probe.calls.contains(&"void_invoice"));
    }
    let mut paid = previous_main();
    paid.payment_state = Some("paid".into());
    let mut probe = Probe {
        invoice: Some(paid),
        ..Probe::default()
    };
    let error = run(&mut probe, &void()).unwrap_err();
    assert_eq!(code(error), (FailureKind::Conflict, "VOID_UNAVAILABLE"));
}

#[test]
fn replay_returns_the_prior_receipt_without_repeating_writes() {
    let receipt = ReceiptSnapshot {
        digest: "a".repeat(64),
        result_json: "{\"ok\":true}".into(),
    };
    let mut probe = Probe {
        receipt: Some(receipt.clone()),
        ..Probe::default()
    };
    run(&mut probe, &create(true)).unwrap();
    assert_eq!(
        probe.calls,
        ["digest", "lock_holding", "lock_command", "receipt"]
    );

    let mut conflict = Probe {
        receipt: Some(ReceiptSnapshot {
            digest: "b".repeat(64),
            result_json: receipt.result_json,
        }),
        ..Probe::default()
    };
    let error = run(&mut conflict, &create(true)).unwrap_err();
    assert_eq!(code(error), (FailureKind::Conflict, "KEY_CONFLICT"));
}

#[test]
fn version_and_holding_failures_precede_any_invoice_write() {
    let mut stale = Probe {
        account: Some(BillingAccountFacts {
            version: 3,
            ..account()
        }),
        ..Probe::default()
    };
    let error = run(&mut stale, &create(true)).unwrap_err();
    assert_eq!(code(error), (FailureKind::Conflict, "VERSION_CONFLICT"));
    assert!(!stale.calls.contains(&"insert_invoice"));

    let mut missing = Probe {
        holding: false,
        ..Probe::default()
    };
    let error = run(&mut missing, &create(true)).unwrap_err();
    assert_eq!(code(error), (FailureKind::NotFound, "NOT_FOUND"));

    let mut no_account = Probe {
        account: None,
        ..Probe::default()
    };
    let error = run(&mut no_account, &create(true)).unwrap_err();
    assert_eq!(code(error), (FailureKind::Conflict, "ACCOUNT_REQUIRED"));
}

#[test]
fn only_the_platform_administrator_may_change_invoices() {
    let mut probe = Probe::default();
    let error = test_block_on(async {
        execute_invoice_command(&owner(), &target(), &create(true), &mut probe, NOW)
            .await
            .unwrap_err()
    });
    assert_eq!(code(error), (FailureKind::Forbidden, "FORBIDDEN"));

    let mut expired = admin();
    expired.expires_at = Some(NOW.get());
    let mut probe = Probe::default();
    let error = test_block_on(async {
        execute_invoice_command(&expired, &target(), &create(true), &mut probe, NOW)
            .await
            .unwrap_err()
    });
    assert_eq!(code(error), (FailureKind::Conflict, "WORK_CONTEXT_CHANGED"));
}

#[test]
fn a_readback_mismatch_never_confirms_the_command() {
    let mut probe = Probe {
        fail_readback: true,
        ..Probe::default()
    };
    let error = run(&mut probe, &create(true)).unwrap_err();
    assert_eq!(error.kind, FailureKind::Unavailable);
    assert!(!probe.calls.contains(&"insert_audit"));
    assert!(!probe.calls.contains(&"insert_receipt"));
}

#[test]
fn receipt_failure_rolls_back_the_command() {
    let mut probe = Probe {
        receipt_insert_ok: false,
        ..Probe::default()
    };
    let error = run(&mut probe, &create(true)).unwrap_err();
    assert_eq!(code(error), (FailureKind::Conflict, "KEY_CONFLICT"));
}

#[test]
fn bounded_transport_validation_rejects_bad_version_and_reason() {
    for command in [
        BillingInvoiceCommand::Create(CreateInvoice {
            key: key(KEY),
            expected_version: -1,
            reason: "Approve the reviewed October costs".into(),
            period_id: RecordId::new("period-10").unwrap(),
            finalize: true,
        }),
        BillingInvoiceCommand::Create(CreateInvoice {
            key: key(KEY),
            expected_version: 4,
            reason: " x ".into(),
            period_id: RecordId::new("period-10").unwrap(),
            finalize: true,
        }),
    ] {
        let mut probe = Probe::default();
        let error = run(&mut probe, &command).unwrap_err();
        assert_eq!(
            code(error),
            (FailureKind::InvalidInput, "BILLING_INVOICE_REQUEST_INVALID")
        );
        assert_eq!(probe.calls, Vec::<&'static str>::new());
    }
}

#[test]
fn a_failed_cost_attachment_never_leaves_a_finalized_invoice() {
    let mut probe = Probe {
        attach_ok: false,
        ..Probe::default()
    };
    let error = run(&mut probe, &create(true)).unwrap_err();
    assert_eq!(code(error), (FailureKind::Conflict, "COST_FROZEN"));
    assert!(!probe.calls.contains(&"insert_audit"));
}

fn void_for(key_value: &str, invoice: &str) -> BillingInvoiceCommand {
    BillingInvoiceCommand::Void(VoidInvoice {
        key: key(key_value),
        expected_version: 4,
        reason: "Void the unpaid invoice".into(),
        invoice_id: RecordId::new(invoice).unwrap(),
    })
}
