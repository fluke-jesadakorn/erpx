//! Pre-action proof for the Service initial Count. Persistence is intentionally
//! unavailable until Flow, Files, Workflow and Finance owner effects are joined.
use crate::{
    service_initial_physical_stock_command::InitialCountActionInput,
    service_initial_physical_stock_record::{ServiceCountRecordSource, read_service_count_record},
};
use erpx_server_commercial::public::purchases::CapabilityPurpose;
use erpx_server_identity_access::public::{
    effective_authority::AuthorizedWorkContext,
    service_initial_count_authority::ServiceCountAuthorityPurpose,
};
use erpx_server_inventory::public::{
    service_initial_count_decision::{
        CurrentInitialCountDecisionDetail, read_initial_count_decision_detail,
    },
    service_initial_physical_count::{
        CountAction, CountAuthority, CountStatus, TrackingMode, transition_count,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, UserId};
use erpx_server_workflow::public::service_count_decision_evidence::service_count_decision_digest;
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The original Count source changed.",
    )
}
fn text<'a>(v: &'a serde_json::Value, key: &str) -> Result<&'a str, BusinessError> {
    v[key].as_str().ok_or_else(changed)
}
pub struct PreparedServiceCountAction<'a> {
    pub source: ServiceCountRecordSource<'a>,
    pub detail: Option<CurrentInitialCountDecisionDetail>,
    pub decision_digest: Option<String>,
    pub tracking: TrackingMode,
    pub next: CountStatus,
    pub action: CountAction,
    pub posting: Option<CurrentCountPostingSources>,
    pub active_masters: Option<
        erpx_server_commerce::public::service_initial_count_masters::CurrentServiceCountMasters,
    >,
}
pub struct CurrentCountPostingSources {
    pub mapping: erpx_server_commerce::public::service_stock_masters::CurrentServiceStockMapping,
    pub finance: erpx_server_finance::public::initial_count_finance::CurrentInitialCountFinance,
}
impl CurrentCountPostingSources {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.mapping.require_current(c).await?;
        self.finance.require_current(c).await?;
        Ok(())
    }
}
/// No effects, receipt, or authority are created by this preflight. Caller
/// resolves replay first and retains all source witnesses in the same unit.
pub async fn prepare_service_count_action<'a, C: Clock>(
    c: &mut PgConnection,
    current: &'a AuthorizedWorkContext,
    clock: &C,
    id: &str,
    action: CountAction,
    input: &InitialCountActionInput,
) -> Result<PreparedServiceCountAction<'a>, Error> {
    let review = matches!(
        action,
        CountAction::Approve | CountAction::Post | CountAction::Reject
    );
    let source = read_service_count_record(
        c,
        current,
        clock,
        id,
        if review {
            ServiceCountAuthorityPurpose::Review
        } else {
            ServiceCountAuthorityPurpose::Prepare
        },
    )
    .await?;
    source
        .purchases
        .require_for(
            c,
            if action == CountAction::Submit {
                CapabilityPurpose::NewWork
            } else {
                CapabilityPurpose::SettleExisting
            },
            clock.now(),
        )
        .await?;
    let count = &source.image["count"];
    let status = match text(count, "status")? {
        "draft" => CountStatus::Draft,
        "submitted" => CountStatus::Submitted,
        "approved" => CountStatus::Approved,
        "rejected" => CountStatus::Rejected,
        "cancelled" => CountStatus::Cancelled,
        "posted" => CountStatus::Posted,
        _ => return Err(changed().into()),
    };
    let creator = UserId::new(text(count, "created_by")?)?;
    let preparer = UserId::new(text(count, "prepared_by")?)?;
    let workspace = current.selection().workspace();
    let actor = CountAuthority {
        same_as_creator: source.authority.same_person(&creator),
        same_as_preparer: source.authority.same_person(&preparer),
        company_scope: workspace.branch_id.is_none(),
        department_scope: workspace.department_id.is_some(),
        can_prepare: current.has_permission("inventory.prepare"),
        can_approve: current.has_permission("inventory.approve"),
    };
    let version = count["version"].as_i64().ok_or_else(changed)?;
    let next = transition_count(
        status,
        version,
        input.expected_version,
        action,
        &actor,
        input.reason.as_deref(),
        input.attachment_set_ids.len(),
    )?;
    if action != CountAction::Cancel && !source.flow.view.can_act {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "FLOW_PRIMARY_REQUIRED",
            "The current Count Flow primary must perform this action.",
        )
        .into());
    }
    let (_, product) = source.masters.data().ok_or_else(changed)?;
    let product: serde_json::Value = serde_json::from_str(product).map_err(|_| changed())?;
    let tracking = match text(&product, "tracking_mode")? {
        "none" => TrackingMode::None,
        "lot" => TrackingMode::Lot,
        "serial" => TrackingMode::Serial,
        _ => return Err(changed().into()),
    };
    let detail = if review {
        Some(
            read_initial_count_decision_detail(
                c,
                id,
                text(count, "warehouse_id")?,
                text(&product, "sku")?,
                text(&product, "tracking_mode")?,
            )
            .await?,
        )
    } else {
        None
    };
    let decision_digest = detail
        .as_ref()
        .map(|d| {
            service_count_decision_digest(
                input.reason.as_deref().ok_or_else(changed)?,
                &d.detail().lines,
                &d.detail().lot_lines,
            )
        })
        .transpose()?;
    let active_masters = if matches!(
        action,
        CountAction::Submit | CountAction::Approve | CountAction::Post
    ) {
        let line = source.image["lines"]
            .as_array()
            .and_then(|v| v.first())
            .ok_or_else(changed)?;
        let masters=erpx_server_commerce::public::service_initial_count_masters::read_service_count_masters(c,workspace.company_id.as_str(),workspace.branch_id.as_ref().map(|v|v.as_str()),Some(text(count,"warehouse_id")?),Some(text(line,"product_id")?)).await?;
        let active_product: serde_json::Value =
            serde_json::from_str(masters.data().product.as_deref().ok_or_else(changed)?)
                .map_err(|_| changed())?;
        if masters.data().warehouse.is_none()
            || active_product["version"] != line["product_version"]
        {
            return Err(changed().into());
        }
        Some(masters)
    } else {
        None
    };
    let posting = if action == CountAction::Post {
        use erpx_server_commerce::public::service_stock_masters::read_service_stock_mapping;
        use erpx_server_finance::public::initial_count_finance::{
            InitialCountFinanceBasis, read_initial_count_finance,
        };
        let proof = &source.image["source"]["proof_json"];
        let mapping = read_service_stock_mapping(c, &workspace.company_id).await?;
        let current_mapping: serde_json::Value =
            serde_json::from_str(mapping.mapping_json().ok_or_else(changed)?)
                .map_err(|_| changed())?;
        if current_mapping["chart_id"] != proof["chartId"]
            || current_mapping["inventory_account_id"] != proof["inventoryAccountId"]
            || current_mapping["currency"] != proof["currency"]
        {
            return Err(changed().into());
        }
        let basis = InitialCountFinanceBasis {
            company_id: workspace.company_id.clone(),
            chart_id: text(proof, "chartId")?.into(),
            inventory_account_id: text(proof, "inventoryAccountId")?.into(),
            variance_account_id: text(proof, "varianceAccountId")?.into(),
            currency: text(proof, "currency")?.into(),
        };
        let finance =
            read_initial_count_finance(c, current.holding_id(), &basis, text(count, "count_date")?)
                .await?;
        Some(CurrentCountPostingSources { mapping, finance })
    } else {
        None
    };
    source.require_current(c, clock).await?;
    source.flow.require_current(c, current, clock).await?;
    if let Some(d) = &detail {
        d.require_current(c).await?;
    }
    if let Some(p) = &posting {
        p.require_current(c).await?;
    }
    if let Some(m) = &active_masters {
        m.require_current(c).await?;
    }
    Ok(PreparedServiceCountAction {
        source,
        detail,
        decision_digest,
        tracking,
        next,
        action,
        posting,
        active_masters,
    })
}

