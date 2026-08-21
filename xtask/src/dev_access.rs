use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use chrono::Utc;
use erpx_domain::{
    access::{RiskTier, RoleAssignment},
    commercial::{BillingLifecycle, BillingState, EntitlementAllocationBook, EntitlementCapacity},
    contracts::AuthorityScope as Scope,
    data_plane::BusinessUnitId,
};
use erpx_identity::{
    AuthenticatedAuthority, AuthenticatedPrincipalKind, AuthenticatedRealm,
    DEVELOPMENT_CLUSTER_MARKER, DEVELOPMENT_CONTROLLER_PRINCIPAL, DEVELOPMENT_CONTROLLER_ROLE,
    DEVELOPMENT_CONTROLLER_TARGET_HOLDING, DirectoryRoleTargetScope, SqlxIdentityStore,
    demo_personas, development_controller_permissions, permission_catalog, platform_personas,
};
use erpx_infrastructure::{
    ApprovedDatabaseProfileConnection, BranchProvisioningState, DatabaseProfileRegistration,
    HoldingDatabaseProvisioner, PersistenceError, ProvisionBranchRequest, SodDecision,
    SqlxBranchProvisioningStore, SqlxCommercialStore, SqlxControlPlane, branch_database_keys,
    holding_holding_scope_id,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    DATA_PROFILE_KEY, DATABASES, DevConfig, HOLDING_ID, RESTRICTED_DATA_PROFILE_KEY,
    RESTRICTED_HOLDING_ID, TaskResult, runtime_dir,
};

const CONTROLLER_PASSWORD_FILE: &str = "secrets/dev-controller.password";
const UCS_SHARED_PASSWORD_FILE: &str = "secrets/ucs-shared.password";
const FIXTURE_VERSION: &str = "erpx-dev-access-v8";
const PLATFORM_ASSIGNMENT_REVISION: &str = "platform-permissions-v6";
const CONTROLLER_ASSIGNMENT_REVISION: &str = "restricted-holding-full-admin-v1";
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

fn branch_fixtures(holding_id: &str) -> &'static [DevelopmentBranchFixture] {
    if holding_id == RESTRICTED_HOLDING_ID {
        &RESTRICTED_BRANCHES
    } else {
        &DEMO_BRANCHES
    }
}

fn branch_ids(holding_id: &str, fixture: DevelopmentBranchFixture) -> (Uuid, Uuid) {
    let company_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!(
            "erpx:dev-access:company:{holding_id}:{}",
            fixture.company_key
        )
        .as_bytes(),
    );
    let branch_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("erpx:dev-access:branch:{holding_id}:{}", fixture.branch_key).as_bytes(),
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

fn controller_catalog_assignment_id(target_holding: &str, role_key: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!(
            "erpx:dev-access:{CONTROLLER_CATALOG_ASSIGNMENT_REVISION}:{target_holding}:{role_key}"
        )
        .as_bytes(),
    )
}

fn controller_catalog_holding_assignment_id(target_holding: &str, role_key: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!(
            "erpx:dev-access:{CONTROLLER_CATALOG_ASSIGNMENT_REVISION}:{target_holding}:{role_key}:holding"
        )
        .as_bytes(),
    )
}

fn controller_catalog_assignments(
    target_holding: &str,
    data_profile_key: &str,
    representative_branch: (Uuid, Uuid),
    now: chrono::DateTime<Utc>,
) -> Vec<RoleAssignment> {
    let holding_id = holding_holding_scope_id(data_profile_key);
    permission_catalog()
        .into_iter()
        .flat_map(|entry| {
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
                    .map_or("holding-administration", |(domain, _)| domain)
                    .to_owned()
            };
            let assignment = RoleAssignment {
                id: controller_catalog_assignment_id(target_holding, &entry.role_key),
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
            };
            let mut assignments = vec![assignment.clone()];
            if matches!(
                assignment.role_key.as_str(),
                "assets_administrator" | "maintenance_administrator"
            ) {
                assignments.push(RoleAssignment {
                    id: controller_catalog_holding_assignment_id(
                        target_holding,
                        &assignment.role_key,
                    ),
                    scope: Scope::Holding { holding_id },
                    ..assignment
                });
            }
            assignments
        })
        .collect()
}

async fn controller_catalog_scope_context(
    write: &PgPool,
    target_holding: &str,
) -> TaskResult<(String, (Uuid, Uuid))> {
    let data_profile_key: String = sqlx::query_scalar(
        "SELECT data_profile_key FROM erpx_control.holding_registry
         WHERE holding_id=$1 AND state='active'",
    )
    .bind(target_holding)
    .fetch_one(write)
    .await?;
    let deterministic_branch = branch_ids(target_holding, RESTRICTED_BRANCHES[0]);
    let representative = sqlx::query(
        "SELECT company_id, branch_id
         FROM erpx_control.branch_database_routes
         WHERE holding_id=$1 AND state='active'
         ORDER BY (branch_id=$2) DESC, branch_id
         LIMIT 1",
    )
    .bind(target_holding)
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
    Ok((
        data_profile_key,
        representative.unwrap_or(deterministic_branch),
    ))
}

