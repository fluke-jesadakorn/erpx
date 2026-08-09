use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use chrono::Utc;
use erpx_domain::{
    access::{RiskTier, RoleAssignment, Scope},
    data_plane::BusinessUnitId,
};
use erpx_identity::{
    DEVELOPMENT_CLUSTER_MARKER, DEVELOPMENT_CONTROLLER_PRINCIPAL, DEVELOPMENT_CONTROLLER_ROLE,
    DEVELOPMENT_CONTROLLER_TARGET_TENANT, DirectoryRoleTargetScope, SqlxIdentityStore,
    demo_personas, development_controller_permissions, permission_catalog, platform_personas,
};
use erpx_infrastructure::{
    ApprovedDatabaseProfileConnection, BranchProvisioningState, DatabaseProfileRegistration,
    PersistenceError, ProvisionBranchRequest, SqlxBranchProvisioningStore, SqlxControlPlane,
    TenantDatabaseProvisioner, branch_database_keys, tenant_holding_scope_id,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    DATABASES, DevConfig, HOLDING_REF, RESTRICTED_HOLDING_REF, RESTRICTED_TENANT_ID, TENANT_ID,
    TaskResult, runtime_dir,
};

const CONTROLLER_PASSWORD_FILE: &str = "secrets/dev-controller.password";
const UCS_SHARED_PASSWORD_FILE: &str = "secrets/ucs-shared.password";
const FIXTURE_VERSION: &str = "erpx-dev-access-v6";
const PLATFORM_ASSIGNMENT_REVISION: &str = "platform-permissions-v6";
const CONTROLLER_ASSIGNMENT_REVISION: &str = "restricted-tenant-full-admin-v1";
const CONTROLLER_CATALOG_ASSIGNMENT_REVISION: &str = "controller-role-catalog-v2";
const BRANCH_PROFILE_KEY: &str = "dev-local";
const BRANCH_CREDENTIAL_PROFILE_KEY: &str = "default";
const BRANCH_PROVISIONING_WORKER_ID: Uuid = Uuid::from_u128(0x8e9e8d7c6b5a49388776655443322110);
const RESTRICTED_UNITS: [&str; 12] = erpx_identity::BUSINESS_UNITS;

#[derive(Serialize)]
struct UcsAuthoritiesFile {
    version: u32,
    authorities: Vec<UcsAuthorityEntry>,
}

#[derive(Serialize)]
struct UcsAuthorityEntry {
    authority: &'static str,
    authority_fingerprint: String,
    state_path: PathBuf,
    password_path: PathBuf,
}

#[derive(Debug, Clone, Copy)]
struct DevelopmentBranchFixture {
    company_key: &'static str,
    company_name: &'static str,
    branch_key: &'static str,
    branch_code: &'static str,
    branch_name: &'static str,
}

const DEMO_BRANCHES: [DevelopmentBranchFixture; 4] = [
    DevelopmentBranchFixture {
        company_key: "atlas-manufacturing",
        company_name: "Atlas Manufacturing",
        branch_key: "bangkok-factory",
        branch_code: "BKK-F",
        branch_name: "Bangkok Factory",
    },
    DevelopmentBranchFixture {
        company_key: "atlas-manufacturing",
        company_name: "Atlas Manufacturing",
        branch_key: "rayong-factory",
        branch_code: "RYG-F",
        branch_name: "Rayong Factory",
    },
    DevelopmentBranchFixture {
        company_key: "atlas-retail",
        company_name: "Atlas Retail",
        branch_key: "bangkok-flagship",
        branch_code: "BKK-S",
        branch_name: "Bangkok Flagship",
    },
    DevelopmentBranchFixture {
        company_key: "atlas-retail",
        company_name: "Atlas Retail",
        branch_key: "chiang-mai-flagship",
        branch_code: "CNX-S",
        branch_name: "Chiang Mai Flagship",
    },
];

const RESTRICTED_BRANCHES: [DevelopmentBranchFixture; 4] = [
    DevelopmentBranchFixture {
        company_key: "beacon-consulting",
        company_name: "Beacon Consulting",
        branch_key: "bangkok-consulting",
        branch_code: "BKK-C",
        branch_name: "Bangkok Consulting",
    },
    DevelopmentBranchFixture {
        company_key: "beacon-consulting",
        company_name: "Beacon Consulting",
        branch_key: "chiang-mai-consulting",
        branch_code: "CNX-C",
        branch_name: "Chiang Mai Consulting",
    },
    DevelopmentBranchFixture {
        company_key: "beacon-distribution",
        company_name: "Beacon Distribution",
        branch_key: "bangkok-distribution",
        branch_code: "BKK-D",
        branch_name: "Bangkok Distribution",
    },
    DevelopmentBranchFixture {
        company_key: "beacon-distribution",
        company_name: "Beacon Distribution",
        branch_key: "rayong-distribution",
        branch_code: "RYG-D",
        branch_name: "Rayong Distribution",
    },
];

fn branch_fixtures(tenant_id: &str) -> &'static [DevelopmentBranchFixture] {
    if tenant_id == RESTRICTED_TENANT_ID {
        &RESTRICTED_BRANCHES
    } else {
        &DEMO_BRANCHES
    }
}

fn branch_ids(tenant_id: &str, fixture: DevelopmentBranchFixture) -> (Uuid, Uuid) {
    let company_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!(
            "erpx:dev-access:company:v1:{tenant_id}:{}",
            fixture.company_key
        )
        .as_bytes(),
    );
    let branch_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!(
            "erpx:dev-access:branch:v1:{tenant_id}:{}",
            fixture.branch_key
        )
        .as_bytes(),
    );
    (company_id, branch_id)
}

fn platform_assignment_id(role_key: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("erpx:dev-access:{PLATFORM_ASSIGNMENT_REVISION}:platform:{role_key}").as_bytes(),
    )
}

fn controller_assignment_id() -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("erpx:dev-access:{CONTROLLER_ASSIGNMENT_REVISION}:controller").as_bytes(),
    )
}

fn controller_catalog_assignment_id(target_tenant: &str, role_key: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!(
            "erpx:dev-access:{CONTROLLER_CATALOG_ASSIGNMENT_REVISION}:{target_tenant}:{role_key}"
        )
        .as_bytes(),
    )
}

fn controller_catalog_assignments(
    target_tenant: &str,
    holding_ref: &str,
    representative_branch: (Uuid, Uuid),
    now: chrono::DateTime<Utc>,
) -> Vec<RoleAssignment> {
    let holding_id = tenant_holding_scope_id(holding_ref);
    permission_catalog()
        .into_iter()
        .map(|entry| {
            let platform_role = entry.target_scope == DirectoryRoleTargetScope::Platform;
            let scope = match entry.target_scope {
                DirectoryRoleTargetScope::Platform => Scope::Platform,
                DirectoryRoleTargetScope::Holding => Scope::Holding { holding_id },
                DirectoryRoleTargetScope::Company => Scope::Company {
                    holding_id,
                    company_id: representative_branch.0,
                },
                DirectoryRoleTargetScope::Branch => Scope::Branch {
                    holding_id,
                    company_id: representative_branch.0,
                    branch_id: representative_branch.1,
                },
            };
            let functional_domain = if platform_role {
                "platform".to_owned()
            } else {
                entry
                    .role_key
                    .split_once('_')
                    .map_or("tenant-administration", |(domain, _)| domain)
                    .to_owned()
            };
            RoleAssignment {
                id: controller_catalog_assignment_id(target_tenant, &entry.role_key),
                principal_ref: DEVELOPMENT_CONTROLLER_PRINCIPAL.into(),
                role_key: entry.role_key,
                permissions: entry.permissions,
                functional_domain,
                authority_level: if platform_role { 64 } else { 32 },
                approval_limit_minor: 0,
                scope,
                valid_from: now,
                valid_until: now + chrono::Duration::days(365),
                timezone: "Asia/Bangkok".into(),
                risk_tier: RiskTier::Standard,
                grant_reason: "environment-gated local controller role catalog".into(),
                ticket_reference: Some("local:dev-controller-role-catalog".into()),
                grantor: DEVELOPMENT_CONTROLLER_PRINCIPAL.into(),
                approver: None,
                renewal_reviewer: None,
                parent_assignment_id: None,
                assignment_version: 1,
            }
        })
        .collect()
}

async fn controller_catalog_scope_context(
    write: &PgPool,
    target_tenant: &str,
) -> TaskResult<(String, (Uuid, Uuid))> {
    let holding_ref: String = sqlx::query_scalar(
        "SELECT holding_ref FROM erpx_control.tenant_registry
         WHERE tenant_id=$1 AND state='active'",
    )
    .bind(target_tenant)
    .fetch_one(write)
    .await?;
    let deterministic_branch = branch_ids(target_tenant, RESTRICTED_BRANCHES[0]);
    let representative = sqlx::query(
        "SELECT company_id, branch_id
         FROM erpx_control.branch_database_routes
         WHERE tenant_id=$1 AND state='active'
         ORDER BY (branch_id=$2) DESC, branch_id
         LIMIT 1",
    )
    .bind(target_tenant)
    .bind(deterministic_branch.1)
    .fetch_optional(write)
    .await?
    .map(|row| {
        Ok::<_, sqlx::Error>((
            row.try_get::<Uuid, _>("company_id")?,
            row.try_get::<Uuid, _>("branch_id")?,
        ))
    })
    .transpose()?;
    Ok((holding_ref, representative.unwrap_or(deterministic_branch)))
}

