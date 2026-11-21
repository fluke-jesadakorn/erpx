use erpx_server_organization::public::organization_access::*;
use erpx_server_organization::public::organization_nodes::OrganizationScopeKind;
use erpx_server_shared::public::FailureKind;

fn actor(scope: OrganizationScopeKind) -> OrganizationAccessActor {
    OrganizationAccessActor {
        actor_id: "user-1".into(),
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        scope_kind: scope,
        assignment_scope: scope,
        may_read_iam: true,
        may_manage_people: false,
        may_manage_organization: false,
    }
}

fn position(id: &str, branch: Option<&str>, department: Option<&str>) -> AccessPosition {
    AccessPosition {
        id: id.into(),
        company_id: "company".into(),
        branch_id: branch.map(str::to_owned),
        department_id: department.map(str::to_owned),
        title: format!("Title {id}"),
        level_id: "level-2".into(),
        level_number: 2,
        reports_to_position_id: Some("ceo".into()),
        requirements: None,
        is_ceo: 0,
        roster_required: 0,
        active: 1,
        version: 3,
    }
}

fn person(id: &str, branch: Option<&str>, department: Option<&str>) -> AccessPerson {
    AccessPerson {
        person_id: id.into(),
        user_id: Some(format!("user-{id}")),
        legal_name: format!("Person {id}"),
        email: Some(format!("{id}@example.test")),
        engagement_id: format!("engagement-{id}"),
        engagement_version: 2,
        person_version: 4,
        company_id: "company".into(),
        branch_id: branch.map(str::to_owned),
        department_id: department.map(str::to_owned),
        position_id: Some("position-2".into()),
        manager_person_id: Some("ceo-person".into()),
        level_id: Some("level-2".into()),
        level_number: Some(2),
        level_name: Some("Executive / Director".into()),
        status: "active".into(),
        person_status: "active".into(),
        start_date: "2024-01-01".into(),
        end_date: None,
        job_title: "Director".into(),
        issues: Vec::new(),
    }
}

fn snapshot() -> OrganizationAccessSnapshot {
    let ceo = AccessPerson {
        person_id: "ceo-person".into(),
        position_id: Some("ceo".into()),
        manager_person_id: None,
        level_id: Some("level-1".into()),
        level_number: Some(1),
        legal_name: "Chief".into(),
        ..person("ceo-person", None, None)
    };
    OrganizationAccessSnapshot {
        people: vec![ceo, person("one", None, None)],
        positions: vec![
            AccessPosition {
                id: "ceo".into(),
                level_id: "level-1".into(),
                level_number: 1,
                is_ceo: 1,
                reports_to_position_id: None,
                active: 1,
                ..position("ceo", None, None)
            },
            position("position-2", None, None),
        ],
        nodes: vec![AccessNode {
            id: "company".into(),
            kind: "company".into(),
            slug: "acme".into(),
            display_name: "Acme".into(),
            branch_id: None,
            status: "active".into(),
            hierarchy_revision: 1,
        }],
        levels: vec![AccessLevel {
            id: "level-1".into(),
            name: "CEO".into(),
            level_number: 1,
        }],
    }
}

#[test]
fn permission_gate_matches_the_retained_read_fence() {
    let mut denied = actor(OrganizationScopeKind::Holding);
    denied.may_read_iam = false;
    assert_eq!(
        assert_organization_access(&denied).unwrap_err().code,
        "PERMISSION_REQUIRED"
    );
    assert_eq!(
        assert_organization_access(&denied).unwrap_err().kind,
        FailureKind::Forbidden
    );
    denied.may_manage_people = true;
    assert!(assert_organization_access(&denied).is_ok());
}