async fn require_replay_origin(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &str,
) -> Result<(String, Option<String>), Error> {
    use erpx_server_inventory::public::service_initial_physical_count::read_initial_count_record;
    use erpx_server_workflow::public::{
        service_count_setup_receipt::read_service_count_setup_history,
        service_initial_count_receipt::read_service_initial_count_receipt,
    };
    let scope = current.selection().workspace();
    let original = read_initial_count_record(
        c,
        scope.company_id.as_str(),
        scope.branch_id.as_ref().map(|b| b.as_str()),
        id,
    )
    .await?
    .ok_or_else(changed)?;
    let image: serde_json::Value =
        serde_json::from_str(&original.data().image).map_err(|_| changed())?;
    let source = &image["source"];
    let receipt = read_service_initial_count_receipt(
        c,
        text(source, "count_command_id")?,
        scope.company_id.as_str(),
        id,
    )
    .await?;
    let fact = receipt.facts().ok_or_else(changed)?;
    let proof: serde_json::Value = serde_json::from_str(&fact.proof_json).map_err(|_| changed())?;
    if proof != source["proof_json"] || fact.actor_id != text(&image["count"], "created_by")? {
        return Err(changed().into());
    }
    let setup = read_service_count_setup_history(
        c,
        scope.company_id.as_str(),
        text(source, "service_setup_command_id")?,
    )
    .await?;
    let setup_fact = setup.facts().ok_or_else(changed)?;
    let setup_proof: serde_json::Value =
        serde_json::from_str(&setup_fact.proof).map_err(|_| changed())?;
    if setup_fact.request_hash != text(&proof, "serviceSetupRequestHash")?
        || setup_proof["mapping"]["chart_id"] != proof["chartId"]
        || setup_proof["mapping"]["inventory_account_id"] != proof["inventoryAccountId"]
        || setup_proof["mapping"]["currency"] != proof["currency"]
    {
        return Err(changed().into());
    }
    original.require_current(c).await?;
    receipt.require_current(c).await?;
    setup.require_current(c).await?;
    let branch = text(&image["count"], "branch_id")?.to_owned();
    let journal = image["count"]["journal_id"].as_str().map(str::to_owned);
    Ok((branch, journal))
}

struct ActionReplayAuthority<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    request: &'a erpx_server_workflow::public::commands::WorkflowRequest,
    purchases: &'a crate::service_initial_physical_stock_purchases::ServiceCountPurchases,
    purpose: ServiceCountAuthorityPurpose,
}
impl<C: Clock + Sync> erpx_server_workflow::public::commands::PgCommandAuthority
    for ActionReplayAuthority<'_, C>
{
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &erpx_server_workflow::public::commands::WorkflowRequest,
    ) -> Result<(), Error> {
        use sqlx::Connection;
        if !c.is_in_transaction() {
            return Err(changed().into());
        }
        Ok(())
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        request: &erpx_server_workflow::public::commands::WorkflowRequest,
    ) -> Result<erpx_server_workflow::public::commands::CommandActor, Error> {
        if request.key != self.request.key
            || request.operation != self.request.operation
            || request.record_id != self.request.record_id
            || request.input != self.request.input
        {
            return Err(changed().into());
        }
        let authority=erpx_server_identity_access::public::service_initial_count_authority::capture_service_count_authority(c,self.current,self.purpose,self.clock).await?;
        authority.require_current(c, self.clock).await?;
        Ok(crate::service_initial_physical_stock_create::actor(
            self.current,
        )?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        actor: &erpx_server_workflow::public::commands::CommandActor,
        request: &erpx_server_workflow::public::commands::WorkflowRequest,
        disposition: erpx_server_workflow::public::commands::CommandDisposition,
    ) -> Result<(), Error> {
        if disposition != erpx_server_workflow::public::commands::CommandDisposition::Replay {
            return Err(changed().into());
        }
        self.purchases
            .require_for(c, CapabilityPurpose::History, self.clock.now())
            .await?;
        let (branch, journal) = require_replay_origin(c, self.current, &request.record_id).await?;
        erpx_server_workflow::public::service_count_command_evidence::verify_historical_count_command(c, actor, request, &branch, journal.as_deref()).await
    }
}
/// Resolve a genuine action receipt before Records or Files preparation. This
/// performs no mutation, including when no receipt exists. The surrounding
/// selected Holding unit remains responsible for every acquired utility lock.
pub struct ServiceCountActionCommand<'a> {
    pub key: &'a str,
    pub id: &'a str,
    pub action: CountAction,
    pub input: &'a InitialCountActionInput,
    pub files_features: erpx_server_files::public::count_submission::CountBindingFeatures,
}
pub struct ServiceCountAssignmentCommand<'a> {
    pub key: &'a str,
    pub id: &'a str,
    pub input: &'a erpx_server_workflow::public::commands::CommandJson,
}

async fn replay_service_count_action<
    C: Clock + Sync,
    I: erpx_server_shared::public::IdentifierSource,
>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    ids: &mut I,
    command: &ServiceCountActionCommand<'_>,
    environment:&crate::service_initial_physical_stock_sources::ServiceInitialPhysicalStockEnvironment,
) -> Result<Option<serde_json::Value>, Error> {
    let ServiceCountActionCommand {
        key,
        id,
        action,
        input,
        ..
    } = command;
    let (key, id, action, input) = (*key, *id, *action, *input);
    use erpx_server_workflow::public::commands::{PgCommandLedger, WorkflowRequest};
    let name = match action {
        CountAction::Submit => "submit",
        CountAction::Approve => "approve",
        CountAction::Post => "post",
        CountAction::Reject => "reject",
        CountAction::Cancel => "cancel",
    };
    let module = crate::service_initial_physical_stock_sources::require_count_modules(
        c,
        current,
        environment,
    )
    .await?;
    let purchases =
        crate::service_initial_physical_stock_purchases::ServiceCountPurchases::capture(
            c,
            current,
            clock.now(),
        )
        .await?;
    let request = WorkflowRequest {
        key: key.into(),
        operation: format!("inventory.count.{name}"),
        record_id: id.into(),
        input: input.normalized.clone(),
    };
    let purpose = if matches!(
        action,
        CountAction::Approve | CountAction::Post | CountAction::Reject
    ) {
        ServiceCountAuthorityPurpose::Review
    } else {
        ServiceCountAuthorityPurpose::Prepare
    };
    let mut authority = ActionReplayAuthority {
        current,
        clock,
        request: &request,
        purchases: &purchases,
        purpose,
    };
    let outcome = PgCommandLedger::new(
        current.selection().workspace().company_id.clone(),
        c,
        &mut authority,
        clock,
        ids,
    )
    .replay(
        &request,
        &mut crate::service_initial_physical_stock_create::ReplayOnly,
    )
    .await?;
    module.require_current(c).await?;
    outcome
        .map(|value| serde_json::from_str(value.result.as_str()).map_err(|_| changed().into()))
        .transpose()
}

