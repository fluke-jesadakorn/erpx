use erpx_server_workflow::public::commands::*;
use serde_json::Value;
fn corpus() -> Value {
    serde_json::from_str(include_str!(
        "../../../fixtures/workflow/rust-commands/conformance.v1.json"
    ))
    .unwrap()
}
#[path = "../../support/workflow/command-actor.rs"]
mod actor_fixture;
use actor_fixture::parse_frozen_actor as actor;
#[test]
fn finite_overflow_utf16_duplicates_and_integer_property_order_match_actual_node_json() {
    let corpus = corpus();
    assert_eq!(corpus["node"], "v26.11.1");
    let cases = corpus["json"].as_array().unwrap();
    assert_eq!(cases.len(), 47);
    for row in cases {
        assert_eq!(
            CommandJson::parse(row["inputJson"].as_str().unwrap())
                .unwrap()
                .as_str(),
            row["expectedJson"].as_str().unwrap(),
            "{}",
            row["inputJson"]
        );
    }
}
#[test]
fn current_actor_fingerprints_match_actual_legacy_application_including_null_presence() {
    let corpus = corpus();
    let cases = corpus["fingerprints"].as_array().unwrap();
    assert_eq!(cases.len(), 227);
    for row in cases {
        assert!(row["expectedError"].is_null());
        let a = actor(row["actorJson"].as_str().unwrap());
        let input = CommandJson::parse(row["inputJson"].as_str().unwrap()).unwrap();
        assert_eq!(
            command_fingerprint(
                &a,
                row["operation"].as_str().unwrap(),
                row["recordId"].as_str().unwrap(),
                &input
            )
            .unwrap(),
            row["expectedHash"].as_str().unwrap(),
            "{}",
            row["id"]
        );
    }
}
#[test]
fn historical_guest_account_aliases_and_prototype_mapping_match_actual_node_source() {
    let corpus = corpus();
    let cases = corpus["compatibility"].as_array().unwrap();
    assert_eq!(cases.len(), 54);
    for row in cases {
        let input = CommandJson::parse(row["inputJson"].as_str().unwrap()).unwrap();
        for legacy in [false, true] {
            let result = if legacy {
                legacy_guest_account_json(&input)
            } else {
                canonical_guest_account_json(&input)
            };
            let error = &row[if legacy {
                "legacyError"
            } else {
                "canonicalError"
            }];
            if !error.is_null() {
                assert_eq!(
                    result.unwrap_err().code,
                    error["message"]
                        .as_str()
                        .unwrap_or_else(|| error.as_str().unwrap())
                );
            } else {
                assert_eq!(
                    result.unwrap().as_str(),
                    row[if legacy {
                        "legacyJson"
                    } else {
                        "canonicalJson"
                    }]
                    .as_str()
                    .unwrap(),
                    "{}",
                    row["inputJson"]
                );
            }
        }
    }
    let operations = corpus["operations"].as_array().unwrap();
    assert_eq!(operations.len(), 15);
    for row in operations {
        let operation = row["operation"].as_str().unwrap();
        assert_eq!(
            canonical_guest_account_operation(operation),
            row["canonical"].as_str().unwrap()
        );
        assert_eq!(
            legacy_guest_account_operation(operation),
            row["legacy"].as_str().unwrap()
        );
    }
}
#[test]
fn malformed_and_oversized_json_never_become_a_receipt_identity() {
    for raw in [
        "",
        "undefined",
        "NaN",
        "Infinity",
        "01",
        "-01",
        "1.",
        "1e+",
        "[1,]",
        "{\"a\":1,}",
        "{\"a\" 1}",
        "[true false]",
        "\"\\x01\"",
        "\"\\uZZZZ\"",
        "\"unfinished",
        "true trailing",
        "\u{feff}null",
    ] {
        assert!(CommandJson::parse(raw).is_err(), "{raw}");
    }
    assert!(CommandJson::parse(&format!("{}null{}", "[".repeat(130), "]".repeat(130))).is_err());
    assert!(CommandJson::parse(&format!("\"{}\"", "x".repeat(2 * 1024 * 1024))).is_err());
}
