//! One original Workflow receipt owns the complete parts command. This module
//! composes real typed sources; none of its data structs creates authority.
use super::{
    Error, FieldServicePartsSourcePreparation, changed, finance::*, inventory::*,
    notifications::BeforeNotificationSource, preparation::*, purchases::FieldServicePurchases,
    source::FieldServiceReferences,
};
use erpx_server_collaboration::public::field_service_notifications::{
    OwnedPreparedFieldServiceNotification, RetainedFieldServiceNotification,
    prepare_field_service_notification,
};
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_field_service::public::{parts::*, parts_source::*};
use erpx_server_finance::public::{
    inventory_consumption::CurrentConsumptionFinance,
    parts_draft::{OwnedPreparedPartsDraft, PartsDraftFinanceReceipt, prepare_owned_parts_draft},
};
use erpx_server_identity_access::public::{
    command_metadata::project_command_iam,
    effective_authority::{AuthorizedWorkContext, HoldingWorkSourcePreparation},
    work_context::WorkScopeKind,
};
use erpx_server_inventory::public::postgres_consumption::*;
use erpx_server_shared::public::{
    BusinessError, Clock, FailureKind, IdentifierSource, TimestampMillis,
};
use erpx_server_support::public::field_service::{
    PreparedSupportServicePartsProjection, SupportServicePartsAction, SupportServicePartsProjection,
};
use erpx_server_workflow::public::commands::*;
use sqlx::PgConnection;
use std::sync::Arc;