async fn reconcile_controller_role(
    write: &PgPool,
    target_tenant: &str,
    now: chrono::DateTime<Utc>,
) -> TaskResult {
    let assignment = RoleAssignment {
        id: controller_assignment_id(),
        principal_ref: DEVELOPMENT_CONTROLLER_PRINCIPAL.into(),
        role_key: DEVELOPMENT_CONTROLLER_ROLE.into(),
        permissions: development_controller_permissions(),
        functional_domain: "platform_tenant_administration".into(),
        authority_level: 64,
        approval_limit_minor: 0,
        scope: Scope::Platform,
        valid_from: now,
        valid_until: now + chrono::Duration::days(365),
        timezone: "Asia/Bangkok".into(),
        risk_tier: RiskTier::Standard,
        grant_reason: "environment-gated local restricted tenant administration".into(),
        ticket_reference: Some("local:dev-controller-role".into()),
        grantor: DEVELOPMENT_CONTROLLER_PRINCIPAL.into(),
        approver: None,
        renewal_reviewer: None,
        parent_assignment_id: None,
        assignment_version: 1,
    };
    let (holding_ref, representative_branch) =
        controller_catalog_scope_context(write, target_tenant).await?;
    let catalog_assignments =
        controller_catalog_assignments(target_tenant, &holding_ref, representative_branch, now);
    for candidate in std::iter::once(&assignment).chain(catalog_assignments.iter()) {
        candidate.validate_for_grant(None)?;
    }
    let managed_assignment_ids = catalog_assignments
        .iter()
        .map(|candidate| candidate.id)
        .collect::<Vec<_>>();
    let mut transaction = write.begin().await?;
    for candidate in std::iter::once(&assignment).chain(catalog_assignments.iter()) {
        let payload = serde_json::to_value(candidate)?;
        sqlx::query(
            "INSERT INTO erpx_control.role_assignments
                (id, principal_ref, role_key, parent_assignment_id, valid_from,
                 valid_until, assignment_version, payload)
             VALUES ($1,$2,$3,NULL,$4,$5,1,$6)
             ON CONFLICT (id) DO UPDATE SET
                principal_ref=EXCLUDED.principal_ref,
                role_key=EXCLUDED.role_key,
                parent_assignment_id=NULL,
                valid_from=EXCLUDED.valid_from,
                valid_until=EXCLUDED.valid_until,
                assignment_version=EXCLUDED.assignment_version,
                payload=EXCLUDED.payload",
        )
        .bind(candidate.id)
        .bind(&candidate.principal_ref)
        .bind(&candidate.role_key)
        .bind(candidate.valid_from)
        .bind(candidate.valid_until)
        .bind(payload)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_control.role_assignment_target_policies
                (assignment_id, policy_kind, environment_gate, configured_by, configured_at)
             VALUES ($1,'tenant_allowlist',$2,$3,$4)
             ON CONFLICT (assignment_id) DO UPDATE SET
                policy_kind=EXCLUDED.policy_kind,
                environment_gate=EXCLUDED.environment_gate,
                configured_by=EXCLUDED.configured_by,
                configured_at=EXCLUDED.configured_at",
        )
        .bind(candidate.id)
        .bind(DEVELOPMENT_CLUSTER_MARKER)
        .bind(DEVELOPMENT_CONTROLLER_PRINCIPAL)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "DELETE FROM erpx_control.role_assignment_tenant_targets
             WHERE assignment_id=$1",
        )
        .bind(candidate.id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_control.role_assignment_tenant_targets
                (assignment_id, tenant_id, configured_at)
             VALUES ($1,$2,$3)",
        )
        .bind(candidate.id)
        .bind(target_tenant)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
    }

    // Preserve retired fixture assignments as immutable evidence while
    // ensuring removed catalog entries can no longer be selected.
    sqlx::query(
        "UPDATE erpx_control.role_assignments
         SET valid_until=LEAST(valid_until,$1),
             payload=jsonb_set(payload,'{valid_until}',to_jsonb($1::timestamptz),true)
         WHERE principal_ref=$2
           AND payload->>'ticket_reference'='local:dev-controller-role-catalog'
           AND NOT (id = ANY($3))
           AND valid_until>$1",
    )
    .bind(now)
    .bind(DEVELOPMENT_CONTROLLER_PRINCIPAL)
    .bind(&managed_assignment_ids)
    .execute(&mut *transaction)
    .await?;

    // Retire the former implicit superuser while preserving its assignment
    // and append-only audit evidence as expired records.
    sqlx::query(
        "UPDATE erpx_control.role_assignments
         SET valid_until=LEAST(valid_until,$1),
             payload=jsonb_set(payload,'{valid_until}',to_jsonb($1::timestamptz),true)
         WHERE role_key='development_superuser' AND valid_until>$1",
    )
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO erpx_control.audit_events
            (id,actor_ref,customer_account_id,action,purpose,correlation_id,payload)
         VALUES ($1,$2,NULL,'development.authorization.role_reconciled',
                 'local restricted tenant controller reconcile',$3,$4)",
    )
    .bind(Uuid::new_v4())
    .bind(DEVELOPMENT_CONTROLLER_PRINCIPAL)
    .bind(Uuid::new_v4())
    .bind(serde_json::json!({
        "role_key": DEVELOPMENT_CONTROLLER_ROLE,
        "target_tenant": target_tenant,
        "catalog_role_count": catalog_assignments.len(),
        "total_controller_role_count": catalog_assignments.len() + 1,
        "legacy_superuser_retired": true,
    }))
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    // Persona contexts live in the identity schema. Record their retirement
    // first, then remove only the legacy manifest/context rows; append-only
    // development_audit remains intact.
    let mut transaction = write.begin().await?;
    sqlx::query(
        "INSERT INTO erpx_identity.development_audit
            (id,controller_principal_ref,persona_principal_ref,persona_key,
             role_key,tenant_id,organization_scope,feature,correlation_id,
             outcome,recorded_at,details)
         SELECT $3,$1,manifest.principal_ref,manifest.persona_key,
                manifest.role_key,manifest.tenant_id,manifest.organization_scope,
                manifest.feature,$4,'ended',$2,
                '{\"reason\":\"legacy_superuser_retired\"}'::jsonb
         FROM erpx_identity.development_persona_manifests manifest
         WHERE manifest.role_key='development_superuser'",
    )
    .bind(DEVELOPMENT_CONTROLLER_PRINCIPAL)
    .bind(now)
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "DELETE FROM erpx_identity.development_persona_contexts
         WHERE persona_key IN (
             SELECT persona_key FROM erpx_identity.development_persona_manifests
             WHERE role_key='development_superuser'
         )",
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "DELETE FROM erpx_identity.development_persona_manifests
         WHERE role_key='development_superuser'",
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "UPDATE erpx_identity.principals SET status='suspended', updated_at=$1
         WHERE principal_ref='dev:platform:development_superuser'",
    )
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

fn persona_assignment_scopes(
    tenant_id: &str,
    holding_ref: &str,
    role_key: &str,
) -> Vec<(Scope, String)> {
    let holding_id = tenant_holding_scope_id(holding_ref);
    if role_key == "company_admin" {
        let mut seen = Vec::new();
        return branch_fixtures(tenant_id)
            .iter()
            .filter_map(|fixture| {
                let (company_id, _) = branch_ids(tenant_id, *fixture);
                if seen.contains(&company_id) {
                    None
                } else {
                    seen.push(company_id);
                    Some((
                        Scope::Company {
                            holding_id,
                            company_id,
                        },
                        format!("company:{company_id}"),
                    ))
                }
            })
            .collect();
    }
    let branch_scoped = role_key == "branch_admin"
        || erpx_identity::BUSINESS_UNITS
            .iter()
            .any(|unit| role_key.starts_with(&format!("{unit}_")));
    if branch_scoped {
        return branch_fixtures(tenant_id)
            .iter()
            .map(|fixture| {
                let (company_id, branch_id) = branch_ids(tenant_id, *fixture);
                (
                    Scope::Branch {
                        holding_id,
                        company_id,
                        branch_id,
                    },
                    format!("branch:{company_id}:{branch_id}"),
                )
            })
            .collect();
    }
    vec![(
        Scope::Holding { holding_id },
        format!("holding:{holding_id}"),
    )]
}

