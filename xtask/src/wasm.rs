use std::{
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

type TaskResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const REPORT_SCHEMA_VERSION: u32 = 3;
const BASE_BROTLI_BUDGET: u64 = 1024 * 1024;
const WORKSPACE_BROTLI_BUDGET: u64 = 750 * 1024;
const COLD_ROUTE_BROTLI_BUDGET: u64 = 1_792 * 1024;

const WORKSPACE_CHUNKS: [(&str, &str); 21] = [
    ("operations", "OperationsWorkspace"),
    ("finance", "FinanceWorkspace"),
    ("sales", "SalesWorkspace"),
    ("procurement", "ProcurementWorkspace"),
    ("inventory", "InventoryWorkspace"),
    ("asset", "AssetWorkspace"),
    ("maintenance", "MaintenanceWorkspace"),
    ("quality", "QualityWorkspace"),
    ("marketing", "MarketingWorkspace"),
    ("people", "PeopleWorkspace"),
    ("pos", "PosWorkspace"),
    ("legal", "LegalWorkspace"),
    ("manufacturing", "ManufacturingWorkspace"),
    ("planning", "PlanningWorkspace"),
    ("project", "ProjectWorkspace"),
    ("commerce", "CommerceWorkspace"),
    ("talent", "TalentWorkspace"),
    ("learning", "LearningWorkspace"),
    ("benefits", "BenefitsWorkspace"),
    ("field-service", "FieldServiceWorkspace"),
    ("ai", "AiWorkspace"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum WasmLayout {
    MonolithicDev,
    SplitRelease,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArtifactReadiness {
    pub public: PathBuf,
    pub stylesheets: Vec<PathBuf>,
    pub javascript: Vec<PathBuf>,
    pub wasm: PathBuf,
    pub wasm_files: Vec<PathBuf>,
    pub layout: WasmLayout,
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
    pub layout: WasmLayout,
    pub chunks: Vec<WasmChunkReport>,
    pub workspaces: Vec<WorkspaceClosureReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WasmChunkReport {
    pub logical_name: String,
    pub path: String,
    pub sha256: String,
    pub raw_bytes: u64,
    pub brotli_bytes: u64,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WorkspaceClosureReport {
    pub workspace: String,
    pub entry_chunk: String,
    pub logical_brotli_bytes: u64,
    pub cold_route_brotli_bytes: u64,
}

pub(crate) fn public_directory(root: &Path, profile: WasmProfile) -> PathBuf {
    root.join("target/dx/erpx-console")
        .join(profile.dx_directory())
        .join("web/public")
}

/// Validate the generated Dioxus Web bundle before a running `dx serve`
/// process is treated as healthy. The directory is generated output, so it
/// must remain entirely below `target/dx`. Development stays monolithic for
/// hot reload; release must contain Dioxus split glue and lazy route modules.
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
    let (layout, wasm) = match profile {
        WasmProfile::Dev => (
            WasmLayout::MonolithicDev,
            validate_monolithic_layout(&files)?,
        ),
        WasmProfile::Release => (
            WasmLayout::SplitRelease,
            validate_split_release_layout(&public, &files)?,
        ),
    };
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
        layout,
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

    let shared_chunks = readiness
        .wasm_files
        .iter()
        .filter(|path| is_shared_chunk(path))
        .map(|path| relative_display(root, path))
        .collect::<Vec<_>>();
    let chunks = readiness
        .wasm_files
        .iter()
        .filter(|path| **path != wasm)
        .map(|path| {
            let logical_name = logical_chunk_name(path);
            let dependencies = if is_module_chunk(path) {
                shared_chunks.clone()
            } else {
                Vec::new()
            };
            Ok(WasmChunkReport {
                logical_name,
                path: relative_display(root, path),
                sha256: file_sha256(path)?,
                raw_bytes: fs::metadata(path)?.len(),
                brotli_bytes: fs::metadata(brotli_path(path))?.len(),
                dependencies,
            })
        })
        .collect::<TaskResult<Vec<_>>>()?;
    let base_brotli_bytes = fs::metadata(brotli_path(&wasm))?.len();
    let workspaces = if profile == WasmProfile::Release {
        workspace_closures(base_brotli_bytes, &chunks)?
    } else {
        Vec::new()
    };
    let report = WasmSizeReport {
        schema_version: REPORT_SCHEMA_VERSION,
        profile: profile.label().into(),
        base_path: relative_display(root, &wasm),
        base_raw_bytes: fs::metadata(&wasm)?.len(),
        base_brotli_bytes,
        layout: readiness.layout,
        chunks,
        workspaces,
    };
    enforce_release_budgets(&report)?;
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

fn validate_split_release_layout(public: &Path, files: &[PathBuf]) -> TaskResult<PathBuf> {
    let wasm_files = files
        .iter()
        .filter(|path| {
            path.extension().and_then(|value| value.to_str()) == Some("wasm")
                && !is_hot_patch_artifact(path)
        })
        .collect::<Vec<_>>();
    let main = wasm_files
        .iter()
        .filter(|path| !is_module_chunk(path) && !is_shared_chunk(path))
        .collect::<Vec<_>>();
    let main = match main.as_slice() {
        [main] => (**main).clone(),
        [] => return Err("split release artifact contains no base WASM".into()),
        _ => return Err("split release artifact contains multiple base WASM files".into()),
    };
    let modules = wasm_files
        .iter()
        .filter(|path| is_module_chunk(path))
        .collect::<Vec<_>>();
    if modules.len() < WORKSPACE_CHUNKS.len() {
        return Err(format!(
            "split release artifact has {} lazy modules; at least {} workspace entries are required",
            modules.len(),
            WORKSPACE_CHUNKS.len()
        )
        .into());
    }
    for (workspace, token) in WORKSPACE_CHUNKS {
        if !modules.iter().any(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(token))
        }) {
            return Err(format!("split release is missing the {workspace} workspace entry").into());
        }
    }
    validate_split_references(public, files, &wasm_files)?;
    Ok(main.to_path_buf())
}

fn validate_split_references(
    public: &Path,
    files: &[PathBuf],
    wasm_files: &[&PathBuf],
) -> TaskResult {
    let javascript = files
        .iter()
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("js"))
        .map(fs::read_to_string)
        .collect::<Result<Vec<_>, _>>()?
        .join("\n");
    if !javascript.contains("__wasm_split_load_") {
        return Err("split release artifact is missing Dioxus wasm-split glue".into());
    }
    for path in wasm_files
        .iter()
        .filter(|path| is_module_chunk(path) || is_shared_chunk(path))
    {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                format!(
                    "split artifact has a non-UTF-8 file name: {}",
                    path.display()
                )
            })?;
        if !javascript.contains(name) {
            return Err(format!("split chunk is stale or unreferenced: {name}").into());
        }
    }
    for name in referenced_wasm_file_names(&javascript) {
        let path = public.join("assets").join(&name);
        if (name.starts_with("module_") || name.starts_with("chunk_")) && !path.is_file() {
            return Err(format!("split glue references missing chunk: {name}").into());
        }
    }
    Ok(())
}

fn referenced_wasm_file_names(source: &str) -> Vec<String> {
    source
        .split(['"', '\'', '(', ')', ',', ' '])
        .filter_map(|token| {
            let token = token.split(['?', '#']).next().unwrap_or_default();
            token
                .ends_with(".wasm")
                .then(|| token.rsplit('/').next().unwrap_or(token).to_owned())
        })
        .collect()
}

fn is_module_chunk(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("module_") && name.ends_with(".wasm"))
}

