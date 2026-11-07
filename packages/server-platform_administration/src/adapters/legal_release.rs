use crate::{application::legal_release::LegalReleaseReadPort, domain::legal_release::*};
use erpx_server_identity_access::public::release_reviewers::{
    PgReleaseReviewerReader, ReleaseReviewerRequest, lock_release_reviewer_sources,
};
use erpx_server_organization::public::{
    PgCompanyBusinessAcceptanceReader, lock_business_acceptance_sources,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis, UserId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
#[cfg(any(unix, test))]
use std::io::Read;
use std::{fs, path::PathBuf};
type Error = PgTransactionError<BusinessError>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Resource {
    schema: String,
    source: serde_json::Value,
    scope_version: String,
    criteria: Vec<Criterion>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Criterion {
    id: String,
    permission: String,
}
pub fn current_legal_release_catalogue() -> Result<LegalReleaseCatalogue, BusinessError> {
    let resource: Resource =
        serde_json::from_str(include_str!("../../resources/legal-release.v1.json"))
            .map_err(|_| invalid())?;
    if resource.schema != "erpx-legal-release/v1"
        || resource.scope_version != "legal-th-operations-v1"
        || resource.criteria.len() != 7
        || !resource.source.is_object()
    {
        return Err(invalid());
    }
    let mut ids = std::collections::BTreeSet::new();
    if resource.criteria.iter().any(|c| {
        c.id.is_empty()
            || !ids.insert(c.id.clone())
            || !matches!(
                c.permission.as_str(),
                "legal.manage" | "organization.manage"
            )
    }) {
        return Err(invalid());
    }
    Ok(LegalReleaseCatalogue {
        scope_version: resource.scope_version,
        criteria: resource
            .criteria
            .into_iter()
            .map(|c| LegalReleaseCriterion {
                id: c.id,
                permission: c.permission,
            })
            .collect(),
    })
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "LEGAL_RELEASE_CONFIGURATION_INVALID",
        "Current release configuration is required.",
    )
}

/// Trusted runtime configuration, never an HTTP/request-deserializable grant.
/// Filesystem attestation bytes are rechecked on every read, including retention.
#[derive(Clone)]
pub struct EngineeringReadConfiguration {
    pub run: String,
    pub environment: String,
    pub port: String,
    pub browser_simulation_port: String,
    pub data_directory: PathBuf,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Attestation {
    scope: String,
    run: String,
    source_fingerprint: String,
    software_version: String,
    file_manifest_sha256: String,
}
fn hexadecimal(value: &str, n: usize) -> bool {
    value.len() == n
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn engineering_paths(configuration: &EngineeringReadConfiguration) -> Option<[PathBuf; 3]> {
    if !hexadecimal(&configuration.run, 12)
        || configuration.environment != "local"
        || configuration.port != "3104"
        || configuration.browser_simulation_port != "3104"
    {
        return None;
    }
    let directory = fs::canonicalize(&configuration.data_directory).ok()?;
    if !directory.ends_with(PathBuf::from(".runtime/browser").join(&configuration.run)) {
        return None;
    }
    let release = fs::canonicalize(directory.join("release")).ok()?;
    let attestation_path = fs::canonicalize(directory.join("engineering-attestation.json")).ok()?;
    let manifest = fs::canonicalize(release.join("release-files.json")).ok()?;
    if release != directory.join("release")
        || attestation_path != directory.join("engineering-attestation.json")
        || manifest != directory.join("release/release-files.json")
    {
        return None;
    }
    Some([attestation_path, release.join("release.json"), manifest])
}
fn validate_engineering_buffers(
    configuration: &EngineeringReadConfiguration,
    artifact: &str,
    files: &[Vec<u8>; 3],
) -> Option<Attestation> {
    let a: Attestation = serde_json::from_slice(&files[0]).ok()?;
    let info: serde_json::Value = serde_json::from_slice(&files[1]).ok()?;
    let digest = hex::encode(Sha256::digest(&files[2]));
    (a.scope == "isolated-engineering"
        && a.run == configuration.run
        && hexadecimal(&a.source_fingerprint, 64)
        && a.software_version == format!("release-{}", a.source_fingerprint)
        && a.software_version == artifact
        && info["sourceFingerprint"] == a.source_fingerprint
        && info["softwareVersion"] == a.software_version
        && digest == a.file_manifest_sha256
        && info["fileManifestSha256"] == digest)
        .then_some(a)
}
fn attestation(
    configuration: &EngineeringReadConfiguration,
    artifact: &str,
) -> Option<Attestation> {
    let paths = engineering_paths(configuration)?;
    let files = [
        fs::read(&paths[0]).ok()?,
        fs::read(&paths[1]).ok()?,
        fs::read(&paths[2]).ok()?,
    ];
    validate_engineering_buffers(configuration, artifact, &files)
}
/// Original owner-validated bytes; the identity fields cannot be constructed
/// independently of the exact filesystem snapshot and sealed content checks.
#[derive(Clone, PartialEq, Eq)]
pub struct BoundedEngineeringAttestation {
    run: String,
    source_fingerprint: String,
    software_version: String,
    file_manifest_sha256: String,
    files: [Vec<u8>; 3],
}
impl BoundedEngineeringAttestation {
    pub fn run(&self) -> &str {
        &self.run
    }
    pub fn source_fingerprint(&self) -> &str {
        &self.source_fingerprint
    }
    pub fn software_version(&self) -> &str {
        &self.software_version
    }
    pub fn file_manifest_sha256(&self) -> &str {
        &self.file_manifest_sha256
    }
}
#[cfg(any(unix, test))]
fn read_bounded(mut reader: impl Read, maximum: usize) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(u64::try_from(maximum).ok()?.checked_add(1)?)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= maximum).then_some(bytes)
}
#[cfg(unix)]
fn open_bounded_regular(path: &std::path::Path, maximum: usize) -> Option<Vec<u8>> {
    use nix::{
        fcntl::{OFlag, open, openat},
        sys::stat::Mode,
    };
    use std::path::Component;
    // Anchor each canonical component to a directory descriptor: no swapped
    // symlink can be followed, and the final FIFO open is nonblocking.
    let mut parts = path.components().peekable();
    if parts.next()? != Component::RootDir {
        return None;
    }
    let mut directory = open(
        "/",
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .ok()?;
    while let Some(part) = parts.next() {
        let Component::Normal(name) = part else {
            return None;
        };
        if parts.peek().is_some() {
            directory = openat(
                &directory,
                name,
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .ok()?;
        } else {
            let file = fs::File::from(
                openat(
                    &directory,
                    name,
                    OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
                    Mode::empty(),
                )
                .ok()?,
            );
            let metadata = file.metadata().ok()?;
            if !metadata.is_file() || metadata.len() > maximum as u64 {
                return None;
            }
            return read_bounded(file, maximum);
        }
    }
    None
}
#[cfg(not(unix))]
fn open_bounded_regular(_path: &std::path::Path, _maximum: usize) -> Option<Vec<u8>> {
    None
}
/// Validate the same bounded file-handle bytes that are retained by the
/// caller. Unlike a check-then-reopen workflow this cannot bless different
/// contents, and growth during the read cannot exceed the allocation limit.
pub fn capture_bounded_engineering_attestation(
    configuration: &EngineeringReadConfiguration,
    artifact: &str,
    maximum_file_bytes: usize,
) -> Option<BoundedEngineeringAttestation> {
    if maximum_file_bytes == 0 || maximum_file_bytes > 16 * 1024 * 1024 {
        return None;
    }
    let paths = engineering_paths(configuration)?;
    let files = [
        open_bounded_regular(&paths[0], maximum_file_bytes)?,
        open_bounded_regular(&paths[1], maximum_file_bytes)?,
        open_bounded_regular(&paths[2], maximum_file_bytes)?,
    ];
    let a = validate_engineering_buffers(configuration, artifact, &files)?;
    Some(BoundedEngineeringAttestation {
        run: a.run,
        source_fingerprint: a.source_fingerprint,
        software_version: a.software_version,
        file_manifest_sha256: a.file_manifest_sha256,
        files,
    })
}
#[cfg(test)]
mod bounded_attestation_tests {
    #[cfg(unix)]
    #[test]
    fn fifo_never_blocks_and_replaced_symlink_is_rejected() {
        use std::time::Duration;
        let root = tempfile::tempdir().unwrap();
        let run = "0123456789ab";
        let directory = root.path().join(".runtime/browser").join(run);
        let release = directory.join("release");
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(directory.join("engineering-attestation.json"), b"{}").unwrap();
        std::fs::write(release.join("release.json"), b"{}").unwrap();
        let manifest = release.join("release-files.json");
        nix::unistd::mkfifo(
            &manifest,
            nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
        )
        .unwrap();
        let configuration = super::EngineeringReadConfiguration {
            run: run.into(),
            environment: "local".into(),
            port: "3104".into(),
            browser_simulation_port: "3104".into(),
            data_directory: directory,
        };
        let (send, receive) = std::sync::mpsc::channel();
        let config = configuration.clone();
        std::thread::spawn(move || {
            let result =
                super::capture_bounded_engineering_attestation(&config, "unbuilt", 1024).is_none();
            let _ = send.send(result);
        });
        assert!(
            receive
                .recv_timeout(Duration::from_secs(2))
                .expect("FIFO open blocked")
        );
        std::fs::remove_file(&manifest).unwrap();
        std::fs::write(&manifest, b"regular").unwrap();
        let paths = super::engineering_paths(&configuration).unwrap();
        assert!(super::open_bounded_regular(&paths[2], 1024).is_some());
        assert!(super::open_bounded_regular(&release, 1024).is_none());
        // Replace the last component after canonicalization but before open.
        let moved = release.join("moved.json");
        std::fs::rename(&manifest, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &manifest).unwrap();
        assert!(super::open_bounded_regular(&paths[2], 1024).is_none());
        assert!(
            super::capture_bounded_engineering_attestation(&configuration, "unbuilt", 1024)
                .is_none()
        );
        std::fs::remove_file(&manifest).unwrap();
        std::fs::rename(&moved, &manifest).unwrap();
        let paths = super::engineering_paths(&configuration).unwrap();
        let moved_release = root.path().join("moved-release");
        std::fs::rename(&release, &moved_release).unwrap();
        std::os::unix::fs::symlink(&moved_release, &release).unwrap();
        assert!(super::open_bounded_regular(&paths[2], 1024).is_none());
    }
    #[test]
    fn read_bound_applies_to_actual_stream_not_reported_size() {
        struct Growing {
            emitted: usize,
        }
        impl std::io::Read for Growing {
            fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
                // A file initially appearing short grows while its already-open
                // handle is consumed. No metadata-sized allocation is trusted.
                let available = if self.emitted == 0 {
                    4
                } else {
                    513usize.saturating_sub(self.emitted)
                };
                let n = available.min(output.len());
                output[..n].fill(b'x');
                self.emitted += n;
                Ok(n)
            }
        }
        assert!(super::read_bounded(Growing { emitted: 0 }, 512).is_none());
        assert_eq!(
            super::read_bounded(std::io::Cursor::new(vec![0u8; 512]), 512)
                .unwrap()
                .len(),
            512
        );
    }
}

/// Acquire this complete owner bundle before the fresh Holding's first SELECT.
/// A table lease is not authority and cannot refresh an older snapshot.
pub async fn lock_legal_release_sources(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("LEGAL_RELEASE_OWNER_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE company_release_acceptances,company_release_gate_reviews,company_release_revisions,company_legal_release_revisions,engineering_verification_contexts IN SHARE MODE").execute(&mut *connection).await?;
    lock_release_reviewer_sources(&mut *connection).await?;
    lock_business_acceptance_sources(connection).await
}
#[derive(Clone)]
pub struct LegalReleaseWitness {
    company: CompanyId,
    transaction: (String, i32, String),
    revision: String,
}
impl LegalReleaseWitness {
    pub fn revision(&self) -> &str {
        &self.revision
    }
}
pub struct LegalReleaseInspection {
    pub accepted: bool,
    pub engineering_allowed: bool,
    pub revision: String,
    pub witness: LegalReleaseWitness,
}
pub struct PgLegalReleaseReader<'c> {
    connection: &'c mut PgConnection,
    artifact: &'c str,
    catalogue: &'c LegalReleaseCatalogue,
    engineering: Option<&'c EngineeringReadConfiguration>,
}
impl<'c> PgLegalReleaseReader<'c> {
    pub fn new(
        connection: &'c mut PgConnection,
        artifact: &'c str,
        catalogue: &'c LegalReleaseCatalogue,
        engineering: Option<&'c EngineeringReadConfiguration>,
    ) -> Result<Self, BusinessError> {
        if artifact.is_empty() || artifact.len() > 200 {
            return Err(invalid());
        }
        Ok(Self {
            connection,
            artifact,
            catalogue,
            engineering,
        })
    }
    pub async fn revalidate(
        &mut self,
        company: &CompanyId,
        now: TimestampMillis,
        witness: &LegalReleaseWitness,
    ) -> Result<LegalReleaseInspection, Error> {
        let current = self.inspect(company, now).await?;
        if witness.company != *company
            || witness.transaction != current.witness.transaction
            || witness.revision != current.revision
        {
            return Err(BusinessError::invalid(
                "LEGAL_RELEASE_CHANGED",
                "The current release evidence changed.",
            )
            .into());
        }
        Ok(current)
    }
}
impl LegalReleaseReadPort for PgLegalReleaseReader<'_> {
    type Error = Error;
    type Inspection = LegalReleaseInspection;
    async fn inspect(
        &mut self,
        company: &CompanyId,
        _now: TimestampMillis,
    ) -> Result<LegalReleaseInspection, Error> {
        lock_legal_release_sources(self.connection).await?;
        let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?;
        if isolation != "serializable" {
            return Err(PersistenceError::new("LEGAL_RELEASE_SERIALIZABLE_REQUIRED").into());
        }
        let row = sqlx::query(
            "SELECT current_database() db,pg_backend_pid() pid,pg_current_xact_id()::text tx",
        )
        .fetch_one(&mut *self.connection)
        .await?;
        let transaction = (
            row.try_get::<String, _>("db")?,
            row.try_get::<i32, _>("pid")?,
            row.try_get::<String, _>("tx")?,
        );
        let latest=sqlx::query("SELECT id,software_version,configuration_revision,setup_version,requested_by,revoked_at FROM company_release_acceptances WHERE company_id=$1 AND scope_version=$2 ORDER BY created_at DESC,id DESC LIMIT 1 FOR SHARE")
            .bind(company.as_str()).bind(self.catalogue.scope_version()).fetch_optional(&mut *self.connection).await?;
        let latest = latest
            .map(|r| {
                Ok::<_, sqlx::Error>(LegalAcceptance {
                    id: r.try_get("id")?,
                    software_version: r.try_get("software_version")?,
                    configuration_revision: r.try_get("configuration_revision")?,
                    setup_version: r.try_get("setup_version")?,
                    requested_by: r.try_get("requested_by")?,
                    revoked_at: r.try_get("revoked_at")?,
                })
            })
            .transpose()?;
        let configuration_revision = sqlx::query_scalar::<_, i64>(
            "SELECT revision FROM company_legal_release_revisions WHERE company_id=$1 FOR SHARE",
        )
        .bind(company.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        let setup = PgCompanyBusinessAcceptanceReader::new(&mut *self.connection)
            .current(company)
            .await?;
        let mut gates = Vec::new();
        if let Some(release) = &latest {
            let rows=sqlx::query("SELECT criterion_id,state,permission,assignment_id,reviewer_id FROM company_release_gate_reviews WHERE company_id=$1 AND release_id=$2 ORDER BY criterion_id FOR SHARE")
                .bind(company.as_str()).bind(&release.id).fetch_all(&mut *self.connection).await?;
            for row in rows {
                let permission: String = row.try_get("permission")?;
                let eligibility = PgReleaseReviewerReader::new(&mut *self.connection)
                    .inspect(&ReleaseReviewerRequest {
                        company_id: company.clone(),
                        assignment_id: RecordId::new(row.try_get::<String, _>("assignment_id")?)?,
                        reviewer_id: UserId::new(row.try_get::<String, _>("reviewer_id")?)?,
                        requested_by: UserId::new(release.requested_by.clone())?,
                        permission: permission.clone(),
                    })
                    .await?;
                gates.push(LegalGateFact {
                    criterion_id: row.try_get("criterion_id")?,
                    state: row.try_get("state")?,
                    permission,
                    reviewer_current: eligibility.allowed,
                    reviewer_revision: eligibility.revision,
                });
            }
        }
        let facts = LegalReleaseFacts {
            latest,
            configuration_revision,
            setup_status: setup.as_ref().map(|s| s.status.clone()),
            setup_version: setup.map(|s| s.version),
            gates,
        };
        let accepted = accepted_legal_release(self.catalogue, self.artifact, &facts);
        let engineering_context = self.engineering.and_then(|c| attestation(c, self.artifact));
        let engineering_allowed = if let Some(a) = &engineering_context {
            let databases = [
                format!("erpx_browser_{}_h001_db", a.run),
                format!("erpx_browser_{}_h002_db", a.run),
                format!("erpx_browser_{}_h003_db", a.run),
            ];
            databases.contains(&transaction.0) && sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM engineering_verification_contexts WHERE company_id=$1 AND run_id=$2 AND source_fingerprint=$3 AND software_version=$4 AND release_manifest_sha256=$5 AND enabled AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)")
                .bind(company.as_str()).bind(&a.run).bind(&a.source_fingerprint).bind(&a.software_version).bind(&a.file_manifest_sha256).fetch_one(&mut *self.connection).await?
        } else {
            false
        };
        let revision = hex::encode(Sha256::digest(
            serde_json::to_vec(&(
                company.as_str(),
                self.catalogue.scope_version(),
                self.catalogue.criteria(),
                self.artifact,
                &facts,
                accepted,
                &engineering_context,
                engineering_allowed,
            ))
            .map_err(|_| invalid())?,
        ));
        Ok(LegalReleaseInspection {
            accepted,
            engineering_allowed,
            revision: revision.clone(),
            witness: LegalReleaseWitness {
                company: company.clone(),
                transaction,
                revision,
            },
        })
    }
}

/// Diagnostic read of the same file-bound predicate used by the SQL adapter.
/// A match is not a database enrollment or permission grant.
pub fn engineering_attestation_matches(
    configuration: &EngineeringReadConfiguration,
    artifact: &str,
) -> bool {
    attestation(configuration, artifact).is_some()
}
