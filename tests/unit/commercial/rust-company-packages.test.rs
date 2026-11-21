use erpx_server_commercial::public::purchases::*;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use std::sync::{Arc, Mutex};

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn grant(company: &str, package: &str, version: i64) -> CompanyPackageGrant {
    CompanyPackageGrant {
        company_id: company.into(),
        package_id: package.into(),
        package_version: 1,
        state: "active".into(),
        starts_at: 0,
        ends_at: None,
        version,
        reason: "Reviewed package change".into(),
        evidence: "PRIVATE_EVIDENCE".into(),
    }
}

fn history(company: &str, package: &str) -> CompanyPackageHistoryRight {
    CompanyPackageHistoryRight {
        company_id: company.into(),
        package_id: package.into(),
        package_version: 1,
        source_grant_version: 1,
        source_event_id: "event".into(),
        retained_at: 10,
    }
}

struct Facts {
    grants: Vec<CompanyPackageGrant>,
    history: Vec<CompanyPackageHistoryRight>,
}

impl CompanyPurchaseReadPort for Facts {
    type Error = BusinessError;

    async fn read(&mut self, _: &CompanyId) -> Result<CompanyPurchaseFacts, BusinessError> {
        Ok(CompanyPurchaseFacts {
            grants: self.grants.clone(),
            history: self.history.clone(),
        })
    }
}

fn signatures() -> (CompanyPackageGrant, CompanyPackageGrant) {
    let catalogue = current_package_catalogue().unwrap();
    let definitions = catalogue.definitions();
    assert!(definitions.len() >= 2);
    (
        grant("company", &definitions[0].id, 1),
        grant("company", &definitions[1].id, 1),
    )
}

#[tokio::test]
async fn overview_keeps_the_company_revision_catalogue_and_history() {
    let catalogue = current_package_catalogue().unwrap();
    let (base, addon) = signatures();
    let mut facts = Facts {
        grants: vec![base.clone(), addon.clone()],
        history: vec![history("company", &base.package_id)],
    };
    let overview = company_package_overview(
        &mut facts,
        &catalogue,
        &company(),
        TimestampMillis::new(1000),
    )
    .await
    .unwrap();
    assert_eq!(overview.company_id, "company");
    assert_eq!(overview.packages.as_slice(), catalogue.definitions());
    assert_eq!(overview.billing_mode, "sandbox");
    assert_eq!(overview.observed_at, 1000);
    assert_eq!(overview.grants, vec![base.clone(), addon]);
    assert_eq!(overview.history, vec![history("company", &base.package_id)]);
    assert_eq!(
        overview.revision,
        package_grant_revision("company", &overview.grants).unwrap()
    );
    assert_eq!(overview.revision.len(), 64);
    assert!(
        overview
            .revision
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    );
    let value = serde_json::to_value(&overview).unwrap();
    assert_eq!(value["companyId"], "company");
    assert_eq!(value["billingMode"], "sandbox");
    assert_eq!(value["observedAt"], 1000);
    assert!(value["packages"].as_array().unwrap().len() >= 2);
}

