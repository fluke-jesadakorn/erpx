use erpx_server_files::public::preview::{
    BoundedPreviewRenderer, PreviewComputeBudget, PreviewFailure, PreviewInput, PreviewRenderer,
    PreviewResult, PreviewSourceMedia, PreviewToolConfiguration,
};
use sha2::Digest;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/files/preview")
            .join(name),
    )
    .unwrap()
}
fn root() -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(t.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    t
}
fn tools() -> PreviewToolConfiguration {
    PreviewToolConfiguration::new(
        Path::new(
            &std::env::var("ERPX_HEIF_CONVERT").expect("qualified actual HEIF converter required"),
        ),
        Path::new(&std::env::var("ERPX_PDFTOPPM_BIN").expect("qualified actual Poppler required")),
    )
    .unwrap()
}
fn renderer(root: &Path) -> BoundedPreviewRenderer {
    BoundedPreviewRenderer::new(
        Path::new(env!("CARGO_BIN_EXE_erpx-render-preview")),
        root,
        tools(),
    )
    .unwrap()
}
#[tokio::test]
async fn actual_bounded_rust_child_matches_retained_b0_artifacts_and_rejections() {
    let d: serde_json::Value = serde_json::from_slice(&fixture("b0-results.json")).unwrap();
    assert_eq!(d["node"], "v26.11.1");
    let cases = d["outcomes"].as_array().unwrap();
    assert_eq!(cases.len(), 22);
    let root = root();
    let renderer = renderer(root.path());
    for row in cases {
        let bytes = fixture(row["input"]["file"].as_str().unwrap());
        let media = PreviewSourceMedia::try_from(row["mime"].as_str().unwrap()).unwrap();
        let input = PreviewInput::new(bytes, media, row["input"]["sha256"].as_str().unwrap());
        if row["detected"] != row["mime"] {
            assert!(
                matches!(input, Err(PreviewFailure::SourceMedia)),
                "{}",
                row["id"]
            );
            continue;
        }
        let result = renderer
            .render(input.unwrap(), PreviewComputeBudget::new(40_000).unwrap())
            .await;
        match row["preview"]["kind"].as_str().unwrap() {
            "rendered" => {
                let expected = &row["preview"];
                let PreviewResult::Rendered(artifact) =
                    result.unwrap_or_else(|f| panic!("{} failed {f:?}", row["id"]))
                else {
                    panic!("unexpected no-preview")
                };
                assert_eq!(
                    artifact.bytes(),
                    fixture(expected["file"].as_str().unwrap()),
                    "{}",
                    row["id"]
                );
                assert_eq!(artifact.media().as_mime(), expected["mediaType"]);
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
            "none" => assert!(matches!(result, Ok(PreviewResult::NoPreview))),
            "failed" => {
                let error = result
                    .err()
                    .unwrap_or_else(|| panic!("{} unexpectedly succeeded", row["id"]));
                assert_eq!(
                    error,
                    if row["preview"]["classification"] == "pixel-limit" {
                        PreviewFailure::PixelLimit
                    } else {
                        PreviewFailure::DecoderRejected
                    },
                    "{}",
                    row["id"]
                );
            }
            _ => panic!(),
        }
        assert_eq!(
            std::fs::read_dir(root.path()).unwrap().count(),
            0,
            "private files retained after {}",
            row["id"]
        );
    }
}
#[tokio::test]
async fn actual_operator_artifact_body_changes_are_observed_without_authority_claim() {
    let root = root();
    let original = tools();
    let heif = root.path().join("heif-convert");
    let pdf = root.path().join("pdftoppm");
    std::fs::copy(original.heif_convert(), &heif).unwrap();
    std::fs::copy(original.pdftoppm(), &pdf).unwrap();
    #[cfg(unix)]
    {
        // The installed tool is intentionally read-only; copy preserves0555.
        // Only this disposable owned copy is writable for the mutation test.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&heif, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let private = root.path().join("private");
    std::fs::create_dir(&private).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let renderer = BoundedPreviewRenderer::new(
        Path::new(env!("CARGO_BIN_EXE_erpx-render-preview")),
        &private,
        PreviewToolConfiguration::new(&heif, &pdf).unwrap(),
    )
    .unwrap();
    let before = renderer.current_provider_profile().await.unwrap();
    std::fs::OpenOptions::new()
        .append(true)
        .open(heif)
        .unwrap()
        .write_all(b"owned-artifact-fingerprint-change")
        .unwrap();
    let after = renderer.current_provider_profile().await.unwrap();
    assert_ne!(before, after);
    assert_eq!(before.sha256().len(), 64);
    renderer.request_shutdown();
    renderer.drain().await;
    assert!(matches!(
        renderer.current_provider_profile().await,
        Err(PreviewFailure::WorkerUnavailable)
    ));
}
#[tokio::test]
async fn relative_tools_and_nonprivate_work_roots_are_rejected() {
    assert!(
        PreviewToolConfiguration::new(Path::new("heif-convert"), Path::new("pdftoppm")).is_err()
    );
    let root = root();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            BoundedPreviewRenderer::new(
                Path::new(env!("CARGO_BIN_EXE_erpx-render-preview")),
                root.path(),
                tools()
            )
            .is_err()
        );
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    // A real non-worker process cannot supply decoder evidence. This exercises
    // both abnormal exit and empty successful output without replacing a codec.
    for program in ["/usr/bin/false", "/usr/bin/true"] {
        let worker = BoundedPreviewRenderer::new(Path::new(program), root.path(), tools()).unwrap();
        let input = fixture("png-exif.input");
        let checksum = hex::encode(sha2::Sha256::digest(&input));
        let input = PreviewInput::new(input, PreviewSourceMedia::Png, &checksum).unwrap();
        assert!(matches!(
            worker
                .render(input, PreviewComputeBudget::new(40_000).unwrap())
                .await,
            Err(PreviewFailure::WorkerProtocol)
        ));
    }
}
