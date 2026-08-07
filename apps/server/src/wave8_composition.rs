//! Runtime composition for the Wave 8 dependent tenant bounded contexts.

use async_trait::async_trait;
use chrono::{Duration, Utc};
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryConsistency, QueryEnvelope,
    QueryHandler, QueryResult,
};
use erpx_cqrs_sqlx::{ReadPool, WritePool};
use erpx_customer_authority::{
    CustomerId, CustomerStatus as AuthorityCustomerStatus, ReadRepository as CustomerReadRepository,
};
use erpx_customer_authority_sqlx::CustomerAuthorityReadRepository;
use erpx_domain::access::Scope;
use erpx_finance as finance;
use erpx_finance_sqlx as finance_sqlx;
use erpx_infrastructure::RoutedDatabaseSide;
use erpx_integration_contracts::{DisplayName, ReferenceCode, customer, organization};
use erpx_kernel::{
    ActorAttribution, CorrelationId, ErrorCategory, ErrorContext, KernelError, SystemClock,
    TenantScope,
};
use erpx_legal as legal;
use erpx_legal_sqlx as legal_sqlx;
use erpx_support as support;
use erpx_support_sqlx as support_sqlx;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tonic::Status;
use uuid::Uuid;

use crate::AppState;
use crate::contexts::{
    finance_cqrs::{
        AuthenticatedFinanceRequest, FinanceAccessMode, FinanceAuthenticationError,
        FinanceAuthenticationRequest, FinanceCommandApplication, FinanceQueryApplication,
        FinanceRequestAuthenticator, FinanceTenantRoute,
    },
    legal_cqrs::{
        AuthenticatedLegalRequest, LegalAccessMode, LegalAuthenticationError,
        LegalAuthenticationRequest, LegalCommandApplication, LegalQueryApplication,
        LegalRequestAuthenticator, LegalTenantRoute,
    },
    support_cqrs::{
        AuthenticatedSupportRequest, SecureCapabilityIssueMaterial, SupportAccessMode,
        SupportAuthenticationError, SupportAuthenticationRequest,
        SupportCapabilityAuthorityResolver, SupportCapabilityDeliverySecret,
        SupportCommandApplication, SupportCustomerFactResolver, SupportQueryApplication,
        SupportRequestAuthenticator, SupportTenantRoute,
    },
};
use crate::wave4_composition::authenticate_context;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccessMode {
    Read,
    Write,
}

#[derive(Debug, Clone)]
struct PreauthorizedAccess {
    principal_ref: String,
    authority_scope: TenantScope,
    mode: AccessMode,
}

impl PreauthorizedAccess {
    fn permits(
        &self,
        actor: &ActorAttribution,
        authority_scope: &TenantScope,
        target_scope: &TenantScope,
        write: bool,
    ) -> bool {
        actor.principal_ref == self.principal_ref
            && authority_scope == &self.authority_scope
            && self.authority_scope.contains(target_scope)
            && write == (self.mode == AccessMode::Write)
    }

    fn entitled(&self, scope: &TenantScope, write: bool) -> bool {
        self.authority_scope.contains(scope) && write == (self.mode == AccessMode::Write)
    }
}

#[async_trait]
impl finance::AuthorizationPort for PreauthorizedAccess {
    async fn authorize(
        &self,
        request: finance::ports::AuthorizationRequest<'_>,
    ) -> Result<(), finance::PortError> {
        if self.permits(
            request.actor,
            request.authority_scope,
            request.target_scope,
            request.action.requires_write_entitlement(),
        ) {
            Ok(())
        } else {
            Err(finance::PortError::Denied)
        }
    }
}

#[async_trait]
impl finance::EntitlementPort for PreauthorizedAccess {
    async fn ensure_finance_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), finance::PortError> {
        if self.entitled(scope, write) {
            Ok(())
        } else {
            Err(finance::PortError::EntitlementDenied)
        }
    }
}

#[async_trait]
impl legal::AuthorizationPort for PreauthorizedAccess {
    async fn authorize(
        &self,
        request: legal::ports::AuthorizationRequest<'_>,
    ) -> Result<(), legal::PortError> {
        if self.permits(
            request.actor,
            request.authority_scope,
            request.target_scope,
            request.action.requires_write_entitlement(),
        ) {
            Ok(())
        } else {
            Err(legal::PortError::Denied)
        }
    }
}

#[async_trait]
impl legal::EntitlementPort for PreauthorizedAccess {
    async fn ensure_legal_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), legal::PortError> {
        if self.entitled(scope, write) {
            Ok(())
        } else {
            Err(legal::PortError::EntitlementDenied)
        }
    }
}