#[test]
fn company_scope_grants_move_and_manage_only_with_both_permissions() {
    let mut manager = actor(OrganizationScopeKind::Holding);
    manager.may_manage_people = true;
    let data = project_organization_access(&manager, snapshot(), "2026-11-10");
    assert!(data.can_move_people);
    assert!(!data.can_manage);
    assert_eq!(data.actor_id, "user-1");
    assert_eq!(data.positions.len(), 2);
    assert_eq!(data.people.len(), 2);
    assert!(data.people.iter().all(|person| person.email.is_some()));
    assert!(data
        .people
        .iter()
        .all(|person| !person.start_date.is_empty()));
    assert_eq!(data.nodes.len(), 1);
    assert_eq!(data.levels.len(), 1);

    manager.may_manage_organization = true;
    let data = project_organization_access(&manager, snapshot(), "2026-11-10");
    assert!(data.can_manage);

    let branch = actor(OrganizationScopeKind::Branch);
    let data = project_organization_access(&branch, snapshot(), "2026-11-10");
    assert!(!data.can_move_people);
    assert!(!data.can_manage);

    let reader = actor(OrganizationScopeKind::Holding);
    let data = project_organization_access(&reader, snapshot(), "2026-11-10");
    assert!(data.people.iter().all(|person| person.email.is_none()));
    assert!(data
        .people
        .iter()
        .all(|person| person.start_date.is_empty()));
    assert!(data.people.iter().all(|person| person.end_date.is_none()));
}

#[test]
fn hr_keeps_private_fields_and_branch_scope_filters_exact_rows() {
    let mut manager = actor(OrganizationScopeKind::Branch);
    manager.branch_id = Some("north".into());
    manager.department_id = Some("service".into());
    manager.may_manage_people = true;
    let mut scoped = snapshot();
    scoped.positions = vec![
        position("north-position", Some("north"), Some("service")),
        position("other-position", Some("south"), Some("service")),
    ];
    scoped.people = vec![
        person("north", Some("north"), Some("service")),
        person("other", Some("south"), Some("service")),
    ];
    for person in &mut scoped.people {
        person.position_id = Some("north-position".into());
    }
    let data = project_organization_access(&manager, scoped, "2026-11-10");
    assert_eq!(data.positions.len(), 1);
    assert_eq!(data.positions[0].id, "north-position");
    assert_eq!(data.people.len(), 1);
    assert_eq!(data.people[0].person_id, "north");
    assert_eq!(data.people[0].email.as_deref(), Some("north@example.test"));
    assert_eq!(data.people[0].start_date, "2024-01-01");
}

#[test]
fn reporting_issues_keep_the_retained_messages_and_ceo_rule() {
    let mut data = snapshot();
    data.people[1].level_number = None;
    data.people[1].manager_person_id = None;
    let issues = reporting_issues(&data.people[1], &data.people, &data.positions, "2026-11-10");
    assert!(issues.contains(&"Assign one of the six staff levels.".to_owned()));
    assert!(issues.contains(&"Assign a reporting manager.".to_owned()));

    let mut equal_level = data.people[1].clone();
    equal_level.level_number = Some(2);
    equal_level.manager_person_id = Some("ceo-person".into());
    data.people[0].level_number = Some(2);
    let issues = reporting_issues(&equal_level, &data.people, &data.positions, "2026-11-10");
    assert!(issues.contains(&"The reporting manager must have a more senior level.".to_owned()));

    let mut circular = data.people[1].clone();
    circular.manager_person_id = Some(circular.person_id.clone());
    let issues = reporting_issues(&circular, &data.people, &data.positions, "2026-11-10");
    assert!(issues.contains(&"Correct the circular reporting relationship.".to_owned()));
}

#[test]
fn wire_shape_keeps_the_page_contract_keys() {
    let manager = actor(OrganizationScopeKind::Holding);
    let value = serde_json::to_value(project_organization_access(
        &manager,
        snapshot(),
        "2026-11-10",
    ))
    .unwrap();
    assert!(value.get("canManage").is_some());
    assert!(value.get("canMovePeople").is_some());
    assert!(value.get("actorId").is_some());
    assert!(value["people"][0].get("person_id").is_some());
    assert!(value["positions"][0]
        .get("reports_to_position_id")
        .is_some());
}

#[test]
fn utc_date_matches_the_retained_iso_slice() {
    assert_eq!(utc_date(0), "1970-01-01");
    assert_eq!(utc_date(1_800_000_000_000), "2027-01-15");
    assert_eq!(utc_date(1_702_000_000_000), "2023-12-08");
}
