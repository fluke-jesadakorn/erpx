//! Native POS cash-movement policy, idempotency and command sequencing.
use erpx_server_pos::public::cash_movement::{
    CashMovementActor, CashMovementFuture, CashMovementReadback, CashMovementReceipt,
    CashMovementRepository, CashReceipt, CashShift, CreateCashMovement, ValidatedCashMovement,
    create_cash_movement, valid_uuid,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
    TimestampMillis, UserId,
};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const KEY: &str = "cash-command-0001";
const SHIFT: &str = "shift-1";
const MOVEMENT: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";

fn actor() -> CashMovementActor {
    CashMovementActor {
        user_id: UserId::new("user").unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: RecordId::new("branch").ok(),
        department_id: None,
        assignment_id: AssignmentId::new("assignment").unwrap(),
        permissions: vec!["pos.sell".into()],
        modules: vec!["pos".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn command() -> CreateCashMovement {
    CreateCashMovement {
        key: CommandKey::new(KEY).unwrap(),
        shift_id: RecordId::new(SHIFT).unwrap(),
        expected_version: 3,
        amount_minor: 50_000,
        reason: "  Opening float top-up  ".into(),
    }
}

fn shift() -> CashShift {
    CashShift {
        id: SHIFT.into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        business_date: "2026-11-04".into(),
        state: "open".into(),
        version: 3,
    }
}

fn readback() -> CashMovementReadback {
    CashMovementReadback {
        id: MOVEMENT.into(),
        shift_id: SHIFT.into(),
        amount_minor: 50_000,
        reason: "Opening float top-up".into(),
        approved_by: None,
        created_at: NOW.get(),
        shift_version: 4,
        shift_state: "open".into(),
    }
}

#[derive(Clone)]
struct Probe {
    hash: String,
    receipt: Option<CashReceipt>,
    shift: Option<CashShift>,
    setup: bool,
    day: bool,
    movement_id: String,
    bump: bool,
    receipt_insert: bool,
    readback: Option<CashMovementReadback>,
    calls: Vec<&'static str>,
    written: Option<(i64, String, String)>,
    audited: usize,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            hash: "a".repeat(64),
            receipt: None,
            shift: Some(shift()),
            setup: true,
            day: true,
            movement_id: MOVEMENT.into(),
            bump: true,
            receipt_insert: true,
            readback: Some(readback()),
            calls: Vec::new(),
            written: None,
            audited: 0,
        }
    }
}

impl CashMovementRepository for Probe {
    type Error = BusinessError;

    fn digest<'a>(&'a mut self, value: &'a str) -> CashMovementFuture<'a, String, Self::Error> {
        self.calls.push("digest");
        assert!(value.contains("\"operation\":\"cash.create\""), "{value}");
        let hash = self.hash.clone();
        Box::pin(async move { Ok(hash) })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> CashMovementFuture<'a, Option<CashReceipt>, Self::Error> {
        self.calls.push("receipt");
        assert_eq!(key, KEY);
        let receipt = self.receipt.clone();
        Box::pin(async move { Ok(receipt) })
    }

    fn shift<'a>(
        &'a mut self,
        company: &'a CompanyId,
        shift_id: &'a str,
    ) -> CashMovementFuture<'a, Option<CashShift>, Self::Error> {
        self.calls.push("shift");
        assert_eq!(company.as_str(), "company");
        assert_eq!(shift_id, SHIFT);
        let shift = self.shift.clone();
        Box::pin(async move { Ok(shift) })
    }

    fn setup_exists<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> CashMovementFuture<'a, bool, Self::Error> {
        self.calls.push("setup");
        assert_eq!(company.as_str(), "company");
        let setup = self.setup;
        Box::pin(async move { Ok(setup) })
    }

    fn day_writable<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: &'a str,
        business_date: &'a str,
    ) -> CashMovementFuture<'a, bool, Self::Error> {
        self.calls.push("day");
        assert_eq!(company.as_str(), "company");
        assert_eq!(branch_id, "branch");
        assert_eq!(business_date, "2026-11-04");
        let day = self.day;
        Box::pin(async move { Ok(day) })
    }

    fn insert_movement<'a>(
        &'a mut self,
        movement: &'a ValidatedCashMovement,
        actor: &'a UserId,
        now: TimestampMillis,
    ) -> CashMovementFuture<'a, String, Self::Error> {
        self.calls.push("insert");
        assert_eq!(actor.as_str(), "user");
        assert_eq!(now, NOW);
        self.written = Some((
            movement.amount_minor,
            movement.reason.clone(),
            movement.shift_id.clone(),
        ));
        let id = self.movement_id.clone();
        Box::pin(async move { Ok(id) })
    }

    fn bump_shift<'a>(
        &'a mut self,
        shift_id: &'a str,
        expected_version: i64,
    ) -> CashMovementFuture<'a, bool, Self::Error> {
        self.calls.push("bump");
        assert_eq!(shift_id, SHIFT);
        assert_eq!(expected_version, 3);
        let bump = self.bump;
        Box::pin(async move { Ok(bump) })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        key: &'a str,
        company: &'a CompanyId,
        request_hash: &'a str,
        result_json: &'a str,
        now: TimestampMillis,
    ) -> CashMovementFuture<'a, bool, Self::Error> {
        self.calls.push("receipt-insert");
        assert_eq!(key, KEY);
        assert_eq!(company.as_str(), "company");
        assert_eq!(request_hash, self.hash);
        assert_eq!(result_json, format!("{{\"id\":\"{MOVEMENT}\"}}"));
        assert_eq!(now, NOW);
        let insert = self.receipt_insert;
        Box::pin(async move { Ok(insert) })
    }

    fn audit<'a>(
        &'a mut self,
        actor: &'a CashMovementActor,
        movement: &'a ValidatedCashMovement,
        receipt: &'a CashMovementReceipt,
        now: TimestampMillis,
    ) -> CashMovementFuture<'a, (), Self::Error> {
        self.calls.push("audit");
        assert_eq!(actor.user_id.as_str(), "user");
        assert_eq!(actor.assignment_id.as_str(), "assignment");
        assert_eq!(movement.amount_minor, 50_000);
        assert_eq!(receipt.id, MOVEMENT);
        assert_eq!(now, NOW);
        self.audited += 1;
        Box::pin(async move { Ok(()) })
    }

    fn readback<'a>(
        &'a mut self,
        movement_id: &'a str,
        shift_id: &'a str,
    ) -> CashMovementFuture<'a, Option<CashMovementReadback>, Self::Error> {
        self.calls.push("readback");
        assert_eq!(movement_id, MOVEMENT);
        assert_eq!(shift_id, SHIFT);
        let readback = self.readback.clone();
        Box::pin(async move { Ok(readback) })
    }
}