#[async_trait]
impl support::AuthorizationPort for PreauthorizedAccess {
    async fn authorize(
        &self,
        request: support::ports::AuthorizationRequest<'_>,
    ) -> Result<(), support::PortError> {
        if !self.permits(
            request.actor,
            request.authority_scope,
            request.target_scope,
            request.action.requires_write_entitlement(),
        ) {
            return Err(support::PortError::Denied);
        }
        if request.action == support::SupportAction::IssueCapability {
            let evidence = request
                .capability_evidence
                .ok_or(support::PortError::Denied)?;
            let mfa_at = evidence.mfa_verified_at.ok_or(support::PortError::Denied)?;
            if request.device_ref != Some(evidence.device_ref.as_str())
                || !evidence.segregation_of_duties_passed
                || evidence.authority_scope != self.authority_scope
                || mfa_at > Utc::now()
                || Utc::now().signed_duration_since(mfa_at) > Duration::minutes(5)
            {
                return Err(support::PortError::Denied);
            }
        }
        Ok(())
    }
}

#[async_trait]
impl support::EntitlementPort for PreauthorizedAccess {
    async fn ensure_support_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> Result<(), support::PortError> {
        if self.entitled(scope, write) {
            Ok(())
        } else {
            Err(support::PortError::EntitlementDenied)
        }
    }
}

#[derive(Clone)]
pub(crate) struct Wave8TenantApplication {
    state: AppState,
    capability_secret_key: std::sync::Arc<Vec<u8>>,
}

impl Wave8TenantApplication {
    pub(crate) fn new(state: AppState) -> Result<Self, &'static str> {
        let key = std::env::var("ERPX_SUPPORT_CAPABILITY_ESCROW_KEY")
            .map_err(|_| "ERPX_SUPPORT_CAPABILITY_ESCROW_KEY is required")?;
        if !(32..=256).contains(&key.len()) || key.chars().any(char::is_control) {
            return Err("ERPX_SUPPORT_CAPABILITY_ESCROW_KEY is invalid");
        }
        Ok(Self {
            state,
            capability_secret_key: std::sync::Arc::new(key.into_bytes()),
        })
    }

    fn derive_capability_secret(
        &self,
        tenant_id: &str,
        command_id: erpx_kernel::CommandId,
        capability_id: support::CapabilityId,
        principal_ref: &str,
        device_ref: &str,
    ) -> Result<[u8; 32], Status> {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.capability_secret_key.as_slice())
            .map_err(|_| Status::internal("Support capability escrow is unavailable"))?;
        for value in [
            tenant_id.as_bytes(),
            command_id.as_uuid().as_bytes(),
            capability_id.as_uuid().as_bytes(),
            principal_ref.as_bytes(),
            device_ref.as_bytes(),
        ] {
            let length = u64::try_from(value.len())
                .map_err(|_| Status::internal("Support capability escrow is unavailable"))?;
            mac.update(&length.to_be_bytes());
            mac.update(value);
        }
        Ok(mac.finalize().into_bytes().into())
    }

    async fn write_pool(
        &self,
        tenant_id: &str,
        holding_ref: &str,
        correlation_id: CorrelationId,
        context: &'static str,
    ) -> Result<WritePool, CqrsError> {
        let pool = self
            .route_pool(
                tenant_id,
                holding_ref,
                RoutedDatabaseSide::Write,
                correlation_id,
                context,
            )
            .await?;
        Ok(WritePool::from_pg_pool(pool))
    }

    async fn read_pool(
        &self,
        tenant_id: &str,
        holding_ref: &str,
        correlation_id: CorrelationId,
        context: &'static str,
    ) -> Result<ReadPool, CqrsError> {
        let pool = self
            .route_pool(
                tenant_id,
                holding_ref,
                RoutedDatabaseSide::Read,
                correlation_id,
                context,
            )
            .await?;
        Ok(ReadPool::from_pg_pool(pool))
    }

    async fn route_pool(
        &self,
        tenant_id: &str,
        holding_ref: &str,
        side: RoutedDatabaseSide,
        correlation_id: CorrelationId,
        context: &'static str,
    ) -> Result<sqlx::PgPool, CqrsError> {
        let router = self.state.tenant_router.as_ref().ok_or_else(|| {
            context_failure(
                correlation_id,
                format!("{context}.routing_unavailable"),
                ErrorCategory::Unavailable,
                "Tenant database routing is unavailable.",
                true,
            )
        })?;
        let routed = router
            .route(tenant_id, holding_ref, side)
            .await
            .map_err(|_| {
                context_failure(
                    correlation_id,
                    format!("{context}.storage_unavailable"),
                    ErrorCategory::Unavailable,
                    "Context storage is temporarily unavailable.",
                    true,
                )
            })?;
        Ok(routed.database_pool().clone())
    }

    fn access(
        actor: &ActorAttribution,
        scope: &TenantScope,
        mode: AccessMode,
    ) -> PreauthorizedAccess {
        PreauthorizedAccess {
            principal_ref: actor.principal_ref.clone(),
            authority_scope: scope.clone(),
            mode,
        }
    }
}

