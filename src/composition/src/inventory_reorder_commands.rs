//! Native Inventory reorder-policy command composition.
//!
//! Selected Account/Holding context, current IAM permission, the reorder
//! policy/event effects and the Workflow command ledger share one serializable
//! Holding unit. Reorder points are Company inventory policy; no Commercial
//! purchase or Files evidence is composed here, matching the retained writer.
use crate::finance_journal_actor::command_actor;
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest, WorkPermissionRequirement,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_inventory::public::reorder_policies::{
    ReorderActionInput, ReorderSaveInput, apply_reorder_save,
    apply_reorder_transition, lock_reorder_sources, plan_save, plan_transition,
    read_reorder_save_source, read_reorder_transition_source,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, IdentifierSource, RecordId,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandJson, PgCommandAuthority, PgCommandLedger, PgOwnerCommandEffects,
    WorkflowOutput, WorkflowRequest,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "REORDER_POLICY_CHANGED",
        "The reorder policy changed. Refresh and review it before continuing.",
    )
}

fn policy_id(company: &str, warehouse: &str, product: &str) -> String {
    let digest = Sha256::digest(format!("{company}:{warehouse}:{product}").as_bytes());
    let hex: String = digest.iter().flat_map(|byte| {
        let pair = format!("{byte:02x}");
        pair.chars().collect::<Vec<_>>()
    }).collect();
    format!("reorder-{}", &hex[..32])
}

#[derive(Clone, Debug)]
enum ReorderWrite {
    Save(ReorderSaveInput),
    Transition(String, ReorderActionInput),
}

impl ReorderWrite {
    fn operation(&self) -> String {
        match self {
            Self::Save(_) => "inventory.reorder.propose".into(),
            Self::Transition(_, input) => input.action.operation(),
        }
    }

    fn permission(&self) -> &'static str {
        match self {
            Self::Transition(_, input) if input.action.review() => "inventory.approve",
            _ => "inventory.prepare",
        }
    }
}

#[derive(Clone)]
pub struct InventoryReorderCommand<C> {
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub holding: HoldingId,
    pub company: CompanyId,
    pub clock: Arc<C>,
}

#[derive(Serialize)]
struct ReorderResult {
    id: String,
    status: String,
    version: i64,
}

impl<C: Clock + Send + Sync + 'static> InventoryReorderCommand<C> {
    pub async fn save<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        input: ReorderSaveInput,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        self.run(host, proof, workspace, ReorderWrite::Save(input), wire, key)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn transition<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: String,
        input: ReorderActionInput,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        self.run(
            host,
            proof,
            workspace,
            ReorderWrite::Transition(id, input),
            wire,
            key,
        )
        .await
    }

    async fn run<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        write: ReorderWrite,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let record = match &write {
            ReorderWrite::Save(input) => policy_id(
                self.company.as_str(),
                &input.warehouse_id,
                &input.product_id,
            ),
            ReorderWrite::Transition(id, _) => id.clone(),
        };
        let expected_company = self.company.clone();
        let company = self.company.clone();
        let clock = self.clock.clone();
        let operation = write.operation();
        let permission = write.permission().to_owned();
        let request = WorkflowRequest {
            key: key.as_str().to_owned(),
            operation: operation.clone(),
            record_id: record.clone(),
            input: wire,
        };
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                workspace()?,
                WorkAuthorizationRequest {
                    permission: WorkPermissionRequirement::AnyOf(vec![permission.clone()]),
                    module: Some("inventory".into()),
                    destination: operation,
                    api: true,
                },
                ReorderSources,
                move |c, current| {
                    let expected_company = expected_company.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let write = write.clone();
                    let request = request.clone();
                    let record = record.clone();
                    let permission = permission.clone();
                    Box::pin(async move {
                        if current.selection().workspace().company_id != expected_company {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "INVENTORY_SCOPE_REQUIRED",
                                "Use a Company or Branch inventory assignment.",
                            )
                            .into());
                        }
                        if current.selection().workspace().department_id.is_some() {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "INVENTORY_SCOPE_REQUIRED",
                                "Use a Company or Branch inventory assignment.",
                            )
                            .into());
                        }
                        current.require_permission(&permission)?;
                        let mut authority = Authority {
                            current,
                            request: &request,
                            permission: &permission,
                        };
                        let mut effects = Effects {
                            current,
                            company: &company,
                            clock: clock.as_ref(),
                            request: &request,
                            record_id: &record,
                            write: &write,
                            prepared: None,
                        };
                        let mut ids = Ids;
                        let outcome = PgCommandLedger::new(
                            company.clone(),
                            c,
                            &mut authority,
                            clock.as_ref(),
                            &mut ids,
                        )
                        .execute(&request, &mut effects)
                        .await?;
                        Ok(outcome.result)
                    })
                },
            )
            .await
    }
}

