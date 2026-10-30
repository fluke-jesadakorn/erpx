//! Trusted, explicitly supplied development Files provider configuration.
//! This parser grants no Account, Holding, database or filesystem authority.
use crate::files_scan_native::NativeFilesScanConfiguration;
use erpx_server_files::public::{
    malware_scanner::{ClamAvConfig, ClamAvScanner},
    preview::PreviewToolConfiguration,
};
use erpx_server_identity_access::public::account_holding_bindings::ConfiguredHoldingRegistry;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
    time::Duration,
};

pub struct FieldFilesProviderTarget {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub providers: NativeFilesScanConfiguration,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Document {
    targets: Vec<Target>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Target {
    holding_id: String,
    company_id: String,
    files_binding: String,
    files_root: PathBuf,
    temporary_root: PathBuf,
    preview_binary: PathBuf,
    heif_convert: PathBuf,
    pdftoppm: PathBuf,
    clamav_socket: PathBuf,
    clamav_database_directory: PathBuf,
    maximum_definition_age_ms: u64,
    command_timeout_ms: u64,
    scan_timeout_ms: u64,
    concurrent_scans: usize,
}
fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_FILES_CONFIGURATION_INVALID",
        "The configured Field file providers are unavailable.",
    )
}
fn explicit_path(path: &Path) -> bool {
    path.is_absolute()
        && path != Path::new("/")
        && path.as_os_str().len() <= 4096
        && path
            .components()
            .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
}
/// Parse bounded operator-owned bytes against the already verified registry.
/// Provider readiness and leased database authority are checked by assembly.
pub fn parse_field_files_runtime_configuration(
    raw: &[u8],
    registry: &ConfiguredHoldingRegistry,
) -> Result<Vec<FieldFilesProviderTarget>, BusinessError> {
    if raw.is_empty() || raw.len() > 256 * 1024 {
        return Err(invalid());
    }
    let doc: Document = serde_json::from_slice(raw).map_err(|_| invalid())?;
    if doc.targets.is_empty() || doc.targets.len() > 64 {
        return Err(invalid());
    }
    let mut scopes = BTreeSet::new();
    let mut bindings = BTreeMap::new();
    let mut temporary_roots: Vec<PathBuf> = Vec::new();
    let mut output = Vec::with_capacity(doc.targets.len());
    for mut t in doc.targets {
        let holding = HoldingId::new(t.holding_id).map_err(|_| invalid())?;
        let company = CompanyId::new(t.company_id).map_err(|_| invalid())?;
        let registered = registry
            .configured()
            .find(|entry| entry.holding_id == holding)
            .ok_or_else(invalid)?;
        if registered.files_binding != t.files_binding
            || !scopes.insert((holding.clone(), company.clone()))
        {
            return Err(invalid());
        }
        if [
            &t.files_root,
            &t.temporary_root,
            &t.preview_binary,
            &t.heif_convert,
            &t.pdftoppm,
            &t.clamav_socket,
            &t.clamav_database_directory,
        ]
        .iter()
        .any(|p| !explicit_path(p))
        {
            return Err(invalid());
        }
        t.files_root = std::fs::canonicalize(&t.files_root).map_err(|_| invalid())?;
        t.temporary_root = std::fs::canonicalize(&t.temporary_root).map_err(|_| invalid())?;
        if !t.files_root.is_dir()
            || !t.temporary_root.is_dir()
            || t.files_root == Path::new("/")
            || t.temporary_root == Path::new("/")
        {
            return Err(invalid());
        }
        if bindings
            .get(&t.files_binding)
            .is_some_and(|root| root != &t.files_root)
        {
            return Err(invalid());
        }
        bindings.insert(t.files_binding.clone(), t.files_root.clone());
        if temporary_roots
            .iter()
            .any(|p| p.starts_with(&t.temporary_root) || t.temporary_root.starts_with(p))
        {
            return Err(invalid());
        }
        temporary_roots.push(t.temporary_root.clone());
        let tools =
            PreviewToolConfiguration::new(&t.heif_convert, &t.pdftoppm).map_err(|_| invalid())?;
        let scanner = ClamAvConfig {
            socket: t.clamav_socket,
            database_directory: t.clamav_database_directory,
            maximum_definition_age: Duration::from_millis(t.maximum_definition_age_ms),
            command_timeout: Duration::from_millis(t.command_timeout_ms),
            scan_timeout: Duration::from_millis(t.scan_timeout_ms),
            concurrent_scans: t.concurrent_scans,
        };
        ClamAvScanner::new(scanner.clone()).map_err(|_| invalid())?;
        let mut files_roots = BTreeMap::new();
        files_roots.insert(t.files_binding, t.files_root);
        output.push(FieldFilesProviderTarget {
            holding_id: holding,
            company_id: company,
            providers: NativeFilesScanConfiguration {
                files_roots,
                temporary_root: t.temporary_root,
                preview_binary: t.preview_binary,
                preview_tools: tools,
                scanner,
            },
        });
    }
    if temporary_roots.iter().any(|temp| {
        bindings
            .values()
            .any(|root: &PathBuf| temp.starts_with(root) || root.starts_with(temp))
    }) {
        return Err(invalid());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_identity_access::public::{
        AccountPolicy, HoldingOrigin, account_holding_bindings::ConfiguredAccountHolding,
    };
    fn registry() -> ConfiguredHoldingRegistry {
        let policy = AccountPolicy {
            account_origin: "https://account.example.test".into(),
            account_host: "account.example.test".into(),
            environment: "development".into(),
            require_email_verification: true,
            holdings: vec![HoldingOrigin {
                origin: "https://acme.example.test".into(),
                host: "acme.example.test".into(),
                audience: "holding:development:h001:acme.example.test".into(),
            }],
        };
        ConfiguredHoldingRegistry::new(
            &policy,
            vec![ConfiguredAccountHolding {
                holding_id: HoldingId::new("h001").unwrap(),
                slug: "acme".into(),
                write_binding: "H001_DB".into(),
                read_binding: "H001_READ".into(),
                files_binding: "H001_FILES".into(),
            }],
        )
        .unwrap()
    }
    fn target(root: &Path) -> serde_json::Value {
        for path in ["files", "temp", "temp2", "other", "files/nested"] {
            std::fs::create_dir_all(root.join(path)).unwrap();
        }
        serde_json::json!({"holdingId":"h001","companyId":"c001","filesBinding":"H001_FILES","filesRoot":root.join("files"),"temporaryRoot":root.join("temp"),"previewBinary":"/usr/bin/true","heifConvert":"/usr/bin/true","pdftoppm":"/usr/bin/true","clamavSocket":root.join("clamd.sock"),"clamavDatabaseDirectory":root.join("clamdb"),"maximumDefinitionAgeMs":86400000,"commandTimeoutMs":1000,"scanTimeoutMs":5000,"concurrentScans":1})
    }
    fn parse(value: serde_json::Value) -> Result<Vec<FieldFilesProviderTarget>, BusinessError> {
        parse_field_files_runtime_configuration(&serde_json::to_vec(&value).unwrap(), &registry())
    }
    #[test]
    fn accepts_exact_registered_scope_and_shared_root_for_distinct_companies() {
        let fixture = tempfile::tempdir().unwrap();
        let first = target(fixture.path());
        let mut second = first.clone();
        second["companyId"] = "c002".into();
        second["temporaryRoot"] = fixture.path().join("temp2").to_str().unwrap().into();
        let result = parse(serde_json::json!({"targets":[first,second]})).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].holding_id.as_str(), "h001");
        assert_eq!(result[1].company_id.as_str(), "c002");
    }
    #[test]
    fn rejects_unknown_fields_duplicates_and_unregistered_or_conflicting_bindings() {
        let fixture = tempfile::tempdir().unwrap();
        let t = target(fixture.path());
        assert!(parse(serde_json::json!({"targets":[t.clone(),t.clone()]})).is_err());
        let mut bad = t.clone();
        bad["filesBinding"] = "OTHER".into();
        assert!(parse(serde_json::json!({"targets":[bad]})).is_err());
        let mut bad = t.clone();
        bad["unknown"] = true.into();
        assert!(parse(serde_json::json!({"targets":[bad]})).is_err());
        let mut bad = t.clone();
        bad["companyId"] = "c002".into();
        bad["temporaryRoot"] = fixture.path().join("temp2").to_str().unwrap().into();
        bad["filesRoot"] = fixture.path().join("other").to_str().unwrap().into();
        assert!(parse(serde_json::json!({"targets":[t,bad]})).is_err());
    }
    #[test]
    fn rejects_relative_paths_and_out_of_bounds_provider_settings() {
        let fixture = tempfile::tempdir().unwrap();
        let mut bad = target(fixture.path());
        bad["filesRoot"] = "relative".into();
        assert!(parse(serde_json::json!({"targets":[bad]})).is_err());
        let mut bad = target(fixture.path());
        bad["clamavSocket"] = "/tmp/../other".into();
        assert!(parse(serde_json::json!({"targets":[bad]})).is_err());
        let mut bad = target(fixture.path());
        bad["temporaryRoot"] = fixture.path().join("files/nested").to_str().unwrap().into();
        assert!(parse(serde_json::json!({"targets":[bad]})).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(fixture.path().join("files"), fixture.path().join("alias"))
                .unwrap();
            let mut bad = target(fixture.path());
            bad["temporaryRoot"] = fixture.path().join("alias/nested").to_str().unwrap().into();
            assert!(parse(serde_json::json!({"targets":[bad]})).is_err());
        }
        let mut bad = target(fixture.path());
        bad["concurrentScans"] = 0.into();
        assert!(parse(serde_json::json!({"targets":[bad]})).is_err());
        let mut bad = target(fixture.path());
        bad["commandTimeoutMs"] = 6000.into();
        assert!(parse(serde_json::json!({"targets":[bad]})).is_err());
    }
}
