use erpx_server_platform_administration::public::legal_release::*;
use sha2::{Digest, Sha256};
fn facts() -> LegalReleaseFacts {
    let catalogue = current_legal_release_catalogue().unwrap();
    LegalReleaseFacts {
        latest: Some(LegalAcceptance {
            id: "release".into(),
            software_version: "synthetic-native-release".into(),
            configuration_revision: 4,
            setup_version: 2,
            requested_by: "requester".into(),
            revoked_at: None,
        }),
        configuration_revision: Some(4),
        setup_status: Some("accepted".into()),
        setup_version: Some(2),
        gates: catalogue
            .criteria()
            .iter()
            .map(|c| LegalGateFact {
                criterion_id: c.id.clone(),
                state: "passed".into(),
                permission: c.permission.clone(),
                reviewer_current: true,
                reviewer_revision: "current".into(),
            })
            .collect(),
    }
}
#[test]
fn legal_release_retains_all_seven_current_independent_gates_and_exact_scope_identity() {
    let catalogue = current_legal_release_catalogue().unwrap();
    assert_eq!(catalogue.scope_version(), "legal-th-operations-v1");
    assert_eq!(catalogue.criteria().len(), 7);
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/platform_administration/rust-legal-release/source.v1.json"
    ))
    .unwrap();
    for (actual, legacy) in catalogue
        .criteria()
        .iter()
        .zip(oracle["criteria"].as_array().unwrap())
    {
        assert_eq!(actual.id, legacy["id"]);
        assert_eq!(actual.permission, legacy["permission"]);
    }
    assert!(accepted_legal_release(
        &catalogue,
        "synthetic-native-release",
        &facts()
    ));
    for i in 0..7 {
        let mut f = facts();
        f.gates[i].reviewer_current = false;
        assert!(!accepted_legal_release(
            &catalogue,
            "synthetic-native-release",
            &f
        ));
        f = facts();
        f.gates[i].state = "not_applicable".into();
        assert!(!accepted_legal_release(
            &catalogue,
            "synthetic-native-release",
            &f
        ));
        f = facts();
        f.gates[i].permission = "organization.read".into();
        assert!(!accepted_legal_release(
            &catalogue,
            "synthetic-native-release",
            &f
        ));
    }
    for variant in 0..8 {
        let mut f = facts();
        let mut artifact = "synthetic-native-release";
        match variant {
            0 => f.latest = None,
            1 => artifact = "unbuilt",
            2 => artifact = "another-build",
            3 => f.latest.as_mut().unwrap().revoked_at = Some(1),
            4 => f.configuration_revision = Some(5),
            5 => f.setup_status = Some("draft".into()),
            6 => f.setup_version = Some(3),
            _ => f.gates.clear(),
        }
        assert!(!accepted_legal_release(&catalogue, artifact, &f));
    }
}
#[test]
fn engineering_context_rechecks_real_files_and_exact_local_configuration() {
    let temporary = tempfile::tempdir().unwrap();
    let run = "0123456789ab";
    let directory = temporary.path().join(".runtime/browser").join(run);
    let release = directory.join("release");
    std::fs::create_dir_all(&release).unwrap();
    let source = "a".repeat(64);
    let artifact = format!("release-{source}");
    let manifest = b"synthetic source manifest";
    let digest = hex::encode(Sha256::digest(manifest));
    let attestation = serde_json::json!({"scope":"isolated-engineering","run":run,"sourceFingerprint":source,"softwareVersion":artifact,"fileManifestSha256":digest});
    std::fs::write(
        directory.join("engineering-attestation.json"),
        serde_json::to_vec(&attestation).unwrap(),
    )
    .unwrap();
    std::fs::write(
        release.join("release.json"),
        serde_json::to_vec(&attestation).unwrap(),
    )
    .unwrap();
    std::fs::write(release.join("release-files.json"), manifest).unwrap();
    let configuration = EngineeringReadConfiguration {
        run: run.into(),
        environment: "local".into(),
        port: "3104".into(),
        browser_simulation_port: "3104".into(),
        data_directory: directory.clone(),
    };
    assert!(engineering_attestation_matches(&configuration, &artifact));
    let captured =
        capture_bounded_engineering_attestation(&configuration, &artifact, 1024).unwrap();
    assert_eq!(captured.run(), run);
    assert_eq!(captured.source_fingerprint(), source);
    // A retained valid capture cannot be substituted for different bytes.
    std::fs::write(release.join("release-files.json"), b"changed-buffer").unwrap();
    assert!(capture_bounded_engineering_attestation(&configuration, &artifact, 1024).is_none());
    std::fs::write(release.join("release-files.json"), vec![b'x'; 1025]).unwrap();
    assert!(capture_bounded_engineering_attestation(&configuration, &artifact, 1024).is_none());
    std::fs::write(release.join("release-files.json"), manifest).unwrap();
    let restored =
        capture_bounded_engineering_attestation(&configuration, &artifact, 1024).unwrap();
    assert!(captured == restored);
    for variant in 0..5 {
        let mut c = configuration.clone();
        match variant {
            0 => c.environment = "production".into(),
            1 => c.port = "3100".into(),
            2 => c.browser_simulation_port = "3100".into(),
            3 => c.run = "ABCDEF012345".into(),
            _ => c.data_directory = temporary.path().into(),
        };
        assert!(!engineering_attestation_matches(&c, &artifact));
    }
    assert!(!engineering_attestation_matches(&configuration, "unbuilt"));
    std::fs::write(release.join("release-files.json"), b"tampered").unwrap();
    assert!(!engineering_attestation_matches(&configuration, &artifact));
    std::fs::write(release.join("release-files.json"), manifest).unwrap();
    assert!(engineering_attestation_matches(&configuration, &artifact));
    #[cfg(unix)]
    {
        std::fs::rename(
            release.join("release-files.json"),
            release.join("moved.json"),
        )
        .unwrap();
        std::os::unix::fs::symlink(
            release.join("moved.json"),
            release.join("release-files.json"),
        )
        .unwrap();
        assert!(!engineering_attestation_matches(&configuration, &artifact));
        assert!(capture_bounded_engineering_attestation(&configuration, &artifact, 1024).is_none());
    }
}

