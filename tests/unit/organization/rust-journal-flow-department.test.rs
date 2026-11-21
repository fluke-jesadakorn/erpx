use erpx_server_organization::public::journal_flow_department::team_name_from_department;

#[test]
fn only_missing_department_uses_company() {
    assert_eq!(team_name_from_department(None), "Company");
    assert_eq!(team_name_from_department(Some("")), "");
    assert_eq!(team_name_from_department(Some("   ")), "   ");
}

#[test]
fn active_department_name_is_preserved() {
    assert_eq!(
        team_name_from_department(Some("Engineering")),
        "Engineering"
    );
}
