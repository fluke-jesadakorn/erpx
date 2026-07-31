//! Main-owned SQLx adapter for the registration-free Marketing preview
//! expiry worker seam.

use super::marketing_content_workflow::{
    MarketingContentPreviewExpiryApplyResult, MarketingContentPreviewExpiryCandidate,
    MarketingContentPreviewExpiryExecutor, MarketingContentPreviewExpiryItemError,
    MarketingContentPreviewExpiryItemRequest, MarketingContentPreviewExpiryRequest,
};
use async_trait::async_trait;
use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope};
use erpx_infrastructure::{PersistenceError, SqlxMarketingContentStore};
use uuid::Uuid;

use super::marketing_content_workflow::{
    MARKETING_CONTENT_PREVIEW_EXPIRY_MAX_BATCH_SIZE, MarketingContentPreviewExpiryWorkerError,
};

#[derive(Debug, Clone)]
pub struct SqlxMarketingContentPreviewExpiryExecutor {
    store: SqlxMarketingContentStore,
    tenant_id: String,
    scope: TenantScope,
    actor_ref: String,
}

impl SqlxMarketingContentPreviewExpiryExecutor {
    pub fn new(
        store: SqlxMarketingContentStore,
        tenant_id: impl Into<String>,
        scope: TenantScope,
        actor_ref: impl Into<String>,
    ) -> Result<Self, MarketingContentPreviewExpiryWorkerError> {
        let tenant_id = tenant_id.into();
        let actor_ref = actor_ref.into();
        if tenant_id.trim().is_empty()
            || tenant_id.len() > 255
            || actor_ref.trim().is_empty()
            || actor_ref.len() > 255
        {
            return Err(MarketingContentPreviewExpiryWorkerError::Configuration(
                "preview expiry route references are invalid".into(),
            ));
        }
        if matches!(scope, TenantScope::Platform) {
            return Err(MarketingContentPreviewExpiryWorkerError::Configuration(
                "preview expiry requires tenant scope".into(),
            ));
        }
        Ok(Self {
            store,
            tenant_id,
            scope,
            actor_ref,
        })
    }

    fn ensure_route(
        &self,
        tenant_id: &str,
        holding_ref: &str,
    ) -> Result<(), MarketingContentPreviewExpiryWorkerError> {
        if tenant_id != self.tenant_id || holding_ref != self.store.holding_ref() {
            return Err(MarketingContentPreviewExpiryWorkerError::Configuration(
                "preview expiry route does not match the registered tenant holding".into(),
            ));
        }
        Ok(())
    }

    fn context(
        &self,
        operation_key: &str,
    ) -> Result<CommandContext, MarketingContentPreviewExpiryItemError> {
        let actor = ActorAttribution {
            principal_ref: self.actor_ref.clone(),
            kind: ActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        };
        let mut context = CommandContext::try_new(
            operation_key,
            actor,
            self.scope.clone(),
            "marketing.content.preview.expiry",
        )
        .map_err(|_| MarketingContentPreviewExpiryItemError::Permanent)?;
        let correlation_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, operation_key.as_bytes());
        context.command_id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("command:{operation_key}").as_bytes(),
        );
        context.correlation_id = correlation_id;
        context.audit.correlation_id = correlation_id;
        Ok(context)
    }
}

#[async_trait]
impl MarketingContentPreviewExpiryExecutor for SqlxMarketingContentPreviewExpiryExecutor {
    async fn list_due_previews(
        &self,
        request: &MarketingContentPreviewExpiryRequest,
    ) -> Result<Vec<MarketingContentPreviewExpiryCandidate>, MarketingContentPreviewExpiryWorkerError>
    {
        self.ensure_route(&request.tenant_id, &request.holding_ref)?;
        let ids = self
            .store
            .list_due_preview_ids(
                &self.scope,
                request.now,
                request
                    .batch_size
                    .min(MARKETING_CONTENT_PREVIEW_EXPIRY_MAX_BATCH_SIZE),
            )
            .await
            .map_err(|_| MarketingContentPreviewExpiryWorkerError::Source)?;
        Ok(ids
            .into_iter()
            .map(|preview_id| MarketingContentPreviewExpiryCandidate { preview_id })
            .collect())
    }

    async fn expire_due_preview(
        &self,
        request: &MarketingContentPreviewExpiryItemRequest,
    ) -> Result<MarketingContentPreviewExpiryApplyResult, MarketingContentPreviewExpiryItemError>
    {
        self.ensure_route(&request.tenant_id, &request.holding_ref)
            .map_err(|_| MarketingContentPreviewExpiryItemError::Permanent)?;
        let context = self.context(&request.operation_key)?;
        let receipt = self
            .store
            .expire_preview(
                request.preview_id,
                self.scope.clone(),
                request.now,
                &context,
            )
            .await
            .map_err(classify_persistence_error)?;
        if receipt.replayed {
            Ok(MarketingContentPreviewExpiryApplyResult::AlreadyExpired)
        } else {
            Ok(MarketingContentPreviewExpiryApplyResult::Expired)
        }
    }
}

fn classify_persistence_error(error: PersistenceError) -> MarketingContentPreviewExpiryItemError {
    match error {
        PersistenceError::Database(_) | PersistenceError::Migration(_) => {
            MarketingContentPreviewExpiryItemError::Retryable
        }
        PersistenceError::InvalidData(_)
        | PersistenceError::Configuration(_)
        | PersistenceError::Serialization(_)
        | PersistenceError::OptimisticConcurrency { .. }
        | PersistenceError::IdempotencyConflict { .. }
        | PersistenceError::NotFound { .. }
        | PersistenceError::ClaimLost { .. } => MarketingContentPreviewExpiryItemError::Permanent,
    }
}
