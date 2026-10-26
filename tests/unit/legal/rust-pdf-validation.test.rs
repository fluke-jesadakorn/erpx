use erpx_server_legal::public::{
    OperatorTrustStore, TrustMaterial, ValidationState, validate_signed_pdf,
};
use time::macros::datetime;

#[test]
fn malformed_pdf_never_claims_signature_validity() {
    let trust = OperatorTrustStore::from_materials(Vec::new()).expect("empty explicit trust store");
    let result = validate_signed_pdf(b"not a PDF", &trust, datetime!(2026-10-27 06:00 UTC));
    assert_eq!(result.status, ValidationState::Indeterminate);
    assert!(result.signatures.is_empty());
    assert_eq!(
        result.reasons.first().map(String::as_str),
        Some("VALIDATION_FAILED")
    );
}

#[test]
fn trust_store_refuses_path_traversal_and_private_keys() {
    for path in [
        "roots/../key.pem",
        "roots/private.key",
        "../root.pem",
        "roots/root.pem/extra",
    ] {
        assert!(
            OperatorTrustStore::from_materials(vec![TrustMaterial {
                path: path.to_owned(),
                content: Vec::new()
            }])
            .is_err()
        );
    }
}

#[test]
fn real_pdf_signatures_match_the_independent_b0_oracle() {
    assert_exact_cases(include_str!(
        "../../../fixtures/legal/pdf-signatures/conformance.v1.json"
    ));
}

#[test]
fn supported_seed_and_dss_policies_match_independent_b0_evidence() {
    assert_exact_cases(include_str!(
        "../../../fixtures/legal/pdf-signatures/conformance-policy.v1.json"
    ));
}

fn assert_exact_cases(source: &str) {
    let corpus: serde_json::Value = serde_json::from_str(source).expect("conformance corpus");
    let moment = time::OffsetDateTime::parse(
        corpus["validationTime"].as_str().unwrap(),
        &time::format_description::well_known::Rfc3339,
    )
    .unwrap();
    let mut mismatches = Vec::new();
    for case in corpus["cases"].as_array().unwrap() {
        if !case["conservativeGap"].is_null() {
            continue;
        }
        let pdf = hex::decode(case["pdfHex"].as_str().unwrap()).unwrap();
        let trust = OperatorTrustStore::from_materials(
            case["trust"]
                .as_array()
                .unwrap()
                .iter()
                .map(|material| TrustMaterial {
                    path: material["path"].as_str().unwrap().to_owned(),
                    content: hex::decode(material["hex"].as_str().unwrap()).unwrap(),
                })
                .collect(),
        )
        .unwrap();
        let actual = serde_json::to_value(validate_signed_pdf(&pdf, &trust, moment)).unwrap();
        let expected = &case["expected"];
        if actual["status"] != expected["status"] {
            mismatches.push(format!(
                "{} overall expected={} actual={} details={}",
                case["name"], expected["status"], actual["status"], actual
            ));
        }
        if actual["signatures"].as_array().unwrap().len()
            != expected["signatures"].as_array().unwrap().len()
        {
            mismatches.push(format!(
                "{} signature count differs: {}",
                case["name"], actual
            ));
            continue;
        }
        for (index, signature) in expected["signatures"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            for (key, value) in signature.as_object().unwrap() {
                if actual["signatures"][index][key] != *value {
                    mismatches.push(format!(
                        "{} signature{} {} expected={} actual={} details={}",
                        case["name"],
                        index,
                        key,
                        value,
                        actual["signatures"][index][key],
                        actual["signatures"][index]
                    ));
                }
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn document_timestamps_preserve_b0_unresolved_policy_without_partial_validity() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/legal/pdf-signatures/conformance-policy.v1.json"
    ))
    .unwrap();
    let moment = time::OffsetDateTime::parse(
        corpus["validationTime"].as_str().unwrap(),
        &time::format_description::well_known::Rfc3339,
    )
    .unwrap();
    let mut checked = 0;
    for case in corpus["cases"].as_array().unwrap() {
        if case["name"] != "document_timestamp" {
            continue;
        }
        let reason = "DOCUMENT_TIMESTAMP_POLICY_UNRESOLVED";
        let pdf = hex::decode(case["pdfHex"].as_str().unwrap()).unwrap();
        let trust = OperatorTrustStore::from_materials(
            case["trust"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| TrustMaterial {
                    path: item["path"].as_str().unwrap().to_owned(),
                    content: hex::decode(item["hex"].as_str().unwrap()).unwrap(),
                })
                .collect(),
        )
        .unwrap();
        let actual = validate_signed_pdf(&pdf, &trust, moment);
        assert_ne!(
            actual.status,
            ValidationState::Valid,
            "{}: oracle={} actual={actual:?}",
            case["name"],
            case["expected"]
        );
        assert!(actual.signatures.is_empty());
        assert!(
            actual
                .reasons
                .iter()
                .chain(
                    actual
                        .signatures
                        .iter()
                        .flat_map(|signature| &signature.reasons)
                )
                .any(|actual| actual == reason),
            "{} missing explicit gap reason {reason}: {actual:?}",
            case["name"]
        );
        checked += 1;
    }
    assert!(checked >= 1, "unsupported corpus was unexpectedly reduced");
}

#[test]
fn legacy_seed_forbidden_only_bug_never_becomes_a_valid_rust_result() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/legal/pdf-signatures/seed-forbidden-only-b0.v1.json"
    ))
    .unwrap();
    let case = &corpus["case"];
    assert_eq!(
        case["expected"]["status"], "valid",
        "independent B0 bug evidence changed; review policy"
    );
    let trust = OperatorTrustStore::from_materials(
        case["trust"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| TrustMaterial {
                path: item["path"].as_str().unwrap().to_owned(),
                content: hex::decode(item["hex"].as_str().unwrap()).unwrap(),
            })
            .collect(),
    )
    .unwrap();
    let pdf = hex::decode(case["pdfHex"].as_str().unwrap()).unwrap();
    use sha2::Digest;
    assert_eq!(hex::encode(sha2::Sha256::digest(&pdf)), corpus["pdfSha256"]);
    let actual = validate_signed_pdf(&pdf, &trust, datetime!(2026-10-27 06:00 UTC));
    assert_eq!(actual.status, ValidationState::Indeterminate);
    assert_eq!(actual.signatures[0].integrity, ValidationState::Valid);
    assert_eq!(actual.signatures[0].policy, ValidationState::Invalid);
    assert_eq!(
        actual.signatures[0].certificate_chain,
        ValidationState::Valid
    );
    assert_eq!(
        actual.signatures[0].certificate_validity,
        ValidationState::Valid
    );
    assert_eq!(actual.signatures[0].revocation, ValidationState::Valid);
    assert_eq!(actual.signatures[0].timestamp, ValidationState::Valid);
    assert!(
        actual.signatures[0]
            .reasons
            .iter()
            .any(|reason| reason == "SEED_VALUE_CONSTRAINTS_FAILURE")
    );
}