fn scoped_demo_personas(
    tenant_id: &str,
    holding_ref: &str,
    entitled_units: &[&str],
) -> Vec<erpx_identity::DevelopmentPersona> {
    let mut personas = demo_personas(tenant_id, entitled_units);
    for persona in &mut personas {
        if let Some((scope, _)) =
            persona_assignment_scopes(tenant_id, holding_ref, &persona.role_key).first()
        {
            // The selector presents one bounded representative scope per test
            // persona. Durable assignments still cover every fixture branch.
            persona.organization_scope = scope.canonical_key();
        }
    }
    personas
}

pub(crate) fn controller_password_path(root: &Path) -> PathBuf {
    runtime_dir(root).join(CONTROLLER_PASSWORD_FILE)
}

pub(crate) fn init(root: &Path, _config: &DevConfig) -> TaskResult {
    let secrets = runtime_dir(root).join("secrets");
    fs::create_dir_all(&secrets)?;
    fs::set_permissions(&secrets, fs::Permissions::from_mode(0o700))?;
    let path = controller_password_path(root);
    if !path.exists() {
        let password = format!(
            "ERPX-DEV-{}-{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        );
        fs::write(&path, password)?;
    }
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    init_ucs_tls(root)?;
    let shared_password = runtime_dir(root).join(UCS_SHARED_PASSWORD_FILE);
    if !shared_password.exists() {
        let legacy_platform = runtime_dir(root).join("secrets/ucs-platform.password");
        let value = if legacy_platform.is_file() {
            fs::read(&legacy_platform)?
        } else {
            format!(
                "ERPX-UCS-{}-{}",
                Uuid::new_v4().simple(),
                Uuid::new_v4().simple()
            )
            .into_bytes()
        };
        fs::write(&shared_password, value)?;
    }
    fs::set_permissions(&shared_password, fs::Permissions::from_mode(0o600))?;
    write_ucs_authorities_file(root)?;
    for authority in ["platform", TENANT_ID, RESTRICTED_TENANT_ID] {
        let legacy = runtime_dir(root).join(format!("secrets/ucs-{authority}.password"));
        if legacy.is_file() {
            fs::remove_file(legacy)?;
        }
    }
    Ok(())
}

pub(crate) fn ucs_authorities_path(root: &Path) -> PathBuf {
    runtime_dir(root).join("ucs-lab/authorities.json")
}

fn write_ucs_authorities_file(root: &Path) -> TaskResult {
    let runtime = runtime_dir(root);
    let directory = runtime.join("ucs-lab");
    fs::create_dir_all(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    let authorities = ["platform", TENANT_ID, RESTRICTED_TENANT_ID]
        .into_iter()
        .map(|authority| UcsAuthorityEntry {
            authority,
            authority_fingerprint: format!(
                "{:x}",
                Sha256::digest(format!("erpx-ucs-authority:{authority}").as_bytes())
            ),
            state_path: runtime.join(format!("ucs-lab/{authority}.state.json")),
            password_path: runtime.join(UCS_SHARED_PASSWORD_FILE),
        })
        .collect();
    let destination = ucs_authorities_path(root);
    if destination.is_symlink() {
        return Err("refusing symlinked UCS authorities file".into());
    }
    let temporary = directory.join(format!(".authorities.{}.tmp", Uuid::new_v4().simple()));
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(&UcsAuthoritiesFile {
            version: 1,
            authorities,
        })?,
    )?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    fs::rename(temporary, destination)?;
    Ok(())
}

pub(crate) fn start_ucs_lab(root: &Path) -> TaskResult {
    let runtime = runtime_dir(root);
    let tls = runtime.join("ucs-lab/tls");
    let binary = root.join("target/debug/erpx-ucs-lab");
    if !binary.exists() {
        return Err("build erpx-ucs-lab before starting the development stack".into());
    }
    write_ucs_authorities_file(root)?;
    let environment = vec![
        ("ERPX_UCS_LAB_BIND", "127.0.0.1:15443".into()),
        (
            "ERPX_UCS_LAB_CERT",
            tls.join("ucs.crt").to_string_lossy().into_owned(),
        ),
        (
            "ERPX_UCS_LAB_KEY",
            tls.join("ucs.key").to_string_lossy().into_owned(),
        ),
        (
            "ERPX_UCS_LAB_AUTHORITIES_FILE",
            ucs_authorities_path(root).to_string_lossy().into_owned(),
        ),
    ];
    super::spawn_service(
        root,
        &runtime,
        "ucs-lab",
        binary,
        std::iter::empty::<&str>(),
        environment,
    )?;
    Ok(())
}

fn init_ucs_tls(root: &Path) -> TaskResult {
    let tls = runtime_dir(root).join("ucs-lab/tls");
    fs::create_dir_all(&tls)?;
    fs::set_permissions(&tls, fs::Permissions::from_mode(0o700))?;
    let cert = tls.join("ucs.crt");
    let key = tls.join("ucs.key");
    if cert.exists() && key.exists() {
        return Ok(());
    }
    let status = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            key.to_str().ok_or("invalid UCS lab key path")?,
            "-out",
            cert.to_str().ok_or("invalid UCS lab certificate path")?,
            "-days",
            "825",
            "-subj",
            "/CN=localhost",
            "-addext",
            "subjectAltName=DNS:localhost,IP:127.0.0.1",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        return Err("failed to generate local UCS lab TLS material".into());
    }
    fs::set_permissions(key, fs::Permissions::from_mode(0o600))?;
    fs::set_permissions(cert, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

async fn reconcile_controller_principal(
    root: &Path,
    write: &PgPool,
    now: chrono::DateTime<Utc>,
) -> TaskResult {
    let identity = SqlxIdentityStore::new(write.clone());
    let controller_password = fs::read_to_string(controller_password_path(root))?;
    let controller_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM erpx_identity.principals WHERE principal_ref=$1)",
    )
    .bind(DEVELOPMENT_CONTROLLER_PRINCIPAL)
    .fetch_one(write)
    .await?;
    if !controller_exists {
        identity
            .create_principal_with_password(
                DEVELOPMENT_CONTROLLER_PRINCIPAL,
                controller_password.trim(),
                now,
            )
            .await?;
    }
    if !sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM erpx_identity.password_credentials c
                       JOIN erpx_identity.principals p ON p.id=c.principal_id
                       WHERE p.principal_ref=$1)",
    )
    .bind(DEVELOPMENT_CONTROLLER_PRINCIPAL)
    .fetch_one(write)
    .await?
    {
        identity
            .rotate_development_controller_password(controller_password.trim(), now)
            .await?;
    }
    Ok(())
}

async fn reconcile_cluster_identity(write: &PgPool, now: chrono::DateTime<Utc>) -> TaskResult {
    let authority_fingerprint = format!("{:x}", Sha256::digest(b"erpx-local-ucs-platform"));
    sqlx::query(
        "INSERT INTO erpx_identity.development_cluster_identity
            (cluster_id, marker, authority_fingerprint, profile, created_at, reconciled_at)
         VALUES ($1,'local-ucs-lab',$2,'local',$3,$3)
         ON CONFLICT (marker) DO UPDATE SET
            authority_fingerprint=EXCLUDED.authority_fingerprint,
            reconciled_at=EXCLUDED.reconciled_at",
    )
    .bind(Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        b"erpx:local-ucs-platform",
    ))
    .bind(authority_fingerprint)
    .bind(now)
    .execute(write)
    .await?;
    Ok(())
}