fn run(
    probe: &mut Probe,
    actor: &CashMovementActor,
    command: &CreateCashMovement,
) -> Result<CashMovementReceipt, BusinessError> {
    sqlx::test_block_on(create_cash_movement(actor, command, probe, NOW))
}

#[test]
fn permission_module_expiry_and_department_are_checked_before_any_read() {
    let mut denied = Probe::default();
    let mut no_permission = actor();
    no_permission.permissions.clear();
    assert_eq!(
        run(&mut denied, &no_permission, &command()).unwrap_err(),
        BusinessError::new(
            FailureKind::Forbidden,
            "POS_PERMISSION_REQUIRED",
            "Your selected role does not grant POS selling access."
        )
    );
    assert!(denied.calls.is_empty());

    let mut no_module = actor();
    no_module.modules.clear();
    let error = run(&mut Probe::default(), &no_module, &command()).unwrap_err();
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert_eq!(error.code, "POS_MODULE_FORBIDDEN");

    let mut expired = actor();
    expired.expires_at = NOW.get();
    let error = run(&mut Probe::default(), &expired, &command()).unwrap_err();
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    assert_eq!(error.kind, FailureKind::Conflict);

    let mut department = actor();
    department.branch_id = None;
    department.department_id = RecordId::new("department").ok();
    let error = run(&mut Probe::default(), &department, &command()).unwrap_err();
    assert_eq!(error.code, "POS_BRANCH_REQUIRED");

    for (expected, amount) in [(0, 0), (-1, 100_000_000_000)] {
        let mut input = command();
        input.expected_version = expected;
        input.amount_minor = amount;
        let mut probe = Probe::default();
        assert_eq!(
            run(&mut probe, &actor(), &input).unwrap_err().code,
            "POS_CASH_REQUEST_INVALID"
        );
        assert!(probe.calls.is_empty());
    }
    for reason in ["  a  ", "x"] {
        let mut input = command();
        input.reason = reason.into();
        let mut probe = Probe::default();
        assert_eq!(
            run(&mut probe, &actor(), &input).unwrap_err().code,
            "POS_CASH_REQUEST_INVALID"
        );
        assert!(probe.calls.is_empty());
    }
    let mut input = command();
    input.reason = "x".repeat(1_001);
    assert_eq!(
        run(&mut Probe::default(), &actor(), &input)
            .unwrap_err()
            .code,
        "POS_CASH_REQUEST_INVALID"
    );
}

