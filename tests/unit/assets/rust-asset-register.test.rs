use erpx_server_assets::public::asset_register::*;
use erpx_server_shared::public::UserId;
use serde_json::json;

fn state(
    lifecycle: &str,
    condition: &str,
    cost_minor: i64,
    finance_asset_id: Option<&str>,
    finance_asset_version: Option<i64>,
) -> AssetState {
    AssetState {
        version: 3,
        condition: match condition {
            "good" => erpx_server_assets::public::maintenance::MaintenanceAssetCondition::Good,
            "degraded" => {
                erpx_server_assets::public::maintenance::MaintenanceAssetCondition::Degraded
            }
            "unsafe" => erpx_server_assets::public::maintenance::MaintenanceAssetCondition::Unsafe,
            _ => panic!("closed fixture condition"),
        },
        lifecycle: match lifecycle {
            "registered" => {
                erpx_server_assets::public::maintenance::MaintenanceAssetLifecycle::Registered
            }
            "in_service" => {
                erpx_server_assets::public::maintenance::MaintenanceAssetLifecycle::InService
            }
            "maintenance" => {
                erpx_server_assets::public::maintenance::MaintenanceAssetLifecycle::Maintenance
            }
            "retirement_pending" => {
                erpx_server_assets::public::maintenance::MaintenanceAssetLifecycle::RetirementPending
            }
            "retired" => {
                erpx_server_assets::public::maintenance::MaintenanceAssetLifecycle::Retired
            }
            _ => panic!("closed fixture lifecycle"),
        },
        placement: None,
        custodian_id: None,
        retirement_reason: None,
        finance_asset_id: finance_asset_id.map(str::to_owned),
        finance_asset_version,
        currency: "THB".into(),
        cost_minor,
    }
}

fn change(action: AssetAction) -> AssetChange {
    AssetChange {
        expected_version: 3,
        action,
        actor_id: UserId::new("user-1").unwrap(),
        command_key: "synthetic-command".into(),
        reason: "Synthetic reason".into(),
        at: 100,
    }
}

fn place() -> AssetAction {
    AssetAction::Place {
        placement: "HQ desk".into(),
        custodian_id: erpx_server_shared::public::RecordId::new("user-2").unwrap(),
    }
}

#[test]
fn retained_action_schema_accepts_exact_integer_forms_and_rejects_other_shapes() {
    let parsed = AssetChangeInput::parse(&json!({
        "action": "place",
        "expectedVersion": 1.0,
        "placement": " HQ ",
        "custodianId": "user-2",
        "ignored": "value"
    }))
    .unwrap();
    assert_eq!(parsed.expected_version, 1);
    assert!(matches!(
        parsed.action,
        AssetAction::Place {
            ref placement,
            ..
        } if placement == "HQ"
    ));
    assert!(
        AssetChangeInput::parse(&json!({
            "action": "condition",
            "expectedVersion": 0,
            "condition": "good"
        }))
        .is_err()
    );
    assert!(
        AssetChangeInput::parse(&json!({
            "action": "condition",
            "expectedVersion": 1.5,
            "condition": "good"
        }))
        .is_err()
    );
    assert!(
        AssetChangeInput::parse(&json!({
            "action": "unknown",
            "expectedVersion": 1
        }))
        .is_err()
    );
    assert!(
        AssetChangeInput::parse(&json!({
            "action": "link-finance",
            "expectedVersion": 1,
            "financeAssetId": "finance-1"
        }))
        .is_err()
    );
    assert!(
        AssetChangeInput::parse(&json!({
            "action": "place",
            "expectedVersion": 1,
            "placement": " ",
            "custodianId": "user-2"
        }))
        .is_err()
    );
}

#[test]
fn placement_and_condition_refusals_match_the_retained_owner_rules() {
    let next = asset_after(
        &state("registered", "good", 0, None, None),
        &change(place()),
        None,
        true,
    )
    .unwrap();
    assert_eq!(next.version, 4);
    assert_eq!(next.lifecycle.as_str(), "in_service");
    assert_eq!(next.placement.as_deref(), Some("HQ desk"));
    assert_eq!(next.custodian_id.as_deref(), Some("user-2"));

    assert_eq!(
        asset_after(
            &state("registered", "unsafe", 0, None, None),
            &change(place()),
            None,
            true
        )
        .unwrap_err()
        .code,
        "ASSET_UNSAFE"
    );
    for lifecycle in ["maintenance", "retirement_pending", "retired"] {
        assert_eq!(
            asset_after(
                &state(lifecycle, "good", 0, None, None),
                &change(place()),
                None,
                true
            )
            .unwrap_err()
            .code,
            "ASSET_PLACEMENT_BLOCKED",
            "{lifecycle}"
        );
    }
    assert_eq!(
        asset_after(
            &state("retired", "good", 0, None, None),
            &change(AssetAction::Condition {
                condition:
                    erpx_server_assets::public::maintenance::MaintenanceAssetCondition::Degraded
            }),
            None,
            true
        )
        .unwrap_err()
        .code,
        "ASSET_RETIRED"
    );
    let conditioned = asset_after(
        &state("maintenance", "good", 0, None, None),
        &change(AssetAction::Condition {
            condition: erpx_server_assets::public::maintenance::MaintenanceAssetCondition::Unsafe,
        }),
        None,
        true,
    )
    .unwrap();
    assert_eq!(conditioned.condition.as_str(), "unsafe");
    assert_eq!(conditioned.lifecycle.as_str(), "maintenance");
}