/// Same-unit source retained across actual owner effects. Its after-check uses
/// Inventory and Workflow planned witnesses, rather than accepting a fresh row
/// as its own expected state.
pub struct ServiceCountActionOwner<'a,C>{
    pub prepared:PreparedServiceCountAction<'a>,
    pub current:&'a AuthorizedWorkContext,
    pub clock:&'a C,
    pub now:i64,
    pub flow:erpx_server_workflow::public::stock_count_flow::StockCountFlowActionWitness,
    actor_names:Vec<(String,String)>,
    pub transition:std::sync::OnceLock<erpx_server_inventory::public::service_initial_physical_count::InitialCountTransitionEffects>,
    pub expected_journal:std::sync::OnceLock<Option<String>>,
}
fn status_name(status: CountStatus) -> &'static str {
    match status {
        CountStatus::Draft => "draft",
        CountStatus::Submitted => "submitted",
        CountStatus::Approved => "approved",
        CountStatus::Rejected => "rejected",
        CountStatus::Cancelled => "cancelled",
        CountStatus::Posted => "posted",
    }
}
fn action_name(action: CountAction) -> &'static str {
    match action {
        CountAction::Submit => "submit",
        CountAction::Approve => "approve",
        CountAction::Post => "post",
        CountAction::Reject => "reject",
        CountAction::Cancel => "cancel",
    }
}
impl<'a, C: Clock> ServiceCountActionOwner<'a, C> {
    pub async fn prepare(
        c: &mut PgConnection,
        prepared: PreparedServiceCountAction<'a>,
        current: &'a AuthorizedWorkContext,
        clock: &'a C,
        reason: &str,
        command: &str,
        event: &str,
    ) -> Result<Self, Error> {
        use erpx_server_workflow::public::stock_count_flow::{
            StockCountFlowActionRequest, StockCountFlowActionWitness, read_stock_count_flow_basis,
        };
        prepared.source.require_current(c, clock).await?;
        prepared
            .source
            .flow
            .require_current(c, current, clock)
            .await?;
        let record = &prepared.source.flow.view.record;
        let actor_names=erpx_server_people_payroll::public::service_count_flow_people::read_service_count_member_names(c,&current.selection().workspace().company_id,&[current.user_id().as_str().to_owned()]).await?;
        let actor_name = actor_names
            .first()
            .map(|(_, name)| name.as_str())
            .unwrap_or(current.user_id().as_str());
        let basis =
            read_stock_count_flow_basis(c, &record.company_id, &record.count_id, &record.branch_id)
                .await?;
        let now = clock.now().get();
        let flow = StockCountFlowActionWitness::prepare(
            basis,
            &prepared.source.flow.view,
            StockCountFlowActionRequest {
                actor: current.user_id().as_str(),
                actor_name,
                action: action_name(prepared.action),
                reason,
                command,
                event,
                now,
            },
        )?;
        Ok(Self {
            prepared,
            current,
            clock,
            now,
            flow,
            actor_names,
            transition: std::sync::OnceLock::new(),
            expected_journal: std::sync::OnceLock::new(),
        })
    }
    pub async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_actor_name(c).await?;
        self.prepared.source.require_current(c, self.clock).await?;
        self.prepared
            .source
            .flow
            .require_current(c, self.current, self.clock)
            .await?;
        self.prepared
            .source
            .purchases
            .require_for(
                c,
                if self.prepared.action == CountAction::Submit {
                    CapabilityPurpose::NewWork
                } else {
                    CapabilityPurpose::SettleExisting
                },
                self.clock.now(),
            )
            .await?;
        if let Some(d) = &self.prepared.detail {
            d.require_current(c).await?;
        }
        if let Some(p) = &self.prepared.posting {
            p.require_current(c).await?;
        }
        if let Some(m) = &self.prepared.active_masters {
            m.require_current(c).await?;
        }
        Ok(())
    }
    pub async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_actor_name(c).await?;
        let s = &self.prepared.source;
        s.receipt.require_current(c).await?;
        s.setup.require_current(c).await?;
        s.masters.require_current(c).await?;
        s.authority.require_current(c, self.clock).await?;
        s.purchases
            .require_for(
                c,
                if self.prepared.action == CountAction::Submit {
                    CapabilityPurpose::NewWork
                } else {
                    CapabilityPurpose::SettleExisting
                },
                self.clock.now(),
            )
            .await?;
        s.flow
            .require_participants_after(c, self.current, self.clock)
            .await?;
        self.flow.retain(c, status_name(self.prepared.next)).await?;
        self.transition
            .get()
            .ok_or_else(changed)?
            .require_current(c)
            .await?;
        if let Some(p) = &self.prepared.posting {
            p.require_current(c).await?;
        }
        if let Some(m) = &self.prepared.active_masters {
            m.require_current(c).await?;
        }
        Ok(())
    }
    async fn require_actor_name(&self, c: &mut PgConnection) -> Result<(), Error> {
        let current=erpx_server_people_payroll::public::service_count_flow_people::read_service_count_member_names(c,&self.current.selection().workspace().company_id,&[self.current.user_id().as_str().to_owned()]).await?;
        if current != self.actor_names {
            return Err(changed().into());
        }
        Ok(())
    }
}

