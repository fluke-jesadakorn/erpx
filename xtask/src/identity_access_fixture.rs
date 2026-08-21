//! Versioned development records for the typed Identity & Access owner.
//!
//! This fixture deliberately enters through the application command handlers
//! and advances the production projectors. It never treats a read-model row as
//! authority and does not bypass the idempotency, version, audit, or outbox
//! boundaries used by the live service.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{CONTRACT_VERSION, CommandEnvelope, CommandHandler, IdempotencyKey};
use erpx_cqrs_sqlx::{
    ProjectionPool, ProjectionRunner, ProjectionRunnerConfig, ProjectionSource, ProjectionStore,
    WritePool,
};
use erpx_identity_access::{
    ActivatePrincipal, AssignRole, AuthorizationPort, CreatePrincipal, DefineRole, EntitlementPort,
    IdentityAccessCommandHandler, PermissionKey, PortError, PrincipalId, RiskTier,
    RoleAssignmentId, RoleCode, RoleId, SuspendPrincipal, ports::AuthorizationRequest,
};
use erpx_identity_access_sqlx::{
    IdentityAccessCommandAdapter, IdentityAccessPrincipalProjector, IdentityAccessRoleProjector,
    IdentityAccessWriteRepository, PRINCIPAL_PROJECTION_NAME, ROLE_PROJECTION_NAME,
};
use erpx_infrastructure::holding_holding_scope_id;
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId, SessionId,
    SystemClock,
};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    DATA_PROFILE_KEY, DATABASES, DevConfig, RESTRICTED_DATA_PROFILE_KEY, TaskResult, pool,
};

const FIXTURE_CONTRACT: &str = "identity-access-owner-v1";
const ROOT_PRINCIPAL: &str = "local:platform-root";
const PAGE_SIZE: u32 = 100;

#[derive(Debug, Clone)]
struct FixtureAccess {
    holding_id: Uuid,
}

#[async_trait]
impl AuthorizationPort for FixtureAccess {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError> {
        let expected_scope = AuthorityScope::Holding {
            holding_id: self.holding_id,
        };
        if request.actor.kind == ActorKind::PlatformUser
            && request.actor.principal_ref == ROOT_PRINCIPAL
            && request.authority_scope == &expected_scope
            && request.target_scope == &expected_scope
        {
            Ok(())
        } else {
            Err(PortError::Denied)
        }
    }
}

