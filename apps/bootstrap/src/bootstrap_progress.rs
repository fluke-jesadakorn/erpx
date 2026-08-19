//! Non-authoritative, local diagnostics for the one-shot bootstrap.
//!
//! This marker never proves resource ownership and must never authorize
//! adoption or resumption of existing databases or object namespaces. Only a
//! handle returned by [`begin`] can advance an attempt; [`read`] returns a
//! read-only snapshot for status and operator diagnostics.

use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{BootstrapError, Result, fresh_state::FreshResourcePlan};

const PROGRESS_VERSION: u32 = 2;
const MAX_PROGRESS_BYTES: u64 = 64 * 1024;
const PROGRESS_DIRECTORY_COMPONENTS: [&str; 3] = [".runtime", "erpx", "bootstrap"];
const PROGRESS_FILE: &str = "progress.json";
const PROGRESS_LOCK_FILE: &str = ".progress.lock";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BootstrapPhase {
    DatabaseCreation,
    DatabaseMigration,
    ObjectNamespaceCreation,
    ControlRegistration,
    PrincipalSeed,
    BusinessSeed,
    OrganizationSeed,
    HoldingActivation,
    ManifestPublication,
    ReceiptCompletion,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BootstrapProgressState {
    Provisioning,
    Failed,
    Complete,
}

/// Closed, secret-free failure categories. Raw provider, SQL, path, endpoint,
/// and credential diagnostics must not be written to the progress marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BootstrapFailureCategory {
    DatabaseCreation,
    DatabaseMigration,
    ObjectNamespaceCreation,
    ControlRegistration,
    PrincipalSeed,
    BusinessSeed,
    OrganizationSeed,
    HoldingActivation,
    ManifestPublication,
    ReceiptCompletion,
}

impl BootstrapFailureCategory {
    const fn for_phase(phase: BootstrapPhase) -> Option<Self> {
        match phase {
            BootstrapPhase::DatabaseCreation => Some(Self::DatabaseCreation),
            BootstrapPhase::DatabaseMigration => Some(Self::DatabaseMigration),
            BootstrapPhase::ObjectNamespaceCreation => Some(Self::ObjectNamespaceCreation),
            BootstrapPhase::ControlRegistration => Some(Self::ControlRegistration),
            BootstrapPhase::PrincipalSeed => Some(Self::PrincipalSeed),
            BootstrapPhase::BusinessSeed => Some(Self::BusinessSeed),
            BootstrapPhase::OrganizationSeed => Some(Self::OrganizationSeed),
            BootstrapPhase::HoldingActivation => Some(Self::HoldingActivation),
            BootstrapPhase::ManifestPublication => Some(Self::ManifestPublication),
            BootstrapPhase::ReceiptCompletion => Some(Self::ReceiptCompletion),
            BootstrapPhase::Complete => None,
        }
    }

