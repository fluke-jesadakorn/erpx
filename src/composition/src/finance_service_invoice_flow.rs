//! Read composition for Field-origin Finance invoice Flow. Every foreign fact
//! arrives through the owning public port on this same retained unit.
use erpx_server_finance::public::field_service_invoice_read::service_invoice_flow_excluded;
use erpx_server_identity_access::public::{
    effective_authority::AuthorizedWorkContext,
    journal_flow_participants::{
        current_journal_flow_principal, eligible_service_invoice_flow_participant,
        service_invoice_flow_administrator,
    },
    selected_command_authority::require_selected_actor_current,
};
use erpx_server_organization::public::journal_flow_department::journal_flow_team_name;
use erpx_server_people_payroll::public::journal_flow_people::current_journal_flow_people;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use erpx_server_workflow::public::service_invoice_flow::{
    ServiceInvoiceFlowBasis, read_service_invoice_flow_basis, service_invoice_flow_view,
};
use serde_json::{Value, json};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FLOW_SOURCE_CHANGED",
        "The invoice workflow changed. Reload its current source.",
    )
    .into()
}
pub struct CurrentServiceInvoiceFlow {
    basis: ServiceInvoiceFlowBasis,
    pub view: Value,
}
async fn view<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    record: &Value,
    basis: &ServiceInvoiceFlowBasis,
    settle: bool,
    clock: &C,
) -> Result<Value, Error> {
    let company = &current.selection().workspace().company_id;
    let actor = erpx_server_finance::public::FinanceActor {
        user_id: current.user_id().clone(),
        holding_id: current.holding_id().clone(),
        company_id: company.clone(),
        assignment_id: current.selection().assignment().id.clone(),
    };
    let id = record["id"].as_str().ok_or_else(changed)?;
    let excluded = service_invoice_flow_excluded(c, &actor, id).await?;
    let creator = record["created_by"]
        .as_str()
        .ok_or_else(changed)?
        .to_owned();
    let mut pools: [Vec<Value>; 2] = [Vec::new(), Vec::new()];
    let staff = current_journal_flow_people(c, company).await?;
    if staff.len() > 500 {
        return Err(changed());
    }
    for person in staff {
        let canonical = current_journal_flow_principal(c, company, &person.user_id).await?;
        let team = journal_flow_team_name(c, company, person.department_id.as_deref()).await?;
        for (i, pool) in pools.iter_mut().enumerate() {
            let exclusions = if i == 0 {
                vec![creator.clone()]
            } else {
                excluded.clone()
            };
            if eligible_service_invoice_flow_participant(
                c,
                company,
                &person.user_id,
                &exclusions,
                i == 1,
            )
            .await?
                && !pool.iter().any(|p| p["userId"] == canonical)
            {
                pool.push(json!({"userId":canonical,"name":person.name,"teamId":person.department_id,"teamName":team}));
            }
        }
    }
    let mut selected: [Option<String>; 2] = [None, None];
    for (i, key) in ["draft", "posted"].iter().enumerate() {
        if let Some(user) = basis.selected(key)? {
            selected[i] = Some(current_journal_flow_principal(c, company, &user).await?);
        }
    }
    let roles = [
        settle
            && (current.has_permission("workflow.finance_approver")
                || current.has_permission("workflow.holding_owner")),
        settle && current.has_permission("workflow.treasury"),
    ];
    Ok(service_invoice_flow_view(
        record,
        basis.version()?,
        basis.revision(),
        basis.history(),
        pools,
        selected,
        current.user_id().as_str(),
        roles,
        settle && service_invoice_flow_administrator(c, current, clock.now().get()).await?,
    ))
}
pub async fn read_service_invoice_flow<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    record: &Value,
    settle: bool,
    clock: &C,
) -> Result<CurrentServiceInvoiceFlow, Error> {
    require_selected_actor_current(c, current, clock.now().get()).await?;
    let id = record["id"].as_str().ok_or_else(changed)?;
    let basis =
        read_service_invoice_flow_basis(c, &current.selection().workspace().company_id, id).await?;
    let result = view(c, current, record, &basis, settle, clock).await?;
    Ok(CurrentServiceInvoiceFlow {
        basis,
        view: result,
    })
}
impl CurrentServiceInvoiceFlow {
    pub fn plan_no_charge(
        self,
        input: erpx_server_workflow::public::service_invoice_flow::ServiceInvoiceNoChargeFlowInput<
            '_,
        >,
    ) -> Result<erpx_server_workflow::public::service_invoice_flow::ServiceInvoiceNoChargeFlow, Error>
    {
        self.basis.plan_no_charge(&self.view, input)
    }
    pub fn plan_assignment(
        self,
        input:erpx_server_workflow::public::service_invoice_flow::ServiceInvoiceFlowAssignmentInput<'_>,
    ) -> Result<
        erpx_server_workflow::public::service_invoice_flow::ServiceInvoiceFlowAssignment,
        Error,
    > {
        self.basis.plan_assignment(&self.view, input)
    }

    pub async fn require_current<C: Clock>(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        record: &Value,
        settle: bool,
        clock: &C,
    ) -> Result<(), Error> {
        self.basis.require_current(c).await?;
        if view(c, current, record, &self.basis, settle, clock).await? != self.view {
            return Err(changed());
        }
        require_selected_actor_current(c, current, clock.now().get()).await?;
        Ok(())
    }
}
