//! Finite source probes; joined HTTP/PG assertions remain separate.
use erpx_server_field_service::public::parts::FieldServiceRawOrderTarget;
use erpx_server_shared::public::{AssignmentId, CompanyId, UserId};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandField, CommandJson, WorkflowRecordIdentity,
    command_fingerprint_source_ordered,
};
fn property(v: &CommandJson, name: &str) -> CommandJson {
    v.property(name).unwrap().unwrap()
}
fn text(v: &CommandJson) -> String {
    v.string_utf8_storage().unwrap().unwrap()
}
fn array(v: &CommandJson) -> Vec<CommandJson> {
    v.array_elements().unwrap().unwrap()
}

#[test]
fn actual_source_target_fingerprints_keep_logical_utf16_separate_from_physical_text() {
    let probe = CommandJson::parse(include_str!(
        "../../../fixtures/field_service/rust-http/source-probes.v1.json"
    ))
    .unwrap();
    let actor = CommandActor {
        user_id: UserId::new("actor").unwrap(),
        assignment_id: AssignmentId::new("assignment").unwrap(),
        role: "technician".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: CommandField::Value("branch".into()),
        department_id: CommandField::Null,
        authority: CommandField::Omitted,
        iam: CommandField::Omitted,
        assignment_scope_kind: "holding".into(),
        assignment_scope_node_id: None,
    };
    let input=CommandJson::parse(r#"{"action":"reserve-part","expectedVersion":7,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":1800000010100,"reason":"reason"}"#).unwrap();
    let targets = array(&property(&probe, "targets"));
    assert_eq!(targets.len(), 8);
    for target in targets {
        let units = array(&property(&target, "originalUtf16"))
            .iter()
            .map(|n| n.as_str().parse::<u16>().unwrap())
            .collect::<Vec<_>>();
        let field = FieldServiceRawOrderTarget::from_utf16(units.clone());
        let logical = WorkflowRecordIdentity::from_utf16(units);
        assert_eq!(
            field.storage_text(),
            text(&property(&target, "physicalUtf8"))
        );
        assert_eq!(logical.physical_text(), field.storage_text());
        let actual = command_fingerprint_source_ordered(
            &actor,
            "field.order.reserve-part",
            &logical,
            &input,
        )
        .unwrap();
        assert_eq!(
            actual,
            text(&property(&property(&target, "fingerprint"), "digest")),
            "{}",
            text(&property(&target, "id"))
        );
    }
}
#[test]
fn actual_schema_probe_families_preserve_first_zod_messages() {
    use erpx_server_field_service::public::parts::{
        field_service_first_wire_message, normalize_field_service_parts,
    };
    let fixture = CommandJson::parse(include_str!(
        "../../../fixtures/field_service/rust-http/source-probes.v1.json"
    ))
    .unwrap();
    let base = serde_json::json!({"action":"issue-part","expectedVersion":7,"reservationId":"reservation","quantityMilli":1000,"entryDate":"2026-01-10","allocations":[],"reason":"reason"});
    let mut cases = Vec::new();
    let mut v = base.clone();
    v.as_object_mut().unwrap().remove("expectedVersion");
    cases.push(("missing-number", v));
    let mut v = base.clone();
    v["expectedVersion"] = "7".into();
    cases.push(("wrong-number", v));
    let mut v = base.clone();
    v.as_object_mut().unwrap().remove("reservationId");
    cases.push(("missing-string", v));
    let mut v = base.clone();
    v["reservationId"] = serde_json::Value::Null;
    cases.push(("wrong-string", v));
    let mut v = base.clone();
    v["reason"] = "x".repeat(3001).into();
    cases.push(("too-big-reason", v));
    let mut v = base.clone();
    v["allocations"] = serde_json::Value::Array(vec![serde_json::json!({}); 201]);
    cases.push(("too-big-allocations", v));
    let mut v = base.clone();
    v["entryDate"] = "2026-02-30".into();
    cases.push(("invalid-entry-date", v));
    let mut v = base;
    v["action"] = "wrong".into();
    cases.push(("invalid-action", v));
    assert_eq!(cases.len(), 8);
    let expected = array(&property(&fixture, "schema"));
    for (id, value) in cases {
        let bytes = serde_json::to_vec(&value).unwrap();
        let failure = match normalize_field_service_parts(&bytes) {
            Err(e) => e,
            Ok(_) => panic!("expected schema failure {id}"),
        };
        let entry = expected
            .iter()
            .find(|v| text(&property(v, "id")) == id)
            .unwrap();
        let message = text(&property(&property(entry, "first"), "message"));
        assert_eq!(
            field_service_first_wire_message(&bytes, &failure).as_deref(),
            Some(message.as_str()),
            "{id}"
        );
    }
}
#[test]
fn source_recorded_dispatch_branches_match_prepared_wire() {
    use erpx_service::field_service_http::{OtherPostBranch, SharedPostWire, classify_shared_post};
    let fixture = CommandJson::parse(include_str!(
        "../../../fixtures/field_service/rust-http/conformance.v1.json"
    ))
    .unwrap();
    let cases = array(&property(&fixture, "cases"));
    assert_eq!(cases.len(), 33);
    let mut compared = 0;
    for case in cases {
        let trace = array(&property(&case, "trace"));
        let owner = trace.iter().find_map(|v| {
            let v = array(v);
            match v.first().map(text).as_deref() {
                Some("owner.changeServiceOrder") => Some("owner.changeServiceOrder".to_owned()),
                Some("fake.dispatch") => Some(text(&v[1])),
                _ => None,
            }
        });
        let Some(owner) = owner else { continue };
        let wire = text(&property(&property(&case, "synthetic"), "wire"));
        let actual = classify_shared_post(wire.as_bytes());
        match (owner.as_str(), actual) {
            ("owner.changeServiceOrder", Ok(SharedPostWire::Parts { .. })) => (),
            ("owner.changeServiceOrder", Err(_))
                if property(&property(&case, "response"), "status").as_str() == "422" => {}
            (
                "request",
                Ok(SharedPostWire::Other {
                    branch: OtherPostBranch::Request,
                    ..
                }),
            ) => (),
            (
                "changeAgreement",
                Ok(SharedPostWire::Other {
                    branch: OtherPostBranch::AgreementChange,
                    ..
                }),
            ) => (),
            (
                "createAgreement",
                Ok(SharedPostWire::Other {
                    branch: OtherPostBranch::AgreementCreate,
                    ..
                }),
            ) => (),
            (
                "createOrder",
                Ok(SharedPostWire::Other {
                    branch: OtherPostBranch::OrderCreate,
                    ..
                }),
            ) => (),
            _ => panic!(
                "dispatch mismatch for {}: {}",
                text(&property(&case, "id")),
                owner
            ),
        }
        compared += 1;
    }
    assert!(compared > 0);
}

// Include the actual shared handler, with a fake executor only in this test crate.
#[path = "../../../src/composition/src/field_service_http_core.rs"]
mod field_service_http_core;
#[path = "../../../src/composition/src/field_service_http_wire.rs"]
mod field_service_http_wire;
use axum::{
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode, header},
};
use erpx_server_field_service::public::parts::{
    FieldServicePartsCommand, FieldServicePartsInput, FieldServicePartsPreparationPorts,
    FieldServicePreparedConsumptionFacts, prepare_new_field_service_parts,
};
use erpx_server_shared::public::{BusinessError, CommandKey, TimestampMillis};
use field_service_http_core::{
    PendingIngress, PostExecutor, PostResponse, TrustedTarget, router_for_executor,
};
use std::{future::Future, pin::Pin, sync::Arc};
use tower::ServiceExt;

