//! Owner-local HTTP adapter tests for the native Billing workspace and
//! Platform cost-review commands. The exact owner dispatch is exercised without
//! opening a socket; mounting the router into composition remains an
//! integration step.
use erpx_server_billing::public::billing_http::{
    BillingCommandHttpRequest, BillingHttpBackend, BillingHttpFuture, BillingHttpResponse,
    BillingHttpState, BillingHttpTarget, BillingWorkspaceHttpRequest, billing_router,
};
use erpx_server_billing::public::cost::BillingCostCommand;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, StatusCode, body::to_bytes, header, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Probe {
    seen: Arc<Mutex<Vec<String>>>,
    fail: Option<BusinessError>,
}

fn workspace_data() -> serde_json::Value {
    json!({
        "sandbox": true,
        "now": 123,
        "account": null,
        "enrollment": null,
        "plans": [],
        "periods": [],
        "costs": [],
        "invoices": [],
        "audit": [],
        "jobs": [],
        "legacy": null
    })
}

impl BillingHttpBackend for Probe {
    fn workspace<'a>(
        &'a self,
        request: BillingWorkspaceHttpRequest<'a>,
    ) -> BillingHttpFuture<'a, BillingHttpResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(format!(
                "workspace|{}|{}",
                request.host, request.holding_slug
            ));
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(BillingHttpResponse {
                data: workspace_data(),
                context: json!({"userId":"admin"}),
            })
        })
    }

    fn command<'a>(
        &'a self,
        request: BillingCommandHttpRequest<'a>,
    ) -> BillingHttpFuture<'a, BillingHttpResponse> {
        Box::pin(async move {
            let (action, note) = match &request.command {
                BillingCostCommand::Record(command) => ("cost", command.note.as_str()),
                BillingCostCommand::Delete(_) => ("delete-cost", ""),
                BillingCostCommand::Complete(_) => ("complete-costs", ""),
            };
            self.seen.lock().unwrap().push(format!(
                "command|{}|{}|{}|{}|{}",
                request.host,
                request.holding_slug,
                action,
                request.command.key().as_str(),
                note
            ));
            if let Some(error) = self.fail.clone() {
                return Err(error);
            }
            Ok(BillingHttpResponse {
                data: json!({"ok":true}),
                context: json!({"userId":"admin"}),
            })
        })
    }
}