#[async_trait]
impl FinanceCommandApplication for Wave8TenantApplication {
    async fn execute(
        &self,
        route: FinanceTenantRoute,
        envelope: CommandEnvelope<finance::FinanceCommand>,
    ) -> Result<CommandReceipt<finance::FinanceMutationResult>, CqrsError> {
        let pool = self
            .write_pool(
                &route.tenant_id,
                &route.holding_ref,
                envelope.correlation_id,
                "finance",
            )
            .await?;
        let access = Self::access(&envelope.actor, &envelope.tenant_scope, AccessMode::Write);
        finance::FinanceCommandHandler::new(
            finance_sqlx::FinanceWriteRepository::new(pool.clone()),
            finance_sqlx::FinanceCommandAdapter::new(pool),
            access.clone(),
            access,
            SystemClock,
        )
        .handle(envelope)
        .await
    }
}

#[async_trait]
impl FinanceQueryApplication for Wave8TenantApplication {
    async fn execute(
        &self,
        route: FinanceTenantRoute,
        envelope: QueryEnvelope<finance::FinanceQuery>,
    ) -> Result<QueryResult<finance::FinanceQueryResponse>, CqrsError> {
        let pool = self
            .read_pool(
                &route.tenant_id,
                &route.holding_ref,
                envelope.correlation_id,
                "finance",
            )
            .await?;
        let read = finance_sqlx::FinanceReadRepository::new(pool)
            .map_err(|_| storage_configuration_error(envelope.correlation_id, "finance"))?;
        let access = Self::access(&envelope.actor, &envelope.tenant_scope, AccessMode::Read);
        finance::FinanceQueryHandler::new(read, access.clone(), access)
            .handle(envelope)
            .await
    }
}

#[async_trait]
impl LegalCommandApplication for Wave8TenantApplication {
    async fn execute(
        &self,
        route: LegalTenantRoute,
        envelope: CommandEnvelope<legal::LegalCommand>,
    ) -> Result<CommandReceipt<legal::LegalMutationResult>, CqrsError> {
        let pool = self
            .write_pool(
                &route.tenant_id,
                &route.holding_ref,
                envelope.correlation_id,
                "legal",
            )
            .await?;
        let access = Self::access(&envelope.actor, &envelope.tenant_scope, AccessMode::Write);
        legal::LegalCommandHandler::new(
            legal_sqlx::LegalWriteRepository::new(pool.clone()),
            legal_sqlx::LegalCommandAdapter::new(pool),
            access.clone(),
            access,
            SystemClock,
        )
        .handle(envelope)
        .await
    }
}

#[async_trait]
impl LegalQueryApplication for Wave8TenantApplication {
    async fn execute(
        &self,
        route: LegalTenantRoute,
        envelope: QueryEnvelope<legal::LegalQuery>,
    ) -> Result<QueryResult<legal::LegalQueryResponse>, CqrsError> {
        let pool = self
            .read_pool(
                &route.tenant_id,
                &route.holding_ref,
                envelope.correlation_id,
                "legal",
            )
            .await?;
        let read = legal_sqlx::LegalReadRepository::new(pool)
            .map_err(|_| storage_configuration_error(envelope.correlation_id, "legal"))?;
        let access = Self::access(&envelope.actor, &envelope.tenant_scope, AccessMode::Read);
        legal::LegalQueryHandler::new(read, access.clone(), access)
            .handle(envelope)
            .await
    }
}