#[test]
fn scope_version_state_day_and_setup_guards_fail_closed() {
    let mut probe = Probe::default();
    probe.shift.as_mut().unwrap().company_id = "foreign".into();
    assert_eq!(
        run(&mut probe, &actor(), &command()).unwrap_err().code,
        "POS_NOT_FOUND"
    );
    assert!(!probe.calls.contains(&"insert"));

    let mut probe = Probe::default();
    probe.shift.as_mut().unwrap().branch_id = "other-branch".into();
    assert_eq!(
        run(&mut probe, &actor(), &command()).unwrap_err().code,
        "POS_NOT_FOUND"
    );
    assert!(!probe.calls.contains(&"insert"));

    let mut probe = Probe::default();
    probe.shift.as_mut().unwrap().version = 4;
    assert_eq!(
        run(&mut probe, &actor(), &command()).unwrap_err().code,
        "POS_VERSION_CONFLICT"
    );
    assert!(!probe.calls.contains(&"insert"));

    let mut probe = Probe::default();
    probe.shift.as_mut().unwrap().state = "closed".into();
    let error = run(&mut probe, &actor(), &command()).unwrap_err();
    assert_eq!(error.code, "POS_CASH_REQUEST_INVALID");
    assert_eq!(error.kind, FailureKind::InvalidInput);

    let mut probe = Probe {
        day: false,
        ..Default::default()
    };
    assert_eq!(
        run(&mut probe, &actor(), &command()).unwrap_err().code,
        "POS_DAY_CLOSED"
    );
    assert!(!probe.calls.contains(&"insert"));

    let mut probe = Probe {
        setup: false,
        ..Default::default()
    };
    assert_eq!(
        run(&mut probe, &actor(), &command()).unwrap_err().code,
        "POS_SETUP_REQUIRED"
    );
    assert!(!probe.calls.contains(&"insert"));
}

#[test]
fn replay_returns_the_original_receipt_without_a_new_write() {
    let receipt = CashReceipt {
        request_hash: "a".repeat(64),
        result_json: format!("{{\"id\":\"{MOVEMENT}\"}}"),
    };
    let mut probe = Probe {
        receipt: Some(receipt),
        ..Default::default()
    };
    let result = run(&mut probe, &actor(), &command()).unwrap();
    assert_eq!(result.id, MOVEMENT);
    assert_eq!(probe.calls, vec!["digest", "receipt"]);
    assert!(probe.written.is_none());
}

#[test]
fn replay_with_a_different_payload_is_an_idempotency_conflict() {
    let mut probe = Probe {
        receipt: Some(CashReceipt {
            request_hash: "b".repeat(64),
            result_json: format!("{{\"id\":\"{MOVEMENT}\"}}"),
        }),
        ..Default::default()
    };
    let error = run(&mut probe, &actor(), &command()).unwrap_err();
    assert_eq!(error.code, "IDEMPOTENCY_CONFLICT");
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(probe.calls, vec!["digest", "receipt"]);
}

