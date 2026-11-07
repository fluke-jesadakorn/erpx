//! Original Service first Count creation on the retained selected Holding unit.
//! The outer caller supplies trusted PMS witness and all utility locks before
//! invoking this function; this module does not expose a route or commit.
use crate::{
    service_initial_physical_stock_command::InitialCountCreate,
    service_initial_physical_stock_purchases::ServiceCountPurchases,
    service_initial_physical_stock_read::{InitialCountSelection, read_initial_count_selection},
    service_initial_physical_stock_sources::{
        ServiceInitialPhysicalStockEnvironment, require_count_modules,
    },
};
use erpx_server_commerce::public::{
    service_initial_count_masters::{
        CurrentServiceCountMapping, CurrentServiceCountMasters, read_service_count_mapping,
        read_service_count_masters,
    },
    service_stock_masters::{CurrentServiceStockMapping, read_service_stock_mapping},
};
use erpx_server_commercial::public::purchases::CapabilityPurpose;
use erpx_server_finance::public::initial_count_finance::{
    CurrentInitialCountReadiness, read_initial_count_readiness,
};
use erpx_server_identity_access::public::{
    command_metadata::project_command_iam,
    effective_authority::AuthorizedWorkContext,
    service_initial_count_authority::{
        CurrentServiceCountAuthority, ServiceCountAuthorityPurpose, capture_service_count_authority,
    },
    work_context::WorkScopeKind,
};
use erpx_server_inventory::public::service_initial_physical_count::{
    InitialCountDraft, InitialCountDraftEffects, InitialCountFinanceBasis,
    InitialCountProductSnapshot, InitialCountSelection as InventorySelection,
    InitialCountSourceProof, TrackingMode, insert_initial_count_draft,
    read_initial_count_inventory_history, read_initial_count_record, value_initial_count,
};
use erpx_server_quality::public::initial_count_history::{
    CurrentInitialCountQualityHistory, read_initial_count_quality_history,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, IdentifierSource};