#[async_trait]
impl SupportCommandApplication for Wave8TenantApplication {
    async fn execute(
        &self,
        route: SupportTenantRoute,
        envelope: CommandEnvelope<support::SupportCommand>,
    ) -> Result<CommandReceipt<support::SupportMutationResult>, CqrsError> {
        let pool = self
            .write_pool(
                &route.tenant_id,
                &route.holding_ref,
                envelope.correlation_id,
                "support",
            )
            .await?;
        let access = Self::access(&envelope.actor, &envelope.tenant_scope, AccessMode::Write);
        support::SupportCommandHandler::new(
            support_sqlx::SupportWriteRepository::new(pool.clone()),
            support_sqlx::SupportCommandAdapter::new(pool),
            access.clone(),
            access,
            SystemClock,
        )
        .handle(envelope)
        .await
    }
}

#[async_trait]
impl SupportQueryApplication for Wave8TenantApplication {
    async fn execute(
        &self,
        route: SupportTenantRoute,
        envelope: QueryEnvelope<support::SupportQuery>,
    ) -> Result<QueryResult<support::SupportQueryResponse>, CqrsError> {
        let customer_channel = matches!(
            &envelope.query,
            support::SupportQuery::GetTicket(support::GetSupportTicket {
                audience: support::TicketReadAudience::Customer,
                ..
            }) | support::SupportQuery::ListTickets(support::ListSupportTickets {
                filter: support::SupportTicketListFilter {
                    audience: support::TicketReadAudience::Customer,
                    ..
                },
            })
        );
        let customer_id = customer_channel
            .then(|| Uuid::parse_str(&route.tenant_id))
            .transpose()
            .map_err(|_| {
                context_failure(
                    envelope.correlation_id,
                    "support.customer_authority_invalid",
                    ErrorCategory::Denied,
                    "customer Support authority is invalid",
                    false,
                )
            })?;
        let pool = self
            .read_pool(
                &route.tenant_id,
                &route.holding_ref,
                envelope.correlation_id,
                "support",
            )
            .await?;
        let read = support_sqlx::SupportReadRepository::new(pool)
            .map_err(|_| storage_configuration_error(envelope.correlation_id, "support"))?;
        let access = Self::access(&envelope.actor, &envelope.tenant_scope, AccessMode::Read);
        let result = support::SupportQueryHandler::new(read, access.clone(), access)
            .handle(envelope)
            .await?;
        if let Some(customer_id) = customer_id {
            let bound = match &result.value {
                support::SupportQueryResponse::Ticket(Some(ticket)) => {
                    ticket.customer.as_uuid() == customer_id
                        && ticket.customer_account.as_uuid() == customer_id
                        && ticket.internal_messages.is_none()
                }
                support::SupportQueryResponse::Ticket(None) => true,
                support::SupportQueryResponse::Tickets(page) => page.tickets.iter().all(|ticket| {
                    ticket.customer.as_uuid() == customer_id
                        && ticket.customer_account.as_uuid() == customer_id
                        && ticket.internal_messages.is_none()
                }),
                _ => false,
            };
            if !bound {
                return Err(context_failure(
                    result.metadata.correlation_id,
                    "support.customer_ticket_scope_denied",
                    ErrorCategory::Denied,
                    "customer Support record is outside the authenticated tenant",
                    false,
                ));
            }
        }
        Ok(result)
    }
}

#[async_trait]
impl SupportCustomerFactResolver for Wave8TenantApplication {
    async fn resolve_customer(
        &self,
        route: &SupportTenantRoute,
        authority: &TenantScope,
        customer_id: Uuid,
        account_id: Uuid,
    ) -> Result<customer::CustomerProfileFact, Status> {
        if customer_id != account_id {
            return Err(Status::permission_denied(
                "customer account binding changed",
            ));
        }
        let pool = self
            .read_pool(
                &route.tenant_id,
                &route.holding_ref,
                fresh_correlation_id()?,
                "customer_authority",
            )
            .await
            .map_err(|_| Status::unavailable("customer authority is unavailable"))?;
        let repository = CustomerAuthorityReadRepository::new(pool)
            .map_err(|_| Status::unavailable("customer authority is unavailable"))?;
        let id = CustomerId::try_from_uuid(customer_id)
            .map_err(|_| Status::invalid_argument("customer_id is invalid"))?;
        let projection = repository
            .get(authority, id, &QueryConsistency::Eventual)
            .await
            .map_err(|_| Status::unavailable("customer authority is unavailable"))?;
        let view = projection
            .value
            .ok_or_else(|| Status::not_found("customer was not found"))?;
        let owning_company_id = match view.primary_authority {
            TenantScope::Company { company_id, .. } | TenantScope::Branch { company_id, .. } => {
                company_id
            }
            _ => {
                return Err(Status::failed_precondition(
                    "customer owning company is unavailable",
                ));
            }
        };
        Ok(customer::CustomerProfileFact {
            customer: customer::CustomerRef::try_from_uuid(customer_id)
                .map_err(|_| Status::internal("customer reference is invalid"))?,
            account: customer::CustomerAccountRef::try_from_uuid(account_id)
                .map_err(|_| Status::internal("customer account reference is invalid"))?,
            customer_code: ReferenceCode::try_new(view.code.as_str())
                .map_err(|_| Status::internal("customer code is invalid"))?,
            display_name: DisplayName::try_new(view.trading_name)
                .map_err(|_| Status::internal("customer display name is invalid"))?,
            owning_company: organization::CompanyRef::try_from_uuid(owning_company_id)
                .map_err(|_| Status::internal("customer company reference is invalid"))?,
            status: match view.status {
                AuthorityCustomerStatus::Prospect | AuthorityCustomerStatus::Onboarding => {
                    customer::CustomerStatus::Prospect
                }
                AuthorityCustomerStatus::Active => customer::CustomerStatus::Active,
                AuthorityCustomerStatus::Suspended => customer::CustomerStatus::Blocked,
                AuthorityCustomerStatus::Churned => customer::CustomerStatus::Closed,
            },
        })
    }
}

