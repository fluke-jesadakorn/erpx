//! Directory routing/current-source adapter. Configured pools are explicit;
//! a connection/company string can never select an ambient database fallback.
use super::{
    account_holding_bindings::{ConfiguredHoldingRegistry, CurrentHoldingBinding},
    directory_profiles::{DirectoryProfileLease, DirectoryProfileStore},
};
use crate::{
    application::{
        account_ports::{AccountFuture, AuthenticationSecrets},
        account_revocation::AccountWorkContextInvalidation,
        directory::{DirectoryRoutes, VerifiedDirectorySubject},
    },
    domain::{account_auth::RequestAudience, directory::*},
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgRequestLease, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, HoldingId};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Postgres, Row, Transaction};
use std::{collections::BTreeMap, sync::Arc};

pub struct DirectoryHoldingDatabase {
    pub audience: String,
    pub write_binding: String,
    pub pool: DatabasePool,
}
pub struct DirectoryDatabaseBindings {
    pub control: DatabasePool,
    pub holdings: BTreeMap<HoldingId, DirectoryHoldingDatabase>,
    pub registry: Arc<ConfiguredHoldingRegistry>,
    pub environment: String,
}
#[derive(Clone)]
pub struct PostgresDirectory {
    pub(super) control: DatabasePool,
    pub(super) holdings: Arc<BTreeMap<HoldingId, DirectoryHoldingDatabase>>,
    pub(super) registry: Arc<ConfiguredHoldingRegistry>,
    pub(super) profiles: Arc<DirectoryProfileStore>,
    pub(super) clock: Arc<dyn Clock + Send + Sync>,
    pub(super) secrets: Arc<dyn AuthenticationSecrets>,
    pub(super) invalidation: Arc<dyn AccountWorkContextInvalidation>,
    pub(super) environment: String,
}
impl PostgresDirectory {
    pub fn new(
        bindings: DirectoryDatabaseBindings,
        profiles: Arc<DirectoryProfileStore>,
        clock: Arc<dyn Clock + Send + Sync>,
        secrets: Arc<dyn AuthenticationSecrets>,
        invalidation: Arc<dyn AccountWorkContextInvalidation>,
    ) -> Result<Self, BusinessError> {
        let DirectoryDatabaseBindings {
            control,
            holdings,
            registry,
            environment,
        } = bindings;
        if environment != "development" || profiles.provenance_keys.environment() != environment {
            return Err(unavailable());
        }
        if holdings.len() != registry.configured().count() {
            return Err(unavailable());
        }
        for (id, entry) in &holdings {
            let expected = registry.expected_for_audience(&RequestAudience::Holding {
                audience: entry.audience.clone(),
            })?;
            if &expected.holding_id != id
                || expected.write_binding != entry.write_binding
                || entry.pool.raw().options().get_max_connections() < 2
            {
                return Err(unavailable());
            }
        }
        Ok(Self {
            control,
            holdings: Arc::new(holdings),
            registry,
            profiles,
            clock,
            secrets,
            invalidation,
            environment,
        })
    }
    /// Caller acquires this before the first Control SELECT, after any Account
    /// session/device locks. Utility statements do not establish a snapshot.
    pub(super) async fn lock_routes(connection: &mut PgConnection) -> Result<(), sqlx::Error> {
        sqlx::query("LOCK TABLE holding_bindings,iam_connection_routes IN SHARE MODE")
            .execute(connection)
            .await?;
        Ok(())
    }
    pub(super) async fn route(&self, control: &mut PgConnection, id: &str) -> Tx<DirectoryRoute> {
        let row=sqlx::query("SELECT holding_id IS NULL control_scope,CASE WHEN octet_length(holding_id)<=200 THEN holding_id END holding_id,CASE WHEN octet_length(company_id)<=200 THEN company_id END company_id FROM iam_connection_routes WHERE connection_id=$1 FOR SHARE").bind(id).fetch_optional(&mut *control).await?.ok_or_else(unavailable)?;
        let company: String = row
            .try_get::<Option<String>, _>("company_id")?
            .ok_or_else(unavailable)?;
        let control_scope: bool = row.try_get("control_scope")?;
        let holding: Option<String> = row.try_get("holding_id")?;
        if !control_scope {
            let id = holding.ok_or_else(unavailable)?;
            let holding_id = HoldingId::new(id)?;
            let configured = self.holdings.get(&holding_id).ok_or_else(unavailable)?;
            let binding = self
                .registry
                .read_current(
                    control,
                    &RequestAudience::Holding {
                        audience: configured.audience.clone(),
                    },
                )
                .await?;
            if binding.holding_id() != &holding_id || company == "platform" {
                return Err(unavailable().into());
            }
            Ok(DirectoryRoute {
                company_id: company,
                holding_id: Some(holding_id),
                binding: Some(binding),
                pool: configured.pool.clone(),
            })
        } else if company == "platform" {
            // Control has no mirrored provenance migration yet; never admit an unversioned platform source.
            Err(
                crate::domain::directory_provenance::Denial::SourceNotApproved
                    .business()
                    .into(),
            )
        } else {
            Err(unavailable().into())
        }
    }
    /// Normal sign-in retains only Directory eligibility tables here. Holding
    /// tombstones/context retirement uses other tables and can commit without
    /// deadlocking this proof. No Organization/Work permission is asserted.
    pub(super) async fn retain_identity(
        &self,
        control: &mut PgConnection,
        descriptor: &DirectoryConnectionDescriptor,
        subject: &VerifiedDirectorySubject,
    ) -> Tx<RetainedDirectoryIdentity> {
        let profile = self
            .profiles
            .retain(
                &descriptor.config.secret_profile,
                &descriptor.connection_id,
                Some(&descriptor.profile_fingerprint),
            )
            .await?;
        self.profiles.require_selected(descriptor, &profile)?;
        let route = self.route(control, &descriptor.connection_id).await?;
        route.matches(descriptor)?;
        let mut holding = if route.holding_id.is_some() {
            Some(
                route
                    .pool
                    .raw()
                    .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
                    .await?,
            )
        } else {
            None
        };
        if let Some(connection) = holding.as_mut() {
            limits(connection).await?;
            super::directory_mapping_postgres::lock_sources(connection, false).await?;
        } else {
            super::directory_mapping_postgres::lock_sources(control, true).await?;
        }
        let source: &mut PgConnection = if let Some(connection) = holding.as_mut() {
            connection
        } else {
            control
        };
        let connection = connection(source, descriptor.connection_id.as_str(), &route).await?;
        matches_descriptor(&connection, descriptor)?;
        if !subject.matches(descriptor) {
            return Err(unavailable().into());
        }
        let facts = identity(
            source,
            descriptor,
            subject.external_id(),
            self.clock.now().get(),
        )
        .await?;
        let mapping_witness = super::directory_mapping_postgres::policy_witness(
            source,
            descriptor,
            self.clock.now().get(),
        )
        .await?;
        Ok(RetainedDirectoryIdentity {
            holding,
            route,
            profile,
            facts,
            descriptor: descriptor.clone(),
            external_id: subject.external_id().to_owned(),
            mapping_witness,
        })
    }
}
pub(super) type Tx<T> = Result<T, PgTransactionError<BusinessError>>;
pub(super) fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "DIRECTORY_ACCESS_UNAVAILABLE",
        "Your approved Directory access needs current verification.",
    )
}
pub(super) fn finish<T>(result: Tx<T>) -> Result<T, BusinessError> {
    result.map_err(|error| match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    })
}
pub(super) struct DirectoryRoute {
    pub company_id: String,
    pub holding_id: Option<HoldingId>,
    pub binding: Option<CurrentHoldingBinding>,
    pub pool: DatabasePool,
}
impl DirectoryRoute {
    pub(super) fn matches(
        &self,
        expected: &DirectoryConnectionDescriptor,
    ) -> Result<(), BusinessError> {
        if self.company_id != expected.company_id
            || self.holding_id.as_ref().map(HoldingId::as_str) != expected.holding_id.as_deref()
        {
            return Err(unavailable());
        }
        Ok(())
    }
    pub(super) async fn require_current(&self, control: &mut PgConnection) -> Tx<()> {
        if let Some(binding) = &self.binding {
            binding.require_current(control).await?;
        }
        Ok(())
    }
}
pub(super) async fn limits(connection: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *connection)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='15s'")
        .execute(connection)
        .await?;
    Ok(())
}
pub(super) async fn lock_identity_sources(
    connection: &mut PgConnection,
) -> Result<(), sqlx::Error> {
    sqlx::query("LOCK TABLE iam_members,iam_identities,iam_connections,iam_directory_source_selections,iam_directory_observation_sources IN SHARE MODE")
        .execute(connection)
        .await?;
    Ok(())
}
pub(super) async fn connection_without_source(
    source: &mut PgConnection,
    id: &str,
    route: &DirectoryRoute,
) -> Tx<DirectoryConnectionDescriptor> {
    let row=sqlx::query("SELECT CASE WHEN octet_length(company_id)<=200 THEN company_id END company_id,provider,status,version::bigint version,tested_version::bigint tested_version,CASE WHEN octet_length(config::text)<=65536 THEN config::text END config FROM iam_connections WHERE id=$1 FOR SHARE").bind(id).fetch_optional(source).await?.ok_or_else(unavailable)?;
    let company: String = row
        .try_get::<Option<String>, _>("company_id")?
        .ok_or_else(unavailable)?;
    let version: i64 = row.try_get("version")?;
    let config: String = row
        .try_get::<Option<String>, _>("config")?
        .ok_or_else(unavailable)?;
    if company != route.company_id || row.try_get::<&str, _>("status")? != "active" || version <= 0
    {
        return Err(unavailable().into());
    }
    let config_value: DirectoryConnectionConfig =
        serde_json::from_str(&config).map_err(|_| unavailable())?;
    if config_value.secret_profile.is_empty()
        || config_value.secret_profile.len() > 80
        || !config_value
            .secret_profile
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return Err(unavailable().into());
    }
    Ok(DirectoryConnectionDescriptor {
        source: None,
        connection_id: id.to_owned(),
        holding_id: route.holding_id.as_ref().map(|id| id.as_str().to_owned()),
        company_id: company,
        provider: match row.try_get::<&str, _>("provider")? {
            "active_directory" => DirectoryProvider::ActiveDirectory,
            "univention" => DirectoryProvider::Univention,
            "entra_id" => DirectoryProvider::EntraId,
            _ => return Err(unavailable().into()),
        },
        mode: config_value.mode,
        version,
        tested_version: row.try_get("tested_version")?,
        config_fingerprint: hex::encode(Sha256::digest(config.as_bytes())),
        profile_fingerprint: String::new(),
        config: config_value,
    })
}
pub(super) async fn connection(
    source: &mut PgConnection,
    id: &str,
    route: &DirectoryRoute,
) -> Tx<DirectoryConnectionDescriptor> {
    let mut descriptor = connection_without_source(source, id, route).await?;
    descriptor.source =
        Some(super::directory_source_postgres::selected_for_descriptor(source, &descriptor).await?);
    Ok(descriptor)
}
pub(super) fn matches_descriptor(
    actual: &DirectoryConnectionDescriptor,
    expected: &DirectoryConnectionDescriptor,
) -> Result<(), BusinessError> {
    if actual.source != expected.source {
        return Err(
            crate::domain::directory_provenance::Denial::ConfigurationNotSelected.business(),
        );
    }
    if actual.connection_id != expected.connection_id
        || actual.company_id != expected.company_id
        || actual.holding_id != expected.holding_id
        || actual.version != expected.version
        || actual.mode != expected.mode
        || actual.provider != expected.provider
        || actual.config_fingerprint != expected.config_fingerprint
    {
        return Err(unavailable());
    }
    Ok(())
}
pub(super) async fn identity(
    source: &mut PgConnection,
    descriptor: &DirectoryConnectionDescriptor,
    external_id: &str,
    now: i64,
) -> Tx<ApprovedDirectoryIdentity> {
    let row=sqlx::query("SELECT CASE WHEN octet_length(i.id)<=200 THEN i.id END identity_id,CASE WHEN octet_length(i.user_id)<=200 THEN i.user_id END identity_user_id,CASE WHEN octet_length(i.company_id)<=200 THEN i.company_id END identity_company_id,CASE WHEN octet_length(i.connection_id)<=200 THEN i.connection_id END identity_connection_id,i.enabled,i.observed_at,CASE WHEN octet_length(m.user_id)<=200 THEN m.user_id END user_id,CASE WHEN octet_length(m.company_id)<=200 THEN m.company_id END member_company_id,CASE WHEN octet_length(m.principal_id)<=200 THEN m.principal_id END principal_id,m.person_id IS NOT NULL person_present,CASE WHEN octet_length(m.person_id)<=200 THEN m.person_id END person_id,m.status,m.login_policy,m.version::bigint member_version FROM iam_identities i JOIN iam_members m ON m.identity_id=i.id AND m.company_id=i.company_id AND m.user_id=i.user_id AND m.connection_id=i.connection_id WHERE i.connection_id=$1 AND i.external_id=$2 FOR SHARE OF i,m").bind(&descriptor.connection_id).bind(external_id).fetch_optional(&mut *source).await?.ok_or_else(unavailable)?;
    let text = |name| -> Tx<String> {
        row.try_get::<Option<String>, _>(name)?
            .ok_or_else(|| unavailable().into())
    };
    let person: Option<String> = row.try_get("person_id")?;
    if row.try_get::<bool, _>("person_present")? && person.is_none() {
        return Err(unavailable().into());
    }
    let facts = ApprovedDirectoryIdentity {
        user_id: text("user_id")?,
        principal_id: text("principal_id")?,
        person_id: person,
        identity_id: text("identity_id")?,
        member_version: row.try_get("member_version")?,
        observed_at: row.try_get("observed_at")?,
        connection_version: descriptor.version,
        identity_enabled: row.try_get("enabled")?,
        member_active: row.try_get::<&str, _>("status")? == "active",
        member_directory: row.try_get::<&str, _>("login_policy")? == "directory",
        identity_user_id: text("identity_user_id")?,
        identity_company_id: text("identity_company_id")?,
        member_company_id: text("member_company_id")?,
        identity_connection_id: text("identity_connection_id")?,
    };
    facts.require_fresh(descriptor, now)?;
    if super::directory_source_postgres::require_current_observation(
        source,
        descriptor,
        facts.observed_at,
        now,
    )
    .await?
        != crate::domain::directory_provenance::ObservationUse::Fresh
    {
        return Err(unavailable().into());
    }
    Ok(facts)
}
pub(super) struct RetainedDirectoryIdentity {
    holding: Option<Transaction<'static, Postgres>>,
    route: DirectoryRoute,
    profile: DirectoryProfileLease,
    pub facts: ApprovedDirectoryIdentity,
    descriptor: DirectoryConnectionDescriptor,
    external_id: String,
    mapping_witness: String,
}
impl RetainedDirectoryIdentity {
    pub(super) async fn require_current(&mut self, control: &mut PgConnection, now: i64) -> Tx<()> {
        self.route.require_current(control).await?;
        if self.profile.fingerprint() != self.descriptor.profile_fingerprint {
            return Err(unavailable().into());
        }
        let source: &mut PgConnection = if let Some(holding) = self.holding.as_mut() {
            holding
        } else {
            control
        };
        matches_descriptor(
            &connection(source, &self.descriptor.connection_id, &self.route).await?,
            &self.descriptor,
        )?;
        let current = identity(source, &self.descriptor, &self.external_id, now).await?;
        if current.user_id != self.facts.user_id
            || current.principal_id != self.facts.principal_id
            || current.person_id != self.facts.person_id
            || current.identity_id != self.facts.identity_id
            || current.member_version != self.facts.member_version
            || current.observed_at != self.facts.observed_at
        {
            return Err(unavailable().into());
        }
        if super::directory_mapping_postgres::policy_witness(source, &self.descriptor, now).await?
            != self.mapping_witness
        {
            return Err(unavailable().into());
        }
        Ok(())
    }
}
impl DirectoryRoutes for PostgresDirectory {
    fn current<'a>(&'a self, id: &'a str) -> AccountFuture<'a, DirectoryConnectionDescriptor> {
        Box::pin(async move {
            let this = self.clone();
            let id = id.to_owned();
            let mut result = finish(
                PgRequestLease::new(self.control.clone())
                    .run(move |control| {
                        Box::pin(async move {
                            Self::lock_routes(control).await?;
                            let route = this.route(control, &id).await?;
                            if route.holding_id.is_some() {
                                let mut source = route
                                    .pool
                                    .raw()
                                    .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
                                    .await?;
                                limits(&mut source).await?;
                                lock_identity_sources(&mut source).await?;
                                let descriptor = connection(&mut source, &id, &route).await?;
                                source.rollback().await?;
                                Ok(descriptor)
                            } else {
                                lock_identity_sources(control).await?;
                                connection(control, &id, &route).await
                            }
                        })
                    })
                    .await,
            )?;
            result.profile_fingerprint = self
                .profiles
                .fingerprint(&result.config.secret_profile, &result.connection_id)
                .await?;
            let lease = self
                .profiles
                .retain(
                    &result.config.secret_profile,
                    &result.connection_id,
                    Some(&result.profile_fingerprint),
                )
                .await?;
            self.profiles.require_selected(&result, &lease)?;
            Ok(result)
        })
    }
}
