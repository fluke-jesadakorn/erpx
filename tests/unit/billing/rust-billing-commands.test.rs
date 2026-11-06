//! Native Billing cost-review command policy, CAS, replay and readback.
use erpx_server_billing::public::cost::{
    AccountSnapshot, AuditEntry, BillingCostCommand, BillingCostCommandRepository,
    CompleteCostReview, DeleteCost, ReceiptSnapshot, RecordCost, command_identity,
    execute_cost_command,
};
use erpx_server_billing::public::workspace::{
    BillingActor, BillingCostView, BillingFuture, BillingPeriodView,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, FailureKind, HoldingId, RecordId, TimestampMillis,
    UserId,
};
use sqlx::test_block_on;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const KEY: &str = "billing-command-0001";

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

fn period() -> BillingPeriodView {
    BillingPeriodView {
        id: "period-10".into(),
        month: "2026-10".into(),
        start_at: 1_778_000_000_000,
        end_at: 1_780_000_000_000,
        plan_id: "standard".into(),
        plan_revision: 1,
        support_satang: 50_000,
        costs_complete: false,
        closed_at: None,
    }
}

fn record() -> RecordCost {
    RecordCost {
        key: CommandKey::new(KEY).unwrap(),
        expected_version: 4,
        reason: "Reviewed actual cost".into(),
        period_id: RecordId::new("period-10").unwrap(),
        cost_id: None,
        category: "hosting".into(),
        amount_satang: 12_500,
        service_start: 1_778_000_000_000,
        service_end: 1_780_000_000_000,
        note: "  Provider hosting  ".into(),
        simulated: false,
        evidence_id: Some(RecordId::new("evidence-1").unwrap()),
        original_invoice_id: None,
    }
}

fn command() -> BillingCostCommand {
    BillingCostCommand::Record(record())
}

fn readback() -> BillingCostView {
    BillingCostView {
        id: format!("billing-cost-{KEY}"),
        period_id: "period-10".into(),
        category: "hosting".into(),
        amount_satang: 12_500,
        service_start: 1_778_000_000_000,
        service_end: 1_780_000_000_000,
        note: "Provider hosting".into(),
        simulated: false,
        evidence_id: Some("evidence-1".into()),
        invoice_id: None,
        original_invoice_id: None,
    }
}

struct Probe {
    digest: String,
    calls: Vec<&'static str>,
    receipt: Option<ReceiptSnapshot>,
    holding: bool,
    account: Option<AccountSnapshot>,
    period: Option<BillingPeriodView>,
    evidence: bool,
    original: bool,
    existing: Option<BillingCostView>,
    inserted: Option<erpx_server_billing::public::cost::ValidatedCostDraft>,
    readback: Option<BillingCostView>,
    deleted_period: Option<String>,
    cost_exists: bool,
    version: Option<i64>,
    audit_ok: bool,
    audit: Option<String>,
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
            account: Some(AccountSnapshot { version: 4 }),
            period: Some(period()),
            evidence: true,
            original: true,
            existing: None,
            inserted: None,
            readback: Some(readback()),
            deleted_period: Some("period-10".into()),
            cost_exists: false,
            version: Some(5),
            audit_ok: true,
            audit: None,
            receipt_insert_ok: true,
            serialized: None,
            fail_readback: false,
        }
    }
}

