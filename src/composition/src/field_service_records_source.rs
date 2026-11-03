//! Field owner-backed Records source in the caller's retained Holding unit.
use crate::field_service_source_record::{
    self, PreparedFieldSourceRecord, SourceRecordContext, SourceRecordMode,
};
use erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation;
use erpx_server_records::public::source_record::{
    SourceKind, SourceRecord, SourceRecordPort, SourceReference,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;
mod recursive;
use recursive::PreparedLeafSource;
/// All participating sources are fenced before the first selected IAM
/// snapshot, following the existing Finance/Commercial/Collaboration/Support/
/// Assets/People/IAM/Field order. Workflow's row-share preparation preserves
/// concurrent receipt writers; it does not confer replay authority.
#[derive(Clone)]
pub(crate) struct FieldCollaborationSources;
impl HoldingWorkSourcePreparation for FieldCollaborationSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_finance::public::field_service_workspace::lock_service_workspace_customers(
                c,
            )
            .await?;
            erpx_server_commercial::public::purchases::lock_commercial_authority_sources(c).await?;
            erpx_server_workflow::public::record_origin::lock_record_origin_sources(c).await?;
            erpx_server_collaboration::public::collaboration_work::lock_collaboration_work_sources(
                c,
            )
            .await?;
            erpx_server_support::public::field_service_workspace::lock_service_workspace_tickets(c)
                .await?;
            erpx_server_assets::public::field_service_workspace::lock_service_workspace_assets(c)
                .await?;
            erpx_server_pms::public::record_source::lock_record_sources(c).await?;
            erpx_server_pos::public::record_source::lock_record_sources(c).await?;
            erpx_server_organization::public::field_service_workspace::lock_service_workspace_locations(c).await?;
            erpx_server_people_payroll::public::field_service_worker::lock_people_authority_sources(c).await?;
            erpx_server_identity_access::public::field_service_workspace::lock_service_workspace_authority(c).await?;
            erpx_server_field_service::public::source_record::lock_source_record(c).await?;
            Ok(())
        })
    }
}
/// Every retained owner proof belongs to the caller's serializable unit.
/// Field-managed Support cases have no recursive source_record_json by schema;
/// their service_order_id is navigation, not an extra visibility requirement.
#[allow(clippy::large_enum_variant)]
pub(crate) enum PreparedRecordsSource {
    Field(PreparedFieldSourceRecord),
    Support {
        ticket: erpx_server_support::public::field_ticket_read::CurrentSupportTicketRead,
        purchase: erpx_server_commercial::public::purchases::PurchaseWitness,
        origin: Option<PreparedLeafSource>,
    },
}
impl PreparedRecordsSource {
    pub(crate) fn support_data(&self) -> Option<&serde_json::Value> {
        match self {
            Self::Support { ticket, .. } => Some(ticket.data()),
            _ => None,
        }
    }
    pub(crate) async fn require_current<C: Clock>(
        &self,
        c: &mut PgConnection,
        ctx: &SourceRecordContext<'_, C>,
    ) -> Result<(), Error> {
        match self {
            Self::Field(w) => w.require_current(c, ctx).await?,
            Self::Support {
                ticket,
                purchase,
                origin,
            } => {
                if let Some(origin) = origin {
                    origin.require_current(c, ctx).await?;
                }
                ticket.require_current(c).await?;
                erpx_server_commercial::public::purchases::PgCompanyPurchaseReader::new(
                    ctx.holding.clone(),
                    ctx.company.clone(),
                    c,
                    ctx.catalogue,
                )
                .revalidate(purchase, ctx.clock.now())
                .await?;
                if ctx.clock.now().get() >= ctx.actor.expires_at() {
                    return Err(BusinessError::new(
                        FailureKind::NotFound,
                        "SOURCE_RECORD_NOT_FOUND",
                        "Source record is unavailable in this work scope.",
                    )
                    .into());
                }
            }
        }
        Ok(())
    }
}
/// The caller must first use `SourceRecordSources` to lock all owner facts
/// before its selected IAM snapshot. No independent transaction is opened.
pub(crate) struct FieldRecordsSourcePort<'a, C> {
    pub connection: &'a mut PgConnection,
    pub context: SourceRecordContext<'a, C>,
}
impl<C: Clock + Sync + Send> SourceRecordPort for FieldRecordsSourcePort<'_, C> {
    type Error = Error;
    type Witness = PreparedRecordsSource;
    async fn prepare(
        &mut self,
        reference: &SourceReference,
    ) -> Result<(SourceRecord, Self::Witness), Error> {
        if !matches!(
            reference.kind,
            SourceKind::FieldServiceOrder | SourceKind::SupportTicket
        ) {
            return Err(BusinessError::invalid(
                "SOURCE_RECORD_INVALID",
                "This native source kind is not available in the Field dependency closure.",
            )
            .into());
        }
        let actor = self.context.actor;
        if actor.holding_id() != self.context.holding
            || &actor.selection().workspace().company_id != self.context.company
        {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "SOURCE_RECORD_NOT_FOUND",
                "Source record is unavailable in this work scope.",
            )
            .into());
        }
        if reference.kind == SourceKind::SupportTicket {
            use erpx_server_commercial::public::purchases::{
                CapabilityPurpose, PackageEnforcementMode, PgCompanyPurchaseReader, PurchaseRequest,
            };
            let scope = actor.selection().workspace();
            let ticket =
                erpx_server_support::public::field_ticket_read::read_support_record_ticket(
                    self.connection,
                    self.context.company,
                    &reference.id,
                    scope.branch_id.as_ref().map(|v| v.as_str()),
                    scope.department_id.as_ref().map(|v| v.as_str()),
                )
                .await?
                .ok_or_else(|| {
                    BusinessError::new(
                        FailureKind::NotFound,
                        "SOURCE_RECORD_NOT_FOUND",
                        "Source record is unavailable in this work scope.",
                    )
                })?;
            ticket.require_reader(
                actor.user_id().as_str(),
                actor.has_permission("support.read"),
                actor.has_permission("records.read"),
            )?;
            let row = &ticket.data()["row"];
            let branch = row["branch_id"].as_str().map(str::to_owned);
            let department = row["department_id"].as_str().map(str::to_owned);
            let assignment = actor.selection().assignment();
            let node = assignment.scope_node_id.as_ref().map(|v| v.as_str());
            let visible = match assignment.scope_kind {
                erpx_server_identity_access::public::work_context::WorkScopeKind::Holding => true,
                erpx_server_identity_access::public::work_context::WorkScopeKind::Branch => {
                    node.is_some() && node == branch.as_deref()
                }
                erpx_server_identity_access::public::work_context::WorkScopeKind::Department => {
                    node.is_some() && node == department.as_deref()
                }
            };
            if !visible {
                return Err(BusinessError::new(
                    FailureKind::NotFound,
                    "SOURCE_RECORD_NOT_FOUND",
                    "Source record is unavailable in this work scope.",
                )
                .into());
            }
            let record = ticket.record()?;
            let origin = if let Some(reference) = ticket.origin()? {
                match recursive::prepare(self.connection, &self.context, &reference).await {
                    Ok(value) => Some(value),
                    Err(PgTransactionError::Business(e))
                        if matches!(e.kind, FailureKind::Forbidden | FailureKind::NotFound) =>
                    {
                        return Err(BusinessError::new(
                            FailureKind::NotFound,
                            "SUPPORT_TICKET_NOT_FOUND",
                            "Ticket not found in this scope.",
                        )
                        .into());
                    }
                    Err(error) => return Err(error),
                }
            } else {
                None
            };
            let purchase = PgCompanyPurchaseReader::new(
                self.context.holding.clone(),
                self.context.company.clone(),
                self.connection,
                self.context.catalogue,
            )
            .inspect(
                &PurchaseRequest {
                    capabilities: vec!["support.tickets".into()],
                    purpose: CapabilityPurpose::History,
                    mode: PackageEnforcementMode::Enforced,
                },
                self.context.clock.now(),
            )
            .await?;
            purchase.require_allowed()?;
            return Ok((
                record,
                PreparedRecordsSource::Support {
                    ticket,
                    purchase: purchase.witness,
                    origin,
                },
            ));
        }
        actor.require_permission("records.read")?;
        if !actor
            .modules()
            .iter()
            .any(|module| module == "field_service")
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MODULE_FORBIDDEN",
                "This source's owning workspace is not available to the selected role.",
            )
            .into());
        }
        let witness = field_service_source_record::prepare(
            self.connection,
            &self.context,
            &reference.id,
            reference.expected_version.map(|v| v as f64),
            SourceRecordMode::Enforced,
        )
        .await?;
        let data = &witness.data().data;
        let record = SourceRecord {
            kind: reference.kind,
            id: data.id.clone(),
            company_id: data.company_id.clone(),
            branch_id: data.branch_id.clone(),
            department_id: data.department_id.clone(),
            version: data.version,
            title: data.title.clone(),
            status: data.status.clone(),
        };
        Ok((record, PreparedRecordsSource::Field(witness)))
    }
    async fn require_current(&mut self, witness: &Self::Witness) -> Result<(), Error> {
        witness
            .require_current(self.connection, &self.context)
            .await
    }
}