pub struct ServiceCountPostingOwner<'a, 'b, C> {
    pub action: &'a ServiceCountActionOwner<'b, C>,
    pub facts: erpx_server_finance::public::initial_count_finance::InitialCountPostingFacts,
}
impl<'a, 'b, C: Clock> ServiceCountPostingOwner<'a, 'b, C> {
    pub fn new(
        action: &'a ServiceCountActionOwner<'b, C>,
        key: &str,
        reason: &str,
    ) -> Result<Self, BusinessError> {
        use erpx_server_finance::public::{
            FinanceActor, initial_count_finance::InitialCountPostingFacts,
        };
        use erpx_server_shared::public::CommandKey;
        if action.prepared.action != CountAction::Post || action.prepared.posting.is_none() {
            return Err(changed());
        }
        let count = &action.prepared.source.image["count"];
        let line = action.prepared.source.image["lines"]
            .as_array()
            .and_then(|l| l.first())
            .ok_or_else(changed)?;
        let value = line["counted_value_minor"].as_i64().ok_or_else(changed)?;
        let snapshot = line["snapshot_value_minor"].as_i64().ok_or_else(changed)?;
        let current = action.current;
        let facts = InitialCountPostingFacts {
            actor: FinanceActor {
                user_id: current.user_id().clone(),
                holding_id: current.holding_id().clone(),
                company_id: current.selection().workspace().company_id.clone(),
                assignment_id: current.selection().assignment().id.clone(),
            },
            command_key: CommandKey::new(key)?,
            count_id: text(count, "id")?.into(),
            count_number: text(count, "number")?.into(),
            reason: reason.into(),
            actual_cost_delta_minor: value.checked_sub(snapshot).ok_or_else(changed)?,
        };
        Ok(Self { action, facts })
    }
}
impl<C: Clock + Sync>
    erpx_server_finance::public::initial_count_finance::PgInitialCountPostingSource
    for ServiceCountPostingOwner<'_, '_, C>
{
    fn facts(
        &self,
    ) -> &erpx_server_finance::public::initial_count_finance::InitialCountPostingFacts {
        &self.facts
    }
    fn finance(
        &self,
    ) -> &erpx_server_finance::public::initial_count_finance::CurrentInitialCountFinance {
        &self
            .action
            .prepared
            .posting
            .as_ref()
            .expect("post source established at construction")
            .finance
    }
    async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.action.require_before(c).await
    }
    async fn require_after(
        &self,
        c: &mut PgConnection,
        journal_id: Option<&str>,
    ) -> Result<(), Error> {
        if self
            .action
            .expected_journal
            .get()
            .ok_or_else(changed)?
            .as_deref()
            != journal_id
        {
            return Err(changed().into());
        }
        self.action.require_after(c).await
    }
}

/// Collaboration consumes Workflow's actual planned recipient only after the
/// Inventory transition and Flow handoff have been witnessed in this unit.
pub struct ServiceCountActionNotification<'a,'b,C>{
 pub action:&'a ServiceCountActionOwner<'b,C>,
 facts:erpx_server_collaboration::public::service_count_flow_notification::CountFlowNotificationFacts,
}
impl<'a, 'b, C: Clock> ServiceCountActionNotification<'a, 'b, C> {
    pub fn new(
        action: &'a ServiceCountActionOwner<'b, C>,
        notification: erpx_server_workflow::public::stock_count_flow::StockCountNotification,
        id: String,
    ) -> Result<Self, BusinessError> {
        let record = &action.prepared.source.flow.view.record;
        if id.is_empty()
            || notification.recipient == action.current.user_id().as_str()
            || notification.record_version != record.version + 1
            || notification.message != "Workflow updated: review your next action"
        {
            return Err(changed());
        }
        Ok(Self{action,facts:erpx_server_collaboration::public::service_count_flow_notification::CountFlowNotificationFacts{id,company_id:record.company_id.clone(),recipient:notification.recipient,count_id:record.count_id.clone(),version:notification.record_version,message:notification.message,created_at:action.now}})
    }
}
impl<C:Clock+Sync> erpx_server_collaboration::public::service_count_flow_notification::PgCountFlowNotificationSource for ServiceCountActionNotification<'_,'_,C>{
 fn facts(&self)->&erpx_server_collaboration::public::service_count_flow_notification::CountFlowNotificationFacts{&self.facts}
 async fn require_before(&self,c:&mut PgConnection)->Result<(),Error>{self.action.require_after(c).await}
 async fn require_after(&self,c:&mut PgConnection)->Result<(),Error>{self.action.require_after(c).await}
}

/// Files receives the exact Records selection and current Inventory action
/// witness. Feature flags originate in trusted runtime configuration.
pub struct ServiceCountFilesOwner<'a, 'b, C> {
    action: &'a ServiceCountActionOwner<'b, C>,
    facts: erpx_server_files::public::count_submission::CountBindingFacts,
}
impl<'a, 'b, C: Clock> ServiceCountFilesOwner<'a, 'b, C> {
    pub fn new(
        action: &'a ServiceCountActionOwner<'b, C>,
        binding: erpx_server_records::public::business_evidence::BusinessEvidenceBinding,
        features: erpx_server_files::public::count_submission::CountBindingFeatures,
        command: &str,
    ) -> Result<Self, BusinessError> {
        use erpx_server_files::public::count_submission::CountBindingFacts;
        let record = &action.prepared.source.flow.view.record;
        if action.prepared.action != CountAction::Submit
            || binding.target_id != record.count_id
            || binding.expected_target_version != record.version
            || binding.command_id != command
            || binding.binding_type != "stock_count_submission"
            || binding.binding_id != format!("count-submit-{command}")
            || !["stock_count", "inventory_stock_count"]
                .contains(&binding.selection.target_type.as_str())
            || binding.selection.contexts.is_empty()
        {
            return Err(changed());
        }
        let facts = CountBindingFacts {
            holding_id: action.current.holding_id().clone(),
            company_id: action.current.selection().workspace().company_id.clone(),
            actor_id: action.current.user_id().as_str().into(),
            target_type: binding.selection.target_type,
            target_id: record.count_id.clone(),
            target_version: record.version,
            command_id: command.into(),
            set_ids: binding.selection.set_ids,
            features,
            now: action.now,
        };
        Ok(Self { action, facts })
    }
}
impl<C: Clock + Sync> erpx_server_files::public::count_submission::PgCountBindingOwner
    for ServiceCountFilesOwner<'_, '_, C>
{
    fn facts(&self) -> &erpx_server_files::public::count_submission::CountBindingFacts {
        &self.facts
    }
    async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.action.require_before(c).await
    }
    async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.action.require_after(c).await
    }
    async fn require_replay(&self, _: &mut PgConnection) -> Result<(), Error> {
        Err(changed().into())
    }
}

