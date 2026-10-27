//! Actual original owner command assembly. No HTTP route or standalone commit.
mod source;
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, HoldingWorkSourcePreparation,
};
use erpx_server_maintenance::public::{
    MaintenanceTransition,
    work_commands::MaintenanceWorkCommand,
    work_source::{
        CurrentMaintenanceWork, PgMaintenanceIssuePartPlan, PgMaintenanceIssuePartReceipt,
        read_current_maintenance_work,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind, TimestampMillis};
pub use source::{
    MaintenancePartsSourcePreparation, PgMaintenancePartsSource, prepare_issue_part_inventory,
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
use erpx_server_collaboration::public::maintenance_notifications::{
    MaintenanceNotificationFacts, MaintenanceNotificationKind,
    OwnedPreparedMaintenanceNotification, PgMaintenanceNotificationSource,
    RetainedMaintenanceNotification, prepare_maintenance_notification,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue, lock_commercial_authority_sources,
};
use erpx_server_finance::public::inventory_consumption::CurrentConsumptionFinance;
use erpx_server_finance::public::parts_draft::{
    OwnedPreparedPartsDraft, PartsDraftFinanceReceipt, prepare_owned_parts_draft,
};
use erpx_server_identity_access::public::{
    command_metadata::project_command_iam, work_context::WorkScopeKind,
};
use erpx_server_inventory::public::postgres_consumption::PgUnreservedConsumptionEffects;
use erpx_server_shared::public::{Clock, IdentifierSource};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandField, CommandJson, CommandOutcome,
    PgCommandAuthority, PgCommandLedger, PgOwnerCommandEffects, WorkAuthority, WorkflowOutput,
    WorkflowRequest,
};

struct Authority<'a, G> {
    current: &'a AuthorizedWorkContext,
    command: &'a MaintenanceWorkCommand,
    mode: PackageEnforcementMode,
    guard_clock: &'a G,
    catalogue: CommercialCatalogue,
    purchase: Option<PurchaseWitness>,
}
impl<G: Clock + Sync> Authority<'_, G> {
    fn bind(&self, request: &WorkflowRequest) -> Result<CommandActor, BusinessError> {
        self.current.require_permission("records.write")?;
        if !self.current.modules().iter().any(|m| m == "maintenance")
            || self.guard_clock.now().get() >= self.current.expires_at()
            || request.key != self.command.key().as_str()
            || request.operation != "maintenance.issue-part"
            || request.operation != self.command.operation()
            || request.record_id != self.command.work_id().as_str()
            || request.input.as_str() != self.command.receipt_input().as_str()
        {
            return Err(changed());
        }
        command_actor(self.current)
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
        request: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        Ok(self.bind(request)?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        request: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        if &self.bind(request)? != actor {
            return Err(changed().into());
        }
        let now = self.guard_clock.now();
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
                            capabilities: vec!["assets.parts-costs".into()],
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
        // The original purchase wrapper requires a real scoped Work/Asset even
        // for History replay; owner/state/version action policy is later.
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
fn command_actor(current: &AuthorizedWorkContext) -> Result<CommandActor, BusinessError> {
    let selection = current.selection();
    let workspace = selection.workspace();
    let assignment = selection.assignment();
    Ok(CommandActor {
        user_id: current.user_id().clone(),
        assignment_id: assignment.id.clone(),
        role: assignment.role.clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace
            .branch_id
            .as_ref()
            .map(|id| CommandField::Value(id.as_str().into()))
            .unwrap_or(CommandField::Null),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|id| CommandField::Value(id.as_str().into()))
            .unwrap_or(CommandField::Null),
        authority: CommandField::Value(WorkAuthority {
            session_id: current.session_id().as_str().into(),
            revision: selection.revision(),
            assignment_version: assignment.version,
        }),
        iam: CommandField::Value(CommandJson::from_serializable(&project_command_iam(
            current,
        )?)?),
        assignment_scope_kind: match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        assignment_scope_node_id: assignment
            .scope_node_id
            .as_ref()
            .map(|id| id.as_str().into()),
    })
}
struct NotificationSource<'p, 'a> {
    source: &'p CurrentMaintenanceWork<'a>,
    work: &'p PgMaintenanceIssuePartPlan,
    facts: MaintenanceNotificationFacts,
}
impl<'p, 'a> NotificationSource<'p, 'a> {
    fn new(source: &'p CurrentMaintenanceWork<'a>, work: &'p PgMaintenanceIssuePartPlan) -> Self {
        Self {
            source,
            work,
            facts: MaintenanceNotificationFacts {
                holding_id: source.holding_id().clone(),
                company_id: source.facts().company_id.clone(),
                work_id: source.facts().id.clone(),
                owner_user_id: source.facts().owner_id.clone(),
                work_version_after: work.version_after(),
                updated_at_after: work.application_at(),
                owner_key: work.command().key().clone(),
                kind: MaintenanceNotificationKind::Updated,
            },
        }
    }
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
        self.work.require_current_source(c, self.source, now).await
    }
    async fn require_after(&self, c: &mut PgConnection, now: TimestampMillis) -> Result<(), Error> {
        self.work.require_after(c, self.source, now).await
    }
}
struct Prepared<'a> {
    source: CurrentMaintenanceWork<'a>,
    work: PgMaintenanceIssuePartPlan,
    inventory: PgUnreservedConsumptionEffects<CurrentConsumptionFinance>,
    finance: OwnedPreparedPartsDraft,
    notification: OwnedPreparedMaintenanceNotification,
}
struct Written<'a> {
    source: CurrentMaintenanceWork<'a>,
    work: PgMaintenanceIssuePartReceipt,
    inventory: PgUnreservedConsumptionEffects<CurrentConsumptionFinance>,
    finance: PartsDraftFinanceReceipt,
    notification: RetainedMaintenanceNotification,
}
struct Effects<'a, W, G> {
    current: &'a AuthorizedWorkContext,
    command: &'a MaintenanceWorkCommand,
    mode: PackageEnforcementMode,
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
        MaintenancePartsSourcePreparation.prepare(c).await
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
        source.require_action(&MaintenanceTransition::IssuePart)?;
        let at = self.write_clock.now();
        let inventory = prepare_issue_part_inventory(
            c,
            &source,
            self.command,
            self.write_clock,
            self.guard_clock,
        )
        .await?;
        let work = source
            .retain_issue_part_effects(c, self.command, at, self.guard_clock.now())
            .await?
            .into_plan();
        let owner = PgMaintenancePartsSource::retain(
            c,
            &source,
            &work,
            &inventory,
            self.mode,
            self.guard_clock.now(),
        )
        .await?;
        let finance = prepare_owned_parts_draft(c, &owner, self.write_clock, self.guard_clock)
            .await?
            .into_owned();
        let notification_source = NotificationSource::new(&source, &work);
        let notification = prepare_maintenance_notification(
            c,
            &notification_source,
            self.command.key(),
            MaintenanceNotificationKind::Updated,
            self.guard_clock.now(),
        )
        .await?
        .into_owned();
        // A stale MAX_SAFE request may carry a provisional MAX_SAFE+1 into
        // readonly preparation. Its late Work CAS must remain the refusal;
        // only the actual current successor can ever become a committed result.
        let version = source.facts().version.checked_add(1).ok_or_else(changed)?;
        let result = CommandJson::from_serializable(&WorkResult {
            id: self.command.work_id().as_str(),
            version,
            status: "in_progress",
        })?;
        self.prepared = Some(Prepared {
            source,
            work,
            inventory,
            finance,
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
            inventory,
            finance,
            notification,
        } = self.prepared.take().ok_or_else(changed)?;
        let owner = PgMaintenancePartsSource::retain(
            c,
            &source,
            &work,
            &inventory,
            self.mode,
            self.guard_clock.now(),
        )
        .await?;
        let finance = finance.persist_journal(c, &owner, self.guard_clock).await?;
        inventory.write_consumption_and_lots(c).await?;
        let finance = finance.persist_stock_cost(c).await?;
        inventory.write_stock(c).await?;
        work.write(c, &source, self.guard_clock.now()).await?;
        let notification_source = NotificationSource::new(&source, &work);
        let notification = notification
            .persist(c, &notification_source, self.guard_clock.now())
            .await?;
        let finance = finance.into_finance_receipt(c, self.guard_clock).await?;
        let work = work
            .into_retention(c, &source, self.guard_clock.now())
            .await?;
        self.written = Some(Written {
            source,
            work,
            inventory,
            finance,
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
            .require_after(c, &written.source, self.guard_clock.now())
            .await?;
        written.inventory.require_inventory_after(c).await?;
        written.finance.require_current(c).await?;
        written.notification.require_current(c).await
    }
}
/// The calling native Account/Holding coordinator MUST use the exact source
/// preparation before its first Holding snapshot and retain its Control lease
/// through commit. HTTP/bootstrap never accepts a caller-supplied Actor/mode.
pub async fn execute_maintenance_issue_part<
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
        guard_clock,
        catalogue: current_package_catalogue()?,
        purchase: None,
    };
    let mut effects = Effects {
        current,
        command,
        mode,
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
