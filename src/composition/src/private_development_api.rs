//! Explicit private development assembly. No environment discovery, migrations,
//! public listener, installed service, business readiness or legacy backend proxy.
use crate::{
    business_http::{BusinessHttpTarget, business_router},
    finance_journal_detail::{FinanceJournalDetailService, FinanceJournalReadConfiguration},
    finance_journal_detail_http::{
        FinanceJournalReadScopeHeaders, FinanceJournalReadTarget, finance_journal_read_router,
    },
    finance_journal_review::{
        FinanceJournalConfiguration, FinanceJournalEnvironment, FinanceJournalReviewAdministration,
    },
};
use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_identity_access::public::{
    AccountAuthentication, AccountPolicy, Argon2Passwords, AuthenticationSecrets, HoldingOrigin,
    PostgresAccountRepository, SystemAccountClock, SystemAuthenticationSecrets,
    account_auth_router,
    account_holding_bindings::{
        ConfiguredAccountHolding, ConfiguredHoldingRegistry, lock_holding_binding_sources,
    },
    account_revocation::{AccountHoldingWriter, PostgresAccountWorkContextInvalidation},
    account_session_router,
    account_sso::{
        AccountSso, PostgresAccountSso, SsoPolicy, SystemSsoCryptography, account_sso_router,
    },
    effective_authority::HoldingWorkAuthorizer,
};
use erpx_server_runtime::public::{
    foundation_router,
    postgres::{
        DatabasePool, LeasedDevelopmentDatabase, MigrationTrack, PgRequestLease, PoolLimits,
        load_migrations, verify_current_migration_inventory,
    },
    validate_foundation_address,
};
use erpx_server_shared::public::{Clock, CompanyId, HoldingId, TimestampMillis};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    io::Read,
    net::SocketAddr,
    path::Path,
    sync::Arc,
    time::Duration,
};
use tokio::sync::Semaphore;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DatabaseDocument {
    url: String,
    lease_token: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HoldingDocument {
    holding_id: String,
    slug: String,
    host: String,
    write_binding: String,
    read_binding: String,
    files_binding: String,
    database: DatabaseDocument,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FinanceReadDocument {
    holding_id: String,
    company_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfigurationDocument {
    version: u8,
    mode: String,
    transport: String,
    listen: String,
    account_host: String,
    require_email_verification: bool,
    password_workers: usize,
    maximum_requests: usize,
    request_timeout_ms: u64,
    control: DatabaseDocument,
    holdings: Vec<HoldingDocument>,
    #[serde(default)]
    finance_journal_reads: Vec<FinanceReadDocument>,
    #[serde(default)]
    finance_journal_posts: Vec<String>,
    #[serde(default)]
    finance_journal_reversals: Vec<String>,
    #[serde(default)]
    account_bootstrap: bool,
}
struct HoldingConfiguration {
    host: String,
    registration: ConfiguredAccountHolding,
    database: LeasedDevelopmentDatabase,
}
/// Only a bounded explicit parser can construct this configuration. Debug never
/// exposes database credentials, key material, cookies or private file contents.
pub struct DevelopmentApiConfiguration {
    listen: SocketAddr,
    policy: Arc<AccountPolicy>,
    password_workers: usize,
    maximum_requests: usize,
    request_timeout: Duration,
    holdings_registry: Arc<ConfiguredHoldingRegistry>,
    control: LeasedDevelopmentDatabase,
    holdings: Vec<HoldingConfiguration>,
    finance_journal_reads: BTreeMap<HoldingId, CompanyId>,
    finance_journal_posts: BTreeSet<HoldingId>,
    finance_journal_reversals: BTreeSet<HoldingId>,
    account_bootstrap: bool,
}
impl fmt::Debug for DevelopmentApiConfiguration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DevelopmentApiConfiguration([REDACTED])")
    }
}
impl DevelopmentApiConfiguration {
    pub fn parse(json: &str) -> Result<Self, &'static str> {
        if json.len() > 32 * 1024 {
            return Err("PRIVATE_API_CONFIGURATION_INVALID");
        }
        let doc: ConfigurationDocument =
            serde_json::from_str(json).map_err(|_| "PRIVATE_API_CONFIGURATION_INVALID")?;
        if doc.version != 1
            || doc.mode != "development"
            || doc.transport != "direct-loopback"
            || doc.holdings.len() != 2
            || !(1..=8).contains(&doc.password_workers)
            || !(1..=64).contains(&doc.maximum_requests)
            || !(250..=60_000).contains(&doc.request_timeout_ms)
        {
            return Err("EXPLICIT_LEASED_DEVELOPMENT_API_REQUIRED");
        }
        let listen = validate_foundation_address(&doc.listen)
            .map_err(|_| "DISPOSABLE_PRIVATE_API_PORT_REQUIRED")?;
        let control = LeasedDevelopmentDatabase::parse(&doc.control.url, &doc.control.lease_token)
            .map_err(|_| "EXACT_LEASED_API_DATABASE_REQUIRED")?;
        let mut tokens = BTreeSet::from([doc.control.lease_token]);
        let mut ids = BTreeSet::new();
        let mut slugs = BTreeSet::new();
        let mut hosts = BTreeSet::from([doc.account_host.clone()]);
        let mut writes = BTreeSet::new();
        let mut holdings = Vec::new();
        let mut origins = Vec::new();
        for h in doc.holdings {
            let holding_id =
                HoldingId::new(&h.holding_id).map_err(|_| "PRIVATE_API_HOLDING_INVALID")?;
            if !tokens.insert(h.database.lease_token.clone())
                || !ids.insert(h.holding_id.clone())
                || !slugs.insert(h.slug.clone())
                || !hosts.insert(h.host.clone())
                || !writes.insert(h.write_binding.clone())
                || h.slug.is_empty()
                || h.slug.len() > 128
                || !h
                    .slug
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
                || [&h.write_binding, &h.read_binding, &h.files_binding]
                    .iter()
                    .any(|s| {
                        s.is_empty()
                            || s.len() > 128
                            || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    })
            {
                return Err("DISTINCT_EXACT_API_HOLDINGS_REQUIRED");
            }
            let database =
                LeasedDevelopmentDatabase::parse(&h.database.url, &h.database.lease_token)
                    .map_err(|_| "EXACT_LEASED_API_DATABASE_REQUIRED")?;
            origins.push(HoldingOrigin {
                origin: format!("https://{}", h.host),
                host: h.host.clone(),
                audience: format!("holding:development:{}:{}", h.holding_id, h.host),
            });
            holdings.push(HoldingConfiguration {
                host: h.host,
                registration: ConfiguredAccountHolding {
                    holding_id,
                    slug: h.slug,
                    write_binding: h.write_binding,
                    read_binding: h.read_binding,
                    files_binding: h.files_binding,
                },
                database,
            });
        }
        if ids != BTreeSet::from(["h001".to_owned(), "h002".to_owned()]) {
            return Err("EXACT_H001_H002_RESOURCES_REQUIRED");
        }
        let mut finance_journal_reads = BTreeMap::new();
        for read in doc.finance_journal_reads {
            let holding = HoldingId::new(&read.holding_id)
                .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            let company = CompanyId::new(&read.company_id)
                .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            if !ids.contains(&read.holding_id)
                || finance_journal_reads.insert(holding, company).is_some()
            {
                return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID");
            }
        }
        let mut finance_journal_posts = BTreeSet::new();
        for id in doc.finance_journal_posts {
            let holding =
                HoldingId::new(id).map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            if !finance_journal_reads.contains_key(&holding)
                || !finance_journal_posts.insert(holding)
            {
                return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID");
            }
        }
        let mut finance_journal_reversals = BTreeSet::new();
        for id in doc.finance_journal_reversals {
            let holding =
                HoldingId::new(id).map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            if !finance_journal_reads.contains_key(&holding)
                || !finance_journal_reversals.insert(holding)
            {
                return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID");
            }
        }
        if doc.account_bootstrap && finance_journal_reads.is_empty() {
            return Err("PRIVATE_API_FINANCE_CONFIGURATION_INVALID");
        }
        let policy = Arc::new(AccountPolicy {
            account_origin: format!("https://{}", doc.account_host),
            account_host: doc.account_host,
            environment: "development".into(),
            require_email_verification: doc.require_email_verification,
            holdings: origins,
        });
        policy
            .validate()
            .map_err(|_| "PRIVATE_API_AUTHORITY_INVALID")?;
        let holdings_registry = Arc::new(
            ConfiguredHoldingRegistry::new(
                &policy,
                holdings.iter().map(|h| h.registration.clone()).collect(),
            )
            .map_err(|_| "PRIVATE_API_BINDINGS_INVALID")?,
        );
        Ok(Self {
            listen,
            policy,
            password_workers: doc.password_workers,
            maximum_requests: doc.maximum_requests,
            request_timeout: Duration::from_millis(doc.request_timeout_ms),
            holdings_registry,
            control,
            holdings,
            finance_journal_reads,
            finance_journal_posts,
            finance_journal_reversals,
            account_bootstrap: doc.account_bootstrap,
        })
    }
    pub fn from_file(path: &Path) -> Result<Self, &'static str> {
        if !path.is_absolute() {
            return Err("PRIVATE_ABSOLUTE_API_CONFIGURATION_REQUIRED");
        }
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(nix::fcntl::OFlag::O_NOFOLLOW.bits());
        }
        let file = options
            .open(path)
            .map_err(|_| "PRIVATE_API_CONFIGURATION_REQUIRED")?;
        let meta = file
            .metadata()
            .map_err(|_| "PRIVATE_API_CONFIGURATION_REQUIRED")?;
        if !meta.is_file() || meta.len() > 32 * 1024 {
            return Err("PRIVATE_API_CONFIGURATION_REQUIRED");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o077 != 0 {
                return Err("PRIVATE_API_CONFIGURATION_REQUIRED");
            }
        }
        let mut json = String::new();
        file.take(32 * 1024 + 1)
            .read_to_string(&mut json)
            .map_err(|_| "PRIVATE_API_CONFIGURATION_INVALID")?;
        Self::parse(&json)
    }
    pub fn listen_address(&self) -> SocketAddr {
        self.listen
    }
}

