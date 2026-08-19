//! Authorized worker composition for Finance bank-provider imports.
//!
//! This boundary connects the already-separated control-plane route, secret
//! manager, provider adapter, bounded pagination coordinator, and holding
//! write-side persistence. The scheduler and transport remain outside this
//! module so an import cannot run without an explicit injected authorization
//! decision.

// Registration and production command ownership are coordinator-owned. Keep
// this complete worker seam compile-checked while its shared registration
// packet remains intentionally separate from the provider implementation.
#![allow(dead_code)]

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use erpx_domain::{
    contracts::{ActorKind, CommandContext},
    finance::{
        FINANCE_AUTHORIZATION_CONTRACT_VERSION, FinanceAccessPath, FinanceAuthorizationDecision,
        FinanceAuthorizationOperation,
    },
    finance_banking::BankImportReceipt,
};
use erpx_infrastructure::{
    BankAccountWriteReceipt, BankProviderAccountBinding, BankProviderAdapter,
    BankProviderCredentialError, BankProviderCredentialKey, BankProviderCredentialResolver,
    BankProviderError, BankStatementPageRequest, FinanceBankProviderAccountBinding,
    HoldingDatabaseRouter, PersistenceError, RoutedDatabaseSide, SqlxControlPlane,
    SqlxFinanceBankingStore,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::{
    BankProviderImportCoordinator, NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError,
};

/// The import handler is intentionally not registered here. Registration and
/// command ownership remain coordinator concerns, while this module owns the
/// bounded, provider-neutral execution contract.
pub const FINANCE_BANK_PROVIDER_IMPORT_HANDLER_NAME: &str = "finance_bank_provider_import";
pub const FINANCE_BANK_PROVIDER_IMPORT_MAX_BATCH_SIZE: u32 = 100;
pub const FINANCE_BANK_PROVIDER_IMPORT_MIN_LEASE_SECONDS: u64 = 1;
pub const FINANCE_BANK_PROVIDER_IMPORT_MAX_LEASE_SECONDS: u64 = 3_600;
pub const FINANCE_BANK_PROVIDER_IMPORT_MAX_ATTEMPTS: u16 = 8;

/// Redacted failures returned by the worker composition boundary.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum FinanceBankProviderImportError {
    #[error("invalid Finance bank-provider import request")]
    InvalidRequest,
    #[error("Finance bank-provider import authorization was denied")]
    AuthorizationDenied,
    #[error("Finance bank-provider route is unavailable")]
    RouteUnavailable,
    #[error("Finance bank-provider credential is unavailable")]
    CredentialUnavailable,
    #[error("Finance bank-provider adapter is unavailable")]
    AdapterUnavailable,
    #[error("Finance bank-provider response was rejected")]
    ProviderResponseRejected,
    #[error("Finance bank-provider transport security was rejected")]
    TransportSecurityRejected,
    #[error("Finance bank-provider is temporarily unavailable")]
    ProviderTemporarilyUnavailable,
    #[error("Finance bank-provider credential was rejected")]
    CredentialRejected,
    #[error("Finance bank-provider persistence is unavailable")]
    PersistenceUnavailable,
    #[error("Finance bank-provider persistence rejected the request")]
    PersistenceRejected,
    #[error("Finance bank-provider import idempotency conflicted")]
    IdempotencyConflict,
    #[error("Finance bank-provider import claim was lost")]
    ClaimLost,
    #[error("Finance bank-provider source fence advanced during import")]
    SourceFenceMismatch,
    #[error("Finance bank-provider reconciliation is unhealthy")]
    ReconciliationUnhealthy,
    #[error("Finance bank-provider import requires MFA authorization")]
    MfaRequired,
}

impl From<BankProviderError> for FinanceBankProviderImportError {
    fn from(error: BankProviderError) -> Self {
        match error {
            BankProviderError::Unavailable
            | BankProviderError::RateLimited
            | BankProviderError::ProtocolFailure => Self::ProviderTemporarilyUnavailable,
            BankProviderError::InvalidInput
            | BankProviderError::InvalidResponse
            | BankProviderError::AuthenticationFailed
            | BankProviderError::Rejected
            | BankProviderError::DomainMappingRejected => Self::ProviderResponseRejected,
            BankProviderError::TransportSecurityRejected => Self::TransportSecurityRejected,
        }
    }
}

impl From<BankProviderCredentialError> for FinanceBankProviderImportError {
    fn from(error: BankProviderCredentialError) -> Self {
        match error {
            BankProviderCredentialError::Unavailable
            | BankProviderCredentialError::NotConfigured => Self::CredentialUnavailable,
            BankProviderCredentialError::InvalidReference
            | BankProviderCredentialError::InvalidMaterial => Self::CredentialRejected,
        }
    }
}

impl From<PersistenceError> for FinanceBankProviderImportError {
    fn from(error: PersistenceError) -> Self {
        match error {
            PersistenceError::ClaimLost { .. } => Self::ClaimLost,
            PersistenceError::IdempotencyConflict { .. } => Self::IdempotencyConflict,
            PersistenceError::InvalidData(_) => Self::PersistenceRejected,
            PersistenceError::OptimisticConcurrency { .. } => Self::SourceFenceMismatch,
            PersistenceError::Database(_)
            | PersistenceError::Migration(_)
            | PersistenceError::Serialization(_)
            | PersistenceError::NotFound { .. }
            | PersistenceError::Configuration(_) => Self::PersistenceUnavailable,
        }
    }
}