    const fn phase(self) -> BootstrapPhase {
        match self {
            Self::DatabaseCreation => BootstrapPhase::DatabaseCreation,
            Self::DatabaseMigration => BootstrapPhase::DatabaseMigration,
            Self::ObjectNamespaceCreation => BootstrapPhase::ObjectNamespaceCreation,
            Self::ControlRegistration => BootstrapPhase::ControlRegistration,
            Self::PrincipalSeed => BootstrapPhase::PrincipalSeed,
            Self::BusinessSeed => BootstrapPhase::BusinessSeed,
            Self::OrganizationSeed => BootstrapPhase::OrganizationSeed,
            Self::HoldingActivation => BootstrapPhase::HoldingActivation,
            Self::ManifestPublication => BootstrapPhase::ManifestPublication,
            Self::ReceiptCompletion => BootstrapPhase::ReceiptCompletion,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BootstrapResourceNames {
    control_write_database: String,
    control_read_database: String,
    holding_write_database: String,
    holding_read_database: String,
    object_namespace: String,
}

impl BootstrapResourceNames {
    pub(crate) fn database_names(&self) -> [&str; 4] {
        [
            &self.control_write_database,
            &self.control_read_database,
            &self.holding_write_database,
            &self.holding_read_database,
        ]
    }

    fn from_plan(plan: &FreshResourcePlan) -> Self {
        Self {
            control_write_database: plan.databases[0].name.clone(),
            control_read_database: plan.databases[1].name.clone(),
            holding_write_database: plan.databases[2].name.clone(),
            holding_read_database: plan.databases[3].name.clone(),
            object_namespace: plan.storage_namespace.clone(),
        }
    }

    fn validate(&self, prefix: &str) -> Result<()> {
        let names = self.database_names();
        if names.iter().any(|name| !valid_database_name(name))
            || names
                .iter()
                .enumerate()
                .any(|(index, name)| names[..index].contains(name))
            || self.control_write_database != format!("{prefix}_control_write")
            || self.control_read_database != format!("{prefix}_control_read")
            || self.object_namespace != format!("{}-holding-objects", prefix.replace('_', "-"))
        {
            return Err(BootstrapError(
                "bootstrap progress resource inventory is invalid",
            ));
        }
        let holding_write_token = self
            .holding_write_database
            .strip_prefix(&format!("{prefix}_"))
            .and_then(|value| value.strip_suffix("_holding_write"));
        let holding_read_token = self
            .holding_read_database
            .strip_prefix(&format!("{prefix}_"))
            .and_then(|value| value.strip_suffix("_holding_read"));
        if holding_write_token != holding_read_token
            || !holding_write_token.is_some_and(|token| {
                token.len() == 12
                    && token
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
        {
            return Err(BootstrapError(
                "bootstrap progress resource inventory is invalid",
            ));
        }
        Ok(())
    }
}

/// Read-only diagnostic snapshot. It intentionally exposes no mutation or
/// resume API; only `BootstrapProgressHandle` returned by `begin` can write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BootstrapProgressMarker {
    version: u32,
    prefix: String,
    generation: Uuid,
    resources: BootstrapResourceNames,
    current_phase: BootstrapPhase,
    state: BootstrapProgressState,
    failure_category: Option<BootstrapFailureCategory>,
    updated_at: DateTime<Utc>,
}

impl BootstrapProgressMarker {
    pub(crate) fn generation(&self) -> Uuid {
        self.generation
    }

    pub(crate) const fn current_phase(&self) -> BootstrapPhase {
        self.current_phase
    }

    pub(crate) const fn state(&self) -> BootstrapProgressState {
        self.state
    }

    pub(crate) const fn failure_category(&self) -> Option<BootstrapFailureCategory> {
        self.failure_category
    }

    #[cfg(test)]
    pub(crate) const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Check diagnostic identity only. A match is not resource-ownership proof
    /// and callers must not use it to resume or adopt an installation.
    pub(crate) fn require_plan(&self, plan: &FreshResourcePlan) -> Result<()> {
        if self.prefix != plan.prefix
            || self.generation != plan.installation_generation
            || self.resources != BootstrapResourceNames::from_plan(plan)
        {
            return Err(BootstrapError(
                "bootstrap progress does not match the requested resource plan",
            ));
        }
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        if self.version != PROGRESS_VERSION
            || !valid_prefix(&self.prefix)
            || self.generation.is_nil()
            || self.updated_at.timestamp() < 0
        {
            return Err(BootstrapError("bootstrap progress marker is invalid"));
        }
        self.resources.validate(&self.prefix)?;
        let state_is_valid = match self.state {
            BootstrapProgressState::Provisioning => {
                self.failure_category.is_none() && self.current_phase != BootstrapPhase::Complete
            }
            BootstrapProgressState::Failed => {
                self.failure_category
                    .is_some_and(|category| category.phase() == self.current_phase)
                    && self.current_phase != BootstrapPhase::Complete
            }
            BootstrapProgressState::Complete => {
                self.failure_category.is_none() && self.current_phase == BootstrapPhase::Complete
            }
        };
        if !state_is_valid {
            return Err(BootstrapError("bootstrap progress marker is invalid"));
        }
        Ok(())
    }
}

impl BootstrapPhase {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::DatabaseCreation => "database_creation",
            Self::DatabaseMigration => "database_migration",
            Self::ObjectNamespaceCreation => "object_namespace_creation",
            Self::ControlRegistration => "control_registration",
            Self::PrincipalSeed => "principal_seed",
            Self::BusinessSeed => "business_seed",
            Self::OrganizationSeed => "organization_seed",
            Self::HoldingActivation => "holding_activation",
            Self::ManifestPublication => "manifest_publication",
            Self::ReceiptCompletion => "receipt_completion",
            Self::Complete => "complete",
        }
    }
}

impl BootstrapProgressState {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Provisioning => "provisioning",
            Self::Failed => "failed",
            Self::Complete => "complete",
        }
    }
}

