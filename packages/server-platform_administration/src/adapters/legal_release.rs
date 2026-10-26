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
fn attestation(
    configuration: &EngineeringReadConfiguration,
    artifact: &str,
) -> Option<Attestation> {
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
    let a: Attestation = serde_json::from_slice(&fs::read(attestation_path).ok()?).ok()?;
    // Legacy release.json carries additional artifact metadata. Only its three
    // source identity fields participate in this existing predicate.
    let info: serde_json::Value =
        serde_json::from_slice(&fs::read(release.join("release.json")).ok()?).ok()?;
    let digest = hex::encode(Sha256::digest(fs::read(manifest).ok()?));
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
