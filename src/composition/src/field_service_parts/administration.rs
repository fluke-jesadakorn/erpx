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

#[cfg(feature = "business-http")]
impl<W: Clock + Send + Sync + 'static, G: Clock + Send + Sync + 'static>
    FieldServicePartsAdministration<W, G>
{
    pub(crate) async fn execute_http<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        ingress_body: Arc<crate::field_service_http_core::PendingIngress>,
        key: String,
        dispatch: super::http_transport::HttpDispatch,
    ) -> Result<
        Result<erpx_server_workflow::public::commands::CommandJson, HttpIngressError>,
        PgTransactionError<BusinessError>,
    >
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        use super::http_transport::{HttpDispatch, OtherPostInput};
        use crate::field_service_http_core::WireError;
        use crate::field_service_http_wire::SharedPostWire;
        use erpx_server_field_service::public::parts::PendingFieldServiceParts;
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
        let ingress_dispatch = dispatch.clone();
        let write = self.write.clone();
        let guard = self.guard.clone();
        self.authorizer
            .execute_with_selected_transport_ingress_deferred(
                host,
                proof,
                workspace,
                (
                    WorkAuthorizationRequest {
                        permission: "records.read".into(),
                        module: None,
                        destination: "service/orders".into(),
                        api: true,
                    },
                    FieldServicePartsSourcePreparation,
                ),
                erpx_server_identity_access::public::effective_authority::SelectedTransportCallbacks::<_, HttpPrepared, HttpIngressError, _>::new::<erpx_server_workflow::public::commands::CommandJson>(
                 move |selected: &erpx_server_identity_access::public::effective_authority::SelectedWorkIngress<'_>| -> erpx_server_identity_access::public::effective_authority::SelectedIngressFuture<HttpPrepared, HttpIngressError> {
                    let valid = selected.holding_id() == &ingress_config.holding_id
                        && selected.company_id() == &ingress_config.company_id;
                    let ingress_body = ingress_body.clone();
                    let dispatch = ingress_dispatch.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if !valid {
                            return Err(HttpIngressError::Scope);
                        }
                        let reserve_only = matches!(dispatch, HttpDispatch::ReserveOnly);
                        let (bytes, wire) = ingress_body.decode_selected(reserve_only).await.map_err(|e| match e {
                            WireError::InvalidJson => HttpIngressError::InvalidJson,
                            WireError::Validation(message) => HttpIngressError::Validation(message),
                            WireError::UnavailableBranch => HttpIngressError::UnavailableBranch,
                        })?;
                        match wire {
                            SharedPostWire::Parts { target, normalized } => {
                                if matches!((&dispatch, normalized.input()), (HttpDispatch::PrivateCreates(_, _, _, _, _, false, _), erpx_server_field_service::public::parts::FieldServicePartsInput::Reserve { .. }) | (HttpDispatch::PrivateCreates(_, _, _, _, _, _, false), erpx_server_field_service::public::parts::FieldServicePartsInput::Issue { .. })) {
                                    return Err(HttpIngressError::UnavailableBranch);
                                }
                                if reserve_only && !matches!(normalized.input(), erpx_server_field_service::public::parts::FieldServicePartsInput::Reserve { .. }) {
                                    return Err(HttpIngressError::UnavailableBranch);
                                }
                                Ok(HttpPrepared::Parts(PendingFieldServiceParts {
                                    target,
                                    normalized,
                                    key,
                                }))
                            }
                            SharedPostWire::Other { branch, id_utf16 } => {
                                let other = match dispatch { HttpDispatch::Shared(other) => other, HttpDispatch::PrivateCreates(Some(other), _, _, _, _, _, _) if branch == crate::field_service_http_wire::OtherPostBranch::Request => other, HttpDispatch::PrivateCreates(_, Some(other), _, _, _, _, _) if branch == crate::field_service_http_wire::OtherPostBranch::OrderCreate => other, HttpDispatch::PrivateCreates(_, _, Some(other), _, _, _, _) if branch == crate::field_service_http_wire::OtherPostBranch::OrderChange => other, HttpDispatch::PrivateCreates(_, _, _, Some(other), _, _, _) if branch == crate::field_service_http_wire::OtherPostBranch::AgreementChange => other, HttpDispatch::PrivateCreates(_, _, _, _, Some(other), _, _) if branch == crate::field_service_http_wire::OtherPostBranch::AgreementCreate => other, _ => return Err(HttpIngressError::UnavailableBranch) };
                                let input = OtherPostInput {
                                    branch,
                                    id_utf16,
                                    body: bytes,
                                    key,
                                };
                                if !other.supports(&input) { return Err(HttpIngressError::UnavailableBranch); }
                                other
                                    .validate(&input)
                                    .map_err(HttpIngressError::Validation)?;
                                Ok(HttpPrepared::Other(input))
                            }
                        }
                    })
                },
                |prepared: &HttpPrepared, error: BusinessError| match prepared {
                    HttpPrepared::Parts(_) => Ok(HttpIngressError::InitialOwnerReadDenied(error)),
                    HttpPrepared::Other(input) => {
                        Ok(HttpIngressError::OtherAdmissionDenied(input.branch, error))
                    }
                },
                move |c, current, prepared| {
                    let config = config.clone();
                    let write = write.clone();
                    let guard = guard.clone();
                    let dispatch = dispatch.clone();
                    Box::pin(async move {
                        if current.holding_id() != &config.holding_id
                            || current.selection().workspace().company_id != config.company_id
                        {
                            return Err(scope().into());
                        }
                        match prepared {
                            HttpPrepared::Other(input) => {
                                let other = match dispatch { HttpDispatch::Shared(other) => other, HttpDispatch::PrivateCreates(Some(other), _, _, _, _, _, _) if input.branch == crate::field_service_http_wire::OtherPostBranch::Request => other, HttpDispatch::PrivateCreates(_, Some(other), _, _, _, _, _) if input.branch == crate::field_service_http_wire::OtherPostBranch::OrderCreate => other, HttpDispatch::PrivateCreates(_, _, Some(other), _, _, _, _) if input.branch == crate::field_service_http_wire::OtherPostBranch::OrderChange => other, HttpDispatch::PrivateCreates(_, _, _, Some(other), _, _, _) if input.branch == crate::field_service_http_wire::OtherPostBranch::AgreementChange => other, HttpDispatch::PrivateCreates(_, _, _, _, Some(other), _, _) if input.branch == crate::field_service_http_wire::OtherPostBranch::AgreementCreate => other, _ => return Ok(Err(HttpIngressError::UnavailableBranch)) };
                                other.execute(c, current, input).await.map(Ok)
                            }
                            HttpPrepared::Parts(pending) => {
                                if let Err(error) = current.require_permission("records.write") {
                                    return Ok(Err(HttpIngressError::InitialOwnerWriteDenied(
                                        error,
                                    )));
                                }
                                if !current.modules().iter().any(|m| m == "field_service") {
                                    return Ok(Err(HttpIngressError::InitialOwnerModuleDenied));
                                }
                                let mut ids = Ids;
                                Ok(Ok(super::command::execute_pending_field_service_parts(
                                    c,
                                    current,
                                    pending,
                                    config.mode,
                                    write.as_ref(),
                                    guard.as_ref(),
                                    &mut ids,
                                )
                                .await?
                                .result))
                            }
                        }
                    })
                },
                ),
            )
            .await
    }
}
#[cfg(feature = "business-http")]
pub enum HttpIngressError {
    InvalidJson,
    Validation(String),
    Scope,
    InitialOwnerReadDenied(BusinessError),
    InitialOwnerWriteDenied(BusinessError),
    InitialOwnerModuleDenied,
    OtherAdmissionDenied(
        crate::field_service_http_wire::OtherPostBranch,
        BusinessError,
    ),
    UnavailableBranch,
}
#[cfg(feature = "business-http")]
enum HttpPrepared {
    Parts(erpx_server_field_service::public::parts::PendingFieldServiceParts),
    Other(super::http_transport::OtherPostInput),
}

#[cfg(feature = "business-http")]
pub use super::http_transport::{OtherPost, OtherPostInput};
