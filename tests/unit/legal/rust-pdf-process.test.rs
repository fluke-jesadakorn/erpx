use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    time::{Duration, Instant},
};

use erpx_server_legal::public::{
    ApprovedCompanyTrustMaterial, AuthorizedPdfSnapshot, BoundedPdfVerifier, PDF_BUSINESS_LIMIT,
    PdfProcessLimits, PdfVerificationEvidence, PdfVerificationFailure, PdfVerificationJob,
    PdfVerificationRepository, PdfVerifierProcess, TrustMaterialKind, TrustMaterialState,
    ValidationState, inspect_company_pdf_for_processing, verify_company_pdf,
};
use erpx_server_shared::public::{Clock, CompanyId, HoldingId, RecordId, TimestampMillis, UserId};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

fn record(value: &str) -> RecordId {
    RecordId::new(value).unwrap()
}
fn user(value: &str) -> UserId {
    UserId::new(value).unwrap()
}
fn company(value: &str) -> CompanyId {
    CompanyId::new(value).unwrap()
}
fn holding(value: &str) -> HoldingId {
    HoldingId::new(value).unwrap()
}

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
            let directory = private_tempdir();
            let binary = directory.path().join("pdf-process-helper");
            let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/support/legal/pdf-process-helper.rs");
            let result = Command::new("rustc")
                .arg("--edition=2024")
                .arg("--crate-name=pdf_process_helper")
                .arg(source)
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            (directory, binary)
        })
        .1
}

fn snapshot(bytes: &[u8]) -> AuthorizedPdfSnapshot {
    AuthorizedPdfSnapshot {
        holding_id: holding("holding-a"),
        company_id: company("company-a"),
        document_version_id: record("version-a"),
        document_checksum_sha256: hex::encode(Sha256::digest(bytes)),
        authorization_revision: "authority-1".to_owned(),
        trust_revision: "trust-1".to_owned(),
        pdf: bytes.to_vec(),
        trust: Vec::new(),
    }
}

fn job() -> PdfVerificationJob {
    PdfVerificationJob {
        job_id: record("job-a"),
        holding_id: holding("holding-a"),
        company_id: company("company-a"),
        document_version_id: record("version-a"),
        requested_by: user("reviewer-a"),
    }
}

fn trusted_snapshot() -> AuthorizedPdfSnapshot {
    corpus_snapshot(
        include_str!("../../../fixtures/legal/pdf-signatures/conformance.v1.json"),
        "trusted_crl",
    )
}

fn corpus_snapshot(source: &str, name: &str) -> AuthorizedPdfSnapshot {
    let corpus: serde_json::Value = serde_json::from_str(source).unwrap();
    let case = corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap();
    let mut result = snapshot(&hex::decode(case["pdfHex"].as_str().unwrap()).unwrap());
    result.trust = case["trust"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let path = item["path"].as_str().unwrap();
            let content = hex::decode(item["hex"].as_str().unwrap()).unwrap();
            ApprovedCompanyTrustMaterial {
                id: record(&format!("material-{index}")),
                company_id: company("company-a"),
                version: 1,
                kind: if path.starts_with("roots/") {
                    TrustMaterialKind::Root
                } else if path.ends_with(".crl") {
                    TrustMaterialKind::Crl
                } else if path.ends_with(".ocsp") {
                    TrustMaterialKind::Ocsp
                } else {
                    TrustMaterialKind::Intermediate
                },
                state: TrustMaterialState::Approved,
                created_by: user("creator"),
                approved_by: Some(user("independent-reviewer")),
                independent_person_review: true,
                checksum_sha256: hex::encode(Sha256::digest(&content)),
                content,
            }
        })
        .collect();
    result
}

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1_793_080_800_000)
    }
}

fn verifier(binary: &Path, root: &Path, millis: u64, bytes: usize) -> BoundedPdfVerifier {
    BoundedPdfVerifier::new(
        binary,
        root,
        PdfProcessLimits::new(Duration::from_millis(millis), bytes).unwrap(),
    )
    .unwrap()
}

fn process_running(pid: &str) -> bool {
    Command::new("/bin/kill")
        .arg("-0")
        .arg(pid)
        .output()
        .unwrap()
        .status
        .success()
}

