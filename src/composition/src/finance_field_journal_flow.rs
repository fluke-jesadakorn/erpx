//! Field-cost journal responsibility read/plan on the original native owner unit.
//! No HTTP source guard and no foreign owner SQL.
use crate::{
    finance_field_service_lineage::FieldServicePartsSource,
    finance_journal_create::FinanceJournalCreateConfiguration,
    finance_journal_review::finance_actor,
    finance_presentation::{CurrentFinancePresentation, read_finance_presentation},
};
use erpx_server_finance::public::{journal_detail::*, journal_review::*, journal_workspace::*};
use erpx_server_identity_access::public::{
    effective_authority::AuthorizedWorkContext,
    journal_flow_participants::{
        current_journal_flow_principal, eligible_service_invoice_flow_participant,
        service_invoice_flow_administrator,
    },
    selected_command_authority::require_selected_actor_current,
};
use erpx_server_organization::public::journal_flow_department::journal_flow_team_name;
use erpx_server_people_payroll::public::finance_journal_names::{
    CurrentFinanceJournalNames, read_finance_journal_names,
};
use erpx_server_people_payroll::public::journal_flow_people::current_journal_flow_people;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind, RecordId, UserId};
use erpx_server_workflow::public::field_journal_flow::*;
use serde_json::{Value, json};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "FLOW_SOURCE_CHANGED",
        "The current Field journal workflow changed. Reload its source.",
    )
}
/// Private native transport DTOs. They carry rendered facts, never authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum FinanceFieldJournalFlowRuntime {
    Rust,
    Legacy,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceFieldJournalFlowResponse {
    pub runtime: FinanceFieldJournalFlowRuntime,
    #[cfg_attr(feature = "contracts", ts(type = "Record<string, unknown> | null"))]
    pub data: Option<Value>,
    pub workspace: Option<FinanceJournalRecordWorkspace>,
    pub access: Option<FinanceFieldJournalFlowAccess>,
    pub context: crate::finance_journal_detail::FinanceRenderedWorkContext,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceFieldJournalFlowAccess {
    pub source_only: bool,
    pub can_post: bool,
    pub purchase_controls:
        std::collections::BTreeMap<String, crate::finance_presentation::FinancePurchaseAccess>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceFieldJournalFlowInput {
    pub step_key: String,
    pub user_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub expected_version: i64,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub record_version: i64,
    pub reason: String,
    #[serde(default)]
    pub save_default: bool,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceFieldJournalFlowResult {
    pub id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub version: i64,
}
#[derive(serde::Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceFieldJournalFlowCommandResponse {
    pub data: FinanceFieldJournalFlowResult,
}
pub struct CurrentFieldJournalFlow {
    source: FieldServicePartsSource,
    workspace: CurrentFieldServiceJournalWorkspace,
    names: CurrentFinanceJournalNames,
    creator: UserId,
    poster: Option<UserId>,
    purchase: CurrentFinancePresentation,
    basis: FieldJournalFlowBasis,
    record: Value,
    pub view: Value,
}
async fn presentation(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    record: &Value,
    basis: &FieldJournalFlowBasis,
    settle: bool,
    now: i64,
) -> Result<Value, Error> {
    let company = &current.selection().workspace().company_id;
    let creator = record["created_by"].as_str().ok_or_else(changed)?;
    let staff = current_journal_flow_people(c, company).await?;
    if staff.len() > 500 {
        return Err(changed().into());
    }
    let mut candidates = Vec::new();
    for person in staff {
        let canonical = current_journal_flow_principal(c, company, &person.user_id).await?;
        if eligible_service_invoice_flow_participant(
            c,
            company,
            &person.user_id,
            &[creator.to_owned()],
            false,
        )
        .await?
            && !candidates.iter().any(|p: &Value| p["userId"] == canonical)
        {
            let team = journal_flow_team_name(c, company, person.department_id.as_deref()).await?;
            candidates.push(json!({"userId":canonical,"name":person.name,"teamId":person.department_id,"teamName":team}));
        }
    }
    let selected = match basis.selected("draft")? {
        Some(user) => Some(current_journal_flow_principal(c, company, &user).await?),
        None => None,
    };
    let role = settle
        && (current.has_permission("workflow.finance_approver")
            || current.has_permission("workflow.holding_owner"));
    let manage = settle && service_invoice_flow_administrator(c, current, now).await?;
    Ok(field_journal_flow_view(
        record,
        basis.version()?,
        basis.revision(),
        basis.history(),
        candidates,
        selected,
        current.user_id().as_str(),
        role,
        manage,
    ))
}
/// The legacy partition is selected from owner-persisted facts, never a client
/// source label. Any surviving native Inventory reference prevents downgrade.
pub async fn field_journal_flow_is_native(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &RecordId,
) -> Result<bool, Error> {
    let actor = finance_actor(current);
    let native = field_service_parts_journal_label(c, &actor, id).await?;
    let consumed=erpx_server_inventory::public::field_service_consumption_finance::journal_has_field_service_consumption(c,&actor.company_id,id.as_str()).await?;
    if !native && consumed {
        return Err(changed().into());
    }
    Ok(native)
}
pub async fn read_field_journal_flow<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &RecordId,
    config: &FinanceJournalCreateConfiguration,
    clock: &C,
) -> Result<CurrentFieldJournalFlow, Error> {
    require_selected_actor_current(c, current, clock.now().get()).await?;
    if current.holding_id() != &config.holding
        || current.selection().workspace().company_id != config.company
    {
        return Err(changed().into());
    }
    current.require_permission("finance.read")?;
    current.require_permission("module.finance.read")?;
    let actor = finance_actor(current);
    // This finite inspection binds the persisted Field parts lineage. The
    // synthetic review command is never executed and grants no post authority.
    let inspect = JournalReviewCommand {
        key: CommandKey::new("field-journal-flow-source")?,
        journal_id: id.clone(),
        input: JournalReviewInput::Post {
            expected_version: 1,
        },
    };
    let target = inspect_field_service_parts_journal_target(c, &actor, &inspect)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::Unavailable,
                "NATIVE_FIELD_JOURNAL_SOURCE_REQUIRED",
                "This workflow requires an actual native Field parts journal.",
            )
        })?;
    let workspace = read_field_service_journal_workspace(c, &actor, id, &target).await?;
    let user = |key: &str| -> Result<Option<UserId>, BusinessError> {
        match workspace.data().journal.get(key) {
            Some(FinanceLedgerValue::Text(id)) => Ok(Some(UserId::new(id)?)),
            Some(FinanceLedgerValue::Null) => Ok(None),
            _ => Err(changed()),
        }
    };
    let creator = user("created_by")?.ok_or_else(changed)?;
    let poster = user("posted_by")?;
    let mut people = vec![creator.clone()];
    if let Some(user) = &poster {
        people.push(user.clone())
    }
    let names = read_finance_journal_names(c, &config.company, &people).await?;
    let source = FieldServicePartsSource::retain(c, target).await?;
    let purchase = read_finance_presentation(
        c,
        &config.holding,
        &config.company,
        &config.catalogue,
        clock,
    )
    .await?;
    if !purchase.values()["finance.accounting"].history
        && !purchase.values()["finance.service-invoice"].history
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "FINANCE_SOURCE_PURCHASE_REQUIRED",
            "The original Finance source history is unavailable.",
        )
        .into());
    }
    let record = serde_json::to_value(&workspace.data().journal).map_err(|_| changed())?;
    let basis = read_field_journal_flow_basis(c, &config.company, id.as_str()).await?;
    let view = presentation(
        c,
        current,
        &record,
        &basis,
        purchase.can_post_invoice(),
        clock.now().get(),
    )
    .await?;
    Ok(CurrentFieldJournalFlow {
        source,
        workspace,
        names,
        creator,
        poster,
        purchase,
        basis,
        record,
        view,
    })
}
impl CurrentFieldJournalFlow {
    pub fn workspace(&self) -> Result<FinanceJournalRecordWorkspace, BusinessError> {
        let mut data = self.workspace.data().clone();
        data.journal.insert(
            "created_by_name".into(),
            FinanceLedgerValue::Text(self.names.name(&self.creator)?.to_owned()),
        );
        data.journal.insert(
            "posted_by_name".into(),
            FinanceLedgerValue::Text(
                match &self.poster {
                    Some(user) => self.names.name(user)?,
                    None => "Unlinked account",
                }
                .to_owned(),
            ),
        );
        Ok(data)
    }
    pub fn access(&self, current: &AuthorizedWorkContext) -> FinanceFieldJournalFlowAccess {
        FinanceFieldJournalFlowAccess {
            source_only: !self.purchase.values()["finance.accounting"].history,
            can_post: self.view["canAct"] == true
                && current.has_permission("finance.write")
                && current.has_permission("finance.approve"),
            purchase_controls: self.purchase.values().clone(),
        }
    }