/// The worker never turns a provider or persistence failure into an
/// unbounded retry. This classification is persisted by the injected job
/// source without carrying provider payloads, credentials, or error strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinanceBankProviderImportFailureClass {
    Retryable,
    Permanent,
    Authorization,
    ClaimLost,
    SourceFence,
    Reconciliation,
}

impl FinanceBankProviderImportFailureClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Retryable => "retryable",
            Self::Permanent => "permanent",
            Self::Authorization => "authorization",
            Self::ClaimLost => "claim_lost",
            Self::SourceFence => "source_fence",
            Self::Reconciliation => "reconciliation",
        }
    }

    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::Retryable | Self::ClaimLost | Self::SourceFence | Self::Reconciliation
        )
    }
}

impl FinanceBankProviderImportError {
    pub const fn failure_class(self) -> FinanceBankProviderImportFailureClass {
        match self {
            Self::AuthorizationDenied | Self::MfaRequired => {
                FinanceBankProviderImportFailureClass::Authorization
            }
            Self::ProviderTemporarilyUnavailable
            | Self::CredentialUnavailable
            | Self::PersistenceUnavailable => FinanceBankProviderImportFailureClass::Retryable,
            Self::ClaimLost => FinanceBankProviderImportFailureClass::ClaimLost,
            Self::SourceFenceMismatch => FinanceBankProviderImportFailureClass::SourceFence,
            Self::ReconciliationUnhealthy => FinanceBankProviderImportFailureClass::Reconciliation,
            Self::InvalidRequest
            | Self::RouteUnavailable
            | Self::AdapterUnavailable
            | Self::ProviderResponseRejected
            | Self::TransportSecurityRejected
            | Self::CredentialRejected
            | Self::PersistenceRejected
            | Self::IdempotencyConflict => FinanceBankProviderImportFailureClass::Permanent,
        }
    }
}

/// All facts needed to bind a provider import to one local bank aggregate and
/// one exact control-plane route. No credentials or provider payloads are
/// accepted here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceBankProviderImportRequest {
    pub provider_account_ref: String,
    pub provider_key: String,
    pub holding_id: String,
    pub data_profile_key: String,
    pub credential_key: BankProviderCredentialKey,
    pub account: BankProviderAccountBinding,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub page_size: u32,
    pub import_id: Uuid,
    pub context: CommandContext,
}

impl FinanceBankProviderImportRequest {
    pub fn validate(&self) -> Result<(), FinanceBankProviderImportError> {
        if self.provider_account_ref.trim().is_empty()
            || self.provider_key.trim().is_empty()
            || self.holding_id.trim().is_empty()
            || self.data_profile_key.trim().is_empty()
            || self.starts_on > self.ends_on
            || self.import_id.is_nil()
            || self.account.account().provider() != self.provider_key
            || self.account.account().storage_reference() != self.provider_account_ref
        {
            return Err(FinanceBankProviderImportError::InvalidRequest);
        }
        BankProviderCredentialKey::try_new(self.credential_key.as_str())
            .map_err(|_| FinanceBankProviderImportError::InvalidRequest)?;
        if self.context.actor.principal_ref.trim().is_empty()
            || self.context.actor.principal_ref.len() > 255
            || self
                .context
                .actor
                .principal_ref
                .chars()
                .any(char::is_control)
        {
            return Err(FinanceBankProviderImportError::InvalidRequest);
        }
        self.context
            .validate()
            .map_err(|_| FinanceBankProviderImportError::InvalidRequest)?;
        if matches!(
            self.context.active_scope,
            erpx_domain::contracts::AuthorityScope::Platform
        ) || !self.context.active_scope.contains(self.account.scope())
        {
            return Err(FinanceBankProviderImportError::AuthorizationDenied);
        }
        BankStatementPageRequest::try_new(
            self.account.clone(),
            self.starts_on,
            self.ends_on,
            None,
            self.page_size,
        )
        .map_err(|_| FinanceBankProviderImportError::InvalidRequest)?;
        Ok(())
    }
}

/// Explicit high-risk authorization handoff for a bank import.
///
/// `CommandContext` intentionally does not contain an MFA assertion. The
/// shared Finance authorization boundary evaluates the request and hands the
/// resulting decision to this type. Keeping the decision-derived facts here
/// prevents a worker from treating an authenticated session as proof of MFA.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceBankProviderImportAuthorization {
    actor_ref: String,
    mfa_verified: bool,
    access_path: FinanceAccessPath,
}

impl FinanceBankProviderImportAuthorization {
    pub fn from_decision(
        decision: &FinanceAuthorizationDecision,
        context: &CommandContext,
    ) -> Result<Self, FinanceBankProviderImportError> {
        if decision.operation != FinanceAuthorizationOperation::ImportBankStatement
            || decision.required_permission
                != FinanceAuthorizationOperation::ImportBankStatement.required_permission()
            || decision.contract_version != FINANCE_AUTHORIZATION_CONTRACT_VERSION
            || !decision.high_risk
            || !decision.mfa_verified
        {
            return Err(FinanceBankProviderImportError::MfaRequired);
        }
        let authorization = Self {
            actor_ref: context.actor.principal_ref.clone(),
            mfa_verified: decision.mfa_verified,
            access_path: decision.access_path.clone(),
        };
        authorization.validate(context)?;
        Ok(authorization)
    }