struct ServiceCountActionEffects<'a, 'b, C: Clock + Sync> {
    owner: &'a ServiceCountActionOwner<'b, C>,
    completion: &'a ActionCompletion<'a, 'b, C>,
    transition:
        erpx_server_inventory::public::service_initial_physical_count::InitialCountTransition,
    finance: Option<
        erpx_server_finance::public::initial_count_finance::PreparedInitialCountPosting<
            'a,
            ServiceCountPostingOwner<'a, 'b, C>,
        >,
    >,
    files: Option<
        erpx_server_files::public::count_submission::PreparedCountBinding<
            'a,
            ServiceCountFilesOwner<'a, 'b, C>,
        >,
    >,
    notification_id: String,
}
struct ActionCompletion<'a,'b,C:Clock+Sync>{
 owner:&'a ServiceCountActionOwner<'b,C>,
 decision:Option<erpx_server_workflow::public::service_count_decision_evidence::PlannedServiceCountDecision>,
 finance:std::sync::OnceLock<Option<erpx_server_finance::public::initial_count_finance::RetainedInitialCountPosting<'a,ServiceCountPostingOwner<'a,'b,C>>>>,
 files:std::sync::OnceLock<Option<erpx_server_files::public::count_submission::RetainedCountBinding<'a,ServiceCountFilesOwner<'a,'b,C>>>>,
 notification:std::sync::OnceLock<Option<erpx_server_collaboration::public::service_count_flow_notification::RetainedCountFlowNotification>>,
 ledger_clock:erpx_server_workflow::public::service_count_command_evidence::CountCommandClock,
 generated:&'a erpx_server_workflow::public::service_count_command_evidence::CountCommandGeneratedIds,
 evidence:std::sync::OnceLock<erpx_server_workflow::public::service_count_command_evidence::PlannedCountCommandEvidence>,
 retained:std::sync::OnceLock<erpx_server_workflow::public::service_count_command_evidence::RetainedCountCommandEvidence>,
}
impl<C: Clock + Sync> ActionCompletion<'_, '_, C> {
    async fn require_effects(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.owner.require_after(c).await?;
        if let Some(d) = &self.decision {
            d.require_current(c).await?;
        }
        if let Some(f) = self.files.get().ok_or_else(changed)? {
            f.require_after(c).await?;
        }
        if let Some(f) = self.finance.get().ok_or_else(changed)? {
            f.require_after(c).await?;
        }
        if let Some(n) = self.notification.get().ok_or_else(changed)? {
            n.require_current(c).await?;
        }
        Ok(())
    }
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_effects(c).await?;
        self.retained
            .get()
            .ok_or_else(changed)?
            .require_current(c)
            .await
    }
}
impl<C: Clock + Sync> erpx_server_workflow::public::commands::PgOwnerCommandEffects
    for ServiceCountActionEffects<'_, '_, C>
{
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &erpx_server_workflow::public::commands::WorkflowRequest,
    ) -> Result<(), Error> {
        use sqlx::Connection;
        if !c.is_in_transaction() {
            return Err(changed().into());
        }
        Ok(())
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        actor: &erpx_server_workflow::public::commands::CommandActor,
        request: &erpx_server_workflow::public::commands::WorkflowRequest,
    ) -> Result<erpx_server_workflow::public::commands::WorkflowOutput, Error> {
        use erpx_server_workflow::public::commands::{CommandJson, WorkflowOutput};
        self.owner.require_before(c).await?;
        let t = &self.transition;
        let record = &self.owner.prepared.source.flow.view.record;
        let attachments = request
            .input
            .property("attachmentSetIds")?
            .ok_or_else(changed)?
            .array_elements()?
            .ok_or_else(changed)?;
        if self.files.is_some() != !attachments.is_empty()
            || (!attachments.is_empty() && t.action != CountAction::Submit)
        {
            return Err(changed().into());
        }
        if t.count_id != record.count_id
            || t.company_id != record.company_id
            || t.expected_version != record.version
            || t.action != self.owner.prepared.action
            || t.actor_id != actor.user_id.as_str()
            || t.command_id != request.key
            || request.record_id != record.count_id
            || request.operation != format!("inventory.count.{}", action_name(t.action))
            || t.now != self.owner.now
            || t.journal_id.as_deref() != self.finance.as_ref().and_then(|p| p.journal_id())
            || self.completion.decision.is_some()
                != matches!(
                    t.action,
                    CountAction::Approve | CountAction::Post | CountAction::Reject
                )
        {
            return Err(changed().into());
        }
        if self
            .owner
            .expected_journal
            .set(t.journal_id.clone())
            .is_err()
        {
            return Err(changed().into());
        }
        let output = WorkflowOutput {
            version: record.version + 1,
            result: CommandJson::from_serializable(
                &serde_json::json!({"id":record.count_id,"status":status_name(self.owner.prepared.next),"version":record.version+1,"journalId":t.journal_id}),
            )?,
            event_scope: None,
        };
        let evidence=erpx_server_workflow::public::service_count_command_evidence::plan_count_command_evidence(c,actor,request,&output,self.completion.ledger_clock).await?;
        self.completion
            .evidence
            .set(evidence)
            .map_err(|_| changed())?;
        Ok(output)
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &erpx_server_workflow::public::commands::CommandActor,
        _: &erpx_server_workflow::public::commands::WorkflowRequest,
    ) -> Result<(), Error> {
        self.owner.require_before(c).await?;
        let files = if let Some(p) = self.files.take() {
            Some(p.apply(c).await?)
        } else {
            None
        };
        self.completion.files.set(files).map_err(|_| changed())?;
        let finance = if let Some(p) = self.finance.take() {
            Some(p.persist(c).await?)
        } else {
            None
        };
        self.completion
            .finance
            .set(finance)
            .map_err(|_| changed())?;
        if let Some(d) = &self.completion.decision {
            d.apply(c).await?;
        }
        self.owner.flow.apply_before(c).await?;
        let transition=erpx_server_inventory::public::service_initial_physical_count::apply_initial_count_transition(c,&self.transition,&self.owner.prepared.source.original).await?;
        self.owner
            .transition
            .set(transition)
            .map_err(|_| changed())?;
        let next = self
            .owner
            .flow
            .apply_after(c, status_name(self.owner.prepared.next))
            .await?;
        let notification = if let Some(next) = next {
            let source = ServiceCountActionNotification::new(
                self.owner,
                next,
                self.notification_id.clone(),
            )?;
            let prepared=erpx_server_collaboration::public::service_count_flow_notification::prepare_count_flow_notification(c,&source).await?;
            Some(prepared.persist(c).await?)
        } else {
            None
        };
        self.completion
            .notification
            .set(notification)
            .map_err(|_| changed())?;
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &erpx_server_workflow::public::commands::CommandActor,
        _: &erpx_server_workflow::public::commands::WorkflowRequest,
    ) -> Result<(), Error> {
        if let Some(finance) = self.completion.finance.get().ok_or_else(changed)? {
            finance.flush_and_require_after(c).await?;
        } else {
            sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
                .execute(&mut *c)
                .await?;
        }
        self.completion.require_effects(c).await?;
        let evidence = self
            .completion
            .evidence
            .get()
            .ok_or_else(changed)?
            .retain(c, self.completion.generated)
            .await?;
        self.completion
            .retained
            .set(evidence)
            .map_err(|_| changed())?;
        self.completion.require_current(c).await
    }
}