async fn load_order<'a>(
    c: &mut PgConnection,
    current: &'a AuthorizedWorkContext,
    id: &FieldServicePhysicalOrderId,
    now: TimestampMillis,
) -> Result<CurrentFieldServiceOrder<'a, FieldServiceReferences>, Error> {
    let base = read_field_service_order_base(c, current, id, now)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "FIELD_SERVICE_ORDER_NOT_FOUND",
                "Service order not found.",
            )
        })?;
    let references = FieldServiceReferences::retain(c, current, base.facts(), now).await?;
    base.with_references(c, references, now).await
}
struct Authority<'a, G> {
    current: &'a AuthorizedWorkContext,
    expected: &'a WorkflowRequest,
    mode: PackageEnforcementMode,
    guard: &'a G,
    purchases: Option<FieldServicePurchases>,
    http_source: bool,
}
impl<G: Clock + Sync> Authority<'_, G> {
    fn bind(&self, request: &WorkflowRequest) -> Result<CommandActor, BusinessError> {
        self.current.require_permission("records.read")?;
        self.current.require_permission("records.write")?;
        if !self.current.modules().iter().any(|m| m == "field_service")
            || self.guard.now().get() >= self.current.expires_at()
            || request.key != self.expected.key
            || request.operation != self.expected.operation
            || request.record_id != self.expected.record_id
            || request.input.as_str() != self.expected.input.as_str()
        {
            return Err(changed());
        }
        let selection = self.current.selection();
        let scope = selection.workspace();
        let assignment = selection.assignment();
        Ok(CommandActor {
            user_id: self.current.user_id().clone(),
            assignment_id: assignment.id.clone(),
            role: assignment.role.clone(),
            company_id: scope.company_id.clone(),
            branch_id: scope
                .branch_id
                .as_ref()
                .map(|id| CommandField::Value(id.as_str().into()))
                .unwrap_or(CommandField::Null),
            department_id: scope
                .department_id
                .as_ref()
                .map(|id| CommandField::Value(id.as_str().into()))
                .unwrap_or(CommandField::Null),
            authority: CommandField::Value(WorkAuthority {
                session_id: self.current.session_id().as_str().into(),
                revision: selection.revision(),
                assignment_version: assignment.version,
            }),
            iam: CommandField::Value(CommandJson::from_serializable(&project_command_iam(
                self.current,
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
}
impl<G: Clock + Sync> PgCommandAuthority for Authority<'_, G> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        FieldServicePartsSourcePreparation.prepare(c).await
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
        // Source existence/lineage precedes purchase, including History replay;
        // operational ownership/state is deliberately only in new preparation.
        let physical = FieldServicePhysicalOrderId::from_storage(r.record_id.clone());
        let _ = load_order(c, self.current, &physical, self.guard.now())
            .await
            .map_err(|error| {
                if self.http_source {
                    http_source_error(error)
                } else {
                    error
                }
            })?;
        match &self.purchases {
            Some(p) => p.require_current(c, self.current, self.guard.now()).await?,
            None => {
                self.purchases = Some(
                    FieldServicePurchases::retain(
                        c,
                        self.current,
                        self.mode,
                        disposition,
                        self.guard.now(),
                    )
                    .await?,
                )
            }
        }
        Ok(())
    }
}
enum PreparedBody {
    Reserve {
        reservation: PreparedFieldServiceReservation,
        stock: Box<PreparedFieldServiceStockReservation>,
    },
    Issue {
        original: Arc<CurrentFieldServiceReservation>,
        reservation: PreparedFieldServiceReservationConsumption,
        inventory: Box<PgReservedConsumptionEffects<CurrentConsumptionFinance>>,
        finance: Box<OwnedPreparedPartsDraft>,
    },
}
struct Prepared<'a> {
    source: CurrentFieldServiceOrder<'a, FieldServiceReferences>,
    work: PreparedFieldServiceOrderWrite,
    support: PreparedSupportServicePartsProjection,
    notification: OwnedPreparedFieldServiceNotification,
    purchases: FieldServicePurchases,
    body: PreparedBody,
}
struct WrittenReserve {
    prepared: PreparedFieldServiceReservation,
    inserted: RetainedNewFieldServiceReservation,
    stock: Box<PreparedFieldServiceStockReservation>,
    locked: Box<LockedFieldServiceStockReservation>,
}
struct WrittenIssue {
    original: Arc<CurrentFieldServiceReservation>,
    reservation: PreparedFieldServiceReservationConsumption,
    applied: RetainedFieldServiceReservationConsumption,
    inventory: Box<PgReservedConsumptionEffects<CurrentConsumptionFinance>>,
    finance: Box<PartsDraftFinanceReceipt>,
}
enum WrittenBody {
    Reserve(Box<WrittenReserve>),
    Issue(Box<WrittenIssue>),
}
struct Written<'a> {
    source: CurrentFieldServiceOrder<'a, FieldServiceReferences>,
    work: PreparedFieldServiceOrderWrite,
    order: RetainedFieldServiceOrderParts,
    support: PreparedSupportServicePartsProjection,
    notification: RetainedFieldServiceNotification,
    purchases: FieldServicePurchases,
    body: WrittenBody,
}
struct Effects<'a, W, G> {
    current: &'a AuthorizedWorkContext,
    command: &'a FieldServicePartsCommand,
    mode: PackageEnforcementMode,
    write: &'a W,
    guard: &'a G,
    prepared: Option<Prepared<'a>>,
    written: Option<Written<'a>>,
    quote: Option<PendingIssueQuote>,
    logical: Option<WorkflowRecordIdentity>,
    pending_at: Option<TimestampMillis>,
}

