use erpx_server_identity_access::public::account_security::*;

#[test]
fn rfc4226_and_rfc6238_vectors_preserve_the_existing_six_digit_sha1_protocol() {
    let secret = MfaSecret::from_bytes(*b"12345678901234567890");
    assert_eq!(secret.base32(), "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
    for (counter, expected) in [
        "755224", "287082", "359152", "969429", "338314", "254676", "287922", "162583", "399871",
        "520489",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(secret.code(counter as i64).unwrap(), *expected);
    }
    for (seconds, expected) in [
        (59, "287082"),
        (1_111_111_109, "081804"),
        (1_111_111_111, "050471"),
        (1_234_567_890, "005924"),
        (2_000_000_000, "279037"),
        (20_000_000_000, "353130"),
    ] {
        assert_eq!(secret.code(seconds / 30).unwrap(), expected);
    }
    assert!(secret.code(-1).is_err());
}
#[test]
fn current_key_rotation_is_user_and_purpose_bound_and_unsupported_legacy_fails_closed() {
    let old = AccountMfaKeyring::new("old".into(), vec![("old".into(), [1; 32])]).unwrap();
    let secret = MfaSecret::from_bytes(*b"12345678901234567890");
    let encrypted = old.seal("synthetic-user", &secret).unwrap();
    let current = AccountMfaKeyring::new(
        "current".into(),
        vec![("old".into(), [1; 32]), ("current".into(), [2; 32])],
    )
    .unwrap();
    assert_eq!(
        current
            .open("synthetic-user", &encrypted)
            .unwrap()
            .code(42)
            .unwrap(),
        secret.code(42).unwrap()
    );
    let rotated = current
        .seal(
            "synthetic-user",
            &current.open("synthetic-user", &encrypted).unwrap(),
        )
        .unwrap();
    assert!(rotated.starts_with("mfa1.current."));
    assert!(old.open("synthetic-user", &rotated).is_err());
    assert!(current.open("different-user", &encrypted).is_err());
    let mut changed = encrypted.into_bytes();
    let last = changed.len() - 1;
    changed[last] = if changed[last] == b'0' { b'1' } else { b'0' };
    assert!(
        current
            .open("synthetic-user", std::str::from_utf8(&changed).unwrap())
            .is_err()
    );
    for envelope in ["iv.tag.data", "v1.old.00.00", "mfa1.unknown.00.00", ""] {
        assert!(current.open("synthetic-user", envelope).is_err());
    }
    assert!(AccountMfaKeyring::new("missing".into(), vec![("old".into(), [1; 32])]).is_err());
}
#[test]
fn code_shape_step_order_and_strict_freshness_boundaries_match_source() {
    for code in ["000000", "999999"] {
        assert!(valid_code(code));
    }
    for code in ["12345", "1234567", "１２３４５６", " 123456", "12345a", ""] {
        assert!(!valid_code(code));
    }
    assert_eq!(candidate_counters(60_000), [1, 2, 3]);
    assert!(recent_authentication(1_000_000, 1_300_000).is_ok());
    assert!(recent_authentication(999_999, 1_300_000).is_err());
    assert!(recent_authentication(1_300_001, 1_300_000).is_err());
    assert_eq!(
        fresh_until(true, 1_000_001, 1_300_000, 1_400_000).unwrap(),
        1_300_001
    );
    assert!(fresh_until(true, 1_000_000, 1_300_000, 1_400_000).is_err());
    assert!(fresh_until(true, 1_300_001, 1_300_000, 1_400_000).is_err());
    assert_eq!(
        fresh_until(true, 1_300_000, 1_300_000, 1_300_100).unwrap(),
        1_300_100
    );
}
#[test]
fn unchanged_node_source_oracle_covers_totp_uri_and_private_source_revision_hashes() {
    use erpx_server_identity_access::public::account_security_http::AccountSecuritySeed;
    use serde_json::Value;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../fixtures/identity_access/account-security/conformance.v1.json"
    ))
    .unwrap();
    assert_eq!(fixture["node"], "v26.11.1");
    let secret = MfaSecret::from_bytes(*b"12345678901234567890");
    for row in fixture["vectors"].as_array().unwrap() {
        assert_eq!(
            secret.code(row["counter"].as_i64().unwrap()).unwrap(),
            row["expected"].as_str().unwrap()
        );
    }
    for row in fixture["codes"].as_array().unwrap() {
        assert_eq!(
            valid_code(row["code"].as_str().unwrap()),
            row["expected"].as_bool().unwrap()
        );
    }
    for row in fixture["enrollments"].as_array().unwrap() {
        assert_eq!(
            enrollment_uri(
                row["userId"].as_str().unwrap(),
                row["secret"].as_str().unwrap()
            ),
            row["uri"].as_str().unwrap()
        );
    }
    for row in fixture["snapshots"].as_array().unwrap() {
        let mut image = row["image"].clone();
        for session in image["sessions"].as_array_mut().unwrap() {
            let millis = time::OffsetDateTime::parse(
                session["expires_at"].as_str().unwrap(),
                &time::format_description::well_known::Rfc3339,
            )
            .unwrap()
            .unix_timestamp_nanos()
                / 1_000_000;
            session["expires_ms"] = serde_json::json!(i64::try_from(millis).unwrap());
        }
        let source: AccountSecuritySource = serde_json::from_value(image).unwrap();
        let actor: AccountSecurityActor = serde_json::from_value(row["actor"].clone()).unwrap();
        let seed = project_account_security_snapshot(&source, &actor, row["now"].as_i64().unwrap())
            .unwrap();
        let actual = serde_json::to_value(AccountSecuritySeed::try_from(seed).unwrap()).unwrap();
        assert_eq!(actual, row["expected"], "source oracle {}", row["id"]);
    }
    for row in fixture["displayBoundaries"].as_array().unwrap() {
        let mut image = row["image"].clone();
        for session in image["sessions"].as_array_mut().unwrap() {
            session["expires_ms"] = serde_json::json!(row["now"].as_i64().unwrap() + 43_200_000);
        }
        let source: AccountSecuritySource = serde_json::from_value(image).unwrap();
        let actor: AccountSecurityActor = serde_json::from_value(row["actor"].clone()).unwrap();
        let seed = project_account_security_snapshot(&source, &actor, row["now"].as_i64().unwrap())
            .unwrap();
        assert_eq!(
            seed.revision,
            row["expectedRevision"].as_str().unwrap(),
            "display normalization never alters the full raw source hash"
        );
        assert_eq!(
            seed.families[0].user_agent.as_deref(),
            row["reviewedNativeDisplay"].as_str()
        );
        let b0 = row["expectedB0Json"].as_str().unwrap();
        if row["length"] == 499 {
            assert!(b0.ends_with("\\ud83d\""));
            assert!(!seed.families[0].user_agent.as_ref().unwrap().contains('😀'));
        }
    }
}