struct ActionExecuteAuthority<'a, 'b, C: Clock + Sync> {
    owner: &'a ServiceCountActionOwner<'b, C>,
    completion: &'a ActionCompletion<'a, 'b, C>,
    request: &'a erpx_server_workflow::public::commands::WorkflowRequest,
}
impl<C: Clock + Sync> erpx_server_workflow::public::commands::PgCommandAuthority
    for ActionExecuteAuthority<'_, '_, C>
{
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &erpx_server_workflow::public::commands::WorkflowRequest,
    ) -> Result<(), Error> {
        use sqlx::Connection;
        if !c.is_in_transaction() {
            return Err(changed().into());
        }
        Ok(())
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        r: &erpx_server_workflow::public::commands::WorkflowRequest,
    ) -> Result<erpx_server_workflow::public::commands::CommandActor, Error> {
        if r.key != self.request.key
            || r.operation != self.request.operation
            || r.record_id != self.request.record_id
            || r.input != self.request.input
        {
            return Err(changed().into());
        }
        if self.owner.transition.get().is_some() {
            self.completion.require_current(c).await?;
        } else {
            self.owner.require_before(c).await?;
        }
        Ok(crate::service_initial_physical_stock_create::actor(
            self.owner.current,
        )?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        _: &erpx_server_workflow::public::commands::CommandActor,
        r: &erpx_server_workflow::public::commands::WorkflowRequest,
        disposition: erpx_server_workflow::public::commands::CommandDisposition,
    ) -> Result<(), Error> {
        if disposition != erpx_server_workflow::public::commands::CommandDisposition::Execute
            || r.key != self.request.key
        {
            return Err(changed().into());
        }
        if self.owner.transition.get().is_some() {
            self.completion.require_current(c).await
        } else {
            self.owner.require_before(c).await
        }
    }
}

/// Execute a native action in the selected Holding unit. Call only after all
/// source locks have been acquired by the selected Account authorizer. The
/// Files feature configuration is trusted runtime configuration, never input.
pub async fn execute_service_count_action<
    C: Clock + Sync,
    I: erpx_server_shared::public::IdentifierSource,
>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    ids: &mut I,
    command: ServiceCountActionCommand<'_>,
    environment:&crate::service_initial_physical_stock_sources::ServiceInitialPhysicalStockEnvironment,
) -> Result<serde_json::Value, Error> {
    let (key, id, action, input) = (command.key, command.id, command.action, command.input);
    let checked = crate::service_initial_physical_stock_command::normalize_initial_count_action(
        input.normalized.as_str(),
    )?;
    if checked.expected_version != input.expected_version
        || checked.attachment_set_ids != input.attachment_set_ids
        || checked.reason != input.reason
        || checked.normalized != input.normalized
    {
        return Err(changed().into());
    }
    use erpx_server_inventory::public::service_initial_physical_count::InitialCountTransition;
    use erpx_server_workflow::public::commands::{PgCommandLedger, WorkflowRequest};
    use erpx_server_workflow::public::service_count_decision_evidence::{
        PlannedServiceCountDecision, ServiceCountDecisionFacts,
    };
    let module = crate::service_initial_physical_stock_sources::require_count_modules(
        c,
        current,
        environment,
    )
    .await?;
    if let Some(result) =
        replay_service_count_action(c, current, clock, ids, &command, environment).await?
    {
        module.require_current(c).await?;
        return Ok(result);
    }
    let preflight = prepare_service_count_action(c, current, clock, id, action, input).await?;
    let event = ids.next().as_str().to_owned();
    let owner = ServiceCountActionOwner::prepare(
        c,
        preflight,
        current,
        clock,
        input.reason.as_deref().unwrap_or(""),
        key,
        &event,
    )
    .await?;
    let record = &owner.prepared.source.flow.view.record;
    let actor = crate::service_initial_physical_stock_create::actor(current)?;
    let decision = if let Some(digest) = &owner.prepared.decision_digest {
        Some(
            PlannedServiceCountDecision::prepare(
                c,
                &actor,
                ServiceCountDecisionFacts {
                    id: ids.next().as_str().into(),
                    command_id: key.into(),
                    record_id: id.into(),
                    action: format!("inventory.count.{}", action_name(action)),
                    reviewed_version: record.version,
                    input_digest: digest.clone(),
                    created_at: owner.now,
                },
            )
            .await?,
        )
    } else {
        None
    };
    let posting_owner = if action == CountAction::Post {
        Some(ServiceCountPostingOwner::new(
            &owner,
            key,
            input.reason.as_deref().ok_or_else(changed)?,
        )?)
    } else {
        None
    };
    let finance = if let Some(p) = &posting_owner {
        Some(
            erpx_server_finance::public::initial_count_finance::prepare_initial_count_posting(
                c, p, clock,
            )
            .await?,
        )
    } else {
        None
    };
    let journal_id = finance
        .as_ref()
        .and_then(|p| p.journal_id())
        .map(str::to_owned);
    let binding = if input.attachment_set_ids.is_empty() {
        None
    } else {
        use erpx_server_records::public::business_evidence::{
            BusinessEvidenceBinding, EvidenceContext, EvidencePolicyFact, EvidenceSetFact,
            select_evidence,
        };
        let targets = erpx_server_files::public::count_submission::read_count_binding_target_types(
            c,
            &current.selection().workspace().company_id,
            &input.attachment_set_ids,
        )
        .await?;
        let facts = targets
            .into_iter()
            .map(|(id, target_type)| EvidenceSetFact { id, target_type })
            .collect::<Vec<_>>();
        let policies = erpx_server_files::public::count_evidence_policy::count_evidence_policies()
            .iter()
            .map(|p| EvidencePolicyFact {
                target_type: p.target_type.into(),
                purpose_key: p.purpose_key.into(),
                component_kinds: vec![p.component_kind.into()],
            })
            .collect::<Vec<_>>();
        let contexts = ["count_sheet", "count_photo", "count_evidence"]
            .into_iter()
            .map(|p| EvidenceContext {
                purpose_key: p.into(),
                component_kind: "action".into(),
                component_ref: Some("submit".into()),
            })
            .collect::<Vec<_>>();
        let selection = select_evidence(
            &input.attachment_set_ids,
            &["stock_count".into(), "inventory_stock_count".into()],
            &contexts,
            &facts,
            &policies,
        )?
        .ok_or_else(changed)?;
        Some(BusinessEvidenceBinding {
            selection,
            target_id: id.into(),
            expected_target_version: record.version,
            command_id: key.into(),
            binding_type: "stock_count_submission".into(),
            binding_id: format!("count-submit-{key}"),
        })
    };
    let files_owner = binding
        .map(|b| ServiceCountFilesOwner::new(&owner, b, command.files_features, key))
        .transpose()?;
    let files = if let Some(f) = &files_owner {
        Some(erpx_server_files::public::count_submission::prepare_count_submission(c, f).await?)
    } else {
        None
    };
    let transition = InitialCountTransition {
        count_id: id.into(),
        company_id: record.company_id.clone(),
        tracking: owner.prepared.tracking,
        expected_version: record.version,
        action,
        actor_id: current.user_id().as_str().into(),
        reason: input.reason.clone(),
        command_id: key.into(),
        decision_id: ids.next().as_str().into(),
        journal_id,
        now: owner.now,
    };
    let request = WorkflowRequest {
        key: key.into(),
        operation: format!("inventory.count.{}", action_name(action)),
        record_id: id.into(),
        input: input.normalized.clone(),
    };
    let ledger_clock =
        erpx_server_workflow::public::service_count_command_evidence::CountCommandClock::sample(
            clock,
        )?;
    let generated=erpx_server_workflow::public::service_count_command_evidence::CountCommandGeneratedIds::default();
    let completion = ActionCompletion {
        owner: &owner,
        decision,
        finance: std::sync::OnceLock::new(),
        files: std::sync::OnceLock::new(),
        notification: std::sync::OnceLock::new(),
        ledger_clock,
        generated: &generated,
        evidence: std::sync::OnceLock::new(),
        retained: std::sync::OnceLock::new(),
    };
    let mut effects = ServiceCountActionEffects {
        owner: &owner,
        completion: &completion,
        transition,
        finance,
        files,
        notification_id: ids.next().as_str().into(),
    };
    let mut authority = ActionExecuteAuthority {
        owner: &owner,
        completion: &completion,
        request: &request,
    };
    let mut ledger_ids = generated.wrap(ids);
    let outcome = PgCommandLedger::new(
        current.selection().workspace().company_id.clone(),
        c,
        &mut authority,
        &ledger_clock,
        &mut ledger_ids,
    )
    .execute(&request, &mut effects)
    .await?;
    module.require_current(c).await?;
    serde_json::from_str(outcome.result.as_str()).map_err(|_| changed().into())
}