fn is_shared_chunk(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("chunk_") && name.ends_with(".wasm"))
}

fn logical_chunk_name(path: &Path) -> String {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    WORKSPACE_CHUNKS
        .iter()
        .find(|(_, token)| file_name.contains(token))
        .map_or_else(
            || file_name.to_owned(),
            |(workspace, _)| (*workspace).to_owned(),
        )
}

fn workspace_closures(
    base_brotli_bytes: u64,
    chunks: &[WasmChunkReport],
) -> TaskResult<Vec<WorkspaceClosureReport>> {
    let shared_bytes = chunks
        .iter()
        .filter(|chunk| {
            chunk
                .path
                .rsplit('/')
                .next()
                .is_some_and(|name| name.starts_with("chunk_"))
        })
        .map(|chunk| chunk.brotli_bytes)
        .sum::<u64>();
    WORKSPACE_CHUNKS
        .iter()
        .map(|(workspace, _)| {
            let entry = chunks
                .iter()
                .find(|chunk| chunk.logical_name == *workspace)
                .ok_or_else(|| format!("workspace manifest is missing {workspace}"))?;
            Ok(WorkspaceClosureReport {
                workspace: (*workspace).to_owned(),
                entry_chunk: entry.path.clone(),
                logical_brotli_bytes: entry.brotli_bytes,
                cold_route_brotli_bytes: base_brotli_bytes + shared_bytes + entry.brotli_bytes,
            })
        })
        .collect()
}

