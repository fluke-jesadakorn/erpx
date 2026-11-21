use erpx_server_field_service::public::executive::*;
fn candidate(status: &str, completed: Option<i64>) -> FieldExecutiveCandidate {
    FieldExecutiveCandidate {
        source_order: 1,
        id: format!("order-{status}-{}", completed.unwrap_or(-1)),
        version: 1,
        status: status.into(),
        branch_id: "branch".into(),
        department_id: None,
        technician_id: "worker".into(),
        scheduled_at: Some(0),
        visit_completed_at: completed,
        month: Some("1970-01".into()),
        created_at: 0,
        updated_at: 0,
    }
}
#[test]
fn completion_requires_a_recorded_completion_and_the_current_terminal_visit_state() {
    let rows = [
        candidate("scheduled", None),
        candidate("completed", None),
        candidate("completed", Some(1)),
        candidate("billed", Some(2)),
    ];
    assert_eq!(
        aggregate(
            FieldMetric::Completion,
            rows.iter().map(|r| ("Field visits", r))
        ),
        vec![FieldMetricRow {
            label: "Field visits".into(),
            value: None,
            numerator: Some("2".into()),
            denominator: Some("4".into())
        }]
    );
    assert!(aggregate(FieldMetric::Completion, std::iter::empty()).is_empty());
}
#[test]
fn source_windows_and_metric_breakdowns_are_explicit_and_cursor_parts_are_paired() {
    let base = FieldExecutiveQuery {
        access_only: false,
        metric: FieldMetric::Completion,
        breakdown: FieldBreakdown::Default,
        from_at: 0,
        to_at: 0,
        query: String::new(),
        before_at: None,
        before_id: None,
        evidence_limit: Some(25),
    };
    assert!(base.validate().is_ok());
    for invalid in [
        FieldExecutiveQuery {
            from_at: 1,
            ..base.clone()
        },
        FieldExecutiveQuery {
            to_at: -86_400_000,
            ..base.clone()
        },
        FieldExecutiveQuery {
            before_at: Some(1),
            ..base.clone()
        },
        FieldExecutiveQuery {
            evidence_limit: Some(0),
            ..base.clone()
        },
        FieldExecutiveQuery {
            access_only: true,
            ..base.clone()
        },
        FieldExecutiveQuery {
            metric: FieldMetric::Open,
            breakdown: FieldBreakdown::Technician,
            evidence_limit: None,
            ..base.clone()
        },
    ] {
        assert!(invalid.validate().is_err());
    }
    assert!(
        FieldExecutiveQuery {
            before_at: Some(1),
            before_id: Some("order".into()),
            ..base
        }
        .validate()
        .is_ok()
    );
}

#[test]
fn evidence_cursor_limits_and_query_identity_preserve_the_wire_contract() {
    use erpx_server_field_service::public::executive_evidence as evidence;
    use serde_json::json;
    for (value, expected) in [
        (json!(1), 1),
        (json!(" 0x10 "), 16),
        (json!(true), 1),
        (json!(["2"]), 2),
        (json!([[3]]), 3),
    ] {
        assert_eq!(evidence::limit(&value).unwrap(), expected);
    }
    for invalid in [
        json!(null),
        json!([]),
        json!(false),
        json!(0),
        json!(51),
        json!(1.5),
        json!("NaN"),
        json!({}),
        json!([1, 2]),
    ] {
        assert!(evidence::limit(&invalid).is_err());
    }
    let request: evidence::FieldEvidenceRequest =
        serde_json::from_value(json!({"metric":"field-visit-completion","filters":{}})).unwrap();
    assert_eq!(evidence::limit(&request.limit).unwrap(), 25);
    assert!(
        serde_json::from_value::<evidence::FieldEvidenceRequest>(
            json!({"metric":"field-visit-completion","filters":{},"cursor":null})
        )
        .is_err()
    );
    assert_eq!(evidence::trim("\u{feff}\u{00a0} visit \u{2029}"), "visit");
    assert!(evidence::date("2026-02-29").is_err());
    assert_eq!(evidence::date("2026-01-01").unwrap(), 1_767_225_600_000);
    assert_eq!(evidence::cursor(None).unwrap(), None);
    // Original Node base64url cursor: both the number and the UTF-16 ID bound matter.
    assert_eq!(
        evidence::cursor(Some("eyJzY2hlZHVsZWRBdCI6MSwiaWQiOiJvcmRlciJ9")).unwrap(),
        Some((1, "order".into()))
    );
    assert_eq!(
        evidence::cursor(Some(" eyJzY2hlZHVsZWRBdCI6MSwiaWQiOiJvcmRlciJ9==")).unwrap(),
        Some((1, "order".into()))
    );
    assert_eq!(
        evidence::cursor(Some("invalid")).unwrap_err().code,
        "EVIDENCE_CURSOR_INVALID"
    );
    assert_eq!(
        evidence::encode("order/ไทย ?"),
        "order%2F%E0%B9%84%E0%B8%97%E0%B8%A2%20%3F"
    );
    let hash_input = evidence::SearchHash {
        provider_id: evidence::PROVIDER,
        metric: evidence::METRIC,
        filters: &request.filters,
        query: "",
        cursor: None,
    };
    assert_eq!(
        serde_json::to_string(&hash_input).unwrap(),
        r#"{"providerId":"field-service-orders","metric":"field-visit-completion","filters":{"from":"2026-01-01","to":"2026-12-31","currency":"THB"},"query":"","cursor":null}"#
    );
}
