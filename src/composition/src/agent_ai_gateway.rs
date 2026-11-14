//! Native Platform AI-gateway composition.
//!
//! The current Platform Commercial authority is verified in the Control unit:
//! a Holding Account Control session, a current `platform` assignment and
//! `platform.commercial.read`. The retained route's role gate is preserved
//! exactly (`platform_admin`/`platform_auditor` read, `platform_admin` write)
//! and a write additionally requires the existing
//! `platform.commercial.manage` predicate. The target Holding database is
//! exact configuration and the Agent owner owns policy/statement/cost
//! decisions. The ERPX internal Holding is never a customer target.
//!
//! Mutations deliberately re-use the read Commercial operation plus
//! `can_manage` rather than the package-review `Apply` operation: the retained
//! AI-gateway control did not require a fresh MFA ceremony, and adding one
//! would break the incident kill switch this route exists for.
use erpx_server_agent::public::gateway::{
    AiGatewayCostInput, AiGatewayHttpBackend, AiGatewayHttpCommand, AiGatewayHttpCommandRequest,
    AiGatewayHttpFuture, AiGatewayHttpReadRequest, AiGatewayHttpTarget, AiGatewayStatementInput,
    AiPolicyChange, AiProviderStatement, AiSetEnabledInput, AiVerifiedCost, PgAiGateway,
    read_ai_gateway_policy, reconcile_ai_provider_statement, record_ai_provider_statement,
    record_verified_ai_cost, set_ai_platform_key_enabled, update_ai_gateway_policy,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{parse_inbound_account_request, unique_request_header},
    platform_commercial_authority::{
        CurrentPlatformCommercialAuthority, PlatformCommercialOperation,
        verify_current_platform_commercial_authority,
    },
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{DatabasePool, PgRequestLease, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, HoldingId};
use serde_json::Value;
use sqlx::PgConnection;
use std::collections::BTreeMap;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unauthenticated() -> BusinessError {
    BusinessError::new(
        FailureKind::Unauthenticated,
        "UNAUTHENTICATED",
        "Sign in to continue.",
    )
}

fn platform_read_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FORBIDDEN",
        "Platform billing permission required.",
    )
}

fn platform_write_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FORBIDDEN",
        "Only a Platform Admin can change the AI gateway policy.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "AI_GATEWAY_UNAVAILABLE",
        "The native AI-gateway service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// Exact registered Platform Host, target Holding pools and Control authority.
/// A request body selects a target Holding only; it never selects the Platform
/// assignment, role or permissions.
#[derive(Clone)]
pub struct AiGatewayConfiguration {
    platform_host: String,
    platform_holding: HoldingId,
    control: DatabasePool,
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    holdings: BTreeMap<HoldingId, DatabasePool>,
    clock: Arc<dyn Clock + Send + Sync>,
}

impl AiGatewayConfiguration {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        platform_host: String,
        platform_holding: HoldingId,
        control: DatabasePool,
        policy: Arc<AccountPolicy>,
        registry: Arc<ConfiguredHoldingRegistry>,
        holdings: BTreeMap<HoldingId, DatabasePool>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Self {
        Self {
            platform_host,
            platform_holding,
            control,
            policy,
            registry,
            holdings,
            clock,
        }
    }
}

enum OwnerCommand {
    SetEnabled(AiSetEnabledInput),
    UpdateLimits(AiPolicyChange),
    Statement(AiGatewayStatementInput),
    Cost(AiGatewayCostInput),
    Reconcile(String),
}

async fn owner_command(
    connection: &mut PgConnection,
    now: i64,
    user_id: &str,
    holding: &HoldingId,
    command: OwnerCommand,
) -> Result<Value, Error> {
    let mut ports = PgAiGateway::new(connection);
    match command {
        OwnerCommand::SetEnabled(input) => {
            let audit_id = uuid::Uuid::new_v4().to_string();
            let view =
                set_ai_platform_key_enabled(&mut ports, now, &audit_id, &input, user_id).await?;
            Ok(serde_json::to_value(view)
                .map_err(|_| PgTransactionError::Business(unavailable()))?)
        }
        OwnerCommand::UpdateLimits(input) => {
            let audit_id = uuid::Uuid::new_v4().to_string();
            let view =
                update_ai_gateway_policy(&mut ports, now, &audit_id, &input, user_id).await?;
            Ok(serde_json::to_value(view)
                .map_err(|_| PgTransactionError::Business(unavailable()))?)
        }
        OwnerCommand::Statement(input) => {
            let statement = AiProviderStatement {
                id: input.id,
                holding_id: holding.as_str().to_owned(),
                provider: input.provider,
                month: input.month,
                currency: input.currency,
                amount_minor: input.amount_minor,
                statement_digest: input.statement_digest,
                verified_by: user_id.to_owned(),
            };
            record_ai_provider_statement(&mut ports, now, &statement).await
        }
        OwnerCommand::Cost(input) => {
            let cost = AiVerifiedCost {
                request_id: input.request_id,
                provider_statement_id: input.provider_statement_id,
                currency: input.currency,
                amount_minor: input.amount_minor,
                thb_satang: input.thb_satang,
                fx_evidence_ref: input.fx_evidence_ref,
                verified_by: user_id.to_owned(),
            };
            record_verified_ai_cost(&mut ports, now, &cost).await
        }
        OwnerCommand::Reconcile(statement_id) => {
            reconcile_ai_provider_statement(&mut ports, now, &statement_id).await
        }
    }
}

