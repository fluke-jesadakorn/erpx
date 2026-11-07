//! Reference composition backend for the native Assets HTTP router.
//!
//! This file is the exact adapter the shared composition must provide: normal
//! Account session proof, current Holding/Company scope, the serializable owner
//! unit, Workflow receipt/audit/outbox retention and the Finance cost handoff.
use erpx_server_assets::public::asset_http::{
    AssetCommandResponse, AssetDetailResponse, AssetHttpBackend, AssetHttpCommandRequest,
    AssetHttpFuture, AssetHttpReadReply, AssetHttpReadRequest, AssetRenderedContext,
    AssetWorkspaceResponse,
};
use erpx_server_assets::public::asset_register::{AssetFuture, NewAsset};
use erpx_server_assets::public::asset_http::{AssetHttpCommand, decode_asset_command};
use erpx_server_assets::public::asset_register::{
    AcquisitionCostProvider, AssetActor, AssetChangeResult, NoCommandLedger, PgAssetRegister,
    asset_detail, asset_workspace, change_asset, create_asset_in_unit,
};
use erpx_server_identity_access::public::{
    AccountPolicy, RequestAudience,
    account_inbound_http::{CapturedHoldingMutation, parse_inbound_account_request},
    command_metadata::project_command_iam,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandEventScope, CommandField, CommandJson, WorkAuthority, command_fingerprint,
};
use serde::Serialize;
use serde_json::Value;
use sqlx::{PgConnection, Row};
use std::sync::Arc;

pub type AssetError = PgTransactionError<BusinessError>;

#[derive(Clone)]
pub struct AssetHttpConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl AssetHttpConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }
    pub fn company_id(&self) -> &CompanyId {
        &self.company
    }
}

#[derive(Clone, Copy)]
struct AssetSources;

impl HoldingWorkSourcePreparation for AssetSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

fn scope_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ASSETS_SCOPE_REQUIRED",
        "Choose the current authorized Assets workspace.",
    )
}

fn rendered_integer(value: i64) -> Result<i64, BusinessError> {
    if (0..=9_007_199_254_740_991).contains(&value) {
        Ok(value)
    } else {
        Err(BusinessError::new(
            FailureKind::Unavailable,
            "ASSETS_CONTEXT_UNAVAILABLE",
            "The current Assets work context is unavailable.",
        ))
    }
}

fn rendered(current: &AuthorizedWorkContext) -> Result<AssetRenderedContext, BusinessError> {
    let selection = current.selection();
    let workspace = selection.workspace();
    let grant = selection.assignment();
    Ok(AssetRenderedContext {
        user_id: current.user_id().as_str().into(),
        holding_id: current.holding_id().as_str().into(),
        company_id: workspace.company_id.as_str().into(),
        branch_id: workspace
            .branch_id
            .as_ref()
            .map(|value| value.as_str().into()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|value| value.as_str().into()),
        branch_slug: workspace.branch_slug.clone(),
        department_slug: workspace.department_slug.clone(),
        assignment_id: grant.id.as_str().into(),
        assignment_version: rendered_integer(grant.version)?,
        revision: rendered_integer(selection.revision())?,
        role: grant.role.clone(),
        assignment_scope_kind: match grant.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        modules: current.modules().to_vec(),
        permissions: current.command_iam_metadata().permissions.clone(),
        expires_at: rendered_integer(current.expires_at())?,
    })
}

fn command_actor(current: &AuthorizedWorkContext) -> Result<CommandActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(CommandActor {
        user_id: current.user_id().clone(),
        assignment_id: assignment.id.clone(),
        role: assignment.role.clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace
            .branch_id
            .as_ref()
            .map(|id| CommandField::Value(id.as_str().into()))
            .unwrap_or(CommandField::Null),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|id| CommandField::Value(id.as_str().into()))
            .unwrap_or(CommandField::Null),
        authority: CommandField::Value(WorkAuthority {
            session_id: current.session_id().as_str().into(),
            revision: selected.revision(),
            assignment_version: assignment.version,
        }),
        iam: CommandField::Value(CommandJson::from_serializable(&project_command_iam(
            current,
        )?)?),
        assignment_scope_kind: match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        assignment_scope_node_id: assignment
            .scope_node_id
            .as_ref()
            .map(|id| id.as_str().into()),
    })
}

fn asset_actor(current: &AuthorizedWorkContext) -> Result<AssetActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(AssetActor {
        user_id: current.user_id().clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
    })
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ASSET_COMMAND_RESULT_UNAVAILABLE",
        "The retained Asset command result is unavailable.",
    )
}

