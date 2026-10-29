//! Actual Service request composition; no other branch or actor factory.
use super::{Error, FieldServicePartsSourcePreparation, changed};
use erpx_server_assets::public::field_service_reference::{
    CurrentFieldServicePhysicalAsset, read_current_field_service_physical_asset,
};
use erpx_server_collaboration::public::field_service_notifications::{
    FieldServiceNotificationFacts, OwnedPreparedFieldServiceNotification,
    PgFieldServiceNotificationSource, RetainedFieldServiceNotification,
    prepare_field_service_created_notification,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue,
};
use erpx_server_field_service::public::{
    parts::{NormalizedServiceRequest, normalize_service_request},
    request_agreement::{CurrentServiceRequestAgreement, read_service_request_agreement},
    request_order::{
        PgServiceRequestOrderSource, PreparedServiceRequestOrder, RetainedServiceRequestOrder,
        ServiceRequestOrderFacts, prepare_service_request_order,
    },
};
use erpx_server_finance::public::field_service_customer::{
    CurrentFieldServicePhysicalCustomer, read_current_field_service_physical_customer,
};
use erpx_server_identity_access::public::{
    command_metadata::project_command_iam,
    effective_authority::{AuthorizedWorkContext, HoldingWorkSourcePreparation},
    field_service_assignment::{
        CurrentFieldServiceRawAssignment, FieldServiceAssignmentPermission,
        FieldServiceRawAssignmentRequest, inspect_field_service_raw_assignment,
    },
    work_context::WorkScopeKind,
};
use erpx_server_people_payroll::public::field_service_worker::{
    CurrentFieldServicePeopleReference, inspect_field_service_people_reference,
};
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, FailureKind, IdentifierSource, RecordId, TimestampMillis,
};
use erpx_server_support::public::field_service_request::{
    PgServiceIntakeTicketSource, PreparedServiceIntakeTicket, RetainedServiceIntakeTicket,
    ServiceIntakeTicketFacts, prepare_service_intake_ticket,
};
use erpx_server_workflow::public::commands::*;
use sqlx::PgConnection;
use std::sync::Arc;
pub(super) async fn database_now(c: &mut PgConnection) -> Result<TimestampMillis, Error> {
    let ms: i64 =
        sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint")
            .fetch_one(c)
            .await?;
    Ok(TimestampMillis::new(ms))
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "FIELD_SERVICE_AGREEMENT_NOT_FOUND",
        "The original Service source is not available in this workspace.",
    )
}
pub(super) fn actor(
    current: &AuthorizedWorkContext,
    now: TimestampMillis,
) -> Result<CommandActor, BusinessError> {
    current.require_permission("records.read")?;
    current.require_permission("records.write")?;
    if !current.modules().iter().any(|m| m == "field_service") || now.get() >= current.expires_at()
    {
        return Err(changed());
    }
    let s = current.selection();
    let w = s.workspace();
    let a = s.assignment();
    Ok(CommandActor {
        user_id: current.user_id().clone(),
        assignment_id: a.id.clone(),
        role: a.role.clone(),
        company_id: w.company_id.clone(),
        branch_id: w
            .branch_id
            .as_ref()
            .map(|id| CommandField::Value(id.as_str().into()))
            .unwrap_or(CommandField::Null),
        department_id: w
            .department_id
            .as_ref()
            .map(|id| CommandField::Value(id.as_str().into()))
            .unwrap_or(CommandField::Null),
        authority: CommandField::Value(WorkAuthority {
            session_id: current.session_id().as_str().into(),
            revision: s.revision(),
            assignment_version: a.version,
        }),
        iam: CommandField::Value(CommandJson::from_serializable(&project_command_iam(
            current,
        )?)?),
        assignment_scope_kind: match a.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        assignment_scope_node_id: a.scope_node_id.as_ref().map(|id| id.as_str().into()),
    })
}
struct RequestSources<'a> {
    current: &'a AuthorizedWorkContext,
    agreement: CurrentServiceRequestAgreement,
    customer: CurrentFieldServicePhysicalCustomer,
    asset: Option<CurrentFieldServicePhysicalAsset>,
    technician: CurrentFieldServiceRawAssignment,
    person: CurrentFieldServicePeopleReference,
    order: ServiceRequestOrderFacts,
    ticket: ServiceIntakeTicketFacts,
    notification: FieldServiceNotificationFacts,
}
impl<'a> RequestSources<'a> {
    async fn new(
        c: &mut PgConnection,
        current: &'a AuthorizedWorkContext,
        input: &NormalizedServiceRequest,
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
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "FIELD_SERVICE_AGREEMENT_CHANGED",
                "Refresh the agreement before requesting service.",
            )
            .into());
        }
        agreement.require_new_work(c).await?;
        let customer =
            read_current_field_service_physical_customer(c, &a.company_id, &a.customer_id)
                .await?
                .ok_or_else(unavailable)?;
        let asset = if let Some(id) = &a.asset_id {
            let asset = read_current_field_service_physical_asset(
                c,
                current.holding_id(),
                &a.company_id,
                id,
            )
            .await?
            .ok_or_else(unavailable)?;
            let facts = asset.facts();
            if facts.branch_id != a.branch_id
                || matches!(
                    facts.lifecycle_status.as_str(),
                    "retired" | "retirement_pending"
                )
                || current
                    .selection()
                    .workspace()
                    .department_id
                    .as_ref()
                    .is_some_and(|d| facts.department_id.as_deref() != Some(d.as_str()))
            {
                return Err(unavailable().into());
            }
            Some(asset)
        } else {
            None
        };
        let db_now = database_now(c).await?;
        let technician = inspect_field_service_raw_assignment(
            c,
            FieldServiceRawAssignmentRequest {
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
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "FIELD_SERVICE_TECHNICIAN_UNAVAILABLE",
                "The selected technician is unavailable.",
            )
            .into());
        }
        let k = key.as_str().to_owned();
        let order_id = format!("service-order-{k}");
        let ticket_id = format!("ticket-{k}");
        let order = ServiceRequestOrderFacts {
            key: k.clone(),
            company_id: a.company_id.clone(),
            branch_id: a.branch_id.as_str().into(),
            department_id: a.department_id.as_ref().map(|v| v.as_str().into()),
            ticket_id,
            ticket_version: 1,
            agreement_id: a.id.as_str().into(),
            agreement_version: a.version,
            asset_id: a.asset_id.as_ref().map(|v| v.as_str().into()),
            issue: input.issue.clone(),
            site: input.site.clone(),
            technician_id: input.technician_id.clone(),
            scheduled_at: input.scheduled_at,
            actor_id: current.user_id().as_str().into(),
            at: at.get(),
        };
        let ticket = ServiceIntakeTicketFacts {
            key: k.clone(),
            company_id: a.company_id.clone(),
            branch_id: a.branch_id.as_str().into(),
            department_id: a.department_id.as_ref().map(|v| v.as_str().into()),
            customer_id: a.customer_id.as_str().into(),
            agreement_id: a.id.as_str().into(),
            agreement_version: a.version,
            title: input.title.clone(),
            issue: input.issue.clone(),
            technician_id: input.technician_id.clone(),
            scheduled_at: input.scheduled_at,
            actor_id: current.user_id().as_str().into(),
            at: at.get(),
        };
        let notification = FieldServiceNotificationFacts {
            owner_key: key.clone(),
            holding_id: current.holding_id().clone(),
            company_id: a.company_id.clone(),
            order_id,
            technician_user_id: input.technician_id.clone(),
            order_version_after: 1,
            updated_at_after: at,
        };
        Ok(Self {
            current,
            agreement,
            customer,
            asset,
            technician,
            person,
            order,
            ticket,
            notification,
        })
    }
    async fn check(&self, c: &mut PgConnection, now: TimestampMillis) -> Result<(), Error> {
        actor(self.current, now)?;
        self.agreement.require_current(c, self.current).await?;
        self.agreement.require_new_work(c).await?;
        self.customer.require_current(c).await?;
        if let Some(asset) = &self.asset {
            asset.require_current(c).await?;
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
impl PgServiceRequestOrderSource for RequestSources<'_> {
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
impl PgServiceIntakeTicketSource for RequestSources<'_> {
    fn facts(&self) -> &ServiceIntakeTicketFacts {
        &self.ticket
    }
    async fn require_before(&self, c: &mut PgConnection, n: TimestampMillis) -> Result<(), Error> {
        self.check(c, n).await
    }
    async fn require_after(&self, _: &mut PgConnection, _: TimestampMillis) -> Result<(), Error> {
        Err(changed().into())
    }
}
impl PgFieldServiceNotificationSource for RequestSources<'_> {
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
    source: &'a RequestSources<'b>,
    order: &'a RetainedServiceRequestOrder,
}
impl PgServiceIntakeTicketSource for After<'_, '_> {
    fn facts(&self) -> &ServiceIntakeTicketFacts {
        &self.source.ticket
    }
    async fn require_before(&self, _: &mut PgConnection, _: TimestampMillis) -> Result<(), Error> {
        Err(changed().into())
    }
    async fn require_after(&self, c: &mut PgConnection, n: TimestampMillis) -> Result<(), Error> {
        self.source.check(c, n).await?;
        self.order.require_image(c).await
    }
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
pub(super) struct Purchases {
    catalogue: CommercialCatalogue,
    history: PurchaseWitness,
    new_work: Option<PurchaseWitness>,
}
impl Purchases {
    pub(super) async fn read(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        asset: bool,
        disposition: CommandDisposition,
        now: TimestampMillis,
    ) -> Result<Self, Error> {
        let catalogue = current_package_catalogue()?;
        let mut caps = vec!["service.orders".into(), "support.source-ticket".into()];
        if asset {
            caps.push("service.parts-assets".into());
        }
        let mut reader = PgCompanyPurchaseReader::new(
            current.holding_id().clone(),
            current.selection().workspace().company_id.clone(),
            c,
            &catalogue,
        );
        let inspect = |purpose| PurchaseRequest {
            capabilities: caps.clone(),
            purpose,
            mode: PackageEnforcementMode::Enforced,
        };
        let history = reader
            .inspect(&inspect(CapabilityPurpose::History), now)
            .await?
            .require_allowed()?
            .clone();
        let new_work = if disposition == CommandDisposition::Execute {
            Some(
                reader
                    .inspect(&inspect(CapabilityPurpose::NewWork), now)
                    .await?
                    .require_allowed()?
                    .clone(),
            )
        } else {
            None
        };
        Ok(Self {
            catalogue,
            history,
            new_work,
        })
    }
    pub(super) async fn check(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        let mut reader = PgCompanyPurchaseReader::new(
            current.holding_id().clone(),
            current.selection().workspace().company_id.clone(),
            c,
            &self.catalogue,
        );
        reader
            .revalidate(&self.history, now)
            .await?
            .require_allowed()?;
        if let Some(w) = &self.new_work {
            reader.revalidate(w, now).await?.require_allowed()?;
        }
        Ok(())
    }
}
async fn require_replay_source(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    key: &str,
    agreement_id: &str,
    _now: TimestampMillis,
) -> Result<(), Error> {
    let order = erpx_server_field_service::public::request_order::read_service_request_order(
        c, current, key,
    )
    .await?
    .ok_or_else(unavailable)?;
    let ticket = erpx_server_support::public::field_service_request::read_service_intake(
        c,
        &current.selection().workspace().company_id,
        key,
    )
    .await?
    .ok_or_else(unavailable)?;
    let agreement = read_service_request_agreement(c, current, agreement_id)
        .await?
        .ok_or_else(unavailable)?;
    let o = order.facts();
    let t = ticket.facts();
    let a = agreement.facts();
    if o.agreement_id != agreement_id
        || o.order_id != t.order_id
        || o.ticket_id != t.ticket_id
        || o.agreement_id != t.agreement_id
        || o.agreement_version != t.agreement_version
        || o.actor_id != t.actor_id
        || o.event_status
            != if o.scheduled_at.is_some() {
                "scheduled"
            } else {
                "open"
            }
        || o.initial_at != t.initial_at
        || o.issue != t.initial_issue
        || o.technician_id != t.initial_owner
        || o.branch_id != t.branch_id
        || o.department_id != t.department_id
        || a.branch_id.as_str() != o.branch_id
        || a.department_id.as_deref() != o.department_id.as_deref()
        || a.customer_id.as_str() != t.customer_id
    {
        return Err(changed().into());
    }
    let db_now = database_now(c).await?;
    let technician = inspect_field_service_raw_assignment(
        c,
        FieldServiceRawAssignmentRequest {
            company_id: a.company_id.clone(),
            user_id: o.current_technician_id.clone(),
            branch_id: a.branch_id.clone(),
            department_id: a.department_id.clone(),
            permission: FieldServiceAssignmentPermission::Write,
        },
        db_now,
    )
    .await?;
    let person =
        inspect_field_service_people_reference(c, &a.company_id, &o.current_technician_id).await?;
    order.require_current(c, current, key).await?;
    ticket.require_current(c, &a.company_id, key).await?;
    ticket
        .require_initial_schedule(c, key, o.scheduled_at, o.initial_at)
        .await?;
    agreement.require_current(c, current).await?;
    let db_now = database_now(c).await?;
    technician.require_current(c, db_now).await?;
    person.require_current(c).await?;
    Ok(())
}
struct Authority<'a, C> {
    current: &'a AuthorizedWorkContext,
    input: &'a NormalizedServiceRequest,
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
        let agreement = read_service_request_agreement(c, self.current, &self.input.agreement_id)
            .await?
            .ok_or_else(unavailable)?;
        let facts = agreement.facts();
        let customer =
            read_current_field_service_physical_customer(c, &facts.company_id, &facts.customer_id)
                .await?
                .ok_or_else(unavailable)?;
        let asset = if let Some(id) = &facts.asset_id {
            let value = read_current_field_service_physical_asset(
                c,
                self.current.holding_id(),
                &facts.company_id,
                id,
            )
            .await?
            .ok_or_else(unavailable)?;
            if value.facts().branch_id != facts.branch_id {
                return Err(unavailable().into());
            }
            Some(value)
        } else {
            None
        };
        if disposition == CommandDisposition::Replay {
            require_replay_source(c, self.current, &r.key, &facts.id, self.clock.now()).await?;
        }
        match &self.purchases {
            Some(p) => p.check(c, self.current, self.clock.now()).await?,
            None => {
                self.purchases = Some(
                    Purchases::read(
                        c,
                        self.current,
                        asset.is_some(),
                        disposition,
                        self.clock.now(),
                    )
                    .await?,
                )
            }
        }
        agreement.require_current(c, self.current).await?;
        customer.require_current(c).await?;
        if let Some(value) = asset {
            value.require_current(c).await?;
        }
        Ok(())
    }
}
struct Prepared<'a> {
    source: RequestSources<'a>,
    order: PreparedServiceRequestOrder,
    ticket: PreparedServiceIntakeTicket,
    notification: OwnedPreparedFieldServiceNotification,
    purchases: Purchases,
}
struct Written<'a> {
    source: RequestSources<'a>,
    order: RetainedServiceRequestOrder,
    ticket: RetainedServiceIntakeTicket,
    notification: RetainedFieldServiceNotification,
    purchases: Purchases,
}
struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    input: &'a NormalizedServiceRequest,
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
        let at = self.clock.now();
        let source =
            RequestSources::new(c, self.current, self.input, self.key, at, self.clock.now())
                .await?;
        let purchases = Purchases::read(
            c,
            self.current,
            source.asset.is_some(),
            CommandDisposition::Execute,
            self.clock.now(),
        )
        .await?;
        let ticket = prepare_service_intake_ticket(c, &source, self.clock.now()).await?;
        let order = prepare_service_request_order(c, &source, self.clock.now()).await?;
        let notification =
            prepare_field_service_created_notification(c, &source, self.key, self.clock.now())
                .await?
                .into_owned();
        let result = CommandJson::from_serializable(
            &serde_json::json!({"id":source.order.order_id(),"ticketId":source.ticket.ticket_id(),"version":1,"status":source.order.status()}),
        )?;
        self.prepared = Some(Prepared {
            source,
            order,
            ticket,
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
        let ticket = prepared
            .ticket
            .persist(c, &source, self.clock.now())
            .await?;
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
            ticket,
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
        let after = After {
            source: &w.source,
            order: &w.order,
        };
        w.ticket
            .require_current(c, &after, self.clock.now())
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
/// Real Request branch, only constructed by explicit private bootstrap.
pub struct RequestDispatcher<C> {
    pub clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> super::OtherPost for RequestDispatcher<C> {
    fn validate(&self, input: &super::OtherPostInput) -> Result<(), String> {
        if input.branch != crate::field_service_http_wire::OtherPostBranch::Request {
            return Err("This action is not available.".into());
        }
        normalize_service_request(&input.body)
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
            if input.branch != crate::field_service_http_wire::OtherPostBranch::Request {
                return Err(changed().into());
            }
            let normalized = normalize_service_request(&input.body).map_err(|_| {
                BusinessError::invalid("FIELD_SERVICE_REQUEST_INVALID", "Invalid Service request.")
            })?;
            // Real original dependency and History precede late key admission.
            let agreement = read_service_request_agreement(c, current, &normalized.agreement_id)
                .await?
                .ok_or_else(unavailable)?;
            actor(current, self.clock.now())?;
            let facts = agreement.facts();
            let customer = read_current_field_service_physical_customer(
                c,
                &facts.company_id,
                &facts.customer_id,
            )
            .await?
            .ok_or_else(unavailable)?;
            let asset = if let Some(id) = &facts.asset_id {
                let value = read_current_field_service_physical_asset(
                    c,
                    current.holding_id(),
                    &facts.company_id,
                    id,
                )
                .await?
                .ok_or_else(unavailable)?;
                if value.facts().branch_id != facts.branch_id {
                    return Err(unavailable().into());
                }
                Some(value)
            } else {
                None
            };
            let physical = format!("service-order-{}", input.key);
            let candidate:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM command_receipts WHERE command_id=$1 AND operation='field.order.request' AND record_id=$2 AND actor_id=$3 AND authority_json::jsonb->>'companyId'=$4)")
                .bind(&input.key).bind(&physical).bind(current.user_id().as_str()).bind(facts.company_id.as_str()).fetch_one(&mut *c).await?;
            if candidate {
                require_replay_source(c, current, &input.key, &facts.id, self.clock.now()).await?;
            }
            let preflight = Purchases::read(
                c,
                current,
                asset.is_some(),
                if candidate {
                    CommandDisposition::Replay
                } else {
                    CommandDisposition::Execute
                },
                self.clock.now(),
            )
            .await?;
            preflight.check(c, current, self.clock.now()).await?;
            customer.require_current(c).await?;
            if let Some(value) = asset {
                value.require_current(c).await?;
            }
            let key = CommandKey::new(input.key.clone())?;
            let request = WorkflowRequest {
                key: input.key,
                operation: "field.order.request".into(),
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
            agreement.require_current(c, current).await?;
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
