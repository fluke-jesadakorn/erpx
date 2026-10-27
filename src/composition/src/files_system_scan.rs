//! Unmounted Files/system assembly. Trusted reviewed binding-key configuration
//! selects physical leased development databases; request labels/pools do not.
//! No Person, saved parent or created_by permission is invented for scanning.
use erpx_server_files::public::attachment_scan::{
    AttachmentScanEvidence, AttachmentScanFailure, AttachmentScanScope, AttachmentScanSettlement,
    AttachmentScanTransactions, PreparedAttachmentScan, claim_attachment_scan,
    fail_current_attachment_scan, prepare_current_attachment_scan, retain_current_attachment_scan,
};
use erpx_server_identity_access::public::{
    RequestAudience,
    account_holding_bindings::{ConfiguredHoldingRegistry, lock_holding_binding_sources},
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, LeasedDevelopmentDatabase, MigrationTrack, PgRequestLease, PgTransactionError,
    PgUnitOfWork, PgWorkFuture, PoolLimits, TransactionOptions, load_migrations,
    verify_current_migration_inventory,
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, IdentifierSource, RecordId};
use sqlx::PgConnection;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
    time::Duration,
};
type Error = PgTransactionError<BusinessError>;
type DatabaseIdentityRow = (String, i64, String, String, String, String, Option<String>);

/// Explicit trusted operator configuration, not an HTTP/Actor DTO. The same
/// registry is used at startup and in every short phase. No ambient .env load,
/// database creation, migration, pool+label adoption or production fallback.
pub struct DevelopmentFilesScanConfiguration {
    control: LeasedDevelopmentDatabase,
    holding: LeasedDevelopmentDatabase,
    registry: Arc<ConfiguredHoldingRegistry>,
    scope: AttachmentScanScope,
    audience: RequestAudience,
    write_binding: String,
    files_binding: String,
}
impl DevelopmentFilesScanConfiguration {
    pub fn new(
        control: LeasedDevelopmentDatabase,
        mut reviewed_write_databases: BTreeMap<String, LeasedDevelopmentDatabase>,
        registry: Arc<ConfiguredHoldingRegistry>,
        scope: AttachmentScanScope,
    ) -> Result<Self, BusinessError> {
        let expected = registry
            .configured()
            .map(|h| h.write_binding.clone())
            .collect::<BTreeSet<_>>();
        if reviewed_write_databases
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>()
            != expected
        {
            return Err(unavailable());
        }
        let audience = registry.audience_for_holding(&scope.holding_id)?;
        let registration = registry.expected_for_audience(&audience)?;
        let write_binding = registration.write_binding.clone();
        let files_binding = registration.files_binding.clone();
        let holding = reviewed_write_databases
            .remove(&write_binding)
            .ok_or_else(unavailable)?;
        Ok(Self {
            control,
            holding,
            registry,
            scope,
            audience,
            write_binding,
            files_binding,
        })
    }
}

#[derive(Clone, Eq, PartialEq)]
struct DatabaseIdentity {
    name: String,
    oid: i64,
    postmaster: String,
    role: String,
    session_role: String,
    owner: String,
    lease: String,
}
impl DatabaseIdentity {
    async fn read(c: &mut PgConnection) -> Result<Self, Error> {
        let (name,oid,postmaster,role,session_role,owner,lease):DatabaseIdentityRow=sqlx::query_as("SELECT current_database(),d.oid::bigint,pg_postmaster_start_time()::text,current_user::text,session_user::text,pg_get_userbyid(d.datdba)::text,CASE WHEN octet_length(shobj_description(d.oid,'pg_database'))<=128 THEN shobj_description(d.oid,'pg_database') END FROM pg_catalog.pg_database d WHERE d.datname=current_database()").fetch_one(c).await?;
        let lease = lease.ok_or_else(unavailable)?;
        let token = name
            .strip_prefix("erpx_dev_rust_")
            .filter(|token| {
                token.len() == 32
                    && token
                        .bytes()
                        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            })
            .ok_or_else(unavailable)?;
        if role != "erpx_test_admin"
            || session_role != "erpx_test_admin"
            || owner != "erpx_test_admin"
            || lease != format!("erpx-rust-test:{token}")
        {
            return Err(unavailable().into());
        }
        Ok(Self {
            name,
            oid,
            postmaster,
            role,
            session_role,
            owner,
            lease,
        })
    }
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if &Self::read(c).await? != self {
            return Err(unavailable().into());
        }
        Ok(())
    }
}