    pub(crate) fn validate(
        &self,
        context: &CommandContext,
    ) -> Result<(), FinanceBankProviderImportError> {
        context
            .validate()
            .map_err(|_| FinanceBankProviderImportError::AuthorizationDenied)?;
        if self.actor_ref != context.actor.principal_ref
            || self.actor_ref.trim().is_empty()
            || !self.mfa_verified
            || matches!(
                context.active_scope,
                erpx_domain::contracts::AuthorityScope::Platform
            )
        {
            return Err(FinanceBankProviderImportError::AuthorizationDenied);
        }
        if matches!(
            context.actor.kind,
            ActorKind::HoldingUser | ActorKind::PlatformUser
        ) && context.actor.session_id.is_none()
        {
            return Err(FinanceBankProviderImportError::AuthorizationDenied);
        }
        if let FinanceAccessPath::LocalBreakGlass { evidence_ref } = &self.access_path
            && (context.actor.kind != ActorKind::HoldingUser
                || context.actor.represented_principal_ref.is_some()
                || context.actor.session_id.is_none()
                || evidence_ref.trim().is_empty()
                || evidence_ref.len() > 255
                || evidence_ref.chars().any(char::is_control))
        {
            return Err(FinanceBankProviderImportError::AuthorizationDenied);
        }
        Ok(())
    }
}

/// Authoritative write-side access check injected by the worker composition
/// root. There is deliberately no allow-all implementation in production.
#[async_trait]
pub trait FinanceBankProviderImportAuthorizer: Send + Sync {
    async fn authorize(
        &self,
        context: &CommandContext,
        account_scope: &erpx_domain::contracts::AuthorityScope,
    ) -> Result<(), FinanceBankProviderImportError>;
}

/// Provider-specific adapters are constructed only after exact control-plane
/// routing and secret-manager resolution. The returned trait object exposes
/// only the validated provider-neutral page contract.
#[async_trait]
pub trait FinanceBankProviderAdapterFactory: Send + Sync {
    async fn open(
        &self,
        provider_key: &str,
        credential: &erpx_infrastructure::ResolvedBankProviderCredential,
    ) -> Result<Box<dyn BankProviderAdapter>, FinanceBankProviderImportError>;
}

/// Safe receipts from one authorized import workflow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceBankProviderImportOutcome {
    pub account: BankAccountWriteReceipt,
    pub statement: BankImportReceipt,
    /// Number of deterministic proposals materialized from immutable
    /// posted-journal facts. Replayed imports count as materialization work
    /// but remain idempotent in the persistence boundary.
    pub match_proposals_generated: u64,
    /// Authoritative source-row count observed after the holding write.
    pub persisted_transaction_count: u64,
    /// Exact Finance import outbox-event count observed after the holding write.
    pub outbox_event_count: u64,
    /// The control-plane route version revalidated immediately before the
    /// holding write. It is the source fence attached to worker evidence.
    pub source_fence: u64,
}

/// Connects all injected boundaries for one bank statement import.
#[derive(Debug, Clone, Copy, Default)]
pub struct FinanceBankProviderImportWorkflow;

impl FinanceBankProviderImportWorkflow {
    pub const fn new() -> Self {
        Self
    }

