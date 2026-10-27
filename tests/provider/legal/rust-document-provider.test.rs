//! Explicit provider lane: frozen host tool/language receipt, never daily unit
//! acceptance and never a claim that English OCR proves Thai OCR readiness.
use erpx_server_legal::public::{
    BoundedDocumentExtractor, DocumentExtractionFailure as Failure, DocumentExtractionInput,
    DocumentExtractionSource, DocumentExtractor, DocumentProcessLimits, DocumentToolConfiguration,
    ExtractionStatus, LegalDocumentMediaType,
};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

#[tokio::test]
async fn real_native_provider_matches_frozen_b0_text_pages_and_refusal() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/legal/documents/native-conformance.v1.json"
    ))
    .unwrap();
    let tools: DocumentToolConfiguration =
        serde_json::from_value(corpus["oracle"]["tools"].clone()).unwrap();
    for receipt in corpus["oracle"]["toolReceipts"].as_array().unwrap() {
        let path = receipt["path"].as_str().unwrap();
        let bytes = fs::read(path)
            .expect("provider lane requires the explicit captured native tool profile");
        assert_eq!(
            hex::encode(Sha256::digest(&bytes)),
            receipt["sha256"].as_str().unwrap(),
            "provider profile changed; preserve frozen B0 and capture a separate explicit tool profile: {path}"
        );
    }
    assert_eq!(
        hex::encode(Sha256::digest(
            fs::read(tools.tessdata.join("eng.traineddata")).unwrap()
        )),
        corpus["oracle"]["language"]["sha256"].as_str().unwrap()
    );
    let mut builder = tempfile::Builder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    let root = builder.tempdir().unwrap();
    let worker = BoundedDocumentExtractor::new(
        Path::new(env!("CARGO_BIN_EXE_erpx-extract-document")),
        root.path(),
        tools,
        DocumentProcessLimits::default(),
    )
    .unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let bytes = hex::decode(case["hex"].as_str().unwrap()).unwrap();
        let input = DocumentExtractionInput {
            checksum_sha256: case["sha256"].as_str().unwrap().into(),
            source: DocumentExtractionSource::Original {
                media_type: LegalDocumentMediaType::try_from(case["mediaType"].as_str().unwrap())
                    .unwrap(),
                bytes,
            },
        };
        let result = worker.extract(input).await;
        if case["expected"]["error"].is_string() {
            assert_eq!(result.unwrap_err(), Failure::ScannedPageLimit);
        } else {
            let report = result.unwrap();
            assert_eq!(report.status, ExtractionStatus::Extracted);
            assert_eq!(
                report.text,
                case["expected"]["text"].as_str().unwrap(),
                "{}",
                case["id"]
            );
            assert_eq!(
                serde_json::to_value(report.method).unwrap(),
                case["expected"]["method"]
            );
            assert_eq!(
                serde_json::to_value(report.pages).unwrap(),
                case["expected"]["pages"],
                "{}",
                case["id"]
            );
            assert!(report.requires_review);
        }
    }
    worker.shutdown_and_drain().await;
    assert!(fs::read_dir(root.path()).unwrap().next().is_none());
}
#[tokio::test]
async fn explicit_english_profile_proves_bounded_functional_probes_without_claiming_thai_readiness()
{
    use erpx_server_shared::public::{Clock, TimestampMillis};
    struct Fixed;
    impl Clock for Fixed {
        fn now(&self) -> TimestampMillis {
            TimestampMillis::new(123_000)
        }
    }
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/legal/documents/native-conformance.v1.json"
    ))
    .unwrap();
    let tools: DocumentToolConfiguration =
        serde_json::from_value(corpus["oracle"]["tools"].clone()).unwrap();
    let mut builder = tempfile::Builder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    let root = builder.tempdir().unwrap();
    let worker = BoundedDocumentExtractor::new(
        Path::new(env!("CARGO_BIN_EXE_erpx-extract-document")),
        root.path(),
        tools,
        DocumentProcessLimits::default(),
    )
    .unwrap();
    let proof = worker.probe_readiness(&Fixed).await.unwrap();
    assert_eq!(proof.checked_at(), 123_000);
    assert_eq!(proof.probes().len(), 5);
    assert_eq!(proof.profile().sha256().len(), 64);
    assert_eq!(
        worker.current_provider_profile().await.unwrap().sha256(),
        proof.profile().sha256()
    );
    let json = serde_json::to_value(&proof).unwrap();
    assert_eq!(json["profile"]["languages"], "eng");
    assert!(
        !serde_json::to_string(&proof)
            .unwrap()
            .contains("/opt/homebrew")
    );
    worker.shutdown_and_drain().await;
    assert!(!worker.can_accept_work());
    assert!(worker.probe_readiness(&Fixed).await.is_err());
    assert!(fs::read_dir(root.path()).unwrap().next().is_none());
}