#[test]
fn success_writes_one_movement_bumps_the_exact_version_receipts_and_audits() {
    let mut probe = Probe::default();
    let result = run(&mut probe, &actor(), &command()).unwrap();
    assert_eq!(result.id, MOVEMENT);
    assert_eq!(
        probe.calls,
        vec![
            "digest",
            "receipt",
            "shift",
            "day",
            "setup",
            "insert",
            "bump",
            "receipt-insert",
            "audit",
            "readback"
        ]
    );
    assert_eq!(
        probe.written,
        Some((50_000, "Opening float top-up".into(), SHIFT.into()))
    );
    assert_eq!(probe.audited, 1);
}

#[test]
fn a_lost_cas_race_or_bad_readback_fails_before_success_is_returned() {
    let mut probe = Probe {
        bump: false,
        ..Default::default()
    };
    assert_eq!(
        run(&mut probe, &actor(), &command()).unwrap_err().code,
        "POS_VERSION_CONFLICT"
    );
    assert!(!probe.calls.contains(&"receipt-insert"));
    assert!(!probe.calls.contains(&"audit"));

    let mut probe = Probe::default();
    probe.readback.as_mut().unwrap().amount_minor = 1;
    assert_eq!(
        run(&mut probe, &actor(), &command()).unwrap_err().code,
        "POS_CASH_SOURCE_INVALID"
    );

    let mut probe = Probe {
        movement_id: "not-a-uuid".into(),
        ..Default::default()
    };
    assert_eq!(
        run(&mut probe, &actor(), &command()).unwrap_err().code,
        "POS_CASH_SOURCE_INVALID"
    );
    assert!(!probe.calls.contains(&"bump"));
}

#[test]
fn a_corrupt_stored_receipt_is_not_presented_as_success() {
    let mut probe = Probe {
        receipt: Some(CashReceipt {
            request_hash: "a".repeat(64),
            result_json: "{\"id\":\"not-a-uuid\"}".into(),
        }),
        ..Default::default()
    };
    assert_eq!(
        run(&mut probe, &actor(), &command()).unwrap_err().code,
        "POS_CASH_SOURCE_INVALID"
    );
    assert!(valid_uuid(MOVEMENT));
    assert!(!valid_uuid("0f8fad5b-d9cb-469f-a165-70867728950"));
}

#[test]
fn receipt_identity_is_company_assignment_and_input_bound() {
    let mut other = actor();
    other.assignment_id = AssignmentId::new("assignment-2").unwrap();
    let first = erpx_server_pos::public::cash_movement::receipt_identity(
        &actor(),
        &command().validate().unwrap(),
    );
    let second = erpx_server_pos::public::cash_movement::receipt_identity(
        &other,
        &command().validate().unwrap(),
    );
    assert_ne!(first, second);
    assert_eq!(
        first,
        r#"{"user":"user","assignment":"assignment","company":"company","branch":"branch","operation":"cash.create","input":{"id":"shift-1","expectedVersion":3,"amountMinor":50000,"reason":"Opening float top-up"}}"#
    );
}