#[cfg(unix)]
#[test]
fn native_roster_snapshot_rechecks_private_file_seal_and_exact_run_directory() {
    use erpx_server_platform_administration::public::native_roster_snapshot::capture_native_roster_snapshot;
    use std::os::unix::fs::{PermissionsExt, symlink};
    let _temporary = tempfile::tempdir().unwrap();let root = _temporary.path().canonicalize().unwrap();
    let run = "0123456789ab";
    let dir = root.join(".runtime/browser").join(run);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = dir.join("native-engineering-leases.json");
    let source = "a".repeat(64);
    let manifest = "b".repeat(64);
    let bindings = [
        "CONTROL_DB",
        "CONTROL_READ_DB",
        "H001_DB",
        "H001_READ_DB",
        "H002_DB",
        "H002_READ_DB",
        "H003_DB",
        "H003_READ_DB",
    ];
    let roster = serde_json::json!({"version":1,"run":run,"sourceFingerprint":source,"fileManifestSha256":manifest,"postmasterStartedMs":1,"entries":bindings.iter().enumerate().map(|(i,b)|serde_json::json!({"binding":b,"token":format!("{:032x}",i+1),"oid":i+1})).collect::<Vec<_>>()});
    let bytes = serde_json::to_vec(&roster).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let seal = hex::encode(Sha256::digest(&bytes));
    let capture = || capture_native_roster_snapshot(&dir, &seal, run, &source, &manifest);
    let snapshot = capture().unwrap();
    assert_eq!(snapshot.roster().entries().len(), 8);
    assert!(snapshot.require_current(&dir, &seal, run, &source, &manifest));
    // The retained attester only reads the exact private run directory, seal,
    // source and manifest; none of those can be substituted.
    let other = root.join(".runtime/browser").join("0123456789ac");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::set_permissions(&other, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(
        other.join("native-engineering-leases.json"),
        &bytes,
    )
    .unwrap();
    std::fs::set_permissions(
        other.join("native-engineering-leases.json"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert!(capture_native_roster_snapshot(&other, &seal, run, &source, &manifest).is_none());
    assert!(
        capture_native_roster_snapshot(&dir, &"c".repeat(64), run, &source, &manifest).is_none()
    );
    assert!(capture_native_roster_snapshot(
        &dir,
        &seal,
        "0123456789ac",
        &source,
        &manifest
    )
    .is_none());
    assert!(
        capture_native_roster_snapshot(&dir, &seal, run, &"c".repeat(64), &manifest).is_none()
    );
    assert!(
        capture_native_roster_snapshot(&dir, &seal, run, &source, &"c".repeat(64)).is_none()
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(capture().is_none());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(capture().is_some());
    std::fs::write(&path, b"{}").unwrap();
    assert!(capture().is_none());
    assert!(!snapshot.require_current(&dir, &seal, run, &source, &manifest));
    std::fs::remove_file(&path).unwrap();
    let target = dir.join("target.json");
    std::fs::write(&target, &bytes).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
    symlink(&target, &path).unwrap();
    assert!(capture().is_none());
    std::fs::remove_file(&path).unwrap();
    std::fs::write(&path, vec![b'x'; 16385]).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(capture().is_none());
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(capture().is_none());
}

#[cfg(unix)]
#[test]
fn native_roster_snapshot_rejects_replaced_directory_even_with_identical_bytes() {
    use erpx_server_platform_administration::public::native_roster_snapshot::capture_native_roster_snapshot;
    use std::os::unix::fs::{PermissionsExt, symlink};
    let _temporary = tempfile::tempdir().unwrap();let root = _temporary.path().canonicalize().unwrap();
    let run = "0123456789ab";
    let dir = root.join(".runtime/browser").join(run);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let source = "a".repeat(64);
    let manifest = "b".repeat(64);
    let bindings = [
        "CONTROL_DB",
        "CONTROL_READ_DB",
        "H001_DB",
        "H001_READ_DB",
        "H002_DB",
        "H002_READ_DB",
        "H003_DB",
        "H003_READ_DB",
    ];
    let roster = serde_json::json!({"version":1,"run":run,"sourceFingerprint":source,"fileManifestSha256":manifest,"postmasterStartedMs":1,"entries":bindings.iter().enumerate().map(|(i,b)|serde_json::json!({"binding":b,"token":format!("{:032x}",i+1),"oid":i+1})).collect::<Vec<_>>()});
    let bytes = serde_json::to_vec(&roster).unwrap();
    let seal = hex::encode(Sha256::digest(&bytes));
    let path = dir.join("native-engineering-leases.json");
    std::fs::write(&path, &bytes).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let snapshot = capture_native_roster_snapshot(&dir, &seal, run, &source, &manifest).unwrap();
    let prior = root.join("prior");
    std::fs::rename(&dir, &prior).unwrap();
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(dir.join("native-engineering-leases.json"), &bytes).unwrap();
    std::fs::set_permissions(
        dir.join("native-engineering-leases.json"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert!(!snapshot.require_current(&dir, &seal, run, &source, &manifest));
    assert!(capture_native_roster_snapshot(&dir, &seal, run, &source, &manifest).is_some());
    std::fs::remove_file(dir.join("native-engineering-leases.json")).unwrap();
    std::fs::remove_dir(&dir).unwrap();
    symlink(&prior, &dir).unwrap();
    assert!(capture_native_roster_snapshot(&dir, &seal, run, &source, &manifest).is_none());
}

#[cfg(unix)]
#[test]
fn attested_roster_capture_binds_declared_lease_to_exact_private_file() {
    use erpx_server_platform_administration::public::native_roster_snapshot::capture_attested_native_roster_snapshot;
    use std::os::unix::fs::PermissionsExt;
    let _temporary = tempfile::tempdir().unwrap();let root = _temporary.path().canonicalize().unwrap();
    let run = "0123456789ab";
    let directory = root.join(".runtime/browser").join(run);
    let release = directory.join("release");
    std::fs::create_dir_all(&release).unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    let source = "a".repeat(64);
    let artifact = format!("release-{source}");
    let manifest_bytes = b"[]";
    let manifest = hex::encode(Sha256::digest(manifest_bytes));
    std::fs::write(release.join("release-files.json"), manifest_bytes).unwrap();
    std::fs::write(
        release.join("release.json"),
        serde_json::to_vec(&serde_json::json!({"sourceFingerprint":source,"softwareVersion":artifact,"fileManifestSha256":manifest})).unwrap(),
    )
    .unwrap();
    let bindings = [
        "CONTROL_DB",
        "CONTROL_READ_DB",
        "H001_DB",
        "H001_READ_DB",
        "H002_DB",
        "H002_READ_DB",
        "H003_DB",
        "H003_READ_DB",
    ];
    let roster = serde_json::json!({"version":1,"run":run,"sourceFingerprint":source,"fileManifestSha256":manifest,"postmasterStartedMs":1,"entries":bindings.iter().enumerate().map(|(i,b)|serde_json::json!({"binding":b,"token":format!("{:032x}",i+1),"oid":i+1})).collect::<Vec<_>>()});
    let roster_bytes = serde_json::to_vec(&roster).unwrap();
    let seal = hex::encode(Sha256::digest(&roster_bytes));
    let roster_path = directory.join("native-engineering-leases.json");
    std::fs::write(&roster_path, &roster_bytes).unwrap();
    std::fs::set_permissions(&roster_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let attestation_path = directory.join("engineering-attestation.json");
    let attestation = serde_json::json!({"scope":"isolated-engineering","run":run,"sourceFingerprint":source,"softwareVersion":artifact,"fileManifestSha256":manifest,"nativeLeaseSha256":seal});
    std::fs::write(&attestation_path, serde_json::to_vec(&attestation).unwrap()).unwrap();
    std::fs::set_permissions(&attestation_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let configuration = EngineeringReadConfiguration {
        run: run.into(),
        environment: "local".into(),
        port: "3104".into(),
        browser_simulation_port: "3104".into(),
        data_directory: directory.clone(),
    };
    let (attestation, roster) =
        capture_attested_native_roster_snapshot(&configuration, &artifact, 1024).unwrap();
    assert_eq!(attestation.native_lease_sha256(), Some(seal.as_str()));
    let roster = roster.unwrap();
    assert_eq!(roster.roster().entries().len(), 8);
    assert!(roster.require_current());
    // A declared lease also requires the private attestation file mode.
    std::fs::set_permissions(&attestation_path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(capture_attested_native_roster_snapshot(&configuration, &artifact, 1024).is_none());
    std::fs::set_permissions(&attestation_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    // A declared lease with a changed or missing roster fails closed.
    std::fs::write(&roster_path, b"{}").unwrap();
    std::fs::set_permissions(&roster_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(!roster.require_current());
    assert!(capture_attested_native_roster_snapshot(&configuration, &artifact, 1024).is_none());
    assert!(capture_bounded_engineering_attestation(&configuration, &artifact, 1024).is_none());
    std::fs::write(&roster_path, &roster_bytes).unwrap();
    std::fs::set_permissions(&roster_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(roster.require_current());
    // Without a declared lease the legacy browser-name path remains available,
    // and no roster file is inferred.
    let legacy = serde_json::json!({"scope":"isolated-engineering","run":run,"sourceFingerprint":source,"softwareVersion":artifact,"fileManifestSha256":manifest});
    std::fs::write(&attestation_path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    std::fs::set_permissions(&attestation_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let (_, legacy_roster) =
        capture_attested_native_roster_snapshot(&configuration, &artifact, 1024).unwrap();
    assert!(legacy_roster.is_none());
    // Malformed or mismatched seals are never treated as a roster presence.
    for invalid in ["z".repeat(64), "c".repeat(64)] {
        let changed = serde_json::json!({"scope":"isolated-engineering","run":run,"sourceFingerprint":source,"softwareVersion":artifact,"fileManifestSha256":manifest,"nativeLeaseSha256":invalid});
        std::fs::write(&attestation_path, serde_json::to_vec(&changed).unwrap()).unwrap();
        std::fs::set_permissions(&attestation_path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(capture_attested_native_roster_snapshot(&configuration, &artifact, 1024).is_none());
    }
}
