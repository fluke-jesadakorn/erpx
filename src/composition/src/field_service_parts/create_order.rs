//! Existing Support ticket -> Service order, within the original Holding unit.
use super::{
    Error, FieldServicePartsSourcePreparation, changed,
    request::{Purchases, actor, database_now},
};
use erpx_server_assets::public::field_service_reference::{
    CurrentFieldServicePhysicalAsset, read_current_field_service_physical_asset,
};
use erpx_server_collaboration::public::field_service_notifications::{
    FieldServiceNotificationFacts, OwnedPreparedFieldServiceNotification,
    PgFieldServiceNotificationSource, RetainedFieldServiceNotification,
    prepare_field_service_created_notification,
};
use erpx_server_field_service::public::{
    parts::{NormalizedServiceOrderCreate, normalize_service_order_create},
    request_agreement::{CurrentServiceRequestAgreement, read_service_request_agreement},
    request_order::{
        PgServiceRequestOrderSource, PreparedServiceRequestOrder, RetainedServiceRequestOrder,
        ServiceRequestOrderFacts, prepare_service_request_order, read_service_request_order,
    },
};
use erpx_server_finance::public::field_service_customer::{
    CurrentFieldServiceCustomer, read_current_field_service_customer,
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
    BusinessError, Clock, CommandKey, FailureKind, IdentifierSource, RecordId, TimestampMillis,
};
use erpx_server_support::public::field_service_existing_ticket::{
    CurrentExistingServiceTicket, read_existing_service_ticket,
};
use erpx_server_workflow::public::commands::*;
use sqlx::PgConnection;
use std::sync::Arc;
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "FIELD_SERVICE_ORDER_SOURCE_NOT_FOUND",
        "The original Service source is not available in this workspace.",
    )
}
struct Sources<'a> {
    current: &'a AuthorizedWorkContext,
    agreement: CurrentServiceRequestAgreement,
    ticket: CurrentExistingServiceTicket,
    customer: CurrentFieldServiceCustomer,
    asset: Option<CurrentFieldServicePhysicalAsset>,
    technician: CurrentFieldServiceAssignment,
    person: CurrentFieldServicePeopleReference,
    order: ServiceRequestOrderFacts,
    notification: FieldServiceNotificationFacts,
}
impl<'a> Sources<'a> {
    async fn new(
        c: &mut PgConnection,
        current: &'a AuthorizedWorkContext,
        input: &NormalizedServiceOrderCreate,
        key: &CommandKey,
        at: TimestampMillis,
        guard: TimestampMillis,
    ) -> Result<Self, Error> {
        actor(current, guard)?;
        let agreement = read_service_request_agreement(c, current, &input.agreement_id)
            .await?
            .ok_or_else(unavailable)?;
        let a = agreement.facts();
        if a.version != input.agreement_version {
            return Err(changed().into());
        }
        agreement.require_new_work(c).await?;
        let ticket = read_existing_service_ticket(c, &a.company_id, &input.ticket_id)
            .await?
            .ok_or_else(unavailable)?;
        ticket.require_new_work(input.ticket_version)?;
        let t = ticket.facts();
        if t.branch_id != a.branch_id
            || t.department_id != a.department_id
            || t.customer_id != a.customer_id
            || a.asset_id
                .as_ref()
                .is_some_and(|v| Some(v.as_str()) != input.asset_id.as_deref())
        {
            return Err(changed().into());
        }
        let customer = read_current_field_service_customer(c, &a.company_id, &a.customer_id)
            .await?
            .ok_or_else(unavailable)?;
        let asset = if let Some(id) = input.asset_id.as_ref().filter(|v| !v.is_empty()) {
            let asset = read_current_field_service_physical_asset(
                c,
                current.holding_id(),
                &a.company_id,
                id,
            )
            .await?
            .ok_or_else(unavailable)?;
            let f = asset.facts();
            if f.branch_id != a.branch_id
                || matches!(
                    f.lifecycle_status.as_str(),
                    "retired" | "retirement_pending"
                )
                || current
                    .selection()
                    .workspace()
                    .department_id
                    .as_ref()
                    .is_some_and(|d| f.department_id.as_ref() != Some(d))
            {
                return Err(unavailable().into());
            }
            Some(asset)
        } else {
            None
        };
        let db_now = database_now(c).await?;
        let technician = inspect_field_service_assignment(
            c,
            FieldServiceAssignmentRequest {
                company_id: a.company_id.clone(),
                user_id: input.technician_id.clone(),
                branch_id: a.branch_id.clone(),
                department_id: a.department_id.clone(),
                permission: FieldServiceAssignmentPermission::Write,
            },
            db_now,
        )
        .await?;
        let person =
            inspect_field_service_people_reference(c, &a.company_id, &input.technician_id).await?;
        if !technician.is_eligible() || !person.is_eligible() {
            return Err(changed().into());
        }
        let order = ServiceRequestOrderFacts {
            key: key.as_str().into(),
            company_id: a.company_id.clone(),
            branch_id: a.branch_id.as_str().into(),
            department_id: a.department_id.as_ref().map(|v| v.as_str().into()),
            ticket_id: input.ticket_id.clone(),
            ticket_version: input.ticket_version,
            agreement_id: a.id.clone(),
            agreement_version: a.version,
            asset_id: input.asset_id.clone(),
            issue: input.issue.clone(),
            site: input.site.clone(),
            technician_id: input.technician_id.clone(),
            scheduled_at: input.scheduled_at,
            actor_id: current.user_id().as_str().into(),
            at: at.get(),
        };
        let notification = FieldServiceNotificationFacts {
            owner_key: key.clone(),
            holding_id: current.holding_id().clone(),
            company_id: a.company_id.clone(),
            order_id: order.order_id(),
            technician_user_id: input.technician_id.clone(),
            order_version_after: 1,
            updated_at_after: at,
        };
        Ok(Self {
            current,
            agreement,
            ticket,
            customer,
            asset,
            technician,
            person,
            order,
            notification,
        })
    }
    async fn check(&self, c: &mut PgConnection, now: TimestampMillis) -> Result<(), Error> {
        actor(self.current, now)?;
        self.agreement.require_current(c, self.current).await?;
        self.agreement.require_new_work(c).await?;
        self.ticket.require_current(c).await?;
        self.ticket.require_new_work(self.order.ticket_version)?;
        self.customer.require_current(c).await?;
        if let Some(a) = &self.asset {
            a.require_current(c).await?;
        }
        let db_now = database_now(c).await?;
        self.technician.require_current(c, db_now).await?;
        self.person.require_current(c).await?;
        if !self.technician.is_eligible() || !self.person.is_eligible() {
            return Err(changed().into());
        }
        Ok(())
    }
}
impl PgServiceRequestOrderSource for Sources<'_> {
    fn facts(&self) -> &ServiceRequestOrderFacts {
        &self.order
    }
    async fn require_before(&self, c: &mut PgConnection, n: TimestampMillis) -> Result<(), Error> {
        self.check(c, n).await
    }
    async fn require_after(&self, c: &mut PgConnection, n: TimestampMillis) -> Result<(), Error> {
        self.check(c, n).await
    }
}
impl PgFieldServiceNotificationSource for Sources<'_> {
    fn facts(&self) -> &FieldServiceNotificationFacts {
        &self.notification
    }
    async fn require_before(&self, c: &mut PgConnection, n: TimestampMillis) -> Result<(), Error> {
        self.check(c, n).await
    }
    async fn require_after(&self, _: &mut PgConnection, _: TimestampMillis) -> Result<(), Error> {
        Err(changed().into())
    }
}
struct After<'a, 'b> {
    source: &'a Sources<'b>,
    order: &'a RetainedServiceRequestOrder,
}
impl PgFieldServiceNotificationSource for After<'_, '_> {
    fn facts(&self) -> &FieldServiceNotificationFacts {
        &self.source.notification
    }
    async fn require_before(&self, _: &mut PgConnection, _: TimestampMillis) -> Result<(), Error> {
        Err(changed().into())
    }
    async fn require_after(&self, c: &mut PgConnection, n: TimestampMillis) -> Result<(), Error> {
        self.source.check(c, n).await?;
        self.order.require_image(c).await
    }
}
async fn agreement_dependencies(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    input: &NormalizedServiceOrderCreate,
) -> Result<
    (
        CurrentServiceRequestAgreement,
        CurrentFieldServiceCustomer,
        Option<CurrentFieldServicePhysicalAsset>,
    ),
    Error,