#[derive(serde::Serialize)]
struct FieldServiceResult<'a> {
    id: &'a str,
    version: i64,
    status: &'a str,
}
impl<W: Clock + Sync, G: Clock + Sync> PgOwnerCommandEffects for Effects<'_, W, G> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        FieldServicePartsSourcePreparation.prepare(c).await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let source = load_order(c, self.current, self.command.order_id(), self.guard.now()).await?;
        let application = prepare_new_field_service_parts(
            &mut PreparationPorts {
                c,
                write: self.write,
                guard: self.guard,
                scope: std::marker::PhantomData,
                quote: self.quote.take(),
                pending_at: self.pending_at,
            },
            &source,
            self.command,
        )
        .await?;
        let at = application.application_at();
        let details = field_service_parts_details_json(self.command, application.details())?;
        let purchases = FieldServicePurchases::retain(
            c,
            self.current,
            self.mode,
            CommandDisposition::Execute,
            self.guard.now(),
        )
        .await?;
        let (work, body) = match application.into_consumption() {
            None => {
                let intent = source
                    .retain_parts_intent(c, self.command, at, self.guard.now())
                    .await?;
                let reservation = intent
                    .prepare_new_reservation(c, &source, self.guard.now())
                    .await?;
                let owner = NewReservationSource::new(&reservation, &source, &intent, self.guard)?;
                let stock = prepare_field_service_stock_reservation(c, &owner).await?;
                let work = intent
                    .prepare_order_write(c, &source, details, self.guard.now())
                    .await?;
                (
                    work,
                    PreparedBody::Reserve {
                        reservation,
                        stock: Box::new(stock),
                    },
                )
            }
            Some(IssuePreparation {
                original,
                reservation,
                inventory,
                intent,
            }) => {
                let work = intent
                    .prepare_order_write(c, &source, details, self.guard.now())
                    .await?;
                let owner =
                    ReservationSource::new(&reservation, &original, self.command.key(), &source)?;
                let finance_source = PreparedPartsSource::new(
                    self.current,
                    &source,
                    &work,
                    &inventory,
                    &owner,
                    &purchases,
                )?;
                let finance = prepare_owned_parts_draft(c, &finance_source, self.write, self.guard)
                    .await?
                    .into_owned();
                (
                    work,
                    PreparedBody::Issue {
                        original,
                        reservation,
                        inventory: Box::new(inventory),
                        finance: Box::new(finance),
                    },
                )
            }
        };
        // The retained batch precompiler rejects unsafe numeric arguments after
        // stock/quote preparation and the Workflow clock, before SQL dispatch.
        if self.command.input().expected_version() >= 9_007_199_254_740_991 {
            let _ = self.write.now();
            return Err(
                erpx_server_runtime::public::postgres::PersistenceError::new(
                    "DATABASE_NUMBER_OUT_OF_RANGE",
                )
                .into(),
            );
        }
        let support = source
            .references()
            .support
            .prepare_parts_projection(
                c,
                SupportServicePartsProjection {
                    order_id: source.facts().id.as_str().to_owned(),
                    event_id: work.event_id().into(),
                    order_version_after: work.version_after(),
                    action: match self.command.input().action() {
                        FieldServicePartsActionKind::Reserve => SupportServicePartsAction::Reserve,
                        FieldServicePartsActionKind::Issue => SupportServicePartsAction::Issue,
                    },
                    actor_id: source.actor_user_id().clone(),
                    technician_id: work.technician_id().to_owned(),
                    scheduled_at: work.scheduled_at(),
                    application_at: at,
                    reason: self.command.input().reason().into(),
                },
            )
            .await?;
        let notification_source = BeforeNotificationSource::new(&source, &work)?;
        let notification = prepare_field_service_notification(
            c,
            &notification_source,
            self.command.key(),
            self.guard.now(),
        )
        .await?
        .into_owned();
        // Only successful late CAS can commit this candidate; the provisional
        // requested MAX_SAFE+1 stays in data so quote/stock failures remain first.
        let version = work.version_after();
        let result = if let Some(logical) = &self.logical {
            CommandJson::parse(&format!(
                "{{\"id\":{},\"version\":{},\"status\":{}}}",
                logical.logical_json()?.as_str(),
                version,
                serde_json::to_string(&source.facts().state).map_err(|_| changed())?
            ))?
        } else {
            CommandJson::from_serializable(&FieldServiceResult {
                id: self.command.order_id().as_str(),
                version,
                status: &source.facts().state,
            })?
        };
        self.prepared = Some(Prepared {
            source,
            work,
            support,
            notification,
            purchases,
            body,
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
            support,
            notification,
            purchases,
            body,
        } = self.prepared.take().ok_or_else(changed)?;
        let version = work.require_version(c, &source, self.guard.now()).await?;
        let (order, notification, body) = match body {
            PreparedBody::Reserve { reservation, stock } => {
                let owner =
                    NewReservationSource::new(&reservation, &source, work.intent(), self.guard)?;
                let locked = stock.lock_and_require_available(c, &owner).await?;
                let inserted = reservation
                    .insert(c, &source, work.intent(), &version, self.guard.now())
                    .await?;
                stock.require_after(c, &owner, &locked, &inserted).await?;
                support.require_before(c).await?;
                let order = work
                    .write_order_and_event(c, &source, &version, self.guard.now())
                    .await?;
                let before = BeforeNotificationSource::new(&source, &work)?;
                let notification = notification
                    .persist(c, &before.after(&order), self.guard.now())
                    .await?;
                (
                    order,
                    notification,
                    WrittenBody::Reserve(Box::new(WrittenReserve {
                        prepared: reservation,
                        inserted,
                        stock,
                        locked: Box::new(locked),
                    })),
                )
            }
            PreparedBody::Issue {
                original,
                reservation,
                inventory,
                finance,
            } => {
                let r =
                    ReservationSource::new(&reservation, &original, self.command.key(), &source)?;
                let before = PreparedPartsSource::new(
                    self.current,
                    &source,
                    &work,
                    &inventory,
                    &r,
                    &purchases,
                )?;
                inventory
                    .require_before(
                        c,
                        &mut FieldServiceFinanceFacts {
                            holding: Some(source.facts().holding_id.clone()),
                        },
                        &r,
                    )
                    .await?;
                inventory
                    .finance_source()
                    .require_posting_mapping(c)
                    .await?;
                let applied = reservation
                    .apply(c, &source, work.intent(), &version, self.guard.now())
                    .await?;
                let owner =
                    AppliedPartsSource::new(c, &before, &applied, &version, self.guard.now())
                        .await?;
                let finance = (*finance).persist_journal(c, &owner, self.guard).await?;
                inventory
                    .write_consumption_and_lots(
                        c,
                        &mut FieldServiceFinanceFacts {
                            holding: Some(source.facts().holding_id.clone()),
                        },
                        &r,
                        &applied,
                    )
                    .await?;
                let finance = finance.persist_stock_cost(c).await?;
                inventory.write_stock(c, &r, &applied).await?;
                support.require_before(c).await?;
                let order = work
                    .write_order_and_event(c, &source, &version, self.guard.now())
                    .await?;
                let notification_before = BeforeNotificationSource::new(&source, &work)?;
                let notification = notification
                    .persist(c, &notification_before.after(&order), self.guard.now())
                    .await?;
                owner.retain_order(c, order, self.guard.now()).await?;
                let finance = finance.into_finance_receipt(c, self.guard).await?;
                let order = owner.into_order()?;
                (
                    order,
                    notification,
                    WrittenBody::Issue(Box::new(WrittenIssue {
                        original,
                        reservation,
                        applied,
                        inventory,
                        finance: Box::new(finance),
                    })),
                )
            }
        };
        self.written = Some(Written {
            source,
            work,
            order,
            support,
            notification,
            purchases,
            body,
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
        let w = self.written.as_ref().ok_or_else(changed)?;
        w.order
            .require_plan(c, &w.source, &w.work, self.guard.now())
            .await?;
        match &w.body {
            WrittenBody::Reserve(payload) => {
                let WrittenReserve {
                    prepared,
                    inserted,
                    stock,
                    locked,
                } = payload.as_ref();
                let owner =
                    NewReservationSource::new(prepared, &w.source, w.work.intent(), self.guard)?;
                stock.require_after(c, &owner, locked, inserted).await?;
            }
            WrittenBody::Issue(payload) => {
                let WrittenIssue {
                    original,
                    reservation,
                    applied,
                    inventory,
                    finance,
                } = payload.as_ref();
                let owner =
                    ReservationSource::new(reservation, original, self.command.key(), &w.source)?;
                inventory
                    .require_inventory_after(c, &owner, applied)
                    .await?;
                finance.require_current(c).await?;
            }
        }
        w.support.require_after(c).await?;
        w.notification.require_current(c).await?;
        w.purchases
            .require_current(c, self.current, self.guard.now())
            .await
    }
}
/// Call ONLY within normal HoldingWorkAuthorizer.execute_with_sources with
/// FieldServicePartsSourcePreparation before its first snapshot. Retain actual
/// Control binding/IAM through commit and force workspace mutation/CAS=true.
pub(crate) async fn execute_field_service_parts<
    W: Clock + Sync,
    G: Clock + Sync,
    I: IdentifierSource,
>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    command: &FieldServicePartsCommand,
    mode: PackageEnforcementMode,
    write: &W,
    guard: &G,
    identifiers: &mut I,
) -> Result<CommandOutcome, Error> {
    let request = WorkflowRequest {
        key: command.key().as_str().into(),
        operation: command.operation().into(),
        record_id: command.order_id().as_str().into(),
        input: field_service_parts_receipt_json(command.receipt_input())?,
    };
    let mut authority = Authority {
        current,
        expected: &request,
        mode,
        guard,
        purchases: None,
        http_source: false,
    };
    let mut effects = Effects {
        current,
        command,
        mode,
        write,
        guard,
        prepared: None,
        written: None,
        quote: None,
        logical: None,
        pending_at: None,
    };
    PgCommandLedger::new(
        current.selection().workspace().company_id.clone(),
        c,
        &mut authority,
        write,
        identifiers,
    )
    .execute(&request, &mut effects)
    .await
}

