//! Native People roster Position-exit plan facts.
//!
//! These are owner unit facts: bounded plan validation, the retained no-op for
//! an empty engagement list, the exact phase order including the Collaboration
//! notice interleaves, and the retained SQL predicates. PostgreSQL behaviour is
//! proven by the sibling `roster_position_exit_postgres` integration target.
use erpx_server_people_payroll::public::roster_position_exit::{
    ROSTER_POSITION_EXIT_MAX_ENGAGEMENTS, ROSTER_POSITION_EXIT_ORDER, RosterPositionExitPhase,
    affected_assignments, pending_position_requests, roster_position_exit_order,
    roster_position_exit_plan,
};
use erpx_server_shared::public::{CompanyId, TimestampMillis};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn ids() -> Vec<String> {
    vec!["engagement-a".into(), "engagement-b".into()]
}

#[test]
fn empty_engagement_list_stays_the_retained_no_op() {
    assert!(
        roster_position_exit_plan(
            &company(),
            "owner",
            &[],
            "roster-exit-command-1",
            "Close the Position",
            NOW,
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn plan_keeps_the_exact_engagement_list_and_trims_the_reason() {
    let plan = roster_position_exit_plan(
        &company(),
        "owner",
        &ids(),
        "roster-exit-command-1",
        "  Close the Position  ",
        NOW,
    )
    .unwrap()
    .unwrap();
    assert_eq!(plan.engagement_ids, ids());
    assert_eq!(plan.reason, "Close the Position");
    assert_eq!(plan.command_id, "roster-exit-command-1");
    assert_eq!(plan.actor_user_id, "owner");
    assert_eq!(plan.now, NOW);
    assert_eq!(plan.company_id.as_str(), "company");
}

#[test]
fn invalid_inputs_fail_closed_before_any_sql() {
    let base = ids();
    let long_reason = "x".repeat(1_001);
    let padded = vec![" engagement-a".to_owned()];
    let too_many: Vec<String> = (0..=ROSTER_POSITION_EXIT_MAX_ENGAGEMENTS)
        .map(|index| format!("engagement-{index}"))
        .collect();
    let cases: Vec<(&[String], &str, &str, &str, TimestampMillis)> = vec![
        (
            base.as_slice(),
            "",
            "roster-exit-command-1",
            "Close it",
            NOW,
        ),
        (base.as_slice(), "owner", "short", "Close it", NOW),
        (
            base.as_slice(),
            "owner",
            "roster exit command",
            "Close it",
            NOW,
        ),
        (base.as_slice(), "owner", "roster-exit-command-1", "no", NOW),
        (
            base.as_slice(),
            "owner",
            "roster-exit-command-1",
            long_reason.as_str(),
            NOW,
        ),
        (
            padded.as_slice(),
            "owner",
            "roster-exit-command-1",
            "Close it",
            NOW,
        ),
        (
            too_many.as_slice(),
            "owner",
            "roster-exit-command-1",
            "Close it",
            NOW,
        ),
        (
            base.as_slice(),
            "owner",
            "roster-exit-command-1",
            "Close it",
            TimestampMillis::new(-1),
        ),
    ];
    for (engagement_ids, actor, command, reason, now) in cases {
        let error =
            roster_position_exit_plan(&company(), actor, engagement_ids, command, reason, now)
                .unwrap_err();
        assert_eq!(error.code, "ROSTER_POSITION_EXIT_INVALID");
    }
}

#[test]
fn phase_order_interleaves_only_the_two_collaboration_notices() {
    let order = roster_position_exit_order();
    assert_eq!(order, ROSTER_POSITION_EXIT_ORDER.as_slice());
    assert_eq!(
        order.iter().map(|phase| phase.label()).collect::<Vec<_>>(),
        vec![
            "locks",
            "weeks",
            "assignment.released.position",
            "notice.roster_week",
            "assignments.released",
            "request.invalidated.position",
            "notice.roster_request",
            "requests.invalidated",
        ]
    );
    assert_eq!(
        order
            .iter()
            .filter(|phase| !phase.people_owned())
            .copied()
            .collect::<Vec<_>>(),
        vec![
            RosterPositionExitPhase::WeekNotice,
            RosterPositionExitPhase::RequestNotice,
        ]
    );
}

#[test]
fn affected_predicate_is_the_retained_future_active_assignment_selection() {
    let sql = affected_assignments("$2", "$6", "$5");
    assert!(sql.contains("FROM people_roster_assignments a"));
    assert!(sql.contains("JOIN people_roster_shifts s ON s.id=a.shift_id"));
    assert!(sql.contains("JOIN people_roster_weeks w ON w.id=s.week_id"));
    assert!(sql.contains("w.company_id=$2"));
    assert!(sql.contains("a.engagement_id=ANY($6::text[])"));
    assert!(sql.contains("a.state='assigned'"));
    assert!(sql.contains("s.state='active'"));
    assert!(sql.contains("s.starts_at>$5"));
    assert!(!sql.contains("a.state='released'"));
}

#[test]
fn request_predicate_keeps_pending_states_and_every_engagement_join() {
    let sql = pending_position_requests("$2", "$6");
    assert!(sql.contains("r.company_id=$2"));
    assert!(sql.contains("r.state IN ('pending_coworker','pending_manager')"));
    assert!(sql.contains(
        "r.requester_person_id IN (SELECT person_id FROM hr_engagements WHERE id=ANY($6::text[]))"
    ));
    assert!(sql.contains(
        "r.coworker_person_id IN (SELECT person_id FROM hr_engagements WHERE id=ANY($6::text[]))"
    ));
    assert!(sql.contains(
        "r.source_assignment_id IN (SELECT id FROM people_roster_assignments WHERE engagement_id=ANY($6::text[]))"
    ));
    assert!(sql.contains(
        "r.target_assignment_id IN (SELECT id FROM people_roster_assignments WHERE engagement_id=ANY($6::text[]))"
    ));
}