fn target(backend: Arc<dyn BillingHttpBackend>) -> BillingHttpTarget {
    BillingHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

fn headers(overrides: &[(&'static str, &'static str)]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in [
        ("host", "acme.example.test"),
        ("content-type", "application/json"),
        ("idempotency-key", "billing-command-0001"),
        ("origin", "https://acme.example.test"),
        ("cookie", "__Host-erpx-product=session"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    for (name, value) in overrides {
        if value.is_empty() {
            headers.remove(*name);
        } else {
            headers.insert(*name, HeaderValue::from_static(value));
        }
    }
    headers
}

fn record_body() -> Vec<u8> {
    br#"{"version":4,"reason":"Reviewed actual cost","periodId":"period-10","category":"hosting","amountSatang":12500,"serviceStart":1778000000000,"serviceEnd":1780000000000,"note":" Provider hosting ","simulated":false,"evidenceId":"evidence-1"}"#.to_vec()
}

fn delete_body() -> Vec<u8> {
    br#"{"version":4,"reason":"Remove incorrect cost","costId":"cost-1"}"#.to_vec()
}

fn complete_body() -> Vec<u8> {
    br#"{"version":4,"reason":"Reviewed the whole month","periodId":"period-10","complete":true}"#
        .to_vec()
}

async fn reply(
    response: erpx_server_runtime::public::http_support::Response,
) -> (StatusCode, serde_json::Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[test]
fn workspace_forwards_the_exact_target_and_returns_private_json() {
    sqlx::test_block_on(async {
        let probe = Arc::new(Probe::default());
        let state = BillingHttpState::new(vec![target(probe.clone())]).unwrap();
        let response = state.workspace(&headers(&[]), "acme", None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        assert_eq!(
            response.headers().get(header::REFERRER_POLICY).unwrap(),
            "no-referrer"
        );
        assert_eq!(
            response
                .headers()
                .get(header::X_CONTENT_TYPE_OPTIONS)
                .unwrap(),
            "nosniff"
        );
        let (_, value) = reply(response).await;
        assert_eq!(value["data"]["sandbox"], true);
        assert_eq!(value["context"], json!({"userId":"admin"}));
        assert_eq!(
            probe.seen.lock().unwrap().as_slice(),
            ["workspace|acme.example.test|acme"]
        );
    });
}

#[test]
fn workspace_rejects_unknown_targets_origins_queries_and_malformed_backend_data() {
    sqlx::test_block_on(async {
        let state = BillingHttpState::new(vec![target(Arc::new(Probe::default()))]).unwrap();
        for (headers, expected) in [
            (
                headers(&[("host", "evil.example.test")]),
                StatusCode::NOT_FOUND,
            ),
            (
                headers(&[("origin", "https://evil.example.test")]),
                StatusCode::FORBIDDEN,
            ),
        ] {
            let response = state.workspace(&headers, "acme", None).await;
            assert_eq!(response.status(), expected);
        }
        let response = state.workspace(&headers(&[]), "other", None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let response = state
            .workspace(&headers(&[]), "acme", Some("holdingId=h002"))
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    });
}

#[test]
fn record_cost_forwards_the_exact_command_and_maps_failures() {
    sqlx::test_block_on(async {
        let probe = Arc::new(Probe::default());
        let state = BillingHttpState::new(vec![target(probe.clone())]).unwrap();
        let response = state
            .record_cost(&headers(&[]), "acme", None, &record_body())
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let (_, value) = reply(response).await;
        assert_eq!(value["data"], json!({"ok":true}));
        assert_eq!(
            probe.seen.lock().unwrap().as_slice(),
            ["command|acme.example.test|acme|cost|billing-command-0001| Provider hosting "]
        );

        for (headers, body) in [
            (headers(&[("content-type", "text/plain")]), record_body()),
            (headers(&[("idempotency-key", "")]), record_body()),
            (
                headers(&[]),
                br#"{"version":4,"reason":"Reviewed actual cost","periodId":"period-10","category":"hosting","amountSatang":12500,"serviceStart":1778000000000,"serviceEnd":1780000000000,"note":"Provider","evidenceId":"evidence-1","extra":1}"#.to_vec(),
            ),
            (
                headers(&[]),
                br#"{"version":4,"reason":"Reviewed actual cost","periodId":"period-10","category":"other","amountSatang":12500,"serviceStart":1778000000000,"serviceEnd":1780000000000,"note":"Provider","evidenceId":"evidence-1"}"#.to_vec(),
            ),
        ] {
            let response = state.record_cost(&headers, "acme", None, &body).await;
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
        let negative = br#"{"version":4,"reason":"Reviewed actual cost","periodId":"period-10","category":"hosting","amountSatang":-1,"serviceStart":1778000000000,"serviceEnd":1780000000000,"note":"Provider","evidenceId":"evidence-1"}"#;
        assert_eq!(
            state
                .record_cost(&headers(&[]), "acme", None, negative)
                .await
                .status(),
            StatusCode::CONFLICT
        );

        let mut duplicated = headers(&[]);
        duplicated.append(
            "idempotency-key",
            HeaderValue::from_static("billing-command-0002"),
        );
        let response = state
            .record_cost(&duplicated, "acme", None, &record_body())
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let conflict = Arc::new(Probe {
            fail: Some(BusinessError::new(
                FailureKind::Conflict,
                "VERSION_CONFLICT",
                "Billing changed. Refresh and review again.",
            )),
            ..Probe::default()
        });
        let state = BillingHttpState::new(vec![target(conflict)]).unwrap();
        let response = state
            .record_cost(&headers(&[]), "acme", None, &record_body())
            .await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let (_, value) = reply(response).await;
        assert_eq!(value["error"]["code"], "VERSION_CONFLICT");
    });
}

#[test]
fn delete_and_complete_commands_use_exact_bodies() {
    sqlx::test_block_on(async {
        let probe = Arc::new(Probe::default());
        let state = BillingHttpState::new(vec![target(probe.clone())]).unwrap();
        assert_eq!(
            state
                .delete_cost(&headers(&[]), "acme", None, &delete_body())
                .await
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            state
                .complete_costs(&headers(&[]), "acme", None, &complete_body())
                .await
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            probe.seen.lock().unwrap().as_slice(),
            [
                "command|acme.example.test|acme|delete-cost|billing-command-0001|",
                "command|acme.example.test|acme|complete-costs|billing-command-0001|"
            ]
        );
        for (kind, body) in [
            (
                "delete",
                br#"{"version":4,"reason":"Remove incorrect cost","costId":"cost-1","extra":1}"#
                    .to_vec(),
            ),
            (
                "complete",
                br#"{"version":4,"reason":"Reviewed the whole month","periodId":"period-10"}"#
                    .to_vec(),
            ),
        ] {
            let response = if kind == "delete" {
                state.delete_cost(&headers(&[]), "acme", None, &body).await
            } else {
                state
                    .complete_costs(&headers(&[]), "acme", None, &body)
                    .await
            };
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    });
}

#[test]
fn bad_receipts_and_invalid_configuration_fail_closed() {
    sqlx::test_block_on(async {
        struct Bad;

        impl BillingHttpBackend for Bad {
            fn workspace<'a>(
                &'a self,
                _: BillingWorkspaceHttpRequest<'a>,
            ) -> BillingHttpFuture<'a, BillingHttpResponse> {
                Box::pin(async {
                    Ok(BillingHttpResponse {
                        data: json!({"sandbox":false,"now":1,"account":null,"enrollment":null,"plans":[],"periods":[],"costs":[],"invoices":[],"audit":[],"jobs":[],"legacy":null}),
                        context: json!({}),
                    })
                })
            }

            fn command<'a>(
                &'a self,
                _: BillingCommandHttpRequest<'a>,
            ) -> BillingHttpFuture<'a, BillingHttpResponse> {
                Box::pin(async {
                    Ok(BillingHttpResponse {
                        data: json!({"ok":false}),
                        context: json!({}),
                    })
                })
            }
        }

        let state = BillingHttpState::new(vec![BillingHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Bad),
        }])
        .unwrap();
        assert_eq!(
            state.workspace(&headers(&[]), "acme", None).await.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            state
                .record_cost(&headers(&[]), "acme", None, &record_body())
                .await
                .status(),
            StatusCode::SERVICE_UNAVAILABLE
        );

        let backend = Arc::new(Probe::default());
        assert!(billing_router(vec![target(backend.clone())]).is_ok());
        assert!(billing_router(vec![target(backend.clone()), target(backend)]).is_err());
        for invalid in [
            BillingHttpTarget {
                host: "".into(),
                holding_slug: "acme".into(),
                origin: "https://acme.example.test".into(),
                backend: Arc::new(Probe::default()),
            },
            BillingHttpTarget {
                host: "acme.example.test".into(),
                holding_slug: "ACME".into(),
                origin: "https://acme.example.test".into(),
                backend: Arc::new(Probe::default()),
            },
            BillingHttpTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
                origin: "http://acme.example.test".into(),
                backend: Arc::new(Probe::default()),
            },
            BillingHttpTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
                origin: "https://acme.example.test/path".into(),
                backend: Arc::new(Probe::default()),
            },
        ] {
            assert!(billing_router(vec![invalid]).is_err());
        }
    });

}

#[test]
fn billing_context_serializes_the_exact_transport_contract() {
    use erpx_server_billing::public::workspace::BillingContext;
    for (platform, role) in [(true, "platform_admin"), (false, "holding_owner")] {
        let value = serde_json::to_value(BillingContext {
            user_id: "user-1".into(),
            holding_id: "h001".into(),
            platform,
            role: role.into(),
            assignment_id: "assignment-1".into(),
            expires_at: Some(1_760_000_000_000),
        })
        .unwrap();
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "assignmentId",
                "expiresAt",
                "holdingId",
                "platform",
                "role",
                "userId"
            ]
        );
        assert_eq!(value["platform"], serde_json::json!(platform));
        assert_eq!(value["expiresAt"], serde_json::json!(1_760_000_000_000i64));
        assert_eq!(value["holdingId"], serde_json::json!("h001"));
    }
}