// Replay-only phase cannot prepare or apply owner effects.
struct ReplayEffects;
impl PgOwnerCommandEffects for ReplayEffects {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        FieldServicePartsSourcePreparation.prepare(c).await
    }
    async fn prepare(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        Err(changed().into())
    }
    async fn apply(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Err(changed().into())
    }
    async fn retain(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Err(changed().into())
    }
}

pub(super) async fn execute_pending_field_service_parts<
    W: Clock + Sync,
    G: Clock + Sync,
    I: IdentifierSource,
>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    pending: PendingFieldServiceParts,
    mode: PackageEnforcementMode,
    write: &W,
    guard: &G,
    identifiers: &mut I,
) -> Result<CommandOutcome, Error> {
    let logical = WorkflowRecordIdentity::from_utf16(pending.target.original_utf16().to_vec());
    let request = WorkflowRequest {
        key: pending.key.clone(),
        operation: pending.normalized.input().action().operation().into(),
        record_id: logical.physical_text().to_owned(),
        input: field_service_parts_receipt_json(pending.normalized.receipt_input())?,
    };
    let source_request = SourceOrderedWorkflowRequest {
        key: request.key.clone(),
        operation: request.operation.clone(),
        record_id: logical.clone(),
        input: request.input.clone(),
    };
    let mut authority = Authority {
        current,
        expected: &request,
        mode,
        guard,
        purchases: None,
        http_source: true,
    };
    let actor = authority.bind(&request)?;
    // The retained source chooses History from the receipt's identifying columns,
    // before application fingerprint comparison. No fingerprint grants purpose.
    let candidate: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM command_receipts WHERE command_id=$1 AND operation=$2 AND record_id=$3 AND actor_id=$4 AND authority_json::jsonb->>'companyId'=$5)")
        .bind(&request.key).bind(&request.operation).bind(&request.record_id)
        .bind(current.user_id().as_str()).bind(current.selection().workspace().company_id.as_str())
        .fetch_one(&mut *c).await?;
    let purpose = if candidate {
        CommandDisposition::Replay
    } else {
        CommandDisposition::Execute
    };
    authority
        .verify_command(c, &actor, &request, purpose)
        .await?;
    let mut replay = ReplayEffects;
    if let Some(result) = PgCommandLedger::new(
        current.selection().workspace().company_id.clone(),
        c,
        &mut authority,
        write,
        identifiers,
    )
    .replay_source_ordered(&source_request, &mut replay)
    .await?
    {
        return Ok(result);
    }
    // A History preflight never grants settlement for a new command.
    if candidate {
        authority.purchases = None;
        authority
            .verify_command(c, &actor, &request, CommandDisposition::Execute)
            .await?;
    }
    let target = FieldServicePhysicalOrderId::from_storage(pending.target.storage_text());
    let source = load_order(c, current, &target, guard.now())
        .await
        .map_err(http_source_error)?;
    let (at, quote) = quote_pending_issue(c, &source, &pending, write, guard).await?;
    let key = erpx_server_shared::public::CommandKey::new(pending.key)?;
    let command = pending.normalized.into_physical_command(key, target);
    let mut effects = Effects {
        current,
        command: &command,
        mode,
        write,
        guard,
        prepared: None,
        written: None,
        quote,
        logical: Some(logical),
        pending_at: Some(at),
    };
    PgCommandLedger::new(
        current.selection().workspace().company_id.clone(),
        c,
        &mut authority,
        write,
        identifiers,
    )
    .execute_source_ordered(&source_request, &mut effects)
    .await
}

fn http_source_error(error: Error) -> Error {
    match error {
        erpx_server_runtime::public::postgres::PgTransactionError::Business(e)
            if e.code == "FIELD_SERVICE_ORDER_NOT_FOUND" =>
        {
            BusinessError::new(
                FailureKind::NotFound,
                "FIELD_SERVICE_ORDER_NOT_FOUND",
                "The original Service source is not available in this workspace.",
            )
            .into()
        }
        other => other,
    }
}