pub struct DevelopmentApiRuntime {
    router: Router,
    listen: SocketAddr,
    pools: Vec<DatabasePool>,
}
impl DevelopmentApiRuntime {
    pub fn router(&self) -> Router {
        self.router.clone()
    }
    pub fn listen_address(&self) -> SocketAddr {
        self.listen
    }
    pub async fn close(self) {
        for pool in &self.pools {
            pool.close().await;
        }
    }
}
/// Does not bind a port. Opens only the exact leased candidate databases and
/// performs read-only migration/registry checks before assembling real services.
pub async fn assemble_development_api(
    config: DevelopmentApiConfiguration,
    clock: Arc<dyn Clock + Send + Sync>,
    secrets: Arc<dyn AuthenticationSecrets>,
) -> Result<DevelopmentApiRuntime, &'static str> {
    let limits = PoolLimits {
        max_connections: 3,
        acquire_timeout: Duration::from_secs(5),
    };
    let control = config
        .control
        .connect(limits)
        .await
        .map_err(|_| "PRIVATE_API_DATABASE_UNAVAILABLE")?;
    let mut pools = vec![control.clone()];
    for holding in &config.holdings {
        match holding.database.connect(limits).await {
            Ok(pool) => pools.push(pool),
            Err(_) => {
                for p in &pools {
                    p.close().await;
                }
                return Err("PRIVATE_API_DATABASE_UNAVAILABLE");
            }
        }
    }
    let result = assemble_checked(&config, &pools, clock, secrets).await;
    match result {
        Ok(router) => Ok(DevelopmentApiRuntime {
            router,
            listen: config.listen,
            pools,
        }),
        Err(error) => {
            for pool in &pools {
                pool.close().await;
            }
            Err(error)
        }
    }
}
async fn assemble_checked(
    config: &DevelopmentApiConfiguration,
    pools: &[DatabasePool],
    clock: Arc<dyn Clock + Send + Sync>,
    secrets: Arc<dyn AuthenticationSecrets>,
) -> Result<Router, &'static str> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let control_files = load_migrations(&root, MigrationTrack::Control)
        .await
        .map_err(|_| "PRIVATE_API_REVIEWED_SCHEMA_UNAVAILABLE")?;
    let holding_files = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .map_err(|_| "PRIVATE_API_REVIEWED_SCHEMA_UNAVAILABLE")?;
    verify_current_migration_inventory(&pools[0], MigrationTrack::Control, &control_files)
        .await
        .map_err(|_| "PRIVATE_API_SCHEMA_MISMATCH")?;
    for pool in &pools[1..] {
        verify_current_migration_inventory(pool, MigrationTrack::HoldingWrite, &holding_files)
            .await
            .map_err(|_| "PRIVATE_API_SCHEMA_MISMATCH")?;
    }
    let registry = config.holdings_registry.clone();
    PgRequestLease::new(pools[0].clone())
        .run(move |c| {
            Box::pin(async move {
                lock_holding_binding_sources(c).await?;
                registry.verify_startup(c).await
            })
        })
        .await
        .map_err(|_| "PRIVATE_API_BINDINGS_MISMATCH")?;
    // One shared bounded password worker, never one independent limit per route.
    let passwords = Arc::new(
        Argon2Passwords::new(config.password_workers)
            .map_err(|_| "PRIVATE_API_PASSWORD_WORKER_UNAVAILABLE")?,
    );
    let writers = config
        .holdings
        .iter()
        .enumerate()
        .map(|(i, h)| {
            (
                h.registration.holding_id.clone(),
                AccountHoldingWriter {
                    write_binding: h.registration.write_binding.clone(),
                    pool: pools[i + 1].clone(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let invalidation = Arc::new(
        PostgresAccountWorkContextInvalidation::new(writers, clock.clone())
            .map_err(|_| "PRIVATE_API_BINDINGS_MISMATCH")?,
    );
    let auth = Arc::new(
        AccountAuthentication::new(
            (*config.policy).clone(),
            Arc::new(
                PostgresAccountRepository::from_control_database(
                    pools[0].clone(),
                    "development".into(),
                    config.policy.require_email_verification,
                    invalidation.clone(),
                    clock.clone(),
                    config.holdings_registry.clone(),
                )
                .map_err(|_| "PRIVATE_API_ACCOUNT_UNAVAILABLE")?,
            ),
            passwords,
            secrets.clone(),
            clock.clone(),
        )
        .map_err(|_| "PRIVATE_API_ACCOUNT_UNAVAILABLE")?,
    );
    let mut router = foundation_router();
    if config.account_bootstrap {
        let sso_policy = SsoPolicy::new((*config.policy).clone())
            .map_err(|_| "PRIVATE_API_ACCOUNT_UNAVAILABLE")?;
        let sso = Arc::new(AccountSso::new(
            sso_policy.clone(),
            Arc::new(PostgresAccountSso::from_control_database(
                pools[0].clone(),
                sso_policy,
                clock.clone(),
                config.holdings_registry.clone(),
            )),
            Arc::new(SystemSsoCryptography),
            secrets,
            clock.clone(),
        ));
        router = router
            .merge(account_auth_router(auth))
            .merge(account_sso_router(sso));
    } else {
        router = router.merge(account_session_router(auth));
    }
    let mut finance_targets = BTreeSet::new();
    let mut finance_post_targets = BTreeSet::new();
    let mut finance_reversal_targets = BTreeSet::new();
    if !config.finance_journal_reads.is_empty() {
        let catalogue = Arc::new(
            current_package_catalogue().map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
        );
        let mut targets = Vec::new();
        let mut post_targets = Vec::new();
        for (i, holding) in config.holdings.iter().enumerate() {
            let Some(company) = config
                .finance_journal_reads
                .get(&holding.registration.holding_id)
            else {
                continue;
            };
            let authorizer = Arc::new(
                HoldingWorkAuthorizer::new(
                    pools[0].clone(),
                    pools[i + 1].clone(),
                    holding.registration.holding_id.clone(),
                    (*config.policy).clone(),
                    clock.clone(),
                    config.holdings_registry.clone(),
                )
                .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
            );
            let configuration = FinanceJournalReadConfiguration::new(
                FinanceJournalEnvironment::Development,
                holding.registration.holding_id.clone(),
                company.clone(),
                catalogue.clone(),
            )
            .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
            finance_targets.insert((holding.host.clone(), holding.registration.slug.clone()));
            let post = config
                .finance_journal_posts
                .contains(&holding.registration.holding_id);
            let reverse = config
                .finance_journal_reversals
                .contains(&holding.registration.holding_id);
            if post || reverse {
                let action_config = FinanceJournalConfiguration::new_private_actions(
                    FinanceJournalEnvironment::Development,
                    holding.registration.holding_id.clone(),
                    company.clone(),
                    catalogue.clone(),
                    post,
                    reverse,
                )
                .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?;
                let target = (holding.host.clone(), holding.registration.slug.clone());
                if post {
                    finance_post_targets.insert(target.clone());
                }
                if reverse {
                    finance_reversal_targets.insert(target);
                }
                post_targets.push(BusinessHttpTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                    legal: None,
                    finance: Some(Arc::new(FinanceJournalReviewAdministration::new(
                        authorizer.clone(),
                        action_config,
                        Arc::new(SharedClock(clock.clone())),
                    ))),
                });
            }
            targets.push(FinanceJournalReadTarget {
                host: holding.host.clone(),
                holding_slug: holding.registration.slug.clone(),
                service: Arc::new(FinanceJournalDetailService::new(
                    authorizer,
                    configuration,
                    Arc::new(SharedClock(clock.clone())),
                )),
            });
        }
        if !post_targets.is_empty() {
            router = router.merge(
                business_router(
                    config.policy.clone(),
                    config.holdings_registry.clone(),
                    Arc::new(FinanceJournalReadScopeHeaders),
                    post_targets,
                )
                .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
            );
        }
        router = router.merge(
            finance_journal_read_router(
                config.policy.clone(),
                config.holdings_registry.clone(),
                Arc::new(FinanceJournalReadScopeHeaders),
                targets,
            )
            .map_err(|_| "PRIVATE_API_FINANCE_CONFIGURATION_INVALID")?,
        );
    }
    let state = Arc::new(Boundary {
        policy: config.policy.clone(),
        loopback: config.listen.to_string(),
        permits: Arc::new(Semaphore::new(config.maximum_requests)),
        timeout: config.request_timeout,
        finance_targets,
        finance_post_targets,
        finance_reversal_targets,
        account_bootstrap: config.account_bootstrap,
    });
    Ok(router.layer(middleware::from_fn_with_state(state, boundary)))
}
struct SharedClock(Arc<dyn Clock + Send + Sync>);
impl Clock for SharedClock {
    fn now(&self) -> TimestampMillis {
        self.0.now()
    }
}
struct Boundary {
    policy: Arc<AccountPolicy>,
    loopback: String,
    permits: Arc<Semaphore>,
    timeout: Duration,
    finance_targets: BTreeSet<(String, String)>,
    finance_post_targets: BTreeSet<(String, String)>,
    finance_reversal_targets: BTreeSet<(String, String)>,
    account_bootstrap: bool,
}
enum BoundaryDenial {
    InvalidAuthority,
    ProxyMetadata,
    NotFound,
}
impl BoundaryDenial {
    fn into_response(self) -> Response {
        match self {
            Self::InvalidAuthority => {
                rejected(StatusCode::BAD_REQUEST, "INVALID_REQUEST_AUTHORITY")
            }
            Self::ProxyMetadata => rejected(StatusCode::FORBIDDEN, "UNTRUSTED_PROXY_METADATA"),
            Self::NotFound => (StatusCode::NOT_FOUND, "Not Found").into_response(),
        }
    }
}
fn validate_request(state: &Boundary, request: &Request) -> Result<(), BoundaryDenial> {
    let mut values = request.headers().get_all(header::HOST).iter();
    let host = values
        .next()
        .and_then(|h| h.to_str().ok())
        .ok_or(BoundaryDenial::InvalidAuthority)?;
    if values.next().is_some()
        || request.uri().authority().is_some()
        || request.uri().scheme().is_some()
        || request.headers().get_all(header::ORIGIN).iter().count() > 1
    {
        return Err(BoundaryDenial::InvalidAuthority);
    }
    if [
        "forwarded",
        "x-forwarded-host",
        "x-forwarded-proto",
        "x-forwarded-port",
        "x-original-host",
    ]
    .iter()
    .any(|h| request.headers().contains_key(*h))
    {
        return Err(BoundaryDenial::ProxyMetadata);
    }
    if state.policy.audience(host).is_none()
        && !(host == state.loopback && matches!(request.uri().path(), "/api/health" | "/api/ready"))
    {
        return Err(BoundaryDenial::NotFound);
    }
    let finance_workspace = match request
        .uri()
        .path()
        .split('/')
        .collect::<Vec<_>>()
        .as_slice()
    {
        [
            "",
            "api",
            "v1",
            "holdings",
            slug,
            "finance",
            "journals",
            id,
            "workspace",
        ] if !id.is_empty() => state
            .finance_targets
            .contains(&(host.to_owned(), (*slug).to_owned())),
        _ => false,
    };
    let finance_post = request.method() == axum::http::Method::POST
        && match request
            .uri()
            .path()
            .split('/')
            .collect::<Vec<_>>()
            .as_slice()
        {
            [
                "",
                "api",
                "v1",
                "holdings",
                slug,
                "finance",
                "journals",
                id,
                "post",
            ] if !id.is_empty() => state
                .finance_post_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let finance_reverse = request.method() == axum::http::Method::POST
        && match request
            .uri()
            .path()
            .split('/')
            .collect::<Vec<_>>()
            .as_slice()
        {
            [
                "",
                "api",
                "v1",
                "holdings",
                slug,
                "finance",
                "journals",
                id,
                "reverse",
            ] if !id.is_empty() => state
                .finance_reversal_targets
                .contains(&(host.to_owned(), (*slug).to_owned())),
            _ => false,
        };
    let bootstrap = state.account_bootstrap
        && match (request.method(), request.uri().path()) {
            (&axum::http::Method::POST, "/api/auth/sign-in/email") => {
                host == state.policy.account_host
            }
            (&axum::http::Method::GET, "/sso/authorize") => host == state.policy.account_host,
            (&axum::http::Method::GET, "/sso/start" | "/sso/callback") => state
                .finance_targets
                .iter()
                .any(|(target_host, _)| target_host == host),
            _ => false,
        };
    if !(matches!(
        request.uri().path(),
        "/api/auth/get-session" | "/api/health" | "/api/ready"
    ) || finance_workspace
        || finance_post
        || finance_reverse
        || bootstrap)
        || !(bootstrap
            || finance_post
            || finance_reverse
            || matches!(
                *request.method(),
                axum::http::Method::GET | axum::http::Method::HEAD
            ))
    {
        return Err(BoundaryDenial::NotFound);
    }
    Ok(())
}
async fn boundary(State(state): State<Arc<Boundary>>, request: Request, next: Next) -> Response {
    if let Err(denial) = validate_request(&state, &request) {
        return private(denial.into_response());
    }
    let Ok(permit) = state.permits.clone().try_acquire_owned() else {
        return private(rejected(
            StatusCode::SERVICE_UNAVAILABLE,
            "PRIVATE_API_BUSY",
        ));
    };
    let result = tokio::time::timeout(state.timeout, next.run(request)).await;
    drop(permit);
    private(result.unwrap_or_else(|_| {
        rejected(
            StatusCode::SERVICE_UNAVAILABLE,
            "PRIVATE_API_REQUEST_TIMEOUT",
        )
    }))
}
fn rejected(status: StatusCode, code: &'static str) -> Response {
    (status,axum::Json(serde_json::json!({"error":{"code":code,"message":"The private API request is unavailable."}}))).into_response()
}
fn private(mut response: Response) -> Response {
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

/// Explicit CLI assembly only. This does not install or select the native API
/// for any gateway/Next process, and the mandatory configuration remains leased
/// disposable Development configuration even when this feature is compiled.
pub async fn run_development_api_file(
    path: &Path,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), &'static str> {
    let configuration = DevelopmentApiConfiguration::from_file(path)?;
    let runtime = assemble_development_api(
        configuration,
        Arc::new(SystemAccountClock),
        Arc::new(SystemAuthenticationSecrets),
    )
    .await?;
    let listener = match tokio::net::TcpListener::bind(runtime.listen_address()).await {
        Ok(listener) => listener,
        Err(_) => {
            runtime.close().await;
            return Err("PRIVATE_API_LISTEN_FAILED");
        }
    };
    eprintln!(
        "Private development session API listening on {}; readiness remains unavailable.",
        runtime.listen_address()
    );
    let result = axum::serve(listener, runtime.router())
        .with_graceful_shutdown(shutdown)
        .await;
    runtime.close().await;
    result.map_err(|_| "PRIVATE_API_HTTP_FAILED")
}