async fn reconcile_persona_catalog(
    root: &Path,
    config: &DevConfig,
    control_write_database: &str,
    control_read_database: &str,
    controller_target_tenant: &str,
) -> TaskResult<(
    PgPool,
    std::sync::Arc<SqlxControlPlane>,
    chrono::DateTime<Utc>,
)> {
    init(root, config)?;
    let write = super::pool(&config.database_url(control_write_database)).await?;
    let identity = SqlxIdentityStore::new(write.clone());
    let now = Utc::now();
    reconcile_controller_principal(root, &write, now).await?;

    let tenants = [
        (
            TENANT_ID,
            HOLDING_REF,
            erpx_identity::BUSINESS_UNITS.as_slice(),
        ),
        (
            RESTRICTED_TENANT_ID,
            RESTRICTED_HOLDING_REF,
            &RESTRICTED_UNITS,
        ),
    ];
    let control = std::sync::Arc::new(SqlxControlPlane::new(crate::ControlPlanePools {
        write: write.clone(),
        read: super::pool(&config.database_url(control_read_database)).await?,
    }));
    let lifecycle = erpx_application::RoleLifecycleService::new(
        control.clone(),
        std::sync::Arc::new(erpx_application::SystemClock),
        control.clone(),
    );
    reconcile_controller_role(&write, controller_target_tenant, now).await?;
    let platform_personas = platform_personas();
    let platform_persona_count = platform_personas.len();
    identity
        .reconcile_development_personas(&platform_personas, now)
        .await?;
    for persona in platform_personas {
        ensure_principal(&write, &persona.principal_ref, now).await?;
        let assignment_id = platform_assignment_id(&persona.role_key);
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM erpx_control.role_assignments WHERE id=$1)",
        )
        .bind(assignment_id)
        .fetch_one(&write)
        .await?;
        if !exists {
            lifecycle
                .grant(
                    RoleAssignment {
                        id: assignment_id,
                        principal_ref: persona.principal_ref,
                        role_key: persona.role_key,
                        permissions: persona.permissions,
                        functional_domain: "platform".into(),
                        authority_level: 64,
                        approval_limit_minor: 0,
                        scope: Scope::Platform,
                        valid_from: now,
                        valid_until: now + chrono::Duration::days(365),
                        timezone: "Asia/Bangkok".into(),
                        risk_tier: RiskTier::Standard,
                        grant_reason: "local development platform persona fixture".into(),
                        ticket_reference: Some("local:dev-access".into()),
                        grantor: "local:dev-controller".into(),
                        approver: Some("local:dev-controller".into()),
                        renewal_reviewer: None,
                        parent_assignment_id: None,
                        assignment_version: 1,
                    },
                    None,
                )
                .await?;
        }
    }
    sqlx::query(
        "INSERT INTO erpx_identity.development_seed_receipts
            (receipt_key, fixture_version, tenant_id, manifest_digest, applied_at, counts)
         VALUES ('persona-manifest:platform',$1,NULL,$2,$3,$4)
         ON CONFLICT (receipt_key) DO UPDATE SET
            fixture_version=EXCLUDED.fixture_version,
            tenant_id=EXCLUDED.tenant_id,
            manifest_digest=EXCLUDED.manifest_digest,
            applied_at=EXCLUDED.applied_at,
            counts=EXCLUDED.counts",
    )
    .bind(FIXTURE_VERSION)
    .bind(platform_manifest_digest())
    .bind(now)
    .bind(serde_json::json!({"platform_personas": platform_persona_count}))
    .execute(&write)
    .await?;
    for (tenant_id, holding_ref, entitled_units) in tenants {
        let personas = scoped_demo_personas(tenant_id, holding_ref, entitled_units);
        identity
            .reconcile_development_personas(&personas, now)
            .await?;
        for persona in personas {
            ensure_principal(&write, &persona.principal_ref, now).await?;
            for (scope, scope_key) in
                persona_assignment_scopes(tenant_id, holding_ref, &persona.role_key)
            {
                let assignment_id = Uuid::new_v5(
                    &Uuid::NAMESPACE_URL,
                    format!(
                        "erpx:dev-access:{tenant_id}:{}:{scope_key}",
                        persona.role_key
                    )
                    .as_bytes(),
                );
                let exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM erpx_control.role_assignments WHERE id=$1)",
                )
                .bind(assignment_id)
                .fetch_one(&write)
                .await?;
                if !exists {
                    lifecycle
                        .grant(
                            RoleAssignment {
                                id: assignment_id,
                                principal_ref: persona.principal_ref.clone(),
                                role_key: persona.role_key.clone(),
                                permissions: persona.permissions.clone(),
                                functional_domain: persona.feature.clone(),
                                authority_level: 32,
                                approval_limit_minor: 0,
                                scope,
                                valid_from: now,
                                valid_until: now + chrono::Duration::days(365),
                                timezone: "Asia/Bangkok".into(),
                                risk_tier: RiskTier::Standard,
                                grant_reason: "local development persona fixture".into(),
                                ticket_reference: Some("local:dev-access".into()),
                                grantor: "local:dev-controller".into(),
                                approver: Some("local:dev-controller".into()),
                                renewal_reviewer: None,
                                parent_assignment_id: None,
                                assignment_version: 1,
                            },
                            None,
                        )
                        .await?;
                }
            }
        }
        let digest = manifest_digest(tenant_id);
        sqlx::query(
            "INSERT INTO erpx_identity.development_seed_receipts
                (receipt_key, fixture_version, tenant_id, manifest_digest, applied_at, counts)
             VALUES ($1,$2,$3,$4,$5,$6)
             ON CONFLICT (receipt_key) DO UPDATE SET
                fixture_version=EXCLUDED.fixture_version,
                manifest_digest=EXCLUDED.manifest_digest,
                applied_at=EXCLUDED.applied_at,
                counts=EXCLUDED.counts",
        )
        .bind(format!("persona-manifest:{tenant_id}"))
        .bind(FIXTURE_VERSION)
        .bind(tenant_id)
        .bind(digest)
        .bind(now)
        .bind(serde_json::json!({
            "core_personas": 65,
            "negative_personas": 4,
            "companies": 2,
            "branches": 4,
            "holding_name": if tenant_id == TENANT_ID { "Atlas Holdings" } else { "Beacon Services" },
            "entitled_business_units": entitled_units,
        }))
        .execute(&write)
        .await?;
    }
    reconcile_cluster_identity(&write, now).await?;
    Ok((write, control, now))
}

/// Reconcile the secret-free QA persona catalog into the active local runtime
/// database. Fresh-state mode intentionally calls this path instead of
/// importing retained identity or business data.
pub(crate) async fn reconcile_runtime_catalog(
    root: &Path,
    config: &DevConfig,
    control_write_database: &str,
    control_read_database: &str,
    controller_target_tenant: &str,
) -> TaskResult {
    let (write, control, _) = reconcile_persona_catalog(
        root,
        config,
        control_write_database,
        control_read_database,
        controller_target_tenant,
    )
    .await?;
    drop(control);
    write.close().await;
    Ok(())
}

pub(crate) async fn reconcile(root: &Path, config: &DevConfig) -> TaskResult {
    let (write, control, now) = reconcile_persona_catalog(
        root,
        config,
        DATABASES[0],
        DATABASES[1],
        DEVELOPMENT_CONTROLLER_TARGET_TENANT,
    )
    .await?;
    let identity = SqlxIdentityStore::new(write.clone());
    let lifecycle = erpx_application::RoleLifecycleService::new(
        control.clone(),
        std::sync::Arc::new(erpx_application::SystemClock),
        control.clone(),
    );
    let local_tenants = sqlx::query(
        "SELECT tenant_id, holding_ref
         FROM erpx_control.tenant_registry
         WHERE state='active' AND credential_key_ref='local-development-disabled'",
    )
    .fetch_all(&write)
    .await?;
    for row in local_tenants {
        let tenant_id: String = row.try_get("tenant_id")?;
        if [TENANT_ID, RESTRICTED_TENANT_ID].contains(&tenant_id.as_str()) {
            continue;
        }
        let holding_ref: String = row.try_get("holding_ref")?;
        let personas =
            scoped_demo_personas(&tenant_id, &holding_ref, &erpx_identity::BUSINESS_UNITS);
        identity
            .reconcile_development_personas(&personas, now)
            .await?;
        let holding_id = tenant_holding_scope_id(&holding_ref);
        for persona in personas {
            ensure_principal(&write, &persona.principal_ref, now).await?;
            let assignment_id = Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!(
                    "erpx:dev-access:{tenant_id}:{}:holding:{holding_id}",
                    persona.role_key
                )
                .as_bytes(),
            );
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM erpx_control.role_assignments WHERE id=$1)",
            )
            .bind(assignment_id)
            .fetch_one(&write)
            .await?;
            if !exists {
                lifecycle
                    .grant(
                        RoleAssignment {
                            id: assignment_id,
                            principal_ref: persona.principal_ref,
                            role_key: persona.role_key,
                            permissions: persona.permissions,
                            functional_domain: persona.feature,
                            authority_level: 32,
                            approval_limit_minor: 0,
                            scope: Scope::Holding { holding_id },
                            valid_from: now,
                            valid_until: now + chrono::Duration::days(365),
                            timezone: "Asia/Bangkok".into(),
                            risk_tier: RiskTier::Standard,
                            grant_reason: "local development auto-enrolled persona fixture".into(),
                            ticket_reference: Some("local:dev-access".into()),
                            grantor: "local:dev-controller".into(),
                            approver: Some("local:dev-controller".into()),
                            renewal_reviewer: None,
                            parent_assignment_id: None,
                            assignment_version: 1,
                        },
                        None,
                    )
                    .await?;
            }
        }
        sqlx::query(
            "INSERT INTO erpx_identity.development_seed_receipts
                (receipt_key, fixture_version, tenant_id, manifest_digest, applied_at, counts)
             VALUES ($1,$2,$3,$4,$5,$6)
             ON CONFLICT (receipt_key) DO UPDATE SET
                fixture_version=EXCLUDED.fixture_version,
                manifest_digest=EXCLUDED.manifest_digest,
                applied_at=EXCLUDED.applied_at,
                counts=EXCLUDED.counts",
        )
        .bind(format!("persona-manifest:{tenant_id}"))
        .bind(FIXTURE_VERSION)
        .bind(&tenant_id)
        .bind(manifest_digest(&tenant_id))
        .bind(now)
        .bind(
            serde_json::json!({"core_personas": 65, "negative_personas": 4, "auto_enrolled": true}),
        )
        .execute(&write)
        .await?;
    }
    sqlx::query(
        "UPDATE erpx_control.tenant_registry
         SET isolation_mode='holding_and_branch_pairs', updated_at=$2
         WHERE tenant_id IN ($1, $3)",
    )
    .bind(TENANT_ID)
    .bind(now)
    .bind(RESTRICTED_TENANT_ID)
    .execute(&write)
    .await?;
    reconcile_branch_fixtures(root, config, control.clone()).await?;
    reconcile_cluster_identity(&write, now).await?;
    write.close().await;
    Ok(())
}