use erpx_server_workflow::public::{
    commands::{
        CommandActor, CommandDisposition, CommandField, CommandJson, PgCommandAuthority,
        PgCommandLedger, PgOwnerCommandEffects, WorkAuthority, WorkflowOutput, WorkflowRequest,
    },
    service_count_setup_receipt::{
        CurrentServiceCountSetupReceipt, read_service_count_setup_history,
        read_service_count_setup_receipt,
    },
    service_initial_count_receipt::read_service_initial_count_receipt,
    service_parts_posting::{
        CurrentServiceStockPostingReceipt, read_service_stock_posting_receipt,
    },
};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The original Service first Count source changed.",
    )
}
pub(crate) fn actor(current: &AuthorizedWorkContext) -> Result<CommandActor, BusinessError> {
    let s = current.selection();
    let a = s.assignment();
    let w = s.workspace();
    Ok(CommandActor {
        user_id: current.user_id().clone(),
        assignment_id: a.id.clone(),
        role: a.role.clone(),
        company_id: w.company_id.clone(),
        branch_id: w
            .branch_id
            .as_ref()
            .map(|v| CommandField::Value(v.as_str().into()))
            .unwrap_or(CommandField::Null),
        department_id: CommandField::Null,
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
        assignment_scope_node_id: a.scope_node_id.as_ref().map(|v| v.as_str().into()),
    })
}
struct Authority<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    request: &'a WorkflowRequest,
    purchases: &'a ServiceCountPurchases,
    completion: Option<&'a CreateCompletion<'a, C>>,
}
impl<C: Clock + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        if !c.is_in_transaction() {
            return Err(PersistenceError::new("COUNT_UNIT_REQUIRED").into());
        }
        Ok(())
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        request: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if request.key != self.request.key
            || request.operation != self.request.operation
            || request.record_id != self.request.record_id
            || request.input != self.request.input
        {
            return Err(changed().into());
        }
        let current = capture_service_count_authority(
            c,
            self.current,
            ServiceCountAuthorityPurpose::Prepare,
            self.clock,
        )
        .await?;
        current.require_current(c, self.clock).await?;
        if let Some(completion) = self.completion
            && completion.plan.get().is_some()
        {
            completion.require_current(c).await?;
        }
        Ok(actor(self.current)?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        request: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        self.purchases
            .require_for(
                c,
                if disposition == CommandDisposition::Replay {
                    CapabilityPurpose::History
                } else {
                    CapabilityPurpose::NewWork
                },
                self.clock.now(),
            )
            .await?;
        if disposition == CommandDisposition::Execute
            && let Some(completion) = self.completion
            && completion.plan.get().is_some()
        {
            completion.require_current(c).await?;
        }
        if disposition == CommandDisposition::Replay {
            let original = read_initial_count_record(
                c,
                self.current.selection().workspace().company_id.as_str(),
                self.current
                    .selection()
                    .workspace()
                    .branch_id
                    .as_ref()
                    .map(|v| v.as_str()),
                &request.record_id,
            )
            .await?
            .ok_or_else(changed)?;
            let image: serde_json::Value =
                serde_json::from_str(&original.data().image).map_err(|_| changed())?;
            let source = &image["source"];
            let receipt = read_service_initial_count_receipt(
                c,
                &request.key,
                self.current.selection().workspace().company_id.as_str(),
                &request.record_id,
            )
            .await?;
            let r = receipt.facts().ok_or_else(changed)?;
            let proof: serde_json::Value =
                serde_json::from_str(&r.proof_json).map_err(|_| changed())?;
            if proof != source["proof_json"]
                || r.actor_id != image["count"]["created_by"].as_str().unwrap_or("")
            {
                return Err(changed().into());
            }
            let setup_key = source["service_setup_command_id"]
                .as_str()
                .ok_or_else(changed)?;
            let setup = read_service_count_setup_history(
                c,
                self.current.selection().workspace().company_id.as_str(),
                setup_key,
            )
            .await?;
            let fact = setup.facts().ok_or_else(changed)?;
            let mapping: serde_json::Value =
                serde_json::from_str(&fact.proof).map_err(|_| changed())?;
            if Some(fact.request_hash.as_str()) != proof["serviceSetupRequestHash"].as_str()
                || mapping["mapping"]["chart_id"] != proof["chartId"]
                || mapping["mapping"]["inventory_account_id"] != proof["inventoryAccountId"]
                || mapping["mapping"]["currency"] != proof["currency"]
            {
                return Err(changed().into());
            }
            original.require_current(c).await?;
            receipt.require_current(c).await?;
            setup.require_current(c).await?;
        }
        Ok(())
    }
}
pub(crate) struct ReplayOnly;
impl PgOwnerCommandEffects for ReplayOnly {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        if !c.is_in_transaction() {
            return Err(PersistenceError::new("COUNT_UNIT_REQUIRED").into());
        }
        Ok(())
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
struct CreateSource<'a, C> {
    clock: &'a C,
    authority: CurrentServiceCountAuthority<'a>,
    purchases: &'a ServiceCountPurchases,
    mapping: CurrentServiceStockMapping,
    ordered: CurrentServiceCountMapping,
    setup: CurrentServiceCountSetupReceipt,
    setup_origin: CurrentServiceStockPostingReceipt,
    masters: CurrentServiceCountMasters,
    finance: CurrentInitialCountReadiness,
    quality: CurrentInitialCountQualityHistory,
    draft: InitialCountDraft,
}
impl<C: Clock> CreateSource<'_, C> {
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.authority.require_current(c, self.clock).await?;
        self.purchases
            .require_for(c, CapabilityPurpose::NewWork, self.clock.now())
            .await?;
        self.mapping.require_current(c).await?;
        self.ordered.require_current(c).await?;
        self.setup.require_current(c).await?;
        self.setup_origin.require_current(c).await?;
        self.masters.require_current(c).await?;
        self.finance.require_current(c).await?;
        self.quality.require_current(c).await?;
        Ok(())
    }
}
struct Effects<'a, C> {
    completion: &'a CreateCompletion<'a, C>,
}
struct CreateCompletion<'a, C> {
    source: &'a CreateSource<'a, C>,
    plan: std::sync::OnceLock<InitialCountDraftEffects>,
    clock: erpx_server_workflow::public::service_count_command_evidence::CountCommandClock,
    generated:
        &'a erpx_server_workflow::public::service_count_command_evidence::CountCommandGeneratedIds,
    evidence: std::sync::OnceLock<
        erpx_server_workflow::public::service_count_command_evidence::PlannedCountCommandEvidence,
    >,
    retained: std::sync::OnceLock<
        erpx_server_workflow::public::service_count_command_evidence::RetainedCountCommandEvidence,
    >,
}
impl<C: Clock> CreateCompletion<'_, C> {
    async fn require_owner(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.plan
            .get()
            .ok_or_else(changed)?
            .require_current(c)
            .await?;
        self.source.require_current(c).await?;
        let s = &self.source.draft.proof.selection;
        let history = read_initial_count_inventory_history(
            c,
            &s.warehouse_id,
            &s.product_id,
            Some(&self.source.draft.proof.count_id),
        )
        .await?;
        if history.data().has_history {
            return Err(changed().into());
        }
        history.require_current(c).await?;
        let record = read_initial_count_record(
            c,
            &s.company_id,
            Some(&s.branch_id),
            &self.source.draft.proof.count_id,
        )
        .await?
        .ok_or_else(changed)?;
        record.require_current(c).await?;
        Ok(())
    }
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_owner(c).await?;
        self.retained
            .get()
            .ok_or_else(changed)?
            .require_current(c)
            .await
    }
}
impl<C: Clock + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        if !c.is_in_transaction() {
            return Err(PersistenceError::new("COUNT_UNIT_REQUIRED").into());
        }
        Ok(())
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        request: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        self.completion.source.require_current(c).await?;
        let p = &self.completion.source.draft.proof;
        let s = &p.selection;
        let result = serde_json::json!({"id":p.count_id,"status":"draft","version":1,"_serviceInitialPhysicalStockCount":{"kind":"service-initial-physical-stock-count","countId":p.count_id,"companyId":s.company_id,"warehouseId":s.warehouse_id,"branchId":s.branch_id,"productId":s.product_id,"countCommandId":p.count_command_id,"serviceSetupCommandId":p.service_setup_command_id,"serviceSetupRequestHash":p.service_setup_request_hash,"chartId":p.finance.chart_id,"inventoryAccountId":p.finance.inventory_account_id,"varianceAccountId":p.finance.variance_account_id,"currency":p.finance.currency}});
        let output = WorkflowOutput {
            version: 1,
            result: CommandJson::from_serializable(&result)?,
            event_scope: None,
        };
        let plan=erpx_server_workflow::public::service_count_command_evidence::plan_count_command_evidence(c,actor,request,&output,self.completion.clock).await?;
        self.completion.evidence.set(plan).map_err(|_| changed())?;
        Ok(output)
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.completion.source.require_current(c).await?;
        let s = &self.completion.source.draft.proof.selection;
        let history =
            read_initial_count_inventory_history(c, &s.warehouse_id, &s.product_id, None).await?;
        if history.data().has_history {
            return Err(changed().into());
        }
        let plan = insert_initial_count_draft(c, &self.completion.source.draft, &history).await?;
        self.completion.plan.set(plan).map_err(|_| changed())?;
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
        self.completion.require_owner(c).await?;
        let retained = self
            .completion
            .evidence
            .get()
            .ok_or_else(changed)?
            .retain(c, self.completion.generated)
            .await?;
        self.completion
            .retained
            .set(retained)
            .map_err(|_| changed())?;
        self.completion.require_current(c).await
    }
}
/// Must be invoked under trusted module availability and source preparation.
/// Replay is resolved before any new source/effect/Files preparation. Caller
/// must repeat and retain the PMS witness after this function and before commit.
pub async fn create_service_initial_count<C: Clock + Sync, I: IdentifierSource>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    ids: &mut I,
    key: &str,
    input: &InitialCountCreate,
    environment: &ServiceInitialPhysicalStockEnvironment,
) -> Result<serde_json::Value, Error> {
    let checked = crate::service_initial_physical_stock_command::normalize_initial_count_create(
        input.normalized.as_str(),
    )?;
    if checked.context_key != input.context_key
        || checked.warehouse_id != input.warehouse_id
        || checked.product_id != input.product_id
        || checked.count_date != input.count_date
        || checked.reason != input.reason
        || checked.quantity_milli != input.quantity_milli
        || checked.unit_cost_minor != input.unit_cost_minor
        || checked.lots != input.lots
        || checked.normalized != input.normalized
    {
        return Err(changed().into());
    }
    let module = require_count_modules(c, current, environment).await?;
    let request = WorkflowRequest {
        key: key.into(),
        operation: "inventory.count.create".into(),
        record_id: format!("count-{key}"),
        input: input.normalized.clone(),
    };
    let purchases = ServiceCountPurchases::capture(c, current, clock.now()).await?;
    let mut auth = Authority {
        current,
        clock,
        request: &request,
        purchases: &purchases,
        completion: None,
    };
    let mut ledger = PgCommandLedger::new(
        current.selection().workspace().company_id.clone(),
        c,
        &mut auth,
        clock,
        ids,
    );
    if let Some(outcome) = ledger.replay(&request, &mut ReplayOnly).await? {
        let mut result: serde_json::Value =
            serde_json::from_str(outcome.result.as_str()).map_err(|_| changed())?;
        result
            .as_object_mut()
            .ok_or_else(changed)?
            .remove("_serviceInitialPhysicalStockCount");
        module.require_current(c).await?;
        return Ok(result);
    }
    let selected = read_initial_count_selection(
        c,
        current,
        clock,
        InitialCountSelection {
            warehouse_id: Some(input.warehouse_id.clone()),
            product_id: Some(input.product_id.clone()),
            count_date: Some(input.count_date.clone()),
        },
        environment,
    )
    .await?;
    if !selected.can_prepare || selected.context_key.as_deref() != Some(input.context_key.as_str())
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SERVICE_INITIAL_PHYSICAL_STOCK_CONTEXT_CHANGED",
            "The selected Count source changed.",
        )
        .into());
    }
    let company = &current.selection().workspace().company_id;
    let authority =
        capture_service_count_authority(c, current, ServiceCountAuthorityPurpose::Prepare, clock)
            .await?;
    purchases
        .require_for(c, CapabilityPurpose::NewWork, clock.now())
        .await?;
    let mapping = read_service_stock_mapping(c, company).await?;
    let mapping_text = mapping.mapping_json().ok_or_else(changed)?;
    let m: serde_json::Value = serde_json::from_str(mapping_text).map_err(|_| changed())?;
    let chart = m["chart_id"].as_str().ok_or_else(changed)?;
    let inv = m["inventory_account_id"].as_str().ok_or_else(changed)?;
    let ordered = read_service_count_mapping(c, company.as_str()).await?;
    let setup_origin = read_service_stock_posting_receipt(c, company, mapping_text).await?;
    let setup_id = setup_origin.facts().ok_or_else(changed)?.command_id.clone();
    let setup =
        read_service_count_setup_receipt(c, company.as_str(), &setup_id, mapping_text).await?;
    let setup_hash = setup.facts().ok_or_else(changed)?.request_hash.clone();
    let masters = read_service_count_masters(
        c,
        company.as_str(),
        current
            .selection()
            .workspace()
            .branch_id
            .as_ref()
            .map(|b| b.as_str()),
        Some(&input.warehouse_id),
        Some(&input.product_id),
    )
    .await?;
    let warehouse: serde_json::Value =
        serde_json::from_str(masters.data().warehouse.as_deref().ok_or_else(changed)?)
            .map_err(|_| changed())?;
    let product: serde_json::Value =
        serde_json::from_str(masters.data().product.as_deref().ok_or_else(changed)?)
            .map_err(|_| changed())?;
    let tracking = match product["tracking_mode"].as_str() {
        Some("none") => TrackingMode::None,
        Some("lot") => TrackingMode::Lot,
        Some("serial") => TrackingMode::Serial,
        _ => return Err(changed().into()),
    };
    let value = value_initial_count(
        tracking,
        input.quantity_milli,
        input.unit_cost_minor,
        input.lots.clone(),
    )?;
    let finance = read_initial_count_readiness(
        c,
        current.holding_id(),
        company,
        Some(chart),
        Some(inv),
        &input.count_date,
    )
    .await?;
    let basis = finance.basis().ok_or_else(changed)?;
    if finance.period_id().is_none() {
        return Err(changed().into());
    }
    let quality =
        read_initial_count_quality_history(c, &input.warehouse_id, &input.product_id).await?;
    if quality.has_history() {
        return Err(changed().into());
    }
    let draft = InitialCountDraft {
        proof: InitialCountSourceProof {
            count_id: request.record_id.clone(),
            selection: InventorySelection {
                company_id: company.as_str().into(),
                branch_id: warehouse["branch_id"].as_str().ok_or_else(changed)?.into(),
                warehouse_id: input.warehouse_id.clone(),
                product_id: input.product_id.clone(),
                count_date: input.count_date.clone(),
            },
            count_command_id: key.into(),
            service_setup_command_id: setup_id,
            service_setup_request_hash: setup_hash,
            finance: InitialCountFinanceBasis {
                chart_id: basis.chart_id.clone(),
                inventory_account_id: basis.inventory_account_id.clone(),
                variance_account_id: basis.variance_account_id.clone(),
                currency: basis.currency.clone(),
            },
        },
        product: InitialCountProductSnapshot {
            version: product["version"].as_i64().ok_or_else(changed)?,
            description: product["name"].as_str().ok_or_else(changed)?.into(),
            unit: product["unit"].as_str().ok_or_else(changed)?.into(),
            tracking,
        },
        reason: input.reason.clone(),
        actor_id: current.user_id().as_str().into(),
        value,
        created_at: clock.now().get(),
    };
    let source = CreateSource {
        clock,
        authority,
        purchases: &purchases,
        mapping,
        ordered,
        setup,
        setup_origin,
        masters,
        finance,
        quality,
        draft,
    };
    let ledger_clock =
        erpx_server_workflow::public::service_count_command_evidence::CountCommandClock::sample(
            clock,
        )?;
    let generated=erpx_server_workflow::public::service_count_command_evidence::CountCommandGeneratedIds::default();
    let completion = CreateCompletion {
        source: &source,
        plan: std::sync::OnceLock::new(),
        clock: ledger_clock,
        generated: &generated,
        evidence: std::sync::OnceLock::new(),
        retained: std::sync::OnceLock::new(),
    };
    let mut effects = Effects {
        completion: &completion,
    };
    let mut auth = Authority {
        current,
        clock,
        request: &request,
        purchases: &purchases,
        completion: Some(&completion),
    };
    let mut ledger_ids = generated.wrap(ids);
    let outcome = PgCommandLedger::new(
        company.clone(),
        c,
        &mut auth,
        &ledger_clock,
        &mut ledger_ids,
    )
    .execute(&request, &mut effects)
    .await?;
    let mut result: serde_json::Value =
        serde_json::from_str(outcome.result.as_str()).map_err(|_| changed())?;
    result
        .as_object_mut()
        .ok_or_else(changed)?
        .remove("_serviceInitialPhysicalStockCount");
    module.require_current(c).await?;
    Ok(result)
}