#[async_trait]
impl SupportCapabilityAuthorityResolver for Wave8TenantApplication {
    async fn resolve_issue_material(
        &self,
        route: &SupportTenantRoute,
        actor: &ActorAttribution,
        authority: &TenantScope,
        command_id: erpx_kernel::CommandId,
        capability_id: support::CapabilityId,
        session_id: support::SessionId,
        device_ref: &str,
    ) -> Result<SecureCapabilityIssueMaterial, Status> {
        let actor_session = actor
            .session_id
            .ok_or_else(|| Status::permission_denied("active identity session is required"))?;
        let active_device = self
            .state
            .auth
            .identity
            .active_session_device_id(actor_session.as_uuid(), Utc::now())
            .await
            .map_err(|_| Status::permission_denied("active identity device is required"))?;
        if active_device.to_string() != device_ref {
            return Err(Status::permission_denied("identity device binding changed"));
        }

        let write_pool = self
            .write_pool(
                &route.tenant_id,
                &route.holding_ref,
                fresh_correlation_id()?,
                "support",
            )
            .await
            .map_err(|_| Status::unavailable("support authority is unavailable"))?;
        let repository = support_sqlx::SupportWriteRepository::new(write_pool);
        let session = support::WriteRepository::load_session(&repository, authority, session_id)
            .await
            .map_err(|_| Status::unavailable("support authority is unavailable"))?
            .ok_or_else(|| Status::not_found("support session was not found"))?;
        let actor_id = Uuid::parse_str(&actor.principal_ref)
            .map_err(|_| Status::permission_denied("support agent identity is invalid"))?;
        let now = Utc::now();
        if session.agent().as_uuid() != actor_id
            || session.status() != support::SessionStatus::Approved
            || session.approved_at().is_none()
            || session.revoked_at().is_some()
            || session.expires_at() <= now
            || !authority.contains(session.scope())
        {
            return Err(Status::permission_denied(
                "support session is not active for this authority",
            ));
        }
        let sod_passed = self
            .state
            .store
            .sod_decision_active(
                &actor.principal_ref,
                session.module(),
                &Scope::Platform,
                now,
            )
            .await
            .map_err(|_| Status::unavailable("support duty separation is unavailable"))?;
        if !sod_passed {
            return Err(Status::permission_denied(
                "support duty separation is required",
            ));
        }

        let delivery_secret = self.derive_capability_secret(
            &route.tenant_id,
            command_id,
            capability_id,
            &actor.principal_ref,
            &active_device.to_string(),
        )?;
        let secret_hash_sha256: [u8; 32] = Sha256::digest(delivery_secret).into();
        Ok(SecureCapabilityIssueMaterial {
            evidence: support::CapabilityEvidence {
                device_ref: active_device.to_string(),
                mfa_verified_at: Some(now),
                segregation_of_duties_passed: true,
                authority_scope: authority.clone(),
            },
            secret_hash_sha256,
            delivery_secret: SupportCapabilityDeliverySecret::from_bytes(delivery_secret),
        })
    }
}

#[derive(Clone)]
pub(crate) struct Wave8ServerAuthenticator {
    state: AppState,
}

