//! Native Finance due-draft schedule/decide commands on the selected Holding unit.
//!
//! Minimal composition backend over the landed
//! `erpx-server-finance` port
//! (`modules/finance/server/src/adapters/due_draft_schedule.rs`): the
//! authorizer supplies the holding-scoped actor, this module enforces the
//! retained purchase boundary facts from the TypeScript oracle
//! (`modules/finance/server/src/adapters/finance-due-drafts.ts`) and calls
//! `schedule_due_draft` / `decide_due_draft` on the caller transaction. No
//! HTTP mounting lives here.
use erpx_commercial_server::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, lock_commercial_authority_sources,
};
use erpx_finance_server::public::due_draft_schedule::{
    DueDraftCommandActor, decide_due_draft, lock_due_draft_schedule_sources, schedule_due_draft,
};
use erpx_identity_access_server::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    selected_command_authority::require_selected_actor_current,
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_platform_runtime_server::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_platform_shared_server::public::{BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId};
use erpx_workflow_server::public::commands::CommandJson;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn holding_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FINANCE_SCHEDULE_HOLDING_REQUIRED",
        "Schedule Company-wide Finance drafts with a Holding Finance assignment.",
    )
}

fn context_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your work role changed. Reload and review before submitting.",
    )
}

/// Company-scoped Finance due-draft configuration. The holding, company and
/// purchase catalogue mirror the sibling Finance write targets; there is no
/// production-release gate for due drafts.
#[derive(Clone)]
pub struct FinanceDueDraftConfiguration {
    pub(crate) holding: HoldingId,
    pub(crate) company: CompanyId,
    pub(crate) catalogue: Arc<CommercialCatalogue>,
}

impl FinanceDueDraftConfiguration {
    pub fn new(
        holding: HoldingId,
        company: CompanyId,
        catalogue: Arc<CommercialCatalogue>,
    ) -> Result<Self, BusinessError> {
        Ok(Self {
            holding,
            company,
            catalogue,
        })
    }
}

impl HoldingWorkSourcePreparation for FinanceDueDraftConfiguration {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_due_draft_schedule_sources(c).await
        })
    }
}

pub(crate) fn due_draft_actor(current: &AuthorizedWorkContext) -> DueDraftCommandActor {
    let assignment = current.selection().assignment();
    DueDraftCommandActor {
        user_id: current.user_id().as_str().to_owned(),
        assignment_id: assignment.id.as_str().to_owned(),
        role: assignment.role.clone(),
        company_id: current
            .selection()
            .workspace()
            .company_id
            .as_str()
            .to_owned(),
        holding_id: current.holding_id().as_str().to_owned(),
        scope_kind: match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .to_owned(),
    }
}

fn require_holding_scope(current: &AuthorizedWorkContext) -> Result<(), BusinessError> {
    let workspace = current.selection().workspace();
    if workspace.branch_id.is_some() || workspace.department_id.is_some() {
        return Err(holding_required());
    }
    if !matches!(
        current.selection().assignment().scope_kind,
        WorkScopeKind::Holding
    ) {
        return Err(holding_required());
    }
    Ok(())
}

async fn has_receipt(c: &mut PgConnection, key: &str, operation: &str) -> Result<bool, Error> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM command_receipts WHERE command_id=$1 AND operation=$2)",
    )
    .bind(key)
    .bind(operation)
    .fetch_one(&mut *c)
    .await?)
}

async fn inspect_allowed<C: Clock>(
    c: &mut PgConnection,
    config: &FinanceDueDraftConfiguration,
    clock: &C,
    capability: &str,
    purpose: CapabilityPurpose,
) -> Result<PurchaseWitness, Error> {
    let mut reader = PgCompanyPurchaseReader::new(
        config.holding.clone(),
        config.company.clone(),
        c,
        &config.catalogue,
    );
    let inspection = reader
        .inspect(
            &PurchaseRequest {
                capabilities: vec![capability.into()],
                purpose,
                mode: PackageEnforcementMode::Enforced,
            },
            clock.now(),
        )
        .await?;
    Ok(inspection.require_allowed()?.clone())
}

async fn revalidate_allowed<C: Clock>(
    c: &mut PgConnection,
    config: &FinanceDueDraftConfiguration,
    clock: &C,
    witness: &PurchaseWitness,
) -> Result<(), Error> {
    let mut reader = PgCompanyPurchaseReader::new(
        config.holding.clone(),
        config.company.clone(),
        c,
        &config.catalogue,
    );
    reader.revalidate(witness, clock.now()).await?.require_allowed()?;
    Ok(())
}

async fn draft_kind(c: &mut PgConnection, company: &CompanyId, id: &str) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar("SELECT kind FROM finance_due_drafts WHERE id=$1 AND company_id=$2")
        .bind(id)
        .bind(company.as_str())
        .fetch_optional(&mut *c)
        .await?)
}