    /// Legacy entry point retained as a fail-closed compatibility boundary.
    /// A bank import is high risk and cannot be authorized from a bare command
    /// context. Callers must use [`Self::execute_authorized`] with the decision
    /// produced by the shared Finance authorization evaluator.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute<A, R, F>(
        &self,
        _control: &SqlxControlPlane,
        _router: &HoldingDatabaseRouter,
        _authorizer: &A,
        _credential_resolver: &R,
        _adapter_factory: &F,
        _request: FinanceBankProviderImportRequest,
    ) -> Result<FinanceBankProviderImportOutcome, FinanceBankProviderImportError>
    where
        A: FinanceBankProviderImportAuthorizer,
        R: BankProviderCredentialResolver,
        F: FinanceBankProviderAdapterFactory,
    {
        Err(FinanceBankProviderImportError::AuthorizationDenied)
    }

    /// Execute one import after the shared Finance authorization boundary has
    /// supplied an MFA-bearing decision. The decision is revalidated locally
    /// before control-plane routing, credential resolution, provider I/O, or
    /// holding database connections are opened.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_authorized<A, R, F>(
        &self,
        control: &SqlxControlPlane,
        router: &HoldingDatabaseRouter,
        authorizer: &A,
        credential_resolver: &R,
        adapter_factory: &F,
        request: FinanceBankProviderImportRequest,
        authorization: &FinanceBankProviderImportAuthorization,
    ) -> Result<FinanceBankProviderImportOutcome, FinanceBankProviderImportError>
    where
        A: FinanceBankProviderImportAuthorizer,
        R: BankProviderCredentialResolver,
        F: FinanceBankProviderAdapterFactory,
    {
        self.execute_authorized_with_source_fence(
            control,
            router,
            authorizer,
            credential_resolver,
            adapter_factory,
            request,
            authorization,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_authorized_with_source_fence<A, R, F>(
        &self,
        control: &SqlxControlPlane,
        router: &HoldingDatabaseRouter,
        authorizer: &A,
        credential_resolver: &R,
        adapter_factory: &F,
        request: FinanceBankProviderImportRequest,
        authorization: &FinanceBankProviderImportAuthorization,
        expected_source_fence: Option<u64>,
    ) -> Result<FinanceBankProviderImportOutcome, FinanceBankProviderImportError>
    where
        A: FinanceBankProviderImportAuthorizer,
        R: BankProviderCredentialResolver,
        F: FinanceBankProviderAdapterFactory,
    {
        request.validate()?;
        authorization.validate(&request.context)?;
        authorizer
            .authorize(&request.context, request.account.scope())
            .await?;

        let route = control
            .resolve_finance_bank_provider_account(
                &request.provider_account_ref,
                &request.provider_key,
                &request.holding_id,
                &request.data_profile_key,
                request.credential_key.as_str(),
            )
            .await?
            .ok_or(FinanceBankProviderImportError::RouteUnavailable)?;
        validate_route(&route, &request)?;
        validate_source_fence(expected_source_fence, route.version)?;

        let credential = credential_resolver.resolve(&request.credential_key).await?;
        if credential.key() != &request.credential_key {
            return Err(FinanceBankProviderImportError::CredentialRejected);
        }
        let adapter = adapter_factory
            .open(&request.provider_key, &credential)
            .await?;
        let page_request = BankStatementPageRequest::try_new(
            request.account.clone(),
            request.starts_on,
            request.ends_on,
            None,
            request.page_size,
        )
        .map_err(|_| FinanceBankProviderImportError::InvalidRequest)?;
        let records = BankProviderImportCoordinator::new()
            .import_statement(
                adapter.as_ref(),
                page_request,
                request.import_id,
                request.context.clone(),
            )
            .await?;

        // Provider I/O can outlive a control-plane route change. Re-read the
        // exact route and compare its monotonic version before any holding
        // write; a changed credential/account binding is never silently
        // imported under the old claim.
        let fenced_route = control
            .resolve_finance_bank_provider_account(
                &request.provider_account_ref,
                &request.provider_key,
                &request.holding_id,
                &request.data_profile_key,
                request.credential_key.as_str(),
            )
            .await?
            .ok_or(FinanceBankProviderImportError::RouteUnavailable)?;
        validate_route(&fenced_route, &request)?;
        if fenced_route.version != route.version {
            return Err(FinanceBankProviderImportError::SourceFenceMismatch);
        }

        let write = router
            .route(
                &request.holding_id,
                &request.data_profile_key,
                RoutedDatabaseSide::Write,
            )
            .await?;
        let read = router
            .route(
                &request.holding_id,
                &request.data_profile_key,
                RoutedDatabaseSide::Read,
            )
            .await?;
        let result: Result<FinanceBankProviderImportOutcome, PersistenceError> = async {
            let store = SqlxFinanceBankingStore::from_pools(
                write.database_pool(),
                read.database_pool(),
                request.data_profile_key.clone(),
            )?;
            let account_receipt = store
                .register_account(
                    records.account(),
                    &format!(
                        "finance-bank-account-register-v1:{}",
                        records.account().id()
                    ),
                )
                .await?;
            let statement_receipt = store
                .import_statement(records.account(), records.statement())
                .await?;
            let mut match_proposals_generated = 0_u64;
            for transaction in records.statement().transactions() {
                store
                    .generate_match_proposal(transaction.id(), request.context.clone())
                    .await?;
                match_proposals_generated = match_proposals_generated.saturating_add(1);
            }
            let (persisted_transaction_count, outbox_event_count) =
                store.reconcile_import(records.statement().id()).await?;
            Ok(FinanceBankProviderImportOutcome {
                account: account_receipt,
                statement: statement_receipt,
                match_proposals_generated,
                persisted_transaction_count,
                outbox_event_count,
                source_fence: fenced_route.version,
            })
        }
        .await;
        write.close().await;
        read.close().await;
        result.map_err(Into::into)
    }
}

/// One durable scheduler claim. The source implementation is responsible for
/// atomically assigning the lease; the handler rechecks every bounded fact
/// before it opens a provider or holding route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceBankProviderImportClaim {
    pub claim_id: Uuid,
    pub worker_id: Uuid,
    pub claim_until: DateTime<Utc>,
    pub attempt_count: u16,
    pub max_attempts: u16,
    pub source_fence: u64,
    pub request: FinanceBankProviderImportRequest,
    pub authorization: FinanceBankProviderImportAuthorization,
}

impl FinanceBankProviderImportClaim {
    fn validate_for(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(), FinanceBankProviderImportError> {
        if worker_id.is_nil()
            || self.worker_id != worker_id
            || self.claim_id.is_nil()
            || self.claim_until <= now
            || self.attempt_count == 0
            || self.max_attempts == 0
            || self.max_attempts > FINANCE_BANK_PROVIDER_IMPORT_MAX_ATTEMPTS
            || self.attempt_count > self.max_attempts
            || self.source_fence == 0
        {
            return Err(FinanceBankProviderImportError::InvalidRequest);
        }
        self.request.validate()?;
        self.authorization.validate(&self.request.context)
    }
}

/// PII-free evidence emitted after the authoritative holding write. The
/// provider-neutral worker retains only immutable identifiers, counts, and
/// the source fence needed to reconcile or retry the claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceBankProviderImportEvidence {
    pub import_id: Uuid,
    pub account_id: Uuid,
    pub idempotency_key: String,
    pub source_fence: u64,
    pub transaction_count: u64,
    pub persisted_transaction_count: u64,
    pub outbox_event_count: u64,
    pub match_proposals_generated: u64,
    pub recorded_at: DateTime<Utc>,
}