impl Wave8ServerAuthenticator {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }

    async fn verify_presented_support_capability(
        &self,
        request: &SupportAuthenticationRequest,
        mut actor: ActorAttribution,
    ) -> Result<ActorAttribution, SupportAuthenticationError> {
        let capability_id = request
            .transport_metadata()
            .get("x-erpx-support-capability-id");
        let bearer = request
            .transport_metadata()
            .get_bin("x-erpx-support-capability-bin");
        let (Some(capability_id), Some(bearer)) = (capability_id, bearer) else {
            return if capability_id.is_none() && bearer.is_none() {
                Ok(actor)
            } else {
                Err(SupportAuthenticationError::Denied)
            };
        };
        let capability_id = capability_id
            .to_str()
            .ok()
            .and_then(|value| Uuid::parse_str(value).ok())
            .and_then(|value| support::CapabilityId::try_from_uuid(value).ok())
            .ok_or(SupportAuthenticationError::Denied)?;
        let bearer = bearer
            .to_bytes()
            .map_err(|_| SupportAuthenticationError::Denied)?;
        if bearer.len() != 32 {
            return Err(SupportAuthenticationError::Denied);
        }
        let router = self
            .state
            .tenant_router
            .as_ref()
            .ok_or(SupportAuthenticationError::Unavailable)?;
        let routed = router
            .route(
                &request.presented_route().tenant_id,
                &request.presented_route().holding_ref,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| SupportAuthenticationError::Unavailable)?;
        let repository = support_sqlx::SupportWriteRepository::new(WritePool::from_pg_pool(
            routed.database_pool().clone(),
        ));
        let capability = support::WriteRepository::load_capability(
            &repository,
            request.presented_scope(),
            capability_id,
        )
        .await
        .map_err(|_| SupportAuthenticationError::Unavailable)?
        .ok_or(SupportAuthenticationError::Denied)?;
        let session = support::WriteRepository::load_session(
            &repository,
            request.presented_scope(),
            capability.session_id(),
        )
        .await
        .map_err(|_| SupportAuthenticationError::Unavailable)?
        .ok_or(SupportAuthenticationError::Denied)?;
        let ticket = support::WriteRepository::load_ticket(
            &repository,
            request.presented_scope(),
            capability.ticket_id(),
        )
        .await
        .map_err(|_| SupportAuthenticationError::Unavailable)?
        .ok_or(SupportAuthenticationError::Denied)?;
        let digest = Sha256::digest(bearer.as_ref());
        if !bool::from(
            capability
                .secret_hash_sha256()
                .as_slice()
                .ct_eq(digest.as_ref()),
        ) || capability.agent().as_uuid().to_string() != actor.principal_ref
            || ticket.id() != capability.ticket_id()
            || ticket.status() == support::TicketStatus::Closed
            || session.ticket_id() != capability.ticket_id()
        {
            return Err(SupportAuthenticationError::Denied);
        }
        let identity_session = actor.session_id.ok_or(SupportAuthenticationError::Denied)?;
        let active_device = self
            .state
            .auth
            .identity
            .active_session_device_id(identity_session.as_uuid(), Utc::now())
            .await
            .map_err(|_| SupportAuthenticationError::Denied)?;
        let legacy_scope = crate::quality_access_scope(request.presented_scope());
        let sod_passed = self
            .state
            .store
            .sod_decision_active(
                &actor.principal_ref,
                capability.module(),
                &legacy_scope,
                Utc::now(),
            )
            .await
            .map_err(|_| SupportAuthenticationError::Unavailable)?;
        let mfa_verified_at = request
            .mfa_code()
            .filter(|code| !code.trim().is_empty())
            .map(|_| Utc::now());
        let evidence = support::CapabilityEvidence {
            device_ref: active_device.to_string(),
            mfa_verified_at,
            segregation_of_duties_passed: sod_passed,
            authority_scope: request.presented_scope().clone(),
        };
        capability
            .authorize(
                &session,
                support_permission(request.action()),
                request.presented_scope(),
                &evidence,
                Utc::now(),
            )
            .map_err(|_| SupportAuthenticationError::Denied)?;
        actor.support_capability_id = Some(capability_id.as_uuid());
        actor
            .validate()
            .map_err(|_| SupportAuthenticationError::Internal)?;
        Ok(actor)
    }

    // Keep all transport authority inputs explicit at the shared wave boundary.
    #[allow(clippy::too_many_arguments)]
    async fn authenticate(
        &self,
        metadata: &tonic::metadata::MetadataMap,
        actor: &ActorAttribution,
        scope: &TenantScope,
        tenant_id: &str,
        subscription_id: Option<&str>,
        mfa_code: Option<&str>,
        mode: AccessMode,
        context: &'static str,
    ) -> Result<ActorAttribution, Status> {
        let write = mode == AccessMode::Write;
        authenticate_context(
            &self.state,
            metadata,
            actor,
            scope,
            tenant_id,
            if write {
                write_permission(context)
            } else {
                read_permission(context)
            },
            product_name(context),
            write,
            subscription_id,
            mfa_code,
        )
        .await
    }
}

