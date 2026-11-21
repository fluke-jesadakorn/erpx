#[path = "../../support/rust-directory-ldaps.rs"]
mod harness;
use erpx_server_identity_access::public::directory::*;
use harness::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::time::{sleep, timeout};
use tracing::instrument::WithSubscriber;

async fn settled(server: &Server, expected: impl Fn(&Stats) -> bool) {
    timeout(Duration::from_secs(2), async {
        loop {
            if expected(&server.stats.lock().unwrap()) {
                break;
            }
            sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("expected synthetic protocol boundary was observed");
}

#[tokio::test]
async fn actual_private_ca_sni_service_lookup_and_user_bind_preserve_immutable_subject_only() {
    let server = Server::start(Scenario::Complete, false, None).await;
    let fixture = Fixture::new(
        server.port,
        false,
        Some(&material().ca),
        "ldap.fixture.test",
    )
    .await;
    let subject = fixture
        .provider
        .verify(
            &fixture.descriptor,
            "alice",
            DirectoryPassword::new(USER_PASSWORD.into()).unwrap(),
        )
        .await
        .unwrap();
    assert!(
        subject.connection_id() == CONNECTION
            && subject.external_id() == "alice-id"
            && subject.matches(&fixture.descriptor)
    );
    assert!(
        fixture
            .provider
            .verify(
                &fixture.descriptor,
                "alice",
                DirectoryPassword::new("wrong-synthetic-password".into()).unwrap()
            )
            .await
            .is_err()
    );
    settled(&server, |s| s.service_binds == 2 && s.user_binds == 2).await;
    {
        let stats = server.stats.lock().unwrap();
        assert!(stats.connections == 2 && stats.sni_ok == 2 && stats.searches == 2);
    }
    server.finish().await;
}

#[tokio::test]
async fn incorrect_ca_name_or_absent_private_ca_never_falls_back_to_unverified_tls() {
    for (ca, name) in [
        (Some(material().ca.as_str()), "wrong.fixture.test"),
        (Some(material().wrong_ca.as_str()), "ldap.fixture.test"),
        (None, "ldap.fixture.test"),
    ] {
        let server = Server::start(Scenario::Complete, false, None).await;
        let fixture = Fixture::new(server.port, false, ca, name).await;
        assert!(
            timeout(
                Duration::from_secs(3),
                fixture.provider.verify(
                    &fixture.descriptor,
                    "alice",
                    DirectoryPassword::new(USER_PASSWORD.into()).unwrap()
                )
            )
            .await
            .unwrap()
            .is_err()
        );
        settled(&server, |s| s.tls_failed == 1).await;
        assert!(server.stats.lock().unwrap().service_binds == 0);
        server.finish().await;
    }
}

#[tokio::test]
async fn a_profile_for_a_different_connection_is_denied_before_opening_a_socket() {
    let server = Server::start(Scenario::Complete, false, None).await;
    let mut fixture = Fixture::new(
        server.port,
        false,
        Some(&material().ca),
        "ldap.fixture.test",
    )
    .await;
    fixture.descriptor.connection_id = "unapproved-connection".into();
    assert!(
        fixture
            .provider
            .verify(
                &fixture.descriptor,
                "alice",
                DirectoryPassword::new(USER_PASSWORD.into()).unwrap()
            )
            .await
            .is_err()
    );
    assert!(server.stats.lock().unwrap().connections == 0);
    server.finish().await;
}

#[tokio::test]
async fn complete_paged_observation_retains_membership_hierarchy_and_no_password_in_results() {
    let server = Server::start(Scenario::Paged, false, None).await;
    let fixture = Fixture::new(
        server.port,
        false,
        Some(&material().ca),
        "ldap.fixture.test",
    )
    .await;
    let snapshot = fixture
        .provider
        .observe(&fixture.descriptor, NOW)
        .await
        .unwrap();
    assert!(
        snapshot.observed_at() == NOW
            && snapshot.users().len() == 2
            && snapshot.groups().len() == 1
            && snapshot.hierarchy().containers.len() == 1
            && snapshot.hierarchy().placements.len() == 3
    );
    assert!(
        snapshot
            .users()
            .iter()
            .all(|u| u.enabled && u.group_ids == ["everyone-id"])
    );
    settled(&server, |s| s.unbinds == 1).await;
    {
        let stats = server.stats.lock().unwrap();
        assert!(stats.searches == 4 && stats.service_binds == 1 && stats.user_binds == 0);
    }
    server.finish().await;
}

#[tokio::test]
async fn repeated_or_missing_paging_cookies_and_total_page_limit_never_finalize_partial_snapshots()
{
    for (scenario, searches) in [
        (Scenario::RepeatedCookie, 2),
        (Scenario::MissingCookie, 1),
        (Scenario::PageLimit, 100),
    ] {
        let server = Server::start(scenario, false, None).await;
        let fixture = Fixture::new(
            server.port,
            false,
            Some(&material().ca),
            "ldap.fixture.test",
        )
        .await;
        assert!(
            timeout(
                Duration::from_secs(12),
                fixture.provider.observe(&fixture.descriptor, NOW)
            )
            .await
            .unwrap()
            .is_err()
        );
        assert!(server.stats.lock().unwrap().searches == searches);
        server.finish().await;
    }
}

#[tokio::test]
async fn ucs_referral_and_truncated_success_stream_are_not_a_complete_snapshot_or_new_endpoint() {
    let forbidden = listener().await;
    let port = forbidden.local_addr().unwrap().port();
    for scenario in [Scenario::UcsReferral, Scenario::Truncated] {
        let server = Server::start(scenario, false, Some(port)).await;
        let fixture = Fixture::new(
            server.port,
            false,
            Some(&material().ca),
            "ldap.fixture.test",
        )
        .await;
        assert!(
            timeout(
                Duration::from_secs(3),
                fixture.provider.observe(&fixture.descriptor, NOW)
            )
            .await
            .unwrap()
            .is_err()
        );
        assert!(server.stats.lock().unwrap().searches == 1);
        server.finish().await;
    }
    assert!(
        timeout(Duration::from_millis(100), forbidden.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn only_main_ad_partition_referrals_are_ignored_and_never_followed() {
    let forbidden = listener().await;
    let port = forbidden.local_addr().unwrap().port();
    let server = Server::start(Scenario::AdReferral, false, Some(port)).await;
    let fixture = Fixture::new(server.port, true, Some(&material().ca), "ldap.fixture.test").await;
    let snapshot = fixture
        .provider
        .observe(&fixture.descriptor, NOW)
        .await
        .unwrap();
    assert!(
        snapshot.users().len() == 1 && snapshot.groups().len() == 1 && snapshot.users()[0].enabled
    );
    assert!(snapshot.users()[0].external_id == "01010101-0101-0101-0101-010101010101");
    assert!(
        timeout(Duration::from_millis(100), forbidden.accept())
            .await
            .is_err()
    );
    server.finish().await;
}

#[tokio::test]
async fn every_ad_member_range_must_finish_without_referral_or_a_missing_continuation() {
    for scenario in [
        Scenario::AdRangeComplete,
        Scenario::AdRangeMissing,
        Scenario::AdRangeReferral,
    ] {
        let server = Server::start(scenario, false, None).await;
        let fixture =
            Fixture::new(server.port, true, Some(&material().ca), "ldap.fixture.test").await;
        let result = fixture.provider.observe(&fixture.descriptor, NOW).await;
        if scenario == Scenario::AdRangeComplete {
            let snapshot = result.unwrap();
            assert!(snapshot.users().len() == 1 && snapshot.users()[0].group_ids.len() == 1);
        } else {
            assert!(result.is_err());
        }
        assert!(server.stats.lock().unwrap().ranges == 1);
        server.finish().await;
    }
}

#[tokio::test]
async fn unsolicited_ids_malformed_ber_and_declared_length_bombs_fail_before_body_wait() {
    for scenario in [
        Scenario::WrongMessage,
        Scenario::Unsolicited,
        Scenario::DeclaredBomb,
        Scenario::Indefinite,
        Scenario::Deep,
        Scenario::MalformedPrimitive,
    ] {
        let server = Server::start(scenario, false, None).await;
        let fixture = Fixture::new(
            server.port,
            false,
            Some(&material().ca),
            "ldap.fixture.test",
        )
        .await;
        let result = timeout(
            Duration::from_secs(1),
            fixture.provider.verify(
                &fixture.descriptor,
                "alice",
                DirectoryPassword::new(USER_PASSWORD.into()).unwrap(),
            ),
        )
        .await
        .expect("bad frame must not wait for the fifteen-second body deadline");
        assert!(result.is_err());
        assert!(server.stats.lock().unwrap().searches == 0);
        server.finish().await;
    }
}

#[tokio::test]
async fn active_observation_retains_its_profile_generation_while_explicit_reload_waits() {
    let server = Server::start(Scenario::Complete, true, None).await;
    let fixture = Fixture::new(
        server.port,
        false,
        Some(&material().ca),
        "ldap.fixture.test",
    )
    .await;
    let provider = fixture.provider.clone();
    let descriptor = fixture.descriptor.clone();
    let observing = tokio::spawn(async move { provider.observe(&descriptor, NOW).await });
    timeout(Duration::from_secs(3), server.started.notified())
        .await
        .unwrap();
    write_profile(
        fixture.directory.path(),
        Some(&material().ca),
        "ldap.fixture.test",
        true,
    );
    let profiles = fixture.profiles.clone();
    let reloading = tokio::spawn(async move { profiles.reload().await });
    // Observe actual write-queue priority, rather than assuming a slow task
    // proves that its profile-file I/O has completed and its writer is waiting.
    timeout(Duration::from_secs(2), async {
        loop {
            if timeout(
                Duration::from_millis(10),
                fixture.profiles.fingerprint("fixture", CONNECTION),
            )
            .await
            .is_err()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual reload writer queued behind the retained provider generation");
    assert!(!reloading.is_finished());
    server.resume.notify_one();
    let snapshot = timeout(Duration::from_secs(3), observing)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(snapshot.users()[0].external_id == "alice-id");
    timeout(Duration::from_secs(3), reloading)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        fixture
            .profiles
            .fingerprint("fixture", CONNECTION)
            .await
            .unwrap()
            != fixture.descriptor.profile_fingerprint
    );
    assert!(
        fixture
            .provider
            .observe(&fixture.descriptor, NOW)
            .await
            .is_err()
    );
    assert!(server.stats.lock().unwrap().connections == 1);
    server.finish().await;
}

#[derive(Clone)]
struct Capture(Arc<AtomicUsize>);
impl tracing::Subscriber for Capture {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        metadata.target().starts_with("ldap3_proto") || metadata.target().starts_with("ldap3_lber")
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, _: &tracing::Event<'_>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}
#[tokio::test]
async fn actual_upstream_codec_tracing_is_silenced_inside_success_and_malformed_provider_calls() {
    use bytes::BytesMut;
    use tokio_util::codec::Decoder;
    let baseline = Arc::new(AtomicUsize::new(0));
    let mut codec = ldap3_proto::LdapCodec::default();
    let invalid = tracing::subscriber::with_default(Capture(baseline.clone()), || {
        codec.decode(&mut BytesMut::from(&[0x30, 4, 0x01, 2, 0x7f, 0x7f][..]))
    });
    assert!(invalid.is_err() && baseline.load(Ordering::SeqCst) > 0);
    for scenario in [Scenario::Complete, Scenario::MalformedPrimitive] {
        let server = Server::start(scenario, false, None).await;
        let fixture = Fixture::new(
            server.port,
            false,
            Some(&material().ca),
            "ldap.fixture.test",
        )
        .await;
        let captured = Arc::new(AtomicUsize::new(0));
        let result = fixture
            .provider
            .verify(
                &fixture.descriptor,
                "alice",
                DirectoryPassword::new(USER_PASSWORD.into()).unwrap(),
            )
            .with_subscriber(Capture(captured.clone()))
            .await;
        assert!(result.is_ok() == (scenario == Scenario::Complete));
        assert!(captured.load(Ordering::SeqCst) == 0);
        server.finish().await;
    }
}

#[tokio::test]
async fn a_search_entry_flood_is_rejected_before_any_later_snapshot_search() {
    let server = Server::start(Scenario::RowFlood, false, None).await;
    let fixture = Fixture::new(
        server.port,
        false,
        Some(&material().ca),
        "ldap.fixture.test",
    )
    .await;
    assert!(
        timeout(
            Duration::from_secs(8),
            fixture.provider.observe(&fixture.descriptor, NOW)
        )
        .await
        .unwrap()
        .is_err()
    );
    assert!(server.stats.lock().unwrap().searches == 1);
    server.finish().await;
}
