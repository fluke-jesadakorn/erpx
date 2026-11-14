//! Staged observation of the private native engineering roster file.
//! Inputs must come from separately retained attestation evidence. A valid file
//! is not an attestation, database identity, engineering grant, or authority.
use crate::adapters::legal_release::{
    BoundedEngineeringAttestation, EngineeringReadConfiguration,
    capture_bounded_engineering_attestation,
};
use erpx_server_runtime::public::native_engineering_roster::{
    NativeEngineeringRoster, validate_native_engineering_roster,
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn hexadecimal(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
/// The retained TS attestation only accepts the exact private run directory.
/// Enforce the same `.runtime/browser/<run>` location before observing a file.
fn roster_directory(directory: &Path, run: &str) -> bool {
    if !hexadecimal(run, 12) {
        return false;
    }
    let (Some(name), Some(browser), Some(runtime)) = (
        directory.file_name().and_then(|name| name.to_str()),
        directory.parent(),
        directory.parent().and_then(|parent| parent.parent()),
    ) else {
        return false;
    };
    name == run
        && browser.file_name().and_then(|name| name.to_str()) == Some("browser")
        && runtime.file_name().and_then(|name| name.to_str()) == Some(".runtime")
}

#[derive(Clone, PartialEq, Eq)]
pub struct NativeRosterSnapshot {
    roster: NativeEngineeringRoster,
    file: RosterFile,
}
impl NativeRosterSnapshot {
    pub fn roster(&self) -> &NativeEngineeringRoster {
        &self.roster
    }
    /// Re-observe the same bytes and validation; caller separately rechecks its
    /// attestation, process profile, database identity and persisted context.
    pub fn require_current(
        &self,
        directory: &Path,
        seal: &str,
        run: &str,
        source: &str,
        manifest: &str,
    ) -> bool {
        capture_native_roster_snapshot(directory, seal, run, source, manifest).as_ref() == Some(self)
    }
}
/// Attested, artifact-bound wrapper around the roster bytes. The exact seal,
/// run, source fingerprint and manifest are retained from the bounded
/// attestation rather than supplied by a request or caller.
#[derive(Clone, PartialEq, Eq)]
pub struct AttestedNativeRosterSnapshot {
    directory: PathBuf,
    seal: String,
    run: String,
    source: String,
    manifest: String,
    snapshot: NativeRosterSnapshot,
}
impl AttestedNativeRosterSnapshot {
    pub fn roster(&self) -> &NativeEngineeringRoster {
        self.snapshot.roster()
    }
    /// Re-observe the exact private roster and every retained binding. The
    /// caller still rechecks the attestation, database identity and enrollment.
    pub fn require_current(&self) -> bool {
        capture_native_roster_snapshot(
            &self.directory,
            &self.seal,
            &self.run,
            &self.source,
            &self.manifest,
        )
        .as_ref()
            == Some(&self.snapshot)
    }
}
#[derive(Clone, PartialEq, Eq)]
struct RosterFile {
    bytes: Vec<u8>,
    directory_dev: u64,
    directory_ino: u64,
    file_dev: u64,
    file_ino: u64,
}
#[cfg(unix)]
fn private_roster_bytes(directory: &Path) -> Option<RosterFile> {
    use nix::{
        fcntl::{OFlag, open, openat},
        sys::stat::Mode,
        unistd::getuid,
    };
    use std::{
        io::Read,
        os::unix::fs::MetadataExt,
        path::Component,
    };
    // Resolve the caller's path once, then pin every component through directory
    // descriptors. A rename or symlink swap cannot redirect the final open
    // outside the validated chain.
    let canonical = std::fs::canonicalize(directory).ok()?;
    let directory = canonical.as_path();
    let mut parts = directory.components();
    if parts.next()? != Component::RootDir {
        return None;
    }
    let mut anchor = std::fs::File::from(
        open(
            "/",
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .ok()?,
    );
    for part in parts {
        let Component::Normal(name) = part else {
            return None;
        };
        anchor = std::fs::File::from(
            openat(
                &anchor,
                name,
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .ok()?,
        );
    }
    let dir = anchor.metadata().ok()?;
    let named_dir = std::fs::symlink_metadata(directory).ok()?;
    if !dir.is_dir()
        || named_dir.file_type().is_symlink()
        || dir.dev() != named_dir.dev()
        || dir.ino() != named_dir.ino()
        || dir.uid() != getuid().as_raw()
        || dir.mode() & 0o077 != 0
    {
        return None;
    }
    let path: PathBuf = directory.join("native-engineering-leases.json");
    if std::fs::canonicalize(&path).ok()? != path {
        return None;
    }
    let fd = openat(
        &anchor,
        "native-engineering-leases.json",
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .ok()?;
    let mut file = std::fs::File::from(fd);
    let before = file.metadata().ok()?;
    let named = std::fs::symlink_metadata(&path).ok()?;
    if !before.is_file()
        || named.file_type().is_symlink()
        || before.dev() != named.dev()
        || before.ino() != named.ino()
        || before.uid() != getuid().as_raw()
        || before.mode() & 0o777 != 0o600
        || before.len() > 16384
    {
        return None;
    }
    let mut bytes = Vec::new();
    (&mut file).take(16385).read_to_end(&mut bytes).ok()?;
    let after = file.metadata().ok()?;
    let named_after = std::fs::symlink_metadata(&path).ok()?;
    let dir_after = anchor.metadata().ok()?;
    let named_dir_after = std::fs::symlink_metadata(directory).ok()?;
    if named_dir_after.file_type().is_symlink()
        || dir_after.dev() != dir.dev()
        || dir_after.ino() != dir.ino()
        || named_dir_after.dev() != dir.dev()
        || named_dir_after.ino() != dir.ino()
        || dir_after.uid() != getuid().as_raw()
        || dir_after.mode() & 0o077 != 0
        || named_after.file_type().is_symlink()
        || named_after.dev() != before.dev()
        || named_after.ino() != before.ino()
        || std::fs::canonicalize(&path).ok()? != path
        || bytes.len() as u64 != before.len()
        || after.len() != before.len()
        || after.mtime() != before.mtime()
        || after.mtime_nsec() != before.mtime_nsec()
        || after.ctime() != before.ctime()
        || after.ctime_nsec() != before.ctime_nsec()
    {
        return None;
    }
    Some(RosterFile {
        bytes,
        directory_dev: dir.dev(),
        directory_ino: dir.ino(),
        file_dev: before.dev(),
        file_ino: before.ino(),
    })
}
#[cfg(not(unix))]
fn private_roster_bytes(_directory: &Path) -> Option<RosterFile> {
    None
}
#[cfg(unix)]
pub(crate) fn private_attestation(directory: &Path) -> bool {
    use nix::unistd::getuid;
    use std::os::unix::fs::MetadataExt;
    let path = directory.join("engineering-attestation.json");
    let Ok(named) = std::fs::symlink_metadata(&path) else {
        return false;
    };
    named.is_file()
        && !named.file_type().is_symlink()
        && std::fs::canonicalize(&path).ok().as_deref() == Some(path.as_path())
        && named.uid() == getuid().as_raw()
        && named.mode() & 0o777 == 0o600
}
#[cfg(not(unix))]
pub(crate) fn private_attestation(_directory: &Path) -> bool {
    false
}
/// The `seal`, run and fingerprints are input facts, not proof. Only the caller
/// retaining their independently verified source may use this staged snapshot.
pub fn capture_native_roster_snapshot(
    directory: &Path,
    seal: &str,
    run: &str,
    source: &str,
    manifest: &str,
) -> Option<NativeRosterSnapshot> {
    if !hexadecimal(seal, 64)
        || !hexadecimal(run, 12)
        || !hexadecimal(source, 64)
        || !hexadecimal(manifest, 64)
    {
        return None;
    }
    let directory = std::fs::canonicalize(directory).ok()?;
    if !roster_directory(&directory, run) {
        return None;
    }
    let file = private_roster_bytes(&directory)?;
    if hex::encode(Sha256::digest(&file.bytes)) != seal {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&file.bytes).ok()?;
    let roster = validate_native_engineering_roster(&value, run, source, manifest)?;
    Some(NativeRosterSnapshot { roster, file })
}
/// Capture the bounded engineering attestation and, only when that exact
/// attestation declares a native lease, the matching private roster. A declared
/// lease with a missing or changed roster fails the whole observation. The
/// result still grants no permission and is not a release or rollout decision.
pub fn capture_attested_native_roster_snapshot(
    configuration: &EngineeringReadConfiguration,
    artifact: &str,
    maximum_file_bytes: usize,
) -> Option<(
    BoundedEngineeringAttestation,
    Option<AttestedNativeRosterSnapshot>,
)> {
    let directory = std::fs::canonicalize(&configuration.data_directory).ok()?;
    let attestation =
        capture_bounded_engineering_attestation(configuration, artifact, maximum_file_bytes)?;
    let roster = match attestation.native_lease_sha256() {
        Some(seal) => {
            // The retained TS attester requires a private attestation file
            // whenever it declares a native lease. Check after the bounded read
            // so a replaced file cannot be accepted on a stale earlier stat.
            if !private_attestation(&directory) {
                return None;
            }
            Some(AttestedNativeRosterSnapshot {
                directory: directory.clone(),
                seal: seal.to_owned(),
                run: attestation.run().to_owned(),
                source: attestation.source_fingerprint().to_owned(),
                manifest: attestation.file_manifest_sha256().to_owned(),
                snapshot: capture_native_roster_snapshot(
                    &directory,
                    seal,
                    attestation.run(),
                    attestation.source_fingerprint(),
                    attestation.file_manifest_sha256(),
                )?,
            })
        }
        None => None,
    };
    Some((attestation, roster))
}