impl BillingCostCommandRepository for Probe {
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
    ) -> BillingFuture<'a, Option<AccountSnapshot>, Self::Error> {
        self.calls.push("account_for_update");
        let account = self.account.clone();
        Box::pin(async move { Ok(account) })
    }

    fn period<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a HoldingId,
    ) -> BillingFuture<'a, Option<BillingPeriodView>, Self::Error> {
        self.calls.push("period");
        let period = self.period.clone();
        Box::pin(async move { Ok(period) })
    }

    fn available_evidence<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a HoldingId,
    ) -> BillingFuture<'a, bool, Self::Error> {
        self.calls.push("available_evidence");
        let evidence = self.evidence;
        Box::pin(async move { Ok(evidence) })
    }

    fn original_invoice<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a HoldingId,
        _: &'a str,
    ) -> BillingFuture<'a, bool, Self::Error> {
        self.calls.push("original_invoice");
        let original = self.original;
        Box::pin(async move { Ok(original) })
    }

    fn existing_cost<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a HoldingId,
        _: &'a str,
    ) -> BillingFuture<'a, Option<BillingCostView>, Self::Error> {
        self.calls.push("existing_cost");
        let existing = self.existing.clone();
        Box::pin(async move { Ok(existing) })
    }

    fn insert_cost<'a>(
        &'a mut self,
        cost: &'a erpx_server_billing::public::cost::ValidatedCostDraft,
        _: &'a HoldingId,
        _: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        self.calls.push("insert_cost");
        self.inserted = Some(cost.clone());
        Box::pin(async { Ok(true) })
    }

    fn cost_readback<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a HoldingId,
        _: &'a str,
    ) -> BillingFuture<'a, Option<BillingCostView>, Self::Error> {
        self.calls.push("cost_readback");
        if self.fail_readback {
            return Box::pin(async { Ok(None) });
        }
        let readback = self.readback.clone();
        Box::pin(async move { Ok(readback) })
    }

    fn delete_unissued_cost<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a HoldingId,
    ) -> BillingFuture<'a, Option<String>, Self::Error> {
        self.calls.push("delete_unissued_cost");
        let period = self.deleted_period.clone();
        Box::pin(async move { Ok(period) })
    }

    fn cost_exists<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a HoldingId,
    ) -> BillingFuture<'a, bool, Self::Error> {
        self.calls.push("cost_exists");
        let exists = self.cost_exists;
        Box::pin(async move { Ok(exists) })
    }

    fn mark_costs_incomplete<'a>(&'a mut self, _: &'a str) -> BillingFuture<'a, (), Self::Error> {
        self.calls.push("mark_costs_incomplete");
        Box::pin(async { Ok(()) })
    }

    fn set_costs_complete<'a>(
        &'a mut self,
        _: &'a str,
        _: bool,
    ) -> BillingFuture<'a, (), Self::Error> {
        self.calls.push("set_costs_complete");
        Box::pin(async { Ok(()) })
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
        self.audit = Some(format!(
            "{}|{}|{}|{}",
            entry.action,
            entry.reason,
            entry.details_json,
            entry.target.as_str()
        ));
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
        let serialized = self.serialized.clone();
        Box::pin(async move { Ok(serialized) })
    }
}

fn run_with(
    command: BillingCostCommand,
    actor: &BillingActor,
    probe: &mut Probe,
) -> Result<erpx_server_billing::public::cost::CommandReceipt, BusinessError> {
    test_block_on(async { execute_cost_command(actor, &target(), &command, probe, NOW).await })
}

fn run(
    command: BillingCostCommand,
) -> Result<erpx_server_billing::public::cost::CommandReceipt, BusinessError> {
    let mut probe = Probe::default();
    run_with(command, &admin(), &mut probe)
}

#[test]
fn command_identity_covers_version_reason_and_normalized_fields() {
    let baseline = command();
    let identity = command_identity(&admin(), &target(), &baseline);
    let mut version = record();
    version.expected_version = 5;
    assert_ne!(
        identity,
        command_identity(&admin(), &target(), &BillingCostCommand::Record(version))
    );
    let mut reason = record();
    reason.reason = "Different reason".into();
    assert_ne!(
        identity,
        command_identity(&admin(), &target(), &BillingCostCommand::Record(reason))
    );
    let mut spaced = record();
    spaced.note = "Provider hosting".into();
    assert_eq!(
        identity,
        command_identity(&admin(), &target(), &BillingCostCommand::Record(spaced))
    );
}

#[test]
fn requires_a_platform_administrator_before_any_repository_call() {
    let owner = BillingActor {
        platform: false,
        role: "holding_owner".into(),
        holding_id: Some(target()),
        ..admin()
    };
    let mut probe = Probe::default();
    let error = run_with(command(), &owner, &mut probe).unwrap_err();
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert!(probe.calls.is_empty());

    let auditor = BillingActor {
        role: "platform_auditor".into(),
        ..admin()
    };
    let mut probe = Probe::default();
    assert_eq!(
        run_with(command(), &auditor, &mut probe).unwrap_err().code,
        "FORBIDDEN"
    );
    assert!(probe.calls.is_empty());
}