#[derive(Clone)]
struct FakeExecutor {
    deny_auth: bool,
    deny_read: bool,
    deny_write: bool,
    no_source: bool,
    no_quote: bool,
}
struct FakeConsumption;
struct FakePorts;
impl FieldServicePartsPreparationPorts for FakePorts {
    type Source = ();
    type Reservation = ();
    type Consumption = FakeConsumption;
    type Error = BusinessError;
    async fn require_action(
        &mut self,
        _: &(),
        command: &FieldServicePartsCommand,
    ) -> Result<(), BusinessError> {
        let action = command.input().action();
        let state = match action {
            erpx_server_field_service::public::parts::FieldServicePartsActionKind::Reserve => {
                "scheduled"
            }
            erpx_server_field_service::public::parts::FieldServicePartsActionKind::Issue => {
                "in_visit"
            }
        };
        assert!(action.allows_state(state));
        Ok(())
    }
    fn application_now(&self) -> TimestampMillis {
        TimestampMillis::new(1_800_000_000_100)
    }
    async fn reservation(&mut self, _: &(), _: &str) -> Result<Option<()>, BusinessError> {
        Ok(Some(()))
    }
    async fn prepare_consumption(
        &mut self,
        _: &(),
        _: &(),
        _: &FieldServicePartsCommand,
        _: TimestampMillis,
    ) -> Result<FakeConsumption, BusinessError> {
        Ok(FakeConsumption)
    }
    fn consumption_facts(_: &FakeConsumption) -> FieldServicePreparedConsumptionFacts<'_> {
        FieldServicePreparedConsumptionFacts {
            consumption_id: "synthetic",
            journal_id: "synthetic",
            value_minor: 0,
        }
    }
}
impl PostExecutor for FakeExecutor {
    fn execute<'a>(
        &'a self,
        _host: String,
        _slug: String,
        _headers: HeaderMap,
        key: String,
        ingress: Arc<PendingIngress>,
    ) -> Pin<Box<dyn Future<Output = PostResponse> + Send + 'a>> {
        Box::pin(async move {
            use field_service_http_wire::{OtherPostBranch, SharedPostWire};
            if self.deny_auth {
                return PostResponse::error(
                    StatusCode::UNAUTHORIZED,
                    "UNAUTHENTICATED",
                    "Sign in to continue.",
                );
            }
            let (_, wire) = match ingress.decode_selected(false).await {
                Ok(value) => value,
                Err(error) => return PostResponse::wire(error),
            };
            match wire {
                SharedPostWire::Other { branch, id_utf16 } => {
                    match branch {
                        OtherPostBranch::AgreementChange | OtherPostBranch::OrderChange => {
                            assert!(id_utf16.is_some())
                        }
                        OtherPostBranch::AgreementCreate | OtherPostBranch::OrderCreate => {
                            assert!(id_utf16.is_none())
                        }
                        OtherPostBranch::Request => (),
                    }
                    let name = match branch {
                        OtherPostBranch::Request => "request",
                        OtherPostBranch::AgreementChange => "changeAgreement",
                        OtherPostBranch::AgreementCreate => "createAgreement",
                        OtherPostBranch::OrderChange => "changeOrder",
                        OtherPostBranch::OrderCreate => "createOrder",
                    };
                    PostResponse::Success(
                        CommandJson::parse(&format!("{{\"fakeDispatch\":\"{name}\"}}")).unwrap(),
                    )
                }
                SharedPostWire::Parts { target, normalized } => {
                    if self.deny_read {
                        return PostResponse::error(
                            StatusCode::FORBIDDEN,
                            "WORK_ROLE_UNAVAILABLE",
                            "The original Service assignment is no longer current.",
                        );
                    }
                    if self.deny_write {
                        return PostResponse::error(
                            StatusCode::FORBIDDEN,
                            "WORK_ROLE_UNAVAILABLE",
                            "The original Service assignment is no longer current.",
                        );
                    }
                    if self.no_source {
                        return PostResponse::error(
                            StatusCode::NOT_FOUND,
                            "FIELD_SERVICE_ORDER_NOT_FOUND",
                            "The original Service source is not available in this workspace.",
                        );
                    }
                    if self.no_quote
                        && matches!(normalized.input(), FieldServicePartsInput::Issue { .. })
                    {
                        return PostResponse::error(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            "MANUAL_RATE_REQUIRED",
                            "Synthetic quote unavailable.",
                        );
                    }
                    let key = match CommandKey::new(key) {
                        Ok(key) => key,
                        Err(error) => return PostResponse::business(error),
                    };
                    let action = normalized.input().action();
                    let version = normalized.input().expected_version() + 1;
                    let command=normalized.into_physical_command(key,erpx_server_field_service::public::parts::FieldServicePhysicalOrderId::from_storage(target.storage_text()));
                    if let Err(error) =
                        prepare_new_field_service_parts(&mut FakePorts, &(), &command).await
                    {
                        return PostResponse::business(error);
                    }
                    let id = CommandJson::from_utf16_string(target.original_utf16()).unwrap();
                    let status=match action {erpx_server_field_service::public::parts::FieldServicePartsActionKind::Reserve=>"scheduled",erpx_server_field_service::public::parts::FieldServicePartsActionKind::Issue=>"in_visit"};
                    PostResponse::Success(
                        CommandJson::parse(&format!(
                            "{{\"id\":{},\"version\":{},\"status\":\"{}\"}}",
                            id.as_str(),
                            version,
                            status
                        ))
                        .unwrap(),
                    )
                }
            }
        })
    }
}
#[tokio::test]
async fn shared_http_handler_matches_all_33_source_envelopes_at_declared_fake_port_scope() {
    let fixture = CommandJson::parse(include_str!(
        "../../../fixtures/field_service/rust-http/conformance.v1.json"
    ))
    .unwrap();
    let cases = array(&property(&fixture, "cases"));
    assert_eq!(cases.len(), 33);
    for case in cases {
        let synthetic = property(&case, "synthetic");
        let options = property(&synthetic, "options");
        let flag = |name| property(&options, name).as_str() == "true";
        let executor = FakeExecutor {
            deny_auth: flag("denyAuth"),
            deny_read: false,
            deny_write: flag("denyWrite"),
            no_source: flag("noSource"),
            no_quote: flag("noQuote"),
        };
        let router = router_for_executor(
            vec![TrustedTarget {
                host: "holding.example.test".into(),
                slug: "acme".into(),
                origin: text(&property(&synthetic, "expectedOrigin")),
            }],
            executor,
        );
        let mut builder = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/field-service");
        for pair in array(&property(&synthetic, "headers")) {
            let pair = array(&pair);
            builder = builder.header(text(&pair[0]), text(&pair[1]));
        }
        let response = router
            .oneshot(
                builder
                    .body(Body::from(text(&property(&synthetic, "wire"))))
                    .unwrap(),
            )
            .await
            .unwrap();
        let expected = property(&case, "response");
        let status = property(&expected, "status")
            .as_str()
            .parse::<u16>()
            .unwrap();
        assert_eq!(
            response.status().as_u16(),
            status,
            "{}",
            text(&property(&case, "id"))
        );
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            String::from_utf8(body.to_vec()).unwrap(),
            text(&property(&expected, "body")),
            "{}",
            text(&property(&case, "id"))
        );
        assert_eq!(
            headers.get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
        assert_eq!(
            headers
                .get(header::CACHE_CONTROL)
                .map(|v| v.to_str().unwrap()),
            if status == 200 {
                Some("private, no-store")
            } else {
                None
            }
        );
    }
}

