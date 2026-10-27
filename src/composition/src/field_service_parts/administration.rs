//! Unmounted development-only guarded Field Service parts administration.
use super::{FieldServicePartsSourcePreparation, command::execute_field_service_parts};
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_field_service::public::parts::{
    FieldServiceWireFailure, normalize_field_service_parts,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{HoldingWorkAuthorizer, WorkAuthorizationRequest},
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, IdentifierSource, RecordId,
};
use erpx_server_workflow::public::commands::CommandOutcome;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub enum FieldServicePartsEnvironment {
    Development,
    Production,
}
#[derive(Clone)]
pub struct FieldServicePartsConfiguration {
    holding_id: HoldingId,
    company_id: CompanyId,
    mode: PackageEnforcementMode,
}
impl FieldServicePartsConfiguration {
    pub fn new(
        environment: FieldServicePartsEnvironment,
        holding_id: HoldingId,
        company_id: CompanyId,
        mode: PackageEnforcementMode,
    ) -> Result<Self, BusinessError> {
        if matches!(environment, FieldServicePartsEnvironment::Production) {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "FIELD_SERVICE_PRODUCTION_COMPOSITION_UNAVAILABLE",
                "Field Service production release is not available.",
            ));
        }
        Ok(Self {
            holding_id,
            company_id,
            mode,
        })
    }
}
#[derive(Debug)]
pub enum FieldServicePartsAdministrationError {
    Wire(FieldServiceWireFailure),
    Transaction(PgTransactionError<BusinessError>),
}
pub struct FieldServicePartsAdministration<W, G> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: FieldServicePartsConfiguration,
    write: Arc<W>,
    guard: Arc<G>,
}
impl<W: Clock + Send + Sync + 'static, G: Clock + Send + Sync + 'static>
    FieldServicePartsAdministration<W, G>
{
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: FieldServicePartsConfiguration,
        write: Arc<W>,
        guard: Arc<G>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            write,
            guard,
        }
    }
    pub async fn execute(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: HoldingWorkspaceRequest,
        order: String,
        body: Vec<u8>,
        key: String,
    ) -> Result<CommandOutcome, FieldServicePartsAdministrationError> {
        self.execute_deferred(host, proof, move || Ok(workspace), order, body, key)
            .await
    }
    pub async fn execute_deferred<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        order: String,
        body: Vec<u8>,
        key: String,
    ) -> Result<CommandOutcome, FieldServicePartsAdministrationError>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let config = self.configuration.clone();
        let expected_company = config.company_id.clone();
        let workspace = move || {
            let mut value = workspace()?;
            value.mutation = true;
            if value.company_id != expected_company {
                return Err(scope());
            }
            Ok(value)
        };
        let write = self.write.clone();
        let guard = self.guard.clone();
        let outcome = self
            .authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                workspace,
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("field_service".into()),
                    destination: "service/orders".into(),
                    api: true,
                },
                FieldServicePartsSourcePreparation,
                move |c, current| {
                    let config = config.clone();
                    let write = write.clone();
                    let guard = guard.clone();
                    let body = body.clone();
                    let order = order.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &config.holding_id
                            || current.selection().workspace().company_id != config.company_id
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        current.require_permission("records.write")?;
                        let normalized = match normalize_field_service_parts(&body) {
                            Ok(v) => v,
                            Err(e) => return Ok(Err(e)),
                        };
                        let command =
                            normalized.into_command(CommandKey::new(key)?, RecordId::new(order)?);
                        let mut ids = Ids;
                        Ok(Ok(execute_field_service_parts(
                            c,
                            current,
                            &command,
                            config.mode,
                            write.as_ref(),
                            guard.as_ref(),
                            &mut ids,
                        )
                        .await?))
                    })
                },
            )
            .await
            .map_err(FieldServicePartsAdministrationError::Transaction)?;
        outcome.map_err(FieldServicePartsAdministrationError::Wire)
    }
    /// Internal selected-context/schema phase for callers holding typed IDs.
    pub async fn execute_schema_ingress<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        order: RecordId,
        body: Vec<u8>,
        key: CommandKey,
    ) -> Result<CommandOutcome, FieldServicePartsAdministrationError>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let config = self.configuration.clone();
        let expected_company = config.company_id.clone();
        let workspace = move || {
            let mut value = workspace()?;
            value.mutation = true;
            if value.company_id != expected_company {
                return Err(scope());
            }
            Ok(value)
        };
        let ingress_config = config.clone();
        let write = self.write.clone();
        let guard = self.guard.clone();
        self.authorizer
            .execute_with_selected_ingress_deferred(
                host,
                proof,
                workspace,
                (
                    WorkAuthorizationRequest {
                        permission: "records.read".into(),
                        module: Some("field_service".into()),
                        destination: "service/orders".into(),
                        api: true,
                    },
                    FieldServicePartsSourcePreparation,
                ),
                move |selected| {
                    if selected.holding_id() != &ingress_config.holding_id
                        || selected.company_id() != &ingress_config.company_id
                    {
                        return Err(FieldServicePartsAdministrationError::Transaction(
                            scope().into(),
                        ));
                    }
                    normalize_field_service_parts(&body)
                        .map_err(FieldServicePartsAdministrationError::Wire)
                },
                move |c, current, normalized| {
                    let config = config.clone();
                    let write = write.clone();
                    let guard = guard.clone();
                    let order = order.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &config.holding_id
                            || current.selection().workspace().company_id != config.company_id
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        current.require_permission("records.write")?;
                        let command = normalized.into_command(key, order);
                        let mut ids = Ids;
                        execute_field_service_parts(
                            c,
                            current,
                            &command,
                            config.mode,
                            write.as_ref(),
                            guard.as_ref(),
                            &mut ids,
                        )
                        .await
                    })
                },
            )
            .await
            .map_err(FieldServicePartsAdministrationError::Transaction)?
    }
}
fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FIELD_SERVICE_CONFIGURATION_SCOPE_CHANGED",
        "The configured Field Service scope changed.",
    )
}
struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}
