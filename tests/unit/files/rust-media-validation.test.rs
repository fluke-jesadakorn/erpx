use erpx_server_files::public::media_signature::{
    detected_attachment_media_type, matches_upload_signature,
};

#[test]
fn byte_signature_policy_matches_unchanged_baseline_on_pinned_node() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/files/rust-media/conformance.v1.json"
    ))
    .unwrap();
    assert_eq!(corpus["node"], "v26.11.1");
    let cases = corpus["cases"].as_array().unwrap();
    assert!(cases.len() > 1000);
    for row in cases {
        let bytes = hex::decode(row["hex"].as_str().unwrap()).unwrap();
        let declared = row["declared"].as_str().unwrap();
        assert_eq!(
            detected_attachment_media_type(&bytes, declared),
            row["detected"].as_str(),
            "{}",
            row["name"]
        );
        assert_eq!(
            matches_upload_signature(declared, &bytes),
            row["matches"].as_bool().unwrap(),
            "{}",
            row["name"]
        );
    }
}
