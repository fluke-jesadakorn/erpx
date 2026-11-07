//! Native Maintenance work-order creation on the coordinator transaction.
use erpx_server_assets::public::maintenance::{
    inspect_maintenance_creation_asset, lock_maintenance_creation_asset_sources,
};
use erpx_server_collaboration::public::maintenance_notifications::{
    MaintenanceNotificationFacts, MaintenanceNotificationKind,
    OwnedPreparedMaintenanceNotification, PgMaintenanceNotificationSource,
    RetainedMaintenanceNotification, lock_maintenance_notification_sources,
    prepare_maintenance_notification,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue, lock_commercial_authority_sources,
};
use erpx_server_identity_access::public::{
    effective_authority::{AuthorizedWorkContext, HoldingWorkSourcePreparation},
    operational_worker::{
        OperationalWorkerPermission, OperationalWorkerRequest, lock_operational_worker_sources,
        inspect_operational_worker,
    },
};
use erpx_server_maintenance::public::{
    work_commands::NormalizedMaintenanceCreate,
    work_source::{
        MaintenanceCreationPlan, MaintenanceWorkSourcePreparation, insert_maintenance_creation,
        maintenance_creation_vacancy,
    },
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, FailureKind, HoldingId, IdentifierSource, RecordId,
    TimestampMillis,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandJson, PgCommandAuthority, PgCommandLedger,
    PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MAINTENANCE_STATE_CHANGED",
        "Refresh the asset before creating work.",
    )
}

#[derive(Clone)]
pub struct MaintenanceCreationPreparation;
impl HoldingWorkSourcePreparation for MaintenanceCreationPreparation {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_maintenance_notification_sources(c).await?;
            lock_operational_worker_sources(c).await?;
            lock_maintenance_creation_asset_sources(c).await?;
            MaintenanceWorkSourcePreparation.prepare(c).await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
struct CreateFacts {
    holding: HoldingId,
    company: erpx_server_shared::public::CompanyId,
    work_id: RecordId,
    key: CommandKey,
    branch_id: RecordId,
    department_id: Option<RecordId>,
    asset_id: RecordId,
    asset_version: i64,
    title: String,
    priority: String,
    status: &'static str,
    owner_id: String,
    scheduled_at: Option<i64>,
    reason: String,
    actor_id: String,
    now: TimestampMillis,
}
impl PgMaintenanceNotificationSource for CreateFacts {
    fn facts(&self) -> &MaintenanceNotificationFacts {
        // The notification facts are retained separately by the prepared value;
        // this source only proves the owner images.
        unreachable!("create notification facts are supplied by CreateNotificationSource")
    }
    async fn require_before(&self, c: &mut PgConnection, _: TimestampMillis) -> Result<(), Error> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM maintenance_work_orders WHERE id=$1)",
        )
        .bind(self.work_id.as_str())
        .fetch_one(&mut *c)
        .await?;
        if exists {
            return Err(changed().into());
        }
        Ok(())
    }
    async fn require_after(&self, c: &mut PgConnection, _: TimestampMillis) -> Result<(), Error> {
        let row: Option<(String, i64, String, String, i64, String)> = sqlx::query_as(
            "SELECT w.company_id,w.version,w.status,w.owner_id,w.version,a.company_id \
             FROM maintenance_work_orders w JOIN operational_assets a ON a.id=w.asset_id \
             WHERE w.id=$1 AND w.asset_id=$2 AND a.company_id=$3 AND a.version=$4",
        )
        .bind(self.work_id.as_str())
        .bind(self.asset_id.as_str())
        .bind(self.company.as_str())
        .bind(self.asset_version)
        .fetch_optional(&mut *c)
        .await?;
        let event: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM maintenance_work_events WHERE work_order_id=$1 AND version=1 AND action='create')",
        )
        .bind(self.work_id.as_str())
        .fetch_one(c)
        .await?;
        if row.as_ref().is_none_or(|(company, version, status, owner, _, _)| {
            company != self.company.as_str()
                || *version != 1
                || status != self.status
                || owner != &self.owner_id
        }) || !event
        {
            return Err(changed().into());
        }
        Ok(())
    }
}

struct CreateNotificationSource {
    facts: MaintenanceNotificationFacts,
    image: CreateFacts,
}
impl PgMaintenanceNotificationSource for CreateNotificationSource {
    fn facts(&self) -> &MaintenanceNotificationFacts {
        &self.facts
    }
    async fn require_before(&self, c: &mut PgConnection, now: TimestampMillis) -> Result<(), Error> {
        self.image.require_before(c, now).await
    }
    async fn require_after(&self, c: &mut PgConnection, now: TimestampMillis) -> Result<(), Error> {
        self.image.require_after(c, now).await
    }
}

