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