#[async_trait]
impl EntitlementPort for FixtureAccess {
    async fn ensure_identity_access_enabled(
        &self,
        scope: &AuthorityScope,
        write: bool,
    ) -> Result<(), PortError> {
        if write
            && scope
                == &(AuthorityScope::Holding {
                    holding_id: self.holding_id,
                })
        {
            Ok(())
        } else {
            Err(PortError::Denied)
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FixtureIds {
    administrator: PrincipalId,
    pending_reviewer: PrincipalId,
    suspended_auditor: PrincipalId,
    security_role: RoleId,
    auditor_role: RoleId,
    administrator_grant: RoleAssignmentId,
}

impl FixtureIds {
    fn for_profile(data_profile_key: &str) -> TaskResult<Self> {
        let id = |kind: &str| {
            Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!("erpx:development:{FIXTURE_CONTRACT}:{data_profile_key}:{kind}").as_bytes(),
            )
        };
        Ok(Self {
            administrator: PrincipalId::try_from_uuid(id("principal:administrator"))?,
            pending_reviewer: PrincipalId::try_from_uuid(id("principal:pending-reviewer"))?,
            suspended_auditor: PrincipalId::try_from_uuid(id("principal:suspended-auditor"))?,
            security_role: RoleId::try_from_uuid(id("role:security-administrator"))?,
            auditor_role: RoleId::try_from_uuid(id("role:auditor"))?,
            administrator_grant: RoleAssignmentId::try_from_uuid(id("grant:administrator"))?,
        })
    }
}

pub(super) async fn seed(config: &DevConfig) -> TaskResult {
    for (write_database, read_database, data_profile_key, label) in fixture_routes() {
        seed_holding(
            pool(&config.database_url(write_database)).await?,
            pool(&config.database_url(read_database)).await?,
            data_profile_key,
            label,
        )
        .await?;
    }
    Ok(())
}

pub(super) async fn is_current(config: &DevConfig) -> TaskResult<bool> {
    for (write_database, read_database, data_profile_key, _) in fixture_routes() {
        let holding_id = holding_holding_scope_id(data_profile_key);
        let write = pool(&config.database_url(write_database)).await?;
        let read = pool(&config.database_url(read_database)).await?;
        let ids = FixtureIds::for_profile(data_profile_key)?;
        let write_capability = WritePool::for_holding(write.clone(), holding_id)?;
        let mut write_transaction = write_capability.begin().await?;
        let commands_current: bool = sqlx::query_scalar(
            "SELECT
                EXISTS(
                    SELECT 1 FROM erpx_identity_access_write.principal_authority
                    WHERE holding_id=$1 AND principal_id = ANY($2)
                )
                AND EXISTS(
                    SELECT 1 FROM erpx_identity_access_write.role_authority
                    WHERE holding_id=$1 AND role_id = ANY($3)
                )
                AND EXISTS(
                    SELECT 1 FROM erpx_identity_access_write.principal_authority
                    WHERE holding_id=$1 AND principal_id=$4
                      AND principal_status='suspended'
                )",
        )
        .bind(holding_id)
        .bind([
            ids.administrator.as_uuid(),
            ids.pending_reviewer.as_uuid(),
            ids.suspended_auditor.as_uuid(),
        ])
        .bind([ids.security_role.as_uuid(), ids.auditor_role.as_uuid()])
        .bind(ids.suspended_auditor.as_uuid())
        .fetch_one(&mut *write_transaction)
        .await?;
        write_transaction.commit().await?;
        let projection_capability = ProjectionPool::for_holding(read.clone(), holding_id)?;
        let mut read_transaction = projection_capability.begin().await?;
        let projections_current: bool = sqlx::query_scalar(
            "SELECT
                EXISTS(
                    SELECT 1 FROM erpx_cqrs_read.projection_generations
                    WHERE holding_id=$1 AND projection_name=$2 AND status='active'
                )
                AND EXISTS(
                    SELECT 1 FROM erpx_cqrs_read.projection_generations
                    WHERE holding_id=$1 AND projection_name=$3 AND status='active'
                )
                AND (
                    SELECT count(*) FROM erpx_identity_access_read.principal_projection
                    WHERE holding_id=$1 AND principal_id = ANY($4)
                ) = 3
                AND (
                    SELECT count(*) FROM erpx_identity_access_read.role_projection
                    WHERE holding_id=$1 AND role_id = ANY($5)
                ) = 2
                AND EXISTS(
                    SELECT 1 FROM erpx_identity_access_read.principal_projection
                    WHERE holding_id=$1 AND principal_id=$6
                      AND principal_status='suspended'
                )",
        )
        .bind(holding_id)
        .bind(PRINCIPAL_PROJECTION_NAME)
        .bind(ROLE_PROJECTION_NAME)
        .bind([
            ids.administrator.as_uuid(),
            ids.pending_reviewer.as_uuid(),
            ids.suspended_auditor.as_uuid(),
        ])
        .bind([ids.security_role.as_uuid(), ids.auditor_role.as_uuid()])
        .bind(ids.suspended_auditor.as_uuid())
        .fetch_one(&mut *read_transaction)
        .await?;
        read_transaction.commit().await?;
        write.close().await;
        read.close().await;
        if !commands_current || !projections_current {
            return Ok(false);
        }
    }
    Ok(true)
}

async fn seed_holding(
    write: PgPool,
    read: PgPool,
    data_profile_key: &str,
    label: &str,
) -> TaskResult {
    let holding_id = holding_holding_scope_id(data_profile_key);
    let write_capability = WritePool::for_holding(write.clone(), holding_id)?;
    let access = FixtureAccess { holding_id };
    let handler = IdentityAccessCommandHandler::new(
        IdentityAccessWriteRepository::new(write_capability.clone()),
        IdentityAccessCommandAdapter::new(write_capability.clone()),
        access.clone(),
        access,
        SystemClock,
    );
    let ids = FixtureIds::for_profile(data_profile_key)?;
    let scope = AuthorityScope::Holding { holding_id };

    handler
        .handle(envelope(
            data_profile_key,
            "define-security-role",
            None,
            DefineRole {
                role_id: ids.security_role,
                scope: scope.clone(),
                code: RoleCode::try_new("holding_security_administrator")?,
                display_name: format!("{label} Security Administrator"),
                risk_tier: RiskTier::Standard,
                permissions: permissions(&[
                    "identity_access.read",
                    "identity_access.write",
                    "security.read",
                ])?,
            },
        )?)
        .await
        .map_err(|error| fixture_command_error(data_profile_key, "define-security-role", error))?;
    handler
        .handle(envelope(
            data_profile_key,
            "define-auditor-role",
            None,
            DefineRole {
                role_id: ids.auditor_role,
                scope: scope.clone(),
                code: RoleCode::try_new("holding_access_auditor")?,
                display_name: format!("{label} Access Auditor"),
                risk_tier: RiskTier::Standard,
                permissions: permissions(&["identity_access.read", "security.read"])?,
            },
        )?)
        .await
        .map_err(|error| fixture_command_error(data_profile_key, "define-auditor-role", error))?;

    handler
        .handle(envelope(
            data_profile_key,
            "create-administrator",
            None,
            CreatePrincipal {
                principal_id: ids.administrator,
                scope: scope.clone(),
                display_name: format!("{label} Security Administrator"),
            },
        )?)
        .await
        .map_err(|error| fixture_command_error(data_profile_key, "create-administrator", error))?;
    handler
        .handle(envelope(
            data_profile_key,
            "activate-administrator",
            Some(1),
            ActivatePrincipal {
                principal_id: ids.administrator,
            },
        )?)
        .await
        .map_err(|error| {
            fixture_command_error(data_profile_key, "activate-administrator", error)
        })?;
    handler
        .handle(envelope(
            data_profile_key,
            "assign-administrator-role",
            Some(2),
            AssignRole {
                principal_id: ids.administrator,
                assignment_id: ids.administrator_grant,
                role_id: ids.security_role,
                scope: scope.clone(),
                risk_tier: RiskTier::Standard,
                valid_from: fixture_time("2026-01-01T00:00:00Z")?,
                valid_until: fixture_time("2026-12-31T00:00:00Z")?,
                grant_reason: "Versioned development authority fixture".into(),
                ticket_reference: Some("DEV-IAM-001".into()),
                grantor_ref: ROOT_PRINCIPAL.into(),
                approver_ref: None,
            },
        )?)
        .await
        .map_err(|error| {
            fixture_command_error(data_profile_key, "assign-administrator-role", error)
        })?;

    handler
        .handle(envelope(
            data_profile_key,
            "create-pending-reviewer",
            None,
            CreatePrincipal {
                principal_id: ids.pending_reviewer,
                scope: scope.clone(),
                display_name: format!("{label} Pending Reviewer"),
            },
        )?)
        .await
        .map_err(|error| {
            fixture_command_error(data_profile_key, "create-pending-reviewer", error)
        })?;

    handler
        .handle(envelope(
            data_profile_key,
            "create-suspended-auditor",
            None,
            CreatePrincipal {
                principal_id: ids.suspended_auditor,
                scope,
                display_name: format!("{label} Suspended Auditor"),
            },
        )?)
        .await
        .map_err(|error| {
            fixture_command_error(data_profile_key, "create-suspended-auditor", error)
        })?;
    handler
        .handle(envelope(
            data_profile_key,
            "activate-suspended-auditor",
            Some(1),
            ActivatePrincipal {
                principal_id: ids.suspended_auditor,
            },
        )?)
        .await
        .map_err(|error| {
            fixture_command_error(data_profile_key, "activate-suspended-auditor", error)
        })?;
    handler
        .handle(envelope(
            data_profile_key,
            "suspend-auditor",
            Some(2),
            SuspendPrincipal {
                principal_id: ids.suspended_auditor,
            },
        )?)
        .await
        .map_err(|error| fixture_command_error(data_profile_key, "suspend-auditor", error))?;

    // A browser scenario can be interrupted after reactivation but before its
    // compensating suspension. Reconcile that retained development record via
    // a new, versioned command so a later run starts from an authoritative
    // suspended state without rewriting aggregate or projection tables.
    let (auditor_version, auditor_status): (i64, String) = {
        let mut transaction = write_capability.begin().await?;
        let row = sqlx::query_as(
            "SELECT aggregate_version, principal_status
             FROM erpx_identity_access_write.principal_authority
             WHERE holding_id=$1 AND principal_id=$2",
        )
        .bind(holding_id)
        .bind(ids.suspended_auditor.as_uuid())
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        row
    };
    if auditor_status == "active" {
        let expected_version = u64::try_from(auditor_version)?;
        let command_kind = format!("reconcile-suspended-auditor-v{expected_version}");
        handler
            .handle(envelope(
                data_profile_key,
                &command_kind,
                Some(expected_version),
                SuspendPrincipal {
                    principal_id: ids.suspended_auditor,
                },
            )?)
            .await
            .map_err(|error| fixture_command_error(data_profile_key, &command_kind, error))?;
    } else if auditor_status != "suspended" {
        return Err(format!(
            "Identity & Access fixture {data_profile_key} suspended auditor has unsupported retained status {auditor_status}"
        )
        .into());
    }

    project_and_wait(write_capability, read.clone(), holding_id).await?;
    write.close().await;
    read.close().await;
    Ok(())
}

async fn project_and_wait(write: WritePool, read: PgPool, holding_id: Uuid) -> TaskResult {
    let runner = ProjectionRunner::new(
        ProjectionSource::new(write),
        ProjectionStore::new(ProjectionPool::for_holding(read, holding_id)?),
        ProjectionRunnerConfig {
            page_size: PAGE_SIZE,
            ..ProjectionRunnerConfig::default()
        },
    )?;
    run_until_current(&runner, &IdentityAccessPrincipalProjector::new()).await?;
    run_until_current(&runner, &IdentityAccessRoleProjector::new()).await?;
    Ok(())
}

async fn run_until_current<P>(runner: &ProjectionRunner, projector: &P) -> TaskResult
where
    P: erpx_cqrs_sqlx::Projector,
{
    loop {
        let run = runner.run_active(projector).await?;
        if run.retry_scheduled || run.dead_lettered != 0 {
            return Err(format!(
                "{} projection did not reach its checkpoint",
                projector.name()
            )
            .into());
        }
        if run.fetched < PAGE_SIZE {
            return Ok(());
        }
    }
}

fn envelope<C>(
    data_profile_key: &str,
    command_kind: &str,
    expected_aggregate_version: Option<u64>,
    command: C,
) -> TaskResult<CommandEnvelope<C>> {
    let stable = |kind: &str| {
        Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("erpx:development:{FIXTURE_CONTRACT}:{data_profile_key}:{command_kind}:{kind}")
                .as_bytes(),
        )
    };
    Ok(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(stable("command"))?,
        idempotency_key: IdempotencyKey::try_new(format!(
            "development:{FIXTURE_CONTRACT}:{data_profile_key}:{command_kind}"
        ))?,
        correlation_id: CorrelationId::try_from_uuid(stable("correlation"))?,
        causation_id: None,
        actor: ActorAttribution {
            principal_ref: ROOT_PRINCIPAL.into(),
            kind: ActorKind::PlatformUser,
            session_id: Some(SessionId::try_from_uuid(stable("session"))?),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        active_scope: AuthorityScope::Holding {
            holding_id: holding_holding_scope_id(data_profile_key),
        },
        audit: AuditContext {
            purpose: "Versioned Identity & Access development fixture".into(),
            customer_visible: false,
            module: Some("xtask:development-fixture".into()),
        },
        expected_aggregate_version,
        command,
    })
}

fn permissions(values: &[&str]) -> TaskResult<Vec<PermissionKey>> {
    values
        .iter()
        .map(|value| PermissionKey::try_new(*value).map_err(Into::into))
        .collect()
}

fn fixture_command_error(
    data_profile_key: &str,
    command_kind: &str,
    error: erpx_cqrs::CqrsError,
) -> Box<dyn std::error::Error + Send + Sync> {
    format!("Identity & Access fixture command {data_profile_key}/{command_kind} failed: {error:?}")
        .into()
}

fn fixture_time(value: &str) -> TaskResult<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

const fn fixture_routes() -> [(&'static str, &'static str, &'static str, &'static str); 2] {
    [
        (DATABASES[2], DATABASES[3], DATA_PROFILE_KEY, "Atlas"),
        (
            DATABASES[4],
            DATABASES[5],
            RESTRICTED_DATA_PROFILE_KEY,
            "Beacon",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_ids_are_stable_and_holding_specific() {
        let atlas = FixtureIds::for_profile(DATA_PROFILE_KEY).unwrap();
        let atlas_replay = FixtureIds::for_profile(DATA_PROFILE_KEY).unwrap();
        let beacon = FixtureIds::for_profile(RESTRICTED_DATA_PROFILE_KEY).unwrap();
        assert_eq!(atlas.administrator, atlas_replay.administrator);
        assert_ne!(atlas.administrator, beacon.administrator);
    }

    #[test]
    fn fixture_source_uses_typed_commands_and_projectors() {
        let source = include_str!("identity_access_fixture.rs");
        assert!(source.contains("IdentityAccessCommandHandler::new"));
        assert!(source.contains("ProjectionRunner::new"));
        assert!(!source.contains(concat!("INSERT", " INTO erpx_identity_access")));
    }
}
