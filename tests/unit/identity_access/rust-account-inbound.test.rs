use axum::http::{HeaderMap, HeaderValue};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, HoldingOrigin, RequestAudience, account_inbound_http::*,
};
fn policy() -> AccountPolicy {
    AccountPolicy {
        account_origin: "https://account.example.test".into(),
        account_host: "account.example.test".into(),
        environment: "development".into(),
        require_email_verification: true,
        holdings: vec![HoldingOrigin {
            origin: "https://acme.example.test".into(),
            host: "acme.example.test".into(),
            audience: "holding:development:h001:acme.example.test".into(),
        }],
    }
}
fn headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("host", HeaderValue::from_static("acme.example.test"));
    h
}
#[test]
fn parsed_evidence_uses_exact_host_and_audience_cookie_without_authenticating_any_actor() {
    let mut h = headers();
    let account = format!("__Secure-erpx-account.session_token={}", "a".repeat(64));
    h.insert("cookie", HeaderValue::from_str(&account).unwrap());
    let parsed = parse_inbound_account_request(&policy(), &h).unwrap();
    assert_eq!(parsed.host(), "acme.example.test");
    assert_eq!(parsed.expected_origin(), "https://acme.example.test");
    assert!(matches!(parsed.audience(), RequestAudience::Holding { .. }));
    assert!(parsed.into_proof().active.is_none());
    h.insert(
        "cookie",
        HeaderValue::from_str(&format!("__Host-erpx-product={}", "b".repeat(64))).unwrap(),
    );
    assert!(
        parse_inbound_account_request(&policy(), &h)
            .unwrap()
            .into_proof()
            .active
            .is_some()
    );
    h.insert("host", HeaderValue::from_static("retired.example.test"));
    assert_eq!(
        parse_inbound_account_request(&policy(), &h)
            .err()
            .unwrap()
            .code,
        "NOT_FOUND"
    );
    let _: DeviceProof = Default::default();
}
#[test]
fn duplicate_cookie_names_across_headers_and_aggregate_resource_limits_reuse_actual_account_parser()
{
    let mut h = headers();
    h.append("cookie", HeaderValue::from_static("unused=value"));
    h.append("cookie", HeaderValue::from_static("unused=other"));
    assert_eq!(
        parse_inbound_account_request(&policy(), &h)
            .err()
            .unwrap()
            .code,
        "AMBIGUOUS_COOKIE_HEADER"
    );
    let mut h = headers();
    h.append(
        "cookie",
        HeaderValue::from_str(&format!("a={}", "x".repeat(4095))).unwrap(),
    );
    h.append(
        "cookie",
        HeaderValue::from_str(&format!("b={}", "x".repeat(4095))).unwrap(),
    );
    assert_eq!(
        parse_inbound_account_request(&policy(), &h)
            .err()
            .unwrap()
            .code,
        "COOKIE_HEADER_TOO_LARGE"
    );
    let mut h = headers();
    h.append("host", HeaderValue::from_static("acme.example.test"));
    assert!(parse_inbound_account_request(&policy(), &h).is_err());
}
fn selected() -> HeaderMap {
    let mut h = headers();
    for (name, value) in [
        ("x-erpx-context-company", "company"),
        ("x-erpx-context-assignment", "grant"),
        ("x-erpx-context-revision", "7"),
        ("x-erpx-context-assignment-version", "3"),
    ] {
        h.insert(name, HeaderValue::from_static(value));
    }
    h
}
#[test]
fn mutation_capture_preserves_only_exact_complete_safe_cas_and_requested_scope() {
    let mut h = selected();
    h.insert(
        "authorization",
        HeaderValue::from_static("private-test-only"),
    );
    let input = CapturedHoldingMutation::capture(&h, Some("north".into()), Some("legal".into()))
        .resolve()
        .unwrap();
    assert!(input.mutation);
    assert_eq!(input.company_id.as_str(), "company");
    let e = input.expected.unwrap();
    assert_eq!((e.revision, e.assignment_version), (7, 3));
    assert_eq!(input.branch_slug.as_deref(), Some("north"));
    for field in [
        "x-erpx-context-company",
        "x-erpx-context-assignment",
        "x-erpx-context-revision",
        "x-erpx-context-assignment-version",
    ] {
        let mut h = selected();
        h.remove(field);
        assert_eq!(
            CapturedHoldingMutation::capture(&h, None, None)
                .resolve()
                .err()
                .unwrap()
                .code,
            "WORK_CONTEXT_CHANGED"
        );
    }
}
#[test]
fn missing_malformed_duplicate_and_oversize_cas_is_deferred_then_context_denied() {
    for value in [
        "0",
        "-1",
        "+3",
        "03",
        "3.0",
        "3e0",
        "9007199254740992",
        "not-an-int",
        "",
    ] {
        let mut h = selected();
        h.insert(
            "x-erpx-context-assignment-version",
            HeaderValue::from_str(value).unwrap(),
        );
        assert!(
            CapturedHoldingMutation::capture(&h, None, None)
                .resolve()
                .is_err(),
            "{value}"
        );
    }
    let mut h = selected();
    h.append(
        "x-erpx-context-assignment-version",
        HeaderValue::from_static("3"),
    );
    assert!(
        CapturedHoldingMutation::capture(&h, None, None)
            .resolve()
            .is_err()
    );
    let mut h = selected();
    h.insert(
        "x-erpx-context-company",
        HeaderValue::from_str(&"a".repeat(257)).unwrap(),
    );
    assert!(
        CapturedHoldingMutation::capture(&h, None, None)
            .resolve()
            .is_err()
    );
    let mut h = selected();
    h.insert("x-erpx-context-revision", HeaderValue::from_static("0"));
    assert_eq!(
        CapturedHoldingMutation::capture(&h, None, None)
            .resolve()
            .unwrap()
            .expected
            .unwrap()
            .revision,
        0
    );
    let mut h = selected();
    h.insert(
        "x-erpx-context-revision",
        HeaderValue::from_static("9007199254740991"),
    );
    assert_eq!(
        CapturedHoldingMutation::capture(&h, None, None)
            .resolve()
            .unwrap()
            .expected
            .unwrap()
            .revision,
        9007199254740991
    );
}