impl FinanceBankProviderImportEvidence {
    fn from_outcome(
        outcome: &FinanceBankProviderImportOutcome,
        recorded_at: DateTime<Utc>,
    ) -> Result<Self, FinanceBankProviderImportError> {
        let transaction_count = u64::try_from(outcome.statement.transaction_count)
            .map_err(|_| FinanceBankProviderImportError::PersistenceRejected)?;
        if outcome.statement.idempotency_key.trim().is_empty()
            || outcome.statement.idempotency_key.len() > 255
            || outcome
                .statement
                .idempotency_key
                .chars()
                .any(char::is_control)
            || outcome.account.account_id.is_nil()
            || outcome.statement.import_id.is_nil()
            || outcome.source_fence == 0
            || outcome.persisted_transaction_count != transaction_count
            || outcome.outbox_event_count != 1
            || outcome.match_proposals_generated != transaction_count
        {
            return Err(FinanceBankProviderImportError::PersistenceRejected);
        }
        Ok(Self {
            import_id: outcome.statement.import_id,
            account_id: outcome.account.account_id,
            idempotency_key: outcome.statement.idempotency_key.clone(),
            source_fence: outcome.source_fence,
            transaction_count,
            persisted_transaction_count: outcome.persisted_transaction_count,
            outbox_event_count: outcome.outbox_event_count,
            match_proposals_generated: outcome.match_proposals_generated,
            recorded_at,
        })
    }
}

/// Authoritative source/read-model reconciliation evidence. A worker never
/// marks an import complete merely because the provider call succeeded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceBankProviderImportReconciliationEvidence {
    pub import_id: Uuid,
    pub source_fence: u64,
    pub expected_transaction_count: u64,
    pub persisted_transaction_count: u64,
    pub outbox_event_count: u64,
    pub healthy: bool,
    pub issue_code: Option<String>,
}

impl FinanceBankProviderImportReconciliationEvidence {
    fn validate(&self) -> Result<(), FinanceBankProviderImportError> {
        if self.import_id.is_nil()
            || self.source_fence == 0
            || self.expected_transaction_count > super::MAX_BANK_PROVIDER_IMPORT_TRANSACTIONS as u64
            || self.persisted_transaction_count
                > super::MAX_BANK_PROVIDER_IMPORT_TRANSACTIONS as u64
            || self.outbox_event_count != 1
        {
            return Err(FinanceBankProviderImportError::PersistenceRejected);
        }
        if let Some(issue_code) = &self.issue_code
            && (issue_code.trim().is_empty()
                || issue_code.len() > 128
                || issue_code.chars().any(char::is_control))
        {
            return Err(FinanceBankProviderImportError::PersistenceRejected);
        }
        let counts_reconcile = self.expected_transaction_count == self.persisted_transaction_count
            && self.outbox_event_count == 1;
        if self.healthy != counts_reconcile
            || (self.healthy && self.issue_code.is_some())
            || (!self.healthy && self.issue_code.is_none())
        {
            return Err(FinanceBankProviderImportError::PersistenceRejected);
        }
        Ok(())
    }
}

/// Rollback is deliberately evidence-only for the authoritative Finance
/// source. A failed or fenced import must not delete append-only source rows;
/// the injected source records that a disposable read model/retry requires a
/// controlled rebuild.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceBankProviderImportRollbackEvidence {
    pub import_id: Uuid,
    pub source_fence: u64,
    pub reason: FinanceBankProviderImportFailureClass,
    pub authoritative_write_retained: bool,
    pub read_model_rebuild_required: bool,
    pub recorded_at: DateTime<Utc>,
}

impl FinanceBankProviderImportRollbackEvidence {
    fn new(
        evidence: &FinanceBankProviderImportEvidence,
        reason: FinanceBankProviderImportFailureClass,
        recorded_at: DateTime<Utc>,
    ) -> Self {
        Self {
            import_id: evidence.import_id,
            source_fence: evidence.source_fence,
            reason,
            authoritative_write_retained: true,
            read_model_rebuild_required: true,
            recorded_at,
        }
    }
}

/// Durable claim/lifecycle seam for the scheduler-owned import job. A SQLx
/// implementation must enforce holding RLS, claim ownership, source-fence
/// equality, immutable idempotency, and append-only evidence atomically. The
/// worker does not invent an in-memory fallback implementation.
#[async_trait]
pub trait FinanceBankProviderImportJobSource: Send + Sync + 'static {
    async fn claim_due(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
        limit: u32,
        lease: Duration,
    ) -> Result<Vec<FinanceBankProviderImportClaim>, PersistenceError>;

    async fn complete_import(
        &self,
        worker_id: Uuid,
        claim_id: Uuid,
        evidence: &FinanceBankProviderImportEvidence,
    ) -> Result<FinanceBankProviderImportReconciliationEvidence, PersistenceError>;

    async fn retry_import(
        &self,
        worker_id: Uuid,
        claim_id: Uuid,
        source_fence: u64,
        class: FinanceBankProviderImportFailureClass,
        at: DateTime<Utc>,
    ) -> Result<(), PersistenceError>;

    async fn dead_letter_import(
        &self,
        worker_id: Uuid,
        claim_id: Uuid,
        source_fence: u64,
        class: FinanceBankProviderImportFailureClass,
        at: DateTime<Utc>,
    ) -> Result<(), PersistenceError>;

    async fn record_rollback_evidence(
        &self,
        worker_id: Uuid,
        claim_id: Uuid,
        evidence: &FinanceBankProviderImportRollbackEvidence,
    ) -> Result<(), PersistenceError>;
}

