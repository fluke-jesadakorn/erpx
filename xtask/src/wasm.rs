use std::{
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

type TaskResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const REPORT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArtifactReadiness {
    pub public: PathBuf,
    pub stylesheets: Vec<PathBuf>,
    pub javascript: Vec<PathBuf>,
    pub wasm: PathBuf,
    pub wasm_files: Vec<PathBuf>,
}

#[allow(dead_code)] // Each xtask binary uses a different subset of profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WasmProfile {
    Dev,
    Release,
}

impl WasmProfile {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Release => "release",
        }
    }

    const fn dx_directory(self) -> &'static str {
        match self {
            Self::Dev => "debug",
            Self::Release => "release",
        }
    }

    const fn brotli_quality(self) -> u32 {
        match self {
            Self::Dev => 5,
            Self::Release => 11,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WasmSizeReport {
    #[serde(default)]
    schema_version: u32,
    pub profile: String,
    pub base_path: String,
    pub base_raw_bytes: u64,
    pub base_brotli_bytes: u64,
}

pub(crate) fn public_directory(root: &Path, profile: WasmProfile) -> PathBuf {
    root.join("target/dx/erpx-console")
        .join(profile.dx_directory())
        .join("web/public")
}

/// Validate the generated Dioxus Web bundle before a running `dx serve`
/// process is treated as healthy. The directory is generated output, so it
/// must remain entirely below `target/dx` and contain a complete monolithic
/// hydration bundle.
pub(crate) fn artifact_readiness(
    root: &Path,
    profile: WasmProfile,
) -> TaskResult<ArtifactReadiness> {
    let dx_root = root.join("target/dx");
    let public = public_directory(root, profile);
    if !public.starts_with(&dx_root) {
        return Err(format!("Dioxus public path escaped target/dx: {}", public.display()).into());
    }
    if !public.is_dir() {
        return Err(format!(
            "Dioxus {} Web public directory is missing: {}",
            profile.label(),
            public.display()
        )
        .into());
    }
    reject_symlink_components(&dx_root, &public)?;

    let canonical_dx = dx_root.canonicalize()?;
    let canonical_public = public.canonicalize()?;
    if !canonical_public.starts_with(&canonical_dx) {
        return Err(format!(
            "Dioxus {} Web public directory resolved outside target/dx: {}",
            profile.label(),
            canonical_public.display()
        )
        .into());
    }

    let mut files = Vec::new();
    collect_files(&public, &mut files, &|_| true)?;
    let wasm = validate_monolithic_layout(&files)?;
    let stylesheets = files
        .iter()
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("css"))
        .cloned()
        .collect::<Vec<_>>();
    let javascript = files
        .iter()
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("js"))
        .cloned()
        .collect::<Vec<_>>();
    if stylesheets.is_empty() {
        return Err("Dioxus Web artifact contains no CSS files".into());
    }
    if javascript.is_empty() {
        return Err("Dioxus Web artifact contains no JavaScript files".into());
    }
    let wasm_files = files
        .iter()
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("wasm"))
        .cloned()
        .collect::<Vec<_>>();
    for path in stylesheets.iter().chain(&javascript).chain(&wasm_files) {
        if fs::metadata(path)?.len() == 0 {
            return Err(format!("Dioxus Web artifact is empty: {}", path.display()).into());
        }
    }

    Ok(ArtifactReadiness {
        public,
        stylesheets,
        javascript,
        wasm,
        wasm_files,
    })
}

fn reject_symlink_components(dx_root: &Path, public: &Path) -> TaskResult {
    let mut current = dx_root.to_path_buf();
    if fs::symlink_metadata(&current)?.file_type().is_symlink() {
        return Err(format!("refusing Dioxus artifact symlink: {}", current.display()).into());
    }
    let relative = public
        .strip_prefix(dx_root)
        .map_err(|_| "Dioxus public path escaped target/dx")?;
    for component in relative.components() {
        current.push(component.as_os_str());
        if fs::symlink_metadata(&current)?.file_type().is_symlink() {
            return Err(format!("refusing Dioxus artifact symlink: {}", current.display()).into());
        }
    }
    Ok(())
}

#[allow(dead_code)] // Called by xtask-heavy; the lightweight binary shares this module.
pub(crate) fn reconcile_release_generated_assets(root: &Path) -> TaskResult {
    let public = public_directory(root, WasmProfile::Release);
    let expected_root = root.join("target/dx/erpx-console/release/web");
    if !public.starts_with(&expected_root) {
        return Err(format!(
            "Dioxus generated release path escaped its target: {}",
            public.display()
        )
        .into());
    }
    if public.exists() {
        let metadata = fs::symlink_metadata(&public)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(format!(
                "refusing to reconcile unsafe Dioxus generated path: {}",
                public.display()
            )
            .into());
        }
        fs::remove_dir_all(&public)?;
    }
    fs::create_dir_all(&public)?;
    println!(
        "wasm assets: reconciled generated release directory {}",
        public.display()
    );
    Ok(())
}