impl BootstrapFailureCategory {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::DatabaseCreation => "database_creation",
            Self::DatabaseMigration => "database_migration",
            Self::ObjectNamespaceCreation => "object_namespace_creation",
            Self::ControlRegistration => "control_registration",
            Self::PrincipalSeed => "principal_seed",
            Self::BusinessSeed => "business_seed",
            Self::OrganizationSeed => "organization_seed",
            Self::HoldingActivation => "holding_activation",
            Self::ManifestPublication => "manifest_publication",
            Self::ReceiptCompletion => "receipt_completion",
        }
    }
}

/// A write capability for one in-process attempt. It cannot be reconstructed
/// by `read`, deliberately preventing this diagnostic file from becoming an
/// automatic resume or resource-adoption token.
pub(crate) struct BootstrapProgressHandle {
    workspace_root: PathBuf,
    current: BootstrapProgressMarker,
}

impl Drop for BootstrapProgressHandle {
    fn drop(&mut self) {
        if self.current.state == BootstrapProgressState::Provisioning
            && let Some(category) = BootstrapFailureCategory::for_phase(self.current.current_phase)
        {
            let _ = self.fail(category, Utc::now());
        }
    }
}

impl BootstrapProgressHandle {
    #[cfg(test)]
    pub(crate) const fn snapshot(&self) -> &BootstrapProgressMarker {
        &self.current
    }

    pub(crate) fn update_phase(
        &mut self,
        next_phase: BootstrapPhase,
        updated_at: DateTime<Utc>,
    ) -> Result<()> {
        if self.current.state != BootstrapProgressState::Provisioning
            || next_phase == BootstrapPhase::Complete
            || next_phase <= self.current.current_phase
            || updated_at < self.current.updated_at
        {
            return Err(BootstrapError(
                "bootstrap progress phase transition is invalid",
            ));
        }
        let mut next = self.current.clone();
        next.current_phase = next_phase;
        next.updated_at = updated_at;
        replace_marker(&self.workspace_root, &self.current, &next)?;
        self.current = next;
        Ok(())
    }

    pub(crate) fn fail(
        &mut self,
        category: BootstrapFailureCategory,
        updated_at: DateTime<Utc>,
    ) -> Result<()> {
        if self.current.state != BootstrapProgressState::Provisioning
            || category.phase() != self.current.current_phase
            || updated_at < self.current.updated_at
        {
            return Err(BootstrapError(
                "bootstrap progress failure transition is invalid",
            ));
        }
        let mut next = self.current.clone();
        next.state = BootstrapProgressState::Failed;
        next.failure_category = Some(category);
        next.updated_at = updated_at;
        replace_marker(&self.workspace_root, &self.current, &next)?;
        self.current = next;
        Ok(())
    }

    pub(crate) fn complete(&mut self, updated_at: DateTime<Utc>) -> Result<()> {
        if self.current.state != BootstrapProgressState::Provisioning
            || self.current.current_phase != BootstrapPhase::ReceiptCompletion
            || updated_at < self.current.updated_at
        {
            return Err(BootstrapError(
                "bootstrap progress completion transition is invalid",
            ));
        }
        let mut next = self.current.clone();
        next.current_phase = BootstrapPhase::Complete;
        next.state = BootstrapProgressState::Complete;
        next.failure_category = None;
        next.updated_at = updated_at;
        replace_marker(&self.workspace_root, &self.current, &next)?;
        self.current = next;
        Ok(())
    }
}

/// Start one diagnostic attempt. An existing marker always fails closed; this
/// API never resumes or adopts the resources named by an earlier marker.
pub(crate) fn begin(
    workspace_root: &Path,
    plan: &FreshResourcePlan,
    updated_at: DateTime<Utc>,
) -> Result<BootstrapProgressHandle> {
    let marker = BootstrapProgressMarker {
        version: PROGRESS_VERSION,
        prefix: plan.prefix.clone(),
        generation: plan.installation_generation,
        resources: BootstrapResourceNames::from_plan(plan),
        current_phase: BootstrapPhase::DatabaseCreation,
        state: BootstrapProgressState::Provisioning,
        failure_category: None,
        updated_at,
    };
    marker.validate()?;
    write_new_marker(workspace_root, &marker)?;
    Ok(BootstrapProgressHandle {
        workspace_root: workspace_root.to_path_buf(),
        current: marker,
    })
}

