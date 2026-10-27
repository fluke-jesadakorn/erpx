use base64::{Engine, engine::general_purpose::STANDARD};
use erpx_server_finance::public::journal_review_wire::*;
use erpx_server_shared::public::{CommandKey, RecordId};
#[test]
fn every_actual_b0_node_body_schema_result_matches_exact_normalized_values_and_errors() {
    let oracle: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/finance/rust-journal-wire/conformance.v1.json"
    )))
    .unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 45);
    for row in cases {
        let bytes = STANDARD.decode(row["base64"].as_str().unwrap()).unwrap();
        let result = if row["kind"] == "post" {
            normalize_journal_post(&bytes).map(|v| v.normalized_json().to_owned())
        } else {
            normalize_journal_reverse(&bytes).map(|v| v.normalized_json().to_owned())
        };
        if let Some(expected) = row["expected"]["normalized"].as_str() {
            assert_eq!(result.unwrap(), expected, "{}", row["id"])
        } else {
            let error = result.unwrap_err();
            assert_eq!(
                error.status,
                row["expected"]["status"].as_u64().unwrap() as u16,
                "{}",
                row["id"]
            );
            assert_eq!(error.error.code, row["expected"]["code"], "{}", row["id"]);
            assert_eq!(
                error.error.message, row["expected"]["message"],
                "{}",
                row["id"]
            )
        }
    }
}
#[test]
fn accepted_unpaired_utf16_and_replacement_text_have_distinct_sealed_input_identity_but_same_utf8_storage()
 {
    let unpaired = normalize_journal_reverse(
        br#"{"periodId":"period","entryDate":"2026-01-01","memo":"\ud800"}"#,
    )
    .unwrap();
    let replacement = normalize_journal_reverse(
        br#"{"periodId":"period","entryDate":"2026-01-01","memo":"\ufffd"}"#,
    )
    .unwrap();
    assert_ne!(unpaired.normalized_json(), replacement.normalized_json());
    let first = unpaired.into_command(
        CommandKey::new("same-key").unwrap(),
        RecordId::new("journal").unwrap(),
    );
    let second = replacement.into_command(
        CommandKey::new("same-key").unwrap(),
        RecordId::new("journal").unwrap(),
    );
    assert_eq!(first.command().input, second.command().input);
}
#[test]
fn absent_empty_and_null_memo_remain_distinct_and_unknown_fields_cannot_supply_a_hash_or_grant() {
    let absent =
        normalize_journal_reverse(br#"{"periodId":"period","entryDate":"2026-01-01"}"#).unwrap();
    let empty =
        normalize_journal_reverse(br#"{"periodId":"period","entryDate":"2026-01-01","memo":""}"#)
            .unwrap();
    assert_ne!(absent.normalized_json(), empty.normalized_json());
    assert!(
        normalize_journal_reverse(br#"{"periodId":"period","entryDate":"2026-01-01","memo":null}"#)
            .is_err()
    );
    assert_eq!(normalize_journal_post(br#"{"expectedVersion":1,"input":{"memo":"private"},"request_hash":"attacker","permissions":["*"]}"#).unwrap().normalized_json(),"{\"expectedVersion\":1}");
}
