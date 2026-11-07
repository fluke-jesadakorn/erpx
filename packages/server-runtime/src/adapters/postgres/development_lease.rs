//! The deliberately narrow bootstrap permit for disposable native-worker tests.
//! Endpoint checks happen before any connection; server ownership/lease checks
//! happen before a caller can obtain the pool. It never creates/migrates/drops DBs.
use super::{
    PersistenceError,
    config::DatabaseConfig,
    pool::{DatabasePool, PoolLimits, connect_pool},
};
use sqlx::Row;
#[derive(Clone)]
pub struct LeasedDevelopmentDatabase {
    configuration: DatabaseConfig,
    database: String,
    lease: String,
}
impl std::fmt::Debug for LeasedDevelopmentDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LeasedDevelopmentDatabase([REDACTED])")
    }
}
impl LeasedDevelopmentDatabase {
    pub fn parse(url: &str, token: &str) -> Result<Self, PersistenceError> {
        let endpoint = url::Url::parse(url)
            .map_err(|_| PersistenceError::new("EXACT_LEASED_DEVELOPMENT_DATABASE_REQUIRED"))?;
        let expected = format!("erpx_dev_rust_{token}");
        if endpoint.host_str() != Some("127.0.0.1")
            || endpoint.port() != Some(5432)
            || endpoint.username() != "erpx_test_admin"
            || endpoint.password().is_none_or(str::is_empty)
            || endpoint.path() != format!("/{expected}")
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(PersistenceError::new(
                "EXACT_LEASED_DEVELOPMENT_DATABASE_REQUIRED",
            ));
        }
        let mut configuration = DatabaseConfig::from_url(url)?;
        if token.len() != 32
            || !token
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            || configuration.options.get_socket().is_some()
            || configuration.options.get_options().is_some()
            || configuration.options.get_host() != "127.0.0.1"
            || configuration.options.get_port() != 5432
            || configuration.options.get_username() != "erpx_test_admin"
            || configuration.options.get_database()
                != Some(format!("erpx_dev_rust_{token}").as_str())
        {
            return Err(PersistenceError::new(
                "EXACT_LEASED_DEVELOPMENT_DATABASE_REQUIRED",
            ));
        }
        configuration.options = configuration
            .options
            .ssl_mode(sqlx::postgres::PgSslMode::Disable)
            .application_name("erpx-leased-development-worker");
        Ok(Self {
            configuration,
            database: format!("erpx_dev_rust_{token}"),
            lease: format!("erpx-rust-test:{token}"),
        })
    }
    pub async fn connect(&self, limits: PoolLimits) -> Result<DatabasePool, PersistenceError> {
        self.connect_named(limits, "erpx-leased-development-worker").await
    }
    /// A generated process identity permits cancellation of only this worker's
    /// queries at shutdown; it changes neither endpoint nor lease authority.
    pub async fn connect_named(&self, limits: PoolLimits, application: &str) -> Result<DatabasePool, PersistenceError> {
        if application.is_empty() || application.len() > 63 || !application.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'/')) {
            return Err(PersistenceError::new("DEVELOPMENT_WORKER_IDENTITY_INVALID"));
        }
        if limits.max_connections > 3 {
            return Err(PersistenceError::new("DEVELOPMENT_WORKER_POOL_LIMIT"));
        }
        let mut configuration = self.configuration.clone();
        configuration.options = configuration.options.application_name(application);
        let pool = connect_pool(&configuration, limits).await?;
        let result=async{
            let row=sqlx::query("SELECT current_database() database,current_user::text role,session_user::text session_role,pg_get_userbyid(d.datdba)::text owner,shobj_description(d.oid,'pg_database') lease,r.rolsuper,r.rolcreaterole FROM pg_database d JOIN pg_roles r ON r.rolname=current_user WHERE d.datname=current_database()").fetch_one(pool.raw()).await?;
            if row.try_get::<String,_>("database")?!=self.database||row.try_get::<String,_>("role")?!="erpx_test_admin"||row.try_get::<String,_>("session_role")?!="erpx_test_admin"||row.try_get::<String,_>("owner")?!="erpx_test_admin"||row.try_get::<Option<String>,_>("lease")?.as_deref()!=Some(self.lease.as_str())||row.try_get::<bool,_>("rolsuper")?||row.try_get::<bool,_>("rolcreaterole")?{
                return Err(PersistenceError::new("DEVELOPMENT_DATABASE_LEASE_NOT_CURRENT"));
            }Ok(())
        }.await;
        if let Err(error) = result {
            pool.close().await;
            return Err(error);
        }
        Ok(pool)
    }
}
