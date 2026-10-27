use erpx_server_identity_access::public::directory::provenance::*;
use std::collections::{BTreeMap, BTreeSet};
fn keys() -> ProvenanceKeyRing {
    ProvenanceKeyRing::from_keys(
        "local".into(),
        Vec::from([
            ("synthetic_key_a".into(), [0x31; 32]),
            ("synthetic_key_b".into(), [0x72; 32]),
        ]),
    )
    .unwrap()
}
fn material(password: &str) -> (BTreeSet<String>, BTreeMap<String, String>, &str) {
    (
        BTreeSet::from(["directory.fixture.invalid".into()]),
        BTreeMap::from([("new-id".into(), "old-id".into())]),
        password,
    )
}
fn tag(password: &str, key: &str, connection: &str) -> ProfileTag {
    let (hosts, aliases, password) = material(password);
    selected_profile_tag(
        &keys(),
        key,
        &SourceNamespace {
            environment: "local",
            holding_id: Some("h001"),
            company_id: "company",
            connection_id: connection,
            profile_name: "fixture_a",
        },
        &SelectedProfile {
            allowed_hosts: &hosts,
            bind_dn: Some("cn=synthetic-reader"),
            bind_password: Some(password),
            ca_pem: None,
            tls_server_name: Some("directory.fixture.invalid"),
            identity_aliases: &aliases,
            allows_selected_connection: true,
        },
    )
    .unwrap()
}
fn source() -> SelectedSource {
    SelectedSource {
        revision: SourceRevision::checked(7).unwrap(),
        key_id: "synthetic_key_a".into(),
        profile_tag: tag("synthetic-password", "synthetic_key_a", "connection"),
        connection_version: 3,
        config_fingerprint: [0x44; 32],
    }
}
fn local(source: &SelectedSource) -> CurrentLocalSource {
    CurrentLocalSource {
        key_id: source.key_id.clone(),
        profile_tag: source.profile_tag,
        connection_version: source.connection_version,
        config_fingerprint: source.config_fingerprint,
    }
}
fn observation(source: &SelectedSource) -> ObservationProvenance {
    ObservationProvenance {
        source: source.clone(),
        observed_at: 1_000_000,
    }
}

