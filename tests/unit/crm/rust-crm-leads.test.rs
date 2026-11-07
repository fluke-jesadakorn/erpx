//! Native CRM lead / opportunity command validation facts.
use erpx_server_crm::public::leads_native::{
    LeadActor, LeadChangeInput, LeadCreateInput, OpportunityChangeInput, validate_lead_change,
    validate_lead_create, validate_opportunity_change,
};
use serde_json::json;

fn actor() -> LeadActor {
    LeadActor {
        user_id: "owner".into(),
        company_id: "company".into(),
        can_read: true,
        can_sales: true,
        can_manage: false,
        can_pms: false,
        can_support: false,
        same_person_ids: vec![],
    }
}

fn create_input() -> LeadCreateInput {
    serde_json::from_value(json!({
        "branchId": "branch", "title": "New lead", "contactName": "Contact",
        "contactDetail": "Detail", "source": "staff inquiry", "brief": "Brief",
        "dueDate": "2026-11-20"
    }))
    .unwrap()
}

#[test]
fn lead_create_trims_and_accepts() {
    let plan = validate_lead_create(&actor(), &create_input()).unwrap();
    assert_eq!(plan.title, "New lead");
    assert_eq!(plan.branch_id, "branch");
}

#[test]
fn lead_create_date_matches_iso_year_zero_and_leap_boundaries() {
    for date in ["0000-01-01", "0000-02-29", "2000-02-29", "9999-12-31"] {
        let mut input = create_input();
        input.due_date = date.into();
        assert!(validate_lead_create(&actor(), &input).is_ok(), "{date}");
    }
    for date in ["0000-02-30", "1900-02-29"] {
        let mut input = create_input();
        input.due_date = date.into();
        assert!(validate_lead_create(&actor(), &input).is_err(), "{date}");
    }
}

#[test]
fn qualification_retains_year_zero_close_date_and_rejects_invalid_leap_day() {
    let mut input = LeadChangeInput {
        action: "qualify".into(), expected_version: 1, owner_id: None, due_date: None,
        reason: "Qualify lead".into(), customer_id: Some("customer".into()),
        expected_value: Some("100.00".into()), close_date: Some("0000-02-29".into()),
    };
    let plan = validate_lead_change(&actor(), &input).unwrap();
    assert_eq!(plan.close_date.as_deref(), Some("0000-02-29"));
    input.close_date = Some("0000-02-30".into());
    assert_eq!(validate_lead_change(&actor(), &input).unwrap_err().code, "CRM_LEAD_INPUT_INVALID");
}

#[test]
fn lead_create_requires_account_for_contact() {
    let mut input = create_input();
    input.contact_id = Some("contact-1".into());
    let err = validate_lead_create(&actor(), &input).unwrap_err();
    assert_eq!(err.code, "CRM_ACCOUNT_REQUIRED");
}

#[test]
fn lead_change_qualify_requires_manager_for_reassign() {
    let input = LeadChangeInput {
        action: "reassign".into(),
        expected_version: 1,
        owner_id: Some("other".into()),
        due_date: Some("2026-11-21".into()),
        reason: "Reassign needed".into(),
        customer_id: None,
        expected_value: None,
        close_date: None,
    };
    assert_eq!(
        validate_lead_change(&actor(), &input).unwrap_err().code,
        "CRM_MANAGER_REQUIRED"
    );
}

#[test]
fn opportunity_change_validates_win_pms_permission() {
    let input = OpportunityChangeInput {
        action: "win-pms".into(),
        expected_version: 1,
        lead_version: 1,
        quote_id: None,
        quote_version: None,
        loss_reason: None,
        sales_id: None,
        sales_version: None,
        reservation_id: Some("reservation-1".into()),
        property_id: Some("property-1".into()),
        reason: "Won via PMS".into(),
    };
    let err = validate_opportunity_change(&actor(), &input).unwrap_err();
    assert_eq!(err.code, "CRM_OPPORTUNITY_PMS_REQUIRED");
}

#[tokio::test]
async fn lead_create_rejects_empty_title() {
    let mut input = create_input();
    input.title = "   ".into();
    assert!(validate_lead_create(&actor(), &input).is_err());
}

#[test]
fn native_action_slice_fails_closed_for_cross_owner_work() {
    use erpx_server_crm::public::leads_native::{
        NATIVE_LEAD_ACTIONS, NATIVE_OPPORTUNITY_ACTIONS, native_lead_action,
        native_opportunity_action,
    };
    assert_eq!(NATIVE_LEAD_ACTIONS, ["reassign", "disqualify", "reopen"]);
    assert_eq!(
        NATIVE_OPPORTUNITY_ACTIONS,
        ["link-quote", "negotiate", "lose", "reopen", "win"]
    );
    for action in ["create", "qualify", "win-pms"] {
        assert!(!native_lead_action(action), "{action}");
        assert!(!native_opportunity_action(action), "{action}");
    }
    assert!(native_lead_action("reassign"));
    assert!(native_opportunity_action("lose"));
    assert!(native_opportunity_action("link-quote"));
    assert!(native_opportunity_action("win"));
}

#[test]
fn lead_change_plan_snapshot_omits_absent_fields_like_the_retained_wire() {
    let plan = validate_lead_change(
        &actor(),
        &LeadChangeInput {
            action: "disqualify".into(),
            expected_version: 1,
            owner_id: None,
            due_date: None,
            reason: "No response".into(),
            customer_id: None,
            expected_value: None,
            close_date: None,
        },
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(plan).unwrap(),
        json!({"action":"disqualify","expectedVersion":1,"reason":"No response"})
    );
}

#[test]
fn opportunity_change_plan_snapshot_retains_default_loss_reason_and_stage() {
    let plan = validate_opportunity_change(
        &actor(),
        &OpportunityChangeInput {
            action: "lose".into(),
            expected_version: 2,
            lead_version: 3,
            quote_id: None,
            quote_version: None,
            loss_reason: None,
            sales_id: None,
            sales_version: None,
            reservation_id: None,
            property_id: None,
            reason: "Customer deferred".into(),
        },
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&plan).unwrap(),
        json!({"action":"lose","expectedVersion":2,"leadVersion":3,"lossReason":"other","reason":"Customer deferred","stage":"lost"})
    );
}