async fn reconcile_branch_fixtures(
    root: &Path,
    config: &DevConfig,
    control: std::sync::Arc<SqlxControlPlane>,
) -> TaskResult {
    let jobs = SqlxBranchProvisioningStore::new(control.clone());
    sqlx::query(
        "UPDATE erpx_control.branch_provisioning_jobs
         SET state='failed', failure_category='development_reconcile_recovery',
             completed_at=NULL, claimed_by=NULL, claimed_at=NULL,
             lease_expires_at=NULL, updated_at=now()
         WHERE tenant_id IN ($1,$2)
           AND state IN ('provisioning','verifying')
           AND claimed_by IS NULL",
    )
    .bind(TENANT_ID)
    .bind(RESTRICTED_TENANT_ID)
    .execute(&control.pools().write)
    .await?;
    sqlx::query(
        "UPDATE erpx_control.branch_database_routes
         SET state='failed', failure_category='development_reconcile_recovery',
             activated_at=NULL, suspended_at=NULL, updated_at=now()
         WHERE tenant_id IN ($1,$2) AND state IN ('provisioning','verifying')",
    )
    .bind(TENANT_ID)
    .bind(RESTRICTED_TENANT_ID)
    .execute(&control.pools().write)
    .await?;
    jobs.register_profile(&DatabaseProfileRegistration {
        profile_key: BRANCH_PROFILE_KEY.into(),
        database_prefix: "devbranch".into(),
        deployment_mode: "erpx_managed".into(),
        credential_profile_key: BRANCH_CREDENTIAL_PROFILE_KEY.into(),
        state: "active".into(),
        profile_version: 1,
        created_by: DEVELOPMENT_CONTROLLER_PRINCIPAL.into(),
    })
    .await?;
    jobs.bind_profile(
        TENANT_ID,
        BRANCH_PROFILE_KEY,
        DEVELOPMENT_CONTROLLER_PRINCIPAL,
    )
    .await?;
    jobs.bind_profile(
        RESTRICTED_TENANT_ID,
        BRANCH_PROFILE_KEY,
        DEVELOPMENT_CONTROLLER_PRINCIPAL,
    )
    .await?;

    let (profile_url, profile_username, profile_password, target_username) =
        if config.port == 55_432 {
            let (url, username, password) = crate::infra::dedicated_branch_profile(root)?;
            (url, username, password, "erpx_app".to_owned())
        } else {
            (
                format!(
                    "postgres://{}:{}/postgres?sslmode=disable",
                    config.host, config.port
                ),
                config.user.clone(),
                config.password.clone(),
                config.user.clone(),
            )
        };
    let profile = ApprovedDatabaseProfileConnection::from_values(
        BRANCH_PROFILE_KEY,
        &profile_url,
        &profile_username,
        &profile_password,
    )?;
    let mut branch_databases = Vec::with_capacity(16);
    for (tenant_id, holding_ref) in [
        (TENANT_ID, HOLDING_REF),
        (RESTRICTED_TENANT_ID, RESTRICTED_HOLDING_REF),
    ] {
        let holding_id = tenant_holding_scope_id(holding_ref);
        let units = if tenant_id == RESTRICTED_TENANT_ID {
            RESTRICTED_UNITS
                .iter()
                .map(|unit| BusinessUnitId::parse(unit).map_err(|_| "invalid fixture unit"))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            BusinessUnitId::ALL.to_vec()
        };
        for fixture in branch_fixtures(tenant_id) {
            let (company_id, branch_id) = branch_ids(tenant_id, *fixture);
            let (write_database_key, read_database_key) =
                branch_database_keys("devbranch", tenant_id, branch_id)?;
            branch_databases.push(write_database_key);
            branch_databases.push(read_database_key);
            let request = ProvisionBranchRequest {
                tenant_id: tenant_id.into(),
                holding_ref: holding_ref.into(),
                holding_id,
                company_id,
                branch_id,
                branch_code: fixture.branch_code.into(),
                branch_name: fixture.branch_name.into(),
                registration_number: format!(
                    "LOCAL/{}/{}",
                    fixture.company_name.replace(' ', "-"),
                    fixture.branch_code
                ),
                profile_key: BRANCH_PROFILE_KEY.into(),
                enabled_business_units: units.clone(),
                idempotency_key: format!(
                    "{FIXTURE_VERSION}:branch:{tenant_id}:{}",
                    fixture.branch_key
                ),
                requested_by: DEVELOPMENT_CONTROLLER_PRINCIPAL.into(),
                correlation_id: Uuid::new_v5(
                    &Uuid::NAMESPACE_URL,
                    format!(
                        "erpx:{FIXTURE_VERSION}:branch-correlation:{tenant_id}:{}",
                        fixture.branch_key
                    )
                    .as_bytes(),
                ),
            };
            let job = jobs.enqueue(&request).await?;
            if let Some(route) = jobs
                .route(tenant_id, branch_id)
                .await?
                .filter(|route| route.state == BranchProvisioningState::Active)
            {
                // Fixture versions may change after a branch database is already active. The
                // route is authoritative here: acknowledge the exact replacement fixture job
                // and reconcile its complete business-unit catalog instead of reprovisioning.
                if job.state != BranchProvisioningState::Active {
                    let updated = sqlx::query(
                        "UPDATE erpx_control.branch_provisioning_jobs
                         SET state='active', completed_at=now(), failure_category=NULL,
                             claimed_by=NULL, claimed_at=NULL, lease_expires_at=NULL,
                             updated_at=now()
                         WHERE tenant_id=$1 AND job_id=$2
                           AND state IN ('queued','failed')",
                    )
                    .bind(tenant_id)
                    .bind(job.job_id)
                    .execute(&control.pools().write)
                    .await?;
                    if updated.rows_affected() != 1 {
                        return Err(PersistenceError::InvalidData(
                            "active branch route has a conflicting development fixture job".into(),
                        )
                        .into());
                    }
                }
                reconcile_active_branch_business_units(
                    &control,
                    &route.tenant_id,
                    route.branch_id,
                    &units,
                )
                .await?;
                continue;
            }
            if job.state == BranchProvisioningState::Failed {
                jobs.retry(tenant_id, job.job_id, request.correlation_id, Utc::now())
                    .await?;
            }
        }
    }

    let provisioner = TenantDatabaseProvisioner::for_approved_profiles(control.clone());
    while let Some(claim) = jobs
        .claim_next(BRANCH_PROVISIONING_WORKER_ID, Utc::now())
        .await?
    {
        let result = async {
            provisioner
                .provision_branch_databases_with_profile(&claim.route, &profile)
                .await?;
            grant_branch_database_objects(&profile, &claim, &target_username).await?;
            provisioner
                .initialize_branch_organization_with_profile(
                    &claim.route,
                    &claim.job,
                    BRANCH_PROVISIONING_WORKER_ID,
                    &profile,
                )
                .await?;
            jobs.mark_verifying(&claim, Utc::now()).await?;
            jobs.complete(&claim, Utc::now()).await
        }
        .await;
        if let Err(error) = result {
            let _ = jobs
                .fail(
                    &claim,
                    "development_fixture_provisioning_failed",
                    Utc::now(),
                )
                .await;
            return Err(error.into());
        }
    }
    if config.port == 55_432 {
        crate::infra::ensure_postgres_hba_for_databases(&branch_databases)?;
    }
    update_holding_display_names(config).await?;
    Ok(())
}

async fn reconcile_active_branch_business_units(
    control: &SqlxControlPlane,
    tenant_id: &str,
    branch_id: Uuid,
    units: &[BusinessUnitId],
) -> TaskResult {
    for unit in units {
        sqlx::query(
            "INSERT INTO erpx_control.branch_business_unit_configurations
                (tenant_id, branch_id, business_unit, active,
                 configuration_version, updated_by)
             VALUES ($1,$2,$3,TRUE,1,$4)
             ON CONFLICT (tenant_id, branch_id, business_unit) DO UPDATE
             SET active=TRUE,
                 configuration_version=CASE
                     WHEN erpx_control.branch_business_unit_configurations.active THEN
                         erpx_control.branch_business_unit_configurations.configuration_version
                     ELSE erpx_control.branch_business_unit_configurations.configuration_version + 1
                 END,
                 updated_by=EXCLUDED.updated_by,
                 updated_at=now()",
        )
        .bind(tenant_id)
        .bind(branch_id)
        .bind(unit.as_str())
        .bind(DEVELOPMENT_CONTROLLER_PRINCIPAL)
        .execute(&control.pools().write)
        .await?;
    }
    Ok(())
}