impl AiGatewayConfiguration {
    fn context(&self, headers: &HeaderMap) -> Result<(DeviceProof, Option<String>), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let proof = inbound.into_proof();
        if proof.active.is_none() {
            return Err(unauthenticated());
        }
        let hint = unique_request_header(headers, "x-erpx-platform-assignment")?.map(str::to_owned);
        Ok((proof, hint))
    }

    fn target(&self, holding: &HoldingId) -> Result<DatabasePool, BusinessError> {
        if holding == &self.platform_holding {
            return Err(not_found());
        }
        self.holdings.get(holding).cloned().ok_or_else(not_found)
    }

    async fn verify(
        &self,
        control: &mut PgConnection,
        proof: DeviceProof,
        hint: Option<&str>,
        operation: PlatformCommercialOperation,
        now: i64,
    ) -> Result<CurrentPlatformCommercialAuthority, Error> {
        verify_current_platform_commercial_authority(
            control,
            &self.platform_host,
            proof,
            (*self.policy).clone(),
            self.registry.clone(),
            &self.platform_holding,
            hint,
            operation,
            now,
        )
        .await
    }
}

pub struct AiGatewayBackend {
    configuration: AiGatewayConfiguration,
}

impl AiGatewayBackend {
    pub fn new(configuration: AiGatewayConfiguration) -> Self {
        Self { configuration }
    }
}

impl AiGatewayHttpBackend for AiGatewayBackend {
    fn read<'a>(&'a self, request: AiGatewayHttpReadRequest<'a>) -> AiGatewayHttpFuture<'a, Value> {
        Box::pin(async move {
            let (proof, hint) = self.configuration.context(request.headers)?;
            let configuration = self.configuration.clone();
            let control = self.configuration.control.clone();
            let clock = self.configuration.clock.clone();
            let holding = request.holding_id.clone();
            PgRequestLease::new(control)
                .run(move |control_connection| {
                    Box::pin(async move {
                        let now = clock.now().get();
                        let authority = configuration
                            .verify(
                                control_connection,
                                proof,
                                hint.as_deref(),
                                PlatformCommercialOperation::Read,
                                now,
                            )
                            .await?;
                        if !matches!(
                            authority.selected().role.as_str(),
                            "platform_admin" | "platform_auditor"
                        ) {
                            return Err(platform_read_forbidden().into());
                        }
                        let target = configuration
                            .target(&holding)
                            .map_err(PgTransactionError::Business)?;
                        let mut unit = target.raw().begin().await?;
                        let outcome = async {
                            let mut ports = PgAiGateway::new(&mut unit);
                            read_ai_gateway_policy(&mut ports, now).await
                        }
                        .await;
                        match outcome {
                            Ok(view) => {
                                unit.commit().await?;
                                serde_json::to_value(view)
                                    .map_err(|_| PgTransactionError::Business(unavailable()))
                            }
                            Err(error) => {
                                unit.rollback().await?;
                                Err(error)
                            }
                        }
                    })
                })
                .await
                .map_err(business)
        })
    }

    fn command<'a>(
        &'a self,
        request: AiGatewayHttpCommandRequest<'a>,
    ) -> AiGatewayHttpFuture<'a, Value> {
        Box::pin(async move {
            let (proof, hint) = self.configuration.context(request.headers)?;
            let holding = request.holding_id.clone();
            let command = match request.command {
                AiGatewayHttpCommand::SetEnabled(input) => OwnerCommand::SetEnabled(input),
                AiGatewayHttpCommand::UpdateLimits(input) => OwnerCommand::UpdateLimits(input),
                AiGatewayHttpCommand::RecordStatement(input) => OwnerCommand::Statement(input),
                AiGatewayHttpCommand::RecordCost(input) => OwnerCommand::Cost(input),
                AiGatewayHttpCommand::Reconcile(statement_id) => {
                    OwnerCommand::Reconcile(statement_id)
                }
            };
            let configuration = self.configuration.clone();
            let control = self.configuration.control.clone();
            let clock = self.configuration.clock.clone();
            PgRequestLease::new(control)
                .run(move |control_connection| {
                    Box::pin(async move {
                        let now = clock.now().get();
                        let authority = configuration
                            .verify(
                                control_connection,
                                proof,
                                hint.as_deref(),
                                PlatformCommercialOperation::Read,
                                now,
                            )
                            .await?;
                        if authority.selected().role != "platform_admin" || !authority.can_manage()
                        {
                            return Err(platform_write_forbidden().into());
                        }
                        let target = configuration
                            .target(&holding)
                            .map_err(PgTransactionError::Business)?;
                        let user_id = authority.selected().user_id.clone();
                        let mut unit = target.raw().begin().await?;
                        let outcome = async {
                            authority.require_current(control_connection, now).await?;
                            owner_command(&mut unit, now, &user_id, &holding, command).await
                        }
                        .await;
                        match outcome {
                            Ok(value) => {
                                unit.commit().await?;
                                Ok(value)
                            }
                            Err(error) => {
                                unit.rollback().await?;
                                Err(error)
                            }
                        }
                    })
                })
                .await
                .map_err(business)
        })
    }
}

pub fn ai_gateway_target(
    host: String,
    origin: String,
    backend: Arc<dyn AiGatewayHttpBackend>,
) -> AiGatewayHttpTarget {
    AiGatewayHttpTarget {
        host,
        origin,
        backend,
    }
}
