use erpx_server_people_payroll::public::identity_facts::{
    EngagementAuthorityWitness, PersonAuthorityFacts,
};
use erpx_server_shared::public::{CompanyId, RecordId, UserId};

fn facts() -> PersonAuthorityFacts {
    PersonAuthorityFacts {
        company_id: CompanyId::new("company").unwrap(),
        person_id: RecordId::new("person").unwrap(),
        user_id: Some(UserId::new("user").unwrap()),
        status: "active".to_owned(),
        merged_into_id: None,
        version: 1,
        active_engagements: vec![EngagementAuthorityWitness {
            id: RecordId::new("engagement").unwrap(),
            version: 1,
        }],
    }
}

#[test]
fn hr_subject_requires_active_unmerged_person_and_current_engagement() {
    let mut person = facts();
    assert!(person.active_hr_subject());
    person.active_engagements.clear();
    assert!(!person.active_hr_subject());
    let mut person = facts();
    person.status = "inactive".to_owned();
    assert!(!person.active_hr_subject());
    let mut person = facts();
    person.merged_into_id = Some(RecordId::new("survivor").unwrap());
    assert!(!person.active_hr_subject());
    // A Person can exist without a login identity. This still grants no access.
    let mut person = facts();
    person.user_id = None;
    assert!(person.active_hr_subject());
}

#[test]
fn corrupt_versions_duplicate_engagements_and_self_merges_fail_closed() {
    let mut person = facts();
    person.version = 0;
    assert!(person.validate().is_err());
    let mut person = facts();
    person.status = "unknown".to_owned();
    assert!(person.validate().is_err());
    let mut person = facts();
    person.active_engagements[0].version = 0;
    assert!(person.validate().is_err());
    let mut person = facts();
    person
        .active_engagements
        .push(person.active_engagements[0].clone());
    assert!(person.validate().is_err());
    let mut person = facts();
    person.merged_into_id = Some(person.person_id.clone());
    assert!(person.validate().is_err());
}