async fn grant_branch_database_objects(
    profile: &ApprovedDatabaseProfileConnection,
    claim: &erpx_infrastructure::BranchProvisioningClaim,
    target_username: &str,
) -> Result<(), PersistenceError> {
    if profile.username() == target_username {
        return Ok(());
    }
    let source = quote_identifier(profile.username());
    let target = quote_identifier(target_username);
    for database_key in [
        &claim.route.write_database_key,
        &claim.route.read_database_key,
    ] {
        let pool = profile.connect(database_key).await?;
        sqlx::query(&format!(
            "GRANT USAGE, CREATE ON SCHEMA erpx_tenant TO {target}"
        ))
        .execute(&pool)
        .await?;
        sqlx::query(&format!(
            "GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER ON ALL TABLES IN SCHEMA erpx_tenant TO {target}"
        ))
        .execute(&pool)
        .await?;
        sqlx::query(&format!(
            "GRANT USAGE, SELECT, UPDATE ON ALL SEQUENCES IN SCHEMA erpx_tenant TO {target}"
        ))
        .execute(&pool)
        .await?;
        sqlx::query(&format!(
            "ALTER DEFAULT PRIVILEGES FOR ROLE {source} IN SCHEMA erpx_tenant GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER ON TABLES TO {target}"
        ))
        .execute(&pool)
        .await?;
        sqlx::query(&format!(
            "ALTER DEFAULT PRIVILEGES FOR ROLE {source} IN SCHEMA erpx_tenant GRANT USAGE, SELECT, UPDATE ON SEQUENCES TO {target}"
        ))
            .execute(&pool)
            .await?;
        pool.close().await;
    }
    Ok(())
}

async fn update_holding_display_names(config: &DevConfig) -> TaskResult {
    for (database_write, database_read, holding_ref, display_name) in [
        (DATABASES[2], DATABASES[3], HOLDING_REF, "Atlas Holdings"),
        (
            DATABASES[4],
            DATABASES[5],
            RESTRICTED_HOLDING_REF,
            "Beacon Services",
        ),
    ] {
        let write_pool = super::pool(&config.database_url(database_write)).await?;
        sqlx::query(
            "UPDATE erpx_tenant.operations_aggregate_snapshots
             SET state=jsonb_set(state, '{name}', to_jsonb($1::text), TRUE), updated_at=now()
             WHERE aggregate_type='organization.holding' AND holding_ref=$2",
        )
        .bind(display_name)
        .bind(holding_ref)
        .execute(&write_pool)
        .await?;
        write_pool.close().await;
        let read_pool = super::pool(&config.database_url(database_read)).await?;
        sqlx::query(
            "UPDATE erpx_tenant.operations_read_rows
             SET state=jsonb_set(state, '{name}', to_jsonb($1::text), TRUE), updated_at=now()
             WHERE aggregate_type='organization.holding' AND holding_ref=$2",
        )
        .bind(display_name)
        .bind(holding_ref)
        .execute(&read_pool)
        .await?;
        read_pool.close().await;
    }
    Ok(())
}

pub(crate) async fn controller_fixture_current(
    root: &Path,
    config: &DevConfig,
    control_write_database: &str,
    target_tenant: &str,
) -> TaskResult<bool> {
    let password_path = controller_password_path(root);
    let password_is_private_file = fs::symlink_metadata(&password_path).is_ok_and(|metadata| {
        metadata.file_type().is_file()
            && !metadata.file_type().is_symlink()
            && metadata.permissions().mode() & 0o077 == 0
    });
    if !password_is_private_file {
        return Ok(false);
    }

    let write = super::pool(&config.database_url(control_write_database)).await?;
    let controller_and_cluster_exist: bool = sqlx::query_scalar(
        "SELECT
            EXISTS(SELECT 1 FROM erpx_identity.principals
                   WHERE principal_ref=$1 AND status='active')
            AND EXISTS(SELECT 1 FROM erpx_identity.development_cluster_identity
                       WHERE marker='local-ucs-lab' AND profile='local')",
    )
    .bind(DEVELOPMENT_CONTROLLER_PRINCIPAL)
    .fetch_one(&write)
    .await?;
    if !controller_and_cluster_exist {
        write.close().await;
        return Ok(false);
    }
    let mut expected_assignment_ids = vec![controller_assignment_id()];
    expected_assignment_ids.extend(
        permission_catalog()
            .into_iter()
            .map(|entry| controller_catalog_assignment_id(target_tenant, &entry.role_key)),
    );
    let controller_role_count: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM erpx_control.role_assignments assignment
         JOIN erpx_control.role_assignment_target_policies policy
           ON policy.assignment_id=assignment.id
         JOIN erpx_control.role_assignment_tenant_targets target
           ON target.assignment_id=assignment.id
         WHERE assignment.id = ANY($1)
           AND assignment.principal_ref=$2
           AND assignment.valid_until>now()
           AND policy.policy_kind='tenant_allowlist'
           AND policy.environment_gate='local-ucs-lab'
           AND target.tenant_id=$3
           AND (SELECT count(*)
                FROM erpx_control.role_assignment_tenant_targets exact_target
                WHERE exact_target.assignment_id=assignment.id)=1",
    )
    .bind(&expected_assignment_ids)
    .bind(DEVELOPMENT_CONTROLLER_PRINCIPAL)
    .bind(target_tenant)
    .fetch_one(&write)
    .await?;
    write.close().await;
    Ok(controller_role_count == i64::try_from(expected_assignment_ids.len())?)
}

pub(crate) async fn runtime_catalog_current(
    root: &Path,
    config: &DevConfig,
    control_write_database: &str,
    target_tenant: &str,
) -> TaskResult<bool> {
    if !controller_fixture_current(root, config, control_write_database, target_tenant).await? {
        return Ok(false);
    }

    let write = super::pool(&config.database_url(control_write_database)).await?;
    let expected_platform_personas = platform_personas();
    let platform_receipt_current: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM erpx_identity.development_seed_receipts
             WHERE receipt_key='persona-manifest:platform'
               AND fixture_version=$1 AND tenant_id IS NULL AND manifest_digest=$2
         )",
    )
    .bind(FIXTURE_VERSION)
    .bind(platform_manifest_digest())
    .fetch_one(&write)
    .await?;
    let platform_manifest_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_identity.development_persona_manifests
         WHERE tenant_id IS NULL",
    )
    .fetch_one(&write)
    .await?;
    let platform_assignment_ids = expected_platform_personas
        .iter()
        .map(|persona| platform_assignment_id(&persona.role_key))
        .collect::<Vec<_>>();
    let platform_assignment_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.role_assignments
         WHERE id = ANY($1) AND valid_until > now()",
    )
    .bind(&platform_assignment_ids)
    .fetch_one(&write)
    .await?;
    let platform_principal_refs = expected_platform_personas
        .iter()
        .map(|persona| persona.principal_ref.clone())
        .collect::<Vec<_>>();
    let platform_principal_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_identity.principals
         WHERE principal_ref = ANY($1) AND status='active'",
    )
    .bind(&platform_principal_refs)
    .fetch_one(&write)
    .await?;
    if !platform_receipt_current
        || platform_manifest_count != i64::try_from(expected_platform_personas.len())?
        || platform_assignment_count != i64::try_from(platform_assignment_ids.len())?
        || platform_principal_count != i64::try_from(platform_principal_refs.len())?
    {
        write.close().await;
        return Ok(false);
    }

    for (tenant_id, holding_ref) in [
        (TENANT_ID, HOLDING_REF),
        (RESTRICTED_TENANT_ID, RESTRICTED_HOLDING_REF),
    ] {
        let expected_personas =
            scoped_demo_personas(tenant_id, holding_ref, entitled_units(tenant_id));
        let receipt_current: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM erpx_identity.development_seed_receipts
                 WHERE receipt_key=$1 AND fixture_version=$2
                   AND tenant_id=$3 AND manifest_digest=$4
             )",
        )
        .bind(format!("persona-manifest:{tenant_id}"))
        .bind(FIXTURE_VERSION)
        .bind(tenant_id)
        .bind(manifest_digest(tenant_id))
        .fetch_one(&write)
        .await?;
        let manifest_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_identity.development_persona_manifests
             WHERE tenant_id=$1",
        )
        .bind(tenant_id)
        .fetch_one(&write)
        .await?;
        let mut principal_refs = expected_personas
            .iter()
            .map(|persona| persona.principal_ref.clone())
            .collect::<Vec<_>>();
        principal_refs.sort();
        principal_refs.dedup();
        let principal_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_identity.principals
             WHERE principal_ref = ANY($1) AND status='active'",
        )
        .bind(&principal_refs)
        .fetch_one(&write)
        .await?;
        let mut assignment_ids = expected_personas
            .iter()
            .flat_map(|persona| {
                persona_assignment_scopes(tenant_id, holding_ref, &persona.role_key)
                    .into_iter()
                    .map(|(_, scope_key)| {
                        Uuid::new_v5(
                            &Uuid::NAMESPACE_URL,
                            format!(
                                "erpx:dev-access:{tenant_id}:{}:{scope_key}",
                                persona.role_key
                            )
                            .as_bytes(),
                        )
                    })
            })
            .collect::<Vec<_>>();
        assignment_ids.sort_unstable();
        assignment_ids.dedup();
        let assignment_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_control.role_assignments
             WHERE id = ANY($1) AND valid_until > now()",
        )
        .bind(&assignment_ids)
        .fetch_one(&write)
        .await?;
        if !receipt_current
            || manifest_count != i64::try_from(expected_personas.len())?
            || principal_count != i64::try_from(principal_refs.len())?
            || assignment_count != i64::try_from(assignment_ids.len())?
        {
            write.close().await;
            return Ok(false);
        }
    }

    write.close().await;
    Ok(true)
}