/// Read diagnostics without granting a write/resume capability.
pub(crate) fn read(workspace_root: &Path) -> Result<Option<BootstrapProgressMarker>> {
    let Some(directory) = progress_directory_for_read(workspace_root)? else {
        return Ok(None);
    };
    let path = directory.join(PROGRESS_FILE);
    match fs::symlink_metadata(&path) {
        Ok(_) => read_marker_path(&path).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(BootstrapError(
            "bootstrap progress marker could not be inspected",
        )),
    }
}

fn write_new_marker(workspace_root: &Path, marker: &BootstrapProgressMarker) -> Result<()> {
    let directory = prepare_progress_directory(workspace_root)?;
    let _lock = ProgressWriteLock::acquire(&directory)?;
    let destination = directory.join(PROGRESS_FILE);
    if path_entry_exists(&destination)? {
        return Err(BootstrapError(
            "bootstrap progress marker already exists; automatic resume is disabled",
        ));
    }
    write_atomic(&directory, &destination, marker, false)
}

fn replace_marker(
    workspace_root: &Path,
    expected: &BootstrapProgressMarker,
    next: &BootstrapProgressMarker,
) -> Result<()> {
    next.validate()?;
    let directory = prepare_progress_directory(workspace_root)?;
    let _lock = ProgressWriteLock::acquire(&directory)?;
    let destination = directory.join(PROGRESS_FILE);
    let on_disk = read_marker_path(&destination)?;
    if &on_disk != expected {
        return Err(BootstrapError(
            "bootstrap progress changed outside the active attempt",
        ));
    }
    write_atomic(&directory, &destination, next, true)
}

fn write_atomic(
    directory: &Path,
    destination: &Path,
    marker: &BootstrapProgressMarker,
    replacing: bool,
) -> Result<()> {
    let mut encoded = serde_json::to_vec_pretty(marker)
        .map_err(|_| BootstrapError("bootstrap progress marker could not be serialized"))?;
    encoded.push(b'\n');
    if encoded.len() as u64 > MAX_PROGRESS_BYTES {
        return Err(BootstrapError("bootstrap progress marker is too large"));
    }
    let temporary = directory.join(format!(".progress-{}.tmp", Uuid::new_v4().simple()));
    let result = (|| -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true).mode(0o600);
        let mut file = options.open(&temporary)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(&encoded)?;
        file.sync_all()?;
        if !replacing && path_entry_exists_io(destination)? {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "progress marker exists",
            ));
        }
        fs::rename(&temporary, destination)?;
        File::open(directory)?.sync_all()?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result.map_err(|_| BootstrapError("bootstrap progress marker could not be written atomically"))
}

fn path_entry_exists(path: &Path) -> Result<bool> {
    path_entry_exists_io(path)
        .map_err(|_| BootstrapError("bootstrap progress marker could not be inspected"))
}

fn path_entry_exists_io(path: &Path) -> std::io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn read_marker_path(path: &Path) -> Result<BootstrapProgressMarker> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| BootstrapError("bootstrap progress marker could not be inspected"))?;
    if metadata.file_type().is_symlink()
        || !metadata.file_type().is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_PROGRESS_BYTES
        || metadata.permissions().mode() & 0o777 != 0o600
    {
        return Err(BootstrapError("bootstrap progress marker is unsafe"));
    }
    let file = File::open(path)
        .map_err(|_| BootstrapError("bootstrap progress marker could not be read"))?;
    let capacity = usize::try_from(metadata.len())
        .map_err(|_| BootstrapError("bootstrap progress marker is too large"))?;
    let mut encoded = Vec::with_capacity(capacity);
    file.take(MAX_PROGRESS_BYTES + 1)
        .read_to_end(&mut encoded)
        .map_err(|_| BootstrapError("bootstrap progress marker could not be read"))?;
    if encoded.len() as u64 > MAX_PROGRESS_BYTES {
        return Err(BootstrapError("bootstrap progress marker is too large"));
    }
    let marker: BootstrapProgressMarker = serde_json::from_slice(&encoded)
        .map_err(|_| BootstrapError("bootstrap progress marker is malformed"))?;
    marker.validate()?;
    Ok(marker)
}

