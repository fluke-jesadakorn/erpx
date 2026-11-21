use erpx_server_commercial::public::purchases::{
    CompanyPackageGrant, CompanyPackageHistoryRight, package_grant_revision,
};
use erpx_server_commercial::public::subscriptions::*;
use erpx_server_shared::public::{CompanyId, TimestampMillis};

fn now(value: i64) -> TimestampMillis {
    TimestampMillis::new(value)
}

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn grant(state: &str, version: i64, starts_at: i64, ends_at: Option<i64>) -> CompanyPackageGrant {
    CompanyPackageGrant {
        company_id: "company".into(),
        package_id: "legal".into(),
        package_version: 1,
        state: state.into(),
        starts_at,
        ends_at,
        version,
        reason: "Reviewed package grant".into(),
        evidence: "PRIVATE_EVIDENCE".into(),
    }
}

fn transition(state: &str, version: i64, recorded_at: i64) -> SubscriptionTransition {
    SubscriptionTransition {
        package_id: "legal".into(),
        package_version: 1,
        state: state.into(),
        starts_at: 0,
        ends_at: Some(3000),
        version,
        recorded_at,
    }
}

fn facts(
    grants: Vec<CompanyPackageGrant>,
    history: Vec<CompanyPackageHistoryRight>,
    transitions: Vec<SubscriptionTransition>,
) -> SubscriptionFacts {
    SubscriptionFacts {
        grants,
        history,
        transitions,
    }
}

#[test]
fn active_period_reports_its_renewal_boundary_without_grace() {
    let grants = vec![grant("active", 1, 0, Some(3000))];
    let read = current_subscriptions(
        &company(),
        &facts(grants.clone(), Vec::new(), Vec::new()),
        now(1000),
    )
    .unwrap();
    assert_eq!(read.company_id, "company");
    assert_eq!(read.observed_at, 1000);
    assert_eq!(
        read.revision,
        package_grant_revision("company", &grants).unwrap()
    );
    let subscription = &read.subscriptions[0];
    assert_eq!(subscription.state, SubscriptionState::Active);
    assert_eq!(subscription.grant_state, "active");
    assert_eq!(subscription.renewal_at, Some(3000));
    assert!(subscription.grace.is_none());
    assert!(subscription.next_action.is_none());
}

#[test]
fn expiry_becomes_an_unbounded_grace_window_and_a_restore_action() {
    let grants = vec![grant("active", 1, 0, Some(3000))];
    let read = current_subscriptions(
        &company(),
        &facts(grants, Vec::new(), Vec::new()),
        now(3000),
    )
    .unwrap();
    let subscription = &read.subscriptions[0];
    assert_eq!(subscription.state, SubscriptionState::Grace);
    assert_eq!(subscription.renewal_at, Some(3000));
    assert_eq!(
        subscription.grace,
        Some(GraceWindow {
            starts_at: 3000,
            ends_at: None,
            remaining_ms: None,
        })
    );
    assert_eq!(subscription.next_action, Some(SubscriptionAction::Restore));

    // Draining without a transition still reports a grace window from its
    // retained end boundary.
    let read = current_subscriptions(
        &company(),
        &facts(
            vec![grant("draining", 2, 0, Some(3000))],
            Vec::new(),
            Vec::new(),
        ),
        now(1000),
    )
    .unwrap();
    assert_eq!(read.subscriptions[0].state, SubscriptionState::Grace);
    assert_eq!(
        read.subscriptions[0].grace.as_ref().unwrap().starts_at,
        3000
    );
}

#[test]
fn draining_start_is_taken_from_retained_transition_history() {
    let read = current_subscriptions(
        &company(),
        &facts(
            vec![grant("draining", 2, 0, Some(3000))],
            Vec::new(),
            vec![transition("active", 1, 500), transition("draining", 2, 900)],
        ),
        now(1000),
    )
    .unwrap();
    assert_eq!(read.subscriptions[0].state, SubscriptionState::Grace);
    assert_eq!(read.subscriptions[0].grace.as_ref().unwrap().starts_at, 900);
    assert_eq!(
        read.subscriptions[0].next_action,
        Some(SubscriptionAction::Restore)
    );
}

