//! Production Finance composition gate.
//!
//! `FinanceJournalEnvironment::Production` is accepted only when the current
//! Company carries an unrevoked, latest, configuration-current production
//! release acceptance whose independent gates are still held by their current
//! reviewers, and whose Company setup acceptance matches the release. This is
//! a read port inside the same serializable owner transaction; it grants no
//! personal permission and never falls back to development semantics.
use erpx_server_platform_administration::public::production_release::{
    PgProductionReleaseReader, ProductionReleaseReadPort, current_production_release_catalogue,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;

/// The release artifact identity is generated before a production binary is
/// compiled; development source compiles `unbuilt`, which can never match an
/// accepted release.
fn current_software_version() -> String {
    static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VERSION
        .get_or_init(|| {
            serde_json::from_str::<serde_json::Value>(include_str!(
                "../../../packages/catalogue/src/release-artifact.json"
            ))
            .ok()
            .and_then(|value| {
                value
                    .get("softwareVersion")
                    .and_then(|version| version.as_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "unbuilt".to_owned())
        })
        .clone()
}

pub(crate) async fn require_production_release(
    connection: &mut PgConnection,
    company: &CompanyId,
) -> Result<(), Error> {
    let catalogue = current_production_release_catalogue().map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "FINANCE_PRODUCTION_RELEASE_CATALOGUE_UNAVAILABLE",
            "Current production release configuration is required.",
        )
    })?;
    let artifact = current_software_version();
    let now = sqlx::query_scalar::<_, i64>(
        "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
    )
    .fetch_one(&mut *connection)
    .await?;
    let inspection = PgProductionReleaseReader::new(connection, &artifact, &catalogue)?
        .inspect(company, TimestampMillis::new(now))
        .await?;
    if !inspection.accepted {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "FINANCE_PRODUCTION_RELEASE_REQUIRED",
            "A current Company production release acceptance is required.",
        )
        .into());
    }
    Ok(())
}