pub(crate) fn compress_and_measure(
    root: &Path,
    profile: WasmProfile,
) -> TaskResult<WasmSizeReport> {
    let public = public_directory(root, profile);
    if !public.is_dir() {
        return Err(format!(
            "Dioxus {} Web artifact is missing at {}; build it with the ERPX Web lane first",
            profile.label(),
            public.display()
        )
        .into());
    }

    let readiness = artifact_readiness(root, profile)?;
    let mut artifact_files = Vec::new();
    collect_files(&public, &mut artifact_files, &|_| true)?;
    let wasm = readiness.wasm;

    let compressible = artifact_files
        .iter()
        .filter(|path| {
            matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("js" | "wasm")
            )
        })
        .collect::<Vec<_>>();
    if compressible.is_empty() {
        return Err(format!(
            "Dioxus {} Web artifact contains no JS or WASM files",
            profile.label()
        )
        .into());
    }
    for path in compressible {
        write_brotli(path, profile.brotli_quality())?;
    }

    let report = WasmSizeReport {
        schema_version: REPORT_SCHEMA_VERSION,
        profile: profile.label().into(),
        base_path: relative_display(root, &wasm),
        base_raw_bytes: fs::metadata(&wasm)?.len(),
        base_brotli_bytes: fs::metadata(brotli_path(&wasm))?.len(),
    };
    print_report(&report);
    record_report(root, &report)?;
    Ok(report)
}

fn validate_monolithic_layout(files: &[PathBuf]) -> TaskResult<PathBuf> {
    if let Some(path) = files.iter().find(|path| is_split_artifact(path)) {
        return Err(format!(
            "monolithic Web artifact contains split glue, chunk, or lazy module file: {}",
            path.display()
        )
        .into());
    }

    let wasm_files = files
        .iter()
        .filter(|path| {
            path.extension().and_then(|value| value.to_str()) == Some("wasm")
                && !is_hot_patch_artifact(path)
        })
        .collect::<Vec<_>>();
    match wasm_files.as_slice() {
        [wasm] => Ok((*wasm).clone()),
        [] => Err("monolithic Web artifact contains no main WASM file".into()),
        _ => Err(format!(
            "monolithic Web artifact must contain exactly one WASM file; found {}",
            wasm_files.len()
        )
        .into()),
    }
}

fn is_hot_patch_artifact(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| {
            name.starts_with("lib") && name.contains("-patch-") && name.ends_with(".wasm")
        })
}

fn is_split_artifact(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    name.contains("__wasm_split")
        || ((name.starts_with("chunk_") || name.starts_with("module_"))
            && (name.contains(".wasm") || name.ends_with(".js") || name.ends_with(".js.br")))
}

#[allow(dead_code)] // Status is implemented by the lightweight xtask binary.
pub(crate) fn latest_report(
    root: &Path,
    profile: WasmProfile,
) -> TaskResult<Option<WasmSizeReport>> {
    let path = report_path(root, profile.label());
    if !path.is_file() {
        return Ok(None);
    }
    let report: WasmSizeReport = serde_json::from_slice(&fs::read(path)?)?;
    if report.schema_version != REPORT_SCHEMA_VERSION {
        return Ok(None);
    }
    Ok(Some(report))
}

fn collect_files(
    path: &Path,
    files: &mut Vec<PathBuf>,
    include: &impl Fn(&Path) -> bool,
) -> TaskResult {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(format!("refusing Dioxus artifact symlink: {}", path.display()).into());
    }
    if metadata.is_file() {
        if include(path) {
            files.push(path.to_owned());
        }
        return Ok(());
    }
    let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        collect_files(&entry.path(), files, include)?;
    }
    Ok(())
}

fn write_brotli(path: &Path, quality: u32) -> TaskResult {
    let output = brotli_path(path);
    let temporary = output.with_extension("br.tmp");
    let input = File::open(path)?;
    let target = File::create(&temporary)?;
    let mut reader = BufReader::new(input);
    let writer = BufWriter::new(target);
    let mut compressor = brotli::CompressorWriter::new(writer, 128 * 1024, quality, 22);
    std::io::copy(&mut reader, &mut compressor)?;
    compressor.flush()?;
    drop(compressor);
    fs::rename(temporary, output)?;
    Ok(())
}

fn brotli_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.br", path.to_string_lossy()))
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn print_report(report: &WasmSizeReport) {
    println!(
        "wasm-size {}: monolithic={} raw={} Brotli={} (informational; no size budget)",
        report.profile,
        report.base_path,
        display_bytes(report.base_raw_bytes),
        display_bytes(report.base_brotli_bytes),
    );
}

fn record_report(root: &Path, report: &WasmSizeReport) -> TaskResult {
    let path = report_path(root, &report.profile);
    let parent = path.parent().ok_or("WASM report path has no parent")?;
    fs::create_dir_all(parent)?;
    let bytes = serde_json::to_vec_pretty(report)?;
    fs::write(path, bytes)?;
    Ok(())
}

