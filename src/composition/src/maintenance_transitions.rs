//! Actual start/block/resume owner command composition, currently unmounted.
//! The original Account/Holding coordinator owns authorization and commit.
use erpx_server_assets::public::maintenance::{
    MaintenancePhysicalReceipt, PreparedMaintenancePhysicalChange,
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
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, HoldingWorkSourcePreparation,
};
use erpx_server_maintenance::public::{
    work_commands::MaintenanceWorkCommand,
    work_source::{
        CurrentMaintenanceWork, MaintenanceWorkSourcePreparation, PgMaintenanceTransitionPlan,
        PgMaintenanceTransitionReceipt, read_current_maintenance_work,
    },
    work_wire::MaintenanceChangeAction,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, FailureKind, IdentifierSource, TimestampMillis,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandJson, CommandOutcome, PgCommandAuthority,
    PgCommandLedger, PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MAINTENANCE_STATE_CHANGED",
        "Refresh the work order before continuing.",
    )
}
#[derive(Clone)]
pub struct MaintenanceTransitionsSourcePreparation;
impl HoldingWorkSourcePreparation for MaintenanceTransitionsSourcePreparation {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_maintenance_notification_sources(c).await?;
            MaintenanceWorkSourcePreparation.prepare(c).await
        })
    }
}
struct Authority<'a, G> {
    current: &'a AuthorizedWorkContext,
    command: &'a MaintenanceWorkCommand,
    mode: PackageEnforcementMode,
    clock: &'a G,
    catalogue: CommercialCatalogue,
    purchase: Option<PurchaseWitness>,
}
impl<G: Clock + Sync> Authority<'_, G> {
    fn bind(&self, r: &WorkflowRequest) -> Result<CommandActor, BusinessError> {
        self.current.require_permission("records.write")?;
        if !self.current.modules().iter().any(|m| m == "maintenance")
            || self.clock.now().get() >= self.current.expires_at()
            || !matches!(
                self.command.input().action,
                MaintenanceChangeAction::Start
                    | MaintenanceChangeAction::Block
                    | MaintenanceChangeAction::Resume
            )
            || r.key != self.command.key().as_str()
            || r.operation != self.command.operation()
            || r.record_id != self.command.work_id().as_str()
            || r.input.as_str() != self.command.receipt_input().as_str()
        {
            return Err(changed());
        }
        crate::maintenance_parts::command_actor(self.current)
    }
}
impl<G: Clock + Sync> PgCommandAuthority for Authority<'_, G> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
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
            purchases
                .revalidate(witness, now)
                .await?
                .require_allowed()?;
        } else {
            self.purchase = Some(
                purchases
                    .inspect(
                        &PurchaseRequest {
                            capabilities: vec!["assets.maintenance".into()],
                            purpose: match disposition {
                                CommandDisposition::Execute => CapabilityPurpose::SettleExisting,
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
        if read_current_maintenance_work(c, self.current, self.command.work_id(), now)
            .await?
            .is_none()
        {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "ASSETS_SOURCE_NOT_FOUND",
                "The actual Assets or Maintenance source was not found in this workspace.",
            )
            .into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
enum Image<'p> {
    Before(&'p PgMaintenanceTransitionPlan),
    After(
        &'p PgMaintenanceTransitionReceipt,
        Option<&'p MaintenancePhysicalReceipt>,
    ),
}
struct NotificationSource<'p, 'a> {
    source: &'p CurrentMaintenanceWork<'a>,
    image: Image<'p>,
    facts: MaintenanceNotificationFacts,
}
impl PgMaintenanceNotificationSource for NotificationSource<'_, '_> {
    fn facts(&self) -> &MaintenanceNotificationFacts {
        &self.facts
    }
    async fn require_before(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        match self.image {
            Image::Before(work) => work.require_current_source(c, self.source, now).await,
            _ => Err(changed().into()),
        }
    }
    async fn require_after(&self, c: &mut PgConnection, now: TimestampMillis) -> Result<(), Error> {
        match self.image {
            Image::After(work, physical) => work.require_after(c, self.source, physical, now).await,
            _ => Err(changed().into()),
        }
    }
}
struct Prepared<'a> {
    source: CurrentMaintenanceWork<'a>,
    work: PgMaintenanceTransitionPlan,
    physical: Option<PreparedMaintenancePhysicalChange>,
    notification: OwnedPreparedMaintenanceNotification,
}
struct Written<'a> {
    source: CurrentMaintenanceWork<'a>,
    work: PgMaintenanceTransitionReceipt,
    physical: Option<MaintenancePhysicalReceipt>,
    notification: RetainedMaintenanceNotification,
}
struct Effects<'a, W, G> {
    current: &'a AuthorizedWorkContext,
    command: &'a MaintenanceWorkCommand,
    write_clock: &'a W,
    guard_clock: &'a G,
    prepared: Option<Prepared<'a>>,
    written: Option<Written<'a>>,
}
#[derive(serde::Serialize)]
struct WorkResult<'a> {
    id: &'a str,
    version: i64,
    status: &'static str,
}
impl<W: Clock + Sync, G: Clock + Sync> PgOwnerCommandEffects for Effects<'_, W, G> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        MaintenanceTransitionsSourcePreparation.prepare(c).await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let source = read_current_maintenance_work(
            c,
            self.current,
            self.command.work_id(),
            self.guard_clock.now(),
        )
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "MAINTENANCE_WORK_NOT_FOUND",
                "Maintenance work order not found in this scope.",
            )
        })?;
        // Owner/state refusal precedes the domain clock, as in the application.
        let transition = match self.command.input().action {
            MaintenanceChangeAction::Start => {
                erpx_server_maintenance::public::MaintenanceTransition::Start
            }
            MaintenanceChangeAction::Block => {
                erpx_server_maintenance::public::MaintenanceTransition::Block
            }
            MaintenanceChangeAction::Resume => {
                erpx_server_maintenance::public::MaintenanceTransition::Resume
            }
            _ => return Err(changed().into()),
        };
        source.require_action(&transition)?;
        let at = self.write_clock.now();
        let work = source
            .retain_simple_transition(c, self.command, at, self.guard_clock.now())
            .await?;
        let physical = match work.physical_change() {
            Some(change) => Some(
                source
                    .physical_asset_source()
                    .prepare_physical_change(c, change.clone())
                    .await?,
            ),
            None => None,
        };
        let notification_source = NotificationSource {
            source: &source,
            image: Image::Before(&work),
            facts: MaintenanceNotificationFacts {
                holding_id: source.holding_id().clone(),
                company_id: source.facts().company_id.clone(),
                work_id: source.facts().id.clone(),
                owner_user_id: source.facts().owner_id.clone(),
                work_version_after: work.version_after(),
                updated_at_after: work.application_at(),
                owner_key: self.command.key().clone(),
                kind: MaintenanceNotificationKind::Updated,
            },
        };
        let notification = prepare_maintenance_notification(
            c,
            &notification_source,
            self.command.key(),
            MaintenanceNotificationKind::Updated,
            self.guard_clock.now(),
        )
        .await?
        .into_owned();
        let version = source.facts().version.checked_add(1).ok_or_else(changed)?;
        let result = CommandJson::from_serializable(&WorkResult {
            id: self.command.work_id().as_str(),
            version,
            status: work.status_after(),
        })?;
        self.prepared = Some(Prepared {
            source,
            work,
            physical,
            notification,
        });
        Ok(WorkflowOutput {
            version,
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
        let Prepared {
            source,
            work,
            physical,
            notification,
        } = self.prepared.take().ok_or_else(changed)?;
        work.write_work(c, &source, self.guard_clock.now()).await?;
        let physical = match physical {
            Some(plan) => Some(plan.apply(c, source.physical_asset_source()).await?),
            None => None,
        };
        let work = work
            .finish(c, &source, physical.as_ref(), self.guard_clock.now())
            .await?;
        let facts = notification.notification().facts().clone();
        let actual = NotificationSource {
            source: &source,
            image: Image::After(&work, physical.as_ref()),
            facts,
        };
        let notification = notification
            .persist(c, &actual, self.guard_clock.now())
            .await?;
        self.written = Some(Written {
            source,
            work,
            physical,
            notification,
        });
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *c)
            .await?;
        let written = self.written.as_ref().ok_or_else(changed)?;
        written
            .work
            .require_after(
                c,
                &written.source,
                written.physical.as_ref(),
                self.guard_clock.now(),
            )
            .await?;
        written.notification.require_current(c).await
    }
}
/// Caller MUST use this exact utility preparation before its first Holding
/// snapshot, retain original Account/Control through commit, and supply trusted
/// package mode. No Actor DTO, new transaction or runtime activation is added.
pub async fn execute_maintenance_transition<
    W: Clock + Sync,
    G: Clock + Sync,
    I: IdentifierSource,
>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    command: &MaintenanceWorkCommand,
    mode: PackageEnforcementMode,
    write_clock: &W,
    guard_clock: &G,
    identifiers: &mut I,
) -> Result<CommandOutcome, Error> {
    let request = WorkflowRequest {
        key: command.key().as_str().into(),
        operation: command.operation(),
        record_id: command.work_id().as_str().into(),
        input: CommandJson::parse(command.receipt_input().as_str())?,
    };
    let mut authority = Authority {
        current,
        command,
        mode,
        clock: guard_clock,
        catalogue: current_package_catalogue()?,
        purchase: None,
    };
    let mut effects = Effects {
        current,
        command,
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
