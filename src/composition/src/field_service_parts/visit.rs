//! Visit transitions composed from real owner, People, IAM, Support and Collaboration witnesses.
use super::request::{actor, database_now};
use super::{
    Error, FieldServicePartsSourcePreparation, changed, command::load_order,
    purchases::FieldServicePurchases, source::FieldServiceReferences,
};
use erpx_server_collaboration::public::field_service_notifications::{
    FieldServiceNotificationFacts, OwnedPreparedFieldServiceNotification,
    PgFieldServiceNotificationSource, RetainedFieldServiceNotification,
    prepare_field_service_notification,
};
use erpx_server_field_service::public::{
    parts::{
        FieldServicePhysicalOrderId, FieldServiceRawOrderTarget, field_service_first_wire_message,
    },
    parts_source::CurrentFieldServiceOrder,
    visit::*,
};
use erpx_server_identity_access::public::{
    effective_authority::{AuthorizedWorkContext, HoldingWorkSourcePreparation},
    field_service_assignment::{
        CurrentFieldServiceAssignment, FieldServiceAssignmentPermission,
        FieldServiceAssignmentRequest, inspect_field_service_assignment,
    },
};
use erpx_server_people_payroll::public::field_service_worker::{
    CurrentFieldServicePeopleReference, inspect_field_service_people_reference,
};
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, IdentifierSource, RecordId, TimestampMillis,
};
use erpx_server_support::public::field_service::{
    PreparedSupportServicePartsProjection, SupportServicePartsAction, SupportServicePartsProjection,
};
use erpx_server_workflow::public::commands::*;
use sqlx::PgConnection;
use std::sync::Arc;

