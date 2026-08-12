//! Role-scoped Assets and Maintenance workspace.
//!
//! This surface deliberately has no tenant-route, subscription, or aggregate-ID
//! input. The active role is the only authority presented to the server.

use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use dioxus::prelude::*;
use erpx_assets::{
    AssetId, AssetListFilter, AssetMutationResult, AssetScope, AssetStatus, AssetView,
    BeginMaintenance, CreateAsset, MarkAssetDegraded, PlaceAssetInService, RetireAsset,
    ReturnAssetToService,
};
use erpx_cqrs::{
    CONTRACT_VERSION, CommandEnvelope, CommandReceipt, IdempotencyKey, ProjectionTarget,
    QueryConsistency, QueryEnvelope, ReadYourWrites,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, QueryId, SessionId,
    TenantScope,
};
use erpx_maintenance::{
    BlockWorkOrder, CancelWorkOrder, CompleteWorkOrder, Downtime, RecordDowntime, ResumeWorkOrder,
    ScheduleWorkOrder, StartWorkOrder, WorkOrderListFilter, WorkOrderStatus, WorkOrderView,
};
use erpx_organization::{
    ListOrganizationNodes, NodeListFilter, NodeView, OrganizationNodeKind, OrganizationNodeStatus,
    OrganizationPlacement,
};
use erpx_ui_core::{Locale, SessionController, use_locale};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError, UiErrorKind};
use erpx_ui_kit::{
    SelectControl as KitSelect, Table as KitTable,
    button::{Button as KitButton, ButtonSize, ButtonVariant},
    input::Input as KitInput,
};
use uuid::Uuid;

use crate::product_completion::ProductContext;
use crate::transport::{typed_cqrs::GrpcChannel, wave4_5, wave6_7};

#[cfg(not(target_arch = "wasm32"))]
use crate::transport::GRPC_ENDPOINT;
#[cfg(target_arch = "wasm32")]
use crate::transport::grpc_endpoint;

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkspaceCredentials {
    access_token: String,
    principal_ref: String,
    session_id: String,
    scope_key: String,
    scope: TenantScope,
}

#[derive(Debug, Clone, PartialEq)]
enum WorkspaceRows {
    Assets(Vec<AssetView>),
    WorkOrders(Vec<WorkOrderView>),
}

