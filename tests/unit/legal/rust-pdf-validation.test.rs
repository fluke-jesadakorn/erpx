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
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/legal/pdf-signatures/conformance.v1.json"
    ))
    .expect("conformance corpus");
    let moment = time::OffsetDateTime::parse(
        corpus["validationTime"].as_str().unwrap(),
        &time::format_description::well_known::Rfc3339,
    )
    .unwrap();
    let mut mismatches = Vec::new();
    for case in corpus["cases"].as_array().unwrap() {
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