fn assert_clean(root: &Path) {
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        assert!(
            !entry.file_name().to_string_lossy().starts_with("erpx-pdf-"),
            "owned temporary directory was retained"
        );
    }
    if let Ok(pid) = fs::read_to_string(root.join("child.pid")) {
        assert!(
            !process_running(&pid),
            "owned child is still alive or unreaped"
        );
    }
}

async fn await_child(root: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.join("child.pid").is_file() {
        assert!(Instant::now() < deadline, "helper did not start");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn actual_rust_child_verifies_and_binds_approved_company_material() {
    let root = private_tempdir();
    let worker = verifier(
        Path::new(env!("CARGO_BIN_EXE_erpx-validate-pdf")),
        root.path(),
        90_000,
        2 * 1024 * 1024,
    );
    let mut input = trusted_snapshot();
    // Match database DER storage rather than only the oracle's prepared PEM.
    for material in &mut input.trust {
        if material.kind == TrustMaterialKind::Root {
            material.content = openssl::x509::X509::from_pem(&material.content)
                .unwrap()
                .to_der()
                .unwrap();
            material.checksum_sha256 = hex::encode(Sha256::digest(&material.content));
        }
    }
    let expected_checksum = input.document_checksum_sha256.clone();
    let count = input.trust.len();
    let evidence = worker.verify(input, FixedClock.now()).await.unwrap();
    assert_eq!(evidence.report.status, ValidationState::Valid);
    assert_eq!(
        evidence.snapshot.document_checksum_sha256,
        expected_checksum
    );
    assert_eq!(evidence.snapshot.trust.len(), count);
    assert_clean(root.path());
}

#[tokio::test]
async fn required_path_proof_is_distinct_from_the_legacy_soft_diagnostic() {
    let source =
        include_str!("../../../fixtures/legal/pdf-signatures/conformance-crl-scope.v1.json");
    let root = private_tempdir();
    let worker = verifier(
        Path::new(env!("CARGO_BIN_EXE_erpx-validate-pdf")),
        root.path(),
        90_000,
        2 * 1024 * 1024,
    );
    for (name, expected) in [
        ("reason_partition_complete_union", ValidationState::Valid),
        (
            "reason_partition_incomplete",
            ValidationState::Indeterminate,
        ),
    ] {
        let result = worker
            .verify(corpus_snapshot(source, name), FixedClock.now())
            .await
            .unwrap();
        assert_eq!(result.report.status, expected, "{name}");
        assert_eq!(
            result.report.signatures[0].certificate_chain,
            ValidationState::Indeterminate
        );
        assert_eq!(
            result.report.signatures[0].required_certificate_chain,
            expected
        );
        assert_clean(root.path());
    }
}

#[tokio::test]
async fn child_protocol_refuses_valid_without_explicit_required_path_proof() {
    use erpx_server_legal::public::{OperatorTrustStore, TrustMaterial, validate_signed_pdf};
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/legal/pdf-signatures/conformance.v1.json"
    ))
    .unwrap();
    let case = corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "trusted_crl")
        .unwrap();
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
    let moment = time::OffsetDateTime::from_unix_timestamp(FixedClock.now().get() / 1000).unwrap();
    let valid = validate_signed_pdf(
        &hex::decode(case["pdfHex"].as_str().unwrap()).unwrap(),
        &trust,
        moment,
    );
    assert_eq!(valid.status, ValidationState::Valid);
    for missing in [true, false] {
        let root = private_tempdir();
        let worker = verifier(helper(), root.path(), 5_000, 2 * 1024 * 1024);
        let mut report = serde_json::to_value(&valid).unwrap();
        report["trustStoreDigest"] = hex::encode(Sha256::digest(b"[]")).into();
        if missing {
            report["signatures"][0]
                .as_object_mut()
                .unwrap()
                .remove("requiredCertificateChain");
        } else {
            report["signatures"][0]["requiredCertificateChain"] = "indeterminate".into();
        }
        let mut payload = b"report:".to_vec();
        payload.extend(serde_json::to_vec(&report).unwrap());
        assert_eq!(
            worker
                .verify(snapshot(&payload), FixedClock.now())
                .await
                .unwrap_err(),
            PdfVerificationFailure::WorkerProtocol
        );
        assert_clean(root.path());
    }
}

