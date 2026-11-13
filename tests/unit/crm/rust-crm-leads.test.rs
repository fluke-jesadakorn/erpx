//! Native CRM lead / opportunity command validation facts.
use erpx_server_crm::public::leads_native::{
    LeadActor, LeadChangeInput, LeadCreateInput, LeadCreateReceipt, OpportunityChangeInput,
    canonical_lead_create_input, canonical_lead_qualify_input, lead_record_id,
    validate_lead_change, validate_lead_create, validate_opportunity_change,
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
fn lead_create_receipt_and_record_id_match_the_retained_wire() {
    assert_eq!(lead_record_id("create-1"), "lead-create-1");
    assert_eq!(
        serde_json::to_value(LeadCreateReceipt {
            id: lead_record_id("create-1"),
            version: 1,
        })
        .unwrap(),
        json!({"id": "lead-create-1", "version": 1})
    );
}

#[test]
fn qualify_command_input_matches_the_retained_property_order() {
    let input = LeadChangeInput {
        action: "qualify".into(),
        expected_version: 1,
        owner_id: None,
        due_date: None,
        reason: "  Confirmed customer  ".into(),
        customer_id: Some("customer".into()),
        expected_value: Some("100.00".into()),
        close_date: Some("2026-12-01".into()),
    };
    let plan = validate_lead_change(&actor(), &input).unwrap();
    let canonical = canonical_lead_qualify_input(&plan).unwrap();
    assert_eq!(
        serde_json::to_string(&canonical).unwrap(),
        r#"{"action":"qualify","expectedVersion":1,"customerId":"customer","expectedValue":"100.00","closeDate":"2026-12-01","reason":"Confirmed customer"}"#
    );
    let mut other = plan.clone();
    other.action = "disqualify".into();
    assert!(canonical_lead_qualify_input(&other).is_err());
}

#[test]
fn lead_create_canonical_input_omits_absent_optional_keys() {
    let plan = validate_lead_create(&actor(), &create_input()).unwrap();
    assert_eq!(
        canonical_lead_create_input(&plan),
        json!({
            "branchId": "branch", "title": "New lead", "contactName": "Contact",
            "contactDetail": "Detail", "source": "staff inquiry", "brief": "Brief",
            "dueDate": "2026-11-20"
        })
    );
    assert_eq!(
        serde_json::to_string(&plan).unwrap(),
        r#"{"branchId":"branch","title":"New lead","contactName":"Contact","contactDetail":"Detail","source":"staff inquiry","brief":"Brief","dueDate":"2026-11-20"}"#
    );
    let mut input = create_input();
    input.source_evidence = Some("  Evidence  ".into());
    input.source_kind = Some("form".into());
    input.source_id = Some("source-1".into());
    input.campaign_id = Some("campaign-1".into());
    input.account_id = Some("account-1".into());
    input.contact_id = Some("contact-1".into());
    let plan = validate_lead_create(&actor(), &input).unwrap();
    assert_eq!(
        canonical_lead_create_input(&plan),
        json!({
            "branchId": "branch", "title": "New lead", "contactName": "Contact",
            "contactDetail": "Detail", "source": "staff inquiry",
            "sourceEvidence": "Evidence", "sourceKind": "form", "sourceId": "source-1",
            "campaignId": "campaign-1", "accountId": "account-1", "contactId": "contact-1",
            "brief": "Brief", "dueDate": "2026-11-20"
        })
    );
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
    assert_eq!(
        NATIVE_LEAD_ACTIONS,
        ["reassign", "disqualify", "qualify", "reopen"]
    );
    assert_eq!(
        NATIVE_OPPORTUNITY_ACTIONS,
        ["link-quote", "negotiate", "lose", "reopen", "win", "win-pms"]
    );
    for action in ["create", "unknown"] {
        assert!(!native_lead_action(action), "{action}");
        assert!(!native_opportunity_action(action), "{action}");
    }
    assert!(native_lead_action("reassign"));
    assert!(native_lead_action("qualify"));
    assert!(native_opportunity_action("lose"));
    assert!(native_opportunity_action("link-quote"));
    assert!(native_opportunity_action("win"));
    assert!(native_opportunity_action("win-pms"));
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
