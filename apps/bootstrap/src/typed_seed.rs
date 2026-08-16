use async_trait::async_trait;
use erpx_brand::{
    AuthorizationPort, AuthorizationRequest, BrandAction, BrandCommandHandler, BrandDefinition,
    BrandProfileId, BrandScope, CreateBrandDraft, EntitlementPort, PortError, PublishBrandDraft,
};
use erpx_brand_sqlx::{BrandCommandAdapter, BrandParentAuthorityRepository, BrandWriteRepository};
use erpx_cqrs::{CONTRACT_VERSION, CommandEnvelope, CommandHandler, IdempotencyKey};
use erpx_cqrs_sqlx::WritePool;
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, SystemClock, TenantScope,
};
use sqlx::PgPool;

use crate::{BootstrapError, Result, stable_uuid};

#[derive(Debug, Clone)]
struct BootstrapBrandAccess {
    root_principal: String,
}

#[async_trait]
impl AuthorizationPort for BootstrapBrandAccess {
    async fn authorize(
        &self,
        request: AuthorizationRequest<'_>,
    ) -> std::result::Result<(), PortError> {
        let root_is_authoritative = request.actor.principal_ref == self.root_principal
            && request.actor.kind == ActorKind::PlatformUser
            && matches!(request.authority_scope, TenantScope::Platform)
            && matches!(request.target, BrandScope::Platform)
            && matches!(
                request.action,
                BrandAction::CreateDraft | BrandAction::Publish
            );
        if root_is_authoritative {
            Ok(())
        } else {
            Err(PortError::Denied)
        }
    }
}

#[async_trait]
impl EntitlementPort for BootstrapBrandAccess {
    async fn ensure_brand_enabled(
        &self,
        scope: &TenantScope,
        write: bool,
    ) -> std::result::Result<(), PortError> {
        if write && matches!(scope, TenantScope::Platform) {
            Ok(())
        } else {
            Err(PortError::Denied)
        }
    }
}

pub(crate) async fn seed_typed_neutral_brand(pool: &PgPool, root_principal: &str) -> Result<()> {
    let write = WritePool::from_control_pg_pool(pool.clone());
    let access = BootstrapBrandAccess {
        root_principal: root_principal.to_owned(),
    };
    let handler = BrandCommandHandler::new(
        BrandWriteRepository::new(write.clone(), pool.clone()),
        BrandCommandAdapter::new(write),
        access.clone(),
        access,
        BrandParentAuthorityRepository::new(pool.clone()),
        SystemClock,
    );
    let profile_id =
        BrandProfileId::try_from_uuid(stable_uuid("typed-brand-profile", "platform-neutral"))
            .map_err(|_| BootstrapError("typed neutral brand identifier is invalid"))?;
    let created = handler
        .handle(command_envelope(
            root_principal,
            "brand-create",
            None,
            CreateBrandDraft {
                profile_id,
                scope: BrandScope::Platform,
                definition: BrandDefinition::neutral_platform(),
                reason: "fresh-state canonical neutral brand".into(),
            },
        )?)
        .await
        .map_err(|_| BootstrapError("typed neutral brand draft could not be created"))?;
    let published = handler
        .handle(command_envelope(
            root_principal,
            "brand-publish",
            Some(created.aggregate.version),
            PublishBrandDraft {
                profile_id,
                reason: "fresh-state canonical neutral brand publication".into(),
            },
        )?)
        .await
        .map_err(|_| BootstrapError("typed neutral brand could not be published"))?;
    if published.response.profile.published.is_none()
        || published.response.profile.scope != BrandScope::Platform
    {
        return Err(BootstrapError(
            "typed neutral brand publication receipt is invalid",
        ));
    }

    let exact_seed: bool = sqlx::query_scalar(
        "SELECT
            (SELECT count(*) FROM erpx_brand_write.profile_guards) = 1
            AND (SELECT count(*) FROM erpx_brand_write.profile_revisions) = 1
            AND EXISTS(
                SELECT 1 FROM erpx_brand_write.profile_guards
                WHERE profile_id=$1 AND scope_key='platform'
                  AND published_revision IS NOT NULL
            )",
    )
    .bind(profile_id.as_uuid())
    .fetch_one(pool)
    .await
    .map_err(|_| BootstrapError("typed neutral brand seed could not be verified"))?;
    if !exact_seed {
        return Err(BootstrapError(
            "fresh-state typed brand seed contains unexpected records",
        ));
    }
    Ok(())
}

fn command_envelope<C>(
    root_principal: &str,
    command_kind: &str,
    expected_aggregate_version: Option<u64>,
    command: C,
) -> Result<CommandEnvelope<C>> {
    let command_id = CommandId::try_from_uuid(stable_uuid("typed-brand-command", command_kind))
        .map_err(|_| BootstrapError("typed brand command identifier is invalid"))?;
    let correlation_id =
        CorrelationId::try_from_uuid(stable_uuid("typed-brand-correlation", command_kind))
            .map_err(|_| BootstrapError("typed brand correlation identifier is invalid"))?;
    Ok(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id,
        idempotency_key: IdempotencyKey::try_new(format!("fresh-state:brand:{command_kind}:v1"))
            .map_err(|_| BootstrapError("typed brand idempotency key is invalid"))?,
        correlation_id,
        causation_id: None,
        actor: ActorAttribution {
            principal_ref: root_principal.to_owned(),
            kind: ActorKind::PlatformUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        tenant_scope: TenantScope::Platform,
        audit: AuditContext {
            purpose: "fresh-state neutral brand seed".into(),
            customer_visible: false,
            module: Some("fresh_state_bootstrap".into()),
        },
        expected_aggregate_version,
        command,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn typed_brand_seed_source_uses_command_handler_and_no_direct_writes() {
        let source = include_str!("typed_seed.rs");
        assert!(source.contains("BrandCommandHandler::new"));
        assert!(!source.contains(concat!("INSERT", " INTO erpx_brand_write")));
        assert!(!source.contains(concat!("DELETE", " FROM")));
    }
}