#[tokio::test]
async fn timeout_kills_and_reaps_only_the_owned_child_and_removes_files() {
    let root = private_tempdir();
    let worker = verifier(helper(), root.path(), 100, 2048);
    assert_eq!(
        worker
            .verify(snapshot(b"sleep"), FixedClock.now())
            .await
            .unwrap_err(),
        PdfVerificationFailure::WorkerTimeout
    );
    assert_clean(root.path());
}

#[tokio::test]
#[cfg(target_os = "macos")]
async fn resident_memory_overflow_kills_and_reaps_the_owned_rust_helper() {
    let root = private_tempdir();
    let limits = PdfProcessLimits::new(Duration::from_secs(5), 2048)
        .unwrap()
        .with_memory_limit(16 * 1024 * 1024)
        .unwrap();
    let worker = BoundedPdfVerifier::new(helper(), root.path(), limits).unwrap();
    assert_eq!(
        worker
            .verify(snapshot(b"memory"), FixedClock.now())
            .await
            .unwrap_err(),
        PdfVerificationFailure::WorkerMemoryLimit
    );
    assert_clean(root.path());
}

#[tokio::test]
async fn stdout_and_stderr_are_bounded_and_child_failure_never_passes() {
    for (mode, expected) in [
        (
            b"stdout".as_slice(),
            PdfVerificationFailure::WorkerOutputLimit,
        ),
        (b"stderr", PdfVerificationFailure::WorkerOutputLimit),
        (b"exit", PdfVerificationFailure::WorkerFailed),
        (b"invalid", PdfVerificationFailure::WorkerProtocol),
    ] {
        let root = private_tempdir();
        let worker = verifier(helper(), root.path(), 5_000, 2048);
        assert_eq!(
            worker
                .verify(snapshot(mode), FixedClock.now())
                .await
                .unwrap_err(),
            expected
        );
        assert_clean(root.path());
    }
}

#[tokio::test]
async fn caller_cancellation_and_busy_submission_cannot_abandon_a_child() {
    let root = private_tempdir();
    let worker = verifier(helper(), root.path(), 90_000, 2048);
    let caller_worker = worker.clone();
    let caller = tokio::spawn(async move {
        caller_worker
            .verify(snapshot(b"sleep"), FixedClock.now())
            .await
    });
    await_child(root.path()).await;
    assert_eq!(
        worker
            .verify(snapshot(b"sleep"), FixedClock.now())
            .await
            .unwrap_err(),
        PdfVerificationFailure::WorkerUnavailable
    );
    caller.abort();
    let _ = caller.await;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let pid = fs::read_to_string(root.path().join("child.pid")).unwrap();
        let directories = fs::read_dir(root.path())
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().starts_with("erpx-pdf-"));
        if !directories && !process_running(&pid) {
            break;
        }
        assert!(Instant::now() < deadline, "cancelled child or files remain");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_clean(root.path());
}

#[tokio::test]
async fn runtime_shutdown_drains_child_cleanup_and_refuses_new_work() {
    let root = private_tempdir();
    let worker = verifier(helper(), root.path(), 90_000, 2048);
    let active = worker.clone();
    let caller =
        tokio::spawn(async move { active.verify(snapshot(b"sleep"), FixedClock.now()).await });
    await_child(root.path()).await;
    worker.shutdown_and_drain().await;
    assert_eq!(
        caller.await.unwrap().unwrap_err(),
        PdfVerificationFailure::WorkerCancelled
    );
    assert_clean(root.path());
    assert_eq!(
        worker
            .verify(snapshot(b"sleep"), FixedClock.now())
            .await
            .unwrap_err(),
        PdfVerificationFailure::WorkerUnavailable
    );
}

#[tokio::test]
async fn outer_admission_bounds_all_material_before_child_creation() {
    let root = private_tempdir();
    let worker = verifier(helper(), root.path(), 5_000, 2048);
    let mut input = trusted_snapshot();
    let mut material = input.trust[0].clone();
    material.content = vec![0; 4 * 1024 * 1024];
    material.checksum_sha256 = hex::encode(Sha256::digest(&material.content));
    input.trust = (0..8)
        .map(|index| {
            let mut item = material.clone();
            item.id = record(&format!("material-{index}"));
            item
        })
        .collect();
    assert_eq!(
        worker.verify(input, FixedClock.now()).await.unwrap_err(),
        PdfVerificationFailure::AdmissionLimit
    );
    assert!(!root.path().join("child.pid").exists());
    assert_clean(root.path());
}