#[test]
fn replays_the_exact_retained_receipt_and_rejects_a_different_digest() {
    let mut probe = Probe {
        receipt: Some(ReceiptSnapshot {
            digest: "a".repeat(64),
            result_json: "{\"ok\":true}".into(),
        }),
        ..Probe::default()
    };
    let receipt = run_with(command(), &admin(), &mut probe).unwrap();
    assert_eq!(receipt.result_json, "{\"ok\":true}");
    assert_eq!(
        probe.calls,
        ["digest", "lock_holding", "lock_command", "receipt"]
    );

    let mut probe = Probe {
        receipt: Some(ReceiptSnapshot {
            digest: "b".repeat(64),
            result_json: "{\"ok\":true}".into(),
        }),
        ..Probe::default()
    };
    assert_eq!(
        run_with(command(), &admin(), &mut probe).unwrap_err().code,
        "KEY_CONFLICT"
    );
}

#[test]
fn requires_the_holding_and_current_account_version() {
    let mut probe = Probe {
        holding: false,
        ..Probe::default()
    };
    assert_eq!(
        run_with(command(), &admin(), &mut probe).unwrap_err().code,
        "NOT_FOUND"
    );

    let mut probe = Probe {
        account: None,
        ..Probe::default()
    };
    assert_eq!(
        run_with(command(), &admin(), &mut probe).unwrap_err().code,
        "ACCOUNT_REQUIRED"
    );

    let mut probe = Probe {
        account: Some(AccountSnapshot { version: 5 }),
        ..Probe::default()
    };
    assert_eq!(
        run_with(command(), &admin(), &mut probe).unwrap_err().code,
        "VERSION_CONFLICT"
    );
    assert!(!probe.calls.contains(&"insert_cost"));
}

#[test]
fn validates_cost_money_dates_evidence_and_adjustment_source() {
    let mut negative = record();
    negative.amount_satang = -1;
    assert_eq!(
        run(BillingCostCommand::Record(negative)).unwrap_err().code,
        "INVALID_COST"
    );

    let mut no_evidence = record();
    no_evidence.evidence_id = None;
    assert_eq!(
        run(BillingCostCommand::Record(no_evidence))
            .unwrap_err()
            .code,
        "EVIDENCE_REQUIRED"
    );

    let mut adjustment = record();
    adjustment.category = "adjustment".into();
    adjustment.amount_satang = -100;
    assert_eq!(
        run(BillingCostCommand::Record(adjustment.clone()))
            .unwrap_err()
            .code,
        "ORIGINAL_REQUIRED"
    );
    adjustment.original_invoice_id = Some(RecordId::new("invoice-1").unwrap());
    let mut probe = Probe {
        original: false,
        ..Probe::default()
    };
    assert_eq!(
        run_with(BillingCostCommand::Record(adjustment), &admin(), &mut probe)
            .unwrap_err()
            .code,
        "INVALID_ORIGINAL"
    );

    let mut dates = record();
    dates.service_end = dates.service_start;
    assert_eq!(
        run(BillingCostCommand::Record(dates)).unwrap_err().code,
        "INVALID_SERVICE_DATES"
    );

    let mut missing_evidence = record();
    let mut probe = Probe {
        evidence: false,
        ..Probe::default()
    };
    assert_eq!(
        run_with(
            BillingCostCommand::Record(missing_evidence.clone()),
            &admin(),
            &mut probe
        )
        .unwrap_err()
        .code,
        "INVALID_EVIDENCE"
    );
    missing_evidence.simulated = true;
    missing_evidence.evidence_id = None;
    let mut probe = Probe {
        readback: Some(BillingCostView {
            simulated: true,
            evidence_id: None,
            ..readback()
        }),
        ..Probe::default()
    };
    assert!(
        run_with(
            BillingCostCommand::Record(missing_evidence),
            &admin(),
            &mut probe
        )
        .is_ok()
    );
}