pub(crate) async fn fixtures_current(root: &Path, config: &DevConfig) -> TaskResult<bool> {
    if !controller_fixture_current(
        root,
        config,
        DATABASES[0],
        DEVELOPMENT_CONTROLLER_TARGET_TENANT,
    )
    .await?
    {
        return Ok(false);
    }

    let write = super::pool(&config.database_url(DATABASES[0])).await?;
    let platform_receipt_current: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM erpx_identity.development_seed_receipts
             WHERE receipt_key='persona-manifest:platform'
               AND fixture_version=$1 AND tenant_id IS NULL AND manifest_digest=$2
         )",
    )
    .bind(FIXTURE_VERSION)
    .bind(platform_manifest_digest())
    .fetch_one(&write)
    .await?;
    let expected_platform_personas = platform_personas();
    let platform_manifest_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_identity.development_persona_manifests
         WHERE tenant_id IS NULL",
    )
    .fetch_one(&write)
    .await?;
    let platform_assignment_ids = expected_platform_personas
        .iter()
        .map(|persona| platform_assignment_id(&persona.role_key))
        .collect::<Vec<_>>();
    let platform_assignment_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM erpx_control.role_assignments WHERE id = ANY($1)")
            .bind(&platform_assignment_ids)
            .fetch_one(&write)
            .await?;
    if !platform_receipt_current
        || platform_manifest_count < i64::try_from(expected_platform_personas.len())?
        || platform_assignment_count != i64::try_from(platform_assignment_ids.len())?
    {
        write.close().await;
        return Ok(false);
    }

    let local_tenants = sqlx::query_scalar::<_, String>(
        "SELECT tenant_id FROM erpx_control.tenant_registry
         WHERE state='active'
           AND tenant_id IN ($1,$2)
           AND credential_key_ref='local_shared'
         ORDER BY tenant_id",
    )
    .bind(TENANT_ID)
    .bind(RESTRICTED_TENANT_ID)
    .fetch_all(&write)
    .await?;
    if !local_tenants.iter().any(|tenant_id| tenant_id == TENANT_ID)
        || !local_tenants
            .iter()
            .any(|tenant_id| tenant_id == RESTRICTED_TENANT_ID)
    {
        write.close().await;
        return Ok(false);
    }
    for tenant_id in &local_tenants {
        let holding_ref = if tenant_id == TENANT_ID {
            HOLDING_REF
        } else {
            RESTRICTED_HOLDING_REF
        };
        let expected_personas =
            scoped_demo_personas(tenant_id, holding_ref, entitled_units(tenant_id));
        let receipt_current: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM erpx_identity.development_seed_receipts
                 WHERE receipt_key=$1 AND fixture_version=$2
                   AND tenant_id=$3 AND manifest_digest=$4
             )",
        )
        .bind(format!("persona-manifest:{tenant_id}"))
        .bind(FIXTURE_VERSION)
        .bind(tenant_id)
        .bind(manifest_digest(tenant_id))
        .fetch_one(&write)
        .await?;
        let manifest_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_identity.development_persona_manifests
             WHERE tenant_id=$1",
        )
        .bind(tenant_id)
        .fetch_one(&write)
        .await?;
        let mut required_assignment_ids = match tenant_id.as_str() {
            TENANT_ID | RESTRICTED_TENANT_ID => {
                let holding_ref = if tenant_id == TENANT_ID {
                    HOLDING_REF
                } else {
                    RESTRICTED_HOLDING_REF
                };
                expected_personas
                    .iter()
                    .flat_map(|persona| {
                        persona_assignment_scopes(tenant_id, holding_ref, &persona.role_key)
                            .into_iter()
                            .map(|(_, scope_key)| {
                                Uuid::new_v5(
                                    &Uuid::NAMESPACE_URL,
                                    format!(
                                        "erpx:dev-access:{tenant_id}:{}:{scope_key}",
                                        persona.role_key
                                    )
                                    .as_bytes(),
                                )
                            })
                    })
                    .collect::<Vec<_>>()
            }
            _ => Vec::new(),
        };
        // Negative personas intentionally reuse real roles. Count the durable assignments by
        // their deterministic identity, not by the number of persona scenarios that reference
        // each assignment.
        required_assignment_ids.sort_unstable();
        required_assignment_ids.dedup();
        let assignment_count: i64 = if required_assignment_ids.is_empty() {
            0
        } else {
            sqlx::query_scalar(
                "SELECT count(*) FROM erpx_control.role_assignments WHERE id = ANY($1)",
            )
            .bind(&required_assignment_ids)
            .fetch_one(&write)
            .await?
        };
        if !receipt_current
            || manifest_count < i64::try_from(expected_personas.len())?
            || assignment_count != i64::try_from(required_assignment_ids.len())?
        {
            write.close().await;
            return Ok(false);
        }
    }

    let active_branch_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.branch_database_routes
         WHERE tenant_id IN ($1,$2) AND state='active'",
    )
    .bind(TENANT_ID)
    .bind(RESTRICTED_TENANT_ID)
    .fetch_one(&write)
    .await?;
    let configured_branch_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (
             SELECT route.tenant_id, route.branch_id
             FROM erpx_control.branch_database_routes route
             JOIN erpx_control.branch_business_unit_configurations configuration
               ON configuration.tenant_id=route.tenant_id
              AND configuration.branch_id=route.branch_id
             WHERE route.tenant_id IN ($1,$2)
               AND route.state='active'
               AND configuration.active
               AND configuration.business_unit = ANY($3)
             GROUP BY route.tenant_id, route.branch_id
             HAVING count(DISTINCT configuration.business_unit)=$4
         ) configured_branches",
    )
    .bind(TENANT_ID)
    .bind(RESTRICTED_TENANT_ID)
    .bind(erpx_identity::BUSINESS_UNITS.as_slice())
    .bind(i64::try_from(erpx_identity::BUSINESS_UNITS.len())?)
    .fetch_one(&write)
    .await?;
    write.close().await;
    let expected_branch_count = i64::try_from(DEMO_BRANCHES.len() + RESTRICTED_BRANCHES.len())?;
    Ok(active_branch_count == expected_branch_count
        && configured_branch_count == expected_branch_count)
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

pub(crate) async fn status(
    root: &Path,
    config: &DevConfig,
    control_write_database: &str,
) -> TaskResult {
    let path = controller_password_path(root);
    let write = super::pool(&config.database_url(control_write_database)).await?;
    let manifests: i64 =
        sqlx::query_scalar("SELECT count(*) FROM erpx_identity.development_persona_manifests")
            .fetch_one(&write)
            .await?;
    let active_contexts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_identity.development_persona_contexts
         WHERE revoked_at IS NULL AND expires_at > now()",
    )
    .fetch_one(&write)
    .await?;
    let controller_roles: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT assignment.id)
         FROM erpx_control.role_assignments assignment
         JOIN erpx_control.role_assignment_target_policies policy
           ON policy.assignment_id=assignment.id
         JOIN erpx_control.role_assignment_tenant_targets target
           ON target.assignment_id=assignment.id
         WHERE assignment.principal_ref=$1
           AND assignment.valid_until>now()
           AND policy.policy_kind='tenant_allowlist'
           AND policy.environment_gate=$2",
    )
    .bind(DEVELOPMENT_CONTROLLER_PRINCIPAL)
    .bind(DEVELOPMENT_CLUSTER_MARKER)
    .fetch_one(&write)
    .await?;
    println!(
        "Development access: {} · database: {control_write_database} · personas: {manifests} · controller roles: {controller_roles} · active contexts: {active_contexts} · controller secret: {}",
        if path.exists() {
            "initialized"
        } else {
            "not initialized"
        },
        path.display()
    );
    write.close().await;
    Ok(())
}