#[tokio::test]
async fn overview_rejects_a_port_that_returns_another_company() {
    let catalogue = current_package_catalogue().unwrap();
    let mut facts = Facts {
        grants: vec![grant("foreign", "service", 1)],
        history: vec![history("company", "service")],
    };
    let error = company_package_overview(
        &mut facts,
        &catalogue,
        &company(),
        TimestampMillis::new(1000),
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(error.code, "COMMERCIAL_PURCHASE_CHANGED");
}

#[test]
fn purchase_requests_are_bounded_and_unambiguous() {
    let request = |capabilities: Vec<String>| PurchaseRequest {
        capabilities,
        purpose: CapabilityPurpose::NewWork,
        mode: PackageEnforcementMode::Enforced,
    };
    assert!(validate_purchase_request(&request(vec!["legal.contracts".into()])).is_ok());
    assert!(validate_purchase_request(&request(vec!["x".repeat(128)])).is_ok());
    for invalid in [
        request(vec![]),
        request(vec!["".into()]),
        request(vec!["x".repeat(129)]),
        request(vec!["😀".repeat(33)]),
        request(vec!["ก".repeat(43)]),
        request((0..257).map(|index| format!("cap.{index}")).collect()),
    ] {
        let error = validate_purchase_request(&invalid).unwrap_err();
        assert_eq!(error.kind, FailureKind::InvalidInput);
        assert_eq!(error.code, "COMMERCIAL_CAPABILITIES_REQUIRED");
    }
}

#[test]
fn grant_evidence_uses_js_trim_and_lifecycle_batch_counts_utf16_units() {
    let catalogue = current_package_catalogue().unwrap();
    let package_id = catalogue.definitions()[0].id.clone();
    let valid = |reason: &str, evidence: &str| CompanyPackageGrant {
        company_id: "company".into(),
        package_id: package_id.clone(),
        package_version: 1,
        state: "active".into(),
        starts_at: 0,
        ends_at: None,
        version: 1,
        reason: reason.into(),
        evidence: evidence.into(),
    };
    // ECMAScript trim strips U+FEFF and U+00A0: whitespace-only evidence
    // must fail. Rust str::trim would keep U+FEFF and wrongly pass.
    for whitespace in [
        "\u{feff}",
        "\u{00a0}",
        "\u{2028}",
        "\u{3000}",
        "  \u{feff}  ",
    ] {
        assert_eq!(
            catalogue
                .validate_grant_change(None, &valid(whitespace, "Reviewed evidence"), 0, 0)
                .unwrap_err()
                .code,
            "GRANT_EVIDENCE_REQUIRED"
        );
        assert_eq!(
            catalogue
                .validate_grant_change(None, &valid("Reviewed reason", whitespace), 0, 0)
                .unwrap_err()
                .code,
            "GRANT_EVIDENCE_REQUIRED"
        );
    }
    // U+0085 (NEL) is not in the ECMAScript trim set, so it alone is
    // evidence content, not whitespace.
    assert!(
        catalogue
            .validate_grant_change(None, &valid("\u{0085}", "Reviewed evidence"), 0, 0)
            .is_ok()
    );
    // Interior/edge JS whitespace around real content stays valid.
    assert!(
        catalogue
            .validate_grant_change(
                None,
                &valid("\u{feff}Reviewed reason\u{00a0}", "Reviewed evidence"),
                0,
                0,
            )
            .is_ok()
    );

    // normalize_lifecycle_batch stores the JS-trimmed reason/evidence and
    // measures trimmed fields in UTF-16 units (500 emoji = 1000 units).
    let batch = |reason: &str, evidence: &str| GrantBatch {
        company_id: "company".into(),
        expected_revision: "a".repeat(64),
        idempotency_key: "synthetic-lifecycle-key".into(),
        changes: vec![BatchChange {
            expected_version: 0,
            grant: valid(reason, evidence),
        }],
    };
    let stored = normalize_lifecycle_batch(batch(
        "\u{feff}Reviewed reason\u{00a0}",
        "  Reviewed evidence  ",
    ))
    .unwrap();
    assert_eq!(stored.changes[0].grant.reason, "Reviewed reason");
    assert_eq!(stored.changes[0].grant.evidence, "Reviewed evidence");
    assert!(normalize_lifecycle_batch(batch(&"😀".repeat(500), "Reviewed evidence")).is_ok());
    assert_eq!(
        normalize_lifecycle_batch(batch(&"😀".repeat(501), "Reviewed evidence"))
            .unwrap_err()
            .code,
        "VALIDATION_FAILED"
    );
    assert_eq!(
        normalize_lifecycle_batch(batch("  ab  ", "Reviewed evidence"))
            .unwrap_err()
            .code,
        "VALIDATION_FAILED"
    );
    assert_eq!(
        normalize_lifecycle_batch(batch("\u{feff}", "Reviewed evidence"))
            .unwrap_err()
            .code,
        "VALIDATION_FAILED"
    );
}

#[test]
fn admission_is_a_bounded_read_without_a_command_witness() {
    let admission = CompanyPurchaseAdmission {
        company_id: "company".into(),
        revision: "a".repeat(64),
        observed_at: 1000,
        access: vec![CompanyCapabilityAdmission {
            capability: "legal.contracts".into(),
            purpose: CapabilityPurpose::NewWork,
            decision: PackageDecision {
                allowed: false,
                code: PackageDecisionCode::PackageRequired,
            },
            allowed_by_mode: false,
        }],
    };
    let value = serde_json::to_value(&admission).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "companyId": "company",
            "revision": "a".repeat(64),
            "observedAt": 1000,
            "access": [{
                "capability": "legal.contracts",
                "purpose": "new-work",
                "decision": {"allowed": false, "code": "PACKAGE_REQUIRED"},
                "allowedByMode": false,
            }],
        })
    );
    let text = value.to_string();
    assert!(!text.contains("witness"));
    assert!(!text.contains("evidence"));
    assert!(!text.contains("PRIVATE_"));
}

struct Noop;
impl CompanyPackageReadHttpBackend for Noop {
    fn read<'a>(
        &'a self,
        _: CompanyPackageReadHttpRequest<'a>,
    ) -> CompanyPackageReadHttpFuture<'a, CompanyPackageReadHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ))
        })
    }
}

struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
}

impl CompanyPackageReadHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: CompanyPackageReadHttpRequest<'a>,
    ) -> CompanyPackageReadHttpFuture<'a, CompanyPackageReadHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "{}:{}:{:?}",
                request.host,
                request.company.as_str(),
                request.kind
            ));
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "COMMERCIAL_PURCHASE_CHANGED",
                    "Synthetic changed purchase source.",
                ));
            }
            Ok(CompanyPackageReadHttpResponse {
                data: serde_json::json!({
                    "companyId": request.company.as_str(),
                    "revision": "a".repeat(64),
                }),
                context: serde_json::json!({"userId": "operator"}),
            })
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || CompanyPackageReadHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(company_package_router(vec![target()]).is_ok());
    assert!(company_package_router(vec![target(), target()]).is_err());
    assert!(
        company_package_router(vec![CompanyPackageReadHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        company_package_router(vec![CompanyPackageReadHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
}

async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let text = String::from_utf8_lossy(&bytes).to_string();
    let status = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .unwrap();
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
    (
        status,
        text.to_ascii_lowercase(),
        serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
    )
}

#[tokio::test]
async fn actual_router_retains_targets_query_and_error_mapping() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = company_package_router(vec![
        CompanyPackageReadHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: false,
            }),
        },
        CompanyPackageReadHttpTarget {
            host: "fail.example.test".into(),
            holding_slug: "fail".into(),
            origin: "https://fail.example.test".into(),
            backend: Arc::new(Probe {
                seen: seen.clone(),
                fail: true,
            }),
        },
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
    });
    let get = |host: &str, path: &str| {
        format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")
    };

    let (status, headers, body) = http(
        addr,
        get(
            "acme.example.test",
            "/api/v1/holdings/acme/commercial/companies/company/packages",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["companyId"], "company");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));

    let (status, _, body) = http(
        addr,
        get(
            "acme.example.test",
            "/api/v1/holdings/acme/commercial/companies/company/packages?capability=legal.contracts",
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "COMMERCIAL_REQUEST_INVALID");

    let (status, _, _) = http(
        addr,
        get(
            "acme.example.test",
            "/api/v1/holdings/acme/commercial/companies/company/purchase-admission?capability=legal.contracts&capability=legal.documents&purpose=history&mode=shadow",
        ),
    )
    .await;
    assert_eq!(status, 200);

    let (status, _, _) = http(
        addr,
        get(
            "acme.example.test",
            "/api/v1/holdings/acme/commercial/companies/company/purchase-admission?capability=legal%2Econtracts",
        ),
    )
    .await;
    assert_eq!(status, 200);

    for path in [
        "/api/v1/holdings/acme/commercial/companies/company/purchase-admission",
        "/api/v1/holdings/acme/commercial/companies/company/purchase-admission?capability=legal.contracts&capability=legal.contracts",
        "/api/v1/holdings/acme/commercial/companies/company/purchase-admission?capability=legal.contracts&unknown=1",
        "/api/v1/holdings/acme/commercial/companies/company/purchase-admission?capability=legal.contracts&purpose=history&purpose=new-work",
        "/api/v1/holdings/acme/commercial/companies/company/purchase-admission?capability=legal.contracts&mode=invalid",
        "/api/v1/holdings/acme/commercial/companies/company/purchase-admission?capability=legal%00contracts",
        &format!(
            "/api/v1/holdings/acme/commercial/companies/{}/packages",
            "x".repeat(201)
        ),
    ] {
        let (status, _, body) = http(addr, get("acme.example.test", path)).await;
        assert_eq!(status, 422, "{path}");
        assert!(
            [
                "COMMERCIAL_REQUEST_INVALID",
                "COMMERCIAL_CAPABILITIES_REQUIRED",
                "INVALID_IDENTIFIER"
            ]
            .contains(&body["error"]["code"].as_str().unwrap()),
            "{path}: {body}"
        );
    }

    let (status, _, _) = http(
        addr,
        get(
            "missing.example.test",
            "/api/v1/holdings/acme/commercial/companies/company/packages",
        ),
    )
    .await;
    assert_eq!(status, 404);

    let (status, _, body) = http(
        addr,
        get(
            "fail.example.test",
            "/api/v1/holdings/fail/commercial/companies/company/packages",
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "COMMERCIAL_PURCHASE_CHANGED");

    let seen = seen.lock().unwrap().clone();
    assert!(seen.iter().any(|value| value.contains("company:Overview")));
    assert!(seen.iter().any(|value| value.contains("Admission")));
    assert!(
        seen.iter()
            .any(|value| value.contains("purpose: History") && value.contains("mode: Shadow"))
    );
    server.abort();
}