struct State<C> {
    control_pool: DatabasePool,
    holding_pool: DatabasePool,
    control: PgRequestLease,
    holding: PgUnitOfWork,
    control_identity: DatabaseIdentity,
    holding_identity: DatabaseIdentity,
    registry: Arc<ConfiguredHoldingRegistry>,
    scope: AttachmentScanScope,
    audience: RequestAudience,
    write_binding: String,
    files_binding: String,
    control_schema: String,
    holding_schema: String,
    clock: Arc<C>,
}
/// Only actual leased/schema/registry assembly constructs this value. It owns
/// no current authority between phases and exposes no raw pool or credential.
pub struct DevelopmentFilesScanTransactions<C> {
    state: Arc<State<C>>,
}
impl<C> Clone for DevelopmentFilesScanTransactions<C> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}
impl<C: Clock + Send + Sync + 'static> DevelopmentFilesScanTransactions<C> {
    pub fn files_binding(&self) -> &str {
        &self.state.files_binding
    }
    pub fn scope(&self) -> &AttachmentScanScope {
        &self.state.scope
    }
    /// Startup observations only; no returned digest is a source/read grant.
    pub fn schema_observations(&self) -> (&str, &str) {
        (&self.state.control_schema, &self.state.holding_schema)
    }
    pub async fn close(self) {
        self.state.holding_pool.close().await;
        self.state.control_pool.close().await;
    }
    async fn phase<T, F>(&self, action: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: for<'c> Fn(&'c mut PgConnection) -> PgWorkFuture<'c, T, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let state = self.state.clone();
        self.state
            .control
            .run(move |control| {
                Box::pin(async move {
                    lock_holding_binding_sources(control).await?;
                    state.control_identity.require_current(control).await?;
                    let binding = state
                        .registry
                        .read_current(control, &state.audience)
                        .await?;
                    if binding.holding_id() != &state.scope.holding_id
                        || binding.write_binding() != state.write_binding
                    {
                        return Err(unavailable().into());
                    }
                    let identity = state.holding_identity.clone();
                    let result = state
                        .holding
                        .run(TransactionOptions::default(), move |connection| {
                            let identity = identity.clone();
                            let action = action.clone();
                            Box::pin(async move {
                                identity.require_current(connection).await?;
                                let result = action(connection).await?;
                                identity.require_current(connection).await?;
                                Ok(result)
                            })
                        })
                        .await?;
                    // The outer Control callback NEVER replays after a Holding commit.
                    // Its original binding SHARE locks remain held through this point.
                    binding.require_current(control).await?;
                    Ok(result)
                })
            })
            .await
    }
}

pub async fn assemble_development_files_scan<C: Clock + Send + Sync + 'static>(
    configuration: DevelopmentFilesScanConfiguration,
    clock: Arc<C>,
) -> Result<DevelopmentFilesScanTransactions<C>, Error> {
    let limits = PoolLimits {
        max_connections: 2,
        acquire_timeout: Duration::from_secs(5),
    };
    let control = configuration.control.connect(limits).await?;
    let holding = match configuration.holding.connect(limits).await {
        Ok(pool) => pool,
        Err(error) => {
            control.close().await;
            return Err(error.into());
        }
    };
    let result = async {
        let mut control_connection = control.raw().acquire().await?;
        let mut holding_connection = holding.raw().acquire().await?;
        let control_identity = DatabaseIdentity::read(&mut control_connection).await?;
        let holding_identity = DatabaseIdentity::read(&mut holding_connection).await?;
        if control_identity.name == holding_identity.name
            || control_identity.oid == holding_identity.oid
        {
            return Err(unavailable().into());
        }
        drop(control_connection);
        drop(holding_connection);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let control_files = load_migrations(&root, MigrationTrack::Control).await?;
        let holding_files = load_migrations(&root, MigrationTrack::HoldingWrite).await?;
        let control_schema =
            verify_current_migration_inventory(&control, MigrationTrack::Control, &control_files)
                .await?;
        let holding_schema = verify_current_migration_inventory(
            &holding,
            MigrationTrack::HoldingWrite,
            &holding_files,
        )
        .await?;
        let registry = configuration.registry.clone();
        PgRequestLease::new(control.clone())
            .run(move |c| {
                Box::pin(async move {
                    lock_holding_binding_sources(c).await?;
                    registry.verify_startup(c).await
                })
            })
            .await?;
        Ok::<_, Error>((
            control_identity,
            holding_identity,
            control_schema.sha256().to_owned(),
            holding_schema.sha256().to_owned(),
        ))
    }
    .await;
    match result {
        Ok((control_identity, holding_identity, control_schema, holding_schema)) => {
            Ok(DevelopmentFilesScanTransactions {
                state: Arc::new(State {
                    control: PgRequestLease::new(control.clone()),
                    holding: PgUnitOfWork::new(holding.clone()),
                    control_pool: control,
                    holding_pool: holding,
                    control_identity,
                    holding_identity,
                    registry: configuration.registry,
                    scope: configuration.scope,
                    audience: configuration.audience,
                    write_binding: configuration.write_binding,
                    files_binding: configuration.files_binding,
                    control_schema,
                    holding_schema,
                    clock,
                }),
            })
        }
        Err(error) => {
            holding.close().await;
            control.close().await;
            Err(error)
        }
    }
}

