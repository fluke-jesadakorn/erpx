use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};

type ToolchainResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub(crate) const DX_VERSION: &str = "0.8.0-alpha.1";
pub(crate) const TOOLCHAIN_CONTRACT_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstallationReceipt {
    schema_version: u32,
    dx_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebToolchainStatus {
    pub dx_path: PathBuf,
    pub dx_version: Option<String>,
    pub contract_version: u32,
    pub ready: bool,
}

pub(crate) fn dx_path(root: &Path) -> PathBuf {
    root.join("target/erpx-tools/bin/dx")
}

pub(crate) fn ensure(root: &Path) -> ToolchainResult<PathBuf> {
    let current = status(root)?;
    if current.ready {
        println!("web toolchain: standard pinned dioxus {DX_VERSION}");
        return Ok(current.dx_path);
    }

    let install_root = root.join("target/erpx-tools");
    ensure_local_directory(root, &install_root)?;
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
        .current_dir(root)
        .status()?;
    if !install_status.success() {
        return Err(format!("Dioxus CLI installation failed with {install_status}").into());
    }

    let dx = dx_path(root);
    let version = read_dx_version(&dx)?;
    require_expected_dx_version(&version)?;
    write_receipt(root)?;
    let installed = status(root)?;
    if !installed.ready {
        return Err("Dioxus CLI did not pass its post-install receipt check".into());
    }
    println!("web toolchain: installed standard pinned dioxus {DX_VERSION}");
    Ok(dx)
}

#[allow(dead_code)] // Used by the heavy launcher; the lightweight status binary shares this module.
pub(crate) fn require(root: &Path) -> ToolchainResult<PathBuf> {
    let status = status(root)?;
    if status.ready {
        Ok(status.dx_path)
    } else {
        Err(format!(
            "standard pinned Dioxus CLI is missing or stale; run `cargo xtask web-toolchain` (expected {DX_VERSION}, contract v{TOOLCHAIN_CONTRACT_VERSION})",
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
        && receipt.as_ref().is_some_and(receipt_matches_contract);
    Ok(WebToolchainStatus {
        dx_path: dx,
        dx_version,
        contract_version: TOOLCHAIN_CONTRACT_VERSION,
        ready,
    })
}

fn receipt_matches_contract(receipt: &InstallationReceipt) -> bool {
    receipt.schema_version == TOOLCHAIN_CONTRACT_VERSION && receipt.dx_version == DX_VERSION
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
        schema_version: TOOLCHAIN_CONTRACT_VERSION,
        dx_version: DX_VERSION.into(),
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

    #[test]
    fn standard_toolchain_receipt_rejects_the_splitter_patch_contract() {
        let receipt = InstallationReceipt {
            schema_version: TOOLCHAIN_CONTRACT_VERSION,
            dx_version: DX_VERSION.into(),
        };
        assert!(receipt_matches_contract(&receipt));
        assert!(!receipt_matches_contract(&InstallationReceipt {
            schema_version: TOOLCHAIN_CONTRACT_VERSION - 1,
            dx_version: DX_VERSION.into(),
        }));

        let encoded = serde_json::to_string(&receipt).expect("serialize standard receipt");
        assert!(!encoded.contains("patch"));
        assert!(
            serde_json::from_str::<InstallationReceipt>(
                r#"{"schema_version":1,"dx_version":"0.8.0-alpha.1","patch_fingerprint":"legacy"}"#,
            )
            .is_err()
        );
    }
}