fn digest(
    actor: &CommandActor,
    operation: &str,
    record_id: &str,
    input: &Value,
) -> Result<String, BusinessError> {
    command_fingerprint(
        actor,
        operation,
        record_id,
        &CommandJson::from_serializable(input)?,
    )
}

async fn replay_asset_receipt(
    connection: &mut PgConnection,
    actor: &CommandActor,
    key: &str,
    operation: &str,
    record_id: &str,
    input: &Value,
) -> Result<Option<Value>, AssetError> {
    let row = sqlx::query(
        "SELECT operation,request_hash,result_json FROM command_receipts WHERE command_id=$1",
    )
    .bind(key)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let saved_operation: String = row.try_get("operation")?;
    let saved_hash: String = row.try_get("request_hash")?;
    if saved_operation != operation || saved_hash != digest(actor, operation, record_id, input)? {
        return Err(conflict(
            "IDEMPOTENCY_CONFLICT",
            "This command key belongs to a different request.",
        )
        .into());
    }
    let result: String = row.try_get("result_json")?;
    Ok(Some(
        serde_json::from_str::<Value>(&result).map_err(|_| unavailable())?,
    ))
}

struct Receipt<'a> {
    actor: &'a CommandActor,
    key: &'a str,
    operation: &'a str,
    record_id: &'a str,
    input: &'a Value,
    result: &'a AssetChangeResult,
    at: i64,
}

async fn retain_asset_receipt(
    connection: &mut PgConnection,
    receipt: Receipt<'_>,
) -> Result<(), AssetError> {
    let Receipt {
        actor,
        key,
        operation,
        record_id,
        input,
        result,
        at,
    } = receipt;
    if !(0..=9_007_199_254_740_991).contains(&result.version) {
        return Err(BusinessError::invalid(
            "COMMAND_OUTPUT_INVALID",
            "Provide a valid owner result version.",
        )
        .into());
    }
    let request_hash = digest(actor, operation, record_id, input)?;
    let stored = sqlx::query_scalar::<_, String>(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO UPDATE SET command_id=excluded.command_id RETURNING request_hash",
    )
    .bind(key)
    .bind(operation)
    .bind(actor.user_id.as_str())
    .bind(&request_hash)
    .bind(record_id)
    .bind(result.value().to_string())
    .bind(at)
    .bind(actor.receipt_authority()?.as_str())
    .fetch_one(&mut *connection)
    .await?;
    if stored != request_hash {
        return Err(conflict(
            "IDEMPOTENCY_CONFLICT",
            "This command key belongs to a different request.",
        )
        .into());
    }
    let record_type = operation.split('.').take(2).collect::<Vec<_>>().join("_");
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
         VALUES($1,$2,$3,$4,$2,$5,$6,$7,$2,$8,(SELECT authority_json FROM command_receipts WHERE command_id=$2))",
    )
    .bind(format!("asset-audit-{key}"))
    .bind(key)
    .bind(actor.user_id.as_str())
    .bind(actor.assignment_id.as_str())
    .bind(&record_type)
    .bind(record_id)
    .bind(result.version)
    .bind(at)
    .execute(&mut *connection)
    .await?;
    let scope = CommandEventScope::from(actor);
    let payload = actor.outbox_payload(
        record_id,
        result.version,
        Some(result.status.as_str()),
        &scope,
    )?;
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(format!("asset-outbox-{key}"))
    .bind(key)
    .bind(operation)
    .bind(&record_type)
    .bind(record_id)
    .bind(result.version)
    .bind(payload.as_str())
    .bind(at)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

fn prior_result(id: &str, prior: &Value) -> Result<AssetChangeResult, BusinessError> {
    let version = prior
        .get("version")
        .and_then(Value::as_i64)
        .filter(|value| (0..=9_007_199_254_740_991).contains(value))
        .ok_or_else(unavailable)?;
    let status = prior
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(unavailable)?;
    Ok(AssetChangeResult {
        id: id.to_owned(),
        version,
        status: status.to_owned(),
    })
}

/// Reference backend. Composition supplies the current Account policy, the
/// Holding authorizer, one exact Company target and the Finance cost port.
pub struct AssetHttpService<C> {
    policy: Arc<AccountPolicy>,
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: AssetHttpConfiguration,
    clock: Arc<C>,
    cost: Arc<dyn AcquisitionCostProvider>,
}

