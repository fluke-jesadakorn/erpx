use std::{
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

type TaskResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const DEV_BASE_RAW_BUDGET: u64 = 60 * 1024 * 1024;
const RELEASE_BASE_BROTLI_BUDGET: u64 = 1_310_720;
const RELEASE_TOTAL_BROTLI_BUDGET: u64 = 3_407_872;
const REQUIRED_FEATURE_CHUNKS: usize = 21;

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
    pub profile: String,
    pub base_path: String,
    pub base_raw_bytes: u64,
    pub base_brotli_bytes: u64,
    pub chunk_count: usize,
    pub total_raw_bytes: u64,
    pub total_brotli_bytes: u64,
    pub largest_chunk_path: Option<String>,
    pub largest_chunk_raw_bytes: u64,
}

pub(crate) fn public_directory(root: &Path, profile: WasmProfile) -> PathBuf {
    root.join("target/dx/erpx-console")
        .join(profile.dx_directory())
        .join("web/public")
}

#[allow(dead_code)] // Called by xtask-heavy; the lightweight binary shares the reporting module.
pub(crate) fn reconcile_release_generated_assets(root: &Path) -> TaskResult {
    let assets = public_directory(root, WasmProfile::Release).join("assets");
    if assets.exists() {
        let metadata = fs::symlink_metadata(&assets)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(format!(
                "refusing to reconcile unsafe Dioxus generated asset path: {}",
                assets.display()
            )
            .into());
        }
        fs::remove_dir_all(&assets)?;
    }
    fs::create_dir_all(&assets)?;
    println!(
        "wasm assets: reconciled generated release directory {}",
        assets.display()
    );
    Ok(())
}

pub(crate) fn compress_and_measure(
    root: &Path,
    profile: WasmProfile,
    enforce_budget: bool,
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
    let mut compressible = Vec::new();
    collect_files(&public, &mut compressible, &|path| {
        matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("js" | "wasm")
        )
    })?;
    if compressible.is_empty() {
        return Err(format!(
            "Dioxus {} Web artifact contains no JS or WASM files",
            profile.label()
        )
        .into());
    }
    for path in &compressible {
        write_brotli(path, profile.brotli_quality())?;
    }

    let mut wasm_files = compressible
        .into_iter()
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("wasm"))
        .collect::<Vec<_>>();
    wasm_files.sort();
    let base_candidates = wasm_files
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.contains("_bg"))
        })
        .cloned()
        .collect::<Vec<_>>();
    let base = match base_candidates.as_slice() {
        [base] => base.clone(),
        [] if wasm_files.len() == 1 => wasm_files[0].clone(),
        [] => return Err("split Web artifact has no identifiable base WASM (_bg.wasm)".into()),
        _ => {
            return Err(format!(
                "split Web artifact contains {} base WASM candidates; stale generated assets were not reconciled",
                base_candidates.len()
            )
            .into());
        }
    };
    let mut total_raw_bytes = 0_u64;
    let mut total_brotli_bytes = 0_u64;
    let mut largest_chunk: Option<(PathBuf, u64)> = None;
    for path in &wasm_files {
        let raw = fs::metadata(path)?.len();
        let brotli = fs::metadata(brotli_path(path))?.len();
        total_raw_bytes = total_raw_bytes.saturating_add(raw);
        total_brotli_bytes = total_brotli_bytes.saturating_add(brotli);
        if path != &base && largest_chunk.as_ref().is_none_or(|(_, size)| raw > *size) {
            largest_chunk = Some((path.clone(), raw));
        }
    }
    let base_raw_bytes = fs::metadata(&base)?.len();
    let base_brotli_bytes = fs::metadata(brotli_path(&base))?.len();
    let report = WasmSizeReport {
        profile: profile.label().into(),
        base_path: relative_display(root, &base),
        base_raw_bytes,
        base_brotli_bytes,
        chunk_count: wasm_files.len().saturating_sub(1),
        total_raw_bytes,
        total_brotli_bytes,
        largest_chunk_path: largest_chunk
            .as_ref()
            .map(|(path, _)| relative_display(root, path)),
        largest_chunk_raw_bytes: largest_chunk.map(|(_, size)| size).unwrap_or(0),
    };
    print_report(&report);
    record_report(root, &report)?;
    if enforce_budget {
        enforce(&report, profile)?;
    }
    Ok(report)
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
    Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
}

fn enforce(report: &WasmSizeReport, profile: WasmProfile) -> TaskResult {
    if report.chunk_count < REQUIRED_FEATURE_CHUNKS {
        return Err(format!(
            "{} split artifact has {} lazy chunks; at least {REQUIRED_FEATURE_CHUNKS} registered feature chunks are required",
            profile.label(),
            report.chunk_count
        )
        .into());
    }
    match profile {
        WasmProfile::Dev if report.base_raw_bytes > DEV_BASE_RAW_BUDGET => Err(format!(
            "dev base WASM is {} bytes; budget is {DEV_BASE_RAW_BUDGET} bytes",
            report.base_raw_bytes
        )
        .into()),
        WasmProfile::Release if report.base_brotli_bytes > RELEASE_BASE_BROTLI_BUDGET => {
            Err(format!(
                "release base WASM Brotli is {} bytes; budget is {RELEASE_BASE_BROTLI_BUDGET} bytes",
                report.base_brotli_bytes
            )
            .into())
        }
        WasmProfile::Release if report.total_brotli_bytes > RELEASE_TOTAL_BROTLI_BUDGET => {
            Err(format!(
                "release total WASM Brotli is {} bytes; budget is {RELEASE_TOTAL_BROTLI_BUDGET} bytes",
                report.total_brotli_bytes
            )
            .into())
        }
        _ => Ok(()),
    }
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
        "wasm-size {}: base={} raw={} Brotli={} chunks={} total-raw={} total-Brotli={} largest-chunk={} ({})",
        report.profile,
        report.base_path,
        display_bytes(report.base_raw_bytes),
        display_bytes(report.base_brotli_bytes),
        report.chunk_count,
        display_bytes(report.total_raw_bytes),
        display_bytes(report.total_brotli_bytes),
        report.largest_chunk_path.as_deref().unwrap_or("none"),
        display_bytes(report.largest_chunk_raw_bytes),
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
    use super::*;

    #[test]
    fn budgets_match_the_acceptance_contract() {
        assert_eq!(DEV_BASE_RAW_BUDGET, 60 * 1024 * 1024);
        assert_eq!(RELEASE_BASE_BROTLI_BUDGET, 1_310_720);
        assert_eq!(RELEASE_TOTAL_BROTLI_BUDGET, 3_407_872);
        assert_eq!(REQUIRED_FEATURE_CHUNKS, 21);
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
}