async fn reconcile_controller_role(
    write: &PgPool,
    target_holding: &str,
    now: chrono::DateTime<Utc>,
) -> TaskResult {
    let assignment = RoleAssignment {
        id: controller_assignment_id(),
        principal_ref: DEVELOPMENT_CONTROLLER_PRINCIPAL.into(),
        role_key: DEVELOPMENT_CONTROLLER_ROLE.into(),
        permissions: development_controller_permissions(),
        functional_domain: "platform_holding_administration".into(),
        authority_level: 64,
        approval_limit_minor: 0,
        scope: Scope::Platform,
        valid_from: now,
        valid_until: now + chrono::Duration::days(365),
        timezone: "Asia/Bangkok".into(),
        risk_tier: RiskTier::Standard,
        grant_reason: "environment-gated local restricted holding administration".into(),
        ticket_reference: Some("local:dev-controller-role".into()),
        grantor: DEVELOPMENT_CONTROLLER_PRINCIPAL.into(),
        approver: None,
        renewal_reviewer: None,
        parent_assignment_id: None,
        assignment_version: 1,
    };
    let (data_profile_key, representative_branch) =
        controller_catalog_scope_context(write, target_holding).await?;
    let catalog_assignments = controller_catalog_assignments(
        target_holding,
        &data_profile_key,
        representative_branch,
        now,
    );
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
             VALUES ($1,'holding_allowlist',$2,$3,$4)
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
            "DELETE FROM erpx_control.role_assignment_holding_targets
             WHERE assignment_id=$1",
        )
        .bind(candidate.id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_control.role_assignment_holding_targets
                (assignment_id, holding_id, configured_at)
             VALUES ($1,$2,$3)",
        )
        .bind(candidate.id)
        .bind(target_holding)
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
                 'local restricted holding controller reconcile',$3,$4)",
    )
    .bind(Uuid::new_v4())
    .bind(DEVELOPMENT_CONTROLLER_PRINCIPAL)
    .bind(Uuid::new_v4())
    .bind(serde_json::json!({
        "role_key": DEVELOPMENT_CONTROLLER_ROLE,
        "target_holding": target_holding,
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
             role_key,holding_id,organization_scope,feature,correlation_id,
             outcome,recorded_at,details)
         SELECT $3,$1,manifest.principal_ref,manifest.persona_key,
                manifest.role_key,manifest.holding_id,manifest.organization_scope,
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
    holding_key: &str,
    data_profile_key: &str,
    role_key: &str,
) -> Vec<(Scope, String)> {
    let holding_id = holding_holding_scope_id(data_profile_key);
    if role_key == "company_admin" {
        let mut seen = Vec::new();
        return branch_fixtures(holding_key)
            .iter()
            .filter_map(|fixture| {
                let (company_id, _) = branch_ids(holding_key, *fixture);
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
        return branch_fixtures(holding_key)
            .iter()
            .map(|fixture| {
                let (company_id, branch_id) = branch_ids(holding_key, *fixture);
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
    holding_id: &str,
    data_profile_key: &str,
    entitled_units: &[&str],
) -> Vec<erpx_identity::DevelopmentPersona> {
    let mut personas = demo_personas(holding_id, entitled_units);
    for persona in &mut personas {
        if let Some((scope, _)) =
            persona_assignment_scopes(holding_id, data_profile_key, &persona.role_key).first()
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
    for authority in ["platform", HOLDING_ID, RESTRICTED_HOLDING_ID] {
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
    let authorities = ["platform", HOLDING_ID, RESTRICTED_HOLDING_ID]
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
    let binary = super::runtime_binary(root, "erpx-ucs-lab");
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
    let controller_authority: Option<(String, String)> = sqlx::query_as(
        "SELECT authenticated_realm, principal_kind
         FROM erpx_identity.principals WHERE principal_ref=$1",
    )
    .bind(DEVELOPMENT_CONTROLLER_PRINCIPAL)
    .fetch_optional(write)
    .await?;
    if let Some((realm, kind)) = controller_authority {
        if realm != "platform_administration" || kind != "human" {
            return Err("development controller authenticated authority is contradictory".into());
        }
    } else {
        identity
            .create_principal_with_password_and_authority(
                DEVELOPMENT_CONTROLLER_PRINCIPAL,
                controller_password.trim(),
                AuthenticatedAuthority {
                    realm: AuthenticatedRealm::PlatformAdministration,
                    principal_kind: AuthenticatedPrincipalKind::Human,
                },
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
    controller_target_holding: &str,
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

    let fresh_runtime = controller_target_holding != DEVELOPMENT_CONTROLLER_TARGET_HOLDING;
    let holdings = if fresh_runtime {
        let row = sqlx::query(
            "SELECT registry.holding_id, registry.data_profile_key
             FROM erpx_control.holding_registry registry
             JOIN erpx_control.customer_holding_bindings binding USING (holding_id)
             WHERE registry.holding_id=$1 AND registry.state='active'
               AND binding.binding_state='configured'",
        )
        .bind(controller_target_holding)
        .fetch_optional(&write)
        .await?
        .ok_or("the development controller target has no authoritative holding workspace")?;
        vec![(
            row.try_get::<String, _>("holding_id")?,
            row.try_get::<String, _>("data_profile_key")?,
            erpx_identity::BUSINESS_UNITS.to_vec(),
        )]
    } else {
        vec![
            (
                HOLDING_ID.to_owned(),
                DATA_PROFILE_KEY.to_owned(),
                erpx_identity::BUSINESS_UNITS.to_vec(),
            ),
            (
                RESTRICTED_HOLDING_ID.to_owned(),
                RESTRICTED_DATA_PROFILE_KEY.to_owned(),
                RESTRICTED_UNITS.to_vec(),
            ),
        ]
    };
    if fresh_runtime {
        sqlx::query(
            "DELETE FROM erpx_identity.development_persona_contexts
             WHERE persona_key IN (
                 SELECT persona_key
                 FROM erpx_identity.development_persona_manifests
                 WHERE holding_id IS NOT NULL AND holding_id <> $1
             )",
        )
        .bind(controller_target_holding)
        .execute(&write)
        .await?;
        sqlx::query(
            "DELETE FROM erpx_identity.development_persona_manifests
             WHERE holding_id IS NOT NULL AND holding_id <> $1",
        )
        .bind(controller_target_holding)
        .execute(&write)
        .await?;
    }
    let control = std::sync::Arc::new(SqlxControlPlane::new(crate::ControlPlanePools {
        write: write.clone(),
        read: super::pool(&config.database_url(control_read_database)).await?,
    }));
    let lifecycle = erpx_application::RoleLifecycleService::new(
        control.clone(),
        std::sync::Arc::new(erpx_application::SystemClock),
        control.clone(),
    );
    reconcile_controller_role(&write, controller_target_holding, now).await?;
    let platform_personas = platform_personas();
    let platform_persona_count = platform_personas.len();
    identity
        .reconcile_development_personas(&platform_personas, now)
        .await?;
    for persona in platform_personas {
        ensure_principal(
            &write,
            &persona.principal_ref,
            "platform_administration",
            now,
        )
        .await?;
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
            (receipt_key, fixture_version, holding_id, manifest_digest, applied_at, counts)
         VALUES ('persona-manifest:platform',$1,NULL,$2,$3,$4)
         ON CONFLICT (receipt_key) DO UPDATE SET
            fixture_version=EXCLUDED.fixture_version,
            holding_id=EXCLUDED.holding_id,
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
    for (holding_id, data_profile_key, entitled_units) in holdings {
        let personas = scoped_demo_personas(&holding_id, &data_profile_key, &entitled_units);
        identity
            .reconcile_development_personas(&personas, now)
            .await?;
        for persona in personas {
            ensure_principal(&write, &persona.principal_ref, "internal_workforce", now).await?;
            for (scope, scope_key) in
                persona_assignment_scopes(&holding_id, &data_profile_key, &persona.role_key)
            {
                let assignment_id = Uuid::new_v5(
                    &Uuid::NAMESPACE_URL,
                    format!(
                        "erpx:dev-access:{holding_id}:{}:{scope_key}",
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
        let digest = manifest_digest(&holding_id);
        sqlx::query(
            "INSERT INTO erpx_identity.development_seed_receipts
                (receipt_key, fixture_version, holding_id, manifest_digest, applied_at, counts)
             VALUES ($1,$2,$3,$4,$5,$6)
             ON CONFLICT (receipt_key) DO UPDATE SET
                fixture_version=EXCLUDED.fixture_version,
                manifest_digest=EXCLUDED.manifest_digest,
                applied_at=EXCLUDED.applied_at,
                counts=EXCLUDED.counts",
        )
        .bind(format!("persona-manifest:{holding_id}"))
        .bind(FIXTURE_VERSION)
        .bind(&holding_id)
        .bind(digest)
        .bind(now)
        .bind(serde_json::json!({
            "core_personas": 65,
            "negative_personas": 4,
            "companies": 2,
            "branches": 4,
            "holding_name": if holding_id == HOLDING_ID { "Atlas Holdings" } else if holding_id == RESTRICTED_HOLDING_ID { "Beacon Services" } else { "Development Holding" },
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
    controller_target_holding: &str,
) -> TaskResult {
    let (write, control, now) = reconcile_persona_catalog(
        root,
        config,
        control_write_database,
        control_read_database,
        controller_target_holding,
    )
    .await?;
    reconcile_typed_organization_read_fixtures(config, &control, now).await?;
    reconcile_development_command_entitlements(&control, now).await?;
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
        DEVELOPMENT_CONTROLLER_TARGET_HOLDING,
    )
    .await?;
    let identity = SqlxIdentityStore::new(write.clone());
    let lifecycle = erpx_application::RoleLifecycleService::new(
        control.clone(),
        std::sync::Arc::new(erpx_application::SystemClock),
        control.clone(),
    );
    let local_holdings = sqlx::query(
        "SELECT holding_id, data_profile_key
         FROM erpx_control.holding_registry
         WHERE state='active' AND credential_key_ref='local-development-disabled'",
    )
    .fetch_all(&write)
    .await?;
    for row in local_holdings {
        let holding_key: String = row.try_get("holding_id")?;
        if [HOLDING_ID, RESTRICTED_HOLDING_ID].contains(&holding_key.as_str()) {
            continue;
        }
        let data_profile_key: String = row.try_get("data_profile_key")?;
        let personas = scoped_demo_personas(
            &holding_key,
            &data_profile_key,
            &erpx_identity::BUSINESS_UNITS,
        );
        identity
            .reconcile_development_personas(&personas, now)
            .await?;
        let holding_id = holding_holding_scope_id(&data_profile_key);
        for persona in personas {
            ensure_principal(&write, &persona.principal_ref, "internal_workforce", now).await?;
            let assignment_id = Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!(
                    "erpx:dev-access:{holding_id}:{}:holding:{holding_id}",
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
                (receipt_key, fixture_version, holding_id, manifest_digest, applied_at, counts)
             VALUES ($1,$2,$3,$4,$5,$6)
             ON CONFLICT (receipt_key) DO UPDATE SET
                fixture_version=EXCLUDED.fixture_version,
                manifest_digest=EXCLUDED.manifest_digest,
                applied_at=EXCLUDED.applied_at,
                counts=EXCLUDED.counts",
        )
        .bind(format!("persona-manifest:{holding_key}"))
        .bind(FIXTURE_VERSION)
        .bind(&holding_key)
        .bind(manifest_digest(&holding_key))
        .bind(now)
        .bind(
            serde_json::json!({"core_personas": 65, "negative_personas": 4, "auto_enrolled": true}),
        )
        .execute(&write)
        .await?;
    }
    sqlx::query(
        "UPDATE erpx_control.holding_registry
         SET isolation_mode='holding_and_branch_pairs', updated_at=$2
         WHERE holding_id IN ($1, $3)",
    )
    .bind(HOLDING_ID)
    .bind(now)
    .bind(RESTRICTED_HOLDING_ID)
    .execute(&write)
    .await?;
    reconcile_branch_fixtures(root, config, control.clone()).await?;
    reconcile_typed_organization_read_fixtures(config, &control, now).await?;
    reconcile_development_command_entitlements(&control, now).await?;
    reconcile_cluster_identity(&write, now).await?;
    write.close().await;
    Ok(())
}

async fn reconcile_typed_organization_read_fixtures(
    config: &DevConfig,
    control: &SqlxControlPlane,
    now: chrono::DateTime<Utc>,
) -> TaskResult {
    let holdings = sqlx::query(
        "SELECT registry.holding_id, registry.data_profile_key, registry.write_database_key,
                registry.read_database_key, binding.canonical_holding_id
         FROM erpx_control.holding_registry registry
         JOIN erpx_control.customer_holding_bindings binding USING (holding_id)
         WHERE registry.state='active' AND binding.binding_state='configured'
           AND registry.credential_key_ref IN
               ('local_shared','local-development-disabled','database-profile:default')
         ORDER BY registry.holding_id",
    )
    .fetch_all(&control.pools().write)
    .await?;
    for holding in holdings {
        let holding_key: String = holding.try_get("holding_id")?;
        let data_profile_key: String = holding.try_get("data_profile_key")?;
        let write_database_key: String = holding.try_get("write_database_key")?;
        let read_database_key: String = holding.try_get("read_database_key")?;
        let holding_id: Uuid = holding.try_get("canonical_holding_id")?;
        let write = super::pool(&config.database_url(&write_database_key)).await?;
        let holding_uuid = holding_id;
        let mut write_transaction = write.begin().await?;
        let bound_holding: String =
            sqlx::query_scalar("SELECT set_config('app.holding_id',$1,true)")
                .bind(holding_uuid.to_string())
                .fetch_one(&mut *write_transaction)
                .await?;
        if bound_holding != holding_uuid.to_string() {
            return Err("typed Organization authority holding binding was not retained".into());
        }
        let authoritative_node_ids = sqlx::query_scalar::<_, Uuid>(
            "SELECT node_id
             FROM erpx_organization_write.organization_node_authority
             WHERE holding_id=$1",
        )
        .bind(holding_uuid)
        .fetch_all(&mut *write_transaction)
        .await?
        .into_iter()
        .collect::<BTreeSet<_>>();
        write_transaction.commit().await?;
        write.close().await;
        let read = super::pool(&config.database_url(&read_database_key)).await?;
        let mut read_transaction = read.begin().await?;
        let bound_holding: String =
            sqlx::query_scalar("SELECT set_config('app.holding_id',$1,true)")
                .bind(holding_uuid.to_string())
                .fetch_one(&mut *read_transaction)
                .await?;
        if bound_holding != holding_uuid.to_string() {
            return Err("typed Organization fixture holding binding was not retained".into());
        }
        let generation = match sqlx::query_scalar(
            "SELECT generation FROM erpx_cqrs_read.projection_generations
             WHERE holding_id=$1 AND projection_name='organization.nodes' AND status='active'",
        )
        .bind(holding_uuid)
        .fetch_optional(&mut *read_transaction)
        .await?
        {
            Some(generation) => generation,
            None => {
                let generation = Uuid::new_v5(
                    &Uuid::NAMESPACE_URL,
                    format!("erpx:{FIXTURE_VERSION}:{holding_id}:organization.nodes").as_bytes(),
                );
                sqlx::query(
                    "INSERT INTO erpx_cqrs_read.projection_generations
                        (holding_id,projection_name,generation,status,source_sequence,created_at,activated_at)
                     VALUES ($1,'organization.nodes',$2,'active',0,$3,$3)",
                )
                .bind(holding_uuid)
                .bind(generation)
                .bind(now)
                .execute(&mut *read_transaction)
                .await?;
                generation
            }
        };

        let holding_name = if data_profile_key == DATA_PROFILE_KEY {
            "Atlas Holdings"
        } else if data_profile_key == RESTRICTED_DATA_PROFILE_KEY {
            "Beacon Services"
        } else {
            "Development Holding"
        };
        if !authoritative_node_ids.contains(&holding_id) {
            insert_typed_organization_read_fixture(
                &mut read_transaction,
                holding_uuid,
                generation,
                1,
                holding_id,
                "holding",
                holding_id,
                None,
                None,
                data_profile_key.clone(),
                holding_name.to_owned(),
                now,
            )
            .await?;
        }

        let routes = sqlx::query(
            "SELECT route.company_id, route.branch_id, job.branch_code, job.branch_name
             FROM erpx_control.branch_database_routes route
             JOIN LATERAL (
                 SELECT branch_code, branch_name
                 FROM erpx_control.branch_provisioning_jobs candidate
                 WHERE candidate.holding_id=route.holding_id
                   AND candidate.branch_id=route.branch_id
                   AND candidate.state='active'
                 ORDER BY candidate.completed_at DESC, candidate.job_id DESC
                 LIMIT 1
             ) job ON TRUE
             WHERE route.holding_id=$1 AND route.state='active'
             ORDER BY route.company_id,route.branch_id",
        )
        .bind(&holding_key)
        .fetch_all(&control.pools().write)
        .await?;
        let mut seen_companies = BTreeSet::new();
        let mut seen_branches = BTreeSet::new();
        for (index, route) in routes.into_iter().enumerate() {
            let company_id: Uuid = route.try_get("company_id")?;
            let branch_id: Uuid = route.try_get("branch_id")?;
            let branch_code: String = route.try_get("branch_code")?;
            let branch_name: String = route.try_get("branch_name")?;
            if seen_companies.insert(company_id) && !authoritative_node_ids.contains(&company_id) {
                let fixture = branch_fixtures(&holding_key)
                    .iter()
                    .find(|fixture| branch_ids(&holding_key, **fixture).0 == company_id);
                let company_code = fixture.map_or_else(
                    || format!("COMPANY-{}", &company_id.simple().to_string()[..8]),
                    |fixture| fixture.company_key.to_owned(),
                );
                let company_name = fixture.map_or_else(
                    || "Development Company".to_owned(),
                    |fixture| fixture.company_name.to_owned(),
                );
                insert_typed_organization_read_fixture(
                    &mut read_transaction,
                    holding_uuid,
                    generation,
                    u64::try_from(index)?.saturating_mul(2).saturating_add(2),
                    company_id,
                    "company",
                    holding_id,
                    Some(company_id),
                    Some(holding_id),
                    company_code,
                    company_name,
                    now,
                )
                .await?;
            }
            if !authoritative_node_ids.contains(&branch_id) {
                insert_typed_organization_read_fixture(
                    &mut read_transaction,
                    holding_uuid,
                    generation,
                    u64::try_from(index)?.saturating_mul(2).saturating_add(3),
                    branch_id,
                    "branch",
                    holding_id,
                    Some(company_id),
                    Some(company_id),
                    branch_code,
                    branch_name,
                    now,
                )
                .await?;
            }
            seen_branches.insert(branch_id);
        }

        let assignment_payloads = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT assignment.payload
             FROM erpx_control.role_assignments assignment
             JOIN erpx_control.role_assignment_holding_targets target
               ON target.assignment_id=assignment.id
             WHERE target.holding_id=$1
               AND assignment.valid_from <= $2 AND assignment.valid_until > $2
             ORDER BY assignment.id",
        )
        .bind(&holding_key)
        .bind(now)
        .fetch_all(&control.pools().write)
        .await?;
        for (index, assignment) in assignment_payloads
            .into_iter()
            .map(serde_json::from_value::<RoleAssignment>)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .enumerate()
        {
            let (company_id, branch_id) = match assignment.scope {
                Scope::Company {
                    holding_id: authority_holding,
                    company_id,
                } if authority_holding == holding_id => (company_id, None),
                Scope::Branch {
                    holding_id: authority_holding,
                    company_id,
                    branch_id,
                } if authority_holding == holding_id => (company_id, Some(branch_id)),
                _ => continue,
            };
            let fixture = DEMO_BRANCHES
                .iter()
                .chain(RESTRICTED_BRANCHES.iter())
                .find(|fixture| {
                    let ids = branch_ids(&holding_key, **fixture);
                    ids.0 == company_id && branch_id.is_none_or(|branch_id| ids.1 == branch_id)
                });
            if seen_companies.insert(company_id) && !authoritative_node_ids.contains(&company_id) {
                insert_typed_organization_read_fixture(
                    &mut read_transaction,
                    holding_uuid,
                    generation,
                    u64::try_from(index)?
                        .saturating_mul(2)
                        .saturating_add(10_002),
                    company_id,
                    "company",
                    holding_id,
                    Some(company_id),
                    Some(holding_id),
                    fixture.map_or_else(
                        || format!("COMPANY-{}", &company_id.simple().to_string()[..8]),
                        |fixture| fixture.company_key.to_owned(),
                    ),
                    fixture.map_or_else(
                        || "Authorized Company".to_owned(),
                        |fixture| fixture.company_name.to_owned(),
                    ),
                    now,
                )
                .await?;
            }
            if let Some(branch_id) = branch_id
                && seen_branches.insert(branch_id)
                && !authoritative_node_ids.contains(&branch_id)
            {
                insert_typed_organization_read_fixture(
                    &mut read_transaction,
                    holding_uuid,
                    generation,
                    u64::try_from(index)?
                        .saturating_mul(2)
                        .saturating_add(10_003),
                    branch_id,
                    "branch",
                    holding_id,
                    Some(company_id),
                    Some(company_id),
                    fixture.map_or_else(
                        || format!("BRANCH-{}", &branch_id.simple().to_string()[..8]),
                        |fixture| fixture.branch_code.to_owned(),
                    ),
                    fixture.map_or_else(
                        || "Authorized Branch".to_owned(),
                        |fixture| fixture.branch_name.to_owned(),
                    ),
                    now,
                )
                .await?;
            }
        }
        read_transaction.commit().await?;
        read.close().await;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_typed_organization_read_fixture(
    read: &mut Transaction<'_, Postgres>,
    rls_holding_id: Uuid,
    generation: Uuid,
    source_global_sequence: u64,
    node_id: Uuid,
    node_kind: &str,
    holding_id: Uuid,
    company_id: Option<Uuid>,
    parent_node_id: Option<Uuid>,
    code: String,
    name: String,
    now: chrono::DateTime<Utc>,
) -> TaskResult {
    let (placement, scope, scope_key) = match node_kind {
        "holding" => (
            serde_json::json!({"kind":"holding"}),
            serde_json::json!({"Holding":{"holding_id":holding_id}}),
            format!("h:{holding_id}"),
        ),
        "company" => (
            serde_json::json!({"kind":"company","holding_id":holding_id}),
            serde_json::json!({"Company":{"holding_id":holding_id,"company_id":node_id}}),
            format!("h:{holding_id}/c:{node_id}"),
        ),
        "branch" => {
            let company_id =
                company_id.ok_or("typed Organization branch fixture has no company")?;
            (
                serde_json::json!({"kind":"branch","holding_id":holding_id,"company_id":company_id}),
                serde_json::json!({"Branch":{"holding_id":holding_id,"company_id":company_id,"branch_id":node_id}}),
                format!("h:{holding_id}/c:{company_id}/b:{node_id}"),
            )
        }
        _ => return Err("unsupported typed Organization development fixture kind".into()),
    };
    let state = serde_json::json!({
        "node_id":node_id,
        "version":1,
        "code":code.clone(),
        "name":name.clone(),
        "kind":node_kind,
        "placement":placement.clone(),
        "parent_id":parent_node_id,
        "scope":scope.clone(),
        "status":"active",
        "created_at":now,
        "updated_at":now,
    });
    sqlx::query(
        "INSERT INTO erpx_organization_read.node_projection
            (holding_id,projection_name,generation,partition_key,node_id,source_aggregate_version,
             source_global_sequence,node_kind,node_status,parent_node_id,holding_scope,
             scope_key,placement,node_code,display_name,state,aggregate_created_at,
             aggregate_updated_at,projected_at)
         VALUES ($1,'organization.nodes',$2,$3,$4,1,$5,$6,'active',$7,$8,$9,$10,$11,$12,$13,$14,$14,$14)
         ON CONFLICT (holding_id,projection_name,generation,partition_key,node_id) DO NOTHING",
    )
    .bind(rls_holding_id)
    .bind(generation)
    .bind(format!("organization.node:{}", node_id.simple()))
    .bind(node_id)
    .bind(i64::try_from(source_global_sequence)?)
    .bind(node_kind)
    .bind(parent_node_id)
    .bind(&scope)
    .bind(scope_key)
    .bind(&placement)
    .bind(code)
    .bind(name)
    .bind(state)
    .bind(now)
    .execute(&mut **read)
    .await?;
    Ok(())
}

async fn reconcile_development_command_entitlements(
    control: &SqlxControlPlane,
    now: chrono::DateTime<Utc>,
) -> TaskResult {
    let commercial = SqlxCommercialStore::new(control);
    let bindings = sqlx::query(
        "SELECT binding.holding_id, binding.customer_id, binding.canonical_holding_id
         FROM erpx_control.customer_holding_bindings binding
         JOIN erpx_control.holding_registry registry USING (holding_id)
         WHERE binding.binding_state='configured'
           AND registry.credential_key_ref IN
               ('local_shared','local-development-disabled','database-profile:default')
         ORDER BY binding.holding_id",
    )
    .fetch_all(&control.pools().write)
    .await?;
    for binding in bindings {
        let holding_id: String = binding.try_get("holding_id")?;
        let customer_id: Uuid = binding.try_get("customer_id")?;
        let canonical_holding_id: Uuid = binding.try_get("canonical_holding_id")?;
        let subscription_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("erpx:dev-access:{holding_id}:subscription:v1").as_bytes(),
        );
        let subscriptions = commercial.customer_subscription_ids(customer_id).await?;
        if subscriptions
            .iter()
            .any(|candidate| *candidate != subscription_id)
        {
            return Err(format!(
                "development customer {customer_id} has a subscription outside the deterministic fixture"
            )
            .into());
        }
        match commercial
            .get_billing_lifecycle(customer_id, subscription_id)
            .await?
        {
            Some((lifecycle, _)) if lifecycle.state == BillingState::Active => {}
            Some((_, version)) => {
                commercial
                    .upsert_billing_lifecycle(
                        customer_id,
                        subscription_id,
                        &BillingLifecycle::new(BillingState::Active, now),
                        Some(version),
                    )
                    .await?;
            }
            None => {
                commercial
                    .upsert_billing_lifecycle(
                        customer_id,
                        subscription_id,
                        &BillingLifecycle::new(BillingState::Active, now),
                        None,
                    )
                    .await?;
            }
        }

        let assignment_payloads = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload
             FROM erpx_control.role_assignments
             WHERE valid_from <= $1 AND valid_until > $1
             ORDER BY id",
        )
        .bind(now)
        .fetch_all(&control.pools().write)
        .await?;
        let assignments = assignment_payloads
            .into_iter()
            .map(serde_json::from_value::<RoleAssignment>)
            .collect::<Result<Vec<_>, _>>()?;
        reconcile_development_command_sod(control, &assignments, canonical_holding_id, now)
            .await
            .map_err(|error| {
                format!("development command SoD reconcile failed for {holding_id}: {error}")
            })?;
        let mut companies = BTreeMap::<Uuid, BTreeSet<Uuid>>::new();
        for assignment in &assignments {
            match &assignment.scope {
                Scope::Company {
                    holding_id,
                    company_id,
                } if *holding_id == canonical_holding_id => {
                    companies.entry(*company_id).or_default();
                }
                Scope::Branch {
                    holding_id,
                    company_id,
                    branch_id,
                } if *holding_id == canonical_holding_id => {
                    companies.entry(*company_id).or_default().insert(*branch_id);
                }
                Scope::Subtree {
                    holding_id,
                    organization_node_id,
                } if *holding_id == canonical_holding_id => {
                    companies.entry(*organization_node_id).or_default();
                }
                _ => {}
            }
        }
        if companies.is_empty() {
            return Err(format!(
                "development holding {holding_id} has no company or branch assignment for its root holding"
            )
            .into());
        }
        let company_count = u32::try_from(companies.len())?;
        let branch_capacity = u32::try_from(
            companies
                .values()
                .map(|branches| branches.len().max(1))
                .sum::<usize>(),
        )?;
        for module_key in ["assets", "maintenance", "identity_access"] {
            let mut expected = EntitlementAllocationBook::new(
                module_key,
                EntitlementCapacity {
                    seats: company_count.saturating_mul(50),
                    companies: company_count,
                    branches: branch_capacity,
                },
                None,
            );
            expected.set_active(true, None);
            for (company_id, branch_ids) in &companies {
                expected.allocate_company(
                    *company_id,
                    50,
                    u32::try_from(branch_ids.len().max(1))?,
                )?;
                expected.activate_company(*company_id)?;
                for branch_id in branch_ids {
                    expected.activate_branch(*company_id, *branch_id)?;
                }
            }
            match commercial
                .get_entitlement_book(customer_id, module_key)
                .await
                .map_err(|error| {
                    format!(
                        "development entitlement read failed for {holding_id}/{module_key}: {error}"
                    )
                })? {
                Some((book, _)) if book == expected => {}
                Some((_, version)) => {
                    commercial
                        .upsert_entitlement_book(customer_id, &expected, Some(version))
                        .await
                        .map_err(|error| {
                            format!(
                                "development entitlement update failed for {holding_id}/{module_key}: {error}"
                            )
                        })?;
                }
                None => {
                    commercial
                        .upsert_entitlement_book(customer_id, &expected, None)
                        .await
                        .map_err(|error| {
                            format!(
                                "development entitlement create failed for {holding_id}/{module_key}: {error}"
                            )
                        })?;
                }
            }
        }
    }
    Ok(())
}

fn scope_root_holding(scope: &Scope) -> Option<Uuid> {
    match scope {
        Scope::Holding { holding_id }
        | Scope::Company { holding_id, .. }
        | Scope::Branch { holding_id, .. }
        | Scope::Subtree { holding_id, .. } => Some(*holding_id),
        Scope::Platform => None,
    }
}

fn writable_development_command_modules(assignment: &RoleAssignment) -> Vec<&'static str> {
    let mut modules = ["assets", "maintenance"]
        .into_iter()
        .filter(|module| {
            let permission = format!("{module}.write");
            assignment
                .permissions
                .iter()
                .any(|candidate| candidate == &permission)
        })
        .collect::<Vec<_>>();
    if assignment.permissions.iter().any(|permission| {
        matches!(
            permission.as_str(),
            "identity_access.write"
                | "security.approve"
                | "security.approval"
                | "directory.activation"
        )
    }) {
        modules.push("identity_access");
    }
    modules
}

async fn reconcile_development_command_sod(
    control: &SqlxControlPlane,
    assignments: &[RoleAssignment],
    canonical_holding_id: Uuid,
    now: chrono::DateTime<Utc>,
) -> TaskResult {
    for assignment in assignments
        .iter()
        .filter(|assignment| scope_root_holding(&assignment.scope) == Some(canonical_holding_id))
    {
        for module_key in writable_development_command_modules(assignment) {
            let scope = serde_json::to_value(&assignment.scope)?;
            let existing = sqlx::query(
                "SELECT passed, valid_from, valid_until, decision_version
                 FROM erpx_control.sod_decisions
                 WHERE principal_ref=$1 AND module_key=$2 AND scope=$3",
            )
            .bind(&assignment.principal_ref)
            .bind(module_key)
            .bind(&scope)
            .fetch_optional(&control.pools().write)
            .await
            .map_err(|error| {
                format!(
                    "development SoD lookup failed for role {}/{module_key}: {error}",
                    assignment.role_key
                )
            })?;
            if existing.as_ref().is_some_and(|row| {
                row.try_get::<bool, _>("passed").unwrap_or(false)
                    && row
                        .try_get::<chrono::DateTime<Utc>, _>("valid_from")
                        .is_ok_and(|valid_from| valid_from <= now)
                    && row
                        .try_get::<chrono::DateTime<Utc>, _>("valid_until")
                        .is_ok_and(|valid_until| now < valid_until)
            }) {
                continue;
            }
            let decision_version = existing
                .as_ref()
                .map(|row| row.try_get::<i64, _>("decision_version"))
                .transpose()?
                .map_or(Ok(1_u64), |version| {
                    u64::try_from(version)?.checked_add(1).ok_or_else(|| {
                        Box::<dyn std::error::Error + Send + Sync>::from(
                            "development SoD decision version overflow",
                        )
                    })
                })?;
            control
                .record_sod_decision(&SodDecision {
                    principal_ref: assignment.principal_ref.clone(),
                    module_key: module_key.into(),
                    scope: assignment.scope.clone(),
                    passed: true,
                    reviewer_ref: DEVELOPMENT_CONTROLLER_PRINCIPAL.into(),
                    reason: "local development command authorization fixture".into(),
                    valid_from: now,
                    valid_until: now + chrono::Duration::days(365),
                    decision_version,
                })
                .await
                .map_err(|error| {
                    format!(
                        "development SoD write failed for role {}/{module_key}: {error}",
                        assignment.role_key
                    )
                })?;
        }
    }
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
         WHERE holding_id IN ($1,$2)
           AND state IN ('provisioning','verifying')
           AND claimed_by IS NULL",
    )
    .bind(HOLDING_ID)
    .bind(RESTRICTED_HOLDING_ID)
    .execute(&control.pools().write)
    .await?;
    sqlx::query(
        "UPDATE erpx_control.branch_database_routes
         SET state='failed', failure_category='development_reconcile_recovery',
             activated_at=NULL, suspended_at=NULL, updated_at=now()
         WHERE holding_id IN ($1,$2) AND state IN ('provisioning','verifying')",
    )
    .bind(HOLDING_ID)
    .bind(RESTRICTED_HOLDING_ID)
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
        HOLDING_ID,
        BRANCH_PROFILE_KEY,
        DEVELOPMENT_CONTROLLER_PRINCIPAL,
    )
    .await?;
    jobs.bind_profile(
        RESTRICTED_HOLDING_ID,
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
    for (holding_key, data_profile_key) in [
        (HOLDING_ID, DATA_PROFILE_KEY),
        (RESTRICTED_HOLDING_ID, RESTRICTED_DATA_PROFILE_KEY),
    ] {
        let holding_id = holding_holding_scope_id(data_profile_key);
        let units = if holding_key == RESTRICTED_HOLDING_ID {
            RESTRICTED_UNITS
                .iter()
                .map(|unit| BusinessUnitId::parse(unit).map_err(|_| "invalid fixture unit"))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            BusinessUnitId::ALL.to_vec()
        };
        for fixture in branch_fixtures(holding_key) {
            let (company_id, branch_id) = branch_ids(holding_key, *fixture);
            let (write_database_key, read_database_key) =
                branch_database_keys("devbranch", &holding_id, branch_id)?;
            branch_databases.push(write_database_key);
            branch_databases.push(read_database_key);
            let request = ProvisionBranchRequest {
                holding_key: holding_key.into(),
                data_profile_key: data_profile_key.into(),
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
                    "{FIXTURE_VERSION}:branch:{holding_id}:{}",
                    fixture.branch_key
                ),
                requested_by: DEVELOPMENT_CONTROLLER_PRINCIPAL.into(),
                correlation_id: Uuid::new_v5(
                    &Uuid::NAMESPACE_URL,
                    format!(
                        "erpx:{FIXTURE_VERSION}:branch-correlation:{holding_id}:{}",
                        fixture.branch_key
                    )
                    .as_bytes(),
                ),
            };
            let job = jobs.enqueue(&request).await?;
            if let Some(route) = jobs
                .route(holding_key, branch_id)
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
                         WHERE holding_id=$1 AND job_id=$2
                           AND state IN ('queued','failed')",
                    )
                    .bind(holding_key)
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
                    &route.holding_key,
                    route.branch_id,
                    &units,
                )
                .await?;
                continue;
            }
            if job.state == BranchProvisioningState::Failed {
                jobs.retry(holding_key, job.job_id, request.correlation_id, Utc::now())
                    .await?;
            }
        }
    }

    let provisioner = HoldingDatabaseProvisioner::for_approved_profiles(control.clone());
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
    Ok(())
}

async fn reconcile_active_branch_business_units(
    control: &SqlxControlPlane,
    holding_id: &str,
    branch_id: Uuid,
    units: &[BusinessUnitId],
) -> TaskResult {
    for unit in units {
        sqlx::query(
            "INSERT INTO erpx_control.branch_business_unit_configurations
                (holding_id, branch_id, business_unit, active,
                 configuration_version, updated_by)
             VALUES ($1,$2,$3,TRUE,1,$4)
             ON CONFLICT (holding_id, branch_id, business_unit) DO UPDATE
             SET active=TRUE,
                 configuration_version=CASE
                     WHEN erpx_control.branch_business_unit_configurations.active THEN
                         erpx_control.branch_business_unit_configurations.configuration_version
                     ELSE erpx_control.branch_business_unit_configurations.configuration_version + 1
                 END,
                 updated_by=EXCLUDED.updated_by,
                 updated_at=now()",
        )
        .bind(holding_id)
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
            "GRANT USAGE, CREATE ON SCHEMA erpx_holding TO {target}"
        ))
        .execute(&pool)
        .await?;
        sqlx::query(&format!(
            "GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER ON ALL TABLES IN SCHEMA erpx_holding TO {target}"
        ))
        .execute(&pool)
        .await?;
        sqlx::query(&format!(
            "GRANT USAGE, SELECT, UPDATE ON ALL SEQUENCES IN SCHEMA erpx_holding TO {target}"
        ))
        .execute(&pool)
        .await?;
        sqlx::query(&format!(
            "ALTER DEFAULT PRIVILEGES FOR ROLE {source} IN SCHEMA erpx_holding GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER ON TABLES TO {target}"
        ))
        .execute(&pool)
        .await?;
        sqlx::query(&format!(
            "ALTER DEFAULT PRIVILEGES FOR ROLE {source} IN SCHEMA erpx_holding GRANT USAGE, SELECT, UPDATE ON SEQUENCES TO {target}"
        ))
            .execute(&pool)
            .await?;
        pool.close().await;
    }
    Ok(())
}

pub(crate) async fn controller_fixture_current(
    root: &Path,
    config: &DevConfig,
    control_write_database: &str,
    target_holding: &str,
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
            .map(|entry| controller_catalog_assignment_id(target_holding, &entry.role_key)),
    );
    expected_assignment_ids.extend(
        ["assets_administrator", "maintenance_administrator"]
            .into_iter()
            .map(|role_key| controller_catalog_holding_assignment_id(target_holding, role_key)),
    );
    let controller_role_count: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM erpx_control.role_assignments assignment
         JOIN erpx_control.role_assignment_target_policies policy
           ON policy.assignment_id=assignment.id
         JOIN erpx_control.role_assignment_holding_targets target
           ON target.assignment_id=assignment.id
         WHERE assignment.id = ANY($1)
           AND assignment.principal_ref=$2
           AND assignment.valid_until>now()
           AND policy.policy_kind='holding_allowlist'
           AND policy.environment_gate='local-ucs-lab'
           AND target.holding_id=$3
           AND (SELECT count(*)
                FROM erpx_control.role_assignment_holding_targets exact_target
                WHERE exact_target.assignment_id=assignment.id)=1",
    )
    .bind(&expected_assignment_ids)
    .bind(DEVELOPMENT_CONTROLLER_PRINCIPAL)
    .bind(target_holding)
    .fetch_one(&write)
    .await?;
    write.close().await;
    Ok(controller_role_count == i64::try_from(expected_assignment_ids.len())?)
}

pub(crate) async fn runtime_catalog_current(
    root: &Path,
    config: &DevConfig,
    control_write_database: &str,
    target_holding: &str,
) -> TaskResult<bool> {
    if !controller_fixture_current(root, config, control_write_database, target_holding).await? {
        return Ok(false);
    }

    let write = super::pool(&config.database_url(control_write_database)).await?;
    let expected_platform_personas = platform_personas();
    let platform_receipt_current: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM erpx_identity.development_seed_receipts
             WHERE receipt_key='persona-manifest:platform'
               AND fixture_version=$1 AND holding_id IS NULL AND manifest_digest=$2
         )",
    )
    .bind(FIXTURE_VERSION)
    .bind(platform_manifest_digest())
    .fetch_one(&write)
    .await?;
    let platform_manifest_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_identity.development_persona_manifests
         WHERE holding_id IS NULL",
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

    let runtime_workspace = sqlx::query(
        "SELECT registry.holding_id, registry.data_profile_key
         FROM erpx_control.holding_registry registry
         JOIN erpx_control.customer_holding_bindings binding USING (holding_id)
         WHERE registry.holding_id=$1 AND registry.state='active'
           AND binding.binding_state='configured'",
    )
    .bind(target_holding)
    .fetch_optional(&write)
    .await?;
    let Some(runtime_workspace) = runtime_workspace else {
        write.close().await;
        return Ok(false);
    };
    let holding_id: String = runtime_workspace.try_get("holding_id")?;
    let data_profile_key: String = runtime_workspace.try_get("data_profile_key")?;
    for (holding_id, data_profile_key) in [(holding_id, data_profile_key)] {
        let expected_personas = scoped_demo_personas(
            &holding_id,
            &data_profile_key,
            erpx_identity::BUSINESS_UNITS.as_slice(),
        );
        let receipt_current: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM erpx_identity.development_seed_receipts
                 WHERE receipt_key=$1 AND fixture_version=$2
                   AND holding_id=$3 AND manifest_digest=$4
             )",
        )
        .bind(format!("persona-manifest:{holding_id}"))
        .bind(FIXTURE_VERSION)
        .bind(&holding_id)
        .bind(manifest_digest(&holding_id))
        .fetch_one(&write)
        .await?;
        let manifest_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_identity.development_persona_manifests
             WHERE holding_id=$1",
        )
        .bind(&holding_id)
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
                persona_assignment_scopes(&holding_id, &data_profile_key, &persona.role_key)
                    .into_iter()
                    .map(|(_, scope_key)| {
                        Uuid::new_v5(
                            &Uuid::NAMESPACE_URL,
                            format!(
                                "erpx:dev-access:{holding_id}:{}:{scope_key}",
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

    if !development_command_entitlements_current(&write).await? {
        write.close().await;
        return Ok(false);
    }
    write.close().await;
    Ok(true)
}

async fn development_command_entitlements_current(write: &PgPool) -> TaskResult<bool> {
    let now = Utc::now();
    let canonical_holding_ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT binding.canonical_holding_id
         FROM erpx_control.customer_holding_bindings binding
         JOIN erpx_control.holding_registry registry USING (holding_id)
         WHERE binding.binding_state='configured'
           AND registry.credential_key_ref IN
               ('local_shared','local-development-disabled','database-profile:default')
         ORDER BY binding.canonical_holding_id",
    )
    .fetch_all(write)
    .await?;
    let customer_count: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM erpx_control.customer_holding_bindings binding
         JOIN erpx_control.holding_registry registry USING (holding_id)
         WHERE binding.binding_state='configured'
           AND registry.credential_key_ref IN
               ('local_shared','local-development-disabled','database-profile:default')",
    )
    .fetch_one(write)
    .await?;
    if customer_count == 0 {
        return Ok(false);
    }
    let billing_count: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM erpx_control.billing_lifecycles lifecycle
         JOIN erpx_control.customer_holding_bindings binding
           ON binding.customer_id=lifecycle.customer_id
         JOIN erpx_control.holding_registry registry USING (holding_id)
         WHERE binding.binding_state='configured'
           AND registry.credential_key_ref IN
               ('local_shared','local-development-disabled','database-profile:default')",
    )
    .fetch_one(write)
    .await?;
    let active_billing_count: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM erpx_control.billing_lifecycles lifecycle
         JOIN erpx_control.customer_holding_bindings binding
           ON binding.customer_id=lifecycle.customer_id
         JOIN erpx_control.holding_registry registry USING (holding_id)
         WHERE registry.credential_key_ref IN
               ('local_shared','local-development-disabled','database-profile:default')
           AND binding.binding_state='configured'
           AND lifecycle.state='active'",
    )
    .fetch_one(write)
    .await?;
    let active_entitlement_count: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM erpx_control.entitlement_allocation_books entitlement
         JOIN erpx_control.customer_holding_bindings binding
           ON binding.customer_id=entitlement.customer_id
         JOIN erpx_control.holding_registry registry USING (holding_id)
         WHERE registry.credential_key_ref IN
               ('local_shared','local-development-disabled','database-profile:default')
           AND binding.binding_state='configured'
           AND entitlement.module_key IN ('assets','maintenance','identity_access')
           AND entitlement.active",
    )
    .fetch_one(write)
    .await?;
    let assignment_payloads = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT payload FROM erpx_control.role_assignments
         WHERE valid_from <= $1 AND valid_until > $1 ORDER BY id",
    )
    .bind(now)
    .fetch_all(write)
    .await?;
    let assignments = assignment_payloads
        .into_iter()
        .map(serde_json::from_value::<RoleAssignment>)
        .collect::<Result<Vec<_>, _>>()?;
    let mut required_sod_decisions = 0_usize;
    for assignment in assignments.iter().filter(|assignment| {
        scope_root_holding(&assignment.scope)
            .is_some_and(|holding_id| canonical_holding_ids.contains(&holding_id))
    }) {
        for module_key in writable_development_command_modules(assignment) {
            required_sod_decisions = required_sod_decisions.saturating_add(1);
            let active: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                     SELECT 1 FROM erpx_control.sod_decisions
                     WHERE principal_ref=$1 AND module_key=$2 AND scope=$3
                       AND passed AND valid_from <= $4 AND $4 < valid_until
                 )",
            )
            .bind(&assignment.principal_ref)
            .bind(module_key)
            .bind(serde_json::to_value(&assignment.scope)?)
            .bind(now)
            .fetch_one(write)
            .await?;
            if !active {
                return Ok(false);
            }
        }
    }
    Ok(required_sod_decisions > 0
        && billing_count == customer_count
        && active_billing_count == customer_count
        && active_entitlement_count == customer_count.saturating_mul(3))
}