#[tokio::test]
async fn child_has_private_files_and_no_inherited_service_environment() {
    let root = private_tempdir();
    let worker = verifier(helper(), root.path(), 5_000, 2048);
    assert_eq!(
        worker
            .verify(snapshot(b"inspect"), FixedClock.now())
            .await
            .unwrap_err(),
        PdfVerificationFailure::WorkerProtocol
    );
    assert_eq!(fs::read(root.path().join("inspected")).unwrap(), b"ok");
    assert_clean(root.path());
}

#[tokio::test]
async fn authority_checksum_and_business_limits_fail_before_spawning() {
    let root = private_tempdir();
    let worker = verifier(helper(), root.path(), 5_000, 2048);
    let mut input = trusted_snapshot();
    input.trust[0].independent_person_review = false;
    assert_eq!(
        worker.verify(input, FixedClock.now()).await.unwrap_err(),
        PdfVerificationFailure::UnapprovedTrust
    );
    let mut input = trusted_snapshot();
    input.trust[0].state = TrustMaterialState::Retired;
    assert_eq!(
        worker.verify(input, FixedClock.now()).await.unwrap_err(),
        PdfVerificationFailure::UnapprovedTrust
    );
    let mut input = trusted_snapshot();
    input.trust[0].company_id = company("another-company");
    assert_eq!(
        worker.verify(input, FixedClock.now()).await.unwrap_err(),
        PdfVerificationFailure::InvalidScope
    );
    let mut input = trusted_snapshot();
    input.trust[0].content.push(0);
    assert_eq!(
        worker.verify(input, FixedClock.now()).await.unwrap_err(),
        PdfVerificationFailure::TrustChecksum
    );
    let mut input = snapshot(b"sleep");
    input.pdf.push(0);
    assert_eq!(
        worker.verify(input, FixedClock.now()).await.unwrap_err(),
        PdfVerificationFailure::InvalidSnapshot
    );
    assert_eq!(
        worker
            .verify(snapshot(&vec![0; PDF_BUSINESS_LIMIT + 1]), FixedClock.now())
            .await
            .unwrap_err(),
        PdfVerificationFailure::DocumentSizeLimit
    );
    assert!(!root.path().join("child.pid").exists());
    assert_clean(root.path());
}

struct Repository {
    snapshot: AuthorizedPdfSnapshot,
    changed: bool,
    retained: bool,
}
impl PdfVerificationRepository for Repository {
    type Error = PdfVerificationFailure;
    async fn load_authorized(
        &mut self,
        _: &PdfVerificationJob,
    ) -> Result<AuthorizedPdfSnapshot, PdfVerificationFailure> {
        Ok(self.snapshot.clone())
    }
    async fn retain_if_current(
        &mut self,
        _: &PdfVerificationJob,
        mut evidence: PdfVerificationEvidence,
    ) -> Result<PdfVerificationEvidence, PdfVerificationFailure> {
        if self.changed {
            return Err(PdfVerificationFailure::SnapshotChanged);
        }
        assert_eq!(
            evidence.snapshot.document_checksum_sha256,
            self.snapshot.document_checksum_sha256
        );
        self.retained = true;
        evidence.inspection_id = Some(record("retained-inspection"));
        Ok(evidence)
    }
}

#[tokio::test]
async fn application_requires_exact_scope_and_a_current_retention_transaction() {
    let root = private_tempdir();
    let worker = verifier(
        Path::new(env!("CARGO_BIN_EXE_erpx-validate-pdf")),
        root.path(),
        90_000,
        2 * 1024 * 1024,
    );
    let mut repository = Repository {
        snapshot: trusted_snapshot(),
        changed: true,
        retained: false,
    };
    assert_eq!(
        verify_company_pdf(&mut repository, &worker, &FixedClock, &job())
            .await
            .unwrap_err(),
        PdfVerificationFailure::SnapshotChanged
    );
    assert!(!repository.retained);
    repository.changed = false;
    assert_eq!(
        verify_company_pdf(&mut repository, &worker, &FixedClock, &job())
            .await
            .unwrap()
            .report
            .status,
        ValidationState::Valid
    );
    assert!(repository.retained);
    repository.snapshot.holding_id = holding("different-holding");
    assert_eq!(
        verify_company_pdf(&mut repository, &worker, &FixedClock, &job())
            .await
            .unwrap_err(),
        PdfVerificationFailure::InvalidScope
    );
    assert_clean(root.path());
}

