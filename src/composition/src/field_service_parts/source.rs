//! Real typed foreign references for existing Field Service parts jobs. Current
//! IAM still comes only from the normal retained HoldingWorkAuthorizer unit.
use erpx_server_assets::public::field_service_reference::{
    CurrentFieldServicePhysicalAsset, read_current_field_service_physical_asset,
};
use erpx_server_field_service::public::{
    parts::{
        FieldServicePartsActionKind, FieldServicePartsAuthorityFacts, require_parts_authority,
    },
    parts_source::{FieldServiceOrderFacts, PgFieldServiceOrderReferences},
};
use erpx_server_finance::public::field_service_customer::{
    CurrentFieldServicePhysicalCustomer, read_current_field_service_physical_customer,
};
use erpx_server_identity_access::public::{
    effective_authority::AuthorizedWorkContext,
    field_service_assignment::{
        CurrentFieldServiceRawAssignment, FieldServiceAssignmentPermission,
        FieldServiceRawAssignmentRequest, inspect_field_service_raw_assignment,
    },
};
use erpx_server_people_payroll::public::field_service_worker::{
    CurrentFieldServicePeopleReference, inspect_field_service_people_reference,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind, TimestampMillis};
use erpx_server_support::public::field_service::{
    CurrentSupportServiceTicket, SupportServiceTicketRequest, read_current_service_ticket,
};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
#[derive(Eq, PartialEq)]
struct ActorBinding {
    holding: String,
    company: String,
    user: String,
    session: String,
    assignment: String,
    role: String,
    branch: Option<String>,
    department: Option<String>,
    version: i64,
    revision: i64,
}
fn actor(current: &AuthorizedWorkContext) -> ActorBinding {
    ActorBinding {
        holding: current.holding_id().as_str().into(),
        company: current.selection().workspace().company_id.as_str().into(),
        user: current.user_id().as_str().into(),
        session: current.session_id().as_str().into(),
        assignment: current.selection().assignment().id.as_str().into(),
        role: current.selection().assignment().role.clone(),
        branch: current
            .selection()
            .workspace()
            .branch_id
            .as_ref()
            .map(|id| id.as_str().into()),
        department: current
            .selection()
            .workspace()
            .department_id
            .as_ref()
            .map(|id| id.as_str().into()),
        version: current.selection().assignment().version,
        revision: current.selection().revision(),
    }
}
/// Private constructor: every field comes from an actual owner-public source,
/// not caller labels, Source DTOs or permissive booleans.
pub(super) struct FieldServiceReferences {
    order: FieldServiceOrderFacts,
    actor: ActorBinding,
    pub(super) support: CurrentSupportServiceTicket,
    customer: CurrentFieldServicePhysicalCustomer,
    asset: Option<CurrentFieldServicePhysicalAsset>,
    technician_assignment: CurrentFieldServiceRawAssignment,
    technician_person: CurrentFieldServicePeopleReference,
    reviewer_assignment: CurrentFieldServiceRawAssignment,
    reviewer_person: CurrentFieldServicePeopleReference,
}
impl FieldServiceReferences {
    pub(super) async fn retain(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        order: &FieldServiceOrderFacts,
        now: TimestampMillis,
    ) -> Result<Self, Error> {
        if current.holding_id() != &order.holding_id
            || current.selection().workspace().company_id != order.company_id
        {
            return Err(changed());
        }
        let support = read_current_service_ticket(
            c,
            &SupportServiceTicketRequest {
                holding_id: order.holding_id.clone(),
                company_id: order.company_id.clone(),
                ticket_id: order.ticket_id.clone(),
                branch_id: order.branch_id.clone(),
                department_id: order.department_id.clone(),
                customer_id: order.customer_id.clone(),
            },
        )
        .await?
        .ok_or_else(unavailable)?;
        let customer =
            read_current_field_service_physical_customer(c, &order.company_id, &order.customer_id)
                .await?
                .ok_or_else(unavailable)?;
        let asset = match &order.asset_id {
            None => None,
            Some(id) => {
                let proof = read_current_field_service_physical_asset(
                    c,
                    &order.holding_id,
                    &order.company_id,
                    id,
                )
                .await?
                .ok_or_else(unavailable)?;
                if proof.facts().branch_id != order.branch_id {
                    return Err(unavailable());
                }
                Some(proof)
            }
        };
        let technician_assignment = inspect_field_service_raw_assignment(
            c,
            FieldServiceRawAssignmentRequest {
                company_id: order.company_id.clone(),
                user_id: order.technician_id.clone(),
                branch_id: order.branch_id.clone(),
                department_id: order.department_id.clone(),
                permission: FieldServiceAssignmentPermission::Write,
            },
            now,
        )
        .await?;
        let technician_person =
            inspect_field_service_people_reference(c, &order.company_id, &order.technician_id)
                .await?;
        let reviewer_assignment = inspect_field_service_raw_assignment(
            c,
            FieldServiceRawAssignmentRequest {
                company_id: order.company_id.clone(),
                user_id: current.user_id().as_str().to_owned(),
                branch_id: order.branch_id.clone(),
                department_id: order.department_id.clone(),
                permission: FieldServiceAssignmentPermission::Review,
            },
            now,
        )
        .await?;
        let reviewer_person = inspect_field_service_people_reference(
            c,
            &order.company_id,
            current.user_id().as_str(),
        )
        .await?;
        Ok(Self {
            order: order.clone(),
            actor: actor(current),
            support,
            customer,
            asset,
            technician_assignment,
            technician_person,
            reviewer_assignment,
            reviewer_person,
        })
    }
}
impl PgFieldServiceOrderReferences for FieldServiceReferences {
    async fn require_visit_action(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        order: &FieldServiceOrderFacts,
        action: erpx_server_field_service::public::visit::VisitAction,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.require_current(c, current, order, now).await?;
        erpx_server_field_service::public::visit::require_visit_authority(
            &erpx_server_field_service::public::visit::VisitAuthorityFacts {
                technician_user_id: &order.technician_id,
                actor_user_id: current.user_id().as_str(),
                technician_eligible: self.technician_assignment.is_eligible()
                    && self.technician_person.is_eligible(),
                reviewer_eligible: self.reviewer_assignment.is_eligible()
                    && self.reviewer_person.is_eligible(),
                selected_can_review: current.has_permission("records.review"),
                state: &order.state,
            },
            action,
        )?;
        Ok(())
    }
    async fn require_current(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        order: &FieldServiceOrderFacts,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if &self.order != order || self.actor != actor(current) {
            return Err(changed());
        }
        self.support.require_current(c).await?;
        self.customer.require_current(c).await?;
        if let Some(asset) = &self.asset {
            asset.require_current(c).await?;
        }
        self.technician_assignment.require_current(c, now).await?;
        self.technician_person.require_current(c).await?;
        self.reviewer_assignment.require_current(c, now).await?;
        self.reviewer_person.require_current(c).await?;
        Ok(())
    }
    async fn require_parts_action(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        order: &FieldServiceOrderFacts,
        action: FieldServicePartsActionKind,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.require_current(c, current, order, now).await?;
        require_parts_authority(
            &FieldServicePartsAuthorityFacts {
                technician_user_id: order.technician_id.as_str(),
                actor_user_id: current.user_id().as_str(),
                technician_eligible: self.technician_assignment.is_eligible()
                    && self.technician_person.is_eligible(),
                reviewer_eligible: self.reviewer_assignment.is_eligible()
                    && self.reviewer_person.is_eligible(),
                selected_can_review: current.has_permission("records.review"),
                state: &order.state,
            },
            action,
        )?;
        Ok(())
    }
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_SOURCE_CHANGED",
        "The original Service source or actor binding changed.",
    )
    .into()
}
fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "FIELD_SERVICE_ORDER_NOT_FOUND",
        "Service order not found.",
    )
    .into()
}