pub(crate) async fn fixtures_current(root: &Path, config: &DevConfig) -> TaskResult<bool> {
    if !controller_fixture_current(
        root,
        config,
        DATABASES[0],
        DEVELOPMENT_CONTROLLER_TARGET_HOLDING,
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
               AND fixture_version=$1 AND holding_id IS NULL AND manifest_digest=$2
         )",
    )
    .bind(FIXTURE_VERSION)
    .bind(platform_manifest_digest())
    .fetch_one(&write)
    .await?;
    let expected_platform_personas = platform_personas();
    let platform_manifest_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_identity.development_persona_manifests
         WHERE holding_id IS NULL",
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

    let local_holdings = sqlx::query_scalar::<_, String>(
        "SELECT holding_id FROM erpx_control.holding_registry
         WHERE state='active'
           AND holding_id IN ($1,$2)
           AND credential_key_ref='local_shared'
         ORDER BY holding_id",
    )
    .bind(HOLDING_ID)
    .bind(RESTRICTED_HOLDING_ID)
    .fetch_all(&write)
    .await?;
    if !local_holdings
        .iter()
        .any(|holding_id| holding_id == HOLDING_ID)
        || !local_holdings
            .iter()
            .any(|holding_id| holding_id == RESTRICTED_HOLDING_ID)
    {
        write.close().await;
        return Ok(false);
    }
    for holding_id in &local_holdings {
        let data_profile_key = if holding_id == HOLDING_ID {
            DATA_PROFILE_KEY
        } else {
            RESTRICTED_DATA_PROFILE_KEY
        };
        let expected_personas =
            scoped_demo_personas(holding_id, data_profile_key, entitled_units(holding_id));
        let receipt_current: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM erpx_identity.development_seed_receipts
                 WHERE receipt_key=$1 AND fixture_version=$2
                   AND holding_id=$3 AND manifest_digest=$4
             )",
        )
        .bind(format!("persona-manifest:{holding_id}"))
        .bind(FIXTURE_VERSION)
        .bind(holding_id)
        .bind(manifest_digest(holding_id))
        .fetch_one(&write)
        .await?;
        let manifest_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_identity.development_persona_manifests
             WHERE holding_id=$1",
        )
        .bind(holding_id)
        .fetch_one(&write)
        .await?;
        let mut required_assignment_ids = match holding_id.as_str() {
            HOLDING_ID | RESTRICTED_HOLDING_ID => {
                let data_profile_key = if holding_id == HOLDING_ID {
                    DATA_PROFILE_KEY
                } else {
                    RESTRICTED_DATA_PROFILE_KEY
                };
                expected_personas
                    .iter()
                    .flat_map(|persona| {
                        persona_assignment_scopes(holding_id, data_profile_key, &persona.role_key)
                            .into_iter()
                            .map(|(_, scope_key)| {
                                Uuid::new_v5(
                                    &Uuid::NAMESPACE_URL,
                                    format!(
                                        "erpx:dev-access:{holding_id}:{}:{scope_key}",
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
         WHERE holding_id IN ($1,$2) AND state='active'",
    )
    .bind(HOLDING_ID)
    .bind(RESTRICTED_HOLDING_ID)
    .fetch_one(&write)
    .await?;
    let configured_branch_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (
             SELECT route.holding_id, route.branch_id
             FROM erpx_control.branch_database_routes route
             JOIN erpx_control.branch_business_unit_configurations configuration
               ON configuration.holding_id=route.holding_id
              AND configuration.branch_id=route.branch_id
             WHERE route.holding_id IN ($1,$2)
               AND route.state='active'
               AND configuration.active
               AND configuration.business_unit = ANY($3)
             GROUP BY route.holding_id, route.branch_id
             HAVING count(DISTINCT configuration.business_unit)=$4
         ) configured_branches",
    )
    .bind(HOLDING_ID)
    .bind(RESTRICTED_HOLDING_ID)
    .bind(erpx_identity::BUSINESS_UNITS.as_slice())
    .bind(i64::try_from(erpx_identity::BUSINESS_UNITS.len())?)
    .fetch_one(&write)
    .await?;
    let entitlements_current = development_command_entitlements_current(&write).await?;
    write.close().await;
    let expected_branch_count = i64::try_from(DEMO_BRANCHES.len() + RESTRICTED_BRANCHES.len())?;
    Ok(active_branch_count == expected_branch_count
        && configured_branch_count == expected_branch_count
        && entitlements_current)
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
         JOIN erpx_control.role_assignment_holding_targets target
           ON target.assignment_id=assignment.id
         WHERE assignment.principal_ref=$1
           AND assignment.valid_until>now()
           AND policy.policy_kind='holding_allowlist'
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

pub(crate) fn credentials(root: &Path, holding: Option<&str>) -> TaskResult {
    if let Some(holding) = holding
        && (holding.is_empty()
            || holding.len() > 128
            || !holding
                .chars()
                .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.')))
    {
        return Err("holding credential profile contains an invalid identifier".into());
    }
    let path = holding.map_or_else(
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
    match holding {
        Some(holding) => {
            println!("UCS authority credentials:");
            println!("  authority: {holding}");
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
    println!("Development access secret reset; {revoked} controller session(s) revoked.");
    Ok(())
}

fn entitled_units(holding_id: &str) -> &'static [&'static str] {
    if holding_id == RESTRICTED_HOLDING_ID {
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

fn manifest_digest(holding_id: &str) -> String {
    let mut digest = Sha256::new();
    for value in [
        FIXTURE_VERSION,
        DEVELOPMENT_CONTROLLER_PRINCIPAL,
        BRANCH_PROFILE_KEY,
        BRANCH_CREDENTIAL_PROFILE_KEY,
        "devbranch",
        "erpx_managed",
        "erpx-ucs-authority",
        holding_id,
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
                holding_id,
                if holding_id == HOLDING_ID {
                    DATA_PROFILE_KEY
                } else {
                    RESTRICTED_DATA_PROFILE_KEY
                },
                entitled_units(holding_id),
            )
        ),
    );
    for fixture in branch_fixtures(holding_id) {
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
    authenticated_realm: &'static str,
    at: chrono::DateTime<Utc>,
) -> TaskResult {
    sqlx::query(
        "INSERT INTO erpx_identity.principals
            (id, principal_ref, status, authenticated_realm, principal_kind,
             created_at, updated_at)
         VALUES ($1,$2,'active',$3,'human',$4,$4)
         ON CONFLICT (principal_ref) DO UPDATE
             SET status='active', updated_at=EXCLUDED.updated_at",
    )
    .bind(Uuid::new_v4())
    .bind(principal_ref)
    .bind(authenticated_realm)
    .bind(at)
    .execute(pool)
    .await?;
    let stored: (String, String) = sqlx::query_as(
        "SELECT authenticated_realm, principal_kind
         FROM erpx_identity.principals WHERE principal_ref=$1",
    )
    .bind(principal_ref)
    .fetch_one(pool)
    .await?;
    if stored.0 != authenticated_realm || stored.1 != "human" {
        return Err(format!(
            "development principal {principal_ref} authenticated authority is contradictory"
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_fixture_digests_cover_authority_and_holding_inputs() {
        let platform = platform_manifest_digest();
        let demo = manifest_digest(HOLDING_ID);
        let restricted = manifest_digest(RESTRICTED_HOLDING_ID);
        assert_eq!(platform.len(), 64);
        assert_eq!(demo.len(), 64);
        assert_eq!(restricted.len(), 64);
        assert_ne!(platform, demo);
        assert_ne!(demo, restricted);
    }

    #[test]
    fn runtime_catalog_contains_platform_and_both_demo_holdings() {
        let platform = platform_personas();
        let demo = scoped_demo_personas(HOLDING_ID, DATA_PROFILE_KEY, entitled_units(HOLDING_ID));
        let restricted = scoped_demo_personas(
            RESTRICTED_HOLDING_ID,
            RESTRICTED_DATA_PROFILE_KEY,
            entitled_units(RESTRICTED_HOLDING_ID),
        );
        assert_eq!(platform.len() + demo.len() + restricted.len(), 146);
        assert!(platform.iter().all(|persona| persona.holding_id.is_none()));
        let demo_denied = demo.iter().filter(|persona| !persona.entitled).count();
        let restricted_denied = restricted
            .iter()
            .filter(|persona| !persona.entitled)
            .count();
        assert!(demo_denied > 0);
        assert_eq!(restricted_denied, demo_denied);
        assert_eq!(entitled_units(RESTRICTED_HOLDING_ID).len(), 12);
    }

    #[test]
    fn development_command_authorization_covers_identity_access_and_business_modules() {
        let assignments = controller_catalog_assignments(
            RESTRICTED_HOLDING_ID,
            RESTRICTED_DATA_PROFILE_KEY,
            branch_ids(RESTRICTED_HOLDING_ID, RESTRICTED_BRANCHES[0]),
            Utc::now(),
        );
        let security_head = assignments
            .iter()
            .find(|assignment| assignment.role_key == "customer_security_head")
            .expect("customer security head assignment");
        assert_eq!(
            writable_development_command_modules(security_head),
            ["identity_access"]
        );

        let assets_operator = assignments
            .iter()
            .find(|assignment| assignment.role_key == "assets_operator")
            .expect("assets operator assignment");
        assert_eq!(
            writable_development_command_modules(assets_operator),
            ["assets"]
        );
    }

    #[test]
    fn organization_fixture_insert_uses_the_current_projection_scope_column() {
        let source = include_str!("dev_access.rs");
        let insert = source
            .split_once("INSERT INTO erpx_organization_read.node_projection")
            .expect("organization fixture projection insert")
            .1
            .split_once("ON CONFLICT")
            .expect("organization fixture projection upsert")
            .0;
        assert!(insert.contains("holding_scope"));
        assert!(!insert.contains("active_scope"));
    }

    #[test]
    fn controller_catalog_has_every_role_with_bounded_representative_scopes() {
        let assignments = controller_catalog_assignments(
            RESTRICTED_HOLDING_ID,
            RESTRICTED_DATA_PROFILE_KEY,
            branch_ids(RESTRICTED_HOLDING_ID, RESTRICTED_BRANCHES[0]),
            Utc::now(),
        );
        assert_eq!(assignments.len(), 75);

        let ids = assignments
            .iter()
            .map(|assignment| assignment.id)
            .collect::<std::collections::BTreeSet<_>>();
        let role_keys = assignments
            .iter()
            .map(|assignment| assignment.role_key.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(), 75);
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
            branch_ids(RESTRICTED_HOLDING_ID, RESTRICTED_BRANCHES[0]);
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
        assert_eq!(holding_scopes, 5);
        assert_eq!(company_scopes, 1);
        assert_eq!(branch_scopes, 61);
        for role_key in ["assets_administrator", "maintenance_administrator"] {
            let scopes = assignments
                .iter()
                .filter(|assignment| assignment.role_key == role_key)
                .map(|assignment| &assignment.scope)
                .collect::<Vec<_>>();
            assert_eq!(scopes.len(), 2);
            assert!(
                scopes
                    .iter()
                    .any(|scope| matches!(scope, Scope::Holding { .. }))
            );
            assert!(
                scopes
                    .iter()
                    .any(|scope| matches!(scope, Scope::Branch { .. }))
            );
        }
    }
}