#[test]
fn ambiguous_crl_evidence_cannot_produce_valid_or_erase_proved_revocation() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/legal/pdf-signatures/conformance-crl-guards.v1.json"
    ))
    .unwrap();
    let moment = time::OffsetDateTime::parse(
        corpus["validationTime"].as_str().unwrap(),
        &time::format_description::well_known::Rfc3339,
    )
    .unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let pdf = hex::decode(case["pdfHex"].as_str().unwrap()).unwrap();
        let trust = OperatorTrustStore::from_materials(
            case["trust"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| TrustMaterial {
                    path: item["path"].as_str().unwrap().to_owned(),
                    content: hex::decode(item["hex"].as_str().unwrap()).unwrap(),
                })
                .collect(),
        )
        .unwrap();
        let report = validate_signed_pdf(&pdf, &trust, moment);
        let actual = serde_json::to_value(&report).unwrap();
        assert_eq!(
            actual["status"], case["guard"],
            "{} B0={} actual={actual}",
            case["name"], case["expected"]
        );
        assert_eq!(actual["signatures"][0]["integrity"], "valid");
        if case["guard"] == "indeterminate" {
            assert_eq!(
                report.signatures[0].revocation,
                ValidationState::Indeterminate
            );
            assert!(
                report.signatures[0]
                    .reasons
                    .iter()
                    .any(|reason| reason == "REVOCATION_EVIDENCE_INSUFFICIENT")
            );
        } else if case["guard"] == "invalid" {
            assert_eq!(report.signatures[0].revocation, ValidationState::Invalid);
        }
    }
    assert_exact_cases(include_str!(
        "../../../fixtures/legal/pdf-signatures/conformance-crl-guards.v1.json"
    ));
}
