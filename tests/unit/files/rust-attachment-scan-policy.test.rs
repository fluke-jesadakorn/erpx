use erpx_server_files::public::attachment_scan::{
    SCAN_CLEANUP_HEADROOM_MILLIS, SCAN_LEASE_MILLIS, legacy_purpose, next_scan_attempt,
    preview_object_key, remaining_compute_millis,
};
use serde_json::Value;
fn b0() -> Value {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../fixtures/files/scan-lifecycle/b0-scan-intents.json"
    ))
    .expect("frozen B0 effect-intent receipt");
    assert_eq!(
        receipt["baseline"],
        "a5ed8737a2f05eff1ee9841fda4a094b3926d732"
    );
    assert_eq!(
        receipt["source"]["sha256"],
        "f96d38f7b3ad9b852890e07607bdabeff0bd30d7d7d32bcebbe7ac76d5de3da1"
    );
    assert_eq!(
        receipt["legacySource"]["sha256"],
        "5b62afc7ed709aa3c75a9991cee291b335fc6f500192f41f0f380d077c7a7926"
    );
    receipt
}

#[test]
fn exact_legacy_aliases_match_all_322_independent_b0_combinations() {
    let receipt = b0();
    let cases = receipt["legacy"].as_array().unwrap();
    assert_eq!(cases.len(), 322);
    let mut admitted = 0;
    for case in cases {
        let owner = case["owner"].as_str().unwrap();
        let purpose = case["purpose"].as_str().unwrap();
        let expected = case["result"].get("purposeKey").and_then(Value::as_str);
        assert_eq!(
            legacy_purpose(owner, purpose),
            expected,
            "{owner}/{purpose}"
        );
        admitted += usize::from(expected.is_some());
    }
    assert_eq!(admitted, 26);
    assert_eq!(
        legacy_purpose("invoice_payment", "bank_payment_slip"),
        Some("payment_slip")
    );
    assert_eq!(
        legacy_purpose("social_draft", "post-image"),
        Some("post_image")
    );
    assert_eq!(legacy_purpose("SUPPORT_TICKET", "support_evidence"), None);
}
#[test]
fn retries_match_actual_b0_settlement_effects_through_all_12_schema_attempts() {
    let receipt = b0();
    let cases = receipt["cases"].as_array().unwrap();
    let mut compared = 0;
    for case in cases.iter().filter(|c| {
        c["name"]
            .as_str()
            .unwrap()
            .starts_with("error-retry-attempt-")
    }) {
        let attempt = case["input"]["attempt"].as_i64().unwrap();
        let effects = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e[0] == "atomicEffects")
            .unwrap()[1]
            .as_array()
            .unwrap();
        let retry = effects
            .iter()
            .find(|e| e[0] == "stepAttachmentScanAttempts121");
        let expected = retry.map(|r| {
            (
                r[1]["arg3"].as_i64().unwrap(),
                r[1]["arg4"].as_i64().unwrap(),
            )
        });
        assert_eq!(
            next_scan_attempt(attempt, 1_800_000_000_000).unwrap(),
            expected,
            "attempt{attempt}"
        );
        compared += 1;
    }
    assert_eq!(compared, 12);
    // This API never implements interrupted recovery by incrementing an attempt;
    // it is only called by an actual error settlement.
    assert!(next_scan_attempt(0, 0).is_err());
    assert!(next_scan_attempt(13, 0).is_err());
    assert!(next_scan_attempt(1, i64::MAX).is_err());
    assert_eq!(next_scan_attempt(6, i64::MAX).unwrap(), None);
}
#[test]
fn preview_names_match_actual_b0_put_effects_and_do_not_strip_other_suffixes() {
    let receipt = b0();
    let cases = receipt["cases"].as_array().unwrap();
    let mut compared = 0;
    for case in cases {
        let Some(put) = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e[0] == "putImmutable")
        else {
            continue;
        };
        let original = case["input"]
            .get("objectKey")
            .and_then(Value::as_str)
            .unwrap_or("quarantine/owned/original");
        let checksum = put[1]["bytes"]["sha256"].as_str().unwrap();
        let media = case["input"]["preview"].as_str().unwrap();
        assert_eq!(
            preview_object_key(original, checksum, media).unwrap(),
            put[1]["key"].as_str().unwrap()
        );
        compared += 1;
    }
    assert_eq!(compared, 2);
    let sha = "a".repeat(64);
    assert_eq!(
        preview_object_key("owned/original-copy", &sha, "image/webp").unwrap(),
        format!("owned/original-copy/preview-{sha}.webp")
    );
    for media in ["image/png", "text/html", ""] {
        assert!(preview_object_key("owned/original", &sha, media).is_err());
    }
    assert!(preview_object_key("owned/original", &"A".repeat(64), "image/webp").is_err());
}
#[test]
fn remaining_budget_never_outlives_the_current_lease_cleanup_headroom() {
    assert_eq!(SCAN_LEASE_MILLIS, 60_000);
    assert_eq!(SCAN_CLEANUP_HEADROOM_MILLIS, 5_000);
    assert_eq!(remaining_compute_millis(60_000, 0).unwrap(), 55_000);
    assert_eq!(remaining_compute_millis(60_000, 54_999).unwrap(), 1);
    for elapsed in [55_000, 60_000, 60_001] {
        assert!(remaining_compute_millis(60_000, elapsed).is_err());
    }
    for elapsed in 0..55_000 {
        let budget = remaining_compute_millis(60_000, elapsed).unwrap();
        assert!(budget > 0 && budget <= 55_000);
        assert!(
            i128::from(elapsed) + i128::from(budget) + i128::from(SCAN_CLEANUP_HEADROOM_MILLIS)
                <= 60_000
        );
    }
    assert!(remaining_compute_millis(i64::MIN, i64::MAX).is_err());
}

#[test]
fn field_visit_withdrawal_preserves_bound_owner_and_replay_order() {
    use erpx_server_files::public::field_visit_link::{WithdrawalDecision, withdrawal_decision};
    assert_eq!(
        withdrawal_decision("bound", "creator", None, None, "other", "reason")
            .unwrap_err()
            .code,
        "BOUND_ATTACHMENT_IMMUTABLE"
    );
    assert_eq!(
        withdrawal_decision("draft", "creator", None, None, "other", "reason")
            .unwrap_err()
            .code,
        "ATTACHMENT_WITHDRAW_FORBIDDEN"
    );
    assert_eq!(
        withdrawal_decision(
            "withdrawn",
            "creator",
            Some("creator"),
            Some("reason"),
            "creator",
            "reason"
        )
        .unwrap(),
        WithdrawalDecision::Replay
    );
    assert_eq!(
        withdrawal_decision(
            "withdrawn",
            "creator",
            Some("creator"),
            Some("reason"),
            "creator",
            "different"
        )
        .unwrap_err()
        .code,
        "IDEMPOTENCY_CONFLICT"
    );
}
