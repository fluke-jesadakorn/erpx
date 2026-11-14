use erpx_server_finance::public::service_invoice_posting::*;
fn facts() -> ServiceInvoicePostFacts<'static> {
    ServiceInvoicePostFacts {
        version: 3,
        subtotal: 10001,
        tax: 700,
        total: 10701,
        debit_account: "ar",
        credit_account: "sales",
        tax_account: Some("tax"),
    }
}
#[test]
fn exact_receivable_lines_keep_minor_units_and_tax_balanced() {
    let f = facts();
    let lines = service_invoice_post_lines(&f, Some(3), false, false).unwrap();
    assert_eq!(
        lines
            .iter()
            .map(|l| (l.account_id.as_str(), l.debit.as_str(), l.credit.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("ar", "107.01", "0.00"),
            ("sales", "0.00", "100.01"),
            ("tax", "0.00", "7.00")
        ]
    );
    let mut f = facts();
    f.tax = 0;
    f.total = f.subtotal;
    f.tax_account = None;
    assert_eq!(
        service_invoice_post_lines(&f, None, false, false)
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn current_authority_precedes_fresh_independence_and_version_checks() {
    for (read, write, approve, code) in [
        (true, false, true, "PERMISSION_REQUIRED"),
        (false, true, true, "FINANCE_FORBIDDEN"),
        (true, true, false, "FINANCE_APPROVAL_REQUIRED"),
    ] {
        assert_eq!(
            require_service_invoice_poster(read, write, approve)
                .unwrap_err()
                .code,
            code
        );
    }
    require_service_invoice_poster(true, true, true).unwrap();
    let f = facts();
    assert_eq!(
        service_invoice_post_lines(&f, Some(2), true, false)
            .unwrap_err()
            .code,
        "INDEPENDENT_APPROVER_REQUIRED"
    );
    assert_eq!(
        service_invoice_post_lines(&f, Some(2), false, false)
            .unwrap_err()
            .code,
        "VERSION_CONFLICT"
    );
    assert_eq!(
        service_invoice_post_lines(&f, None, false, true)
            .unwrap_err()
            .code,
        "SUPPLIER_BILL_REQUIRED"
    );
    let mut f = facts();
    f.tax_account = None;
    assert_eq!(
        service_invoice_post_lines(&f, Some(2), true, false)
            .unwrap_err()
            .code,
        "INDEPENDENT_APPROVER_REQUIRED"
    );
    assert_eq!(
        service_invoice_post_lines(&f, None, false, false)
            .unwrap_err()
            .code,
        "TAX_ACCOUNT_REQUIRED"
    );
}
#[test]
fn corrupt_or_zero_source_amounts_never_create_a_posting() {
    for (subtotal, tax, total) in [
        (100, 1, 100),
        (-1, 0, 1),
        (0, 0, 0),
        (i64::MAX, 1, i64::MAX),
    ] {
        let mut f = facts();
        f.subtotal = subtotal;
        f.tax = tax;
        f.total = total;
        assert_eq!(
            service_invoice_post_lines(&f, None, false, false)
                .unwrap_err()
                .code,
            if (subtotal,tax,total)==(0,0,0) { "INVALID_JOURNAL_LINE" } else { "INVALID_INVOICE_AMOUNT" }
        );
    }
}

#[test]
fn posting_request_keeps_calendar_decimal_and_optional_version_constraints() {
    validate_service_invoice_post_request("period", "2026-11-01", Some(1), Some("-1.123456"), &[])
        .unwrap();
    for (date, version, fx) in [
        ("2026-02-29", Some(1), None),
        ("2026-11-01", Some(0), None),
        ("2026-11-01", Some(1), Some("1.1234567")),
        ("2026-11-01", None, Some("1e2")),
    ] {
        assert_eq!(
            validate_service_invoice_post_request("period", date, version, fx, &[])
                .unwrap_err()
                .code,
            "VALIDATION_FAILED"
        );
    }
    assert!(validate_service_invoice_post_request("period", "2024-02-29", None, None, &[]).is_ok());
}

#[test]
fn wire_keeps_schema_order_strips_unknown_and_preserves_logical_utf16_receipt_identity() {
    let raw=br#"{"expectedVersion":1.0,"unknown":"drop","entryDate":"2026-11-01","periodId":"period","fxRate":"1.123456","fxGainAccountId":"gain"}"#;
    let wire = normalize_service_invoice_post(raw).unwrap();
    let mut bom = vec![0xef, 0xbb, 0xbf];
    bom.extend_from_slice(raw);
    assert_eq!(normalize_service_invoice_post(&bom).unwrap(), wire);
    let invalid_utf8 =
        b"{\"periodId\":\"\xff\",\"entryDate\":\"2026-11-01\",\"expectedVersion\":1}";
    assert_eq!(
        service_invoice_post_physical_input(&normalize_service_invoice_post(invalid_utf8).unwrap())
            .unwrap()["periodId"],
        "\u{fffd}"
    );
    assert_eq!(
        normalize_service_invoice_post(b"{").unwrap_err().code,
        "INVALID_JSON"
    );

    assert_eq!(
        wire.object_keys().unwrap().unwrap(),
        vec![
            "periodId",
            "entryDate",
            "expectedVersion",
            "fxRate",
            "fxGainAccountId"
        ]
    );
    assert_eq!(
        wire.property("expectedVersion").unwrap().unwrap().as_str(),
        "1"
    );
    let high = normalize_service_invoice_post(
        br#"{"periodId":"\ud800","entryDate":"2026-11-01","expectedVersion":1}"#,
    )
    .unwrap();
    let low = normalize_service_invoice_post(
        br#"{"periodId":"\udc00","entryDate":"2026-11-01","expectedVersion":1}"#,
    )
    .unwrap();
    assert_ne!(high, low);
    assert_eq!(
        service_invoice_post_physical_input(&high).unwrap(),
        service_invoice_post_physical_input(&low).unwrap()
    );
    for body in [br#"{"periodId":"period","entryDate":"2026-11-01"}"#.as_slice(),br#"{"periodId":"period","entryDate":"2026-11-01","expectedVersion":1,"fxRate":null}"#.as_slice(),br#"{"periodId":"period","entryDate":"2026-11-01","expectedVersion":1,"attachmentSetIds":[null]}"#.as_slice()] {assert_eq!(normalize_service_invoice_post(body).unwrap_err().code,"VALIDATION_FAILED");}
}

#[test]
fn controlled_receipt_keeps_three_person_identity_and_ordered_fresh_decisions() {
    use erpx_server_finance::public::service_receipt::*;
    let mut f = ServiceReceiptReviewFacts {
        status: "draft",
        version: 1,
        has_bank_account: true,
        same_creator: true,
        same_approver: false,
        same_releaser: false,
    };
    assert_eq!(
        validate_service_receipt_approval(&f, 0).unwrap_err().code,
        "VERSION_CONFLICT"
    );
    assert_eq!(
        validate_service_receipt_approval(&f, 1).unwrap_err().code,
        "SELF_REVIEW_FORBIDDEN"
    );
    f.same_creator = false;
    validate_service_receipt_approval(&f, 1).unwrap();
    assert_eq!(
        validate_service_receipt_release(&f, 0).unwrap_err().code,
        "PAYMENT_NOT_APPROVED"
    );
    f.status = "approved";
    f.version = 2;
    f.same_approver = true;
    assert_eq!(
        validate_service_receipt_release(&f, 1).unwrap_err().code,
        "VERSION_CONFLICT"
    );
    assert_eq!(
        validate_service_receipt_release(&f, 2).unwrap_err().code,
        "INDEPENDENT_PAYER_REQUIRED"
    );
    f.same_approver = false;
    validate_service_receipt_release(&f, 2).unwrap();
    f.status = "released";
    f.version = 3;
    assert_eq!(
        validate_service_receipt_confirmation(&f, 3)
            .unwrap_err()
            .code,
        "INDEPENDENT_PAYER_REQUIRED"
    );
    f.same_releaser = true;
    validate_service_receipt_confirmation(&f, 3).unwrap();
    f.same_creator = true;
    assert_eq!(
        validate_service_receipt_confirmation(&f, 3)
            .unwrap_err()
            .code,
        "INDEPENDENT_PAYER_REQUIRED"
    );
    assert_eq!(
        require_service_receipt_evidence("\u{feff}\u{2028}", false)
            .unwrap_err()
            .code,
        "HUMAN_DECISION_REQUIRED"
    );
    require_service_receipt_evidence("\u{0085}", true).unwrap(); // ECMAScript trim, not Rust whitespace.
    assert_eq!(
        service_receipt_amount("90071992547409.91").unwrap(),
        9_007_199_254_740_991
    );
    assert_eq!(
        service_receipt_amount("90071992547409.92")
            .unwrap_err()
            .code,
        "MONEY_OVERFLOW"
    );
    assert_eq!(
        service_receipt_amount("0").unwrap_err().code,
        "INVALID_PAYMENT_AMOUNT"
    );
    assert_eq!(
        service_receipt_amount("1.001").unwrap_err().code,
        "INVALID_MONEY"
    );
    let invoice = ServiceReceiptInvoiceFacts {
        party: "customer",
        currency: "THB",
        kind: "receivable",
        status: "posted",
        source_type: "field_service",
        outstanding: 100,
    };
    validate_service_receipt_invoice(&invoice, "customer", "THB", 100).unwrap();
    assert_eq!(
        validate_service_receipt_invoice(&invoice, "customer", "THB", 101)
            .unwrap_err()
            .code,
        "PAYMENT_SETTLEMENT_SOURCE_INVALID"
    );
    assert_eq!(
        validate_service_receipt_invoice(&invoice, "other", "THB", 1)
            .unwrap_err()
            .code,
        "PAYMENT_SETTLEMENT_SOURCE_INVALID"
    );
}

#[test]
fn customer_receipt_retains_the_original_html_export_bytes() {
    use erpx_server_finance::public::service_receipt::*;
    use sha2::{Digest, Sha256};
    let snapshot = CustomerReceiptSnapshot {
        schema: "finance.customer-receipt.v1".into(),
        document_number: "RC-2026-000001".into(),
        company_name: "Acme & <Company>".into(),
        customer_name: "O'Brien \"ไทย\"".into(),
        customer_code: "C&1".into(),
        date: "2026-11-01".into(),
        currency: "THB".into(),
        amount_minor: "10000".into(),
        payment_id: "pay-one".into(),
        payment_version: 4,
        bank_reference: "PAY&1".into(),
        journal_id: "journal-one".into(),
        allocations: vec![CustomerReceiptAllocation {
            invoice_id: "invoice-one".into(),
            invoice_number: "INV<1>".into(),
            amount_minor: "7500".into(),
        }],
    };
    let html = render_customer_receipt(&snapshot).unwrap();
    // Hash from the retained domain/sales.ts renderer on this exact snapshot,
    // including escaping, UTF-8 text, unallocated money and the print stylesheet.
    assert_eq!(
        format!("{:x}", Sha256::digest(html.as_bytes())),
        "c00a693fd6061e9edd05765aa44fbf088a7930f634868081e80b4e20e43cf4a2"
    );
}

#[test]
fn covered_service_confirmation_keeps_context_and_independence_order() {
    use erpx_server_finance::public::service_no_charge::require_no_charge_decision as decide;
    assert!(decide(true,1,1,true,true).is_ok());
    for (draft,actual,expected,context) in [(false,1,1,true),(true,2,1,true),(true,1,1,false)] {
        assert_eq!(decide(draft,actual,expected,context,false).unwrap_err().code,"SERVICE_NO_CHARGE_CONTEXT_CHANGED");
    }
    assert_eq!(decide(true,1,1,true,false).unwrap_err().code,"INDEPENDENT_APPROVER_REQUIRED");
}
#[test]
fn covered_service_wire_keeps_original_trim_order_and_utf16_identity() {
    use erpx_server_finance::public::service_no_charge::normalize_service_no_charge as parse;
    let hash="a".repeat(64);
    let body=format!(r#"{{"unknown":true,"reason":"\ufeffRetain \ud800 source \u00a0","contextKey":"{hash}","expectedVersion":1.0}}"#);
    let doc=parse(body.as_bytes()).unwrap();
    assert_eq!(doc.input().expected_version,1);
    let reason=doc.normalized().property("reason").unwrap().unwrap().string_utf16_units().unwrap().unwrap();
    assert_eq!(reason,[vec![82,101,116,97,105,110,32,0xd800,32],"source".encode_utf16().collect()].concat());
    assert_eq!(doc.normalized().object_keys().unwrap().unwrap(),vec!["expectedVersion","contextKey","reason"]);
    for bad in ["null".to_owned(),format!(r#"{{"expectedVersion":0,"contextKey":"{hash}","reason":"Long enough reason"}}"#),format!(r#"{{"expectedVersion":1,"contextKey":"{hash}","reason":"short"}}"#)] { assert!(parse(bad.as_bytes()).is_err()); }
}

#[test]
fn field_cost_journal_requires_selected_eligible_reviewer_and_keeps_posted_history_terminal() {
    use erpx_server_workflow::public::field_journal_flow::field_journal_flow_view as view;
    use serde_json::json;
    let source=json!({"id":"actual-field-cost","status":"draft","version":1});
    let staff=vec![json!({"userId":"reviewer-b","name":"B","teamName":"Finance"}),json!({"userId":"reviewer-c","name":"C","teamName":"Finance"})];
    let ambiguous=view(&source,0,0,&[],staff.clone(),None,"reviewer-b",true,true);
    assert_eq!(ambiguous["canAct"],false);
    assert_eq!(ambiguous["steps"].as_array().unwrap().len(),1);
    let stale=view(&source,2,4,&[],staff.clone(),Some("withdrawn-reviewer".into()),"reviewer-b",true,true);
    assert_eq!(stale["steps"][0]["primary"],serde_json::Value::Null);
    assert_eq!(stale["canAct"],false,"a stale saved primary never falls through to an eligible caller");
    let selected=view(&source,3,4,&[],staff.clone(),Some("reviewer-b".into()),"reviewer-b",true,true);
    assert_eq!(selected["canAct"],true);
    assert_eq!(selected["current"]["key"],"draft");
    let revoked=view(&source,3,4,&[],staff.clone(),Some("reviewer-b".into()),"reviewer-b",false,true);
    assert_eq!(revoked["canAct"],false,"a route is not a current permission grant");
    let mut posted=source;posted["status"]=json!("posted");posted["version"]=json!(2);
    let terminal=view(&posted,3,4,&[json!({"step":"draft","action":"finance.journal.post"})],staff,Some("reviewer-b".into()),"reviewer-b",true,true);
    assert_eq!(terminal["current"],serde_json::Value::Null);
    assert_eq!(terminal["next"],serde_json::Value::Null);
    assert_eq!(terminal["canAct"],false);
    assert_eq!(terminal["steps"][0]["status"],"recorded");
}

mod finance_setup_readiness {
    use erpx_server_finance::public::finance_setup::{
        FinanceSetupCounts, finance_setup_checks,
    };

    #[test]
    fn reports_every_complete_company_check_with_stable_detail() {
        let checks = finance_setup_checks(FinanceSetupCounts {
            charts: 2,
            active_accounts: 23,
            primary_control_books: 1,
            effective_profiles: 1,
            preparers: 3,
            approvers: 2,
            independent_approver: true,
            periods: 12,
            active_tax_codes: 2,
            bank_statement_accounts: 1,
        });
        assert_eq!(checks.len(), 8);
        assert!(checks.iter().all(|check| check.complete));
        assert_eq!(checks[0].id, "chart");
        assert_eq!(checks[0].detail, "2 chart(s)");
        assert_eq!(checks[1].detail, "23 active account(s)");
        assert_eq!(
            checks[2].detail,
            "1 primary book(s) with all three control mappings"
        );
        assert_eq!(
            checks[3].detail,
            "1 effective independently approved profile(s)"
        );
        assert_eq!(
            checks[4].detail,
            "3 preparer(s), 2 approver(s) with current Company authority"
        );
        assert_eq!(checks[5].detail, "12 period(s)");
        assert_eq!(checks[6].detail, "2 active code(s)");
        assert_eq!(checks[7].detail, "1 account(s) from statements");
    }

    #[test]
    fn keeps_empty_or_same_person_setup_incomplete() {
        let checks = finance_setup_checks(FinanceSetupCounts {
            preparers: 1,
            approvers: 1,
            independent_approver: false,
            ..FinanceSetupCounts::default()
        });
        assert!(checks.iter().all(|check| !check.complete));
        assert_eq!(checks[0].detail, "0 chart(s)");
        assert_eq!(
            checks[4].detail,
            "1 preparer(s), 1 approver(s) with current Company authority"
        );
    }
}