fn prepare_progress_directory(workspace_root: &Path) -> Result<PathBuf> {
    validate_workspace_root(workspace_root)?;
    let mut current = workspace_root.to_path_buf();
    for component in PROGRESS_DIRECTORY_COMPONENTS {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            }
            Ok(_) => {
                return Err(BootstrapError("bootstrap progress directory is unsafe"));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut builder = DirBuilder::new();
                builder.mode(0o700);
                match builder.create(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        let metadata = fs::symlink_metadata(&current).map_err(|_| {
                            BootstrapError("bootstrap progress directory is unsafe")
                        })?;
                        if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
                            return Err(BootstrapError("bootstrap progress directory is unsafe"));
                        }
                    }
                    Err(_) => {
                        return Err(BootstrapError(
                            "bootstrap progress directory could not be created",
                        ));
                    }
                }
            }
            Err(_) => {
                return Err(BootstrapError("bootstrap progress directory is unsafe"));
            }
        }
    }
    Ok(current)
}

fn progress_directory_for_read(workspace_root: &Path) -> Result<Option<PathBuf>> {
    validate_workspace_root(workspace_root)?;
    let mut current = workspace_root.to_path_buf();
    for component in PROGRESS_DIRECTORY_COMPONENTS {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            }
            Ok(_) => {
                return Err(BootstrapError("bootstrap progress directory is unsafe"));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => {
                return Err(BootstrapError("bootstrap progress directory is unsafe"));
            }
        }
    }
    Ok(Some(current))
}

fn validate_workspace_root(workspace_root: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(workspace_root)
        .map_err(|_| BootstrapError("bootstrap progress workspace root is invalid"))?;
    if !workspace_root.is_absolute()
        || metadata.file_type().is_symlink()
        || !metadata.file_type().is_dir()
    {
        return Err(BootstrapError(
            "bootstrap progress workspace root is invalid",
        ));
    }
    Ok(())
}