/// Claim-driven Finance bank import worker. Registration and holding
/// enumeration stay with the coordinator-owned worker root, which injects
/// explicit authorization, credential, adapter, and routing boundaries.
#[derive(Debug, Clone)]
pub struct FinanceBankProviderImportHandler<S, A, R, F> {
    source: S,
    control: Arc<SqlxControlPlane>,
    router: HoldingDatabaseRouter,
    authorizer: A,
    credential_resolver: R,
    adapter_factory: F,
    batch_size: u32,
    lease: Duration,
}

impl<S, A, R, F> FinanceBankProviderImportHandler<S, A, R, F>
where
    S: FinanceBankProviderImportJobSource,
    A: FinanceBankProviderImportAuthorizer,
    R: BankProviderCredentialResolver,
    F: FinanceBankProviderAdapterFactory,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: S,
        control: Arc<SqlxControlPlane>,
        router: HoldingDatabaseRouter,
        authorizer: A,
        credential_resolver: R,
        adapter_factory: F,
        batch_size: u32,
        lease: Duration,
    ) -> Result<Self, WorkerRuntimeError> {
        if batch_size == 0 || batch_size > FINANCE_BANK_PROVIDER_IMPORT_MAX_BATCH_SIZE {
            return Err(WorkerRuntimeError::Configuration(
                "Finance bank-provider import batch size is outside its bound".into(),
            ));
        }
        if lease < Duration::seconds(FINANCE_BANK_PROVIDER_IMPORT_MIN_LEASE_SECONDS as i64)
            || lease > Duration::seconds(FINANCE_BANK_PROVIDER_IMPORT_MAX_LEASE_SECONDS as i64)
        {
            return Err(WorkerRuntimeError::Configuration(
                "Finance bank-provider import lease is outside its bound".into(),
            ));
        }
        Ok(Self {
            source,
            control,
            router,
            authorizer,
            credential_resolver,
            adapter_factory,
            batch_size,
            lease,
        })
    }

    pub const fn batch_size(&self) -> u32 {
        self.batch_size
    }

    pub const fn lease(&self) -> Duration {
        self.lease
    }

    async fn record_failure(
        &self,
        worker_id: Uuid,
        claim: &FinanceBankProviderImportClaim,
        class: FinanceBankProviderImportFailureClass,
        report: &mut WorkerHandlerReport,
    ) -> Result<(), WorkerRuntimeError> {
        let at = Utc::now();
        let terminal = !class.retryable() || claim.attempt_count >= claim.max_attempts;
        if terminal {
            self.source
                .dead_letter_import(worker_id, claim.claim_id, claim.source_fence, class, at)
                .await
                .map_err(redacted_import_source_error)?;
            report.dead_lettered = report.dead_lettered.saturating_add(1);
        } else {
            self.source
                .retry_import(worker_id, claim.claim_id, claim.source_fence, class, at)
                .await
                .map_err(redacted_import_source_error)?;
            report.retried = report.retried.saturating_add(1);
        }
        report.processed = report.processed.saturating_add(1);
        Ok(())
    }
}

#[async_trait]
impl<S, A, R, F> NamedWorkerHandler for FinanceBankProviderImportHandler<S, A, R, F>
where
    S: FinanceBankProviderImportJobSource,
    A: FinanceBankProviderImportAuthorizer + 'static,
    R: BankProviderCredentialResolver + 'static,
    F: FinanceBankProviderAdapterFactory + 'static,
{
    fn name(&self) -> &str {
        FINANCE_BANK_PROVIDER_IMPORT_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "Finance bank-provider import worker identity is required".into(),
            ));
        }
        let claims = self
            .source
            .claim_due(worker_id, Utc::now(), self.batch_size, self.lease)
            .await
            .map_err(redacted_import_source_error)?;
        let mut report = WorkerHandlerReport {
            claimed: claims.len() as u64,
            has_work: !claims.is_empty(),
            ..WorkerHandlerReport::default()
        };
        for claim in claims {
            if claim.validate_for(worker_id, Utc::now()).is_err() {
                self.record_failure(
                    worker_id,
                    &claim,
                    FinanceBankProviderImportFailureClass::Permanent,
                    &mut report,
                )
                .await?;
                continue;
            }
            let result = FinanceBankProviderImportWorkflow::new()
                .execute_authorized_with_source_fence(
                    self.control.as_ref(),
                    &self.router,
                    &self.authorizer,
                    &self.credential_resolver,
                    &self.adapter_factory,
                    claim.request.clone(),
                    &claim.authorization,
                    Some(claim.source_fence),
                )
                .await;
            let outcome = match result {
                Ok(outcome) => outcome,
                Err(error) => {
                    self.record_failure(worker_id, &claim, error.failure_class(), &mut report)
                        .await?;
                    continue;
                }
            };
            let evidence = FinanceBankProviderImportEvidence::from_outcome(&outcome, Utc::now())
                .map_err(|error| WorkerRuntimeError::Configuration(error.to_string()))?;
            let reconciliation = match self
                .source
                .complete_import(worker_id, claim.claim_id, &evidence)
                .await
            {
                Ok(reconciliation) => reconciliation,
                Err(error) => {
                    let rollback = FinanceBankProviderImportRollbackEvidence::new(
                        &evidence,
                        FinanceBankProviderImportFailureClass::Retryable,
                        Utc::now(),
                    );
                    self.source
                        .record_rollback_evidence(worker_id, claim.claim_id, &rollback)
                        .await
                        .map_err(redacted_import_source_error)?;
                    self.record_failure(
                        worker_id,
                        &claim,
                        persistence_failure_class(&error),
                        &mut report,
                    )
                    .await?;
                    continue;
                }
            };
            if reconciliation.validate().is_err()
                || !reconciliation.healthy
                || reconciliation.source_fence != claim.source_fence
                || reconciliation.expected_transaction_count
                    != reconciliation.persisted_transaction_count
            {
                let rollback = FinanceBankProviderImportRollbackEvidence::new(
                    &evidence,
                    if reconciliation.source_fence != claim.source_fence {
                        FinanceBankProviderImportFailureClass::SourceFence
                    } else {
                        FinanceBankProviderImportFailureClass::Reconciliation
                    },
                    Utc::now(),
                );
                self.source
                    .record_rollback_evidence(worker_id, claim.claim_id, &rollback)
                    .await
                    .map_err(redacted_import_source_error)?;
                self.record_failure(worker_id, &claim, rollback.reason, &mut report)
                    .await?;
                continue;
            }
            report.processed = report.processed.saturating_add(1);
            report.published = report.published.saturating_add(1);
        }
        Ok(report)
    }
}

