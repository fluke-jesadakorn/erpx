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

#[test]
fn manual_create_wire_preserves_ordered_normalized_fields_and_forces_manual_source() {
    use erpx_server_finance::public::journal_create_wire::normalize_manual_journal_create;
    let input = br#"{ "lines":[{"accountId":"cash","debit":"1.00","credit":"0","dimensions":{"z":"last","1":"one","a":"first"}},{"accountId":"sales","debit":"0","credit":"1.00"}],"isOpening":true,"sourceId":"untrusted","sourceType":"fiscal_close","currency":"thb","entryDate":"2026-01-05","periodId":"period","chartId":"chart","ignored":1e400 }"#;
    let normalized = normalize_manual_journal_create(input).unwrap();
    assert_eq!(
        normalized.normalized().as_str(),
        r#"{"chartId":"chart","periodId":"period","entryDate":"2026-01-05","currency":"THB","memo":"","sourceType":"manual","isOpening":false,"lines":[{"accountId":"cash","debit":"1.00","credit":"0","dimensions":{"1":"one","z":"last","a":"first"}},{"accountId":"sales","debit":"0","credit":"1.00"}]}"#
    );
    assert_eq!(normalized.draft().source_type, "manual");
    assert!(normalized.draft().source_id.is_none());
    assert!(!normalized.draft().is_opening);
    for input in [
        br#"{"chartId":"chart","periodId":"period","entryDate":"2026-01-05","currency":"THB","isOpening":null,"lines":[]}"#.as_slice(),
        br#"{"chartId":"chart","periodId":"period","entryDate":"2026-01-05","currency":"THB","lines":[{"accountId":"cash","debit":"1.0000000","credit":"0"},{"accountId":"sales","debit":"0","credit":"1"}]}"#.as_slice(),
        br#"{"chartId":"chart","periodId":"period","entryDate":"2026-01-05","currency":"THB","lines":[{"accountId":"cash","debit":"1","credit":"0","dimensions":{"bad":"\ud800"}},{"accountId":"sales","debit":"0","credit":"1"}]}"#.as_slice(),
    ] { assert!(normalize_manual_journal_create(input).is_err()); }
}

#[test]
fn manual_edit_wire_is_strict_and_normalizes_the_reason_and_defaults() {
    use erpx_server_finance::public::journal_create_wire::normalize_manual_journal_edit;
    let input = br#"{"chartId":"chart","periodId":"period","entryDate":"2026-01-05","currency":"thb","lines":[{"accountId":"cash","debit":"1","credit":"0"},{"accountId":"sales","debit":"0","credit":"1"}],"expectedVersion":1,"reason":"  correction made  "}"#;
    let edit = normalize_manual_journal_edit(input).unwrap();
    assert_eq!(edit.expected_version(), 1);
    assert_eq!(edit.reason(), "correction made");
    assert_eq!(
        edit.normalized().as_str(),
        r#"{"chartId":"chart","periodId":"period","entryDate":"2026-01-05","currency":"THB","memo":"","lines":[{"accountId":"cash","debit":"1","credit":"0"},{"accountId":"sales","debit":"0","credit":"1"}],"expectedVersion":1,"reason":"correction made"}"#
    );
    for field in ["fxRate", "sourceType", "sourceId", "isOpening", "unknown"] {
        let mut object: serde_json::Value = serde_json::from_slice(input).unwrap();
        object[field] = serde_json::Value::Bool(false);
        assert!(
            normalize_manual_journal_edit(serde_json::to_string(&object).unwrap().as_bytes())
                .is_err()
        );
    }
    let surrogate = br#"{"chartId":"chart","periodId":"period","entryDate":"2026-01-05","currency":"THB","lines":[{"accountId":"cash","debit":"1","credit":"0","ignored":1e400},{"accountId":"sales","debit":"0","credit":"1"}],"expectedVersion":1,"reason":"abcdefghij\ud800"}"#;
    let replacement = br#"{"chartId":"chart","periodId":"period","entryDate":"2026-01-05","currency":"THB","lines":[{"accountId":"cash","debit":"1","credit":"0"},{"accountId":"sales","debit":"0","credit":"1"}],"expectedVersion":1,"reason":"abcdefghij\ufffd"}"#;
    let first = normalize_manual_journal_edit(surrogate).unwrap();
    let second = normalize_manual_journal_edit(replacement).unwrap();
    assert_eq!(first.reason(), second.reason());
    assert_ne!(first.normalized(), second.normalized());
}