struct Authority<'a, G> {
    current: &'a AuthorizedWorkContext,
    key: &'a CommandKey,
    input: &'a NormalizedMaintenanceCreate,
    mode: PackageEnforcementMode,
    clock: &'a G,
    catalogue: CommercialCatalogue,
    purchase: Option<PurchaseWitness>,
}
impl<G: Clock + Sync> Authority<'_, G> {
    fn record_id(&self) -> Result<RecordId, BusinessError> {
        RecordId::new(format!("maintenance-{}", self.key.as_str()))
    }
    fn bind(&self, r: &WorkflowRequest) -> Result<CommandActor, BusinessError> {
        self.current.require_permission("records.write")?;
        let record_id = self.record_id()?;
        if !self.current.modules().iter().any(|m| m == "maintenance")
            || self.clock.now().get() >= self.current.expires_at()
            || r.key != self.key.as_str()
            || r.operation != "maintenance.create"
            || r.record_id != record_id.as_str()
            || r.input.as_str() != self.input.receipt_input().as_str()
        {
            return Err(changed());
        }
        crate::maintenance_parts::command_actor(self.current)
    }
}
impl<G: Clock + Sync> PgCommandAuthority for Authority<'_, G> {
    async fn lock_sources(&mut self, c: &mut PgConnection, _: &WorkflowRequest) -> Result<(), Error> {
        lock_commercial_authority_sources(c).await
    }
    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        r: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        Ok(self.bind(r)?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        r: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        if &self.bind(r)? != actor {
            return Err(changed().into());
        }
        let now = self.clock.now();
        let mut purchases = PgCompanyPurchaseReader::new(
            self.current.holding_id().clone(),
            actor.company_id.clone(),
            c,
            &self.catalogue,
        );
        if let Some(witness) = &self.purchase {
            purchases.revalidate(witness, now).await?.require_allowed()?;
        } else {
            self.purchase = Some(
                purchases
                    .inspect(
                        &PurchaseRequest {
                            capabilities: vec!["assets.maintenance".into()],
                            purpose: match disposition {
                                CommandDisposition::Execute => CapabilityPurpose::NewWork,
                                CommandDisposition::Replay => CapabilityPurpose::History,
                            },
                            mode: self.mode,
                        },
                        now,
                    )
                    .await?
                    .require_allowed()?
                    .clone(),
            );
        }
        Ok(())
    }
}

struct Prepare {
    source: CreateFacts,
    facts: MaintenanceNotificationFacts,
    notification: OwnedPreparedMaintenanceNotification,
}
struct Written {
    source: CreateFacts,
    notification: RetainedMaintenanceNotification,
}
struct Effects<'a, W, G> {
    current: &'a AuthorizedWorkContext,
    key: &'a CommandKey,
    input: &'a NormalizedMaintenanceCreate,
    write_clock: &'a W,
    guard_clock: &'a G,
    prepared: Option<Prepare>,
    written: Option<Written>,
}

#[derive(serde::Serialize)]
struct CreateResult<'a> {
    id: &'a str,
    version: i64,
    status: &'a str,
}