#[tokio::test]
async fn shared_http_handler_matches_six_permission_and_json_source_controls() {
    let fixture = CommandJson::parse(include_str!(
        "../../../fixtures/field_service/rust-http/source-probes.v1.json"
    ))
    .unwrap();
    let expected = array(&property(&fixture, "routes"));
    assert_eq!(expected.len(), 6);
    let valid = r#"{"id":"order","action":"issue-part","expectedVersion":7,"reservationId":"reservation","quantityMilli":1000,"reason":"reason"}"#;
    let invalid = r#"{"id":"order","action":"issue-part","expectedVersion":0,"reservationId":"reservation","quantityMilli":1000,"reason":"reason"}"#;
    for (id, body, deny_auth, deny_read, deny_write) in [
        ("valid-denied-write", valid, false, false, true),
        ("valid-denied-read", valid, false, true, false),
        ("schema-denied-read", invalid, false, true, false),
        ("schema-denied-write", invalid, false, false, true),
        ("json-denied-write", "{", false, false, true),
        ("auth-no-json", "{", true, false, false),
    ] {
        let executor = FakeExecutor {
            deny_auth,
            deny_read,
            deny_write,
            no_source: false,
            no_quote: false,
        };
        let router = router_for_executor(
            vec![TrustedTarget {
                host: "holding.example.test".into(),
                slug: "acme".into(),
                origin: "https://holding.example.test".into(),
            }],
            executor,
        );
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/field-service")
            .header("host", "holding.example.test")
            .header("idempotency-key", "valid-key-123")
            .body(Body::from(body))
            .unwrap();
        let response = router.oneshot(request).await.unwrap();
        let observation = expected
            .iter()
            .find(|v| text(&property(v, "id")) == id)
            .unwrap();
        let expected_response = property(observation, "response");
        assert_eq!(
            response.status().as_u16(),
            property(&expected_response, "status")
                .as_str()
                .parse::<u16>()
                .unwrap(),
            "{id}"
        );
        assert_eq!(
            String::from_utf8(
                to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap()
                    .to_vec()
            )
            .unwrap(),
            text(&property(&expected_response, "body")),
            "{id}"
        );
    }
}