impl<C: Clock + Send + Sync + 'static> AttachmentScanTransactions
    for DevelopmentFilesScanTransactions<C>
{
    type Error = Error;
    async fn claim_and_prepare(
        &self,
        source_filter: Option<&str>,
    ) -> Result<Option<PreparedAttachmentScan>, Error> {
        let filter = source_filter.map(RecordId::new).transpose()?;
        let scope = self.state.scope.clone();
        let clock = self.state.clock.clone();
        self.phase(move |c| {
            let filter = filter.clone();
            let scope = scope.clone();
            let clock = clock.clone();
            Box::pin(async move {
                let Some(claim) = claim_attachment_scan(
                    c,
                    &scope,
                    filter.as_ref(),
                    clock.as_ref(),
                    &mut Identifiers,
                )
                .await?
                else {
                    return Ok(None);
                };
                prepare_current_attachment_scan(c, claim, clock.as_ref())
                    .await
                    .map(Some)
            })
        })
        .await
    }
    async fn retain_current(
        &self,
        prepared: PreparedAttachmentScan,
        evidence: AttachmentScanEvidence,
    ) -> Result<AttachmentScanSettlement, Error> {
        if prepared.claim().scope() != &self.state.scope {
            return Err(unavailable().into());
        }
        let prepared = Arc::new(prepared);
        let evidence = Arc::new(evidence);
        let clock = self.state.clock.clone();
        self.phase(move |c| {
            let prepared = prepared.clone();
            let evidence = evidence.clone();
            let clock = clock.clone();
            Box::pin(async move {
                let receipt = retain_current_attachment_scan(
                    c,
                    &prepared,
                    &evidence,
                    clock.as_ref(),
                    &mut Identifiers,
                )
                .await?;
                sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
                    .execute(&mut *c)
                    .await?;
                receipt.require_current(c, clock.as_ref()).await?;
                Ok(receipt.outcome())
            })
        })
        .await
    }
    async fn fail_current(
        &self,
        prepared: PreparedAttachmentScan,
        failure: AttachmentScanFailure,
    ) -> Result<AttachmentScanSettlement, Error> {
        if prepared.claim().scope() != &self.state.scope {
            return Err(unavailable().into());
        }
        let prepared = Arc::new(prepared);
        let failure = Arc::new(failure);
        let clock = self.state.clock.clone();
        self.phase(move |c| {
            let prepared = prepared.clone();
            let failure = failure.clone();
            let clock = clock.clone();
            Box::pin(async move {
                let receipt = fail_current_attachment_scan(
                    c,
                    &prepared,
                    &failure,
                    clock.as_ref(),
                    &mut Identifiers,
                )
                .await?;
                sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
                    .execute(&mut *c)
                    .await?;
                receipt.require_current(c, clock.as_ref()).await?;
                Ok(receipt.outcome())
            })
        })
        .await
    }
}
struct Identifiers;
impl IdentifierSource for Identifiers {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILES_CURRENT_SYSTEM_BINDING_REQUIRED",
        "The configured current Files system binding is unavailable.",
    )
}