#[test]
fn retirement_request_approval_and_refresh_preserve_the_original_evidence() {
    let mut before = state("in_service", "good", 0, None, None);
    before.placement = Some("HQ".into());
    before.custodian_id = Some("user-2".into());
    let requested = asset_after(&before, &change(AssetAction::RetireRequest), None, true).unwrap();
    assert_eq!(requested.lifecycle.as_str(), "retirement_pending");
    assert_eq!(
        requested.retirement_reason.as_deref(),
        Some("Synthetic reason")
    );
    assert_eq!(requested.placement.as_deref(), Some("HQ"));

    let mut pending = state("retirement_pending", "good", 0, None, None);
    pending.retirement_reason = Some("Original request".into());
    pending.placement = Some("HQ".into());
    pending.custodian_id = Some("user-2".into());
    let approved = asset_after(
        &pending,
        &change(AssetAction::RetireApprove),
        Some(&UserId::new("user-1").unwrap()),
        true,
    )
    .unwrap();
    assert_eq!(approved.lifecycle.as_str(), "retired");
    assert_eq!(
        approved.retirement_reason.as_deref(),
        Some("Original request")
    );
    assert!(approved.placement.is_none());
    assert!(approved.custodian_id.is_none());

    assert_eq!(
        asset_after(
            &state("registered", "good", 0, None, None),
            &change(AssetAction::RetireApprove),
            None,
            true
        )
        .unwrap_err()
        .code,
        "ASSET_RETIREMENT_NOT_PENDING"
    );
    assert_eq!(
        asset_after(
            &state("registered", "good", 1000, Some("finance-1"), Some(1)),
            &change(AssetAction::RetireApprove),
            None,
            true
        )
        .unwrap_err()
        .code,
        "ASSET_RETIREMENT_NOT_PENDING"
    );
    let mut costly = state(
        "retirement_pending",
        "good",
        1000,
        Some("finance-1"),
        Some(1),
    );
    costly.retirement_reason = Some("Original request".into());
    assert_eq!(
        asset_after(
            &costly,
            &change(AssetAction::RetireApprove),
            Some(&UserId::new("user-1").unwrap()),
            false
        )
        .unwrap_err()
        .code,
        "ASSET_FINANCE_DISPOSAL_REQUIRED"
    );
    assert_eq!(
        asset_after(
            &costly,
            &change(AssetAction::RetireApprove),
            Some(&UserId::new("user-1").unwrap()),
            true
        )
        .unwrap()
        .lifecycle
        .as_str(),
        "retired"
    );
    assert_eq!(
        asset_after(
            &state("maintenance", "good", 0, None, None),
            &change(AssetAction::RetireRequest),
            None,
            true
        )
        .unwrap_err()
        .code,
        "ASSET_RETIREMENT_BLOCKED"
    );
}

#[test]
fn only_the_original_requester_can_renew_a_pending_request() {
    let mut pending = state("retirement_pending", "good", 0, None, None);
    pending.retirement_reason = Some("Original".into());
    let requester = UserId::new("user-1").unwrap();
    let other = UserId::new("user-2").unwrap();
    let renewed = asset_after(
        &pending,
        &change(AssetAction::RetireRequest),
        Some(&requester),
        true,
    )
    .unwrap();
    assert_eq!(
        renewed.retirement_reason.as_deref(),
        Some("Synthetic reason")
    );
    assert_eq!(
        asset_after(
            &pending,
            &change(AssetAction::RetireRequest),
            Some(&other),
            true
        )
        .unwrap_err()
        .code,
        "ASSET_RETIREMENT_REFRESH_FORBIDDEN"
    );
    assert_eq!(
        asset_after(&pending, &change(AssetAction::RetireRequest), None, true)
            .unwrap_err()
            .code,
        "ASSET_RETIREMENT_REFRESH_FORBIDDEN"
    );
}

#[test]
fn finance_link_is_immutable_and_keeps_the_physical_state() {
    let linked = asset_after(
        &state("retirement_pending", "degraded", 1000, None, None),
        &change(AssetAction::LinkFinance {
            finance_asset_id: erpx_server_shared::public::RecordId::new("finance-1").unwrap(),
            finance_asset_version: 4,
        }),
        None,
        true,
    )
    .unwrap();
    assert_eq!(linked.finance_asset_id.as_deref(), Some("finance-1"));
    assert_eq!(linked.finance_asset_version, Some(4));
    assert_eq!(linked.lifecycle.as_str(), "retirement_pending");
    assert_eq!(linked.condition.as_str(), "degraded");
    assert_eq!(
        asset_after(
            &state("registered", "good", 0, Some("finance-1"), Some(2)),
            &change(AssetAction::LinkFinance {
                finance_asset_id: erpx_server_shared::public::RecordId::new("finance-2").unwrap(),
                finance_asset_version: 1,
            }),
            None,
            true
        )
        .unwrap_err()
        .code,
        "ASSET_FINANCE_LINK_IMMUTABLE"
    );
}

