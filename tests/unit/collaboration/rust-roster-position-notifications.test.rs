//! Native Collaboration roster Position-change notice facts.
//!
//! These are owner unit facts: bounded notice input, and the exact retained
//! `roster_week` / `roster_request` insert shape (record types, versions,
//! message text, joins and duplicate suppression). PostgreSQL receipt facts are
//! proven by the People roster Position-exit integration target.
use erpx_server_collaboration::public::roster_position_notifications::{
    ROSTER_POSITION_REQUEST_MESSAGE, ROSTER_POSITION_WEEK_MESSAGE,
    roster_position_request_notice_sql, roster_position_week_notice_sql,
    validate_roster_position_notice_input,
};
use erpx_server_shared::public::CompanyId;

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn ids() -> Vec<String> {
    vec!["engagement-a".into(), "engagement-b".into()]
}

#[test]
fn notice_input_is_bounded() {
    assert!(validate_roster_position_notice_input(&company(), &ids(), 1_800_000_000_000).is_ok());
    let too_many: Vec<String> = (0..=500)
        .map(|index| format!("engagement-{index}"))
        .collect();
    let cases: Vec<(Vec<String>, i64)> = vec![
        (Vec::new(), 0),
        (vec![" engagement-a".into()], 0),
        (too_many, 0),
        (ids(), -1),
        (ids(), 9_007_199_254_740_992),
    ];
    for (engagement_ids, now) in cases {
        assert_eq!(
            validate_roster_position_notice_input(&company(), &engagement_ids, now)
                .unwrap_err()
                .code,
            "ROSTER_POSITION_NOTICE_INVALID"
        );
    }
}

#[test]
fn week_notice_keeps_the_retained_recipient_version_and_message() {
    let sql = roster_position_week_notice_sql("$1", "$3", "$2");
    assert!(sql.contains("INSERT INTO staff_notifications"));
    assert!(sql.contains("gen_random_uuid()::text"));
    assert!(sql.contains("'roster_week'"));
    assert!(sql.contains(",a.week_id,w.version,"));
    assert!(sql.contains(ROSTER_POSITION_WEEK_MESSAGE));
    assert!(sql.contains("JOIN hr_people p ON p.id=a.person_id AND p.company_id=$1"));
    assert!(sql.contains("JOIN people_roster_weeks w ON w.id=a.week_id"));
    assert!(sql.contains("WHERE p.user_id IS NOT NULL ON CONFLICT DO NOTHING"));
    // The affected predicate is still the retained future active assignment one.
    assert!(sql.contains("a.engagement_id=ANY($3::text[])"));
    assert!(sql.contains("a.state='assigned'"));
    assert!(sql.contains("s.state='active'"));
    assert!(sql.contains("s.starts_at>$2"));
}

#[test]
fn request_notice_keeps_the_pending_state_and_post_invalidation_version() {
    let sql = roster_position_request_notice_sql("$1", "$3", "$2");
    assert!(sql.contains("INSERT INTO staff_notifications"));
    assert!(sql.contains("gen_random_uuid()::text"));
    assert!(sql.contains("'roster_request'"));
    assert!(sql.contains(",r.id,r.version+1,"));
    assert!(sql.contains(ROSTER_POSITION_REQUEST_MESSAGE));
    assert!(sql.contains(
        "JOIN hr_people p ON p.id IN (r.requester_person_id,r.coworker_person_id) AND p.company_id=$1"
    ));
    assert!(sql.contains("WHERE p.user_id IS NOT NULL ON CONFLICT DO NOTHING"));
    assert!(sql.contains("r.state IN ('pending_coworker','pending_manager')"));
    assert!(sql.contains(
        "r.requester_person_id IN (SELECT person_id FROM hr_engagements WHERE id=ANY($3::text[]))"
    ));
    assert!(sql.contains(
        "r.target_assignment_id IN (SELECT id FROM people_roster_assignments WHERE engagement_id=ANY($3::text[]))"
    ));
}
