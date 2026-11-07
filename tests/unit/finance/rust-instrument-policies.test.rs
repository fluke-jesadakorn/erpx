use erpx_server_finance::public::{
    DepreciationInput, InstrumentInput, decimal_to_units, depreciation_amount, half_away,
    instrument_schedule, monthly_date, units_to_decimal,
};
use num_bigint::BigInt;
use serde_json::{Value, json};

fn corpus() -> Value {
    serde_json::from_str(include_str!(
        "../../../fixtures/finance/rust-instruments/conformance.v1.json"
    ))
    .expect("checked-in oracle corpus")
}

#[test]
fn owner_transaction_plans_and_witnesses_are_not_wire_constructible() {
    use erpx_server_finance::public::{JournalPreparation, JournalTransactionWitness};
    trait WireBoundary<Marker> {
        fn requires_owner_preparation() {}
    }
    impl<T: ?Sized> WireBoundary<()> for T {}
    impl<T: serde::de::DeserializeOwned> WireBoundary<u8> for T {}
    let _ = <JournalPreparation as WireBoundary<_>>::requires_owner_preparation;
    let _ = <JournalTransactionWitness as WireBoundary<_>>::requires_owner_preparation;
}

#[test]
fn json_integer_forms_keep_javascript_semantics_without_coercing_strings() {
    for row in corpus()["wireNumbers"].as_array().unwrap() {
        let actual = serde_json::from_str::<InstrumentInput>(row["raw"].as_str().unwrap())
            .map_err(|error| error.to_string())
            .and_then(|input| instrument_schedule(&input).map_err(|error| error.to_string()))
            .map(|schedule| serde_json::to_value(schedule).unwrap());
        check(row, actual);
    }
}

#[test]
fn allocation_unicode_rate_selection_and_quote_conversion_match_pinned_node() {
    use erpx_server_finance::public::{
        AllocationWeight, FinanceCollation, ManualRate, allocate_units, convert_currency_units,
        resolve_manual_rate,
    };
    let data = corpus();
    let oracle = &data["companyMoney"];
    assert_eq!(oracle["runtime"]["node"], "v26.11.1");
    assert_eq!(oracle["runtime"]["icu"], "78.3");
    assert_eq!(oracle["runtime"]["locale"], "en-US");
    let collation = FinanceCollation::english().unwrap();
    for row in oracle["collation"].as_array().unwrap() {
        let order = match collation.compare(
            row["left"].as_str().unwrap(),
            row["right"].as_str().unwrap(),
        ) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        };
        assert_eq!(order, row["order"].as_i64().unwrap(), "{row}");
    }
    for row in oracle["allocations"].as_array().unwrap() {
        let weights: Vec<_> = row["weights"]
            .as_array()
            .unwrap()
            .iter()
            .map(|weight| AllocationWeight {
                key: weight["key"].as_str().unwrap().to_owned(),
                weight: integer(weight, "weight"),
            })
            .collect();
        let actual = allocate_units(&integer(row, "total"), &weights, &collation)
            .map(|rows| {
                Value::Array(
                    rows.into_iter()
                        .map(|line| json!({"key":line.key,"units":line.units.to_string()}))
                        .collect(),
                )
            })
            .map_err(|error| error.to_string());
        check(row, actual);
    }
    for row in oracle["rates"].as_array().unwrap() {
        let rates: Vec<ManualRate> = serde_json::from_value(row["rates"].clone()).unwrap();
        let actual = resolve_manual_rate(
            &rates,
            row["currency"].as_str().unwrap(),
            row["date"].as_str().unwrap(),
            &collation,
        );
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            row["expected"]["value"],
            "{row}"
        );
    }
    for row in oracle["conversions"].as_array().unwrap() {
        let actual = convert_currency_units(
            &integer(row, "amount"),
            u8::try_from(row["sourceScale"].as_u64().unwrap()).unwrap(),
            u8::try_from(row["targetScale"].as_u64().unwrap()).unwrap(),
            row["sourceQuote"].as_str().unwrap(),
            row["targetQuote"].as_str().unwrap(),
        )
        .map(|value| json!(value.to_string()))
        .map_err(|error| error.to_string());
        check(row, actual);
    }
}

#[test]
fn published_currency_scales_and_functional_fx_match_source_policy() {
    use erpx_server_finance::public::{
        FinanceCollation, ManualRate, embedded_currency_rules, functional_fx_evidence,
    };
    let rules = embedded_currency_rules().unwrap();
    let data = corpus();
    let oracle = &data["companyMoney"];
    for row in oracle["currencies"].as_array().unwrap() {
        check(
            row,
            rules
                .scale(
                    row["currency"].as_str().unwrap(),
                    row["companyScale"].as_i64(),
                )
                .map(|scale| json!(scale))
                .map_err(|error| error.to_string()),
        );
    }
    let collation = FinanceCollation::english().unwrap();
    for row in oracle["functional"].as_array().unwrap() {
        let rates: Vec<ManualRate> = serde_json::from_value(row["rates"].clone()).unwrap();
        let actual = functional_fx_evidence(
            &rates,
            row["currency"].as_str().unwrap(),
            row["base"].as_str().unwrap(),
            row["date"].as_str().unwrap(),
            &collation,
        );
        match actual {
            Ok(value) => assert_eq!(
                serde_json::to_value(value).unwrap(),
                row["expected"]["value"],
                "{row}"
            ),
            Err(error) => {
                assert_eq!(
                    error.code,
                    row["expected"]["code"].as_str().unwrap(),
                    "{row}"
                );
                assert_eq!(
                    error.message,
                    row["expected"]["error"].as_str().unwrap(),
                    "{row}"
                );
            }
        }
    }
}