impl<W: Clock + Sync, G: Clock + Sync> PgOwnerCommandEffects for Effects<'_, W, G> {
    async fn lock_sources(&mut self, c: &mut PgConnection, _: &WorkflowRequest) -> Result<(), Error> {
        MaintenanceCreationPreparation.prepare(c).await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let workspace = self.current.selection().workspace();
        let input = self.input.input();
        let asset_id = RecordId::new(input.asset_id.clone())?;
        let asset = inspect_maintenance_creation_asset(
            c,
            self.current.holding_id(),
            &workspace.company_id,
            workspace.branch_id.as_ref(),
            workspace.department_id.as_ref(),
            &asset_id,
            input.asset_version,
        )
        .await?
        .ok_or_else(changed)?;
        if !maintenance_creation_vacancy(c, &asset_id).await? {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "MAINTENANCE_ACTIVE_WORK",
                "This asset already has active maintenance work.",
            )
            .into());
        }
        let owner = inspect_operational_worker(
            c,
            OperationalWorkerRequest {
                company_id: workspace.company_id.clone(),
                user_id: erpx_server_shared::public::UserId::new(input.owner_id.clone())?,
                branch_id: asset.facts().branch_id.clone(),
                department_id: asset.facts().department_id.clone(),
                permission: OperationalWorkerPermission::Write,
            },
            self.guard_clock.now(),
        )
        .await?;
        if !owner.is_eligible() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MAINTENANCE_OWNER_INVALID",
                "Choose an active worker in this branch.",
            )
            .into());
        }
        let status = if input.scheduled_at.is_some() { "scheduled" } else { "open" };
        let key = self.key.clone();
        let work_id = RecordId::new(format!("maintenance-{}", key.as_str()))?;
        let source = CreateFacts {
            holding: self.current.holding_id().clone(),
            company: workspace.company_id.clone(),
            work_id: work_id.clone(),
            key: key.clone(),
            branch_id: asset.facts().branch_id.clone(),
            department_id: asset.facts().department_id.clone(),
            asset_id,
            asset_version: input.asset_version,
            title: input.title.clone(),
            priority: input.priority.clone(),
            status,
            owner_id: input.owner_id.clone(),
            scheduled_at: input.scheduled_at,
            reason: input.reason.clone(),
            actor_id: self.current.user_id().as_str().to_owned(),
            now: self.write_clock.now(),
        };
        let facts = MaintenanceNotificationFacts {
            owner_key: key.clone(),
            kind: MaintenanceNotificationKind::Created,
            holding_id: self.current.holding_id().clone(),
            company_id: workspace.company_id.clone(),
            work_id: work_id.clone(),
            owner_user_id: input.owner_id.clone(),
            work_version_after: 1,
            updated_at_after: source.now,
        };
        let image = source.clone();
        let notification_source = CreateNotificationSource {
            facts: facts.clone(),
            image: source.clone(),
        };
        let notification = prepare_maintenance_notification(
            c,
            &notification_source,
            &key,
            MaintenanceNotificationKind::Created,
            self.guard_clock.now(),
        )
        .await?
        .into_owned();
        let result = CommandJson::from_serializable(&CreateResult {
            id: work_id.as_str(),
            version: 1,
            status,
        })?;
        self.prepared = Some(Prepare { source, facts, notification });
        Ok(WorkflowOutput {
            version: 1,
            result,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let Prepare { source, facts, notification } = self.prepared.take().ok_or_else(changed)?;
        insert_maintenance_creation(
            c,
            &MaintenanceCreationPlan {
                id: &source.work_id,
                key: source.key.as_str(),
                company_id: source.company.as_str(),
                branch_id: &source.branch_id,
                department_id: source.department_id.as_ref(),
                asset_id: &source.asset_id,
                asset_version: source.asset_version,
                title: &source.title,
                priority: &source.priority,
                status: source.status,
                owner_id: &source.owner_id,
                scheduled_at: source.scheduled_at,
                reason: &source.reason,
                actor_id: &source.actor_id,
                now: source.now.get(),
            },
        )
        .await?;
        let actual = CreateNotificationSource { facts, image: source.clone() };
        let notification = notification
            .persist(c, &actual, self.guard_clock.now())
            .await?;
        self.written = Some(Written { source, notification });
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *c).await?;
        let written = self.written.as_ref().ok_or_else(changed)?;
        written.source.require_after(c, self.guard_clock.now()).await?;
        written.notification.require_current(c).await
    }
}

pub async fn execute_maintenance_create<W: Clock + Sync, G: Clock + Sync, I: IdentifierSource>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    key: &CommandKey,
    input: &NormalizedMaintenanceCreate,
    mode: PackageEnforcementMode,
    write_clock: &W,
    guard_clock: &G,
    identifiers: &mut I,
) -> Result<erpx_server_workflow::public::commands::CommandOutcome, Error> {
    let record_id = RecordId::new(format!("maintenance-{}", key.as_str()))?;
    let request = WorkflowRequest {
        key: key.as_str().into(),
        operation: "maintenance.create".into(),
        record_id: record_id.as_str().into(),
        input: CommandJson::parse(input.receipt_input().as_str())?,
    };
    let mut authority = Authority {
        current,
        key,
        input,
        mode,
        clock: guard_clock,
        catalogue: current_package_catalogue()?,
        purchase: None,
    };
    let mut effects = Effects {
        current,
        key,
        input,
        write_clock,
        guard_clock,
        prepared: None,
        written: None,
    };
    PgCommandLedger::new(
        current.selection().workspace().company_id.clone(),
        c,
        &mut authority,
        write_clock,
        identifiers,
    )
    .execute(&request, &mut effects)
    .await
}