struct AssignmentOwner<'a, C> {
    source: ServiceCountRecordSource<'a>,
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    now: i64,
    purpose: CapabilityPurpose,
    plan: erpx_server_workflow::public::stock_count_flow::StockCountFlowAssignmentWitness,
    names: Vec<(String, String)>,
    applied: std::sync::OnceLock<()>,
    notification:std::sync::OnceLock<erpx_server_collaboration::public::service_count_flow_notification::RetainedCountFlowNotification>,
    ledger_clock:erpx_server_workflow::public::service_count_command_evidence::CountCommandClock,
    generated:&'a erpx_server_workflow::public::service_count_command_evidence::CountCommandGeneratedIds,
    evidence:std::sync::OnceLock<erpx_server_workflow::public::service_count_command_evidence::PlannedCountCommandEvidence>,
    retained:std::sync::OnceLock<erpx_server_workflow::public::service_count_command_evidence::RetainedCountCommandEvidence>,
}
impl<C: Clock> AssignmentOwner<'_, C> {
    async fn name_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let v=erpx_server_people_payroll::public::service_count_flow_people::read_service_count_member_names(c,&self.current.selection().workspace().company_id,&[self.current.user_id().as_str().into()]).await?;
        if v != self.names {
            return Err(changed().into());
        }
        Ok(())
    }
    async fn before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.name_current(c).await?;
        self.source.require_current(c, self.clock).await?;
        self.source
            .flow
            .require_current(c, self.current, self.clock)
            .await?;
        self.source
            .purchases
            .require_for(c, self.purpose, self.clock.now())
            .await
    }
    async fn after(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.name_current(c).await?;
        self.source.require_current(c, self.clock).await?;
        self.source
            .purchases
            .require_for(c, self.purpose, self.clock.now())
            .await?;
        self.source
            .flow
            .require_participants_after(c, self.current, self.clock)
            .await?;
        self.plan.retain(c).await
    }
    async fn current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if self.applied.get().is_some() {
            self.after(c).await?;
            self.notification
                .get()
                .ok_or_else(changed)?
                .require_current(c)
                .await?;
            self.retained
                .get()
                .ok_or_else(changed)?
                .require_current(c)
                .await
        } else {
            self.before(c).await
        }
    }
}
struct AssignmentAuthority<'a, 'b, C> {
    owner: &'a AssignmentOwner<'b, C>,
    request: &'a erpx_server_workflow::public::commands::WorkflowRequest,
}
impl<C: Clock + Sync> erpx_server_workflow::public::commands::PgCommandAuthority
    for AssignmentAuthority<'_, '_, C>
{
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &erpx_server_workflow::public::commands::WorkflowRequest,
    ) -> Result<(), Error> {
        use sqlx::Connection;
        if !c.is_in_transaction() {
            return Err(changed().into());
        }
        Ok(())
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        r: &erpx_server_workflow::public::commands::WorkflowRequest,
    ) -> Result<erpx_server_workflow::public::commands::CommandActor, Error> {
        if r.key != self.request.key
            || r.record_id != self.request.record_id
            || r.operation != self.request.operation
            || r.input != self.request.input
        {
            return Err(changed().into());
        }
        self.owner.current(c).await?;
        Ok(crate::service_initial_physical_stock_create::actor(
            self.owner.current,
        )?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        _: &erpx_server_workflow::public::commands::CommandActor,
        _: &erpx_server_workflow::public::commands::WorkflowRequest,
        d: erpx_server_workflow::public::commands::CommandDisposition,
    ) -> Result<(), Error> {
        if d != erpx_server_workflow::public::commands::CommandDisposition::Execute {
            return Err(changed().into());
        }
        self.owner.current(c).await
    }
}
struct AssignmentNotification<'a,'b,C>{owner:&'a AssignmentOwner<'b,C>,facts:erpx_server_collaboration::public::service_count_flow_notification::CountFlowNotificationFacts}
impl<C:Clock+Sync> erpx_server_collaboration::public::service_count_flow_notification::PgCountFlowNotificationSource for AssignmentNotification<'_,'_,C>{
 fn facts(&self)->&erpx_server_collaboration::public::service_count_flow_notification::CountFlowNotificationFacts{&self.facts}
 async fn require_before(&self,c:&mut PgConnection)->Result<(),Error>{self.owner.after(c).await}
 async fn require_after(&self,c:&mut PgConnection)->Result<(),Error>{self.owner.after(c).await}
}
struct AssignmentEffects<'a, 'b, C> {
    owner: &'a AssignmentOwner<'b, C>,
    notification_id: String,
}
impl<C: Clock + Sync> erpx_server_workflow::public::commands::PgOwnerCommandEffects
    for AssignmentEffects<'_, '_, C>
{
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &erpx_server_workflow::public::commands::WorkflowRequest,
    ) -> Result<(), Error> {
        use sqlx::Connection;
        if !c.is_in_transaction() {
            return Err(changed().into());
        }
        Ok(())
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        actor: &erpx_server_workflow::public::commands::CommandActor,
        request: &erpx_server_workflow::public::commands::WorkflowRequest,
    ) -> Result<erpx_server_workflow::public::commands::WorkflowOutput, Error> {
        use erpx_server_workflow::public::commands::{
            CommandEventScope, CommandField, CommandJson, WorkflowOutput,
        };
        self.owner.before(c).await?;
        let r = &self.owner.source.flow.view.record;
        let output = WorkflowOutput {
            version: r.version,
            result: CommandJson::from_serializable(
                &serde_json::json!({"id":r.count_id,"version":self.owner.plan.result_version()}),
            )?,
            event_scope: Some(CommandEventScope {
                company_id: self
                    .owner
                    .current
                    .selection()
                    .workspace()
                    .company_id
                    .clone(),
                branch_id: CommandField::Value(r.branch_id.clone()),
                department_id: CommandField::Null,
            }),
        };
        let evidence=erpx_server_workflow::public::service_count_command_evidence::plan_count_command_evidence(c,actor,request,&output,self.owner.ledger_clock).await?;
        self.owner.evidence.set(evidence).map_err(|_| changed())?;
        Ok(output)
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &erpx_server_workflow::public::commands::CommandActor,
        _: &erpx_server_workflow::public::commands::WorkflowRequest,
    ) -> Result<(), Error> {
        self.owner.before(c).await?;
        self.owner.plan.apply(c).await?;
        self.owner.applied.set(()).map_err(|_| changed())?;
        let n = self.owner.plan.notification();
        let r = &self.owner.source.flow.view.record;
        let source=AssignmentNotification{owner:self.owner,facts:erpx_server_collaboration::public::service_count_flow_notification::CountFlowNotificationFacts{id:self.notification_id.clone(),company_id:r.company_id.clone(),recipient:n.recipient,count_id:r.count_id.clone(),version:n.record_version,message:n.message,created_at:self.owner.now}};
        let p=erpx_server_collaboration::public::service_count_flow_notification::prepare_count_flow_notification(c,&source).await?;
        self.owner
            .notification
            .set(p.persist(c).await?)
            .map_err(|_| changed())?;
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &erpx_server_workflow::public::commands::CommandActor,
        _: &erpx_server_workflow::public::commands::WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *c)
            .await?;
        self.owner.after(c).await?;
        self.owner
            .notification
            .get()
            .ok_or_else(changed)?
            .require_current(c)
            .await?;
        let evidence = self
            .owner
            .evidence
            .get()
            .ok_or_else(changed)?
            .retain(c, self.owner.generated)
            .await?;
        self.owner.retained.set(evidence).map_err(|_| changed())?;
        self.owner.current(c).await
    }
}