pub(crate) fn credentials(root: &Path, tenant: Option<&str>) -> TaskResult {
    if let Some(tenant) = tenant
        && (tenant.is_empty()
            || tenant.len() > 128
            || !tenant
                .chars()
                .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.')))
    {
        return Err("tenant credential profile contains an invalid identifier".into());
    }
    let path = tenant.map_or_else(
        || controller_password_path(root),
        |_| runtime_dir(root).join(UCS_SHARED_PASSWORD_FILE),
    );
    let metadata = fs::symlink_metadata(&path).map_err(
        |_| "development credential is not initialized; run `cargo xtask dev-access init` first",
    )?;
    if !metadata.file_type().is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("development credential must be a private regular file".into());
    }
    let credential = fs::read_to_string(&path)?;
    if credential.is_empty() || credential.chars().any(char::is_control) {
        return Err("development credential file is empty or invalid".into());
    }
    match tenant {
        Some(tenant) => {
            println!("UCS authority credentials:");
            println!("  authority: {tenant}");
            println!("  password: {credential}");
        }
        None => {
            println!("Development Controller credentials:");
            println!("  principal: {DEVELOPMENT_CONTROLLER_PRINCIPAL}");
            println!("  password: {credential}");
        }
    }
    println!("  source: {}", path.display());
    println!("Credentials are intentionally exposed only by this command.");
    Ok(())
}

pub(crate) async fn rotate_controller(
    root: &Path,
    config: &DevConfig,
    control_write_database: &str,
    confirmation: &str,
) -> TaskResult {
    if confirmation != "ROTATE_ERPX_DEV_CONTROLLER" {
        return Err("dev-access rotate requires --confirm ROTATE_ERPX_DEV_CONTROLLER".into());
    }
    init(root, config)?;
    let new_password = format!(
        "ERPX-DEV-{}-{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let write = super::pool(&config.database_url(control_write_database)).await?;
    let revoked = SqlxIdentityStore::new(write.clone())
        .rotate_development_controller_password(&new_password, Utc::now())
        .await?;
    let path = controller_password_path(root);
    fs::write(&path, new_password)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    write.close().await;
    println!("Development Controller password rotated; {revoked} controller session(s) revoked.");
    Ok(())
}

pub(crate) async fn reset(
    root: &Path,
    config: &DevConfig,
    control_write_database: &str,
    confirmation: &str,
) -> TaskResult {
    if confirmation != "RESET_ERPX_DEV_ACCESS" {
        return Err("dev-access reset requires --confirm RESET_ERPX_DEV_ACCESS".into());
    }
    let write = super::pool(&config.database_url(control_write_database)).await?;
    let revoked = SqlxIdentityStore::new(write.clone())
        .reset_development_controller(Utc::now())
        .await?;
    let path = controller_password_path(root);
    if path.exists() {
        fs::remove_file(path)?;
    }
    write.close().await;
    println!(
        "Development access secret reset; {revoked} controller session(s) revoked; .runtime/n8n was not touched."
    );
    Ok(())
}

fn entitled_units(tenant_id: &str) -> &'static [&'static str] {
    if tenant_id == RESTRICTED_TENANT_ID {
        &RESTRICTED_UNITS
    } else {
        erpx_identity::BUSINESS_UNITS.as_slice()
    }
}

fn platform_manifest_digest() -> String {
    let mut digest = Sha256::new();
    for value in [
        FIXTURE_VERSION,
        DEVELOPMENT_CONTROLLER_PRINCIPAL,
        "local-ucs-lab",
        "erpx-local-ucs-platform",
        "erpx-ucs-authority",
        PLATFORM_ASSIGNMENT_REVISION,
        "platform",
        "Asia/Bangkok",
        "local:dev-access",
    ] {
        update_manifest_digest(&mut digest, value);
    }
    update_manifest_digest(&mut digest, &format!("{:?}", platform_personas()));
    format!("{:x}", digest.finalize())
}

fn manifest_digest(tenant_id: &str) -> String {
    let mut digest = Sha256::new();
    for value in [
        FIXTURE_VERSION,
        DEVELOPMENT_CONTROLLER_PRINCIPAL,
        BRANCH_PROFILE_KEY,
        BRANCH_CREDENTIAL_PROFILE_KEY,
        "devbranch",
        "erpx_managed",
        "erpx-ucs-authority",
        tenant_id,
        "Asia/Bangkok",
        "local:dev-access",
    ] {
        update_manifest_digest(&mut digest, value);
    }
    update_manifest_digest(
        &mut digest,
        &format!(
            "{:?}",
            scoped_demo_personas(
                tenant_id,
                if tenant_id == TENANT_ID {
                    HOLDING_REF
                } else {
                    RESTRICTED_HOLDING_REF
                },
                entitled_units(tenant_id),
            )
        ),
    );
    for fixture in branch_fixtures(tenant_id) {
        update_manifest_digest(&mut digest, &format!("{fixture:?}"));
    }
    format!("{:x}", digest.finalize())
}

fn update_manifest_digest(digest: &mut Sha256, value: &str) {
    digest.update(value.len().to_le_bytes());
    digest.update(value.as_bytes());
}

async fn ensure_principal(
    pool: &PgPool,
    principal_ref: &str,
    at: chrono::DateTime<Utc>,
) -> TaskResult {
    sqlx::query(
        "INSERT INTO erpx_identity.principals
            (id, principal_ref, status, created_at, updated_at)
         VALUES ($1,$2,'active',$3,$3)
         ON CONFLICT (principal_ref) DO UPDATE SET status='active', updated_at=EXCLUDED.updated_at",
    )
    .bind(Uuid::new_v4())
    .bind(principal_ref)
    .bind(at)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_fixture_digests_cover_authority_and_tenant_inputs() {
        let platform = platform_manifest_digest();
        let demo = manifest_digest(TENANT_ID);
        let restricted = manifest_digest(RESTRICTED_TENANT_ID);
        assert_eq!(platform.len(), 64);
        assert_eq!(demo.len(), 64);
        assert_eq!(restricted.len(), 64);
        assert_ne!(platform, demo);
        assert_ne!(demo, restricted);
    }

    #[test]
    fn runtime_catalog_contains_platform_and_both_demo_tenants() {
        let platform = platform_personas();
        let demo = scoped_demo_personas(TENANT_ID, HOLDING_REF, entitled_units(TENANT_ID));
        let restricted = scoped_demo_personas(
            RESTRICTED_TENANT_ID,
            RESTRICTED_HOLDING_REF,
            entitled_units(RESTRICTED_TENANT_ID),
        );
        assert_eq!(platform.len() + demo.len() + restricted.len(), 146);
        assert!(platform.iter().all(|persona| persona.tenant_id.is_none()));
        let demo_denied = demo.iter().filter(|persona| !persona.entitled).count();
        let restricted_denied = restricted
            .iter()
            .filter(|persona| !persona.entitled)
            .count();
        assert!(demo_denied > 0);
        assert_eq!(restricted_denied, demo_denied);
        assert_eq!(entitled_units(RESTRICTED_TENANT_ID).len(), 12);
    }

    #[test]
    fn controller_catalog_has_every_role_once_with_bounded_representative_scopes() {
        let assignments = controller_catalog_assignments(
            RESTRICTED_TENANT_ID,
            RESTRICTED_HOLDING_REF,
            branch_ids(RESTRICTED_TENANT_ID, RESTRICTED_BRANCHES[0]),
            Utc::now(),
        );
        assert_eq!(assignments.len(), 73);

        let ids = assignments
            .iter()
            .map(|assignment| assignment.id)
            .collect::<std::collections::BTreeSet<_>>();
        let role_keys = assignments
            .iter()
            .map(|assignment| assignment.role_key.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(), 73);
        assert_eq!(role_keys.len(), 73);

        let permission_by_role = permission_catalog()
            .into_iter()
            .map(|entry| (entry.role_key, entry.permissions))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut platform_scopes = 0;
        let mut holding_scopes = 0;
        let mut company_scopes = 0;
        let mut branch_scopes = 0;
        let (expected_company_id, expected_branch_id) =
            branch_ids(RESTRICTED_TENANT_ID, RESTRICTED_BRANCHES[0]);
        for assignment in &assignments {
            assert_eq!(assignment.principal_ref, DEVELOPMENT_CONTROLLER_PRINCIPAL);
            assert_eq!(
                permission_by_role.get(&assignment.role_key),
                Some(&assignment.permissions)
            );
            assert!(!assignment.permissions.iter().any(|permission| {
                matches!(
                    permission.as_str(),
                    "development.mfa.bypass" | "development.sod.self-approve"
                )
            }));
            match &assignment.scope {
                Scope::Platform => platform_scopes += 1,
                Scope::Holding { .. } => holding_scopes += 1,
                Scope::Company { company_id, .. } => {
                    company_scopes += 1;
                    assert_eq!(*company_id, expected_company_id);
                }
                Scope::Branch {
                    company_id,
                    branch_id,
                    ..
                } => {
                    branch_scopes += 1;
                    assert_eq!(*company_id, expected_company_id);
                    assert_eq!(*branch_id, expected_branch_id);
                }
                Scope::Subtree { .. } => panic!("catalog roles never use subtree scope"),
            }
        }
        assert_eq!(platform_scopes, 8);
        assert_eq!(holding_scopes, 3);
        assert_eq!(company_scopes, 1);
        assert_eq!(branch_scopes, 61);
    }
}