#[path = "../../support/finance/journal-facts.rs"]
mod journal_fixture;
use erpx_server_finance::public::{FinanceActor, JournalDraft};
use erpx_server_shared::public::{CommandKey, CompanyId};
use journal_fixture::JournalFacts;

#[tokio::test]
async fn journal_application_uses_scoped_ports_and_stops_at_failed_guards() {
    use erpx_server_finance::public::{JournalStatus, prepare_journal};
    use erpx_server_shared::public::{AssignmentId, HoldingId, UserId};
    let actor = FinanceActor {
        user_id: UserId::new("user").unwrap(),
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        assignment_id: AssignmentId::new("assignment").unwrap(),
    };
    let draft = JournalDraft { chart_id: "chart".to_owned(), period_id: "period".to_owned(), entry_date: "2026-01-01".to_owned(), currency: "THB".to_owned(), fx_rate: None, is_opening: false,
        memo: String::new(), source_type: "manual".to_owned(), source_id: None,
        lines: serde_json::from_value(json!([{ "accountId": "cash", "debit": "1.00", "credit": "0" }, { "accountId": "sales", "debit": "0", "credit": "1.00" }])).unwrap() };
    let key = CommandKey::new("journal-test-key").unwrap();
    let mut facts = JournalFacts::default();
    let plan = prepare_journal(&mut facts, &actor, &draft, &key, JournalStatus::Draft)
        .await
        .unwrap();
    assert_eq!(plan.result().debit, "1.00");
    assert_eq!(plan.result().credit, "1.00");
    assert_eq!(plan.result().id, "journal-journal-test-key");
    assert_eq!(plan.now().get(), 100);
    assert!(plan.prepared_fx_evidence().is_some());
    assert_eq!(
        facts.calls,
        [
            "authorize",
            "chart",
            "period",
            "scale",
            "scale",
            "fx",
            "accounts",
            "clock",
            "transaction"
        ]
    );
    let cases = [
        (
            JournalFacts {
                deny: true,
                ..Default::default()
            },
            "FINANCE_FORBIDDEN",
            vec!["authorize"],
        ),
        (
            JournalFacts {
                no_chart: true,
                ..Default::default()
            },
            "CHART_NOT_FOUND",
            vec!["authorize", "chart"],
        ),
        (
            JournalFacts {
                unsupported_scale: true,
                ..Default::default()
            },
            "FINANCE_CURRENCY_SCALE_UNSUPPORTED",
            vec!["authorize", "chart", "period", "scale"],
        ),
        (
            JournalFacts {
                wrong_accounts: true,
                ..Default::default()
            },
            "ACCOUNT_INVALID",
            vec![
                "authorize",
                "chart",
                "period",
                "scale",
                "scale",
                "fx",
                "accounts",
            ],
        ),
    ];
    for (mut facts, code, calls) in cases {
        let error = prepare_journal(&mut facts, &actor, &draft, &key, JournalStatus::Draft)
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, code);
        assert_eq!(facts.calls, calls);
    }
    let mut facts = JournalFacts {
        opening: true,
        ..Default::default()
    };
    let mut opening_draft = draft.clone();
    opening_draft.is_opening = true;
    assert_eq!(
        prepare_journal(
            &mut facts,
            &actor,
            &opening_draft,
            &key,
            JournalStatus::Posted
        )
        .await
        .err()
        .unwrap()
        .code,
        "OPENING_BALANCE_EXISTS"
    );
    let mut facts = JournalFacts::default();
    let mut explicit_draft = draft;
    explicit_draft.fx_rate = Some("2".to_owned());
    let plan = prepare_journal(
        &mut facts,
        &actor,
        &explicit_draft,
        &key,
        JournalStatus::Posted,
    )
    .await
    .unwrap();
    assert!(plan.prepared_fx_evidence().is_none());
    assert!(!facts.calls.contains(&"fx"));
    assert_eq!(plan.amounts().lines[0].base_debit, 200);
}

