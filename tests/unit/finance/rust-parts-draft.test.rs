use erpx_server_finance::public::{FinanceActor, parts_draft::*};
use erpx_server_shared::public::{
    AssignmentId, CommandKey, CompanyId, HoldingId, RecordId, TimestampMillis, UserId,
};
use serde_json::{Value, json};
fn facts() -> PartsDraftFacts {
    PartsDraftFacts {
        actor: FinanceActor {
            user_id: UserId::new("operator").unwrap(),
            holding_id: HoldingId::new("holding").unwrap(),
            company_id: CompanyId::new("company").unwrap(),
            assignment_id: AssignmentId::new("assignment").unwrap(),
        },
        owner_key: CommandKey::new("parts-owner-command").unwrap(),
        kind: PartsOwnerKind::Maintenance,
        source_id: PartsDraftSourceIdentity::Existing(RecordId::new("work").unwrap()),
        source_version: 3,
        scope: PartsDraftScope::Maintenance {
            branch_id: RecordId::new("branch").unwrap(),
            department_id: None,
        },
        quantity_milli: 1000,
        value_minor: 123,
        consumption_at: TimestampMillis::new(1800000000001),
        reason: "Recorded original source".into(),
    }
}
fn config() -> PartsDraftConfig {
    PartsDraftConfig {
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        chart_id: RecordId::new("chart").unwrap(),
        period_id: RecordId::new("period").unwrap(),
        entry_date: "2026-01-02".into(),
        currency: "THB".into(),
        base_currency: "THB".into(),
        inventory_account_id: RecordId::new("inventory").unwrap(),
        expense_account_id: RecordId::new("expense").unwrap(),
    }
}
fn wire(p: &PartsDraftPlan) -> Value {
    let d = p.draft();
    json!({"chartId":d.chart_id,"periodId":d.period_id,"entryDate":d.entry_date,"currency":d.currency,
        "memo":d.memo,"sourceType":d.source_type,"sourceId":d.source_id,"isOpening":d.is_opening,"lines":d.lines})
}
#[test]
fn exact_retained_b0_parts_derivation_covers_long_keys_unicode_cost_and_both_data_kinds() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../fixtures/finance/rust-parts-draft/conformance.v1.json"
    ))
    .unwrap();
    let mut cases = 0;
    for row in corpus["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r.get("journalInput").is_some())
    {
        let mut facts = facts();
        facts.owner_key = CommandKey::new(row["ownerKey"].as_str().unwrap()).unwrap();
        facts.kind = match row["kind"].as_str().unwrap() {
            "maintenance" => PartsOwnerKind::Maintenance,
            "field_service" => PartsOwnerKind::FieldService,
            _ => panic!("closed source kind"),
        };
        if facts.kind == PartsOwnerKind::FieldService {
            facts.scope = PartsDraftScope::FieldServicePhysical {
                branch_id: "branch".into(),
                department_id: None,
            };
        }
        facts.reason = row["reason"].as_str().unwrap().into();
        facts.value_minor = row["valueMinor"].as_i64().unwrap();
        let plan = derive_parts_draft(&facts, &config()).unwrap();
        assert_eq!(wire(&plan), row["journalInput"], "{}", row["id"]);
        assert_eq!(
            plan.identity().journal_id().as_str(),
            row["journalId"].as_str().unwrap()
        );
        assert_eq!(
            plan.identity().consumption_id().as_str(),
            row["consumptionId"].as_str().unwrap()
        );
        assert_eq!(
            plan.identity().journal_id().as_str().len(),
            row["journalIdBytes"].as_u64().unwrap() as usize
        );
        assert_eq!(
            plan.consumption_at().get(),
            row["consumptionCreatedAt"].as_i64().unwrap()
        );
        assert!(row["journalCreatedAt"].as_i64().unwrap() > plan.consumption_at().get());
        assert!(
            row["moneyCapturedAt"].as_i64().unwrap() > row["journalCreatedAt"].as_i64().unwrap()
        );
        cases += 1;
    }
    assert_eq!(cases, 10);
}
#[test]
fn every_valid_original_key_length_keeps_the_exact_suffix_and_native_id_bounds() {
    for length in 8..=180 {
        let key = format!("A:_.-{}", "x".repeat(length - 5));
        let id = parts_draft_identity(&CommandKey::new(&key).unwrap()).unwrap();
        assert_eq!(id.journal_id().as_str(), format!("journal-{key}:parts"));
        assert_eq!(id.journal_number(), format!("J-{key}:parts"));
        assert_eq!(
            id.consumption_id().as_str(),
            format!("operational-part-{key}")
        );
        assert!(id.journal_id().as_str().len() <= 200);
        assert!(id.consumption_id().as_str().len() <= 200);
    }
    let last = parts_draft_identity(&CommandKey::new("x".repeat(180)).unwrap()).unwrap();
    assert_eq!(last.journal_id().as_str().len(), 194);
}
#[test]
fn current_scope_and_safe_positive_structural_inputs_have_no_coercion_or_override() {
    let original = facts();
    for bad in [
        PartsDraftFacts {
            source_version: 0,
            ..original.clone()
        },
        PartsDraftFacts {
            source_version: 9_007_199_254_740_993,
            ..original.clone()
        },
        PartsDraftFacts {
            quantity_milli: 0,
            ..original.clone()
        },
        PartsDraftFacts {
            quantity_milli: 9_007_199_254_740_992,
            ..original.clone()
        },
        PartsDraftFacts {
            value_minor: 0,
            ..original.clone()
        },
        PartsDraftFacts {
            value_minor: 9_007_199_254_740_992,
            ..original.clone()
        },
        PartsDraftFacts {
            consumption_at: TimestampMillis::new(-1),
            ..original.clone()
        },
        PartsDraftFacts {
            consumption_at: TimestampMillis::new(9_007_199_254_740_992),
            ..original.clone()
        },
        PartsDraftFacts {
            reason: "x".repeat(2001),
            ..original.clone()
        },
    ] {
        assert!(
            matches!(derive_parts_draft(&bad,&config()),Err(e)if e.code=="PARTS_DRAFT_INPUT_INVALID")
        );
    }
    for config in [
        PartsDraftConfig {
            holding_id: HoldingId::new("other").unwrap(),
            ..config()
        },
        PartsDraftConfig {
            company_id: CompanyId::new("other").unwrap(),
            ..config()
        },
    ] {
        assert!(
            matches!(derive_parts_draft(&original,&config),Err(e)if e.code=="PARTS_DRAFT_SCOPE_CHANGED")
        );
    }
    for config in [
        PartsDraftConfig {
            currency: "USD".into(),
            ..config()
        },
        PartsDraftConfig {
            entry_date: "2026-02-30".into(),
            ..config()
        },
    ] {
        assert!(
            matches!(derive_parts_draft(&original,&config),Err(e)if e.code=="PARTS_DRAFT_INPUT_INVALID")
        );
    }
}
#[test]
fn derived_data_does_not_run_the_later_account_mapping_or_invent_a_manual_source() {
    let mut c = config();
    c.expense_account_id = c.inventory_account_id.clone();
    let plan = derive_parts_draft(&facts(), &c).unwrap();
    // An actual effect requires the retained mapping guard. Derivation cannot
    // preempt the original Journal/Money errors by applying it prematurely.
    assert_eq!(
        plan.draft().lines[0].account_id,
        plan.draft().lines[1].account_id
    );
    assert_eq!(plan.draft().source_type, "maintenance_parts");
    assert!(!plan.draft().is_opening);
    assert_eq!(
        plan.draft().source_id.as_deref(),
        Some("operational-part-parts-owner-command")
    );
}
#[test]
fn original_reason_identity_and_consumption_time_are_captured_as_data_before_later_phases() {
    let mut f = facts();
    let plan = derive_parts_draft(&f, &config()).unwrap();
    f.reason = "later unrelated text".into();
    f.owner_key = CommandKey::new("different-owner-key").unwrap();
    f.consumption_at = TimestampMillis::new(1800000000999);
    assert_eq!(plan.consumption_at().get(), 1800000000001);
    assert_eq!(
        plan.identity().journal_id().as_str(),
        "journal-parts-owner-command:parts"
    );
    assert!(plan.draft().memo.ends_with("Recorded original source"));
}