struct Technician {
    id: String,
    assignment: CurrentFieldServiceAssignment,
    person: CurrentFieldServicePeopleReference,
}
impl PgVisitTechnician for Technician {
    fn technician_id(&self) -> &str {
        &self.id
    }
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let now = database_now(c).await?;
        self.assignment.require_current(c, now).await?;
        self.person.require_current(c).await?;
        if !self.assignment.is_eligible() || !self.person.is_eligible() {
            return Err(changed().into());
        }
        Ok(())
    }
}
async fn technician(
    c: &mut PgConnection,
    source: &CurrentFieldServiceOrder<'_, FieldServiceReferences>,
    id: &str,
) -> Result<Technician, Error> {
    let at = database_now(c).await?;
    let assignment = inspect_field_service_assignment(
        c,
        FieldServiceAssignmentRequest {
            company_id: source.facts().company_id.clone(),
            user_id: id.into(),
            branch_id: source.facts().branch_id.clone(),
            department_id: source.facts().department_id.clone(),
            permission: FieldServiceAssignmentPermission::Write,
        },
        at,
    )
    .await?;
    let person = inspect_field_service_people_reference(c, &source.facts().company_id, id).await?;
    Ok(Technician {
        id: id.into(),
        assignment,
        person,
    })
}
struct Notify<'a, 'b> {
    source: &'a CurrentFieldServiceOrder<'b, FieldServiceReferences>,
    plan: &'a PreparedFieldServiceVisit,
    tech: &'a Technician,
    facts: FieldServiceNotificationFacts,
}
impl PgFieldServiceNotificationSource for Notify<'_, '_> {
    fn facts(&self) -> &FieldServiceNotificationFacts {
        &self.facts
    }
    async fn require_before(&self, c: &mut PgConnection, n: TimestampMillis) -> Result<(), Error> {
        self.plan.require_before(c, self.source, self.tech, n).await
    }
    async fn require_after(&self, c: &mut PgConnection, n: TimestampMillis) -> Result<(), Error> {
        self.plan.require_after(c, self.source, self.tech, n).await
    }
}
fn notify<'a, 'b>(
    source: &'a CurrentFieldServiceOrder<'b, FieldServiceReferences>,
    plan: &'a PreparedFieldServiceVisit,
    tech: &'a Technician,
) -> Notify<'a, 'b> {
    Notify {
        source,
        plan,
        tech,
        facts: FieldServiceNotificationFacts {
            owner_key: plan.key().clone(),
            holding_id: source.facts().holding_id.clone(),
            company_id: source.facts().company_id.clone(),
            order_id: source.facts().id.as_str().into(),
            technician_user_id: plan.technician().into(),
            order_version_after: plan.version(),
            updated_at_after: TimestampMillis::new(plan.at()),
        },
    }
}
struct Authority<'a, C> {
    current: &'a AuthorizedWorkContext,
    expected: &'a WorkflowRequest,
    clock: &'a C,
    purchase: Option<FieldServicePurchases>,
}
impl<C: Clock + Sync> PgCommandAuthority for Authority<'_, C> {
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
        if r.key != self.expected.key
            || r.operation != self.expected.operation
            || r.record_id != self.expected.record_id
            || r.input.as_str() != self.expected.input.as_str()
        {
            return Err(changed().into());
        }
        Ok(actor(self.current, self.clock.now())?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        a: &CommandActor,
        r: &WorkflowRequest,
        d: CommandDisposition,
    ) -> Result<(), Error> {
        if &actor(self.current, self.clock.now())? != a {
            return Err(changed().into());
        }
        let source = load_order(
            c,
            self.current,
            &FieldServicePhysicalOrderId::from_storage(r.record_id.clone()),
            self.clock.now(),
        )
        .await?;
        if let Some(p) = &self.purchase {
            p.require_current(c, self.current, self.clock.now()).await?;
        } else {
            self.purchase = Some(
                FieldServicePurchases::retain_visit(
                    c,
                    self.current,
                    source
                        .facts()
                        .asset_id
                        .as_ref()
                        .is_some_and(|s| !s.is_empty()),
                    d,
                    self.clock.now(),
                )
                .await?,
            );
        }
        Ok(())
    }
}
struct Prepared<'a> {
    source: CurrentFieldServiceOrder<'a, FieldServiceReferences>,
    tech: Technician,
    plan: PreparedFieldServiceVisit,
    support: PreparedSupportServicePartsProjection,
    notice: OwnedPreparedFieldServiceNotification,
    purchase: FieldServicePurchases,
}
struct Written<'a> {
    source: CurrentFieldServiceOrder<'a, FieldServiceReferences>,
    tech: Technician,
    plan: PreparedFieldServiceVisit,
    support: PreparedSupportServicePartsProjection,
    purchase: FieldServicePurchases,
    notice: RetainedFieldServiceNotification,
}
struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    input: &'a NormalizedFieldServiceVisit,
    key: &'a CommandKey,
    target: &'a FieldServicePhysicalOrderId,
    logical: &'a WorkflowRecordIdentity,
    clock: &'a C,
    prepared: Option<Prepared<'a>>,
    written: Option<Written<'a>>,
}
impl<'a, C: Clock + Sync> PgOwnerCommandEffects for Effects<'a, C> {
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
        let source = load_order(c, self.current, self.target, self.clock.now()).await?;
        source
            .require_visit_action(c, self.input.action, self.clock.now())
            .await?;
        let tech = technician(
            c,
            &source,
            self.input
                .technician_id
                .as_deref()
                .unwrap_or(&source.facts().technician_id),
        )
        .await?;
        let at = self.clock.now();
        let plan = PreparedFieldServiceVisit::prepare(
            c,
            &source,
            self.key.clone(),
            &tech,
            self.input.clone(),
            at,
            self.clock.now(),
        )
        .await?;
        if self.input.expected_version >= 9_007_199_254_740_991 {
            return Err(
                erpx_server_runtime::public::postgres::PersistenceError::new(
                    "DATABASE_NUMBER_OUT_OF_RANGE",
                )
                .into(),
            );
        }
        let purchase = FieldServicePurchases::retain_visit(
            c,
            self.current,
            source
                .facts()
                .asset_id
                .as_ref()
                .is_some_and(|s| !s.is_empty()),
            CommandDisposition::Execute,
            self.clock.now(),
        )
        .await?;
        let support = source
            .references()
            .support
            .prepare_parts_projection(
                c,
                SupportServicePartsProjection {
                    order_id: source.facts().id.as_str().into(),
                    event_id: format!("service-order-event-{}", self.key.as_str()),
                    order_version_after: plan.version(),
                    action: match self.input.action {
                        VisitAction::Schedule => SupportServicePartsAction::Schedule,
                        VisitAction::Dispatch => SupportServicePartsAction::Dispatch,
                        VisitAction::Start => SupportServicePartsAction::Start,
                    },
                    actor_id: source.actor_user_id().clone(),
                    technician_id: plan.technician().into(),
                    scheduled_at: plan.scheduled_at().map(TimestampMillis::new),
                    application_at: at,
                    reason: self.input.reason.clone(),
                },
            )
            .await?;
        // Ensure the receipt, immutable event and initial current projection fit
        // the same bounded readback before admitting any durable effects.
        let evidence = crate::field_service_reservation_read::FieldServiceVisitReadResponse {
            context: crate::field_service_reservation_read::context(self.current)?,
            order: erpx_server_field_service::public::reservation_read::FieldServiceReserveResult {
                id: self.target.as_str().into(),
                status: plan.status().into(),
                version: plan.version(),
            },
            receipt:
                erpx_server_field_service::public::reservation_read::FieldServiceReserveResult {
                    id: self.target.as_str().into(),
                    status: plan.status().into(),
                    version: plan.version(),
                },
            event: erpx_server_field_service::public::reservation_read::FieldServiceVisitEvent {
                version: plan.version(),
                action: self
                    .input
                    .action
                    .operation()
                    .strip_prefix("field.order.")
                    .ok_or_else(changed)?
                    .into(),
                actor_id: source.actor_user_id().as_str().into(),
                reason: self.input.reason.clone(),
                status: plan.status().into(),
                technician_id: plan.technician().into(),
                scheduled_at: plan.scheduled_at(),
            },
        };
        let mut budget = crate::field_service_reservation_read::ReplyBudget(0);
        serde_json::to_writer(&mut budget, &evidence).map_err(|_| {
            BusinessError::new(
                erpx_server_shared::public::FailureKind::Unavailable,
                "FIELD_SERVICE_READ_LIMIT",
                "The Service source exceeds this bounded read.",
            )
        })?;
        let notice = prepare_field_service_notification(
            c,
            &notify(&source, &plan, &tech),
            self.key,
            self.clock.now(),
        )
        .await?
        .into_owned();
        let result = CommandJson::parse(&format!(
            "{{\"id\":{},\"version\":{},\"status\":{}}}",
            self.logical.logical_json()?.as_str(),
            plan.version(),
            serde_json::to_string(plan.status()).map_err(|_| changed())?
        ))?;
        let version = plan.version();
        self.prepared = Some(Prepared {
            source,
            tech,
            plan,
            support,
            notice,
            purchase,
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
        let p = self.prepared.take().ok_or_else(changed)?;
        p.support.require_before(c).await?;
        p.plan
            .apply(c, &p.source, &p.tech, self.clock.now())
            .await?;
        let notification_source = notify(&p.source, &p.plan, &p.tech);
        let notice = p
            .notice
            .persist(c, &notification_source, self.clock.now())
            .await?;
        self.written = Some(Written {
            source: p.source,
            tech: p.tech,
            plan: p.plan,
            support: p.support,
            purchase: p.purchase,
            notice,
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
        w.plan
            .require_after(c, &w.source, &w.tech, self.clock.now())
            .await?;
        w.support.require_after(c).await?;
        w.notice.require_current(c).await?;
        w.purchase
            .require_current(c, self.current, self.clock.now())
            .await
    }
}
struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}
struct Replay;
impl PgOwnerCommandEffects for Replay {
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
pub(crate) struct VisitDispatcher<C> {
    pub clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> super::OtherPost for VisitDispatcher<C> {
    fn validate(&self, input: &super::OtherPostInput) -> Result<(), String> {
        if input.branch != crate::field_service_http_wire::OtherPostBranch::OrderChange {
            return Err("This action is not available.".into());
        }
        normalize_field_service_visit(&input.body)
            .map(|_| ())
            .map_err(|e| {
                field_service_first_wire_message(&input.body, &e)
                    .unwrap_or_else(|| "Invalid request.".into())
            })
    }
    fn admission_error(
        &self,
        _: crate::field_service_http_wire::OtherPostBranch,
        e: BusinessError,
    ) -> BusinessError {
        e
    }
    fn execute<'a>(
        &'a self,
        c: &'a mut PgConnection,
        current: &'a AuthorizedWorkContext,
        input: super::OtherPostInput,
    ) -> PgWorkFuture<'a, CommandJson, BusinessError> {
        Box::pin(async move {
            let normalized = normalize_field_service_visit(&input.body).map_err(|_| {
                BusinessError::invalid("FIELD_SERVICE_VISIT_INVALID", "Invalid visit action.")
            })?;
            let raw = FieldServiceRawOrderTarget::from_utf16(input.id_utf16.ok_or_else(changed)?);
            let logical = WorkflowRecordIdentity::from_utf16(raw.original_utf16().to_vec());
            let target = FieldServicePhysicalOrderId::from_storage(raw.storage_text());
            let request = WorkflowRequest {
                key: input.key.clone(),
                operation: normalized.action.operation().into(),
                record_id: logical.physical_text().into(),
                input: normalized.receipt_input.clone(),
            };
            let ordered = SourceOrderedWorkflowRequest {
                key: request.key.clone(),
                operation: request.operation.clone(),
                record_id: logical.clone(),
                input: request.input.clone(),
            };
            let mut authority = Authority {
                current,
                expected: &request,
                clock: self.clock.as_ref(),
                purchase: None,
            };
            let actor = actor(current, self.clock.now())?;
            let candidate:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM command_receipts WHERE command_id=$1 AND operation=$2 AND record_id=$3 AND actor_id=$4 AND authority_json::jsonb->>'companyId'=$5)").bind(&request.key).bind(&request.operation).bind(&request.record_id).bind(current.user_id().as_str()).bind(current.selection().workspace().company_id.as_str()).fetch_one(&mut *c).await?;
            authority
                .verify_command(
                    c,
                    &actor,
                    &request,
                    if candidate {
                        CommandDisposition::Replay
                    } else {
                        CommandDisposition::Execute
                    },
                )
                .await?;
            let mut ids = Ids;
            let mut replay = Replay;
            if let Some(outcome) = PgCommandLedger::new(
                current.selection().workspace().company_id.clone(),
                c,
                &mut authority,
                self.clock.as_ref(),
                &mut ids,
            )
            .replay_source_ordered(&ordered, &mut replay)
            .await?
            {
                return Ok(outcome.result);
            }
            if candidate {
                authority.purchase = None;
                authority
                    .verify_command(c, &actor, &request, CommandDisposition::Execute)
                    .await?;
            }
            let key = CommandKey::new(input.key)?;
            let mut effects = Effects {
                current,
                input: &normalized,
                key: &key,
                target: &target,
                logical: &logical,
                clock: self.clock.as_ref(),
                prepared: None,
                written: None,
            };
            Ok(PgCommandLedger::new(
                current.selection().workspace().company_id.clone(),
                c,
                &mut authority,
                self.clock.as_ref(),
                &mut ids,
            )
            .execute_source_ordered(&ordered, &mut effects)
            .await?
            .result)
        })
    }
}
