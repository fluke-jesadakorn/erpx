use erpx_server_files::public::preview::{
    PREVIEW_INPUT_LIMIT, PreviewArtifact, PreviewComputeBudget, PreviewFailure, PreviewInput,
    PreviewMedia, PreviewResult, PreviewSourceMedia,
};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/files/preview")
            .join(name),
    )
    .unwrap()
}
fn corpus() -> serde_json::Value {
    serde_json::from_slice(&fixture("b0-results.json")).unwrap()
}
#[test]
fn byte_admission_matches_retained_owner_policy_and_b0_artifacts() {
    let d = corpus();
    assert_eq!(d["node"], "v26.11.1");
    assert_eq!(
        d["source"]["sha256"],
        "8b7b5fdeb6621771a00557b5f9b9e05d205a10a275951015c95146825a5f2b29"
    );
    let cases = d["outcomes"].as_array().unwrap();
    assert_eq!(cases.len(), 22);
    for row in cases {
        let bytes = fixture(row["input"]["file"].as_str().unwrap());
        let sha = hex::encode(Sha256::digest(&bytes));
        assert_eq!(sha, row["input"]["sha256"]);
        let media = PreviewSourceMedia::try_from(row["mime"].as_str().unwrap()).unwrap();
        let input = PreviewInput::new(bytes, media, &sha);
        assert_eq!(
            input.is_ok(),
            row["detected"] == row["mime"],
            "{}",
            row["id"]
        );
        if row["preview"]["kind"] == "rendered" {
            let expected = &row["preview"];
            let bytes = fixture(expected["file"].as_str().unwrap());
            let media = match expected["mediaType"].as_str().unwrap() {
                "image/webp" => PreviewMedia::Webp,
                "image/jpeg" => PreviewMedia::Jpeg,
                _ => panic!(),
            };
            let artifact = PreviewArtifact::from_rendered(bytes, media).unwrap();
            assert_eq!(artifact.width() as u64, expected["width"].as_u64().unwrap());
            assert_eq!(
                artifact.height() as u64,
                expected["height"].as_u64().unwrap()
            );
            assert_eq!(
                artifact.checksum_sha256(),
                expected["sha256"].as_str().unwrap()
            );
        }
    }
}
#[test]
fn mime_and_checksum_never_become_scan_or_authorization_proof() {
    let bytes = fixture("png-exif.input");
    let sha = hex::encode(Sha256::digest(&bytes));
    assert!(matches!(
        PreviewInput::new(bytes.clone(), PreviewSourceMedia::Jpeg, &sha),
        Err(PreviewFailure::SourceMedia)
    ));
    assert!(matches!(
        PreviewInput::new(bytes.clone(), PreviewSourceMedia::Png, &"0".repeat(64)),
        Err(PreviewFailure::SourceChecksum)
    ));
    assert!(PreviewSourceMedia::try_from("image/gif").is_err());
    assert!(PreviewSourceMedia::try_from("image/png; charset=utf-8").is_err());
    assert!(PreviewComputeBudget::new(40_001).is_err());
    assert!(PreviewComputeBudget::new(0).is_err());
    let large = vec![b'x'; PREVIEW_INPUT_LIMIT + 1];
    let sha = hex::encode(Sha256::digest(&large));
    assert!(matches!(
        PreviewInput::new(large, PreviewSourceMedia::Csv, &sha),
        Err(PreviewFailure::SourceSize)
    ));
}
#[test]
fn output_types_and_none_preserve_actual_source_capabilities() {
    for m in [
        PreviewSourceMedia::Png,
        PreviewSourceMedia::Jpeg,
        PreviewSourceMedia::Webp,
        PreviewSourceMedia::Heic,
        PreviewSourceMedia::Heif,
        PreviewSourceMedia::Pdf,
    ] {
        assert!(PreviewResult::NoPreview.require_source_kind(m).is_err());
    }
    assert!(
        PreviewResult::NoPreview
            .require_source_kind(PreviewSourceMedia::Csv)
            .is_ok()
    );
    assert!(
        PreviewResult::NoPreview
            .require_source_kind(PreviewSourceMedia::Docx)
            .is_ok()
    );
    let image = PreviewResult::Rendered(
        PreviewArtifact::from_rendered(fixture("png-exif.webp"), PreviewMedia::Webp).unwrap(),
    );
    assert!(image.require_source_kind(PreviewSourceMedia::Png).is_ok());
    assert!(image.require_source_kind(PreviewSourceMedia::Pdf).is_err());
    assert!(image.require_source_kind(PreviewSourceMedia::Csv).is_err());
}
#[test]
fn independent_container_guard_rejects_retained_metadata_animation_and_dimension_lies() {
    let valid = fixture("png-exif.webp");
    for chunk in [*b"EXIF", *b"XMP ", *b"ICCP", *b"ANIM"] {
        let mut changed = valid.clone();
        changed.extend_from_slice(&chunk);
        changed.extend_from_slice(&2u32.to_le_bytes());
        changed.extend_from_slice(b"xx");
        let len = (changed.len() - 8) as u32;
        changed[4..8].copy_from_slice(&len.to_le_bytes());
        let result = PreviewArtifact::from_rendered(changed, PreviewMedia::Webp);
        assert!(result.is_err());
    }
    let mut huge = valid.clone();
    huge[26..28].copy_from_slice(&3000u16.to_le_bytes());
    assert!(matches!(
        PreviewArtifact::from_rendered(huge, PreviewMedia::Webp),
        Err(PreviewFailure::OutputFormat)
    ));
    for end in 0..valid.len() {
        assert!(PreviewArtifact::from_rendered(valid[..end].to_vec(), PreviewMedia::Webp).is_err());
    }
    let valid = fixture("pdf-first-page.jpg");
    let mut metadata = valid[..2].to_vec();
    metadata.extend_from_slice(&[0xff, 0xe1, 0, 4, b'x', b'x']);
    metadata.extend_from_slice(&valid[2..]);
    assert!(matches!(
        PreviewArtifact::from_rendered(metadata, PreviewMedia::Jpeg),
        Err(PreviewFailure::OutputMetadata)
    ));
    // APP metadata after the entropy-coded scan must not escape the guard.
    let mut after_scan = valid[..valid.len() - 2].to_vec();
    after_scan.extend_from_slice(&[0xff, 0xe3, 0, 4, b'x', b'x', 0xff, 0xd9]);
    assert!(matches!(
        PreviewArtifact::from_rendered(after_scan, PreviewMedia::Jpeg),
        Err(PreviewFailure::OutputMetadata)
    ));
    let mut trailer = valid;
    trailer.extend_from_slice(b"private-trailing-data");
    assert!(PreviewArtifact::from_rendered(trailer, PreviewMedia::Jpeg).is_err());
}