#[test]
fn journal_fx_rounding_and_failures_match_original_use_case() {
    use erpx_server_finance::public::{
        JournalLineInput, functional_fx_micros, prepare_journal_amounts,
    };
    for row in corpus()["journals"].as_array().unwrap() {
        let input: Vec<JournalLineInput> =
            serde_json::from_value(row["input"]["lines"].clone()).unwrap();
        let actual = functional_fx_micros("THB", "USD", row["input"]["fxRate"].as_str())
            .and_then(|fx| prepare_journal_amounts(&input, fx));
        match actual {
            Ok(plan) => assert_eq!(
                json!({ "lines": plan.lines, "total": plan.total.to_string(), "accountIds": plan.account_ids }),
                row["expected"]["value"],
                "{row}"
            ),
            Err(error) => {
                assert_eq!(
                    error.code,
                    row["expected"]["code"].as_str().unwrap(),
                    "{row}"
                );
                assert_eq!(
                    error.message,
                    row["expected"]["error"].as_str().unwrap(),
                    "{row}"
                );
            }
        }
    }
}

#[test]
fn invoice_lines_round_once_and_preserve_safe_integer_limits() {
    use erpx_server_finance::public::{InvoiceLineInput, calculate_invoice_lines};
    for row in corpus()["invoices"].as_array().unwrap() {
        let input: Vec<InvoiceLineInput> = serde_json::from_value(row["input"].clone()).unwrap();
        check(
            row,
            calculate_invoice_lines(input)
                .map(|result| serde_json::to_value(result).unwrap())
                .map_err(|error| error.to_string()),
        );
    }
}

#[test]
fn calculation_methods_match_typescript_oracle() {
    use erpx_server_finance::public::{FinanceCalculation, FinanceCollation, calculate_finance};
    let collation = FinanceCollation::english().unwrap();
    for row in corpus()["calculations"].as_array().unwrap() {
        let scale = u8::try_from(row["scale"].as_u64().unwrap()).unwrap();
        let input: FinanceCalculation = serde_json::from_value(row.clone()).unwrap();
        let actual = calculate_finance(&input, scale, &collation)
            .map(|result| serde_json::to_value(result).unwrap());
        check(row, actual.map_err(|error| error.to_string()));
    }
}

fn check(row: &Value, actual: Result<Value, String>) {
    let expected = &row["expected"];
    if let Some(value) = expected.get("value") {
        assert_eq!(actual, Ok(value.clone()), "{}", row);
    } else {
        let error = actual.expect_err("oracle rejected this input");
        if expected["validation"] != true {
            assert_eq!(error, expected["error"].as_str().unwrap(), "{}", row);
        }
    }
}

#[test]
fn instrument_schedules_match_independent_typescript_oracle() {
    for row in corpus()["schedules"].as_array().unwrap() {
        let actual = serde_json::from_value::<InstrumentInput>(row["input"].clone())
            .map_err(|error| error.to_string())
            .and_then(|input| instrument_schedule(&input).map_err(|error| error.to_string()))
            .map(|result| serde_json::to_value(result).unwrap());
        check(row, actual);
    }
}

fn integer(input: &Value, name: &str) -> BigInt {
    input.get(name).map_or_else(
        || BigInt::from(0),
        |value| value.as_str().unwrap().parse().unwrap(),
    )
}

#[test]
fn depreciation_matches_reviewed_policy_and_carrying_evidence() {
    for row in corpus()["depreciation"].as_array().unwrap() {
        let input = &row["input"];
        let actual = serde_json::from_value(input["policy"].clone())
            .map_err(|error| error.to_string())
            .and_then(|policy| {
                depreciation_amount(&DepreciationInput {
                    cost: integer(input, "cost"),
                    residual: integer(input, "residual"),
                    accumulated: integer(input, "accumulated"),
                    basis_accumulated: integer(input, "basisAccumulated"),
                    life: input["life"].as_i64().unwrap(),
                    ordinal: input["ordinal"].as_i64().unwrap(),
                    policy,
                    prior_units: input["priorUnits"].as_i64().unwrap_or(0),
                    units: input["units"].as_i64().unwrap_or(0),
                    basis_ordinal: input["basisOrdinal"].as_i64().unwrap_or(0),
                    basis_units: input["basisUnits"].as_i64().unwrap_or(0),
                })
                .map(|value| json!(value.to_string()))
                .map_err(|error| error.to_string())
            });
        check(row, actual);
    }
}

#[test]
fn exact_decimal_rounding_and_utc_month_anchors_match_oracle() {
    let corpus = corpus();
    for row in corpus["money"].as_array().unwrap() {
        let scale = u8::try_from(row["scale"].as_u64().unwrap()).unwrap();
        let actual = decimal_to_units(row["input"].as_str().unwrap(), scale)
            .and_then(|units| {
                units_to_decimal(&units, scale)
                    .map(|decimal| json!({ "units": units.to_string(), "decimal": decimal }))
            })
            .map_err(|error| error.to_string());
        check(row, actual);
    }
    for row in corpus["rounding"].as_array().unwrap() {
        assert_eq!(
            half_away(&integer(row, "numerator"), &integer(row, "denominator"))
                .unwrap()
                .to_string(),
            row["value"].as_str().unwrap()
        );
    }
    for row in corpus["dates"].as_array().unwrap() {
        assert_eq!(
            monthly_date(
                row["anchor"].as_str().unwrap(),
                i32::try_from(row["offset"].as_i64().unwrap()).unwrap()
            )
            .unwrap(),
            row["value"].as_str().unwrap(),
            "{row}"
        );
    }
}
