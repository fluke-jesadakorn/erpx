use crate::{application::production_release::ProductionReleaseReadPort, domain::production_release::*};
use erpx_server_identity_access::public::release_reviewers::{
    PgReleaseReviewerReader, ReleaseReviewerRequest, lock_release_reviewer_sources,
};
use erpx_server_identity_access::public::permission_catalog_lock::lock_permission_catalog_sources;
use erpx_server_organization::public::{
    PgCompanyBusinessAcceptanceReader, lock_business_acceptance_sources,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis, UserId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Resource {
    schema: String,
    #[allow(dead_code)]
    source: serde_json::Value,
    scope_version: String,
    criteria: Vec<Criterion>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Criterion {
    id: String,
    permission: String,
}
pub fn current_production_release_catalogue() -> Result<ProductionReleaseCatalogue, BusinessError> {
    let resource: Resource = serde_json::from_str(include_str!(
        "../../resources/production-release.v1.json"
    ))
    .map_err(|_| invalid())?;
    if resource.schema != "erpx-production-release/v1"
        || resource.scope_version != "thai-hotel-v1-crm-analytics"
        || resource.criteria.len() != 15
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
                "organization.manage"
                    | "pms.approve"
                    | "commerce.approve"
                    | "records.write"
                    | "pos.approve"
                    | "people.manage"
                    | "payroll.approve"
                    | "finance.approve"
            )
    }) {
        return Err(invalid());
    }
    Ok(ProductionReleaseCatalogue {
        scope_version: resource.scope_version,
        criteria: resource
            .criteria
            .into_iter()
            .map(|c| ProductionReleaseCriterion {
                id: c.id,
                permission: c.permission,
            })
            .collect(),
    })
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PRODUCTION_RELEASE_CONFIGURATION_INVALID",
        "Current production release configuration is required.",
    )
}

/// Acquire this complete owner bundle before the fresh Holding's first SELECT.
pub async fn lock_production_release_sources(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new(
            "PRODUCTION_RELEASE_OWNER_TRANSACTION_REQUIRED",
        )
        .into());
    }
    sqlx::query("LOCK TABLE company_release_acceptances,company_release_gate_reviews,company_release_revisions IN SHARE MODE")
        .execute(&mut *connection)
        .await?;
    lock_release_reviewer_sources(&mut *connection).await?;
    lock_business_acceptance_sources(&mut *connection).await?;
    lock_permission_catalog_sources(&mut *connection).await
}
pub struct ProductionReleaseInspection {
    pub accepted: bool,
}
pub struct PgProductionReleaseReader<'c> {
    connection: &'c mut PgConnection,
    artifact: &'c str,
    catalogue: &'c ProductionReleaseCatalogue,
}
impl<'c> PgProductionReleaseReader<'c> {
    pub fn new(
        connection: &'c mut PgConnection,
        artifact: &'c str,
        catalogue: &'c ProductionReleaseCatalogue,
    ) -> Result<Self, BusinessError> {
        if artifact.is_empty() || artifact.len() > 200 {
            return Err(invalid());
        }
        Ok(Self {
            connection,
            artifact,
            catalogue,
        })
    }
}
impl ProductionReleaseReadPort for PgProductionReleaseReader<'_> {
    type Error = Error;
    type Inspection = ProductionReleaseInspection;
    async fn inspect(
        &mut self,
        company: &CompanyId,
        _now: TimestampMillis,
    ) -> Result<Self::Inspection, Self::Error> {
        lock_production_release_sources(self.connection).await?;
        let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?;
        if isolation != "serializable" {
            return Err(PersistenceError::new(
                "PRODUCTION_RELEASE_SERIALIZABLE_REQUIRED",
            )
            .into());
        }
        let latest = sqlx::query("SELECT id,software_version,configuration_revision,setup_version,requested_by,revoked_at FROM company_release_acceptances WHERE company_id=$1 AND scope_version=$2 ORDER BY created_at DESC,id DESC LIMIT 1 FOR SHARE")
            .bind(company.as_str()).bind(self.catalogue.scope_version()).fetch_optional(&mut *self.connection).await?;
        let latest = latest
            .map(|r| {
                Ok::<_, sqlx::Error>(ProductionAcceptance {
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
            "SELECT revision FROM company_release_revisions WHERE company_id=$1 FOR SHARE",
        )
        .bind(company.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        let setup = PgCompanyBusinessAcceptanceReader::new(&mut *self.connection)
            .current(company)
            .await?;
        let mut gates = Vec::new();
        if let Some(release) = &latest {
            let rows = sqlx::query("SELECT criterion_id,state,permission,assignment_id,reviewer_id FROM company_release_gate_reviews WHERE company_id=$1 AND release_id=$2 ORDER BY criterion_id FOR SHARE")
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
                gates.push(ProductionGateFact {
                    criterion_id: row.try_get("criterion_id")?,
                    state: row.try_get("state")?,
                    permission,
                    reviewer_current: eligibility.allowed,
                    reviewer_revision: eligibility.revision,
                });
            }
        }
        let facts = ProductionReleaseFacts {
            latest,
            configuration_revision,
            setup_status: setup.as_ref().map(|s| s.status.clone()),
            setup_version: setup.map(|s| s.version),
            gates,
        };
        Ok(ProductionReleaseInspection {
            accepted: accepted_production_release(self.catalogue, self.artifact, &facts),
        })
    }
}