    pub async fn require_settle_existing<C: Clock>(
        &self,
        c: &mut PgConnection,
        config: &FinanceJournalCreateConfiguration,
        clock: &C,
    ) -> Result<(), Error> {
        use erpx_server_commercial::public::purchases::*;
        let mut reader = PgCompanyPurchaseReader::new(
            config.holding.clone(),
            config.company.clone(),
            c,
            &config.catalogue,
        );
        let mut input = PurchaseRequest {
            capabilities: vec!["finance.accounting".into()],
            purpose: CapabilityPurpose::SettleExisting,
            mode: PackageEnforcementMode::Enforced,
        };
        let accounting = reader.inspect(&input, clock.now()).await?;
        if let Err(original) = accounting.require_allowed() {
            input.capabilities = vec!["finance.service-invoice".into()];
            reader
                .inspect(&input, clock.now())
                .await?
                .require_allowed()
                .map_err(|_| original)?;
        }
        Ok(())
    }

    pub fn plan_assignment(
        &self,
        input: FieldJournalFlowAssignmentInput<'_>,
    ) -> Result<FieldJournalFlowAssignment, Error> {
        self.basis.clone().plan_assignment(&self.view, input)
    }
    pub async fn require_source_current<C: Clock>(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        config: &FinanceJournalCreateConfiguration,
        clock: &C,
    ) -> Result<(), Error> {
        if current.holding_id() != &config.holding
            || current.selection().workspace().company_id != config.company
        {
            return Err(changed().into());
        }
        self.source.require_current(c).await?;
        self.workspace.require_current(c).await?;
        self.names.require_current(c).await?;
        self.purchase
            .require_current(
                c,
                &config.holding,
                &config.company,
                &config.catalogue,
                clock,
            )
            .await?;
        require_selected_actor_current(c, current, clock.now().get()).await?;
        Ok(())
    }
    pub async fn require_current<C: Clock>(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        config: &FinanceJournalCreateConfiguration,
        clock: &C,
    ) -> Result<(), Error> {
        self.require_source_current(c, current, config, clock)
            .await?;
        self.basis.require_current(c).await?;
        if presentation(
            c,
            current,
            &self.record,
            &self.basis,
            self.purchase.can_post_invoice(),
            clock.now().get(),
        )
        .await?
            != self.view
        {
            return Err(changed().into());
        }
        require_selected_actor_current(c, current, clock.now().get()).await?;
        Ok(())
    }
}