impl<C: Clock + Send + Sync + 'static> AssetHttpService<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: AssetHttpConfiguration,
        clock: Arc<C>,
        cost: Arc<dyn AcquisitionCostProvider>,
    ) -> Self {
        Self {
            policy,
            authorizer,
            configuration,
            clock,
            cost,
        }
    }

    fn access(write: bool) -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: if write {
                "records.write".into()
            } else {
                "records.read".into()
            },
            module: Some("assets".into()),
            destination: "services/assets".into(),
            api: true,
        }
    }

    fn scoped_input<Input>(
        &self,
        input: Input,
        mutation: bool,
    ) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company_id().clone();
        move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope_required());
            }
            request.mutation = mutation;
            Ok(request)
        }
    }

    async fn workspace(
        &self,
        host: &str,
        proof: erpx_server_identity_access::public::DeviceProof,
        input: impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    ) -> Result<AssetWorkspaceResponse, AssetError> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company_id().clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.scoped_input(input, false),
                Self::access(false),
                AssetSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_required().into());
                        }
                        let actor = asset_actor(current)?;
                        let data = asset_workspace(
                            &actor,
                            &mut PgAssetRegister::new(connection, actor.clone()),
                        )
                        .await?;
                        Ok(AssetWorkspaceResponse {
                            data,
                            context: rendered(current)?,
                        })
                    })
                },
            )
            .await
    }

    async fn detail(
        &self,
        host: &str,
        proof: erpx_server_identity_access::public::DeviceProof,
        input: impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        id: &str,
    ) -> Result<AssetDetailResponse, AssetError> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company_id().clone();
        let id = id.to_owned();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.scoped_input(input, false),
                Self::access(false),
                AssetSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let id = id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_required().into());
                        }
                        let actor = asset_actor(current)?;
                        let data = asset_detail(
                            &actor,
                            &mut PgAssetRegister::new(connection, actor.clone()),
                            &id,
                        )
                        .await?;
                        Ok(AssetDetailResponse {
                            data,
                            context: rendered(current)?,
                        })
                    })
                },
            )
            .await
    }

    async fn command_inner(
        &self,
        host: &str,
        proof: erpx_server_identity_access::public::DeviceProof,
        input: impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        raw: Value,
        key: CommandKey,
    ) -> Result<AssetCommandResponse, AssetError> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company_id().clone();
        let clock = self.clock.clone();
        let cost = self.cost.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.scoped_input(input, true),
                Self::access(true),
                AssetSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let cost = cost.clone();
                    let raw = raw.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_required().into());
                        }
                        let actor = asset_actor(current)?;
                        let command = command_actor(current)?;
                        let at = clock.now().get();
                        let parsed = decode_asset_command(&raw)?;
                        let (record_id, operation) = match &parsed {
                            AssetHttpCommand::Create => {
                                (format!("asset-{}", key.as_str()), "asset.create".to_owned())
                            }
                            AssetHttpCommand::Change { id, operation } => {
                                (id.clone(), operation.clone())
                            }
                        };
                        if let Some(prior) = replay_asset_receipt(
                            &mut *connection,
                            &command,
                            key.as_str(),
                            &operation,
                            &record_id,
                            &raw,
                        )
                        .await?
                        {
                            return Ok(AssetCommandResponse {
                                data: prior_result(&record_id, &prior)?,
                                context: rendered(current)?,
                            });
                        }
                        let mut ledger = NoCommandLedger::<AssetError>::default();
                        let outcome = {
                            let mut register = PgAssetRegister::new(connection, actor.clone())
                                .with_cost_provider(cost);
                            match parsed {
                                AssetHttpCommand::Create => {
                                    create_asset_in_unit(
                                        &actor,
                                        &mut register,
                                        &mut ledger,
                                        &raw,
                                        key.as_str(),
                                        at,
                                    )
                                    .await?
                                }
                                AssetHttpCommand::Change { .. } => {
                                    change_asset(
                                        &actor,
                                        &mut register,
                                        &mut ledger,
                                        &record_id,
                                        &raw,
                                        key.as_str(),
                                        at,
                                    )
                                    .await?
                                }
                            }
                        };
                        let receipt = outcome.receipt;
                        retain_asset_receipt(
                            connection,
                            Receipt {
                                actor: &command,
                                key: key.as_str(),
                                operation: &operation,
                                record_id: &record_id,
                                input: &raw,
                                result: &outcome.result,
                                at,
                            },
                        )
                        .await?;
                        if let Some(receipt) = receipt {
                            receipt.require_current(connection).await?;
                        }
                        Ok(AssetCommandResponse {
                            data: outcome.result,
                            context: rendered(current)?,
                        })
                    })
                },
            )
            .await
    }
}

#[derive(Clone, Copy, Default)]
pub struct AssetScopeHeaders;