#[test]
fn independent_instances_and_restart_compute_identical_selected_tags() {
    let a = tag("synthetic-password", "synthetic_key_a", "connection");
    let b = tag("synthetic-password", "synthetic_key_a", "connection");
    assert!(a == b);
}
#[test]
fn changed_selected_password_is_not_accepted_under_old_durable_selection() {
    let s = source();
    let mut l = local(&s);
    l.profile_tag = tag(
        "rotated-synthetic-password",
        "synthetic_key_a",
        "connection",
    );
    assert_eq!(
        classify(Some(&s), &l, Some(&observation(&s)), 1_000_000, 1_000_001),
        Err(Denial::LocalSourceMismatch)
    );
    // A mismatched instance produces no proposed registry update here.
    assert_eq!(s.revision.get(), 7);
}
#[test]
fn tags_cannot_be_replayed_across_connection_or_key_namespaces() {
    assert!(
        tag("synthetic-password", "synthetic_key_a", "connection")
            != tag(
                "synthetic-password",
                "synthetic_key_a",
                "foreign-connection"
            )
    );
    assert!(
        tag("synthetic-password", "synthetic_key_a", "connection")
            != tag("synthetic-password", "synthetic_key_b", "connection")
    );
}
#[test]
fn absent_binding_is_configuration_failure_while_missing_observation_requires_refresh() {
    let s = source();
    let l = local(&s);
    assert_eq!(
        classify(None, &l, None, 1_000_000, 1_000_001),
        Err(Denial::SourceNotApproved)
    );
    assert_eq!(
        classify(Some(&s), &l, None, 1_000_000, 1_000_001),
        Ok(ObservationUse::CompleteRefreshRequired)
    );
}
#[test]
fn already_fresh_observation_becomes_stale_on_durable_rotation() {
    let old = source();
    let evidence = observation(&old);
    let mut next = local(&old);
    next.profile_tag = tag(
        "rotated-synthetic-password",
        "synthetic_key_a",
        "connection",
    );
    let current = rotate(&old, old.revision, next).unwrap();
    assert_eq!(current.revision.get(), 8);
    assert_eq!(
        classify(
            Some(&current),
            &local(&current),
            Some(&evidence),
            1_000_000,
            1_000_001
        ),
        Ok(ObservationUse::CompleteRefreshRequired)
    );
}
#[test]
fn credential_rotation_back_to_old_material_never_revives_an_old_observation() {
    let old = source();
    let mut different = local(&old);
    different.profile_tag = tag("different", "synthetic_key_a", "connection");
    let middle = rotate(&old, old.revision, different).unwrap();
    let again = rotate(&middle, middle.revision, local(&old)).unwrap();
    assert_eq!(again.revision.get(), 9);
    assert!(again.profile_tag == old.profile_tag);
    assert_eq!(
        classify(
            Some(&again),
            &local(&again),
            Some(&observation(&old)),
            1_000_000,
            1_000_001
        ),
        Ok(ObservationUse::CompleteRefreshRequired)
    );
}
#[test]
fn explicit_invalidation_advances_revision_even_with_identical_material() {
    let old = source();
    let current = rotate(&old, old.revision, local(&old)).unwrap();
    assert_eq!(current.revision.get(), 8);
    assert_eq!(
        classify(
            Some(&current),
            &local(&current),
            Some(&observation(&old)),
            1_000_000,
            1_000_001
        ),
        Ok(ObservationUse::CompleteRefreshRequired)
    );
}
#[test]
fn stale_admin_compare_and_swap_cannot_replace_a_newer_source() {
    let old = source();
    let current = rotate(&old, old.revision, local(&old)).unwrap();
    assert!(matches!(
        rotate(&current, old.revision, local(&old)),
        Err(Denial::RevisionConflict)
    ));
}
#[test]
fn configuration_edit_requires_approved_matching_source_before_any_refresh() {
    let s = source();
    let mut l = local(&s);
    l.connection_version += 1;
    assert_eq!(
        classify(Some(&s), &l, None, 1_000_000, 1_000_001),
        Err(Denial::ConfigurationNotSelected)
    );
    l.connection_version = s.connection_version;
    l.config_fingerprint[0] ^= 1;
    assert_eq!(
        classify(Some(&s), &l, None, 1_000_000, 1_000_001),
        Err(Denial::ConfigurationNotSelected)
    );
}
#[test]
fn fresh_cutoff_and_identity_timestamp_mismatch_require_complete_refresh() {
    let s = source();
    let o = observation(&s);
    let l = local(&s);
    assert_eq!(
        classify(
            Some(&s),
            &l,
            Some(&o),
            o.observed_at,
            o.observed_at + 299_999
        ),
        Ok(ObservationUse::Fresh)
    );
    assert_eq!(
        classify(
            Some(&s),
            &l,
            Some(&o),
            o.observed_at,
            o.observed_at + 300_000
        ),
        Ok(ObservationUse::CompleteRefreshRequired)
    );
    assert_eq!(
        classify(Some(&s), &l, Some(&o), o.observed_at + 1, o.observed_at + 1),
        Ok(ObservationUse::CompleteRefreshRequired)
    );
}
#[test]
fn malformed_same_revision_evidence_and_future_timestamps_deny() {
    let s = source();
    let l = local(&s);
    let mut o = observation(&s);
    o.source.config_fingerprint[0] ^= 1;
    assert_eq!(
        classify(Some(&s), &l, Some(&o), o.observed_at, o.observed_at + 1),
        Err(Denial::InconsistentObservation)
    );
    o = observation(&s);
    assert_eq!(
        classify(Some(&s), &l, Some(&o), o.observed_at, o.observed_at - 1),
        Err(Denial::InvalidClock)
    );
}
#[test]
fn missing_retained_key_and_unapproved_profile_never_fall_back() {
    let (missing_hosts, missing_aliases, _) = material("unused");
    assert!(matches!(
        selected_profile_tag(
            &keys(),
            "missing",
            &SourceNamespace {
                environment: "local",
                holding_id: Some("h001"),
                company_id: "company",
                connection_id: "connection",
                profile_name: "fixture_a"
            },
            &SelectedProfile {
                allowed_hosts: &missing_hosts,
                bind_dn: None,
                bind_password: None,
                ca_pem: None,
                tls_server_name: None,
                identity_aliases: &missing_aliases,
                allows_selected_connection: true
            }
        ),
        Err(Denial::MissingKey)
    ));
    let (hosts, aliases, _) = material("unused");
    assert!(matches!(
        selected_profile_tag(
            &keys(),
            "synthetic_key_a",
            &SourceNamespace {
                environment: "local",
                holding_id: Some("h001"),
                company_id: "company",
                connection_id: "not-approved",
                profile_name: "fixture_a"
            },
            &SelectedProfile {
                allowed_hosts: &hosts,
                bind_dn: None,
                bind_password: None,
                ca_pem: None,
                tls_server_name: None,
                identity_aliases: &aliases,
                allows_selected_connection: false
            }
        ),
        Err(Denial::ProfileUnavailable)
    ));
}
#[test]
fn none_empty_and_length_delimited_credentials_are_not_ambiguous() {
    let (hosts, aliases, _) = material("");
    let namespace = SourceNamespace {
        environment: "local",
        holding_id: Some("h001"),
        company_id: "company",
        connection_id: "connection",
        profile_name: "fixture_a",
    };
    let mut p = SelectedProfile {
        allowed_hosts: &hosts,
        bind_dn: None,
        bind_password: None,
        ca_pem: None,
        tls_server_name: None,
        identity_aliases: &aliases,
        allows_selected_connection: true,
    };
    let none = selected_profile_tag(&keys(), "synthetic_key_a", &namespace, &p).unwrap();
    p.bind_password = Some("");
    let empty = selected_profile_tag(&keys(), "synthetic_key_a", &namespace, &p).unwrap();
    assert!(none != empty);
    p.bind_dn = Some("ab");
    p.bind_password = Some("c");
    let a = selected_profile_tag(&keys(), "synthetic_key_a", &namespace, &p).unwrap();
    p.bind_dn = Some("a");
    p.bind_password = Some("bc");
    let b = selected_profile_tag(&keys(), "synthetic_key_a", &namespace, &p).unwrap();
    assert!(a != b);
}
#[test]
fn revision_overflow_fails_instead_of_reusing_old_epoch() {
    let mut s = source();
    s.revision = SourceRevision::checked(i64::MAX).unwrap();
    assert!(matches!(
        rotate(&s, s.revision, local(&s)),
        Err(Denial::InvalidRevision)
    ));
}
#[test]
fn a_newer_timestamp_from_a_retired_source_cannot_skip_current_reconciliation() {
    let old = source();
    let previous = observation(&old);
    let current = rotate(&old, old.revision, local(&old)).unwrap();
    assert_eq!(
        snapshot_order(&current, Some(&previous), 999_999, 1_000_001),
        Ok(SnapshotOrder::ApplyComplete)
    );
    assert_eq!(
        snapshot_order(&old, Some(&previous), 999_999, 1_000_001),
        Ok(SnapshotOrder::RetainNewerFromSameSource)
    );
}
#[test]
fn independent_pinned_node_core_crypto_vector_matches_the_versioned_byte_protocol() {
    let actual = tag("synthetic-password", "synthetic_key_a", "connection");
    let actual_hex = actual
        .database_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let expected = "fd229c71067deacd958a857c1db0f01d5b14ca1d9644988d4e43f73330ddb33c";
    assert!(
        include_str!(
            "../../../fixtures/identity_access/directory/provenance/source-tag-v1-oracle.json"
        )
        .contains(expected)
    );
    assert_eq!(actual_hex, expected);
}
#[test]
fn control_namespace_is_explicitly_absent_holding_and_never_an_empty_fallback() {
    let (hosts, aliases, password) = material("synthetic-password");
    let profile = SelectedProfile {
        allowed_hosts: &hosts,
        bind_dn: Some("cn=synthetic-reader"),
        bind_password: Some(password),
        ca_pem: None,
        tls_server_name: Some("directory.fixture.invalid"),
        identity_aliases: &aliases,
        allows_selected_connection: true,
    };
    let namespace = SourceNamespace {
        environment: "local",
        holding_id: None,
        company_id: "platform",
        connection_id: "connection",
        profile_name: "fixture_a",
    };
    let control = selected_profile_tag(&keys(), "synthetic_key_a", &namespace, &profile).unwrap();
    assert!(control != tag("synthetic-password", "synthetic_key_a", "connection"));
    let invalid = SourceNamespace {
        holding_id: Some(""),
        ..namespace
    };
    assert!(matches!(
        selected_profile_tag(&keys(), "synthetic_key_a", &invalid, &profile),
        Err(Denial::ProfileUnavailable)
    ));
}
