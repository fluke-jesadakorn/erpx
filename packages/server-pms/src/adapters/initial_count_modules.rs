//! PMS-owned exact Service initial-Count module conjunction.
use crate::domain::initial_count_modules::{InitialCountModuleMode, initial_count_modules_allowed};
use erpx_server_platform_administration::public::legal_release::{
    BoundedEngineeringAttestation, EngineeringReadConfiguration,
    capture_bounded_engineering_attestation,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
const FILE_LIMIT: usize = 16 * 1024 * 1024;
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "INITIAL_COUNT_MODULE_SOURCE_CHANGED",
        "The current Count module availability changed.",
    )
    .into()
}
fn limit() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "INITIAL_COUNT_MODULE_SOURCE_LIMIT",
        "The scheduled module source exceeds its bounded snapshot budget.",
    )
    .into()
}
#[derive(Clone, PartialEq, Eq)]
struct Unit(String, i32, String, String);
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("SCHEDULED_RELEASE_OWNER_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("SCHEDULED_RELEASE_SERIALIZABLE_REQUIRED").into());
    }
    let(a,b,d,e)=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?;
    Ok(Unit(a, b, d, e))
}
/// Utility-only lease, before the first snapshot. Development never reads the
/// scheduled tables, matching `assertProductionModuleEnabled`.
pub async fn lock_initial_count_module_sources(
    c: &mut PgConnection,
    mode: InitialCountModuleMode,
) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("SCHEDULED_RELEASE_OWNER_TRANSACTION_REQUIRED").into());
    }
    if mode == InitialCountModuleMode::Production {
        sqlx::query("LOCK TABLE company_feature_rollouts IN SHARE MODE")
            .execute(&mut *c)
            .await?;
        sqlx::query("LOCK TABLE company_release_acceptances,company_release_gate_reviews,company_release_revisions,company_legal_release_revisions,engineering_verification_contexts IN SHARE MODE").execute(c).await?;
    }
    Ok(())
}
#[derive(Clone, PartialEq, Eq)]
struct Image {
    rollouts: String,
    engineering_row: Option<String>,
    attestation: Option<BoundedEngineeringAttestation>,
    engineering_current: Option<bool>,
    allowed: bool,
}
async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    mode: InitialCountModuleMode,
    artifact: &str,
    config: Option<&EngineeringReadConfiguration>,
    database: &str,
) -> Result<Image, Error> {
    if mode == InitialCountModuleMode::Development {
        return Ok(Image {
            rollouts: String::new(),
            engineering_row: None,
            attestation: None,
            engineering_current: None,
            allowed: true,
        });
    }
    let rollouts:Option<String>=sqlx::query_scalar("WITH rows AS MATERIALIZED (SELECT * FROM company_feature_rollouts WHERE company_id=$1 AND module_id IN ('field_service','inventory','finance') ORDER BY module_id), image AS (SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY r.module_id),'[]'::jsonb)::text value FROM rows r) SELECT CASE WHEN NOT EXISTS(SELECT 1 FROM rows r WHERE octet_length(to_jsonb(r)::text)>65536) AND octet_length(value)<=196608 THEN value END FROM image").bind(company.as_str()).fetch_one(&mut *c).await?;
    let rollouts = rollouts.ok_or_else(limit)?;
    let attestation =
        config.and_then(|c| capture_bounded_engineering_attestation(c, artifact, FILE_LIMIT));
    let mut engineering_row = None;
    let mut engineering = false;
    let mut engineering_current = None;
    if let Some(a) = &attestation {
        let row=sqlx::query("SELECT CASE WHEN octet_length(to_jsonb(e)::text)<=65536 THEN to_jsonb(e)::text END image, (enabled AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND source_fingerprint=$3 AND software_version=$4 AND release_manifest_sha256=$5) valid FROM engineering_verification_contexts e WHERE company_id=$1 AND run_id=$2").bind(company.as_str()).bind(a.run()).bind(a.source_fingerprint()).bind(a.software_version()).bind(a.file_manifest_sha256()).fetch_optional(&mut *c).await?;
        if let Some(row) = row {
            engineering_row = Some(
                row.try_get::<Option<String>, _>("image")?
                    .ok_or_else(limit)?,
            );
            let db_match =
                (1..=3).any(|i| database == format!("erpx_browser_{}_h{i:03}_db", a.run()));
            let valid = row.try_get::<bool, _>("valid")?;
            engineering_current = Some(valid);
            engineering = db_match && valid;
        }
    }
    Ok(Image {
        rollouts,
        engineering_row,
        attestation,
        engineering_current,
        allowed: initial_count_modules_allowed(mode, engineering),
    })
}
/// Same-transaction evidence; no identity or purchase rights are implied.
pub struct CurrentInitialCountModules {
    company: CompanyId,
    mode: InitialCountModuleMode,
    artifact: String,
    config: Option<EngineeringReadConfiguration>,
    unit: Unit,
    image: Image,
}
impl CurrentInitialCountModules {
    pub fn allowed(&self) -> bool {
        self.image.allowed
    }
    /// Recaptures original rows/files and recomputes the actual DB-clock expiry.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let current = unit(c).await?;
        if current != self.unit
            || capture(
                c,
                &self.company,
                self.mode,
                &self.artifact,
                self.config.as_ref(),
                &current.0,
            )
            .await?
                != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// Artifact and optional engineering configuration come from trusted runtime
/// configuration, never request data. Only a sealed attestation and matching
/// enrolled browser database can make production available.
pub async fn read_initial_count_modules(
    c: &mut PgConnection,
    company: &CompanyId,
    mode: InitialCountModuleMode,
    artifact: &str,
    config: Option<&EngineeringReadConfiguration>,
) -> Result<CurrentInitialCountModules, Error> {
    if mode == InitialCountModuleMode::Production && artifact.len() > 200 {
        return Err(limit());
    }
    let transaction = unit(c).await?;
    let image = capture(c, company, mode, artifact, config, &transaction.0).await?;
    Ok(CurrentInitialCountModules {
        company: company.clone(),
        mode,
        artifact: artifact.into(),
        config: config.cloned(),
        unit: transaction,
        image,
    })
}
