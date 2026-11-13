//! Native Inventory generic stock-count save composition.
//!
//! The selected Account/Holding context, current IAM permission, Company
//! purchase admission, Inventory count source/effects and the Workflow command
//! ledger share the same serializable Holding unit. Count transitions keep the
//! existing Service Count native path or their retained adapter until a native
//! transition effect port exists; this mount owns draft create/revision only.
use crate::finance_journal_actor::command_actor;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue, lock_commercial_authority_sources,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest, WorkPermissionRequirement,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_inventory::public::counts::{
    CountSaveInput, CountSavePlan, apply_count_save, lock_count_sources, plan_count_save,
    read_count_save_source,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, IdentifierSource, RecordId,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandJson, PgCommandAuthority, PgCommandLedger,
    PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
};
use serde::Serialize;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

const COUNT_CAPABILITY: &str = "supply.counts";

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COUNT_VERSION_CHANGED",
        "The physical count changed. Reload its current state before submitting.",
    )
}

#[derive(Clone)]
pub struct InventoryCountCommand<C> {
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub holding: HoldingId,
    pub company: CompanyId,
    pub catalogue: Arc<CommercialCatalogue>,
    pub clock: Arc<C>,
}

#[derive(Serialize)]
struct CountSaveResult {
    id: String,
    status: String,
    version: i64,
}

impl<C: Clock + Send + Sync + 'static> InventoryCountCommand<C> {
    #[allow(clippy::too_many_arguments)]
    pub async fn save<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: Option<String>,
        input: CountSaveInput,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let record_id = id
            .clone()
            .unwrap_or_else(|| format!("count-{}", key.as_str()));
        if record_id.is_empty() || record_id.len() > 200 {
            return Err(BusinessError::invalid(
                "COUNT_INVALID",
                "Use a current stock Count command.",
            )
            .into());
        }
        let expected_company = self.company.clone();
        let company = self.company.clone();
        let clock = self.clock.clone();
        let operation = if id.is_some() {
            "inventory.count.update"
        } else {
            "inventory.count.create"
        }
        .to_owned();
        let request = WorkflowRequest {
            key: key.as_str().to_owned(),
            operation: operation.clone(),
            record_id: record_id.clone(),
            input: wire,
        };
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                workspace()?,
                WorkAuthorizationRequest {
                    permission: WorkPermissionRequirement::AnyOf(vec!["inventory.prepare".into()]),
                    module: Some("inventory".into()),
                    destination: operation,
                    api: true,
                },
                CountSources,
                move |c, current| {
                    let expected_company = expected_company.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let request = request.clone();
                    let input = input.clone();
                    let record_id = record_id.clone();
                    Box::pin(async move {
                        if current.selection().workspace().company_id != expected_company {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "INVENTORY_SCOPE_REQUIRED",
                                "Use a Company or Branch inventory assignment.",
                            )
                            .into());
                        }
                        current.require_permission("inventory.prepare")?;
                        let mut authority = Authority {
                            current,
                            request: &request,
                            clock: clock.as_ref(),
                            catalogue: current_package_catalogue()?,
                            purchase: None,
                        };
                        let mut effects = Effects {
                            current,
                            company: &company,
                            clock: clock.as_ref(),
                            request: &request,
                            input: &input,
                            record_id: &record_id,
                            plan: None,
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
struct CountSources;

impl HoldingWorkSourcePreparation for CountSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            // Ordered owner fences: Commercial -> Inventory.
            lock_commercial_authority_sources(c).await?;
            lock_count_sources(c).await?;
            Ok(())
        })
    }
}

struct Authority<'a, C> {
    current: &'a AuthorizedWorkContext,
    request: &'a WorkflowRequest,
    clock: &'a C,
    catalogue: CommercialCatalogue,
    purchase: Option<PurchaseWitness>,
}

impl<C: Clock + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        HoldingWorkSourcePreparation::prepare(&CountSources, c).await
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
        c: &mut PgConnection,
        actor: &CommandActor,
        _: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        self.current.require_permission("inventory.prepare")?;
        if &command_actor(self.current)? != actor {
            return Err(changed().into());
        }
        let now = self.clock.now();
        let purpose = match disposition {
            CommandDisposition::Replay => CapabilityPurpose::History,
            CommandDisposition::Execute => CapabilityPurpose::NewWork,
        };
        let mut reader = PgCompanyPurchaseReader::new(
            self.current.holding_id().clone(),
            actor.company_id.clone(),
            c,
            &self.catalogue,
        );
        if let Some(witness) = &self.purchase {
            reader.revalidate(witness, now).await?.require_allowed()?;
        } else {
            self.purchase = Some(
                reader
                    .inspect(
                        &PurchaseRequest {
                            capabilities: vec![COUNT_CAPABILITY.into()],
                            purpose,
                            mode: PackageEnforcementMode::Enforced,
                        },
                        now,
                    )
                    .await?
                    .require_allowed()?
                    .clone(),
            );
        }
        Ok(())
    }
}

struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    company: &'a CompanyId,
    clock: &'a C,
    request: &'a WorkflowRequest,
    input: &'a CountSaveInput,
    record_id: &'a str,
    plan: Option<CountSavePlan>,
}

impl<C: Clock + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        HoldingWorkSourcePreparation::prepare(&CountSources, c).await
    }

    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let workspace = self.current.selection().workspace();
        let product_ids: Vec<String> = self
            .input
            .lines
            .iter()
            .map(|line| line.product_id.clone())
            .collect();
        let source = read_count_save_source(
            c,
            self.company.as_str(),
            workspace.branch_id.as_ref().map(|id| id.as_str()),
            if self.request.operation == "inventory.count.update" {
                Some(self.record_id)
            } else {
                None
            },
            &self.input.warehouse_id,
            &product_ids,
        )
        .await?;
        let plan = plan_count_save(&source, self.input, self.record_id, self.company.as_str())?;
        let version = plan.version;
        let result = CommandJson::from_serializable(&CountSaveResult {
            id: self.record_id.to_owned(),
            status: "draft".into(),
            version,
        })?;
        self.plan = Some(plan);
        Ok(WorkflowOutput {
            version,
            result,
            event_scope: None,
        })
    }

    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let plan = self.plan.take().ok_or_else(changed)?;
        apply_count_save(
            c,
            &plan,
            self.input.expected_version,
            self.current.user_id().as_str(),
            self.clock.now().get(),
        )
        .await
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