#[test]
fn rejects_frozen_cost_updates_and_a_missing_period() {
    let mut frozen = record();
    frozen.cost_id = Some(RecordId::new("cost-1").unwrap());
    let mut probe = Probe {
        existing: Some(BillingCostView {
            invoice_id: Some("invoice-1".into()),
            ..readback()
        }),
        ..Probe::default()
    };
    assert_eq!(
        run_with(BillingCostCommand::Record(frozen), &admin(), &mut probe)
            .unwrap_err()
            .code,
        "COST_FROZEN"
    );
    assert!(!probe.calls.contains(&"insert_cost"));

    let mut probe = Probe {
        period: None,
        ..Probe::default()
    };
    assert_eq!(
        run_with(command(), &admin(), &mut probe).unwrap_err().code,
        "PERIOD_REQUIRED"
    );
}

#[test]
fn records_cost_receipt_and_audit_in_one_sequence_with_persisted_readback() {
    let mut probe = Probe::default();
    let receipt = run_with(command(), &admin(), &mut probe).unwrap();
    assert_eq!(receipt.digest, "a".repeat(64));
    assert_eq!(receipt.result_json, "{\"ok\":true}");
    assert_eq!(
        probe.calls,
        [
            "digest",
            "lock_holding",
            "lock_command",
            "receipt",
            "holding_exists",
            "account_for_update",
            "period",
            "available_evidence",
            "insert_cost",
            "mark_costs_incomplete",
            "cost_readback",
            "increment_account_version",
            "insert_audit",
            "insert_receipt",
            "serialized_receipt",
        ]
    );
    let inserted = probe.inserted.as_ref().unwrap();
    assert_eq!(inserted.id, format!("billing-cost-{KEY}"));
    assert_eq!(inserted.note, "Provider hosting");
    assert!(
        probe
            .audit
            .as_ref()
            .unwrap()
            .starts_with("cost|Reviewed actual cost|")
    );
}

#[test]
fn completes_and_deletes_a_reviewed_period_with_readback() {
    let complete = BillingCostCommand::Complete(CompleteCostReview {
        key: CommandKey::new("billing-complete-0001").unwrap(),
        expected_version: 4,
        reason: "Reviewed the whole month".into(),
        period_id: RecordId::new("period-10").unwrap(),
        complete: true,
    });
    let mut probe = Probe {
        period: Some(BillingPeriodView {
            costs_complete: true,
            ..period()
        }),
        ..Probe::default()
    };
    run_with(complete, &admin(), &mut probe).unwrap();
    assert!(probe.calls.contains(&"set_costs_complete"));
    assert!(!probe.calls.contains(&"insert_cost"));

    let delete = BillingCostCommand::Delete(DeleteCost {
        key: CommandKey::new("billing-delete-0001").unwrap(),
        expected_version: 4,
        reason: "Remove incorrect cost".into(),
        cost_id: RecordId::new("cost-1").unwrap(),
    });
    let mut probe = Probe::default();
    run_with(delete, &admin(), &mut probe).unwrap();
    assert!(probe.calls.contains(&"delete_unissued_cost"));
    assert!(probe.calls.contains(&"cost_exists"));
}

#[test]
fn fails_closed_on_readback_audit_and_receipt_loss() {
    let mut probe = Probe {
        fail_readback: true,
        ..Probe::default()
    };
    assert_eq!(
        run_with(command(), &admin(), &mut probe).unwrap_err().code,
        "BILLING_SOURCE_INVALID"
    );
    assert!(!probe.calls.contains(&"increment_account_version"));

    let mut probe = Probe {
        readback: Some(BillingCostView {
            amount_satang: 1,
            ..readback()
        }),
        ..Probe::default()
    };
    assert_eq!(
        run_with(command(), &admin(), &mut probe).unwrap_err().code,
        "BILLING_SOURCE_INVALID"
    );
    assert!(!probe.calls.contains(&"insert_audit"));

    let mut probe = Probe {
        audit_ok: false,
        ..Probe::default()
    };
    assert_eq!(
        run_with(command(), &admin(), &mut probe).unwrap_err().code,
        "KEY_CONFLICT"
    );
    assert!(!probe.calls.contains(&"insert_receipt"));

    let mut probe = Probe {
        receipt_insert_ok: false,
        ..Probe::default()
    };
    assert_eq!(
        run_with(command(), &admin(), &mut probe).unwrap_err().code,
        "KEY_CONFLICT"
    );
}