#[test]
fn node_headers_bytestring_preserves_each_original_octet() {
    // Observed Node 26.11.1 header-bytes.v1.json; these are byte-to-unit controls.
    for bytes in [&[233_u8][..], &[195, 169][..], &[128][..]] {
        let mut headers = HeaderMap::new();
        headers.append(
            "idempotency-key",
            axum::http::HeaderValue::from_bytes(bytes).unwrap(),
        );
        let actual = field_service_http_core::joined(&headers, "idempotency-key")
            .unwrap()
            .encode_utf16()
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            bytes
                .iter()
                .map(|byte| u16::from(*byte))
                .collect::<Vec<_>>()
        );
    }
}

#[tokio::test]
async fn missing_origin_and_key_outrank_unresolved_host() {
    let executor = FakeExecutor {
        deny_auth: true,
        deny_read: false,
        deny_write: false,
        no_source: false,
        no_quote: false,
    };
    let router = router_for_executor(
        vec![TrustedTarget {
            host: "holding.example.test".into(),
            slug: "acme".into(),
            origin: "https://holding.example.test".into(),
        }],
        executor,
    );
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/holdings/acme/field-service")
                .body(Body::from("{"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert_eq!(
        body,
        r#"{"error":{"code":"COMMAND_KEY_REQUIRED","message":"Idempotency-Key is required."}}"#
    );
}

#[tokio::test]
async fn shared_handler_retains_source_review_numeric_and_nested_object_messages() {
    let fixture = CommandJson::parse(include_str!(
        "../../../fixtures/field_service/rust-http/schema-review.v1.json"
    ))
    .unwrap();
    let cases = array(&property(&fixture, "cases"));
    assert_eq!(cases.len(), 3);
    for case in cases {
        let request = property(&case, "request");
        let wire = text(&property(&request, "body"));
        let expected = property(&case, "response");
        let issue = property(&case, "firstIssue");
        let failure = match erpx_server_field_service::public::parts::normalize_field_service_parts(
            wire.as_bytes(),
        ) {
            Err(error) => error,
            Ok(_) => panic!("expected source schema issue"),
        };
        let first = &failure.issues[0];
        assert_eq!(first.code, text(&property(&issue, "code")));
        let path = first
            .path
            .iter()
            .map(|part| match part {
                erpx_server_field_service::public::parts::FieldServiceIssuePath::Field(value) => {
                    serde_json::json!(value)
                }
                erpx_server_field_service::public::parts::FieldServiceIssuePath::Index(value) => {
                    serde_json::json!(value)
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            serde_json::to_string(&path).unwrap(),
            property(&issue, "path").as_str()
        );
        assert_eq!(first.message, text(&property(&issue, "message")));
        let router = router_for_executor(
            vec![TrustedTarget {
                host: "holding.example.test".into(),
                slug: "holding".into(),
                origin: "https://holding.example.test".into(),
            }],
            FakeExecutor {
                deny_auth: false,
                deny_read: false,
                deny_write: false,
                no_source: false,
                no_quote: false,
            },
        );
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/holding/field-service")
            .header("host", "holding.example.test")
            .header("idempotency-key", "valid-key-123")
            .body(Body::from(wire))
            .unwrap();
        let response = router.oneshot(request).await.unwrap();
        assert_eq!(
            response.status().as_u16(),
            property(&expected, "status")
                .as_str()
                .parse::<u16>()
                .unwrap()
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            String::from_utf8(body.to_vec()).unwrap(),
            text(&property(&expected, "body"))
        );
    }
}