macro_rules! authenticator {
    ($trait:path, $request:path, $authenticated:path, $error:path, $read_mode:path, $context:literal) => {
        #[async_trait]
        impl $trait for Wave8ServerAuthenticator {
            async fn authenticate(&self, request: $request) -> Result<$authenticated, $error> {
                let mode = if request.access_mode() == $read_mode {
                    AccessMode::Read
                } else {
                    AccessMode::Write
                };
                let actor = self
                    .authenticate(
                        request.transport_metadata(),
                        request.presented_actor(),
                        request.presented_scope(),
                        &request.presented_route().tenant_id,
                        request.subscription_id(),
                        request.mfa_code(),
                        mode,
                        $context,
                    )
                    .await
                    .map_err(map_auth_error)?;
                <$authenticated>::try_new(
                    actor,
                    request.presented_scope().clone(),
                    request.presented_route().clone(),
                )
            }
        }
    };
}

authenticator!(
    FinanceRequestAuthenticator,
    FinanceAuthenticationRequest,
    AuthenticatedFinanceRequest,
    FinanceAuthenticationError,
    FinanceAccessMode::Read,
    "finance"
);
authenticator!(
    LegalRequestAuthenticator,
    LegalAuthenticationRequest,
    AuthenticatedLegalRequest,
    LegalAuthenticationError,
    LegalAccessMode::Read,
    "legal"
);

#[async_trait]
impl SupportRequestAuthenticator for Wave8ServerAuthenticator {
    async fn authenticate(
        &self,
        request: SupportAuthenticationRequest,
    ) -> Result<AuthenticatedSupportRequest, SupportAuthenticationError> {
        let action = request.action();
        let expected_mode = if action.requires_write_entitlement() {
            SupportAccessMode::Write
        } else {
            SupportAccessMode::Read
        };
        if request.access_mode() != expected_mode
            || (action.requires_current_mfa()
                && request.mfa_code().is_none_or(|code| code.trim().is_empty()))
        {
            return Err(SupportAuthenticationError::Denied);
        }
        let actor = authenticate_context(
            &self.state,
            request.transport_metadata(),
            request.presented_actor(),
            request.presented_scope(),
            &request.presented_route().tenant_id,
            support_permission(action),
            "support",
            action.requires_write_entitlement(),
            request.subscription_id(),
            request.mfa_code(),
        )
        .await
        .map_err(map_auth_error)?;
        let actor = self
            .verify_presented_support_capability(&request, actor)
            .await?;
        if action.requires_segregation_of_duties() {
            let scope = crate::quality_access_scope(request.presented_scope());
            let separated = self
                .state
                .store
                .sod_decision_active(&actor.principal_ref, "support", &scope, Utc::now())
                .await
                .map_err(|_| SupportAuthenticationError::Unavailable)?;
            if !separated {
                return Err(SupportAuthenticationError::Denied);
            }
        }
        AuthenticatedSupportRequest::try_new(
            actor,
            request.presented_scope().clone(),
            request.presented_route().clone(),
        )
    }
}

trait AuthenticationError: Sized {
    fn unauthenticated() -> Self;
    fn denied() -> Self;
    fn unavailable() -> Self;
    fn internal() -> Self;
}

macro_rules! authentication_error {
    ($error:path) => {
        impl AuthenticationError for $error {
            fn unauthenticated() -> Self {
                <$error>::Unauthenticated
            }
            fn denied() -> Self {
                <$error>::Denied
            }
            fn unavailable() -> Self {
                <$error>::Unavailable
            }
            fn internal() -> Self {
                <$error>::Internal
            }
        }
    };
}

authentication_error!(FinanceAuthenticationError);
authentication_error!(LegalAuthenticationError);
authentication_error!(SupportAuthenticationError);

fn map_auth_error<E: AuthenticationError>(status: Status) -> E {
    match status.code() {
        tonic::Code::Unauthenticated => E::unauthenticated(),
        tonic::Code::PermissionDenied | tonic::Code::InvalidArgument => E::denied(),
        tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => E::unavailable(),
        _ => E::internal(),
    }
}