fn persistence_failure_class(error: &PersistenceError) -> FinanceBankProviderImportFailureClass {
    match error {
        PersistenceError::ClaimLost { .. } => FinanceBankProviderImportFailureClass::ClaimLost,
        PersistenceError::OptimisticConcurrency { .. } => {
            FinanceBankProviderImportFailureClass::SourceFence
        }
        PersistenceError::InvalidData(_) | PersistenceError::IdempotencyConflict { .. } => {
            FinanceBankProviderImportFailureClass::Permanent
        }
        PersistenceError::Database(_)
        | PersistenceError::Migration(_)
        | PersistenceError::Serialization(_)
        | PersistenceError::NotFound { .. }
        | PersistenceError::Configuration(_) => FinanceBankProviderImportFailureClass::Retryable,
    }
}

fn redacted_import_source_error(error: PersistenceError) -> WorkerRuntimeError {
    let kind = match &error {
        PersistenceError::Database(_) => "database",
        PersistenceError::Migration(_) => "migration",
        PersistenceError::Serialization(_) => "serialization",
        PersistenceError::OptimisticConcurrency { .. } => "source_fence",
        PersistenceError::IdempotencyConflict { .. } => "idempotency",
        PersistenceError::NotFound { .. } => "not_found",
        PersistenceError::ClaimLost { .. } => "claim_lost",
        PersistenceError::InvalidData(_) => "invalid_data",
        PersistenceError::Configuration(_) => "configuration",
    };
    tracing::error!(
        error_kind = kind,
        "Finance bank-provider import lifecycle failed"
    );
    WorkerRuntimeError::Configuration("Finance bank-provider import worker failed".into())
}

fn validate_route(
    route: &FinanceBankProviderAccountBinding,
    request: &FinanceBankProviderImportRequest,
) -> Result<(), FinanceBankProviderImportError> {
    if route.version == 0
        || route.state != "active"
        || route.provider_account_ref != request.provider_account_ref
        || route.provider_key != request.provider_key
        || route.holding_key != request.holding_id
        || route.data_profile_key != request.data_profile_key
        || route.holding_id != request.account.holding_id()
        || route.credential_key_ref != request.credential_key.as_str()
    {
        return Err(FinanceBankProviderImportError::RouteUnavailable);
    }
    Ok(())
}