struct FailingVerifier(PdfVerificationFailure);
impl PdfVerifierProcess for FailingVerifier {
    async fn verify(
        &self,
        _: AuthorizedPdfSnapshot,
        _: TimestampMillis,
    ) -> Result<PdfVerificationEvidence, PdfVerificationFailure> {
        Err(self.0)
    }
}

#[tokio::test]
async fn processing_retains_tool_failure_as_unknown_only_after_current_snapshot_revalidation() {
    let root = private_tempdir();
    let worker = verifier(helper(), root.path(), 100, 2048);
    let mut repository = Repository {
        snapshot: snapshot(b"sleep"),
        changed: false,
        retained: false,
    };
    let result = inspect_company_pdf_for_processing(&mut repository, &worker, &FixedClock, &job())
        .await
        .unwrap();
    assert!(repository.retained);
    assert!(result.inspection_id.is_some());
    assert_eq!(result.report.status, ValidationState::Indeterminate);
    assert!(result.report.signatures.is_empty());
    assert!(
        result
            .report
            .reasons
            .iter()
            .any(|reason| reason == "VERIFIER_DEADLINE_EXCEEDED")
    );
    assert!(
        result.report.trust_store_digest.is_empty(),
        "no successful child digest may be invented"
    );
    assert_clean(root.path());
    repository.changed = true;
    repository.retained = false;
    assert_eq!(
        inspect_company_pdf_for_processing(
            &mut repository,
            &FailingVerifier(PdfVerificationFailure::WorkerFailed),
            &FixedClock,
            &job()
        )
        .await
        .unwrap_err(),
        PdfVerificationFailure::SnapshotChanged
    );
    assert!(!repository.retained);
    repository.changed = false;
    for failure in [
        PdfVerificationFailure::WorkerCancelled,
        PdfVerificationFailure::TrustChecksum,
        PdfVerificationFailure::UnapprovedTrust,
    ] {
        assert_eq!(
            inspect_company_pdf_for_processing(
                &mut repository,
                &FailingVerifier(failure),
                &FixedClock,
                &job()
            )
            .await
            .unwrap_err(),
            failure
        );
        assert!(!repository.retained);
    }
}