#[derive(Debug, Clone, PartialEq)]
struct WorkspaceSnapshot {
    scope: TenantScope,
    companies: Vec<NodeView>,
    branches: Vec<NodeView>,
    rows: WorkspaceRows,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkspaceError {
    kind: UiErrorKind,
    code: String,
}

impl From<UiError> for WorkspaceError {
    fn from(value: UiError) -> Self {
        Self {
            kind: value.kind(),
            code: value.code().to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
enum AssetWorkspaceCommand {
    Create(CreateAsset),
    PlaceInService(PlaceAssetInService),
    MarkDegraded(MarkAssetDegraded),
    BeginMaintenance(BeginMaintenance),
    ReturnToService(ReturnAssetToService),
    Retire(RetireAsset),
}

#[derive(Debug, Clone)]
struct AssetWorkspaceAction {
    expected_version: Option<u64>,
    command: AssetWorkspaceCommand,
}

#[derive(Debug, Clone)]
enum MaintenanceWorkspaceCommand {
    Schedule(ScheduleWorkOrder),
    Start(StartWorkOrder),
    Block(BlockWorkOrder),
    Resume(ResumeWorkOrder),
    Complete(CompleteWorkOrder),
    Cancel(CancelWorkOrder),
    RecordDowntime(RecordDowntime),
}

#[derive(Debug, Clone)]
struct WorkspaceMutation<T> {
    value: T,
    target: Option<ProjectionTarget>,
    partition: String,
    replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingProjection {
    projection: String,
    partition: String,
    target: ProjectionTarget,
}

impl PendingProjection {
    fn consistency(&self) -> QueryConsistency {
        QueryConsistency::ReadYourWrites(ReadYourWrites {
            projection: self.projection.clone(),
            partition: self.partition.clone(),
            target: self.target,
            timeout_millis: 10_000,
        })
    }
}

fn copy(locale: Locale, en: &'static str, th: &'static str) -> &'static str {
    match locale {
        Locale::En => en,
        Locale::Th => th,
    }
}

fn scope_from_key(scope_key: &str) -> Result<TenantScope, WorkspaceError> {
    let invalid = || WorkspaceError {
        kind: UiErrorKind::Denied,
        code: "invalid_active_scope".into(),
    };
    let uuid = |value: &str| Uuid::parse_str(value).map_err(|_| invalid());
    let scope = if scope_key == "platform" {
        return Err(WorkspaceError {
            kind: UiErrorKind::Denied,
            code: "platform_scope_not_supported".into(),
        });
    } else if let Some(value) = scope_key.strip_prefix("holding:") {
        TenantScope::Holding {
            holding_id: uuid(value)?,
        }
    } else if let Some(value) = scope_key.strip_prefix("company:") {
        let parts = value.split(':').collect::<Vec<_>>();
        if parts.len() != 2 {
            return Err(invalid());
        }
        TenantScope::Company {
            holding_id: uuid(parts[0])?,
            company_id: uuid(parts[1])?,
        }
    } else if let Some(value) = scope_key.strip_prefix("branch:") {
        let parts = value.split(':').collect::<Vec<_>>();
        if parts.len() != 3 {
            return Err(invalid());
        }
        TenantScope::Branch {
            holding_id: uuid(parts[0])?,
            company_id: uuid(parts[1])?,
            branch_id: uuid(parts[2])?,
        }
    } else if let Some(value) = scope_key.strip_prefix("subtree:") {
        let (holding, descendants) = value.split_once(":companies=").ok_or_else(invalid)?;
        let (companies, branches) = descendants.split_once(":branches=").ok_or_else(invalid)?;
        let parse_ids = |values: &str| -> Result<Vec<Uuid>, WorkspaceError> {
            if values.is_empty() {
                Ok(Vec::new())
            } else {
                values.split(',').map(uuid).collect()
            }
        };
        TenantScope::Subtree {
            holding_id: uuid(holding)?,
            company_ids: parse_ids(companies)?,
            branch_ids: parse_ids(branches)?,
        }
    } else {
        return Err(invalid());
    };
    scope.validate().map_err(|_| invalid())?;
    Ok(scope)
}

fn credentials(auth: Signal<SessionController>) -> Result<WorkspaceCredentials, WorkspaceError> {
    let controller = auth.read();
    let access_token = controller
        .request_credential()
        .map(|value| value.as_str().to_owned())
        .ok_or_else(|| WorkspaceError {
            kind: UiErrorKind::Denied,
            code: "authentication_required".into(),
        })?;
    let principal_ref = controller
        .metadata()
        .and_then(|value| value.principal_label.clone())
        .ok_or_else(|| WorkspaceError {
            kind: UiErrorKind::Denied,
            code: "authentication_required".into(),
        })?;
    let session_id = controller
        .session_id()
        .map(|value| value.as_str().to_owned())
        .ok_or_else(|| WorkspaceError {
            kind: UiErrorKind::Denied,
            code: "authentication_required".into(),
        })?;
    let scope_key = controller
        .metadata()
        .and_then(|value| value.scope_label.clone())
        .ok_or_else(|| WorkspaceError {
            kind: UiErrorKind::Denied,
            code: "active_role_scope_missing".into(),
        })?;
    let scope = scope_from_key(&scope_key)?;
    Ok(WorkspaceCredentials {
        access_token,
        principal_ref,
        session_id,
        scope_key,
        scope,
    })
}

fn actor(value: &WorkspaceCredentials) -> Result<ActorAttribution, WorkspaceError> {
    let session = Uuid::parse_str(&value.session_id).map_err(|_| WorkspaceError {
        kind: UiErrorKind::Denied,
        code: "invalid_session".into(),
    })?;
    Ok(ActorAttribution {
        principal_ref: value.principal_ref.clone(),
        kind: ActorKind::TenantUser,
        session_id: Some(
            SessionId::try_from_uuid(session).map_err(|_| WorkspaceError {
                kind: UiErrorKind::Denied,
                code: "invalid_session".into(),
            })?,
        ),
        represented_principal_ref: None,
        support_capability_id: None,
    })
}

fn query<Q>(
    value: &WorkspaceCredentials,
    module: &'static str,
    consistency: QueryConsistency,
    query: Q,
) -> Result<QueryEnvelope<Q>, WorkspaceError> {
    Ok(QueryEnvelope {
        contract_version: CONTRACT_VERSION,
        query_id: QueryId::try_from_uuid(Uuid::new_v4()).expect("v4 UUID is non-nil"),
        correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4()).expect("v4 UUID is non-nil"),
        actor: actor(value)?,
        tenant_scope: value.scope.clone(),
        audit: AuditContext {
            purpose: format!("{module}_workspace_read"),
            customer_visible: false,
            module: Some(module.into()),
        },
        consistency,
        query,
    })
}

fn command<C>(
    value: &WorkspaceCredentials,
    module: &'static str,
    expected_version: Option<u64>,
    command: C,
) -> Result<CommandEnvelope<C>, WorkspaceError> {
    let command_id = Uuid::new_v4();
    Ok(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(command_id).expect("v4 UUID is non-nil"),
        idempotency_key: IdempotencyKey::try_new(format!("{module}-workspace-{command_id}"))
            .expect("generated idempotency key is bounded"),
        correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4()).expect("v4 UUID is non-nil"),
        causation_id: None,
        actor: actor(value)?,
        tenant_scope: value.scope.clone(),
        audit: AuditContext {
            purpose: format!("{module}_workspace_command"),
            customer_visible: true,
            module: Some(module.into()),
        },
        expected_aggregate_version: expected_version,
        command,
    })
}

async fn channel() -> Result<GrpcChannel, WorkspaceError> {
    #[cfg(target_arch = "wasm32")]
    {
        Ok(tonic_web_wasm_client::Client::new(grpc_endpoint()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        tonic::transport::Endpoint::from_static(GRPC_ENDPOINT)
            .connect()
            .await
            .map_err(|_| WorkspaceError {
                kind: UiErrorKind::Offline,
                code: "transport_unavailable".into(),
            })
    }
}

async fn load_organization_nodes(
    value: &WorkspaceCredentials,
    channel: GrpcChannel,
) -> Result<Vec<NodeView>, WorkspaceError> {
    let gateway = crate::features::organization::gateway::OrganizationGateway::new(
        wave4_5::OrganizationGrpcTransport::try_active_scope(channel, &value.access_token)?,
    );
    let mut nodes = Vec::new();
    let mut cursor = None;
    for _ in 0..10 {
        let result = QueryGateway::<ListOrganizationNodes>::execute(
            &gateway,
            query(
                value,
                "organization",
                QueryConsistency::Eventual,
                ListOrganizationNodes {
                    filter: NodeListFilter {
                        kind: None,
                        status: Some(OrganizationNodeStatus::Active),
                        parent_id: None,
                        page_size: 100,
                        cursor,
                    },
                },
            )?,
        )
        .await?;
        nodes.extend(result.value.nodes);
        cursor = result.value.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok(nodes)
}

async fn load_workspace(
    context: ProductContext,
    value: WorkspaceCredentials,
    pending: Option<PendingProjection>,
) -> Result<WorkspaceSnapshot, WorkspaceError> {
    let transport_channel = channel().await?;
    let nodes = load_organization_nodes(&value, transport_channel.clone())
        .await
        .map_err(|mut error| {
            error.code = format!("organization.{}", error.code);
            error
        })?;
    let companies = nodes
        .iter()
        .filter(|node| node.kind == OrganizationNodeKind::Company)
        .cloned()
        .collect();
    let branches = nodes
        .iter()
        .filter(|node| node.kind == OrganizationNodeKind::Branch)
        .cloned()
        .collect();
    let rows = if context == ProductContext::Assets {
        let gateway = crate::features::assets::gateway::AssetsGateway::new(
            wave6_7::AssetsGrpcTransport::try_active_scope(
                transport_channel,
                value.access_token.clone(),
            )?,
        );
        let consistency = pending
            .filter(|value| value.projection == "assets.assets")
            .map_or(QueryConsistency::Eventual, |value| value.consistency());
        let result = QueryGateway::<erpx_assets::ListAssets>::execute(
            &gateway,
            query(
                &value,
                "assets",
                consistency,
                erpx_assets::ListAssets {
                    filter: AssetListFilter {
                        status: None,
                        location_id: None,
                        page_size: 100,
                        cursor: None,
                    },
                },
            )?,
        )
        .await
        .map_err(|error| WorkspaceError {
            kind: error.kind(),
            code: format!("assets.{}", error.code()),
        })?;
        WorkspaceRows::Assets(result.value.assets)
    } else {
        let gateway = crate::features::maintenance::gateway::MaintenanceGateway::new(
            wave6_7::MaintenanceGrpcTransport::try_active_scope(
                transport_channel,
                value.access_token.clone(),
            )?,
        );
        let consistency = pending
            .filter(|value| value.projection == "maintenance.work_orders")
            .map_or(QueryConsistency::Eventual, |value| value.consistency());
        let result = QueryGateway::<erpx_maintenance::ListWorkOrders>::execute(
            &gateway,
            query(
                &value,
                "maintenance",
                consistency,
                erpx_maintenance::ListWorkOrders {
                    filter: WorkOrderListFilter {
                        status: None,
                        asset_id: None,
                        page_size: 100,
                        cursor: None,
                    },
                },
            )?,
        )
        .await
        .map_err(|error| WorkspaceError {
            kind: error.kind(),
            code: format!("maintenance.{}", error.code()),
        })?;
        WorkspaceRows::WorkOrders(result.value.work_orders)
    };
    Ok(WorkspaceSnapshot {
        scope: value.scope,
        companies,
        branches,
        rows,
    })
}

fn asset_partition(id: AssetId) -> String {
    format!("assets.asset:{}", id.as_uuid().simple())
}

fn work_order_partition(id: erpx_maintenance::WorkOrderId) -> String {
    format!("maintenance.work_order:{}", id.as_uuid().simple())
}

fn mutation<R, T>(receipt: CommandReceipt<R>, value: T, partition: String) -> WorkspaceMutation<T> {
    WorkspaceMutation {
        target: receipt.projection_target(),
        replayed: receipt.replayed,
        value,
        partition,
    }
}

async fn execute_asset(
    value: WorkspaceCredentials,
    expected_version: Option<u64>,
    input: AssetWorkspaceCommand,
) -> Result<WorkspaceMutation<AssetView>, WorkspaceError> {
    let gateway = crate::features::assets::gateway::AssetsGateway::new(
        wave6_7::AssetsGrpcTransport::try_active_scope(channel().await?, &value.access_token)?,
    );
    match input {
        AssetWorkspaceCommand::Create(input) => {
            let id = input.asset_id;
            let receipt = CommandGateway::<CreateAsset>::execute(
                &gateway,
                command(&value, "assets", None, input)?,
            )
            .await?;
            let asset = receipt.response.asset.clone();
            Ok(mutation(receipt, asset, asset_partition(id)))
        }
        AssetWorkspaceCommand::PlaceInService(input) => {
            execute_asset_mutation(&gateway, &value, expected_version, input.asset_id, input).await
        }
        AssetWorkspaceCommand::MarkDegraded(input) => {
            execute_asset_mutation(&gateway, &value, expected_version, input.asset_id, input).await
        }
        AssetWorkspaceCommand::BeginMaintenance(input) => {
            execute_asset_mutation(&gateway, &value, expected_version, input.asset_id, input).await
        }
        AssetWorkspaceCommand::ReturnToService(input) => {
            execute_asset_mutation(&gateway, &value, expected_version, input.asset_id, input).await
        }
        AssetWorkspaceCommand::Retire(input) => {
            execute_asset_mutation(&gateway, &value, expected_version, input.asset_id, input).await
        }
    }
}

async fn execute_asset_mutation<C>(
    gateway: &crate::features::assets::gateway::AssetsGateway<wave6_7::AssetsGrpcTransport>,
    value: &WorkspaceCredentials,
    expected_version: Option<u64>,
    asset_id: AssetId,
    input: C,
) -> Result<WorkspaceMutation<AssetView>, WorkspaceError>
where
    crate::features::assets::gateway::AssetsGateway<wave6_7::AssetsGrpcTransport>:
        CommandGateway<C, Response = AssetMutationResult>,
{
    let current = expected_version.ok_or_else(|| WorkspaceError {
        kind: UiErrorKind::Conflict,
        code: "asset_version_missing".into(),
    })?;
    let receipt =
        CommandGateway::<C>::execute(gateway, command(value, "assets", Some(current), input)?)
            .await?;
    let asset = receipt.response.asset.clone();
    Ok(mutation(receipt, asset, asset_partition(asset_id)))
}

async fn execute_maintenance(
    value: WorkspaceCredentials,
    expected_version: u64,
    input: MaintenanceWorkspaceCommand,
) -> Result<WorkspaceMutation<WorkOrderView>, WorkspaceError> {
    let gateway = crate::features::maintenance::gateway::MaintenanceGateway::new(
        wave6_7::MaintenanceGrpcTransport::try_active_scope(channel().await?, &value.access_token)?,
    );
    macro_rules! execute {
        ($command:expr, $type:ty, $id:expr) => {{
            let receipt = CommandGateway::<$type>::execute(
                &gateway,
                command(&value, "maintenance", Some(expected_version), $command)?,
            )
            .await?;
            let work_order = receipt.response.work_order.clone();
            Ok(mutation(receipt, work_order, work_order_partition($id)))
        }};
    }
    match input {
        MaintenanceWorkspaceCommand::Schedule(value) => {
            let id = value.work_order_id;
            execute!(value, ScheduleWorkOrder, id)
        }
        MaintenanceWorkspaceCommand::Start(value) => {
            let id = value.work_order_id;
            execute!(value, StartWorkOrder, id)
        }
        MaintenanceWorkspaceCommand::Block(value) => {
            let id = value.work_order_id;
            execute!(value, BlockWorkOrder, id)
        }
        MaintenanceWorkspaceCommand::Resume(value) => {
            let id = value.work_order_id;
            execute!(value, ResumeWorkOrder, id)
        }
        MaintenanceWorkspaceCommand::Complete(value) => {
            let id = value.work_order_id;
            execute!(value, CompleteWorkOrder, id)
        }
        MaintenanceWorkspaceCommand::Cancel(value) => {
            let id = value.work_order_id;
            execute!(value, CancelWorkOrder, id)
        }
        MaintenanceWorkspaceCommand::RecordDowntime(value) => {
            let id = value.work_order_id;
            execute!(value, RecordDowntime, id)
        }
    }
}

fn company_id(node: &NodeView) -> Option<Uuid> {
    match node.placement {
        OrganizationPlacement::Company { .. } => Some(node.node_id.as_uuid()),
        OrganizationPlacement::Branch { company_id, .. }
        | OrganizationPlacement::Headquarters { company_id, .. }
        | OrganizationPlacement::Department { company_id, .. }
        | OrganizationPlacement::Team { company_id, .. } => Some(company_id),
        OrganizationPlacement::Holding => None,
    }
}

fn target_scope(
    authority: &TenantScope,
    companies: &[NodeView],
    branches: &[NodeView],
    selected_company: &str,
    selected_branch: &str,
) -> Option<(AssetScope, Uuid)> {
    let holding_id = match authority {
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => *holding_id,
        TenantScope::Platform => return None,
    };
    let fixed_company = match authority {
        TenantScope::Company { company_id, .. } | TenantScope::Branch { company_id, .. } => {
            Some(*company_id)
        }
        _ => None,
    };
    let fixed_branch = match authority {
        TenantScope::Branch { branch_id, .. } => Some(*branch_id),
        _ => None,
    };
    let company = fixed_company
        .or_else(|| Uuid::parse_str(selected_company).ok())
        .or_else(|| companies.first().map(|node| node.node_id.as_uuid()))?;
    let branch = fixed_branch.or_else(|| {
        Uuid::parse_str(selected_branch).ok().filter(|id| {
            branches
                .iter()
                .any(|node| node.node_id.as_uuid() == *id && company_id(node) == Some(company))
        })
    });
    let tenant_scope = branch.map_or(
        TenantScope::Company {
            holding_id,
            company_id: company,
        },
        |branch_id| TenantScope::Branch {
            holding_id,
            company_id: company,
            branch_id,
        },
    );
    if !authority.contains(&tenant_scope) {
        return None;
    }
    let location = branch.unwrap_or(company);
    AssetScope::try_new(tenant_scope)
        .ok()
        .map(|scope| (scope, location))
}

fn parse_local_datetime(value: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M")
        .ok()
        .map(|value| value.and_utc())
}

fn default_local_datetime(offset_minutes: i64) -> String {
    (Utc::now() + Duration::minutes(offset_minutes))
        .format("%Y-%m-%dT%H:%M")
        .to_string()
}

fn error_copy(locale: Locale, error: &WorkspaceError) -> (&'static str, &'static str) {
    match error.kind {
        UiErrorKind::Denied => (
            copy(
                locale,
                "Workspace access unavailable",
                "ไม่สามารถเข้าถึงพื้นที่ทำงาน",
            ),
            copy(
                locale,
                "The active role has no usable Holding, Company, or Branch binding.",
                "บทบาทปัจจุบันไม่มีการผูก Holding, Company หรือ Branch ที่ใช้งานได้",
            ),
        ),
        UiErrorKind::Offline => (
            copy(locale, "You are offline", "การเชื่อมต่อขาดหาย"),
            copy(
                locale,
                "Reconnect and retry. No UUID fallback is required.",
                "เชื่อมต่อใหม่แล้วลองอีกครั้ง โดยไม่ต้องกรอก UUID สำรอง",
            ),
        ),
        UiErrorKind::Conflict => (
            copy(locale, "Data changed", "ข้อมูลมีการเปลี่ยนแปลง"),
            copy(
                locale,
                "Refresh the latest version, then retry the action.",
                "โหลดเวอร์ชันล่าสุดแล้วลองทำรายการอีกครั้ง",
            ),
        ),
        UiErrorKind::NotReady => (
            copy(locale, "Update is still processing", "รายการกำลังประมวลผล"),
            copy(
                locale,
                "The projection is catching up. Retry shortly.",
                "Projection กำลังตามข้อมูลล่าสุด โปรดลองอีกครั้งในอีกสักครู่",
            ),
        ),
        _ => (
            copy(locale, "Workspace unavailable", "พื้นที่ทำงานไม่พร้อมใช้งาน"),
            copy(
                locale,
                "The authorized read model could not be loaded.",
                "ไม่สามารถโหลด read model ที่ได้รับอนุญาตได้",
            ),
        ),
    }
}

#[component]
pub(crate) fn ActiveAssetMaintenanceSurface(
    context: ProductContext,
    auth: Signal<SessionController>,
) -> Element {
    let locale = use_locale().current();
    let mut selected_company = use_signal(String::new);
    let mut selected_branch = use_signal(String::new);
    let mut selected_detail = use_signal(String::new);
    let mut asset_code = use_signal(String::new);
    let mut asset_name = use_signal(String::new);
    let mut maintenance_title = use_signal(String::new);
    let mut schedule_for = use_signal(|| default_local_datetime(1));
    let mut downtime_started = use_signal(|| default_local_datetime(-60));
    let mut downtime_ended = use_signal(|| default_local_datetime(0));
    let mut downtime_reason = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut message = use_signal(|| None::<String>);
    let mut message_locale = use_signal(|| locale);
    let mut loaded_scope_key = use_signal(String::new);
    let mut pending_projection = use_signal(|| None::<PendingProjection>);

    let mut workspace = use_resource(move || {
        let credentials = credentials(auth);
        let pending = pending_projection.read().clone();
        async move {
            let credentials = credentials?;
            load_workspace(context, credentials, pending).await
        }
    });

    use_effect(move || {
        let key = auth
            .read()
            .metadata()
            .and_then(|value| value.scope_label.clone())
            .unwrap_or_default();
        if key != *loaded_scope_key.read() {
            loaded_scope_key.set(key);
            selected_company.set(String::new());
            selected_branch.set(String::new());
            selected_detail.set(String::new());
            message.set(None);
            busy.set(false);
            pending_projection.set(None);
        }
    });

    use_effect(move || {
        if *message_locale.read() != locale {
            message_locale.set(locale);
            message.set(None);
        }
    });

    let snapshot = workspace.read().clone();
    let Some(result) = snapshot else {
        return rsx! {
            section { class: "aggregate-card active-workspace-state", aria_busy: "true",
                h2 { {copy(locale, "Loading authorized workspace…", "กำลังโหลดพื้นที่ทำงานตามสิทธิ์…")} }
                p { {copy(locale, "Resolving your active role and organization scope.", "กำลังตรวจสอบบทบาทและขอบเขตองค์กรที่ใช้งานอยู่")} }
            }
        };
    };
    let snapshot = match result {
        Ok(value) => value,
        Err(error) => {
            let (title, body) = error_copy(locale, &error);
            return rsx! {
                section { class: "aggregate-card active-workspace-state", role: "alert", "data-error-code": error.code,
                    h2 { "{title}" }
                    p { "{body}" }
                    KitButton { size: ButtonSize::Default, variant: ButtonVariant::Secondary, class: "button", r#type: "button", onclick: move |_| workspace.restart(), {copy(locale, "Retry", "ลองอีกครั้ง")} }
                }
            };
        }
    };

    let fixed_company = match snapshot.scope {
        TenantScope::Company { company_id, .. } | TenantScope::Branch { company_id, .. } => {
            Some(company_id)
        }
        _ => None,
    };
    let fixed_branch = match snapshot.scope {
        TenantScope::Branch { branch_id, .. } => Some(branch_id),
        _ => None,
    };
    let effective_company = fixed_company
        .or_else(|| Uuid::parse_str(selected_company.read().as_str()).ok())
        .or_else(|| {
            snapshot
                .companies
                .first()
                .map(|node| node.node_id.as_uuid())
        });
    let visible_branches = snapshot
        .branches
        .iter()
        .filter(|node| effective_company.is_some_and(|id| company_id(node) == Some(id)))
        .cloned()
        .collect::<Vec<_>>();
    let company_name = |id: Uuid| {
        snapshot
            .companies
            .iter()
            .find(|node| node.node_id.as_uuid() == id)
            .map(|node| node.name.clone())
            .unwrap_or_else(|| copy(locale, "Authorized company", "บริษัทตามสิทธิ์").into())
    };
    let branch_name = |id: Uuid| {
        snapshot
            .branches
            .iter()
            .find(|node| node.node_id.as_uuid() == id)
            .map(|node| node.name.clone())
            .unwrap_or_else(|| copy(locale, "Authorized branch", "สาขาตามสิทธิ์").into())
    };

    let on_asset = EventHandler::new(move |action: AssetWorkspaceAction| {
        if *busy.read() {
            return;
        }
        let credentials = credentials(auth);
        busy.set(true);
        message.set(Some(copy(locale, "Saving…", "กำลังบันทึก…").into()));
        let mut workspace = workspace;
        let mut busy = busy;
        let mut message = message;
        let mut selected_detail = selected_detail;
        let mut pending_projection = pending_projection;
        spawn(async move {
            let result = match credentials {
                Ok(credentials) => {
                    execute_asset(credentials, action.expected_version, action.command).await
                }
                Err(error) => Err(error),
            };
            busy.set(false);
            match result {
                Ok(receipt) => {
                    selected_detail.set(receipt.value.asset_id.as_uuid().to_string());
                    message.set(Some(if receipt.replayed {
                        copy(
                            locale,
                            "Already saved. The authorized view refreshes automatically.",
                            "บันทึกไว้แล้ว และมุมมองตามสิทธิ์จะรีเฟรชอัตโนมัติ",
                        )
                        .into()
                    } else {
                        copy(
                            locale,
                            "Saved. The authorized view refreshes automatically.",
                            "บันทึกแล้ว และมุมมองตามสิทธิ์จะรีเฟรชอัตโนมัติ",
                        )
                        .into()
                    }));
                    pending_projection.set(receipt.target.map(|target| PendingProjection {
                        projection: "assets.assets".into(),
                        partition: receipt.partition,
                        target,
                    }));
                    workspace.restart();
                }
                Err(error) => {
                    let (title, _) = error_copy(locale, &error);
                    message.set(Some(format!("{title} · {}", error.code)));
                    if matches!(error.kind, UiErrorKind::Conflict | UiErrorKind::NotReady) {
                        workspace.restart();
                    }
                }
            }
        });
    });

    let on_maintenance = EventHandler::new(
        move |(version, input): (u64, MaintenanceWorkspaceCommand)| {
            if *busy.read() {
                return;
            }
            let credentials = credentials(auth);
            busy.set(true);
            message.set(Some(copy(locale, "Saving…", "กำลังบันทึก…").into()));
            let mut workspace = workspace;
            let mut busy = busy;
            let mut message = message;
            let mut selected_detail = selected_detail;
            let mut pending_projection = pending_projection;
            spawn(async move {
                let result = match credentials {
                    Ok(credentials) => execute_maintenance(credentials, version, input).await,
                    Err(error) => Err(error),
                };
                busy.set(false);
                match result {
                    Ok(receipt) => {
                        selected_detail.set(receipt.value.work_order_id.as_uuid().to_string());
                        message.set(Some(if receipt.replayed {
                            copy(
                                locale,
                                "Already saved. The authorized view refreshes automatically.",
                                "บันทึกไว้แล้ว และมุมมองตามสิทธิ์จะรีเฟรชอัตโนมัติ",
                            )
                            .into()
                        } else {
                            copy(
                                locale,
                                "Saved. The authorized view refreshes automatically.",
                                "บันทึกแล้ว และมุมมองตามสิทธิ์จะรีเฟรชอัตโนมัติ",
                            )
                            .into()
                        }));
                        pending_projection.set(receipt.target.map(|target| PendingProjection {
                            projection: "maintenance.work_orders".into(),
                            partition: receipt.partition,
                            target,
                        }));
                        workspace.restart();
                    }
                    Err(error) => {
                        let (title, _) = error_copy(locale, &error);
                        message.set(Some(format!("{title} · {}", error.code)));
                        if matches!(error.kind, UiErrorKind::Conflict | UiErrorKind::NotReady) {
                            workspace.restart();
                        }
                    }
                }
            });
        },
    );

    rsx! {
        section { class: "active-asset-maintenance-workspace", "data-workspace-context": if context == ProductContext::Assets { "assets" } else { "maintenance" },
            div { class: "active-workspace-toolbar",
                div {
                    span { class: "eyebrow", {copy(locale, "ACTIVE ROLE SCOPE", "ขอบเขตบทบาทที่ใช้งาน")} }
                    h2 { if context == ProductContext::Assets { {copy(locale, "Assets", "ทรัพย์สิน")} } else { {copy(locale, "Work orders", "ใบงานซ่อมบำรุง")} } }
                    p { {copy(locale, "Loaded automatically from the server-authorized role scope.", "โหลดอัตโนมัติจากขอบเขตบทบาทที่เซิร์ฟเวอร์อนุญาต")} }
                }
                KitButton { size: ButtonSize::Default, variant: ButtonVariant::Secondary, class: "button", r#type: "button", disabled: *busy.read(), onclick: move |_| workspace.restart(), {copy(locale, "Refresh", "รีเฟรช")} }
            }

            if let Some(message) = message.read().clone() {
                div { class: "status-banner info", role: "status", "{message}" }
            }

            if context == ProductContext::Assets {
                section { class: "aggregate-card active-asset-create",
                    h3 { {copy(locale, "Create asset", "สร้างทรัพย์สิน")} }
                    div { class: "active-workspace-form",
                        if fixed_company.is_none() {
                            label {
                                span { {copy(locale, "Company", "บริษัท")} }
                                KitSelect {
                                    value: effective_company.map(|id| id.to_string()).unwrap_or_default(),
                                    onchange: move |event: FormEvent| {
                                        selected_company.set(event.value());
                                        selected_branch.set(String::new());
                                    },
                                    for company in &snapshot.companies {
                                        option { value: company.node_id.as_uuid().to_string(), {format!("{} · {}", company.code, company.name)} }
                                    }
                                }
                            }
                        } else if let Some(company) = fixed_company {
                            div { class: "active-workspace-locked-field",
                                span { {copy(locale, "Company", "บริษัท")} }
                                strong { {company_name(company)} }
                            }
                        }
                        if fixed_branch.is_none() {
                            label {
                                span { {copy(locale, "Branch / location", "สาขา / ตำแหน่ง")} }
                                KitSelect {
                                    value: selected_branch.read().clone(),
                                    onchange: move |event: FormEvent| selected_branch.set(event.value()),
                                    option { value: "", {copy(locale, "Company-wide", "ทั้งบริษัท")} }
                                    for branch in &visible_branches {
                                        option { value: branch.node_id.as_uuid().to_string(), {format!("{} · {}", branch.code, branch.name)} }
                                    }
                                }
                            }
                        } else if let Some(branch) = fixed_branch {
                            div { class: "active-workspace-locked-field",
                                span { {copy(locale, "Branch / location", "สาขา / ตำแหน่ง")} }
                                strong { {branch_name(branch)} }
                            }
                        }
                        label {
                            span { {copy(locale, "Asset code", "รหัสทรัพย์สิน")} }
                            KitInput { value: asset_code.read().clone(), oninput: move |event: FormEvent| asset_code.set(event.value()), placeholder: "AST-001" }
                        }
                        label {
                            span { {copy(locale, "Asset name", "ชื่อทรัพย์สิน")} }
                            KitInput { value: asset_name.read().clone(), oninput: move |event: FormEvent| asset_name.set(event.value()), placeholder: copy(locale, "e.g. Packing line", "เช่น สายพานบรรจุ") }
                        }
                    }
                    KitButton {
                        size: ButtonSize::Default,
                        variant: ButtonVariant::Primary,
                        class: "button primary",
                        r#type: "button",
                        disabled: *busy.read() || asset_code.read().trim().is_empty() || asset_name.read().trim().is_empty() || target_scope(&snapshot.scope, &snapshot.companies, &snapshot.branches, selected_company.read().as_str(), selected_branch.read().as_str()).is_none(),
                        onclick: move |_| {
                            let Some((scope, location_id)) = target_scope(&snapshot.scope, &snapshot.companies, &snapshot.branches, selected_company.read().as_str(), selected_branch.read().as_str()) else { return; };
                            let asset_id = AssetId::try_from_uuid(Uuid::new_v4()).expect("v4 UUID is non-nil");
                            on_asset.call(AssetWorkspaceAction {
                                expected_version: None,
                                command: AssetWorkspaceCommand::Create(CreateAsset {
                                    asset_id,
                                    code: asset_code.read().trim().to_owned(),
                                    name: asset_name.read().trim().to_owned(),
                                    scope,
                                    location_id,
                                }),
                            });
                            asset_code.set(String::new());
                            asset_name.set(String::new());
                        },
                        {copy(locale, "Create asset", "สร้างทรัพย์สิน")}
                    }
                }
            }

            section { class: "aggregate-card active-workspace-list",
                match &snapshot.rows {
                    WorkspaceRows::Assets(rows) => rsx! {
                        h3 { {copy(locale, "Asset list", "รายการทรัพย์สิน")} }
                        if rows.is_empty() {
                            p { class: "empty-copy", {copy(locale, "No assets in this authorized scope yet.", "ยังไม่มีทรัพย์สินในขอบเขตที่ได้รับอนุญาตนี้")} }
                        } else {
                            div { class: "table-wrap",
                                KitTable {
                                    thead { tr { th { {copy(locale, "Code", "รหัส")} } th { {copy(locale, "Name", "ชื่อ")} } th { {copy(locale, "Status", "สถานะ")} } th { {copy(locale, "Condition", "สภาพ")} } } }
                                    tbody {
                                        for asset in rows {
                                            tr { class: if selected_detail.read().as_str() == asset.asset_id.as_uuid().to_string() { "selected" } else { "" },
                                                td { KitButton { size: ButtonSize::Default, variant: ButtonVariant::Link, class: "table-row-link", r#type: "button", onclick: { let id = asset.asset_id.as_uuid().to_string(); move |_| selected_detail.set(id.clone()) }, "{asset.code}" } }
                                                td { "{asset.name}" }
                                                td { {format!("{:?}", asset.status)} }
                                                td { {format!("{:?}", asset.condition)} }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    WorkspaceRows::WorkOrders(rows) => rsx! {
                        h3 { {copy(locale, "Work order list", "รายการใบงานซ่อมบำรุง")} }
                        if rows.is_empty() {
                            p { class: "empty-copy", {copy(locale, "No work orders yet. Request maintenance from an asset to open one automatically.", "ยังไม่มีใบงาน เลือกขอซ่อมจากทรัพย์สินเพื่อเปิดใบงานอัตโนมัติ")} }
                        } else {
                            div { class: "table-wrap",
                                KitTable {
                                    thead { tr { th { {copy(locale, "Work order", "ใบงาน")} } th { {copy(locale, "Title", "หัวข้อ")} } th { {copy(locale, "Asset", "ทรัพย์สิน")} } th { {copy(locale, "Status", "สถานะ")} } } }
                                    tbody {
                                        for work_order in rows {
                                            tr { class: if selected_detail.read().as_str() == work_order.work_order_id.as_uuid().to_string() { "selected" } else { "" },
                                                td { KitButton { size: ButtonSize::Default, variant: ButtonVariant::Link, class: "table-row-link", r#type: "button", onclick: { let id = work_order.work_order_id.as_uuid().to_string(); move |_| selected_detail.set(id.clone()) }, "{work_order.code}" } }
                                                td { "{work_order.title}" }
                                                td { {format!("{} · {}", work_order.asset_fact.asset_code.as_str(), work_order.asset_fact.name.as_str())} }
                                                td { {format!("{:?}", work_order.status)} }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                }
            }

            if let WorkspaceRows::Assets(rows) = &snapshot.rows {
                if let Some(asset) = rows.iter().find(|asset| selected_detail.read().as_str() == asset.asset_id.as_uuid().to_string()) {
                    section { class: "aggregate-card active-workspace-detail",
                        div { class: "active-detail-heading",
                            div { h3 { {format!("{} · {}", asset.code, asset.name)} } p { {format!("{:?} · {:?}", asset.status, asset.condition)} } }
                            KitButton { size: ButtonSize::IconSm, variant: ButtonVariant::Ghost, class: "icon-button", r#type: "button", aria_label: copy(locale, "Close details", "ปิดรายละเอียด"), onclick: move |_| selected_detail.set(String::new()), "×" }
                        }
                        div { class: "active-workspace-actions",
                            if asset.status == AssetStatus::Draft {
                                KitButton { size: ButtonSize::Default, variant: ButtonVariant::Secondary, class: "button", r#type: "button", disabled: *busy.read(), onclick: { let id = asset.asset_id; let version = asset.version; move |_| on_asset.call(AssetWorkspaceAction { expected_version: Some(version), command: AssetWorkspaceCommand::PlaceInService(PlaceAssetInService { asset_id: id }) }) }, {copy(locale, "Place in service", "เริ่มใช้งาน")} }
                            }
                            if asset.status == AssetStatus::InService {
                                KitButton { size: ButtonSize::Default, variant: ButtonVariant::Secondary, class: "button", r#type: "button", disabled: *busy.read(), onclick: { let id = asset.asset_id; let version = asset.version; move |_| on_asset.call(AssetWorkspaceAction { expected_version: Some(version), command: AssetWorkspaceCommand::MarkDegraded(MarkAssetDegraded { asset_id: id }) }) }, {copy(locale, "Mark degraded", "ทำเครื่องหมายเสื่อมสภาพ")} }
                                label { class: "maintenance-title-field",
                                    span { {copy(locale, "Maintenance title", "หัวข้อขอซ่อม")} }
                                    KitInput { value: maintenance_title.read().clone(), oninput: move |event: FormEvent| maintenance_title.set(event.value()), placeholder: copy(locale, "What needs attention?", "ต้องการให้ตรวจซ่อมอะไร?") }
                                }
                                KitButton { size: ButtonSize::Default, variant: ButtonVariant::Primary, class: "button primary", r#type: "button", disabled: *busy.read() || maintenance_title.read().trim().is_empty(), onclick: {
                                    let id = asset.asset_id;
                                    let version = asset.version;
                                    move |_| {
                                        let request_id = Uuid::new_v4();
                                        let request_code = format!("MR-{}", &request_id.simple().to_string()[..12]).to_ascii_uppercase();
                                        on_asset.call(AssetWorkspaceAction { expected_version: Some(version), command: AssetWorkspaceCommand::BeginMaintenance(BeginMaintenance { asset_id: id, request_code, title: maintenance_title.read().trim().to_owned() }) });
                                        maintenance_title.set(String::new());
                                    }
                                }, {copy(locale, "Request maintenance", "ขอซ่อม")} }
                            }
                            if asset.status == AssetStatus::UnderMaintenance {
                                KitButton { size: ButtonSize::Default, variant: ButtonVariant::Primary, class: "button primary", r#type: "button", disabled: *busy.read(), onclick: { let id = asset.asset_id; let version = asset.version; move |_| on_asset.call(AssetWorkspaceAction { expected_version: Some(version), command: AssetWorkspaceCommand::ReturnToService(ReturnAssetToService { asset_id: id }) }) }, {copy(locale, "Return to service", "คืนเข้าใช้งาน")} }
                            }
                            if asset.status != AssetStatus::Retired {
                                KitButton { size: ButtonSize::Default, variant: ButtonVariant::Destructive, class: "button danger", r#type: "button", disabled: *busy.read(), onclick: { let id = asset.asset_id; let version = asset.version; move |_| on_asset.call(AssetWorkspaceAction { expected_version: Some(version), command: AssetWorkspaceCommand::Retire(RetireAsset { asset_id: id }) }) }, {copy(locale, "Retire", "เลิกใช้งาน")} }
                            }
                        }
                    }
                }
            }

            if let WorkspaceRows::WorkOrders(rows) = &snapshot.rows {
                if let Some(work_order) = rows.iter().find(|work_order| selected_detail.read().as_str() == work_order.work_order_id.as_uuid().to_string()) {
                    section { class: "aggregate-card active-workspace-detail",
                        div { class: "active-detail-heading",
                            div { h3 { {format!("{} · {}", work_order.code, work_order.title)} } p { {format!("{} · {}", work_order.asset_fact.asset_code.as_str(), work_order.asset_fact.name.as_str())} } }
                            KitButton { size: ButtonSize::IconSm, variant: ButtonVariant::Ghost, class: "icon-button", r#type: "button", aria_label: copy(locale, "Close details", "ปิดรายละเอียด"), onclick: move |_| selected_detail.set(String::new()), "×" }
                        }
                        div { class: "active-workspace-actions",
                            if work_order.status == WorkOrderStatus::Planned {
                                label { span { {copy(locale, "Schedule for", "กำหนดเวลา")} } KitInput { r#type: "datetime-local", value: schedule_for.read().clone(), oninput: move |event: FormEvent| schedule_for.set(event.value()) } }
                                KitButton { size: ButtonSize::Default, variant: ButtonVariant::Primary, class: "button primary", r#type: "button", disabled: *busy.read() || parse_local_datetime(schedule_for.read().as_str()).is_none_or(|scheduled_for| scheduled_for <= Utc::now()), onclick: {
                                    let id = work_order.work_order_id; let version = work_order.version;
                                    move |_| if let Some(scheduled_for) = parse_local_datetime(schedule_for.read().as_str()) { on_maintenance.call((version, MaintenanceWorkspaceCommand::Schedule(ScheduleWorkOrder { work_order_id: id, scheduled_for }))); }
                                }, {copy(locale, "Schedule", "จัดตาราง")} }
                                KitButton { size: ButtonSize::Default, variant: ButtonVariant::Destructive, class: "button danger", r#type: "button", disabled: *busy.read(), onclick: { let id = work_order.work_order_id; let version = work_order.version; move |_| on_maintenance.call((version, MaintenanceWorkspaceCommand::Cancel(CancelWorkOrder { work_order_id: id }))) }, {copy(locale, "Cancel", "ยกเลิก")} }
                            }
                            if work_order.status == WorkOrderStatus::Scheduled {
                                KitButton { size: ButtonSize::Default, variant: ButtonVariant::Primary, class: "button primary", r#type: "button", disabled: *busy.read() || work_order.scheduled_for.is_none_or(|scheduled_for| scheduled_for > Utc::now()), onclick: { let id = work_order.work_order_id; let version = work_order.version; move |_| on_maintenance.call((version, MaintenanceWorkspaceCommand::Start(StartWorkOrder { work_order_id: id }))) }, {copy(locale, "Start", "เริ่มงาน")} }
                                KitButton { size: ButtonSize::Default, variant: ButtonVariant::Destructive, class: "button danger", r#type: "button", disabled: *busy.read(), onclick: { let id = work_order.work_order_id; let version = work_order.version; move |_| on_maintenance.call((version, MaintenanceWorkspaceCommand::Cancel(CancelWorkOrder { work_order_id: id }))) }, {copy(locale, "Cancel", "ยกเลิก")} }
                            }
                            if work_order.status == WorkOrderStatus::InProgress {
                                KitButton { size: ButtonSize::Default, variant: ButtonVariant::Secondary, class: "button", r#type: "button", disabled: *busy.read(), onclick: { let id = work_order.work_order_id; let version = work_order.version; move |_| on_maintenance.call((version, MaintenanceWorkspaceCommand::Block(BlockWorkOrder { work_order_id: id }))) }, {copy(locale, "Block", "พักงาน")} }
                                KitButton { size: ButtonSize::Default, variant: ButtonVariant::Primary, class: "button primary", r#type: "button", disabled: *busy.read(), onclick: { let id = work_order.work_order_id; let version = work_order.version; move |_| on_maintenance.call((version, MaintenanceWorkspaceCommand::Complete(CompleteWorkOrder { work_order_id: id }))) }, {copy(locale, "Complete", "เสร็จงาน")} }
                            }
                            if work_order.status == WorkOrderStatus::Blocked {
                                KitButton { size: ButtonSize::Default, variant: ButtonVariant::Primary, class: "button primary", r#type: "button", disabled: *busy.read(), onclick: { let id = work_order.work_order_id; let version = work_order.version; move |_| on_maintenance.call((version, MaintenanceWorkspaceCommand::Resume(ResumeWorkOrder { work_order_id: id }))) }, {copy(locale, "Resume", "ทำงานต่อ")} }
                                KitButton { size: ButtonSize::Default, variant: ButtonVariant::Destructive, class: "button danger", r#type: "button", disabled: *busy.read(), onclick: { let id = work_order.work_order_id; let version = work_order.version; move |_| on_maintenance.call((version, MaintenanceWorkspaceCommand::Cancel(CancelWorkOrder { work_order_id: id }))) }, {copy(locale, "Cancel", "ยกเลิก")} }
                            }
                            if matches!(work_order.status, WorkOrderStatus::InProgress | WorkOrderStatus::Blocked) {
                                div { class: "downtime-form",
                                    label { span { {copy(locale, "Downtime started", "เริ่มหยุดทำงาน")} } KitInput { r#type: "datetime-local", value: downtime_started.read().clone(), oninput: move |event: FormEvent| downtime_started.set(event.value()) } }
                                    label { span { {copy(locale, "Downtime ended", "สิ้นสุดหยุดทำงาน")} } KitInput { r#type: "datetime-local", value: downtime_ended.read().clone(), oninput: move |event: FormEvent| downtime_ended.set(event.value()) } }
                                    label { span { {copy(locale, "Reason", "เหตุผล")} } KitInput { value: downtime_reason.read().clone(), oninput: move |event: FormEvent| downtime_reason.set(event.value()), placeholder: copy(locale, "Reason for downtime", "เหตุผลที่หยุดทำงาน") } }
                                    KitButton { size: ButtonSize::Default, variant: ButtonVariant::Secondary, class: "button", r#type: "button", disabled: *busy.read() || downtime_reason.read().trim().is_empty() || parse_local_datetime(downtime_started.read().as_str()).zip(parse_local_datetime(downtime_ended.read().as_str())).is_none(), onclick: {
                                        let id = work_order.work_order_id; let version = work_order.version;
                                        move |_| {
                                            let Some((started_at, ended_at)) = parse_local_datetime(downtime_started.read().as_str()).zip(parse_local_datetime(downtime_ended.read().as_str())) else { return; };
                                            on_maintenance.call((version, MaintenanceWorkspaceCommand::RecordDowntime(RecordDowntime { work_order_id: id, downtime: Downtime { downtime_id: Uuid::new_v4(), started_at, ended_at, reason: downtime_reason.read().trim().to_owned() } })));
                                            downtime_reason.set(String::new());
                                        }
                                    }, {copy(locale, "Record downtime", "บันทึกเวลาหยุดทำงาน")} }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_scope_parser_covers_tenant_scopes_and_rejects_platform() {
        let holding = Uuid::from_u128(1);
        let company = Uuid::from_u128(2);
        let branch = Uuid::from_u128(3);
        assert_eq!(
            scope_from_key(&format!("holding:{holding}")),
            Ok(TenantScope::Holding {
                holding_id: holding
            })
        );
        assert_eq!(
            scope_from_key(&format!("company:{holding}:{company}")),
            Ok(TenantScope::Company {
                holding_id: holding,
                company_id: company,
            })
        );
        assert_eq!(
            scope_from_key(&format!("branch:{holding}:{company}:{branch}")),
            Ok(TenantScope::Branch {
                holding_id: holding,
                company_id: company,
                branch_id: branch,
            })
        );
        assert!(scope_from_key("platform").is_err());
    }

    #[test]
    fn workspace_state_copy_is_localized_without_uuid_fallback() {
        for kind in [
            UiErrorKind::Denied,
            UiErrorKind::Offline,
            UiErrorKind::Conflict,
            UiErrorKind::NotReady,
        ] {
            let error = WorkspaceError {
                kind,
                code: "test".into(),
            };
            let english = error_copy(Locale::En, &error);
            let thai = error_copy(Locale::Th, &error);
            assert_ne!(english, thai);
            assert!(!english.0.to_ascii_lowercase().contains("uuid"));
            assert!(!thai.0.to_ascii_lowercase().contains("uuid"));
        }
        let offline = WorkspaceError {
            kind: UiErrorKind::Offline,
            code: "offline".into(),
        };
        assert!(
            error_copy(Locale::En, &offline)
                .1
                .contains("No UUID fallback")
        );
        assert!(
            error_copy(Locale::Th, &offline)
                .1
                .contains("ไม่ต้องกรอก UUID")
        );
    }
}
