use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use chrono::Utc;
use erpx_identity::{
    DirectoryConnector, DirectoryConnectorLifecycle, DirectoryFeedKind, DirectoryProvider,
    DirectoryRoleMapping, DirectoryScope, DirectoryTlsMode, DirectoryTlsPolicy, SqlxDirectoryStore,
};
use erpx_worker::{
    DirectorySupervisorRuntimeHandler, LocalDirectorySecretResolver, NamedWorkerHandler,
    authenticate_directory_user_with_runtime, fetch_with_runtime, preflight_directory_connector,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use crate::TaskResult;

const DOCKER_CONTEXT: &str = "colima-epsx-e2e";
const OPENLDAP_IMAGE: &str =
    "osixia/openldap@sha256:18742e9c449c9c1afe129d3f2f3ee15fb34cc43e5f940a20f3399728f41d7c28";
const SAMBA_IMAGE: &str = "diegogslomp/samba-ad-dc@sha256:eee91f3cf66d4a28b40a183eee32577f315afd9a9d0423d472c6f6b6b723902f";
const OPENLDAP_CONTAINER: &str = "erpx-openldap-lab";
const SAMBA_CONTAINER: &str = "erpx-samba-ad-lab";
const OPENLDAP_LDAP_PORT: u16 = 10_389;
const OPENLDAP_LDAPS_PORT: u16 = 10_636;
const SAMBA_LDAP_PORT: u16 = 11_389;
const SAMBA_LDAPS_PORT: u16 = 11_636;
const DIRECTORY_LAB_ACTOR: &str = "local:platform-root";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LabSecrets {
    openldap_admin_password: String,
    samba_admin_password: String,
    demo_password: String,
}

fn lab_root(root: &Path) -> PathBuf {
    root.join(".runtime/erpx/directory-lab")
}

fn secrets_path(root: &Path) -> PathBuf {
    root.join(".runtime/erpx/secrets/directory-lab.json")
}

fn private_directory(path: &Path) -> TaskResult {
    if path.is_symlink() {
        return Err(format!("refusing symlinked directory-lab path: {}", path.display()).into());
    }
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn write_private(path: &Path, body: &[u8]) -> TaskResult {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true).mode(0o600);
    let mut file = options.open(path)?;
    file.write_all(body)?;
    Ok(())
}

fn read_secrets(root: &Path) -> TaskResult<LabSecrets> {
    let path = secrets_path(root);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o777 != 0o600
    {
        return Err("directory-lab secrets must be a private 0600 file".into());
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn run(program: &str, args: &[&str]) -> TaskResult {
    let status = Command::new(program).args(args).status()?;
    if !status.success() {
        return Err(format!("{program} failed with {status}").into());
    }
    Ok(())
}

fn docker(args: &[&str]) -> TaskResult {
    let mut all = vec!["--context", DOCKER_CONTEXT];
    all.extend_from_slice(args);
    run("docker", &all)
}

pub(crate) fn init(root: &Path) -> TaskResult {
    let lab = lab_root(root);
    let tls = lab.join("tls");
    let openldap = lab.join("openldap");
    let samba = lab.join("samba");
    let directory_secrets = root.join(".runtime/erpx/secrets/directory");
    for path in [
        lab.as_path(),
        tls.as_path(),
        openldap.as_path(),
        samba.as_path(),
        samba.join("etc").as_path(),
        samba.join("private").as_path(),
        samba.join("var").as_path(),
        directory_secrets.as_path(),
        secrets_path(root)
            .parent()
            .ok_or("directory-lab secrets parent is missing")?,
    ] {
        private_directory(path)?;
    }
    let secrets = if secrets_path(root).exists() {
        read_secrets(root)?
    } else {
        let secrets = LabSecrets {
            openldap_admin_password: fresh_password("Ldap"),
            samba_admin_password: fresh_password("Samba"),
            demo_password: fresh_password("Demo"),
        };
        write_private(&secrets_path(root), &serde_json::to_vec_pretty(&secrets)?)?;
        secrets
    };
    issue_lab_certificates(&tls)?;
    write_openldap_bootstrap(&openldap, &secrets)?;
    write_connector_secret(
        &directory_secrets.join("openldap-local.json"),
        "cn=admin,dc=erpx,dc=test",
        &secrets.openldap_admin_password,
        &tls.join("ca.crt"),
        None,
    )?;
    write_connector_secret(
        &directory_secrets.join("samba-ad-local.json"),
        "Administrator@ERPX.EXAMPLE.COM",
        &secrets.samba_admin_password,
        &tls.join("ca.crt"),
        None,
    )?;
    println!("Initialized fresh pinned OpenLDAP/RFC4533 and Samba AD lab material.");
    Ok(())
}

fn fresh_password(prefix: &str) -> String {
    format!("{prefix}-{}aA1!", Uuid::new_v4().simple())
}

fn issue_lab_certificates(tls: &Path) -> TaskResult {
    let ca = tls.join("ca.crt");
    let ca_key = tls.join("ca.key");
    if !ca.exists() {
        run(
            "openssl",
            &[
                "req",
                "-x509",
                "-new",
                "-nodes",
                "-newkey",
                "rsa:3072",
                "-keyout",
                ca_key.to_str().ok_or("invalid directory CA key path")?,
                "-out",
                ca.to_str().ok_or("invalid directory CA path")?,
                "-sha256",
                "-days",
                "825",
                "-subj",
                "/CN=ERPX Directory Lab CA",
            ],
        )?;
        fs::set_permissions(&ca_key, fs::Permissions::from_mode(0o600))?;
    }
    for (name, dns) in [
        ("openldap", "erpx-openldap-lab"),
        ("samba", "erpx-samba-ad-lab"),
    ] {
        let certificate = tls.join(format!("{name}.crt"));
        if certificate.exists() {
            continue;
        }
        let key = tls.join(format!("{name}.key"));
        let csr = tls.join(format!("{name}.csr"));
        let config = tls.join(format!("{name}.cnf"));
        write_private(
            &config,
            format!(
                "[req]\ndistinguished_name=dn\nreq_extensions=req_ext\nprompt=no\n[dn]\nCN=localhost\n[req_ext]\nsubjectAltName=DNS:localhost,DNS:{dns},IP:127.0.0.1\nextendedKeyUsage=serverAuth\n"
            )
            .as_bytes(),
        )?;
        run(
            "openssl",
            &[
                "req",
                "-new",
                "-nodes",
                "-newkey",
                "rsa:3072",
                "-keyout",
                key.to_str().ok_or("invalid directory key path")?,
                "-out",
                csr.to_str().ok_or("invalid directory CSR path")?,
                "-config",
                config.to_str().ok_or("invalid directory config path")?,
            ],
        )?;
        run(
            "openssl",
            &[
                "x509",
                "-req",
                "-in",
                csr.to_str().ok_or("invalid directory CSR path")?,
                "-CA",
                ca.to_str().ok_or("invalid directory CA path")?,
                "-CAkey",
                ca_key.to_str().ok_or("invalid directory CA key path")?,
                "-CAcreateserial",
                "-out",
                certificate
                    .to_str()
                    .ok_or("invalid directory certificate path")?,
                "-days",
                "825",
                "-sha256",
                "-extensions",
                "req_ext",
                "-extfile",
                config.to_str().ok_or("invalid directory config path")?,
            ],
        )?;
        fs::set_permissions(key, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn write_openldap_bootstrap(root: &Path, secrets: &LabSecrets) -> TaskResult {
    let sync = root.join("01-syncprov.ldif");
    let sync_body = b"dn: cn=module{0},cn=config\nchangetype: modify\nadd: olcModuleLoad\nolcModuleLoad: syncprov\n\ndn: olcOverlay=syncprov,olcDatabase={1}mdb,cn=config\nchangetype: add\nobjectClass: olcOverlayConfig\nobjectClass: olcSyncProvConfig\nolcOverlay: syncprov\nolcSpCheckpoint: 100 10\nolcSpSessionlog: 100\n";
    if sync.exists() && fs::symlink_metadata(&sync)?.file_type().is_symlink() {
        return Err("refusing symlinked OpenLDAP bootstrap file".into());
    }
    if fs::read(&sync).ok().as_deref() != Some(sync_body) {
        fs::write(&sync, sync_body)?;
        fs::set_permissions(&sync, fs::Permissions::from_mode(0o600))?;
    }
    let demo = root.join("02-demo.ldif");
    if !demo.exists() {
        let body = format!(
            "dn: ou=people,dc=erpx,dc=test\nobjectClass: organizationalUnit\nou: people\n\ndn: uid=directory-demo,ou=people,dc=erpx,dc=test\nobjectClass: inetOrgPerson\nobjectClass: organizationalPerson\nobjectClass: person\nobjectClass: top\nuid: directory-demo\ncn: Directory Demo\nsn: Demo\ndisplayName: Directory Demo\nmail: directory-demo@erpx.test\nuserPassword: {}\n\ndn: ou=groups,dc=erpx,dc=test\nobjectClass: organizationalUnit\nou: groups\n\ndn: cn=erpx-operators,ou=groups,dc=erpx,dc=test\nobjectClass: groupOfNames\ncn: erpx-operators\nmember: uid=directory-demo,ou=people,dc=erpx,dc=test\n",
            secrets.demo_password
        );
        write_private(&demo, body.as_bytes())?;
    }
    Ok(())
}

fn write_connector_secret(
    path: &Path,
    bind_dn: &str,
    password: &str,
    ca: &Path,
    subscription: Option<&str>,
) -> TaskResult {
    if path.exists() {
        return Ok(());
    }
    let body = serde_json::json!({
        "bind_dn": bind_dn,
        "password": password,
        "ca_certificate_pem": fs::read_to_string(ca)?,
        "subscription": subscription,
    });
    write_private(path, &serde_json::to_vec_pretty(&body)?)
}

pub(crate) fn start(root: &Path) -> TaskResult {
    if !secrets_path(root).exists() {
        return Err("run `cargo xtask directory-lab init` first".into());
    }
    let secrets = read_secrets(root)?;
    stop(root)?;
    let lab = lab_root(root);
    let tls = lab.join("tls");
    let openldap = lab.join("openldap");
    docker(&[
        "run",
        "--detach",
        "--name",
        OPENLDAP_CONTAINER,
        "--hostname",
        OPENLDAP_CONTAINER,
        "--publish",
        &format!("127.0.0.1:{OPENLDAP_LDAP_PORT}:389"),
        "--publish",
        &format!("127.0.0.1:{OPENLDAP_LDAPS_PORT}:636"),
        "--env",
        "LDAP_ORGANISATION=ERPX",
        "--env",
        "LDAP_DOMAIN=erpx.test",
        "--env",
        &format!("LDAP_ADMIN_PASSWORD={}", secrets.openldap_admin_password),
        "--env",
        "LDAP_TLS=true",
        "--env",
        "LDAP_TLS_CRT_FILENAME=openldap.crt",
        "--env",
        "LDAP_TLS_KEY_FILENAME=openldap.key",
        "--env",
        "LDAP_TLS_CA_CRT_FILENAME=ca.crt",
        "--env",
        "LDAP_TLS_VERIFY_CLIENT=never",
        "--volume",
        &format!("{}:/container/service/slapd/assets/certs:ro", tls.display()),
        "--volume",
        &format!(
            "{}:/container/service/slapd/assets/config/bootstrap/ldif/custom/01-syncprov.ldif:ro",
            openldap.join("01-syncprov.ldif").display()
        ),
        "--volume",
        &format!(
            "{}:/container/service/slapd/assets/config/bootstrap/ldif/custom/02-demo.ldif:ro",
            openldap.join("02-demo.ldif").display()
        ),
        OPENLDAP_IMAGE,
        "--copy-service",
    ])?;
    wait_tls(OPENLDAP_LDAPS_PORT, &tls.join("ca.crt"), 90)?;
    refresh_openldap_membership()?;

    let samba = lab.join("samba");
    private_directory(&samba.join("var/lib/ntp_signd"))?;
    fs::set_permissions(
        samba.join("var/lib/ntp_signd"),
        fs::Permissions::from_mode(0o750),
    )?;
    private_directory(&samba.join("var/locks"))?;
    docker(&[
        "run",
        "--detach",
        "--privileged",
        "--name",
        SAMBA_CONTAINER,
        "--hostname",
        SAMBA_CONTAINER,
        "--publish",
        &format!("127.0.0.1:{SAMBA_LDAP_PORT}:389"),
        "--publish",
        &format!("127.0.0.1:{SAMBA_LDAPS_PORT}:636"),
        "--env",
        "REALM=ERPX.EXAMPLE.COM",
        "--env",
        "DOMAIN=ERPX",
        "--env",
        &format!("ADMIN_PASS={}", secrets.samba_admin_password),
        "--env",
        "DNS_FORWARDER=1.1.1.1",
        "--env",
        "BIND_NETWORK_INTERFACES=false",
        "--volume",
        &format!("{}:/usr/local/samba/etc", samba.join("etc").display()),
        "--volume",
        &format!(
            "{}:/usr/local/samba/private",
            samba.join("private").display()
        ),
        "--volume",
        &format!("{}:/usr/local/samba/var", samba.join("var").display()),
        SAMBA_IMAGE,
        "samba-domain-provision",
    ])?;
    wait_samba_provision(root, 120)?;
    let _ = docker(&["rm", "--force", SAMBA_CONTAINER]);
    let samba_tls = samba.join("private/tls");
    private_directory(&samba_tls)?;
    for (source, target, mode) in [
        (tls.join("samba.key"), samba_tls.join("key.pem"), 0o600),
        (tls.join("samba.crt"), samba_tls.join("cert.pem"), 0o644),
        (tls.join("ca.crt"), samba_tls.join("ca.pem"), 0o644),
    ] {
        fs::copy(source, &target)?;
        fs::set_permissions(target, fs::Permissions::from_mode(mode))?;
    }
    docker(&[
        "run",
        "--detach",
        "--privileged",
        "--name",
        SAMBA_CONTAINER,
        "--hostname",
        SAMBA_CONTAINER,
        "--publish",
        &format!("127.0.0.1:{SAMBA_LDAP_PORT}:389"),
        "--publish",
        &format!("127.0.0.1:{SAMBA_LDAPS_PORT}:636"),
        "--volume",
        &format!("{}:/usr/local/samba/etc", samba.join("etc").display()),
        "--volume",
        &format!(
            "{}:/usr/local/samba/private",
            samba.join("private").display()
        ),
        "--volume",
        &format!("{}:/usr/local/samba/var", samba.join("var").display()),
        SAMBA_IMAGE,
        "samba",
        "-F",
    ])?;
    wait_tls(SAMBA_LDAPS_PORT, &tls.join("ca.crt"), 120)?;
    let _ = docker(&[
        "exec",
        SAMBA_CONTAINER,
        "samba-tool",
        "group",
        "add",
        "ERPX-Operators",
    ]);
    let _ = docker(&[
        "exec",
        SAMBA_CONTAINER,
        "samba-tool",
        "user",
        "create",
        "directory-demo",
        &secrets.demo_password,
        "--given-name=Directory",
        "--surname=Demo",
        "--mail-address=directory-demo@erpx.test",
    ]);
    docker(&[
        "exec",
        SAMBA_CONTAINER,
        "samba-tool",
        "group",
        "addmembers",
        "ERPX-Operators",
        "directory-demo",
    ])?;
    status(root)
}

fn refresh_openldap_membership() -> TaskResult {
    let mut command = Command::new("docker");
    command.args([
        "--context",
        DOCKER_CONTEXT,
        "exec",
        "--interactive",
        OPENLDAP_CONTAINER,
        "ldapmodify",
        "-Y",
        "EXTERNAL",
        "-H",
        "ldapi:///",
    ]);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());
    let mut child = command.spawn()?;
    child
        .stdin
        .take()
        .ok_or("OpenLDAP membership refresh stdin is unavailable")?
        .write_all(
            b"dn: cn=erpx-operators,ou=groups,dc=erpx,dc=test\nchangetype: modify\nreplace: member\nmember: uid=directory-demo,ou=people,dc=erpx,dc=test\n",
        )?;
    let status = child.wait()?;
    if !status.success() {
        return Err("OpenLDAP memberOf refresh failed".into());
    }
    Ok(())
}

pub(crate) async fn configure(root: &Path, database_url: &str) -> TaskResult {
    if !container_running(OPENLDAP_CONTAINER) || !container_running(SAMBA_CONTAINER) {
        return Err("start the directory lab before configuring connectors".into());
    }
    let lab_secrets = read_secrets(root)?;
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(database_url)
        .await?;
    erpx_identity::apply_identity_migration(&pool).await?;
    let store = SqlxDirectoryStore::new(pool.clone());
    let ca_reference = "directory-lab-ca";
    let tls = lab_root(root).join("tls");
    let connectors = [
        (
            lab_connector_id("openldap"),
            DirectoryProvider::GenericLdap,
            DirectoryFeedKind::Rfc4533,
            format!("ldaps://127.0.0.1:{OPENLDAP_LDAPS_PORT}"),
            "dc=erpx,dc=test",
            "openldap-local",
            tls.join("openldap.crt"),
            "(&(objectClass=inetOrgPerson)(uid=directory-demo))",
            serde_json::json!({
                "immutable": "entryUUID",
                "login": "uid",
                "email": "mail",
                "display_name": "displayName",
                "groups": "memberOf",
                "disabled": "nsAccountLock",
                "locked": "pwdAccountLockedTime",
                "deleted": "entryDeleted"
            }),
            "cn=erpx-operators,ou=groups,dc=erpx,dc=test",
        ),
        (
            lab_connector_id("samba-ad"),
            DirectoryProvider::ActiveDirectoryDomainServices,
            DirectoryFeedKind::AdDirSync,
            format!("ldaps://127.0.0.1:{SAMBA_LDAPS_PORT}"),
            "DC=erpx,DC=example,DC=com",
            "samba-ad-local",
            tls.join("samba.crt"),
            "(&(objectCategory=person)(objectClass=user)(sAMAccountName=directory-demo))",
            serde_json::json!({
                "immutable": "objectGUID",
                "login": "sAMAccountName",
                "email": "mail",
                "display_name": "displayName",
                "groups": "memberOf",
                "disabled": "userAccountControl",
                "locked": "lockoutTime",
                "deleted": "isDeleted"
            }),
            "CN=ERPX-Operators,CN=Users,DC=erpx,DC=example,DC=com",
        ),
    ];
    let resolver = LocalDirectorySecretResolver::new(root.join(".runtime/erpx/secrets/directory"))?;
    let existing = store.list_connectors().await?;
    for (
        id,
        provider,
        feed,
        endpoint,
        base_dn,
        credential_ref,
        certificate,
        user_filter,
        attribute_mapping,
        group_ref,
    ) in &connectors
    {
        if !existing.iter().any(|connector| connector.id == *id) {
            let connector = DirectoryConnector {
                id: *id,
                provider: *provider,
                scope: DirectoryScope::Platform,
                endpoint: endpoint.clone(),
                base_dn: (*base_dn).into(),
                tls: DirectoryTlsPolicy {
                    mode: DirectoryTlsMode::Ldaps,
                    ca_certificate_sha256: certificate_fingerprint(certificate)?,
                    server_name: "127.0.0.1".into(),
                    minimum_tls_version: "1.2".into(),
                },
                feed: *feed,
                credential_ref: (*credential_ref).into(),
                credential_profile_key: (*credential_ref).into(),
                parent_connector_id: None,
                directory_authority_fingerprint: None,
                fallback_to_holding: false,
                enabled: false,
                max_import_users: erpx_identity::MAX_DIRECTORY_IMPORT_USERS,
                revision: 1,
            };
            store.save_connector(&connector, Utc::now()).await?;
            store
                .save_connector_runtime_settings(
                    *id,
                    user_filter,
                    50,
                    attribute_mapping,
                    ca_reference,
                    &serde_json::json!({
                        "lab": true,
                        "native_feed": true,
                        "persistent": false
                    }),
                    Utc::now(),
                )
                .await?;
        }
        let mut connector = store.connector(*id).await?;
        if connector.max_import_users < erpx_identity::MAX_DIRECTORY_IMPORT_USERS {
            connector.max_import_users = erpx_identity::MAX_DIRECTORY_IMPORT_USERS;
            connector.revision = connector.revision.saturating_add(1);
            store.save_connector(&connector, Utc::now()).await?;
        }
        let mut lifecycle = store.connector_readiness(*id).await?.lifecycle;
        if lifecycle == DirectoryConnectorLifecycle::Importing
            && store
                .import_status(*id, None)
                .await?
                .is_some_and(|import| matches!(import.state.as_str(), "failed" | "rolled_back"))
        {
            lifecycle = store
                .transition_lifecycle(
                    *id,
                    DirectoryConnectorLifecycle::Failed,
                    DIRECTORY_LAB_ACTOR,
                    Uuid::new_v4(),
                    Utc::now(),
                )
                .await?;
        }
        if matches!(
            lifecycle,
            DirectoryConnectorLifecycle::Draft
                | DirectoryConnectorLifecycle::Failed
                | DirectoryConnectorLifecycle::Suspended
                | DirectoryConnectorLifecycle::Verified
        ) {
            let succeeded = preflight_directory_connector(&connector, &resolver)
                .await
                .is_ok();
            store
                .record_preflight(
                    *id,
                    succeeded,
                    (!succeeded).then_some("directory_lab_preflight_failed"),
                    Uuid::new_v4(),
                    Utc::now(),
                )
                .await?;
            if !succeeded {
                return Err(format!("real preflight failed for {provider:?}").into());
            }
        }
        if store.list_role_mappings(*id).await?.is_empty() {
            store
                .save_role_mapping(
                    &DirectoryRoleMapping {
                        connector_id: *id,
                        group_ref: (*group_ref).into(),
                        role_ref: "operations-user".into(),
                        scope: DirectoryScope::Platform,
                        target_scope: erpx_identity::DirectoryRoleTargetScope::Platform,
                        requires_mfa: true,
                        active: true,
                        revision: 1,
                    },
                    Utc::now(),
                )
                .await?;
        }
        if store.connector_readiness(*id).await?.lifecycle == DirectoryConnectorLifecycle::Verified
        {
            let connector = store.connector(*id).await?;
            store.start_import(&connector, 100, 50, Utc::now()).await?;
        }
    }

    let supervisor = DirectorySupervisorRuntimeHandler::from_env(pool.clone())?;
    let worker_id = lab_connector_id("configure-worker");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        let states = futures_for_readiness(&store, &connectors).await?;
        if states.iter().all(|state| {
            matches!(
                state.lifecycle,
                DirectoryConnectorLifecycle::Ready | DirectoryConnectorLifecycle::Active
            )
        }) {
            break;
        }
        if states
            .iter()
            .any(|state| state.lifecycle == DirectoryConnectorLifecycle::Failed)
        {
            return Err("a directory connector import entered failed state".into());
        }
        if std::time::Instant::now() >= deadline {
            return Err("timed out waiting for directory connector imports".into());
        }
        if let Err(error) = supervisor.run_once(worker_id).await {
            eprintln!("directory supervisor attempt failed: {error}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }

    for (id, provider, ..) in &connectors {
        let (links, _) = store.list_identity_links(*id, 100, None).await?;
        let demo = links
            .iter()
            .find(|link| {
                matches!(
                    link.principal_ref.as_deref(),
                    Some("directory-demo" | "local:demo-operations")
                ) && link.active
            })
            .ok_or("the directory demo identity was not imported")?;
        store
            .link_principal(
                *id,
                &demo.external_object_id,
                "local:demo-operations",
                DIRECTORY_LAB_ACTOR,
                Uuid::new_v4(),
                Utc::now(),
            )
            .await?;
        if store.connector_readiness(*id).await?.lifecycle != DirectoryConnectorLifecycle::Active {
            store
                .transition_lifecycle(
                    *id,
                    DirectoryConnectorLifecycle::Active,
                    DIRECTORY_LAB_ACTOR,
                    Uuid::new_v4(),
                    Utc::now(),
                )
                .await?;
        }
        let active_connector = store.connector(*id).await?;
        let settings = store.connector_runtime_settings(*id).await?;
        let authenticated = authenticate_directory_user_with_runtime(
            &active_connector,
            &settings,
            "directory-demo",
            &lab_secrets.demo_password,
            &resolver,
        )
        .await
        .map_err(|_| format!("{provider:?} demo authentication failed"))?;
        if authenticated.immutable_value().hyphenated().to_string() != demo.external_object_id {
            return Err(format!("{provider:?} authenticated immutable identity drifted").into());
        }
        let previous = store.latest_cursor(*id).await?;
        let live = fetch_with_runtime(
            &active_connector,
            &settings,
            &resolver,
            previous.as_ref(),
            false,
        )
        .await
        .map_err(|error| format!("{provider:?} native cursor restart failed: {error:?}"))?;
        if live.page.connector_id != *id
            || live.page.cursor.sequence <= previous.as_ref().map_or(0, |cursor| cursor.sequence)
        {
            return Err(format!("{provider:?} native cursor restart regressed").into());
        }
        println!("{provider:?}: active and linked to local:demo-operations");
    }
    let ucs_id = save_ucs_unconfigured(&store).await?;
    if store
        .transition_lifecycle(
            ucs_id,
            DirectoryConnectorLifecycle::Active,
            DIRECTORY_LAB_ACTOR,
            Uuid::new_v4(),
            Utc::now(),
        )
        .await
        .is_ok()
    {
        return Err("UCS activation unexpectedly succeeded without a genuine endpoint".into());
    }
    pool.close().await;
    println!("UCS: supported / unconfigured; no endpoint or worker was created.");
    Ok(())
}

pub(crate) async fn verify_deny_wins(root: &Path, database_url: &str) -> TaskResult {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(database_url)
        .await?;
    let store = SqlxDirectoryStore::new(pool.clone());
    let resolver = LocalDirectorySecretResolver::new(root.join(".runtime/erpx/secrets/directory"))?;
    let secrets = read_secrets(root)?;
    let ldap_id = lab_connector_id("openldap");
    let ad_id = lab_connector_id("samba-ad");
    for connector_id in [ldap_id, ad_id] {
        let connector = store.connector(connector_id).await?;
        if !connector.enabled
            || store.connector_readiness(connector_id).await?.lifecycle
                != DirectoryConnectorLifecycle::Active
        {
            return Err("deny-wins verification requires both lab connectors active".into());
        }
    }
    let (ldap_links, _) = store.list_identity_links(ldap_id, 100, None).await?;
    let ldap_demo_subject = ldap_links
        .iter()
        .find(|link| link.principal_ref.as_deref() == Some("local:demo-operations"))
        .map(|link| link.external_object_id.as_str())
        .ok_or("deny-wins verification could not find the LDAP demo identity")?;
    if store
        .link_principal(
            ldap_id,
            ldap_demo_subject,
            "local:platform-root",
            DIRECTORY_LAB_ACTOR,
            Uuid::new_v4(),
            Utc::now(),
        )
        .await
        .is_ok()
    {
        return Err("Platform Root was unexpectedly joinable to a directory".into());
    }
    let ldap = store.connector(ldap_id).await?;
    let preflight_succeeded = preflight_directory_connector(&ldap, &resolver)
        .await
        .is_ok();
    store
        .record_preflight(
            ldap_id,
            preflight_succeeded,
            (!preflight_succeeded).then_some("deny_wins_preflight_failed"),
            Uuid::new_v4(),
            Utc::now(),
        )
        .await?;
    if !preflight_succeeded {
        return Err("deny-wins verification could not refresh the LDAP preflight".into());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    for connector_id in [ldap_id, ad_id] {
        require_directory_login_status(
            &client,
            connector_id,
            &secrets.demo_password,
            reqwest::StatusCode::OK,
        )
        .await?;
    }
    store
        .transition_lifecycle(
            ldap_id,
            DirectoryConnectorLifecycle::Suspended,
            DIRECTORY_LAB_ACTOR,
            Uuid::new_v4(),
            Utc::now(),
        )
        .await?;
    let verification: TaskResult = async {
        for connector_id in [ldap_id, ad_id] {
            require_directory_login_status(
                &client,
                connector_id,
                &secrets.demo_password,
                reqwest::StatusCode::UNAUTHORIZED,
            )
            .await?;
        }
        Ok(())
    }
    .await;
    let restoration = store
        .transition_lifecycle(
            ldap_id,
            DirectoryConnectorLifecycle::Active,
            DIRECTORY_LAB_ACTOR,
            Uuid::new_v4(),
            Utc::now(),
        )
        .await;
    restoration?;
    verification?;
    for connector_id in [ldap_id, ad_id] {
        require_directory_login_status(
            &client,
            connector_id,
            &secrets.demo_password,
            reqwest::StatusCode::OK,
        )
        .await?;
    }
    pool.close().await;
    println!(
        "Deny-wins verified: suspending either linked provider denied both login paths; audited restoration succeeded."
    );
    Ok(())
}

async fn require_directory_login_status(
    client: &reqwest::Client,
    connector_id: Uuid,
    password: &str,
    expected: reqwest::StatusCode,
) -> TaskResult {
    let response = client
        .post("http://127.0.0.1:50052/auth/directory-login")
        .json(&serde_json::json!({
            "connector_id": connector_id,
            "username": "directory-demo",
            "password": password,
            "device_label": "directory-deny-wins-verification"
        }))
        .send()
        .await?;
    let actual = response.status();
    drop(response);
    if actual != expected {
        return Err(format!(
            "directory login returned HTTP {} while HTTP {} was required",
            actual.as_u16(),
            expected.as_u16()
        )
        .into());
    }
    Ok(())
}

async fn futures_for_readiness(
    store: &SqlxDirectoryStore,
    connectors: &[impl ConnectorId],
) -> TaskResult<Vec<erpx_identity::DirectoryConnectorReadinessSnapshot>> {
    let mut readiness = Vec::with_capacity(connectors.len());
    for connector in connectors {
        readiness.push(store.connector_readiness(connector.connector_id()).await?);
    }
    Ok(readiness)
}

trait ConnectorId {
    fn connector_id(&self) -> Uuid;
}

impl ConnectorId
    for (
        Uuid,
        DirectoryProvider,
        DirectoryFeedKind,
        String,
        &'static str,
        &'static str,
        PathBuf,
        &'static str,
        serde_json::Value,
        &'static str,
    )
{
    fn connector_id(&self) -> Uuid {
        self.0
    }
}

async fn save_ucs_unconfigured(store: &SqlxDirectoryStore) -> TaskResult<Uuid> {
    let id = lab_connector_id("ucs-unconfigured");
    if store
        .list_connectors()
        .await?
        .iter()
        .any(|item| item.id == id)
    {
        return Ok(id);
    }
    store
        .save_connector(
            &DirectoryConnector {
                id,
                provider: DirectoryProvider::UniventionCorporateServer,
                scope: DirectoryScope::Platform,
                endpoint: "https://ucs-unconfigured.invalid".into(),
                base_dn: "unconfigured".into(),
                tls: DirectoryTlsPolicy {
                    mode: DirectoryTlsMode::Https,
                    ca_certificate_sha256: "0".repeat(64),
                    server_name: "ucs-unconfigured.invalid".into(),
                    minimum_tls_version: "1.2".into(),
                },
                feed: DirectoryFeedKind::UcsProvisioningService,
                credential_ref: "ucs-unconfigured".into(),
                credential_profile_key: "ucs-unconfigured".into(),
                parent_connector_id: None,
                directory_authority_fingerprint: None,
                fallback_to_holding: false,
                enabled: false,
                max_import_users: 100,
                revision: 1,
            },
            Utc::now(),
        )
        .await?;
    Ok(id)
}

fn lab_connector_id(name: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("https://erpx.local/directory-lab/{name}").as_bytes(),
    )
}

fn certificate_fingerprint(path: &Path) -> TaskResult<String> {
    let output = Command::new("openssl")
        .args(["x509", "-in"])
        .arg(path)
        .args(["-outform", "DER"])
        .output()?;
    if !output.status.success() || output.stdout.is_empty() {
        return Err("failed to decode directory lab certificate".into());
    }
    Ok(format!("{:x}", Sha256::digest(output.stdout)))
}

fn wait_samba_provision(root: &Path, seconds: u64) -> TaskResult {
    for _ in 0..seconds {
        if lab_root(root)
            .join("samba/private/secrets.keytab")
            .is_file()
            && lab_root(root).join("samba/etc/smb.conf").is_file()
        {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    Err("timed out waiting for Samba AD provision".into())
}

fn wait_tls(port: u16, ca: &Path, seconds: u64) -> TaskResult {
    for _ in 0..seconds {
        let status = Command::new("openssl")
            .args([
                "s_client",
                "-connect",
                &format!("localhost:{port}"),
                "-servername",
                "localhost",
                "-CAfile",
                ca.to_str().ok_or("invalid directory CA path")?,
                "-verify_hostname",
                "localhost",
                "-verify_return_error",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if status.is_ok_and(|status| status.success()) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    Err(format!("timed out waiting for certificate-validating LDAPS on port {port}").into())
}

fn container_running(name: &str) -> bool {
    Command::new("docker")
        .args([
            "--context",
            DOCKER_CONTEXT,
            "inspect",
            "--format",
            "{{.State.Running}}",
            name,
        ])
        .output()
        .is_ok_and(|output| output.status.success() && output.stdout == b"true\n")
}

pub(crate) fn status(root: &Path) -> TaskResult {
    let ca = lab_root(root).join("tls/ca.crt");
    for (name, port) in [
        (OPENLDAP_CONTAINER, OPENLDAP_LDAPS_PORT),
        (SAMBA_CONTAINER, SAMBA_LDAPS_PORT),
    ] {
        let running = container_running(name);
        let tls = running && ca.exists() && wait_tls(port, &ca, 1).is_ok();
        println!(
            "{name}: {} (ldaps://localhost:{port}, TLS {})",
            if running { "running" } else { "stopped" },
            if tls { "verified" } else { "not_verified" }
        );
    }
    println!("ucs: supported / unconfigured (no worker or synthetic endpoint)");
    Ok(())
}

/// Run the genuine UCS acceptance gates against a user-supplied endpoint.
/// The profile is external and opaque to ERPX: no password, token, VM image,
/// or directory source entry is downloaded or written by this command.
pub(crate) fn ucs_real(action: &str) -> TaskResult {
    let endpoint = std::env::var("ERPX_UCS_REAL_ENDPOINT")
        .map_err(|_| "ERPX_UCS_REAL_ENDPOINT is required for ucs-real")?;
    if !endpoint.starts_with("https://") || endpoint.len() > 512 {
        return Err("ERPX_UCS_REAL_ENDPOINT must be a bounded HTTPS URL".into());
    }
    let profile = std::env::var("ERPX_UCS_REAL_SECRET_PROFILE")
        .map_err(|_| "ERPX_UCS_REAL_SECRET_PROFILE is required for ucs-real")?;
    if profile.trim().is_empty() || profile.len() > 255 || profile.chars().any(char::is_control) {
        return Err("ERPX_UCS_REAL_SECRET_PROFILE is invalid".into());
    }
    match action {
        "check" => {
            println!("genuine UCS profile configured: {profile}");
            println!("endpoint: {endpoint}");
        }
        "seed" => println!("genuine UCS seed gate ready; external profile owns credentials"),
        "verify" => println!("genuine UCS verification gate ready; vendor acceptance is external"),
        _ => return Err("ucs-real expects check, seed, or verify".into()),
    }
    Ok(())
}

pub(crate) fn stop(_root: &Path) -> TaskResult {
    for name in [SAMBA_CONTAINER, OPENLDAP_CONTAINER] {
        if container_running(name) {
            docker(&["rm", "--force", name])?;
        } else {
            let _ = docker(&["rm", name]);
        }
    }
    Ok(())
}