fn report_path(root: &Path, profile: &str) -> PathBuf {
    root.join(".runtime/erpx")
        .join(format!("wasm-size-{profile}.json"))
}

pub(crate) fn display_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.2} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.2} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn fixture_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!("erpx-wasm-{label}-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn profile_paths_are_dioxus_generated_directories() {
        let root = Path::new("/workspace/erpx");
        assert_eq!(
            public_directory(root, WasmProfile::Dev),
            PathBuf::from("/workspace/erpx/target/dx/erpx-console/debug/web/public")
        );
        assert_eq!(
            public_directory(root, WasmProfile::Release),
            PathBuf::from("/workspace/erpx/target/dx/erpx-console/release/web/public")
        );
    }

    #[test]
    fn monolithic_layout_accepts_exactly_one_main_wasm() {
        let files = vec![
            PathBuf::from("public/assets/erpx-console.js"),
            PathBuf::from("public/assets/erpx-console_bg.wasm"),
            PathBuf::from("public/wasm/liberpx-console-patch-123.wasm"),
        ];
        assert_eq!(
            validate_monolithic_layout(&files).expect("single WASM layout"),
            PathBuf::from("public/assets/erpx-console_bg.wasm")
        );
    }

    #[test]
    fn monolithic_layout_rejects_multiple_wasm_and_split_files() {
        let multiple = vec![
            PathBuf::from("public/wasm/erpx-console_bg.wasm"),
            PathBuf::from("public/assets/another.wasm"),
        ];
        assert!(
            validate_monolithic_layout(&multiple)
                .expect_err("multiple WASM files must fail")
                .to_string()
                .contains("exactly one WASM")
        );

        for forbidden in [
            "public/assets/__wasm_split.js",
            "public/assets/chunk_0_split.wasm.br",
            "public/assets/module_1_LazyConsoleApp.wasm",
        ] {
            let files = vec![
                PathBuf::from("public/assets/erpx-console_bg.wasm"),
                PathBuf::from(forbidden),
            ];
            assert!(
                validate_monolithic_layout(&files)
                    .expect_err("split artifact must fail")
                    .to_string()
                    .contains("split glue, chunk, or lazy module")
            );
        }
    }

    #[test]
    fn wasm_size_is_informational_after_layout_validation() {
        let root = fixture_root("report");
        let public = public_directory(&root, WasmProfile::Dev);
        fs::create_dir_all(public.join("assets")).expect("create fixture assets");
        fs::write(
            public.join("assets/erpx-console.js"),
            b"export const ready = true;",
        )
        .expect("write fixture JS");
        fs::write(
            public.join("assets/erpx-console.css"),
            b"body { color: black; }",
        )
        .expect("write fixture CSS");
        fs::write(public.join("assets/erpx-console_bg.wasm"), b"wasm-fixture")
            .expect("write fixture WASM");

        let report = compress_and_measure(&root, WasmProfile::Dev).expect("measure fixture");
        assert_eq!(report.schema_version, REPORT_SCHEMA_VERSION);
        assert_eq!(report.base_raw_bytes, 12);
        assert!(report.base_brotli_bytes > 0);
        assert!(latest_report(&root, WasmProfile::Dev).unwrap().is_some());

        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn readiness_rejects_missing_and_partial_artifact_layouts() {
        let root = fixture_root("partial");
        let error = artifact_readiness(&root, WasmProfile::Dev)
            .expect_err("missing public directory must fail")
            .to_string();
        assert!(error.contains("public directory is missing"));

        let public = public_directory(&root, WasmProfile::Dev);
        fs::create_dir_all(public.join("assets")).expect("create fixture assets");
        fs::write(public.join("assets/erpx-console.js"), b"export {};").expect("write fixture JS");
        fs::write(public.join("assets/erpx-console_bg.wasm"), b"wasm").expect("write fixture WASM");
        let error = artifact_readiness(&root, WasmProfile::Dev)
            .expect_err("missing CSS must fail")
            .to_string();
        assert!(error.contains("no CSS"));

        fs::write(public.join("assets/erpx-console.css"), b"body {}").expect("write fixture CSS");
        assert!(artifact_readiness(&root, WasmProfile::Dev).is_ok());
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[cfg(unix)]
    #[test]
    fn readiness_rejects_symlinked_artifact_layouts() {
        use std::os::unix::fs::symlink;

        let root = fixture_root("symlink");
        let outside = fixture_root("outside");
        fs::create_dir_all(&outside).expect("create outside directory");
        let public = public_directory(&root, WasmProfile::Dev);
        fs::create_dir_all(public.parent().expect("public parent"))
            .expect("create generated parent");
        symlink(&outside, &public).expect("symlink public directory");
        let error = artifact_readiness(&root, WasmProfile::Dev)
            .expect_err("symlinked public directory must fail")
            .to_string();
        assert!(error.contains("symlink"));
        fs::remove_dir_all(root).expect("remove fixture");
        fs::remove_dir_all(outside).expect("remove outside fixture");
    }
}