/// A selected action owner is eligible only when both owning modules confirm
/// the original scoped records.write assignment and active People engagement.
/// This is an owner fact, not a grant to the acting user.
pub(crate) struct PreparedCollaborationAssignee {
    iam: erpx_server_identity_access::public::field_service_workspace::CurrentServiceWorkspaceAuthority,
    people: erpx_server_people_payroll::public::field_service_worker::CurrentFieldServicePeopleReference,
    eligible: bool,
}
impl PreparedCollaborationAssignee {
    pub(crate) fn eligible(&self) -> bool {
        self.eligible
    }
    pub(crate) async fn require_current<C: Clock>(
        &self,
        c: &mut PgConnection,
        clock: &C,
    ) -> Result<(), Error> {
        self.iam.require_current(c, clock.now()).await?;
        self.people.require_current(c).await?;
        Ok(())
    }
}
pub(crate) async fn prepare_assignee<C: Clock>(
    c: &mut PgConnection,
    ctx: &SourceRecordContext<'_, C>,
    user: &str,
    branch: &str,
    department: Option<&str>,
) -> Result<PreparedCollaborationAssignee, Error> {
    use erpx_server_identity_access::public::field_service_workspace::{
        ServiceWorkspaceAssignmentQuery, ServiceWorkspaceAuthorityRequest,
        read_service_workspace_authority,
    };
    use erpx_server_people_payroll::public::field_service_worker::inspect_field_service_people_reference;
    let iam = read_service_workspace_authority(
        c,
        ServiceWorkspaceAuthorityRequest {
            company: ctx.company.clone(),
            actor: ctx.actor.user_id().as_str().into(),
            assignments: vec![ServiceWorkspaceAssignmentQuery {
                user_id: user.into(),
                branch_id: branch.into(),
                department_id: department.map(str::to_owned),
            }],
            same_people: vec![],
        },
        ctx.clock.now(),
    )
    .await?;
    let people = inspect_field_service_people_reference(c, ctx.company, user).await?;
    let eligible = people.is_eligible()
        && iam.data().assignments.iter().any(|fact| {
            fact.user_id == user
                && fact.branch_id == branch
                && fact.department_id.as_deref() == department
                && fact.write
        });
    Ok(PreparedCollaborationAssignee {
        iam,
        people,
        eligible,
    })
}