fn read_permission(context: &str) -> &'static str {
    match context {
        "finance" => "finance.read",
        "legal" => "legal.read",
        "support" => "support.read",
        _ => "context.read",
    }
}

fn write_permission(context: &str) -> &'static str {
    match context {
        "finance" => "finance.write",
        "legal" => "legal.write",
        "support" => "support.write",
        _ => "context.write",
    }
}

const fn support_permission(action: support::SupportAction) -> &'static str {
    match action {
        support::SupportAction::OpenTicket => "support.ticket.open",
        support::SupportAction::TransitionTicket => "support.ticket.transition",
        support::SupportAction::AddCustomerMessage
        | support::SupportAction::ViewCustomerTicket
        | support::SupportAction::ListCustomerTickets => "support.ticket.reply",
        support::SupportAction::AddInternalMessage
        | support::SupportAction::ViewInternalTicket
        | support::SupportAction::ListInternalTickets => "support.ticket.note",
        support::SupportAction::RequestSession | support::SupportAction::ViewSession => {
            "support.session.request"
        }
        support::SupportAction::ApproveSession => "support.session.approve",
        support::SupportAction::RevokeSession => "support.session.revoke",
        support::SupportAction::IssueCapability | support::SupportAction::ViewCapability => {
            "support.capability.issue"
        }
        support::SupportAction::RevokeCapability => "support.capability.revoke",
        support::SupportAction::OpenIncident => "support.incident.open",
        support::SupportAction::TransitionIncident => "support.incident.transition",
        support::SupportAction::AddIncidentTimeline => "support.incident.timeline",
        support::SupportAction::BroadcastCustomerIncident
        | support::SupportAction::BroadcastInternalIncident => "support.incident.broadcast",
        support::SupportAction::ViewIncident | support::SupportAction::ListIncidents => {
            "support.incident.read"
        }
    }
}

fn product_name(context: &str) -> &'static str {
    match context {
        "finance" => "finance",
        "legal" => "legal",
        "support" => "support",
        _ => "context",
    }
}

fn fresh_correlation_id() -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(Uuid::new_v4())
        .map_err(|_| Status::internal("correlation identity could not be generated"))
}

fn storage_configuration_error(correlation_id: CorrelationId, context: &'static str) -> CqrsError {
    context_failure(
        correlation_id,
        format!("{context}.read_configuration"),
        ErrorCategory::Internal,
        "Context read storage could not be initialized.",
        false,
    )
}

fn context_failure(
    correlation_id: CorrelationId,
    code: impl Into<String>,
    category: ErrorCategory,
    safe_message: impl Into<String>,
    retryable: bool,
) -> CqrsError {
    CqrsError::Operation(KernelError {
        code: code.into(),
        category,
        safe_message: safe_message.into(),
        retryable,
        correlation_id,
        context: ErrorContext::empty(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_kernel::{ActorKind, SessionId};

    fn scope() -> TenantScope {
        TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }

    fn actor() -> ActorAttribution {
        ActorAttribution {
            principal_ref: Uuid::from_u128(3).to_string(),
            kind: ActorKind::TenantUser,
            session_id: Some(SessionId::try_from_uuid(Uuid::from_u128(4)).unwrap()),
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }

    #[tokio::test]
    async fn preauthorized_ports_fail_closed_on_mode_mismatch() {
        let access = PreauthorizedAccess {
            principal_ref: actor().principal_ref,
            authority_scope: scope(),
            mode: AccessMode::Read,
        };
        assert!(
            finance::EntitlementPort::ensure_finance_enabled(&access, &scope(), false)
                .await
                .is_ok()
        );
        assert!(
            finance::EntitlementPort::ensure_finance_enabled(&access, &scope(), true)
                .await
                .is_err()
        );
    }

    #[test]
    fn support_capability_material_is_hash_only_and_redacted() {
        let material = SecureCapabilityIssueMaterial {
            evidence: support::CapabilityEvidence {
                device_ref: Uuid::from_u128(5).to_string(),
                mfa_verified_at: Some(Utc::now()),
                segregation_of_duties_passed: true,
                authority_scope: scope(),
            },
            secret_hash_sha256: [7; 32],
            delivery_secret: SupportCapabilityDeliverySecret::from_bytes([9; 32]),
        };
        let rendered = format!("{material:?}");
        assert!(rendered.contains("REDACTED"));
        assert!(!rendered.contains("7, 7, 7"));
    }
}