fn enforce_release_budgets(report: &WasmSizeReport) -> TaskResult {
    if report.layout != WasmLayout::SplitRelease {
        return Ok(());
    }
    if report.base_brotli_bytes > BASE_BROTLI_BUDGET {
        return Err(format!(
            "base WASM exceeds the 1.0 MiB Brotli budget: {}",
            report.base_brotli_bytes
        )
        .into());
    }
    for workspace in &report.workspaces {
        if workspace.logical_brotli_bytes > WORKSPACE_BROTLI_BUDGET {
            return Err(format!(
                "{} workspace exceeds the 750 KiB logical budget",
                workspace.workspace
            )
            .into());
        }
        if workspace.cold_route_brotli_bytes > COLD_ROUTE_BROTLI_BUDGET {
            return Err(format!(
                "{} cold-route closure exceeds the 1.75 MiB budget",
                workspace.workspace
            )
            .into());
        }
    }
    Ok(())
}

fn file_sha256(path: &Path) -> TaskResult<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
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
        "wasm-size {}: layout={:?}, base={} raw={} Brotli={}, chunks={}, workspaces={}",
        report.profile,
        report.layout,
        report.base_path,
        display_bytes(report.base_raw_bytes),
        display_bytes(report.base_brotli_bytes),
        report.chunks.len(),
        report.workspaces.len(),
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
    fn split_release_requires_every_workspace_and_referenced_chunk() {
        let root = fixture_root("split-release");
        let public = public_directory(&root, WasmProfile::Release);
        let assets = public.join("assets");
        fs::create_dir_all(&assets).expect("create release fixture assets");
        fs::write(assets.join("erpx-console.css"), b"body {}").expect("write CSS");
        fs::write(assets.join("erpx-console_bg.wasm"), b"base").expect("write base WASM");

        let mut glue = String::from("export const __wasm_split_load_0 = true;\n");
        for (_, token) in WORKSPACE_CHUNKS {
            let name = format!("module_0_{token}.wasm");
            fs::write(assets.join(&name), b"workspace").expect("write workspace chunk");
            glue.push_str(&format!("fetch('/assets/{name}');\n"));
        }
        fs::write(assets.join("chunk_0_shared.wasm"), b"shared").expect("write shared chunk");
        glue.push_str("fetch('/assets/chunk_0_shared.wasm');\n");
        fs::write(assets.join("erpx-console.js"), &glue).expect("write split glue");

        let readiness =
            artifact_readiness(&root, WasmProfile::Release).expect("complete release split graph");
        assert_eq!(readiness.layout, WasmLayout::SplitRelease);
        assert_eq!(readiness.wasm_files.len(), 23);

        fs::remove_file(assets.join("module_0_FinanceWorkspace.wasm"))
            .expect("remove a workspace entry");
        let error = artifact_readiness(&root, WasmProfile::Release)
            .expect_err("missing workspace chunk must fail")
            .to_string();
        assert!(error.contains("at least 21") || error.contains("finance workspace"));
        fs::remove_dir_all(root).expect("remove fixture");
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