#[test]
fn reason_counts_js_utf16_units_not_utf8_bytes_or_scalars() {
    // 1000 Thai scalars: 1000 UTF-16 units (allowed), 3000 bytes.
    let mut thai = record();
    thai.reason = "ก".repeat(1000);
    let mut probe = Probe::default();
    run_with(BillingCostCommand::Record(thai), &admin(), &mut probe).unwrap();

    // 1001 Thai scalars exceed the JS max.
    let mut over = record();
    over.reason = "ก".repeat(1001);
    let mut probe = Probe::default();
    assert_eq!(
        run_with(BillingCostCommand::Record(over), &admin(), &mut probe)
            .unwrap_err()
            .code,
        "BILLING_REQUEST_INVALID"
    );
    assert!(probe.calls.is_empty());

    // 500 astral scalars: 1000 UTF-16 units, 2000 bytes.
    let mut astral = record();
    astral.reason = "😀".repeat(500);
    let mut probe = Probe::default();
    run_with(BillingCostCommand::Record(astral), &admin(), &mut probe).unwrap();

    // Two astral scalars count 4 UTF-16 units, so they pass the 3-unit
    // minimum where a scalar count would reject them.
    let mut short_astral = record();
    short_astral.reason = "😀😀".into();
    let mut probe = Probe::default();
    run_with(
        BillingCostCommand::Record(short_astral),
        &admin(),
        &mut probe,
    )
    .unwrap();
}

#[test]
fn note_counts_js_utf16_units_not_utf8_bytes() {
    // 2000 Thai scalars: 2000 UTF-16 units (allowed), 6000 bytes.
    let mut thai = record();
    thai.note = "ก".repeat(2000);
    let mut probe = Probe {
        readback: Some(BillingCostView {
            note: thai.note.clone(),
            ..readback()
        }),
        ..Probe::default()
    };
    run_with(BillingCostCommand::Record(thai), &admin(), &mut probe).unwrap();

    let mut over = record();
    over.note = "ก".repeat(2001);
    let mut probe = Probe::default();
    assert_eq!(
        run_with(BillingCostCommand::Record(over), &admin(), &mut probe)
            .unwrap_err()
            .code,
        "BILLING_REQUEST_INVALID"
    );
    assert!(probe.calls.is_empty());

    // 1000 astral scalars: 2000 UTF-16 units, 4000 bytes.
    let mut astral = record();
    astral.note = "😀".repeat(1000);
    let mut probe = Probe {
        readback: Some(BillingCostView {
            note: astral.note.clone(),
            ..readback()
        }),
        ..Probe::default()
    };
    run_with(BillingCostCommand::Record(astral), &admin(), &mut probe).unwrap();
}

#[test]
fn trims_feff_but_not_nel_like_javascript() {
    // JS trim strips ZWNBSP (U+FEFF): the note normalizes and the
    // identity matches the unpadded text.
    let mut padded = record();
    padded.note = "\u{FEFF}Provider hosting\u{FEFF}".into();
    padded.reason = "\u{FEFF}Reviewed actual cost\u{FEFF}".into();
    let mut probe = Probe::default();
    run_with(
        BillingCostCommand::Record(padded.clone()),
        &admin(),
        &mut probe,
    )
    .unwrap();
    assert_eq!(probe.inserted.as_ref().unwrap().note, "Provider hosting");
    assert_eq!(
        command_identity(&admin(), &target(), &BillingCostCommand::Record(record())),
        command_identity(&admin(), &target(), &BillingCostCommand::Record(padded))
    );

    // JS trim does not strip NEL (U+0085): those units count toward the
    // length and remain a control character, so Rust must reject rather
    // than trim and accept.
    let mut nel = record();
    nel.reason = format!("\u{0085}{}", "x".repeat(1000));
    let mut probe = Probe::default();
    assert_eq!(
        run_with(BillingCostCommand::Record(nel), &admin(), &mut probe)
            .unwrap_err()
            .code,
        "BILLING_REQUEST_INVALID"
    );
    assert!(probe.calls.is_empty());
}
