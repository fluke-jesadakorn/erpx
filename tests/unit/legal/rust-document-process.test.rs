use erpx_server_legal::public::{
    BoundedDocumentExtractor, DocumentExtractionFailure as Failure, DocumentExtractionInput,
    DocumentExtractionSource, DocumentExtractor, DocumentProcessLimits, DocumentToolConfiguration,
    ExtractionStatus, LegalDocumentMediaType,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    time::{Duration, Instant},
};
use tempfile::TempDir;
fn private_tempdir() -> TempDir {
    let mut builder = tempfile::Builder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    builder.tempdir().unwrap()
}
fn helper() -> &'static Path {
    static HELPER: OnceLock<(TempDir, PathBuf)> = OnceLock::new();
    &HELPER
        .get_or_init(|| {
            let root = private_tempdir();
            let binary = root.path().join("document-helper");
            let output = Command::new("rustc")
                .args(["--edition=2024", "--crate-name=document_process_helper"])
                .arg(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../tests/support/legal/document-process-helper.rs"),
                )
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            (root, binary)
        })
        .1
}
fn tools() -> DocumentToolConfiguration {
    DocumentToolConfiguration {
        pdftotext: helper().into(),
        pdfinfo: helper().into(),
        pdftoppm: helper().into(),
        tesseract: helper().into(),
        tessdata: helper().parent().unwrap().into(),
        languages: "eng+tha".into(),
    }
}
fn worker(root: &Path, millis: u64, output: usize, memory: u64) -> BoundedDocumentExtractor {
    BoundedDocumentExtractor::new(
        helper(),
        root,
        tools(),
        DocumentProcessLimits::new(Duration::from_millis(millis), output, memory).unwrap(),
    )
    .unwrap()
}
fn input(bytes: &[u8]) -> DocumentExtractionInput {
    DocumentExtractionInput {
        checksum_sha256: hex::encode(Sha256::digest(bytes)),
        source: DocumentExtractionSource::Original {
            media_type: LegalDocumentMediaType::Docx,
            bytes: bytes.into(),
        },
    }
}
async fn await_descendants(root: &Path) {
    let start = Instant::now();
    while pid_paths(root).len() < 3 {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "descendants did not start"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
fn pid_paths(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "pid"))
        .map(|entry| entry.path())
        .collect()
}
async fn assert_clean(root: &Path) {
    for entry in fs::read_dir(root).unwrap().flatten() {
        assert!(
            !entry
                .file_name()
                .to_string_lossy()
                .starts_with("erpx-document-"),
            "owned temp retained"
        );
    }
    let start = Instant::now();
    loop {
        let live = pid_paths(root).iter().any(|path| {
            let pid = path.file_stem().unwrap();
            Command::new("/bin/kill")
                .arg("-0")
                .arg(pid)
                .output()
                .unwrap()
                .status
                .success()
        });
        if !live {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "owned descendant alive or unreaped"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
#[tokio::test]
async fn normal_exit_checks_private_input_and_environment_without_killing_unrelated_child() {
    let root = private_tempdir();
    let worker = worker(root.path(), 5000, 2 * 1024 * 1024, 512 * 1024 * 1024);
    let mut unrelated = Command::new(helper())
        .arg("descendant")
        .arg(root.path())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let unrelated_marker = root.path().join(format!("{}.pid", unrelated.id()));
    let report = worker.extract(input(b"inspect")).await.unwrap();
    assert_eq!(report.status, ExtractionStatus::Extracted);
    assert_eq!(report.text, "draft\n\n");
    assert!(report.requires_review);
    assert!(
        unrelated.try_wait().unwrap().is_none(),
        "unrelated process was signalled"
    );
    unrelated.kill().unwrap();
    unrelated.wait().unwrap();
    let _ = fs::remove_file(unrelated_marker);
    assert_clean(root.path()).await;
}
#[tokio::test]
async fn authored_inline_text_has_its_own_source_and_never_launches_a_file_parser() {
    let root = private_tempdir();
    let worker = worker(root.path(), 5000, 2 * 1024 * 1024, 512 * 1024 * 1024);
    let body = "ข้อความร่าง\nA & B 😀";
    let report = worker
        .extract(DocumentExtractionInput {
            checksum_sha256: hex::encode(Sha256::digest(body.as_bytes())),
            source: DocumentExtractionSource::InlineText { body: body.into() },
        })
        .await
        .unwrap();
    assert_eq!(report.text, body);
    assert_eq!(
        report.method,
        Some(erpx_server_legal::public::ExtractionMethod::Inline)
    );
    assert!(!report.page_mapping);
    assert!(report.requires_review);
    assert_eq!(report.pages[0].text, body);
    assert!(pid_paths(root.path()).is_empty());
    assert!(fs::read_dir(root.path()).unwrap().next().is_none());
}
#[tokio::test]
async fn deadline_kills_and_reaps_entire_nested_group() {
    let root = private_tempdir();
    let worker = worker(root.path(), 800, 2 * 1024 * 1024, 512 * 1024 * 1024);
    assert_eq!(
        worker.extract(input(b"nested")).await.unwrap_err(),
        Failure::DeadlineExceeded
    );
    assert_clean(root.path()).await;
}
#[tokio::test]
async fn reduced_lease_budget_is_owned_by_the_supervisor_until_nested_cleanup_finishes() {
    let root = private_tempdir();
    let worker = worker(root.path(), 5000, 2 * 1024 * 1024, 512 * 1024 * 1024);
    assert_eq!(
        worker
            .extract_with_budget(
                input(b"nested"),
                erpx_server_legal::public::DocumentComputeBudget::new(500).unwrap()
            )
            .await
            .unwrap_err(),
        Failure::DeadlineExceeded
    );
    assert!(pid_paths(root.path()).len() >= 3);
    assert_clean(root.path()).await;
}
#[tokio::test]
async fn a_missing_thai_model_is_unready_and_never_silently_probes_english_instead() {
    let root = private_tempdir();
    let mut config = tools();
    config.tessdata = root.path().into();
    config.languages = "eng+tha".into();
    std::fs::write(
        root.path().join("eng.traineddata"),
        b"synthetic English resource",
    )
    .unwrap();
    let worker = BoundedDocumentExtractor::new(
        helper(),
        root.path(),
        config,
        DocumentProcessLimits::default(),
    )
    .unwrap();
    assert!(matches!(
        worker.current_provider_profile().await,
        Err(Failure::InvalidToolConfiguration)
    ));
    assert!(pid_paths(root.path()).is_empty());
}
#[tokio::test]
async fn observed_provider_profile_binds_model_bytes_and_rejects_oversized_resources() {
    let root = private_tempdir();
    let mut config = tools();
    config.tessdata = root.path().into();
    config.languages = "eng".into();
    let path = root.path().join("eng.traineddata");
    std::fs::write(&path, b"synthetic model one").unwrap();
    let worker = BoundedDocumentExtractor::new(
        helper(),
        root.path(),
        config,
        DocumentProcessLimits::default(),
    )
    .unwrap();
    let before = worker.current_provider_profile().await.unwrap();
    std::fs::write(&path, b"synthetic model two").unwrap();
    assert_ne!(
        before.sha256(),
        worker.current_provider_profile().await.unwrap().sha256()
    );
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(256 * 1024 * 1024 + 1)
        .unwrap();
    assert!(matches!(
        worker.current_provider_profile().await,
        Err(Failure::InvalidToolConfiguration)
    ));
    assert!(pid_paths(root.path()).is_empty());
}
#[tokio::test]
async fn exited_group_leader_with_descendants_holding_pipes_is_still_owned() {
    let root = private_tempdir();
    let worker = worker(root.path(), 800, 2 * 1024 * 1024, 512 * 1024 * 1024);
    assert_eq!(
        worker.extract(input(b"root-exits")).await.unwrap_err(),
        Failure::DeadlineExceeded
    );
    assert_clean(root.path()).await;
}
#[tokio::test]
async fn exited_leader_with_closed_descendant_pipes_still_triggers_group_cleanup() {
    let root = private_tempdir();
    let worker = worker(root.path(), 5000, 2 * 1024 * 1024, 512 * 1024 * 1024);
    assert_eq!(
        worker
            .extract(input(b"root-exits-closed"))
            .await
            .unwrap_err(),
        Failure::WorkerProtocol
    );
    assert_clean(root.path()).await;
}
#[tokio::test]
async fn caller_cancellation_keeps_supervisor_until_all_descendants_are_gone() {
    let root = private_tempdir();
    let worker = worker(root.path(), 5000, 2 * 1024 * 1024, 512 * 1024 * 1024);
    let adapter = worker.clone();
    let request = tokio::spawn(async move { adapter.extract(input(b"nested")).await });
    await_descendants(root.path()).await;
    request.abort();
    let _ = request.await;
    worker.shutdown_and_drain().await;
    assert_clean(root.path()).await;
}
#[tokio::test]
async fn shutdown_refuses_new_jobs_and_drains_owned_group() {
    let root = private_tempdir();
    let worker = worker(root.path(), 5000, 2 * 1024 * 1024, 512 * 1024 * 1024);
    let adapter = worker.clone();
    let request = tokio::spawn(async move { adapter.extract(input(b"nested")).await });
    await_descendants(root.path()).await;
    worker.shutdown_and_drain().await;
    assert_eq!(
        request.await.unwrap().unwrap_err(),
        Failure::WorkerCancelled
    );
    assert_eq!(
        worker.extract(input(b"inspect")).await.unwrap_err(),
        Failure::WorkerUnavailable
    );
    assert_clean(root.path()).await;
}
#[tokio::test]
async fn combined_output_failure_exit_and_protocol_errors_leave_no_children() {
    for (mode, expected) in [
        (b"stdout".as_slice(), Failure::WorkerOutputLimit),
        (b"stderr".as_slice(), Failure::WorkerOutputLimit),
        (b"exit".as_slice(), Failure::WorkerFailed),
        (b"bad".as_slice(), Failure::WorkerProtocol),
    ] {
        let root = private_tempdir();
        let worker = worker(root.path(), 5000, 1024, 512 * 1024 * 1024);
        assert_eq!(worker.extract(input(mode)).await.unwrap_err(), expected);
        assert_clean(root.path()).await;
    }
}
#[tokio::test]
async fn aggregate_group_rss_limit_is_enforced() {
    let root = private_tempdir();
    let worker = worker(root.path(), 5000, 2 * 1024 * 1024, 16 * 1024 * 1024);
    assert_eq!(
        worker.extract(input(b"memory")).await.unwrap_err(),
        Failure::WorkerMemoryLimit
    );
    assert_clean(root.path()).await;
}
#[tokio::test]
async fn actual_rust_docx_child_preserves_frozen_oracle_and_clean_failure() {
    let root = private_tempdir();
    let worker = BoundedDocumentExtractor::new(
        Path::new(env!("CARGO_BIN_EXE_erpx-extract-document")),
        root.path(),
        tools(),
        DocumentProcessLimits::default(),
    )
    .unwrap();
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/legal/documents/docx-conformance.v1.json"
    ))
    .unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let bytes = hex::decode(case["docxHex"].as_str().unwrap()).unwrap();
        if case["expected"]["error"].is_string() {
            assert_eq!(
                worker.extract(input(&bytes)).await.unwrap_err(),
                Failure::DocxTextLimit
            );
        } else {
            let result = worker.extract(input(&bytes)).await.unwrap();
            assert_eq!(
                result.text,
                case["expected"]["text"].as_str().unwrap(),
                "{}",
                case["id"]
            );
        }
    }
    assert_clean(root.path()).await;
}
#[tokio::test]
async fn input_admission_and_busy_submission_never_launch_extra_group() {
    let root = private_tempdir();
    let worker = worker(root.path(), 5000, 2 * 1024 * 1024, 512 * 1024 * 1024);
    let mut bad = input(b"inspect");
    bad.checksum_sha256 = "0".repeat(64);
    assert_eq!(
        worker.extract(bad).await.unwrap_err(),
        Failure::InputChecksum
    );
    let adapter = worker.clone();
    let request = tokio::spawn(async move { adapter.extract(input(b"nested")).await });
    await_descendants(root.path()).await;
    assert_eq!(
        worker.extract(input(b"inspect")).await.unwrap_err(),
        Failure::WorkerUnavailable
    );
    request.abort();
    let _ = request.await;
    worker.shutdown_and_drain().await;
    assert_clean(root.path()).await;
}
#[tokio::test]
async fn aggregate_budget_wins_across_pages_and_never_promotes_partial_ocr() {
    let root = private_tempdir();
    let tool_root = private_tempdir();
    let native = tool_root.path().join("native-tool-helper");
    let output = Command::new("rustc")
        .args(["--edition=2024", "--crate-name=document_native_tool_helper"])
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/support/legal/document-native-tool-helper.rs"),
        )
        .arg("-o")
        .arg(&native)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for name in ["pdftotext", "pdfinfo", "pdftoppm", "tesseract"] {
        fs::hard_link(&native, tool_root.path().join(name)).unwrap();
    }
    fs::write(
        tool_root.path().join("eng.traineddata"),
        b"explicit fixture-only language",
    )
    .unwrap();
    let tools = DocumentToolConfiguration {
        pdftotext: tool_root.path().join("pdftotext"),
        pdfinfo: tool_root.path().join("pdfinfo"),
        pdftoppm: tool_root.path().join("pdftoppm"),
        tesseract: tool_root.path().join("tesseract"),
        tessdata: tool_root.path().into(),
        languages: "eng".into(),
    };
    let worker = BoundedDocumentExtractor::new(
        Path::new(env!("CARGO_BIN_EXE_erpx-extract-document")),
        root.path(),
        tools,
        DocumentProcessLimits::new(
            Duration::from_millis(900),
            2 * 1024 * 1024,
            512 * 1024 * 1024,
        )
        .unwrap(),
    )
    .unwrap();
    let mut input = input(b"explicit PDF tool fixture");
    input.source = DocumentExtractionSource::Original {
        media_type: LegalDocumentMediaType::Pdf,
        bytes: b"explicit PDF tool fixture".to_vec(),
    };
    assert_eq!(
        worker.extract(input).await.unwrap_err(),
        Failure::DeadlineExceeded
    );
    let completed = fs::read_to_string(root.path().join("completed-pages.log"))
        .unwrap()
        .lines()
        .count();
    assert!(
        completed > 0 && completed < 25,
        "fixture must prove multi-page partial work was discarded"
    );
    assert_clean(root.path()).await;
}