#[test]
fn operator_configuration_cannot_raise_ceilings_or_use_a_public_temp_root() {
    assert!(PdfProcessLimits::new(Duration::from_secs(91), 2048).is_err());
    assert!(PdfProcessLimits::new(Duration::from_secs(1), 2 * 1024 * 1024 + 1).is_err());
    assert!(
        BoundedPdfVerifier::new(helper(), Path::new("/tmp"), PdfProcessLimits::default()).is_err()
    );
    assert!(
        BoundedPdfVerifier::new(
            Path::new("relative"),
            Path::new("/tmp"),
            PdfProcessLimits::default()
        )
        .is_err()
    );
}
#[tokio::test]
async fn a_reduced_pdf_lease_budget_kills_and_reaps_before_returning() {
    let root = private_tempdir();
    let worker = verifier(helper(), root.path(), 5000, 2 * 1024 * 1024);
    assert!(erpx_server_legal::public::PdfComputeBudget::new(90_001).is_err());
    let result = worker
        .verify_with_budget(
            snapshot(b"sleep"),
            FixedClock.now(),
            erpx_server_legal::public::PdfComputeBudget::new(300).unwrap(),
        )
        .await;
    assert!(matches!(result, Err(PdfVerificationFailure::WorkerTimeout)));
    assert_clean(root.path());
}
#[tokio::test]
async fn an_adapter_without_reduced_deadline_support_refuses_instead_of_ignoring_it() {
    struct Legacy(std::sync::atomic::AtomicUsize);
    impl PdfVerifierProcess for Legacy {
        async fn verify(
            &self,
            _: AuthorizedPdfSnapshot,
            _: TimestampMillis,
        ) -> Result<PdfVerificationEvidence, PdfVerificationFailure> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(PdfVerificationFailure::WorkerUnavailable)
        }
    }
    let legacy = Legacy(std::sync::atomic::AtomicUsize::new(0));
    assert!(matches!(
        legacy
            .verify_with_budget(
                snapshot(b"data"),
                FixedClock.now(),
                erpx_server_legal::public::PdfComputeBudget::new(300).unwrap()
            )
            .await,
        Err(PdfVerificationFailure::WorkerTimeout)
    ));
    assert_eq!(legacy.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(matches!(
        legacy
            .verify_with_budget(
                snapshot(b"data"),
                FixedClock.now(),
                erpx_server_legal::public::PdfComputeBudget::maximum()
            )
            .await,
        Err(PdfVerificationFailure::WorkerUnavailable)
    ));
    assert_eq!(legacy.0.load(std::sync::atomic::Ordering::SeqCst), 1);
}
#[tokio::test]
async fn actual_pdf_operational_probe_is_unsigned_indeterminate_and_grants_no_company_trust() {
    let root = private_tempdir();
    let binary = Path::new(env!("CARGO_BIN_EXE_erpx-validate-pdf"));
    let worker = verifier(binary, root.path(), 5000, 2 * 1024 * 1024);
    let proof = worker.probe_readiness(&FixedClock).await.unwrap();
    assert_eq!(proof.checked_at(), FixedClock.now().get());
    assert_eq!(proof.probe_outcome(), "unsigned-indeterminate");
    assert_eq!(proof.profile().sha256().len(), 64);
    assert_eq!(proof.profile().probe_sha256().len(), 64);
    assert_eq!(
        proof.profile().binary_sha256(),
        hex::encode(Sha256::digest(fs::read(binary).unwrap()))
    );
    assert_eq!(
        worker.current_provider_profile().await.unwrap().sha256(),
        proof.profile().sha256()
    );
    let json = serde_json::to_string(&proof).unwrap();
    assert!(!json.contains(root.path().to_str().unwrap()));
    assert!(!json.contains("companyId"));
    assert!(!json.contains("trustMaterials"));
    assert!(worker.can_accept_work());
    assert_clean(root.path());
    worker.shutdown_and_drain().await;
    assert!(!worker.can_accept_work());
    assert!(worker.probe_readiness(&FixedClock).await.is_err());
    assert!(worker.current_provider_profile().await.is_err());
}
#[tokio::test]
async fn pdf_operational_probe_requires_real_child_protocol_and_respects_busy_ownership() {
    let invalid_root = private_tempdir();
    let invalid = verifier(helper(), invalid_root.path(), 5000, 2 * 1024 * 1024);
    assert!(matches!(
        invalid.probe_readiness(&FixedClock).await,
        Err(PdfVerificationFailure::WorkerProtocol)
    ));
    assert_clean(invalid_root.path());
    let root = private_tempdir();
    let worker = verifier(helper(), root.path(), 5000, 2 * 1024 * 1024);
    let active = worker.clone();
    let request =
        tokio::spawn(async move { active.verify(snapshot(b"sleep"), FixedClock.now()).await });
    await_child(root.path()).await;
    assert!(!worker.can_accept_work());
    assert!(matches!(
        worker.probe_readiness(&FixedClock).await,
        Err(PdfVerificationFailure::WorkerUnavailable)
    ));
    request.abort();
    let _ = request.await;
    worker.shutdown_and_drain().await;
    assert_clean(root.path());
}
#[tokio::test]
async fn pdf_operational_profile_binds_limits_and_refuses_unsafe_resource_roots() {
    let root = private_tempdir();
    let one = verifier(helper(), root.path(), 5000, 2048);
    let two = verifier(helper(), root.path(), 1000, 2048);
    assert_ne!(
        one.current_provider_profile().await.unwrap().sha256(),
        two.current_provider_profile().await.unwrap().sha256()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(one.current_provider_profile().await.is_err());
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    }
    assert_clean(root.path());
}