#[test]
fn valid_maximum_unicode_reason_is_not_rejected_by_an_ascii_sized_byte_limit() {
    for reason in ["ก".repeat(2000), "🛠".repeat(2000)] {
        let f = PartsDraftFacts {
            reason: reason.clone(),
            ..facts()
        };
        let p = derive_parts_draft(&f, &config()).unwrap();
        assert!(p.draft().memo.ends_with(&reason));
    }
}

#[test]
fn provisional_next_version_does_not_preempt_the_original_money_quote_error() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../fixtures/finance/rust-parts-draft/conformance.v1.json"
    ))
    .unwrap();
    let row = corpus["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "maximum-provisional-source-version-and-no-quote")
        .unwrap();
    assert_eq!(row["sourceVersion"], 9_007_199_254_740_992_i64);
    assert_eq!(row["error"]["code"], "MANUAL_RATE_REQUIRED");
    assert!(
        row["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e == "prepare-money")
    );
    assert!(
        !row["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| { e.as_str().is_some_and(|e| e.starts_with("apply:")) })
    );
    let f = PartsDraftFacts {
        source_version: 9_007_199_254_740_992,
        ..facts()
    };
    assert!(derive_parts_draft(&f, &config()).is_ok());
    // A derived plan has no persistence method. The actual owner still rejects
    // its late Work version/CAS rather than authorizing an unsafe output.
}

#[test]
fn source_kind_requires_its_own_scope_variant() {
    let mut f = facts();
    f.scope = PartsDraftScope::FieldServicePhysical {
        branch_id: "b".repeat(210),
        department_id: Some("d".repeat(210)),
    };
    assert!(
        matches!(derive_parts_draft(&f, &config()), Err(e) if e.code == "PARTS_DRAFT_SCOPE_CHANGED")
    );
    f.kind = PartsOwnerKind::FieldService;
    assert!(derive_parts_draft(&f, &config()).is_ok());
    f.scope = PartsDraftScope::Maintenance {
        branch_id: RecordId::new("branch").unwrap(),
        department_id: None,
    };
    assert!(
        matches!(derive_parts_draft(&f, &config()), Err(e) if e.code == "PARTS_DRAFT_SCOPE_CHANGED")
    );
}

#[test]
fn service_customer_preserves_original_defaults_strings_and_exact_credit_amounts() {
    use erpx_server_finance::public::ServiceCustomerInput;
    let wire = json!({"expectedVersion":0.0,"contextKey":"a".repeat(64),"input":{"kind":"customer","code":" C-01 ","name":" Original customer ","currency":"thb"}});
    let parsed = ServiceCustomerInput::parse(&serde_json::to_vec(&wire).unwrap()).unwrap();
    assert_eq!(parsed.input.code, " C-01 ");
    assert_eq!(parsed.input.name, " Original customer ");
    assert_eq!(parsed.input.currency, "THB");
    assert_eq!(parsed.input.payment_terms_days, 30);
    assert_eq!(parsed.input.credit_limit, "0");
    assert_eq!(parsed.input.credit_minor().unwrap(), 0);
    assert!(parsed.input.tax_registration.is_none());
    let mut party = parsed.input;
    for (amount, expected) in [
        ("1.230000", 123),
        ("-1.23", -123),
        ("0.000000", 0),
        ("90071992547409.91", 9_007_199_254_740_991),
    ] {
        party.credit_limit = amount.into();
        assert_eq!(party.credit_minor().unwrap(), expected);
    }
    for amount in ["1.234", "90071992547409.92", "-90071992547409.92"] {
        party.credit_limit = amount.into();
        assert!(party.credit_minor().is_err());
    }
}
#[test]
fn service_customer_refuses_non_customer_unknown_null_and_invalid_wire_values() {
    use erpx_server_finance::public::ServiceCustomerInput;
    let base = json!({"expectedVersion":0,"contextKey":"a".repeat(64),"input":{"kind":"customer","code":"C01","name":"Customer","currency":"THB"}});
    for (field, value) in [
        ("kind", json!("supplier")),
        ("taxRegistration", Value::Null),
        ("paymentTermsDays", json!(366)),
        ("paymentTermsDays", json!("30")),
        ("creditLimit", json!(0)),
        ("creditLimit", json!(" 0")),
        ("creditLimit", json!("0.0000000")),
        ("code", json!("😀".repeat(33))),
        ("unexpected", json!(true)),
    ] {
        let mut input = base.clone();
        input["input"][field] = value;
        assert!(
            ServiceCustomerInput::parse(&serde_json::to_vec(&input).unwrap()).is_err(),
            "{field}"
        );
    }
    for (field, value) in [
        ("expectedVersion", json!(1)),
        ("contextKey", json!("A".repeat(64))),
        ("unexpected", json!(true)),
    ] {
        let mut input = base.clone();
        input[field] = value;
        assert!(
            ServiceCustomerInput::parse(&serde_json::to_vec(&input).unwrap()).is_err(),
            "{field}"
        );
    }
}