> {
    let agreement = read_service_request_agreement(c, current, &input.agreement_id)
        .await?
        .ok_or_else(unavailable)?;
    let a = agreement.facts();
    let customer = read_current_field_service_customer(c, &a.company_id, &a.customer_id)
        .await?
        .ok_or_else(unavailable)?;
    let asset = if let Some(id) = a.asset_id.as_ref() {
        let value = read_current_field_service_physical_asset(
            c,
            current.holding_id(),
            &a.company_id,
            id.as_str(),
        )
        .await?
        .ok_or_else(unavailable)?;
        if value.facts().branch_id != a.branch_id {
            return Err(unavailable().into());
        }
        Some(value)
    } else {
        None
    };
    Ok((agreement, customer, asset))
}
async fn dependencies(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    input: &NormalizedServiceOrderCreate,
) -> Result<
    (
        CurrentServiceRequestAgreement,
        CurrentExistingServiceTicket,
        CurrentFieldServiceCustomer,
        Option<CurrentFieldServicePhysicalAsset>,
    ),
    Error,
> {
    let (agreement, customer, asset) = agreement_dependencies(c, current, input).await?;
    let a = agreement.facts();
    let ticket = read_existing_service_ticket(c, &a.company_id, &input.ticket_id)
        .await?
        .ok_or_else(unavailable)?;
    let t = ticket.facts();
    if t.branch_id != a.branch_id
        || t.department_id != a.department_id
        || t.customer_id != a.customer_id
    {
        return Err(unavailable().into());
    }
    Ok((agreement, ticket, customer, asset))
}
async fn replay_source(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    key: &str,
    input: &NormalizedServiceOrderCreate,
) -> Result<(), Error> {
    let o = read_service_request_order(c, current, key)
        .await?
        .ok_or_else(unavailable)?;
    let f = o.facts();
    let (a, t, customer, asset) = dependencies(c, current, input).await?;
    let af = a.facts();
    let tf = t.facts();
    if f.ticket_id != tf.id
        || f.ticket_version != input.ticket_version
        || f.agreement_id != af.id
        || f.agreement_version != input.agreement_version
        || f.asset_id != input.asset_id
        || f.branch_id != tf.branch_id.as_str()
        || f.department_id.as_deref() != tf.department_id.as_ref().map(|v| v.as_str())
    {
        return Err(changed().into());
    }
    let selected_asset = if let Some(id) = f.asset_id.as_ref().filter(|v| !v.is_empty()) {
        let value =
            read_current_field_service_physical_asset(c, current.holding_id(), &af.company_id, id)
                .await?
                .ok_or_else(unavailable)?;
        if value.facts().branch_id != tf.branch_id {
            return Err(unavailable().into());
        }
        Some(value)
    } else {
        None
    };
    let db_now = database_now(c).await?;
    let technician = inspect_field_service_assignment(
        c,
        FieldServiceAssignmentRequest {
            company_id: af.company_id.clone(),
            user_id: f.current_technician_id.clone(),
            branch_id: af.branch_id.clone(),
            department_id: af.department_id.clone(),
            permission: FieldServiceAssignmentPermission::Write,
        },
        db_now,
    )
    .await?;
    let person =
        inspect_field_service_people_reference(c, &af.company_id, &f.current_technician_id).await?;
    o.require_current(c, current, key).await?;
    a.require_current(c, current).await?;
    t.require_current(c).await?;
    customer.require_current(c).await?;
    if let Some(v) = asset {
        v.require_current(c).await?;
    }
    if let Some(v) = selected_asset {
        v.require_current(c).await?;
    }
    let db_now = database_now(c).await?;
    technician.require_current(c, db_now).await?;
    person.require_current(c).await?;
    Ok(())
}
struct Authority<'a, C> {
    current: &'a AuthorizedWorkContext,
    input: &'a NormalizedServiceOrderCreate,
    expected: &'a WorkflowRequest,
    clock: &'a C,
    purchases: Option<Purchases>,
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
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        if &actor(self.current, self.clock.now())? != a {
            return Err(changed().into());
        }
        let (agreement, ticket, customer, asset) =
            dependencies(c, self.current, self.input).await?;
        if disposition == CommandDisposition::Replay {
            replay_source(c, self.current, &r.key, self.input).await?;
        }
        match &self.purchases {
            Some(p) => p.check(c, self.current, self.clock.now()).await?,
            None => {
                self.purchases = Some(
                    Purchases::read(
                        c,
                        self.current,
                        asset.is_some()
                            || self.input.asset_id.as_ref().is_some_and(|s| !s.is_empty()),
                        disposition,
                        self.clock.now(),
                    )
                    .await?,
                )
            }
        }
        agreement.require_current(c, self.current).await?;
        ticket.require_current(c).await?;
        customer.require_current(c).await?;
        if let Some(v) = asset {
            v.require_current(c).await?;
        }
        Ok(())
    }
}
struct Prepared<'a> {
    source: Sources<'a>,
    order: PreparedServiceRequestOrder,
    notification: OwnedPreparedFieldServiceNotification,
    purchases: Purchases,
}
struct Written<'a> {
    source: Sources<'a>,
    order: RetainedServiceRequestOrder,
    notification: RetainedFieldServiceNotification,
    purchases: Purchases,
}
struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    input: &'a NormalizedServiceOrderCreate,
    key: &'a CommandKey,
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
        let source = Sources::new(
            c,
            self.current,
            self.input,
            self.key,
            self.clock.now(),
            self.clock.now(),
        )
        .await?;
        let purchases = Purchases::read(
            c,
            self.current,
            source.asset.is_some() || source.agreement.facts().asset_id.is_some(),
            CommandDisposition::Execute,
            self.clock.now(),
        )
        .await?;
        let order = prepare_service_request_order(c, &source, self.clock.now()).await?;
        let notification =
            prepare_field_service_created_notification(c, &source, self.key, self.clock.now())
                .await?
                .into_owned();
        let result = CommandJson::from_serializable(
            &serde_json::json!({"id":source.order.order_id(),"version":1,"status":source.order.status()}),
        )?;
        self.prepared = Some(Prepared {
            source,
            order,
            notification,
            purchases,
        });
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
        let prepared = self.prepared.take().ok_or_else(changed)?;
        let source = prepared.source;
        let order = prepared.order.persist(c, &source, self.clock.now()).await?;
        let after = After {
            source: &source,
            order: &order,
        };
        let notification = prepared
            .notification
            .persist(c, &after, self.clock.now())
            .await?;
        self.written = Some(Written {
            source,
            order,
            notification,
            purchases: prepared.purchases,
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
            .require_current(c, &w.source, self.clock.now())
            .await?;
        w.notification.require_current(c).await?;
        w.purchases.check(c, self.current, self.clock.now()).await
    }
}
struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}
pub(crate) struct OrderCreateDispatcher<C> {
    pub clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> super::OtherPost for OrderCreateDispatcher<C> {
    fn validate(&self, input: &super::OtherPostInput) -> Result<(), String> {
        if input.branch != crate::field_service_http_wire::OtherPostBranch::OrderCreate {
            return Err("This action is not available.".into());
        }
        normalize_service_order_create(&input.body)
            .map(|_| ())
            .map_err(|e| {
                erpx_server_field_service::public::parts::field_service_first_wire_message(
                    &input.body,
                    &e,
                )
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
            if input.branch != crate::field_service_http_wire::OtherPostBranch::OrderCreate {
                return Err(changed().into());
            }
            let normalized = normalize_service_order_create(&input.body).map_err(|_| {
                BusinessError::invalid("FIELD_SERVICE_ORDER_INVALID", "Invalid Service order.")
            })?;
            actor(current, self.clock.now())?;
            let (agreement, customer, asset) =
                agreement_dependencies(c, current, &normalized).await?;
            let physical = format!("service-order-{}", input.key);
            let candidate:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM command_receipts WHERE command_id=$1 AND operation='field.order.create' AND record_id=$2 AND actor_id=$3 AND authority_json::jsonb->>'companyId'=$4)").bind(&input.key).bind(&physical).bind(current.user_id().as_str()).bind(current.selection().workspace().company_id.as_str()).fetch_one(&mut*c).await?;
            if candidate {
                replay_source(c, current, &input.key, &normalized).await?;
            }
            let preflight = Purchases::read(
                c,
                current,
                asset.is_some() || normalized.asset_id.as_ref().is_some_and(|s| !s.is_empty()),
                if candidate {
                    CommandDisposition::Replay
                } else {
                    CommandDisposition::Execute
                },
                self.clock.now(),
            )
            .await?;
            preflight.check(c, current, self.clock.now()).await?;
            agreement.require_current(c, current).await?;
            customer.require_current(c).await?;
            if let Some(v) = asset {
                v.require_current(c).await?;
            }
            let key = CommandKey::new(input.key.clone())?;
            let request = WorkflowRequest {
                key: input.key,
                operation: "field.order.create".into(),
                record_id: format!("service-order-{}", key.as_str()),
                input: normalized.receipt_input.clone(),
            };
            let mut authority = Authority {
                current,
                input: &normalized,
                expected: &request,
                clock: self.clock.as_ref(),
                purchases: None,
            };
            let mut effects = Effects {
                current,
                input: &normalized,
                key: &key,
                clock: self.clock.as_ref(),
                prepared: None,
                written: None,
            };
            let mut ids = Ids;
            Ok(PgCommandLedger::new(
                current.selection().workspace().company_id.clone(),
                c,
                &mut authority,
                self.clock.as_ref(),
                &mut ids,
            )
            .execute(&request, &mut effects)
            .await?
            .result)
        })
    }
}