#[test]
fn review_must_be_independent_and_version_checks_stay_late() {
    let requester = UserId::new("user-1").unwrap();
    let reviewer = UserId::new("user-2").unwrap();
    assert_eq!(
        require_independent_retirement_review(&requester, &requester)
            .unwrap_err()
            .code,
        "ASSET_RETIREMENT_REVIEW_INDEPENDENT"
    );
    require_independent_retirement_review(&requester, &reviewer).unwrap();
    let before = state("registered", "good", 0, None, None);
    require_expected_version(&before, 3).unwrap();
    assert_eq!(
        require_expected_version(&before, 4).unwrap_err().code,
        "CONFLICT"
    );
    let maximum = AssetState {
        version: 9_007_199_254_740_991,
        ..state("registered", "good", 0, None, None)
    };
    assert_eq!(
        asset_after(&maximum, &change(place()), None, true)
            .unwrap_err()
            .code,
        "ASSET_VERSION_INVALID"
    );
}

#[test]
fn text_date_and_currency_validation_match_the_retained_bounds() {
    assert_eq!(validate_reason("  Reason  ").unwrap(), "Reason");
    assert!(validate_reason("  ab  ").is_err());
    assert!(validate_reason(&"a".repeat(2000)).is_ok());
    assert!(validate_reason(&"a".repeat(2001)).is_err());
    // A UTF-16 surrogate pair counts as two code units, as in the retained Zod
    // boundary. One non-BMP scalar therefore reaches the 2000-unit limit with
    // 999 additional ASCII characters.
    assert!(validate_reason(&format!("{}a", "\u{1F600}".repeat(999))).is_ok());
    assert!(validate_reason(&format!("{}a", "\u{1F600}".repeat(1000))).is_err());
    assert_eq!(validate_currency("thb").unwrap(), "THB");
    assert!(validate_currency("TH").is_err());
    assert!(validate_currency("T1B").is_err());
    assert_eq!(validate_date("2024-02-29").unwrap(), "2024-02-29");
    assert!(validate_date("2025-02-29").is_err());
    assert!(validate_date("2026-13-01").is_err());
    assert!(validate_date("2026-1-01").is_err());
}

#[test]
fn registration_uses_the_selected_scope_and_normalizes_the_exact_fields() {
    let parsed = NewAsset::parse(
        &json!({
            "branchId": "branch-b",
            "departmentId": "department-b",
            "code": " AST-1 ",
            "name": "  Laptop  ",
            "category": " hardware ",
            "currency": "thb",
            "costMinor": 1500,
            "acquiredDate": "2026-02-28",
            "warrantyEnd": "2027-02-28"
        }),
        1500,
    )
    .unwrap();
    assert_eq!(parsed.code, "AST-1");
    assert_eq!(parsed.name, "Laptop");
    assert_eq!(parsed.category, "hardware");
    assert_eq!(parsed.currency, "THB");
    let scoped = parsed.scoped(
        Some(&erpx_server_shared::public::RecordId::new("branch").unwrap()),
        Some(&erpx_server_shared::public::RecordId::new("department").unwrap()),
    );
    assert_eq!(scoped.branch_id.as_str(), "branch");
    assert_eq!(
        scoped.department_id.as_ref().map(|id| id.as_str()),
        Some("department")
    );
    assert!(
        NewAsset::parse(
            &json!({
                "branchId": "branch",
                "code": "A",
                "name": "Name",
                "category": "category",
                "currency": "THB",
                "costMinor": 1,
                "acquiredDate": "2026-01-01",
                "financeAssetId": "finance-1"
            }),
            1
        )
        .is_err()
    );
    assert!(
        NewAsset::parse(
            &json!({
                "branchId": "branch",
                "code": "A",
                "name": "Name",
                "category": "category",
                "currency": "THB",
                "costMinor": 1,
                "acquiredDate": "2026-01-01",
                "warrantyEnd": "not-a-date"
            }),
            1
        )
        .is_err()
    );
}

#[test]
fn command_validation_requires_a_current_key_and_bounded_time() {
    let mut value = change(AssetAction::RetireRequest);
    value.command_key.clear();
    assert_eq!(value.validate().unwrap_err().code, "VALIDATION_FAILED");
    let mut value = change(AssetAction::RetireRequest);
    value.at = -1;
    assert_eq!(value.validate().unwrap_err().code, "VALIDATION_FAILED");
    let mut value = change(AssetAction::RetireRequest);
    value.reason = "  x  ".into();
    assert_eq!(value.validate().unwrap_err().code, "VALIDATION_FAILED");
    change(AssetAction::RetireRequest).validate().unwrap();
}