#[test]
fn reason_counts_code_points_not_utf8_bytes_or_utf16_units() {
    // 1000 Thai scalars: 1000 code points (allowed), 3000 bytes.
    let mut thai = command();
    thai.reason = "ก".repeat(1000);
    assert_eq!(thai.validate().unwrap().reason, "ก".repeat(1000));
    // End to end: the stored readback must carry the same text.
    let mut probe = Probe::default();
    probe.readback.as_mut().unwrap().reason = thai.reason.clone();
    run(&mut probe, &actor(), &thai).unwrap();

    // 1001 Thai scalars exceed the JS max.
    let mut over = command();
    over.reason = "ก".repeat(1001);
    let mut probe = Probe::default();
    assert_eq!(
        run(&mut probe, &actor(), &over).unwrap_err().code,
        "POS_CASH_REQUEST_INVALID"
    );
    assert!(probe.calls.is_empty());

    // 1000 astral scalars: 1000 code points (allowed), 4000 bytes.
    let mut astral = command();
    astral.reason = "😀".repeat(1000);
    assert_eq!(astral.validate().unwrap().reason, "😀".repeat(1000));
    let mut probe = Probe::default();
    probe.readback.as_mut().unwrap().reason = astral.reason.clone();
    run(&mut probe, &actor(), &astral).unwrap();

    // 1001 astral scalars: 1001 code points -> over the max as Zod measures.
    let mut over_astral = command();
    over_astral.reason = "😀".repeat(1001);
    let mut probe = Probe::default();
    assert_eq!(
        run(&mut probe, &actor(), &over_astral).unwrap_err().code,
        "POS_CASH_REQUEST_INVALID"
    );
    assert!(probe.calls.is_empty());

    // Three astral scalars are 3 code points and pass the minimum;
    // two are 2 code points and fail it.
    let mut three_astral = command();
    three_astral.reason = "😀😀😀".into();
    assert_eq!(three_astral.validate().unwrap().reason, "😀😀😀");

    let mut short_astral = command();
    short_astral.reason = "😀😀".into();
    let mut probe = Probe::default();
    assert_eq!(
        run(&mut probe, &actor(), &short_astral).unwrap_err().code,
        "POS_CASH_REQUEST_INVALID"
    );
    assert!(probe.calls.is_empty());

    // One astral scalar is 1 code point, below the 3-code-point minimum.
    let mut single_astral = command();
    single_astral.reason = "😀".into();
    let mut probe = Probe::default();
    assert_eq!(
        run(&mut probe, &actor(), &single_astral).unwrap_err().code,
        "POS_CASH_REQUEST_INVALID"
    );
    assert!(probe.calls.is_empty());
}

#[test]
fn trims_feff_but_not_nel_like_javascript() {
    // JS trim strips ZWNBSP (U+FEFF): the reason normalizes and the
    // receipt identity matches the unpadded text.
    let mut padded = command();
    padded.reason = "\u{FEFF}Opening float top-up\u{FEFF}".into();
    assert_eq!(padded.validate().unwrap().reason, "Opening float top-up");
    assert_eq!(
        erpx_server_pos::public::cash_movement::receipt_identity(
            &actor(),
            &command().validate().unwrap(),
        ),
        erpx_server_pos::public::cash_movement::receipt_identity(
            &actor(),
            &padded.validate().unwrap(),
        )
    );
    let mut probe = Probe::default();
    run(&mut probe, &actor(), &padded).unwrap();
    assert_eq!(
        probe.written,
        Some((50_000, "Opening float top-up".into(), SHIFT.into()))
    );

    // FEFF-padded short text: JS trims FEFF leaving length 2 -> reject,
    // where Rust `str::trim` would keep FEFF and incorrectly accept.
    let mut short_padded = command();
    short_padded.reason = "\u{FEFF}\u{FEFF}ab\u{FEFF}".into();
    let mut probe = Probe::default();
    assert_eq!(
        run(&mut probe, &actor(), &short_padded).unwrap_err().code,
        "POS_CASH_REQUEST_INVALID"
    );
    assert!(probe.calls.is_empty());

    // JS trim does not strip NEL (U+0085): it counts toward the length,
    // so NEL plus 1000 letters exceeds the max and must be rejected
    // rather than trimmed and accepted.
    let mut nel_over = command();
    nel_over.reason = format!("\u{0085}{}", "x".repeat(1000));
    let mut probe = Probe::default();
    assert_eq!(
        run(&mut probe, &actor(), &nel_over).unwrap_err().code,
        "POS_CASH_REQUEST_INVALID"
    );
    assert!(probe.calls.is_empty());

    // Conversely, NEL plus "ab" is 3 units under JS rules because NEL
    // remains; Rust `str::trim` would strip NEL and reject the length.
    let mut nel_short = command();
    nel_short.reason = "\u{0085}ab".into();
    assert_eq!(nel_short.validate().unwrap().reason, "\u{0085}ab");
}

#[test]
fn amount_boundaries_match_retained_cash_create() {
    for valid in [99_999_999_999, -99_999_999_999, -1] {
        let mut input = command();
        input.amount_minor = valid;
        assert_eq!(input.validate().unwrap().amount_minor, valid);
    }
    for invalid in [0, 100_000_000_000, -100_000_000_000] {
        let mut input = command();
        input.amount_minor = invalid;
        assert_eq!(
            input.validate().unwrap_err().code,
            "POS_CASH_REQUEST_INVALID"
        );
    }
}