impl AssetScopeHeaders {
    fn slugs(
        &self,
        headers: &axum::http::HeaderMap,
    ) -> Result<(Option<String>, Option<String>), BusinessError> {
        let slug = |key: &'static str| -> Result<Option<String>, BusinessError> {
            let mut values = headers.get_all(key).iter();
            let first = values.next();
            if values.next().is_some() {
                return Err(BusinessError::invalid(
                    "INVALID_WORKSPACE_SELECTOR",
                    "Use one exact workspace selector.",
                ));
            }
            first
                .map(|value| {
                    value
                        .to_str()
                        .ok()
                        .filter(|value| {
                            !value.is_empty()
                                && value.len() <= 128
                                && value.bytes().all(|byte| {
                                    byte.is_ascii_lowercase()
                                        || byte.is_ascii_digit()
                                        || byte == b'-'
                                        || byte == b'_'
                                })
                        })
                        .map(str::to_owned)
                        .ok_or_else(|| {
                            BusinessError::invalid(
                                "INVALID_WORKSPACE_SELECTOR",
                                "Use an exact workspace slug.",
                            )
                        })
                })
                .transpose()
        };
        Ok((
            slug("x-erpx-workspace-branch")?,
            slug("x-erpx-workspace-department")?,
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> AssetHttpBackend for AssetHttpService<C> {
    fn read<'a>(
        &'a self,
        request: AssetHttpReadRequest<'a>,
    ) -> AssetHttpFuture<'a, AssetHttpReadReply> {
        Box::pin(async move {
            let inbound = parse_inbound_account_request(&self.policy, request.headers)?;
            if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
                return Err(BusinessError::new(
                    FailureKind::NotFound,
                    "NOT_FOUND",
                    "Not found.",
                ));
            }
            let (branch, department) = AssetScopeHeaders.slugs(request.headers)?;
            let captured = CapturedHoldingMutation::capture(request.headers, branch, department);
            let proof = inbound.into_proof();
            let company = self.configuration.company_id().clone();
            match request.id {
                Some(id) => self
                    .detail(
                        request.host,
                        proof,
                        move || captured.resolve_read(company),
                        id,
                    )
                    .await
                    .map(|reply| AssetHttpReadReply::Detail(Box::new(reply)))
                    .map_err(unit_error),
                None => self
                    .workspace(request.host, proof, move || captured.resolve_read(company))
                    .await
                    .map(|reply| AssetHttpReadReply::Workspace(Box::new(reply)))
                    .map_err(unit_error),
            }
        })
    }

    fn command<'a>(
        &'a self,
        request: AssetHttpCommandRequest<'a>,
    ) -> AssetHttpFuture<'a, AssetCommandResponse> {
        Box::pin(async move {
            let inbound = parse_inbound_account_request(&self.policy, request.headers)?;
            if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
                return Err(BusinessError::new(
                    FailureKind::NotFound,
                    "NOT_FOUND",
                    "Not found.",
                ));
            }
            let (branch, department) = AssetScopeHeaders.slugs(request.headers)?;
            let captured = CapturedHoldingMutation::capture(request.headers, branch, department);
            self.command_inner(
                request.host,
                inbound.into_proof(),
                move || captured.resolve(),
                request.body.clone(),
                request.key.clone(),
            )
            .await
            .map_err(unit_error)
        })
    }
}

/// The composition backend can pass through the owner's typed business failure;
/// persistence failures become one bounded unavailable outcome.
fn unit_error(error: AssetError) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "ASSETS_UNAVAILABLE",
            "The physical Asset could not be read or changed.",
        ),
    }
}

#[allow(dead_code)]
fn _assert_backend_send_sync<C: Clock + Send + Sync + 'static>(value: Arc<AssetHttpService<C>>) {
    fn assert_send_sync<T: Send + Sync>(_: &T) {}
    assert_send_sync(&value);
}

#[allow(dead_code)]
fn _assert_serializable<T: Serialize>(_: &T) {}


/// Composition-level Finance acquisition-cost handoff. The Finance Money
/// snapshot producer is not yet ported, so a positive acquisition cost fails
/// closed instead of writing an unbacked asset; zero-cost assets remain
/// available.
#[derive(Clone, Copy)]
pub struct FinanceAssetCost;

impl AcquisitionCostProvider for FinanceAssetCost {
    fn prepare<'a>(
        &'a self,
        _: &'a mut PgConnection,
        _: &'a str,
        asset: &'a NewAsset,
    ) -> AssetFuture<'a, (), BusinessError> {
        Box::pin(async move {
            if asset.cost_minor != 0 {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "ASSET_ACQUISITION_COST_UNAVAILABLE",
                    "The Finance acquisition-cost handoff is unavailable for positive-cost assets.",
                ));
            }
            Ok(())
        })
    }
}