#[derive(Clone)]
struct ReorderSources;

impl HoldingWorkSourcePreparation for ReorderSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_reorder_sources(c).await?;
            Ok(())
        })
    }
}

struct Authority<'a> {
    current: &'a AuthorizedWorkContext,
    request: &'a WorkflowRequest,
    permission: &'a str,
}

impl PgCommandAuthority for Authority<'_> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        HoldingWorkSourcePreparation::prepare(&ReorderSources, c).await
    }

    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        r: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if r.key != self.request.key
            || r.operation != self.request.operation
            || r.record_id != self.request.record_id
            || r.input.as_str() != self.request.input.as_str()
        {
            return Err(changed().into());
        }
        Ok(command_actor(self.current)?)
    }

    async fn verify_command(
        &mut self,
        _: &mut PgConnection,
        actor: &CommandActor,
        _: &WorkflowRequest,
        _: erpx_server_workflow::public::commands::CommandDisposition,
    ) -> Result<(), Error> {
        self.current.require_permission(self.permission)?;
        if &command_actor(self.current)? != actor {
            return Err(changed().into());
        }
        Ok(())
    }
}

enum Prepared {
    Save(erpx_server_inventory::public::reorder_policies::ReorderSavePlan),
    Transition(erpx_server_inventory::public::reorder_policies::ReorderTransitionPlan),
}

struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    company: &'a CompanyId,
    clock: &'a C,
    request: &'a WorkflowRequest,
    record_id: &'a str,
    write: &'a ReorderWrite,
    prepared: Option<Prepared>,
}

impl<C: Clock + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        HoldingWorkSourcePreparation::prepare(&ReorderSources, c).await
    }

    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let workspace = self.current.selection().workspace();
        let branch = workspace.branch_id.as_ref().map(|id| id.as_str());
        match self.write {
            ReorderWrite::Save(input) => {
                let source = read_reorder_save_source(
                    c,
                    self.company.as_str(),
                    branch,
                    self.record_id,
                    &input.warehouse_id,
                    &input.product_id,
                )
                .await?;
                let plan = plan_save(&source, input, self.company.as_str(), self.record_id)?;
                let version = plan.version;
                self.prepared = Some(Prepared::Save(plan));
                Ok(WorkflowOutput {
                    version,
                    result: CommandJson::from_serializable(&ReorderResult {
                        id: self.record_id.to_owned(),
                        status: "pending".into(),
                        version,
                    })?,
                    event_scope: None,
                })
            }
            ReorderWrite::Transition(id, input) => {
                let source =
                    read_reorder_transition_source(c, self.company.as_str(), branch, id).await?;
                let plan = plan_transition(
                    &source,
                    input,
                    self.current.user_id().as_str(),
                    branch,
                    self.current.has_permission("inventory.prepare"),
                    self.current.has_permission("inventory.approve"),
                )?;
                let version = plan.version;
                let status = plan.status;
                self.prepared = Some(Prepared::Transition(plan));
                Ok(WorkflowOutput {
                    version,
                    result: CommandJson::from_serializable(&ReorderResult {
                        id: id.clone(),
                        status: status.to_owned(),
                        version,
                    })?,
                    event_scope: None,
                })
            }
        }
    }

    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let now = self.clock.now().get();
        let actor = self.current.user_id().as_str().to_owned();
        match self.prepared.take().ok_or_else(changed)? {
            Prepared::Save(plan) => {
                apply_reorder_save(c, &plan, self.request.key.as_str(), &actor, now).await?;
            }
            Prepared::Transition(plan) => {
                apply_reorder_transition(c, &plan, self.request.key.as_str(), &actor, now).await?;
            }
        }
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
        Ok(())
    }
}

struct Ids;

impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}