/// Reassign a single current/future Count Flow primary through real Workflow,
/// Identity, People, purchase and Collaboration owners in the selected unit.
pub async fn assign_service_count_flow<
    C: Clock + Sync,
    I: erpx_server_shared::public::IdentifierSource,
>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    ids: &mut I,
    command: ServiceCountAssignmentCommand<'_>,
    environment:&crate::service_initial_physical_stock_sources::ServiceInitialPhysicalStockEnvironment,
) -> Result<serde_json::Value, Error> {
    let ServiceCountAssignmentCommand { key, id, input } = command;
    let suffix = input
        .as_str()
        .strip_prefix("{\"kind\":\"stock_count\",")
        .ok_or_else(changed)?;
    let checked = erpx_server_workflow::public::stock_count_flow::normalize_stock_count_assignment(
        &format!("{{{suffix}"),
    )?;
    if &checked != input {
        return Err(changed().into());
    }
    use erpx_server_workflow::public::{
        commands::{PgCommandLedger, WorkflowRequest},
        stock_count_flow::{
            StockCountFlowAssignmentRequest, StockCountFlowAssignmentWitness, StockCountStep,
            read_stock_count_flow_basis,
        },
    };
    let module = crate::service_initial_physical_stock_sources::require_count_modules(
        c,
        current,
        environment,
    )
    .await?;
    let purchases =
        crate::service_initial_physical_stock_purchases::ServiceCountPurchases::capture(
            c,
            current,
            clock.now(),
        )
        .await?;
    let request = WorkflowRequest {
        key: key.into(),
        operation: "flow.assign".into(),
        record_id: id.into(),
        input: input.clone(),
    };
    let mut replay_authority = ActionReplayAuthority {
        current,
        clock,
        request: &request,
        purchases: &purchases,
        purpose: ServiceCountAuthorityPurpose::Read,
    };
    if let Some(outcome) = PgCommandLedger::new(
        current.selection().workspace().company_id.clone(),
        c,
        &mut replay_authority,
        clock,
        ids,
    )
    .replay(
        &request,
        &mut crate::service_initial_physical_stock_create::ReplayOnly,
    )
    .await?
    {
        module.require_current(c).await?;
        return serde_json::from_str(outcome.result.as_str()).map_err(|_| changed().into());
    }
    let data: serde_json::Value = serde_json::from_str(input.as_str()).map_err(|_| changed())?;
    if data["kind"] != "stock_count" || data["saveDefault"] != false {
        return Err(changed().into());
    }
    let step = StockCountStep::parse(text(&data, "stepKey")?).ok_or_else(changed)?;
    let user = text(&data, "userId")?;
    let reason = text(&data, "reason")?;
    let flow_version = data["expectedVersion"].as_i64().ok_or_else(changed)?;
    let record_version = data["recordVersion"].as_i64().ok_or_else(changed)?;
    let source =
        read_service_count_record(c, current, clock, id, ServiceCountAuthorityPurpose::Read)
            .await?;
    let record = &source.flow.view.record;
    let purpose = if step == StockCountStep::Draft
        || ["draft", "rejected"].contains(&record.status.as_str())
    {
        CapabilityPurpose::NewWork
    } else {
        CapabilityPurpose::SettleExisting
    };
    source
        .purchases
        .require_for(c, purpose, clock.now())
        .await?;
    let names=erpx_server_people_payroll::public::service_count_flow_people::read_service_count_member_names(c,&current.selection().workspace().company_id,&[current.user_id().as_str().into()]).await?;
    let actor_name = names
        .first()
        .map(|(_, n)| n.as_str())
        .unwrap_or(current.user_id().as_str());
    let basis = read_stock_count_flow_basis(c, &record.company_id, id, &record.branch_id).await?;
    let now = clock.now().get();
    let event = ids.next().as_str().to_owned();
    let plan = StockCountFlowAssignmentWitness::prepare(
        basis,
        &source.flow.view,
        StockCountFlowAssignmentRequest {
            step,
            user,
            expected_flow: flow_version,
            expected_record: record_version,
            company_admin: source.authority.company_admin(),
            actor: current.user_id().as_str(),
            actor_name,
            reason,
            command: key,
            event: &event,
            now,
        },
    )?;
    let notification_id = ids.next().as_str().to_owned();
    let ledger_clock =
        erpx_server_workflow::public::service_count_command_evidence::CountCommandClock::sample(
            clock,
        )?;
    let generated=erpx_server_workflow::public::service_count_command_evidence::CountCommandGeneratedIds::default();
    let owner = AssignmentOwner {
        source,
        current,
        clock,
        now,
        purpose,
        plan,
        names,
        applied: std::sync::OnceLock::new(),
        notification: std::sync::OnceLock::new(),
        ledger_clock,
        generated: &generated,
        evidence: std::sync::OnceLock::new(),
        retained: std::sync::OnceLock::new(),
    };
    let mut authority = AssignmentAuthority {
        owner: &owner,
        request: &request,
    };
    let mut effects = AssignmentEffects {
        owner: &owner,
        notification_id,
    };
    let mut ledger_ids = generated.wrap(ids);
    let outcome = PgCommandLedger::new(
        current.selection().workspace().company_id.clone(),
        c,
        &mut authority,
        &ledger_clock,
        &mut ledger_ids,
    )
    .execute(&request, &mut effects)
    .await?;
    module.require_current(c).await?;
    serde_json::from_str(outcome.result.as_str()).map_err(|_| changed().into())
}
