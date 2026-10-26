use erpx_server_people_payroll::public::{identity_facts::*, invitation_person_link::*};
use erpx_server_shared::public::{CompanyId, RecordId, TimestampMillis, UserId};

fn request() -> InvitationPersonLinkRequest {
    InvitationPersonLinkRequest {
        company_id: CompanyId::new("company").unwrap(),
        person_id: RecordId::new("person").unwrap(),
        user_id: UserId::new("user").unwrap(),
        expected_person_version: 3,
        engagement_id: RecordId::new("engagement").unwrap(),
        expected_engagement_version: 7,
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: Some(RecordId::new("department").unwrap()),
        now: TimestampMillis::new(100),
    }
}
fn person() -> PersonAuthorityFacts {
    PersonAuthorityFacts {
        company_id: CompanyId::new("company").unwrap(),
        person_id: RecordId::new("person").unwrap(),
        user_id: None,
        status: "active".into(),
        merged_into_id: None,
        version: 3,
        active_engagements: vec![EngagementAuthorityWitness {
            id: RecordId::new("engagement").unwrap(),
            version: 7,
            branch_id: Some(RecordId::new("branch").unwrap()),
            department_id: Some(RecordId::new("department").unwrap()),
        }],
    }
}
#[test]
fn invitation_link_plan_is_versioned_scoped_and_idempotent_for_the_same_user() {
    let request = request();
    let mut person = person();
    let witness = invitation_person_link_plan(&request, &person).unwrap();
    assert!(witness.changed);
    assert_eq!(witness.person_version, 4);
    person.user_id = Some(request.user_id.clone());
    person.version = 4;
    let again = invitation_person_link_plan(&request, &person).unwrap();
    assert!(!again.changed);
    assert_eq!(again.person_version, 4);
    for case in 0..9 {
        let mut person = self::person();
        match case {
            0 => person.version = 4,
            1 => person.user_id = Some(UserId::new("different").unwrap()),
            2 => person.company_id = CompanyId::new("foreign").unwrap(),
            3 => person.status = "inactive".into(),
            4 => person.merged_into_id = Some(RecordId::new("merged").unwrap()),
            5 => person.active_engagements.clear(),
            6 => person.active_engagements[0].version += 1,
            7 => person.active_engagements[0].branch_id = None,
            8 => person.active_engagements[0].department_id = None,
            _ => unreachable!(),
        }
        assert!(
            invitation_person_link_plan(&request, &person).is_err(),
            "case {case}"
        );
    }
}
