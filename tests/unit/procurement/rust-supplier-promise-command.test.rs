//! Native Procurement supplier promise command policy and validation tests.
use erpx_server_procurement::public::supplier_promise_command::{
    PreparedPromiseLine, PreparedSupplierPromise, PromiseOrderLineSource, PromiseOrderSource,
    PromiseSource, RecordSupplierPromiseCommand, SupplierPromiseAuthority,
    SupplierPromiseCommandActor, SupplierPromiseLineCommand, ValidatedSupplierPromise,
    fingerprint, idempotency_conflict, incomplete_receipt, invalid_receipt, line_id, not_found,
    prepare_supplier_promise, promise_id, source_changed, validate_readback,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, FailureKind, RecordId, TimestampMillis,
};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> SupplierPromiseCommandActor {
    SupplierPromiseCommandActor {
        user_id: "owner".into(),
        assignment_id: "assignment".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        revision: 1,
        assignment_version: 1,
        permissions: vec!["commerce.read".into(), "commerce.purchase".into()],
        modules: vec!["procurement".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn confirmed_input() -> RecordSupplierPromiseCommand {
    RecordSupplierPromiseCommand {
        expected_version: 1,
        kind: "confirmed".into(),
        confirmation_date: "2026-10-25".into(),
        supplier_reference: "SUP-REF-1".into(),
        evidence: "Supplier confirmation evidence".into(),
        attachment_set_ids: Vec::new(),
        lines: vec![SupplierPromiseLineCommand {
            order_line_id: "line-a".into(),
            quantity: "4.000".into(),
            promised_date: "2026-10-26".into(),
        }],
    }
}

#[test]
fn validates_confirmed_and_withdrawn_commands() {
    let input = confirmed_input();
    let validated = input.validate().expect("confirmed input should validate");
    assert_eq!(validated.kind, "confirmed");
    assert_eq!(validated.lines.len(), 1);
    assert_eq!(validated.lines[0].quantity_milli, 4000);

    let withdrawn = RecordSupplierPromiseCommand {
        kind: "withdrawn".into(),
        lines: Vec::new(),
        ..confirmed_input()
    };
    let validated = withdrawn.validate().expect("withdrawn input should validate");
    assert_eq!(validated.kind, "withdrawn");
    assert!(validated.lines.is_empty());
}

#[test]
fn rejects_invalid_domain_inputs() {
    let mut input = confirmed_input();
    input.expected_version = 0;
    assert!(input.validate().is_err());

    let mut input = confirmed_input();
    input.kind = "received".into();
    assert!(input.validate().is_err());

    let mut input = confirmed_input();
    input.confirmation_date = "2026-13-40".into();
    assert!(input.validate().is_err());

    let mut input = confirmed_input();
    input.supplier_reference = "ab".into();
    assert!(input.validate().is_err());

    let mut input = confirmed_input();
    input.evidence = "abcd".into();
    assert!(input.validate().is_err());

    let mut input = confirmed_input();
    input.attachment_set_ids = vec!["set-1".into()];
    let err = input.validate().unwrap_err();
    assert_eq!(err.code, "SUPPLIER_PROMISE_EVIDENCE_UNSUPPORTED");

    let mut input = confirmed_input();
    input.attachment_set_ids = vec!["set-1".into(), "set-1".into()];
    assert!(input.validate().is_err());

    let mut input = confirmed_input();
    input.lines.clear();
    assert!(input.validate().is_err());

    let mut input = confirmed_input();
    input.kind = "withdrawn".into();
    let err = input.validate().unwrap_err();
    assert_eq!(err.code, "PROMISE_LINES_REQUIRED");

    let mut input = confirmed_input();
    input.lines.push(input.lines[0].clone());
    assert!(input.validate().is_err());

    let mut input = confirmed_input();
    input.lines[0].promised_date = "2026-10-20".into();
    assert!(input.validate().is_err());

    let mut input = confirmed_input();
    input.lines[0].quantity = "0".into();
    assert!(input.validate().is_err());
}

#[test]
fn prepares_validated_promise_and_enforces_quantity() {
    let actor = actor();
    let validated = confirmed_input().validate().unwrap();
    let source = PromiseSource {
        order: PromiseOrderSource {
            id: "order".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            department_id: None,
            kind: "purchase".into(),
            state: "confirmed".into(),
            version: 1,
            order_date: "2026-10-20".into(),
            due_date: "2026-11-10".into(),
            currency: "THB".into(),
            supplier_id: "supplier".into(),
        },
        lines: vec![PromiseOrderLineSource {
            id: "line-a".into(),
            quantity_milli: 10000,
            fulfilled_milli: 4000,
        }],
    };
    let prepared = prepare_supplier_promise(&actor, &validated, source).expect("should prepare");
    assert_eq!(prepared.lines.len(), 1);
    assert_eq!(prepared.lines[0].fulfilled_snapshot_milli, 4000);

    let validated = {
        let mut c = confirmed_input();
        c.lines[0].quantity = "7.000".into();
        c.validate().unwrap()
    };
    let source = PromiseSource {
        order: PromiseOrderSource {
            id: "order".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            department_id: None,
            kind: "purchase".into(),
            state: "confirmed".into(),
            version: 1,
            order_date: "2026-10-20".into(),
            due_date: "2026-11-10".into(),
            currency: "THB".into(),
            supplier_id: "supplier".into(),
        },
        lines: vec![PromiseOrderLineSource {
            id: "line-a".into(),
            quantity_milli: 10000,
            fulfilled_milli: 4000,
        }],
    };
    let err = prepare_supplier_promise(&actor, &validated, source).unwrap_err();
    assert_eq!(err.code, "PROMISE_QUANTITY_EXCEEDED");
}

#[test]
fn command_identifiers_and_fingerprint_are_stable() {
    let actor = actor();
    let input = confirmed_input();
    let key = CommandKey::new("command-key-1").unwrap();
    assert_eq!(promise_id(&key), "supplier-promise-command-key-1");
    assert_eq!(line_id("promise-1", 2), "promise-1-2");

    let fp1 = fingerprint(&actor, "order", &input).unwrap();
    let fp2 = fingerprint(&actor, "order", &input).unwrap();
    assert_eq!(fp1, fp2);

    let _ = not_found();
    let _ = source_changed();
    let _ = idempotency_conflict();
    let _ = incomplete_receipt();
    let _ = invalid_receipt();
}

#[test]
fn validate_readback_checks_persisted_shape() {
    let actor = actor();
    let validated = confirmed_input().validate().unwrap();
    let source = PromiseSource {
        order: PromiseOrderSource {
            id: "order".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            department_id: None,
            kind: "purchase".into(),
            state: "confirmed".into(),
            version: 1,
            order_date: "2026-10-20".into(),
            due_date: "2026-11-10".into(),
            currency: "THB".into(),
            supplier_id: "supplier".into(),
        },
        lines: vec![PromiseOrderLineSource {
            id: "line-a".into(),
            quantity_milli: 10000,
            fulfilled_milli: 4000,
        }],
    };
    let prepared = prepare_supplier_promise(&actor, &validated, source).unwrap();
    let promise = promise_id(&CommandKey::new("command-key-2").unwrap());
    let readback = erpx_server_procurement::public::supplier_promise_command::SupplierPromiseReadback {
        id: promise.clone(),
        order_id: "order".into(),
        order_version: 1,
        kind: "confirmed".into(),
        recorded_by: "owner".into(),
        created_at: NOW.get(),
        line_count: 1,
    };
    validate_readback(&readback, &prepared, &promise, &actor, NOW).expect("readback should pass");

    let mut bad = readback.clone();
    bad.line_count = 2;
    assert!(validate_readback(&bad, &prepared, &promise, &actor, NOW).is_err());
}

#[test]
fn authority_serializes_from_actor() {
    let actor = actor();
    let authority = SupplierPromiseAuthority::from_actor(&actor);
    assert_eq!(authority.company_id, "company");
    assert_eq!(authority.user_id, "owner");
    let _ = PreparedPromiseLine {
        ordinal: 0,
        order_line_id: "line-a".into(),
        quantity_milli: 1000,
        fulfilled_snapshot_milli: 0,
        promised_date: "2026-10-26".into(),
    };
    let validated = confirmed_input().validate().unwrap();
    let _ = PreparedSupplierPromise {
        order: PromiseOrderSource {
            id: "order".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            department_id: None,
            kind: "purchase".into(),
            state: "confirmed".into(),
            version: 1,
            order_date: "2026-10-20".into(),
            due_date: "2026-11-10".into(),
            currency: "THB".into(),
            supplier_id: "supplier".into(),
        },
        input: validated,
        lines: Vec::new(),
    };
}