/// Schedule one Finance due draft. `prepare` supplies the idempotency key and
/// the retained wire fields; the capability follows the TS oracle (only
/// `instrument_payment` settles under `finance.advanced`).
pub async fn execute_due_draft_schedule<C, F, P>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    selection: F,
    config: FinanceDueDraftConfiguration,
    clock: Arc<C>,
    prepare: P,
) -> Result<CommandJson, Error>
where
    C: Clock + Send + Sync + 'static,
    F: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    P: Fn() -> Result<(CommandKey, String, String, String, String, String), BusinessError>
        + Clone
        + Send
        + Sync
        + 'static,
{
    let company = config.company.clone();
    authorizer
        .execute_with_sources_deferred(
            host,
            proof,
            move || {
                let mut scope = selection()?;
                if scope.company_id != company {
                    return Err(context_changed());
                }
                scope.mutation = true;
                Ok(scope)
            },
            WorkAuthorizationRequest {
                permission: "finance.write".into(),
                module: Some("finance".into()),
                destination: "finance/due-drafts".into(),
                api: true,
            },
            config.clone(),
            move |c, current| {
                let config = config.clone();
                let clock = clock.clone();
                let prepare = prepare.clone();
                Box::pin(async move {
                    require_selected_actor_current(c, current, clock.now().get()).await?;
                    current.require_permission("finance.read")?;
                    current.require_permission("finance.write")?;
                    current.require_permission("module.finance.read")?;
                    require_holding_scope(current)?;
                    let (key, kind, source_id, run_on, payload_json, reason) = prepare()?;
                    let capability = if kind == "instrument_payment" {
                        "finance.advanced"
                    } else {
                        "finance.accounting"
                    };
                    let purpose = if has_receipt(c, key.as_str(), "finance.due-draft.schedule").await? {
                        CapabilityPurpose::History
                    } else {
                        CapabilityPurpose::SettleExisting
                    };
                    let witness =
                        inspect_allowed(c, &config, clock.as_ref(), capability, purpose).await?;
                    let actor = due_draft_actor(current);
                    let result = schedule_due_draft(
                        c,
                        &actor,
                        &key,
                        &kind,
                        &source_id,
                        &run_on,
                        &payload_json,
                        &reason,
                        clock.now().get(),
                    )
                    .await?;
                    revalidate_allowed(c, &config, clock.as_ref(), &witness).await?;
                    Ok(CommandJson::from_serializable(&result)?)
                })
            },
        )
        .await
}

/// Decide a pending Finance due draft (`retry` reopens, `cancel` closes).
/// The base capability is always `finance.accounting`; an
/// `instrument_payment` draft additionally settles under `finance.advanced`,
/// mirroring the TS `companyPurchases` fact. Cancel purposes are `history`.
pub async fn execute_due_draft_decide<C, F, P>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    selection: F,
    config: FinanceDueDraftConfiguration,
    clock: Arc<C>,
    prepare: P,
) -> Result<CommandJson, Error>
where
    C: Clock + Send + Sync + 'static,
    F: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    P: Fn() -> Result<(CommandKey, String, String, i64, String), BusinessError>
        + Clone
        + Send
        + Sync
        + 'static,
{
    let company = config.company.clone();
    authorizer
        .execute_with_sources_deferred(
            host,
            proof,
            move || {
                let mut scope = selection()?;
                if scope.company_id != company {
                    return Err(context_changed());
                }
                scope.mutation = true;
                Ok(scope)
            },
            WorkAuthorizationRequest {
                permission: "finance.write".into(),
                module: Some("finance".into()),
                destination: "finance/due-drafts".into(),
                api: true,
            },
            config.clone(),
            move |c, current| {
                let config = config.clone();
                let clock = clock.clone();
                let prepare = prepare.clone();
                Box::pin(async move {
                    require_selected_actor_current(c, current, clock.now().get()).await?;
                    current.require_permission("finance.read")?;
                    current.require_permission("finance.write")?;
                    current.require_permission("module.finance.read")?;
                    require_holding_scope(current)?;
                    let (key, id, action, expected_version, reason) = prepare()?;
                    if action != "retry" && action != "cancel" {
                        return Err(BusinessError::invalid(
                            "FINANCE_SCHEDULE_INPUT",
                            "Choose a draft decision.",
                        )
                        .into());
                    }
                    let operation = if action == "retry" {
                        "finance.due-draft.retry"
                    } else {
                        "finance.due-draft.cancel"
                    };
                    let purpose = if action == "cancel" {
                        CapabilityPurpose::History
                    } else if has_receipt(c, key.as_str(), operation).await? {
                        CapabilityPurpose::History
                    } else {
                        CapabilityPurpose::SettleExisting
                    };
                    let base =
                        inspect_allowed(c, &config, clock.as_ref(), "finance.accounting", purpose)
                            .await?;
                    let kind = draft_kind(c, &config.company, &id).await?;
                    let extra = if kind.as_deref() == Some("instrument_payment") {
                        Some(
                            inspect_allowed(c, &config, clock.as_ref(), "finance.advanced", purpose)
                                .await?,
                        )
                    } else {
                        None
                    };
                    let actor = due_draft_actor(current);
                    let result = decide_due_draft(
                        c,
                        &actor,
                        &key,
                        &id,
                        &action,
                        expected_version,
                        &reason,
                        clock.now().get(),
                    )
                    .await?;
                    revalidate_allowed(c, &config, clock.as_ref(), &base).await?;
                    if let Some(witness) = &extra {
                        revalidate_allowed(c, &config, clock.as_ref(), witness).await?;
                    }
                    Ok(CommandJson::from_serializable(&result)?)
                })
            },
        )
        .await
}
