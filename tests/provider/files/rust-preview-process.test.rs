use erpx_server_files::public::preview::{
    BoundedPreviewRenderer, PreviewComputeBudget, PreviewFailure, PreviewInput, PreviewRenderer,
    PreviewSourceMedia, PreviewToolConfiguration,
};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
struct Fixture {
    _root: tempfile::TempDir,
    tools: PathBuf,
    work: PathBuf,
    entered: PathBuf,
    release: PathBuf,
    heartbeat: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let tools = root.path().join("tools");
        let work = root.path().join("work");
        std::fs::create_dir(&tools).unwrap();
        std::fs::create_dir(&work).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for path in [root.path(), tools.as_path(), work.as_path()] {
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
            }
        }
        let entered = root.path().join("entered");
        let release = root.path().join("release");
        let heartbeat = root.path().join("heartbeat");
        std::fs::copy(
            env!("CARGO_BIN_EXE_erpx-preview-tool-fixture"),
            tools.join("tool"),
        )
        .unwrap();
        std::fs::write(
            tools.join("fixture.json"),
            serde_json::to_vec(
                &serde_json::json!({"entered":entered,"release":release,"heartbeat":heartbeat}),
            )
            .unwrap(),
        )
        .unwrap();
        Self {
            _root: root,
            tools,
            work,
            entered,
            release,
            heartbeat,
        }
    }
    fn renderer(&self) -> BoundedPreviewRenderer {
        let tool = self.tools.join("tool");
        BoundedPreviewRenderer::new(
            Path::new(env!("CARGO_BIN_EXE_erpx-render-preview")),
            &self.work,
            PreviewToolConfiguration::new(&tool, &tool).unwrap(),
        )
        .unwrap()
    }
    fn input(&self) -> PreviewInput {
        let bytes = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/files/preview/pdf-first-page.input"),
        )
        .unwrap();
        let hash = hex::encode(Sha256::digest(&bytes));
        PreviewInput::new(bytes, PreviewSourceMedia::Pdf, &hash).unwrap()
    }
    async fn wait_entered(&self) {
        for _ in 0..1000 {
            if self.entered.exists() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        panic!("owned native fixture did not reach its compute barrier");
    }
    fn replace_tool(&self) {
        let replacement = self.tools.join("replacement");
        std::fs::copy(self.tools.join("tool"), &replacement).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&replacement, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::OpenOptions::new()
            .append(true)
            .open(&replacement)
            .unwrap()
            .write_all(b"changed operator artifact")
            .unwrap();
        std::fs::rename(replacement, self.tools.join("tool")).unwrap();
    }
    fn release(&self) {
        std::fs::write(&self.release, b"release").unwrap();
    }
    async fn assert_cleaned(&self) {
        assert_eq!(std::fs::read_dir(&self.work).unwrap().count(), 0);
        let tick = std::fs::read(&self.heartbeat).unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            tick,
            std::fs::read(&self.heartbeat).unwrap(),
            "owned native descendant kept running"
        );
    }
}
#[tokio::test]
async fn changed_tool_profile_wins_over_a_typed_failed_decode() {
    let fixture = Fixture::new();
    let renderer = fixture.renderer();
    let input = fixture.input();
    let task = {
        let renderer = renderer.clone();
        tokio::spawn(async move {
            renderer
                .render(input, PreviewComputeBudget::new(40_000).unwrap())
                .await
        })
    };
    fixture.wait_entered().await;
    fixture.replace_tool();
    fixture.release();
    let result = task.await.unwrap();
    fixture.assert_cleaned().await;
    assert!(
        matches!(result, Err(PreviewFailure::ToolConfiguration)),
        "changed tool was treated as a content verdict: {:?}",
        result.err()
    );
}
#[tokio::test]
async fn unchanged_tool_preserves_the_actual_typed_failed_decode() {
    let fixture = Fixture::new();
    let renderer = fixture.renderer();
    let input = fixture.input();
    let task = {
        let renderer = renderer.clone();
        tokio::spawn(async move {
            renderer
                .render(input, PreviewComputeBudget::new(40_000).unwrap())
                .await
        })
    };
    fixture.wait_entered().await;
    fixture.release();
    assert!(matches!(
        task.await.unwrap(),
        Err(PreviewFailure::DecoderRejected)
    ));
    fixture.assert_cleaned().await;
}
#[tokio::test]
async fn a_held_native_tool_obeys_the_reduced_aggregate_deadline() {
    let fixture = Fixture::new();
    let renderer = fixture.renderer();
    let input = fixture.input();
    let task = {
        let renderer = renderer.clone();
        tokio::spawn(async move {
            renderer
                .render(input, PreviewComputeBudget::new(5_000).unwrap())
                .await
        })
    };
    fixture.wait_entered().await;
    assert!(matches!(task.await.unwrap(), Err(PreviewFailure::Deadline)));
    fixture.assert_cleaned().await;
}
#[tokio::test]
async fn caller_cancellation_reaps_the_real_worker_tool_and_descendant() {
    let fixture = Fixture::new();
    let renderer = fixture.renderer();
    let input = fixture.input();
    let task = {
        let renderer = renderer.clone();
        tokio::spawn(async move {
            renderer
                .render(input, PreviewComputeBudget::new(40_000).unwrap())
                .await
        })
    };
    fixture.wait_entered().await;
    task.abort();
    let _ = task.await;
    renderer.drain().await;
    fixture.assert_cleaned().await;
}