fn valid_prefix(value: &str) -> bool {
    (2..=16).contains(&value.len())
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn valid_database_name(value: &str) -> bool {
    (1..=63).contains(&value.len())
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

struct ProgressWriteLock {
    path: PathBuf,
}

impl ProgressWriteLock {
    fn acquire(directory: &Path) -> Result<Self> {
        let path = directory.join(PROGRESS_LOCK_FILE);
        let mut options = OpenOptions::new();
        options.create_new(true).write(true).mode(0o600);
        let file = options
            .open(&path)
            .map_err(|_| BootstrapError("bootstrap progress marker is locked"))?;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| BootstrapError("bootstrap progress marker lock is unsafe"))?;
        file.sync_all()
            .map_err(|_| BootstrapError("bootstrap progress marker lock could not be persisted"))?;
        Ok(Self { path })
    }
}

impl Drop for ProgressWriteLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, os::unix::fs::symlink};

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new() -> Self {
            let path = env::temp_dir().join(format!(
                "erpx-bootstrap-progress-test-{}",
                Uuid::new_v4().simple()
            ));
            let mut builder = DirBuilder::new();
            builder.mode(0o700);
            builder.create(&path).expect("test workspace root");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn plan() -> FreshResourcePlan {
        FreshResourcePlan::new("next", "Holding")
    }

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).expect("timestamp")
    }

    fn write_fixture(root: &Path, bytes: &[u8]) -> PathBuf {
        let directory = prepare_progress_directory(root).expect("progress directory");
        let path = directory.join(PROGRESS_FILE);
        let mut options = OpenOptions::new();
        options.create_new(true).write(true).mode(0o600);
        let mut file = options.open(&path).expect("fixture marker");
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .expect("fixture permissions");
        file.write_all(bytes).expect("fixture contents");
        file.sync_all().expect("fixture sync");
        path
    }

    #[test]
    fn begin_writes_private_secret_free_marker_and_disables_adoption() {
        let root = TestRoot::new();
        let plan = plan();
        let handle = begin(root.path(), &plan, at(1)).expect("begin progress");
        let path = prepare_progress_directory(root.path())
            .expect("directory")
            .join(PROGRESS_FILE);
        let metadata = fs::symlink_metadata(&path).expect("marker metadata");
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);

        let encoded = fs::read_to_string(&path).expect("marker contents");
        for forbidden in [
            "endpoint",
            "password",
            "access_key",
            "secret_key",
            "postgres://",
            "postgresql://",
        ] {
            assert!(!encoded.contains(forbidden), "found {forbidden}");
        }
        assert_eq!(
            handle.snapshot().current_phase(),
            BootstrapPhase::DatabaseCreation
        );
        assert!(begin(root.path(), &plan, at(2)).is_err());
    }

    #[test]
    fn read_rejects_symlink_marker() {
        let root = TestRoot::new();
        let directory = prepare_progress_directory(root.path()).expect("directory");
        let target = root.path().join("target.json");
        fs::write(&target, b"{}").expect("symlink target");
        symlink(&target, directory.join(PROGRESS_FILE)).expect("marker symlink");
        assert!(read(root.path()).is_err());
    }

    #[test]
    fn read_rejects_malformed_and_oversize_markers() {
        let malformed_root = TestRoot::new();
        write_fixture(malformed_root.path(), b"not-json\n");
        assert!(read(malformed_root.path()).is_err());

        let oversize_root = TestRoot::new();
        write_fixture(
            oversize_root.path(),
            &vec![b'x'; MAX_PROGRESS_BYTES as usize + 1],
        );
        assert!(read(oversize_root.path()).is_err());
    }

    #[test]
    fn phase_updates_replace_atomically_and_reject_stale_state() {
        let root = TestRoot::new();
        let plan = plan();
        let mut handle = begin(root.path(), &plan, at(1)).expect("begin progress");
        let stale = handle.snapshot().clone();
        handle
            .update_phase(BootstrapPhase::DatabaseMigration, at(2))
            .expect("advance phase");
        let marker = read(root.path()).expect("read").expect("marker");
        assert_eq!(marker.current_phase(), BootstrapPhase::DatabaseMigration);
        assert_eq!(marker.updated_at(), at(2));
        marker.require_plan(&plan).expect("matching plan");

        let directory = prepare_progress_directory(root.path()).expect("directory");
        let names = fs::read_dir(directory)
            .expect("directory entries")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .into_string()
                    .expect("UTF-8 file name")
            })
            .collect::<Vec<_>>();
        assert_eq!(names, vec![PROGRESS_FILE.to_owned()]);

        let mut stale_handle = BootstrapProgressHandle {
            workspace_root: root.path().to_path_buf(),
            current: stale,
        };
        assert!(
            stale_handle
                .update_phase(BootstrapPhase::ObjectNamespaceCreation, at(3))
                .is_err()
        );
    }

    #[test]
    fn failed_marker_is_terminal_and_bounded() {
        let root = TestRoot::new();
        let mut handle = begin(root.path(), &plan(), at(1)).expect("begin progress");
        handle
            .update_phase(BootstrapPhase::ObjectNamespaceCreation, at(2))
            .expect("advance phase");
        handle
            .fail(BootstrapFailureCategory::ObjectNamespaceCreation, at(3))
            .expect("mark failed");

        let marker = read(root.path()).expect("read").expect("marker");
        assert_eq!(marker.state(), BootstrapProgressState::Failed);
        assert_eq!(
            marker.failure_category(),
            Some(BootstrapFailureCategory::ObjectNamespaceCreation)
        );
        assert!(
            handle
                .update_phase(BootstrapPhase::ControlRegistration, at(4))
                .is_err()
        );
        assert!(handle.complete(at(4)).is_err());
    }

    #[test]
    fn complete_requires_receipt_completion_phase() {
        let root = TestRoot::new();
        let mut handle = begin(root.path(), &plan(), at(1)).expect("begin progress");
        assert!(handle.complete(at(2)).is_err());
        handle
            .update_phase(BootstrapPhase::ReceiptCompletion, at(2))
            .expect("receipt phase");
        handle.complete(at(3)).expect("complete progress");
        let marker = read(root.path()).expect("read").expect("marker");
        assert_eq!(marker.state(), BootstrapProgressState::Complete);
        assert_eq!(marker.current_phase(), BootstrapPhase::Complete);
        assert_eq!(marker.failure_category(), None);
    }
}
