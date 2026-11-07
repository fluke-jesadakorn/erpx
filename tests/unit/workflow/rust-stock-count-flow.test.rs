use erpx_server_workflow::public::stock_count_flow::*;
fn participant(id: &str) -> StockCountParticipant {
    StockCountParticipant {
        user_id: id.into(),
        name: id.into(),
        team_name: "Company".into(),
    }
}
#[test]
fn named_ineligible_primary_blocks_and_never_falls_back() {
    let mut f = StockCountStepFacts {
        step: StockCountStep::Submitted,
        candidates: vec![participant("reviewer")],
        selected: Some("former".into()),
        actor_role_allowed: true,
    };
    assert!(resolve_stock_count_step(&f).primary.is_none());
    assert_eq!(
        require_stock_count_action(&f, "reviewer", "approve")
            .unwrap_err()
            .code,
        "FLOW_PRIMARY_REQUIRED"
    );
    f.selected = None;
    assert_eq!(
        resolve_stock_count_step(&f).primary.unwrap().user_id,
        "reviewer"
    );
    assert!(require_stock_count_action(&f, "reviewer", "approve").is_ok());
    assert!(require_stock_count_action(&f, "other", "approve").is_err());
    f.actor_role_allowed = false;
    assert!(require_stock_count_action(&f, "reviewer", "approve").is_err());
    assert!(require_stock_count_action(&f, "other", "cancel").is_ok());
}
#[test]
fn assignment_is_versioned_eligible_and_only_current_or_future() {
    let people = vec![participant("reviewer")];
    assert!(
        require_stock_count_assignment(StockCountAssignmentPolicy {
            current: Some(StockCountStep::Submitted),
            step: StockCountStep::Approved,
            candidates: &people,
            user: "reviewer",
            expected_flow: 2,
            actual_flow: 2,
            expected_record: 3,
            actual_record: 3,
            company_admin: true
        })
        .is_ok()
    );
    assert_eq!(
        require_stock_count_assignment(StockCountAssignmentPolicy {
            current: Some(StockCountStep::Submitted),
            step: StockCountStep::Draft,
            candidates: &people,
            user: "reviewer",
            expected_flow: 2,
            actual_flow: 2,
            expected_record: 3,
            actual_record: 3,
            company_admin: true
        })
        .unwrap_err()
        .code,
        "FLOW_STEP_LOCKED"
    );
    assert_eq!(
        require_stock_count_assignment(StockCountAssignmentPolicy {
            current: Some(StockCountStep::Submitted),
            step: StockCountStep::Approved,
            candidates: &people,
            user: "reviewer",
            expected_flow: 1,
            actual_flow: 2,
            expected_record: 3,
            actual_record: 3,
            company_admin: true
        })
        .unwrap_err()
        .code,
        "VERSION_CONFLICT"
    );
    assert_eq!(
        require_stock_count_assignment(StockCountAssignmentPolicy {
            current: Some(StockCountStep::Submitted),
            step: StockCountStep::Approved,
            candidates: &people,
            user: "reviewer",
            expected_flow: 2,
            actual_flow: 2,
            expected_record: 2,
            actual_record: 3,
            company_admin: true
        })
        .unwrap_err()
        .code,
        "VERSION_CONFLICT"
    );
    assert_eq!(
        require_stock_count_assignment(StockCountAssignmentPolicy {
            current: Some(StockCountStep::Submitted),
            step: StockCountStep::Approved,
            candidates: &people,
            user: "other",
            expected_flow: 2,
            actual_flow: 2,
            expected_record: 3,
            actual_record: 3,
            company_admin: true
        })
        .unwrap_err()
        .code,
        "FLOW_STAFF_INELIGIBLE"
    );
    assert_eq!(
        require_stock_count_assignment(StockCountAssignmentPolicy {
            current: Some(StockCountStep::Submitted),
            step: StockCountStep::Approved,
            candidates: &people,
            user: "reviewer",
            expected_flow: 2,
            actual_flow: 2,
            expected_record: 3,
            actual_record: 3,
            company_admin: false
        })
        .unwrap_err()
        .code,
        "FLOW_ADMIN_REQUIRED"
    );
    assert_eq!(
        current_stock_count_step("rejected"),
        Some(StockCountStep::Draft)
    );
    assert!(stock_count_terminal("posted"));
}

#[test]
fn assignment_normalization_matches_retained_node_property_order_and_defaults() {
    let input = r#"{"recordVersion":2,"reason":"  verify  ","userId":"reviewer","expectedVersion":0,"stepKey":"submitted"}"#;
    assert_eq!(
        normalize_stock_count_assignment(input).unwrap().as_str(),
        r#"{"kind":"stock_count","stepKey":"submitted","userId":"reviewer","expectedVersion":0,"recordVersion":2,"reason":"verify","saveDefault":false}"#
    );
    assert!(normalize_stock_count_assignment(r#"{"stepKey":"submitted","userId":"reviewer","expectedVersion":0,"recordVersion":2,"reason":"verify","saveDefault":true}"#).is_err());
}
#[test]
fn normalized_assignment_retains_javascript_utf16_trim_and_integral_numbers() {
    let input = r#"{"stepKey":"submitted","userId":"\ud800","expectedVersion":-0.0,"recordVersion":2e0,"reason":"\ufeffok\u0085\ufeff"}"#;
    let result = normalize_stock_count_assignment(input).unwrap();
    assert_eq!(
        result.as_str(),
        format!(
            "{{\"kind\":\"stock_count\",\"stepKey\":\"submitted\",\"userId\":\"\\ud800\",\"expectedVersion\":0,\"recordVersion\":2,\"reason\":\"ok{}\",\"saveDefault\":false}}",
            '\u{0085}'
        )
    );
}