#[test]
fn inactive_period_is_expired_but_restorable() {
    let read = current_subscriptions(
        &company(),
        &facts(
            vec![grant("inactive", 3, 0, Some(3000))],
            Vec::new(),
            Vec::new(),
        ),
        now(4000),
    )
    .unwrap();
    assert_eq!(read.subscriptions[0].state, SubscriptionState::Expired);
    assert!(read.subscriptions[0].grace.is_none());
    assert_eq!(
        read.subscriptions[0].next_action,
        Some(SubscriptionAction::Restore)
    );
}

#[test]
fn active_after_a_retained_drain_is_reported_as_recovered() {
    let read = current_subscriptions(
        &company(),
        &facts(
            vec![grant("active", 3, 0, Some(3000))],
            Vec::new(),
            vec![transition("draining", 2, 500)],
        ),
        now(1000),
    )
    .unwrap();
    assert_eq!(read.subscriptions[0].state, SubscriptionState::Recovered);
    assert!(read.subscriptions[0].next_action.is_none());
    assert!(read.subscriptions[0].grace.is_none());
}

#[test]
fn history_rows_and_transitions_are_company_scoped_bounded_and_ordered() {
    let history = vec![CompanyPackageHistoryRight {
        company_id: "company".into(),
        package_id: "legal".into(),
        package_version: 1,
        source_grant_version: 1,
        source_event_id: "event".into(),
        retained_at: 500,
    }];
    let read = current_subscriptions(
        &company(),
        &facts(
            vec![grant("active", 2, 0, Some(3000))],
            history,
            vec![transition("active", 1, 500), transition("active", 2, 900)],
        ),
        now(1000),
    )
    .unwrap();
    assert_eq!(read.history.len(), 1);
    assert_eq!(read.history[0].company_id, "company");
    assert_eq!(read.transitions.len(), 2);
    assert_eq!(read.transitions[0].recorded_at, 500);
    assert_eq!(read.transitions[1].recorded_at, 900);
}

#[test]
fn malformed_or_foreign_facts_fail_closed() {
    let foreign = facts(
        vec![CompanyPackageGrant {
            company_id: "foreign".into(),
            ..grant("active", 1, 0, None)
        }],
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(
        current_subscriptions(&company(), &foreign, now(1000))
            .unwrap_err()
            .code,
        "COMMERCIAL_SUBSCRIPTION_FACTS_INVALID"
    );

    for invalid in [
        grant("unknown", 1, 0, None),
        grant("active", 0, 0, None),
        grant("active", 1, -1, None),
        grant("active", 1, 0, Some(0)),
    ] {
        assert_eq!(
            current_subscriptions(
                &company(),
                &facts(vec![invalid], Vec::new(), Vec::new()),
                now(1000),
            )
            .unwrap_err()
            .code,
            "COMMERCIAL_SUBSCRIPTION_FACTS_INVALID"
        );
    }

    let duplicate = facts(
        vec![grant("active", 1, 0, None), grant("draining", 2, 0, None)],
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(
        current_subscriptions(&company(), &duplicate, now(1000))
            .unwrap_err()
            .code,
        "COMMERCIAL_SUBSCRIPTION_FACTS_INVALID"
    );

    let mut invalid_transition = transition("draining", 1, 500);
    invalid_transition.state = "paused".into();
    assert_eq!(
        current_subscriptions(
            &company(),
            &facts(
                vec![grant("active", 1, 0, None)],
                Vec::new(),
                vec![invalid_transition],
            ),
            now(1000),
        )
        .unwrap_err()
        .code,
        "COMMERCIAL_SUBSCRIPTION_FACTS_INVALID"
    );
}