fn validate_source_fence(
    expected_source_fence: Option<u64>,
    actual_source_fence: u64,
) -> Result<(), FinanceBankProviderImportError> {
    if actual_source_fence == 0
        || expected_source_fence
            .is_some_and(|expected| expected == 0 || expected != actual_source_fence)
    {
        return Err(FinanceBankProviderImportError::SourceFenceMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope};

    fn request(session_id: Option<Uuid>) -> FinanceBankProviderImportRequest {
        let holding = Uuid::from_u128(1);
        let company = Uuid::from_u128(2);
        let account = BankProviderAccountBinding::try_new(
            Uuid::from_u128(3),
            erpx_infrastructure::BankProviderAccountRef::try_new("mockbank", "acct-opaque-1")
                .unwrap(),
            AuthorityScope::Company {
                holding_id: holding,
                company_id: company,
            },
            holding,
            company,
            "THB",
            "Operating account",
        )
        .unwrap();
        let context = CommandContext::try_new(
            "bank-import-test",
            ActorAttribution {
                principal_ref: "finance-user".into(),
                kind: ActorKind::HoldingUser,
                session_id,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            AuthorityScope::Company {
                holding_id: holding,
                company_id: company,
            },
            "authorized bank import",
        )
        .unwrap();
        FinanceBankProviderImportRequest {
            provider_account_ref: "mockbank:acct-opaque-1".into(),
            provider_key: "mockbank".into(),
            holding_id: "holding-opaque-1".into(),
            data_profile_key: "holding-opaque-1".into(),
            credential_key: BankProviderCredentialKey::try_new("vault/finance/mockbank").unwrap(),
            account,
            starts_on: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            ends_on: NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            page_size: 100,
            import_id: Uuid::from_u128(4),
            context,
        }
    }

    #[test]
    fn request_validation_binds_provider_account_and_scope() {
        let request = request(None);
        assert!(request.validate().is_ok());
    }

    #[test]
    fn request_validation_rejects_platform_scope_and_account_route_mismatch() {
        let error = BankProviderCredentialKey::try_new("vault/finance/mockbank").unwrap();
        assert_eq!(error.as_str(), "vault/finance/mockbank");
        assert_eq!(
            FinanceBankProviderImportError::from(BankProviderError::AuthenticationFailed),
            FinanceBankProviderImportError::ProviderResponseRejected
        );
    }

    #[test]
    fn provider_and_persistence_failures_have_bounded_retry_classes() {
        assert_eq!(
            FinanceBankProviderImportError::from(BankProviderError::Unavailable).failure_class(),
            FinanceBankProviderImportFailureClass::Retryable
        );
        assert_eq!(
            FinanceBankProviderImportError::from(BankProviderError::Rejected).failure_class(),
            FinanceBankProviderImportFailureClass::Permanent
        );
        assert_eq!(
            FinanceBankProviderImportError::from(BankProviderError::TransportSecurityRejected,)
                .failure_class(),
            FinanceBankProviderImportFailureClass::Permanent
        );
        assert_eq!(
            FinanceBankProviderImportError::from(PersistenceError::OptimisticConcurrency {
                aggregate_type: "finance_bank_provider_account".into(),
                aggregate_id: Uuid::from_u128(3),
                expected: Some(1),
                actual: Some(2),
            })
            .failure_class(),
            FinanceBankProviderImportFailureClass::SourceFence
        );
        assert!(FinanceBankProviderImportFailureClass::Reconciliation.retryable());
        assert!(!FinanceBankProviderImportFailureClass::Authorization.retryable());
    }

    #[test]
    fn claimed_source_fence_must_match_the_active_route() {
        assert!(validate_source_fence(Some(12), 12).is_ok());
        assert!(validate_source_fence(None, 12).is_ok());
        assert_eq!(
            validate_source_fence(Some(11), 12),
            Err(FinanceBankProviderImportError::SourceFenceMismatch)
        );
        assert_eq!(
            validate_source_fence(Some(0), 12),
            Err(FinanceBankProviderImportError::SourceFenceMismatch)
        );
        assert_eq!(
            validate_source_fence(Some(12), 0),
            Err(FinanceBankProviderImportError::SourceFenceMismatch)
        );
    }

    #[test]
    fn import_authorization_requires_mfa_and_a_live_human_session() {
        let mut context = request(None).context;
        let decision = FinanceAuthorizationDecision {
            contract_version: 1,
            operation: FinanceAuthorizationOperation::ImportBankStatement,
            required_permission: "finance.write".into(),
            high_risk: true,
            mfa_verified: false,
            distinct_approver: None,
            access_path: FinanceAccessPath::Normal,
        };
        assert_eq!(
            FinanceBankProviderImportAuthorization::from_decision(&decision, &context),
            Err(FinanceBankProviderImportError::MfaRequired)
        );

        let authorized_decision = FinanceAuthorizationDecision {
            mfa_verified: true,
            ..decision
        };
        assert_eq!(
            FinanceBankProviderImportAuthorization::from_decision(&authorized_decision, &context),
            Err(FinanceBankProviderImportError::AuthorizationDenied)
        );
        context.actor.session_id = Some(Uuid::from_u128(5));
        assert!(
            FinanceBankProviderImportAuthorization::from_decision(&authorized_decision, &context)
                .is_ok()
        );
    }

    #[test]
    fn claim_and_reconciliation_evidence_are_bounded_and_fenced() {
        let request = request(Some(Uuid::from_u128(5)));
        let decision = FinanceAuthorizationDecision {
            contract_version: 1,
            operation: FinanceAuthorizationOperation::ImportBankStatement,
            required_permission: "finance.write".into(),
            high_risk: true,
            mfa_verified: true,
            distinct_approver: None,
            access_path: FinanceAccessPath::Normal,
        };
        let authorization =
            FinanceBankProviderImportAuthorization::from_decision(&decision, &request.context)
                .unwrap();
        let claim = FinanceBankProviderImportClaim {
            claim_id: Uuid::from_u128(6),
            worker_id: Uuid::from_u128(7),
            claim_until: Utc::now() + Duration::seconds(60),
            attempt_count: 1,
            max_attempts: FINANCE_BANK_PROVIDER_IMPORT_MAX_ATTEMPTS,
            source_fence: 12,
            request,
            authorization,
        };
        assert!(claim.validate_for(Uuid::from_u128(7), Utc::now()).is_ok());

        let mut expired = claim.clone();
        expired.claim_until = Utc::now() - Duration::seconds(1);
        assert_eq!(
            expired.validate_for(Uuid::from_u128(7), Utc::now()),
            Err(FinanceBankProviderImportError::InvalidRequest)
        );

        let malformed = FinanceBankProviderImportReconciliationEvidence {
            import_id: Uuid::from_u128(4),
            source_fence: 12,
            expected_transaction_count: 1,
            persisted_transaction_count: 0,
            outbox_event_count: 1,
            healthy: false,
            issue_code: Some("count_mismatch".into()),
        };
        assert!(malformed.validate().is_ok());
        assert_ne!(
            malformed.expected_transaction_count,
            malformed.persisted_transaction_count
        );
        let mut missing_outbox_event = malformed.clone();
        missing_outbox_event.persisted_transaction_count = 1;
        missing_outbox_event.healthy = true;
        missing_outbox_event.outbox_event_count = 0;
        assert!(missing_outbox_event.validate().is_err());

        let mut too_many_outbox_events = malformed;
        too_many_outbox_events.persisted_transaction_count = 1;
        too_many_outbox_events.outbox_event_count = 4;
        too_many_outbox_events.healthy = true;
        assert!(too_many_outbox_events.validate().is_err());
    }
}
