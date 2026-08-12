use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

type ToolchainResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub(crate) const DX_VERSION: &str = "0.8.0-alpha.1";
pub(crate) const PATCH_FINGERPRINT: &str =
    "b27e5de15fb965b4525fa5e5c744beb5785552ff16f6a1d8060dfb919d3b50e4";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct InstallationReceipt {
    schema_version: u32,
    dx_version: String,
    patch_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebToolchainStatus {
    pub dx_path: PathBuf,
    pub dx_version: Option<String>,
    pub patch_fingerprint: &'static str,
    pub ready: bool,
}

pub(crate) fn dx_path(root: &Path) -> PathBuf {
    root.join("target/erpx-tools/bin/dx")
}

pub(crate) fn ensure(root: &Path) -> ToolchainResult<PathBuf> {
    verify_patch_source(root)?;
    let current = status(root)?;
    if current.ready {
        println!(
            "web toolchain: dioxus {} with ERPX splitter patch {}",
            DX_VERSION, PATCH_FINGERPRINT
        );
        return Ok(current.dx_path);
    }

    let install_root = root.join("target/erpx-tools");
    ensure_local_directory(root, &install_root)?;
    let splitter = root.join("third_party/wasm-split-cli");
    let patch_config = format!(
        "patch.crates-io.wasm-split-cli.path={:?}",
        splitter.to_string_lossy()
    );
    let install_status = Command::new("cargo")
        .arg("+1.93.1")
        .args([
            "install",
            "dioxus-cli",
            "--version",
            DX_VERSION,
            "--locked",
            "--force",
            "--root",
        ])
        .arg(&install_root)
        .args(["--target-dir"])
        .arg(install_root.join("build"))
        .args(["--config", &patch_config])
        .current_dir(root)
        .status()?;
    if !install_status.success() {
        return Err(format!("patched Dioxus CLI installation failed with {install_status}").into());
    }

    let dx = dx_path(root);
    let version = read_dx_version(&dx)?;
    require_expected_dx_version(&version)?;
    write_receipt(root)?;
    let installed = status(root)?;
    if !installed.ready {
        return Err("patched Dioxus CLI did not pass its post-install fingerprint check".into());
    }
    println!(
        "web toolchain: installed dioxus {} with ERPX splitter patch {}",
        DX_VERSION, PATCH_FINGERPRINT
    );
    Ok(dx)
}

#[allow(dead_code)] // Used by the heavy launcher; the lightweight status binary shares this module.
pub(crate) fn require(root: &Path) -> ToolchainResult<PathBuf> {
    verify_patch_source(root)?;
    let status = status(root)?;
    if status.ready {
        Ok(status.dx_path)
    } else {
        Err(format!(
            "patched Dioxus CLI is missing or stale; run `cargo xtask web-toolchain` (expected {} / {})",
            DX_VERSION, PATCH_FINGERPRINT
        )
        .into())
    }
}

pub(crate) fn status(root: &Path) -> ToolchainResult<WebToolchainStatus> {
    let dx = dx_path(root);
    let dx_version = dx
        .is_file()
        .then(|| read_dx_version(&dx))
        .transpose()
        .ok()
        .flatten();
    let receipt = read_receipt(root).ok().flatten();
    let ready = dx_version
        .as_deref()
        .is_some_and(|version| require_expected_dx_version(version).is_ok())
        && receipt.is_some_and(|receipt| {
            receipt.schema_version == 1
                && receipt.dx_version == DX_VERSION
                && receipt.patch_fingerprint == PATCH_FINGERPRINT
        })
        && verify_patch_source(root).is_ok();
    Ok(WebToolchainStatus {
        dx_path: dx,
        dx_version,
        patch_fingerprint: PATCH_FINGERPRINT,
        ready,
    })
}

fn verify_patch_source(root: &Path) -> ToolchainResult {
    let source = root.join("third_party/wasm-split-cli/src/lib.rs");
    let metadata = fs::symlink_metadata(&source)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("unsafe vendored splitter source: {}", source.display()).into());
    }
    let fingerprint = format!("{:x}", Sha256::digest(fs::read(&source)?));
    if fingerprint != PATCH_FINGERPRINT {
        return Err(format!(
            "vendored splitter fingerprint mismatch: expected {PATCH_FINGERPRINT}, found {fingerprint}"
        )
        .into());
    }
    Ok(())
}

fn require_expected_dx_version(version: &str) -> ToolchainResult {
    let expected = format!("dioxus {DX_VERSION}");
    if version.trim().starts_with(&expected) {
        Ok(())
    } else {
        Err(format!("expected {expected}, found {}", version.trim()).into())
    }
}

fn read_dx_version(dx: &Path) -> ToolchainResult<String> {
    let output = Command::new(dx).arg("--version").output()?;
    if !output.status.success() {
        return Err(format!("{} --version failed", dx.display()).into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn receipt_path(root: &Path) -> PathBuf {
    root.join("target/erpx-tools/erpx-web-toolchain.json")
}

fn read_receipt(root: &Path) -> ToolchainResult<Option<InstallationReceipt>> {
    let path = receipt_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("unsafe Web toolchain receipt: {}", path.display()).into());
    }
    Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
}

fn write_receipt(root: &Path) -> ToolchainResult {
    let path = receipt_path(root);
    let temporary = path.with_extension("json.tmp");
    let receipt = InstallationReceipt {
        schema_version: 1,
        dx_version: DX_VERSION.into(),
        patch_fingerprint: PATCH_FINGERPRINT.into(),
    };
    fs::write(&temporary, serde_json::to_vec_pretty(&receipt)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn ensure_local_directory(root: &Path, path: &Path) -> ToolchainResult {
    if !path.starts_with(root.join("target")) {
        return Err(format!("Web toolchain path escaped target/: {}", path.display()).into());
    }
    for candidate in [root.join("target"), path.to_owned()] {
        if candidate.exists() {
            let metadata = fs::symlink_metadata(&candidate)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(
                    format!("unsafe Web toolchain directory: {}", candidate.display()).into(),
                );
            }
        } else {
            fs::create_dir_all(&candidate)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_check_is_exact_to_the_pinned_alpha() {
        assert!(require_expected_dx_version("dioxus 0.8.0-alpha.1 (abc123)\n").is_ok());
        assert!(require_expected_dx_version("dioxus 0.8.0-alpha.2").is_err());
        assert!(require_expected_dx_version("dioxus 0.7.9").is_err());
    }
}
