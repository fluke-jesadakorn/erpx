//! Actual owner-local HTTP router tests for the native Learning owner.
use erpx_server_learning::public::learning_http::{
    LearningHttpBackend, LearningHttpCommand, LearningHttpFuture, LearningHttpRequest,
    LearningHttpResponse, LearningHttpTarget, learning_http_router,
};
use erpx_server_runtime::public::http_support::{serde_json, serve};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

struct Noop;

impl LearningHttpBackend for Noop {
    fn execute<'a>(
        &'a self,
        _: LearningHttpRequest<'a>,
    ) -> LearningHttpFuture<'a, LearningHttpResponse> {
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

impl LearningHttpBackend for Probe {
    fn execute<'a>(
        &'a self,
        request: LearningHttpRequest<'a>,
    ) -> LearningHttpFuture<'a, LearningHttpResponse> {
        Box::pin(async move {
            let seen = match &request.command {
                LearningHttpCommand::Workspace { personal } => format!("workspace|{personal}"),
                LearningHttpCommand::Detail { assignment_id } => {
                    format!("detail|{assignment_id}")
                }
                LearningHttpCommand::EligibilityFacts { query } => format!(
                    "eligibility|{}|{}|{}",
                    query.person_id, query.eligibility_code, query.effective_date
                ),
                LearningHttpCommand::CreateCourse {
                    input,
                    idempotency_key,
                } => format!("course-create|{idempotency_key}|{}", input.title),
                LearningHttpCommand::ChangeCourse {
                    course_id,
                    input,
                    idempotency_key,
                } => format!(
                    "course-change|{course_id}|{idempotency_key}|{}",
                    input.action.as_str()
                ),
                LearningHttpCommand::CreateAssignment {
                    input,
                    idempotency_key,
                } => format!(
                    "assignment-create|{idempotency_key}|{}|{}",
                    input.course_id, input.learner_id
                ),
                LearningHttpCommand::ChangeAssignment {
                    assignment_id,
                    input,
                    idempotency_key,
                } => format!(
                    "assignment-change|{assignment_id}|{idempotency_key}|{}",
                    input.action.as_str()
                ),
                LearningHttpCommand::RevokeCertificate {
                    certificate_id,
                    input,
                    idempotency_key,
                } => format!(
                    "certificate-revoke|{certificate_id}|{idempotency_key}|{}",
                    input.expected_assignment_version
                ),
                LearningHttpCommand::ConsumeBenefit {
                    input,
                    idempotency_key,
                } => format!(
                    "benefit-consume|{idempotency_key}|{}|{}|{}",
                    input.person_id, input.required_code, input.election_id
                ),
            };
            self.seen.lock().unwrap().push(seen);
            if self.fail {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "LEARNING_STATE_CHANGED",
                    "Synthetic changed Learning source.",
                ));
            }
            Ok(LearningHttpResponse {
                data: match request.command {
                    LearningHttpCommand::Workspace { .. } => serde_json::json!({
                        "courses": [],
                        "assignments": [],
                        "people": [],
                        "canWrite": true,
                        "canReview": true,
                        "actorId": "reviewer"
                    }),
                    LearningHttpCommand::Detail { .. } => serde_json::json!({
                        "assignment": {
                            "id": "learning-assignment-1",
                            "status": "completed",
                            "attendance": "attended",
                            "score_bps": 9500,
                            "result_evidence": "Provider result and practical check",
                            "completion_reason": "Course and assessment completed"
                        },
                        "history": [],
                        "handoff": null,
                        "certificate": null
                    }),
                    LearningHttpCommand::EligibilityFacts { .. } => serde_json::json!({
                        "person_id": "person-learner",
                        "eligibility_code": "equipment_operator",
                        "effective_date": "2026-11-04",
                        "eligible": true,
                        "certificates": []
                    }),
                    _ => serde_json::json!({
                        "id": "learning-assignment-1",
                        "status": "assigned",
                        "version": 1
                    }),
                },
                context: serde_json::json!({"userId": "reviewer"}),
            })
        })
    }
}

#[test]
fn target_configuration_requires_distinct_exact_holdings() {
    let backend = Arc::new(Noop);
    let target = || LearningHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: backend.clone(),
    };
    assert!(learning_http_router(vec![target()]).is_ok());
    assert!(learning_http_router(vec![target(), target()]).is_err());
    assert!(
        learning_http_router(vec![LearningHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        learning_http_router(vec![LearningHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        learning_http_router(vec![LearningHttpTarget {
            holding_slug: "Acme".into(),
            ..target()
        }])
        .is_err()
    );
}

async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value, String) {
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
        serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
        text,
    )
}

fn get(path: &str, host: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")
}

fn post(path: &str, host: &str, key: Option<&str>, body: &str) -> String {
    let key = key
        .map(|value| format!("Idempotency-Key: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n{key}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn target(host: &str, slug: &str, seen: Arc<Mutex<Vec<String>>>, fail: bool) -> LearningHttpTarget {
    LearningHttpTarget {
        host: host.into(),
        holding_slug: slug.into(),
        origin: format!("https://{host}"),
        backend: Arc::new(Probe { seen, fail }),
    }
}

#[tokio::test]
async fn actual_router_forwards_every_learning_command_and_bounded_read() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = learning_http_router(vec![
        target("acme.example.test", "acme", seen.clone(), false),
        target("fail.example.test", "fail", seen.clone(), true),
    ])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, body, headers) = http(
        addr,
        get(
            "/api/v1/holdings/acme/learning/workspace?personal=true",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["actorId"], "reviewer");
    assert_eq!(body["context"]["userId"], "reviewer");
    assert!(headers.contains("cache-control: private, no-store"));
    assert!(headers.contains("referrer-policy: no-referrer"));
    assert!(headers.contains("x-content-type-options: nosniff"));

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/learning/assignments/learning-assignment-1",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["assignment"]["id"], "learning-assignment-1");
    assert_eq!(body["data"]["assignment"]["attendance"], "attended");
    assert_eq!(body["data"]["assignment"]["score_bps"], 9500);
    assert_eq!(
        body["data"]["assignment"]["result_evidence"],
        "Provider result and practical check"
    );

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/learning/eligibility/person-learner?code=equipment_operator&effectiveDate=2026-11-04",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["person_id"], "person-learner");
    assert_eq!(body["data"]["eligible"], true);

    let course = r#"{"title":"Safe equipment operation","description":"Practical operating procedure.","provider":"Internal academy","durationMinutes":90,"requiredFor":"equipment_operator"}"#;
    let (status, body, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/learning/courses",
            "acme.example.test",
            Some("learning-course-command-1"),
            course,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["data"]["status"], "assigned");

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/learning/courses/learning-course-1",
            "acme.example.test",
            Some("learning-course-command-2"),
            r#"{"action":"publish","expectedVersion":1,"reason":"Independent review completed"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);

    let assignment = r#"{"courseId":"learning-course-1","courseVersion":2,"learnerId":"learner","dueDate":"2026-11-30","reason":"Required before operating equipment"}"#;
    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/learning/assignments",
            "acme.example.test",
            Some("learning-assignment-command-1"),
            assignment,
        ),
    )
    .await;
    assert_eq!(status, 200);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/learning/assignments/learning-assignment-1",
            "acme.example.test",
            Some("learning-assignment-command-2"),
            r#"{"action":"complete","expectedVersion":2,"reason":"Course and assessment completed","attendance":"attended","scoreBps":9500,"evidence":"Provider result and practical check","certificateValidFrom":"2026-11-04","certificateExpiresOn":"2028-11-04"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/learning/certificates/learning-certificate-key-1/revoke",
            "acme.example.test",
            Some("learning-certificate-command-1"),
            r#"{"expectedAssignmentVersion":3,"reason":"Independent owner retires this certificate"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/learning/benefits/consumption",
            "acme.example.test",
            Some("benefit-election-learning-1"),
            r#"{"personId":"person-learner","requiredCode":"equipment_operator","effectiveDate":"2026-11-04","electionId":"election-learning"}"#,
        ),
    )
    .await;
    assert_eq!(status, 200);

    let seen = seen.lock().unwrap().clone();
    assert_eq!(
        seen,
        vec![
            "workspace|true",
            "detail|learning-assignment-1",
            "eligibility|person-learner|equipment_operator|2026-11-04",
            "course-create|learning-course-command-1|Safe equipment operation",
            "course-change|learning-course-1|learning-course-command-2|publish",
            "assignment-create|learning-assignment-command-1|learning-course-1|learner",
            "assignment-change|learning-assignment-1|learning-assignment-command-2|complete",
            "certificate-revoke|learning-certificate-key-1|learning-certificate-command-1|3",
            "benefit-consume|benefit-election-learning-1|person-learner|equipment_operator|election-learning",
        ]
    );

    let (status, body, _) = http(
        addr,
        get(
            "/api/v1/holdings/fail/learning/workspace",
            "fail.example.test",
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["error"]["code"], "LEARNING_STATE_CHANGED");
}

#[tokio::test]
async fn router_rejects_unknown_scope_bad_keys_bodies_and_queries() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let router = learning_http_router(vec![target(
        "acme.example.test",
        "acme",
        seen.clone(),
        false,
    )])
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let (status, _, _) = http(
        addr,
        get(
            "/api/v1/holdings/other/learning/workspace",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);

    let (status, _, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/learning/workspace?personal=maybe",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/learning/workspace?personal=true&personal=false",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/learning/courses",
            "acme.example.test",
            None,
            "{}",
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/learning/courses",
            "acme.example.test",
            Some("short"),
            "{}",
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, body, _) = http(
        addr,
        "POST /api/v1/holdings/acme/learning/courses HTTP/1.1\r\nHost: acme.example.test\r\nContent-Type: text/plain\r\nIdempotency-Key: learning-course-command-3\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string(),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["error"]["code"], "LEARNING_REQUEST_INVALID");

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/learning/courses",
            "acme.example.test",
            Some("learning-course-command-4"),
            r#"{"title":"Safe equipment","description":"Practical procedure","provider":"Academy","durationMinutes":90,"requiredFor":null,"extra":"field"}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);

    let body = "{\"action\":\"start\",\"expectedVersion\":1,\"reason\":\"Learner starts\"}";
    let (status, _, _) = http(
        addr,
        format!(
            "GET /api/v1/holdings/acme/learning/workspace HTTP/1.1\r\nHost: acme.example.test\r\nContent-Type: application/json\r\nIdempotency-Key: learning-command-5\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/learning/assignments/learning-assignment-1",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 200);

    let (status, _, _) = http(
        addr,
        get(
            "/api/v1/holdings/acme/learning/assignments/bad%20id",
            "acme.example.test",
        ),
    )
    .await;
    assert_eq!(status, 404);

    for path in [
        "/api/v1/holdings/acme/learning/eligibility/person-learner",
        "/api/v1/holdings/acme/learning/eligibility/person-learner?code=equipment_operator",
        "/api/v1/holdings/acme/learning/eligibility/person-learner?effectiveDate=2026-11-04",
        "/api/v1/holdings/acme/learning/eligibility/person-learner?code=equipment_operator&code=other&effectiveDate=2026-11-04",
        "/api/v1/holdings/acme/learning/eligibility/person-learner?code=equipment_operator&effectiveDate=2026-11-04&extra=1",
        "/api/v1/holdings/acme/learning/eligibility/person-learner?code=equipment_operator&effectiveDate=2026-02-30",
        "/api/v1/holdings/acme/learning/eligibility/person-learner?code=%20equipment_operator&effectiveDate=2026-11-04",
        "/api/v1/holdings/acme/learning/eligibility/bad%20id?code=equipment_operator&effectiveDate=2026-11-04",
    ] {
        let (status, _, _) = http(addr, get(path, "acme.example.test")).await;
        assert_eq!(
            status,
            if path.contains("bad%20id") { 404 } else { 422 },
            "{path}"
        );
    }

    let (status, _, _) = http(
        addr,
        "GET /api/v1/holdings/acme/learning/eligibility/person-learner?code=equipment_operator&effectiveDate=2026-11-04 HTTP/1.1\r\nHost: acme.example.test\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string(),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/learning/benefits/consumption",
            "acme.example.test",
            Some("benefit-election-learning-2"),
            r#"{"personId":"person-learner","requiredCode":"equipment_operator","effectiveDate":"2026-11-04","electionId":"election-learning","extra":true}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, _, _) = http(
        addr,
        post(
            "/api/v1/holdings/acme/learning/benefits/consumption",
            "acme.example.test",
            None,
            r#"{"personId":"person-learner","requiredCode":"equipment_operator","effectiveDate":"2026-11-04","electionId":"election-learning"}"#,
        ),
    )
    .await;
    assert_eq!(status, 422);

    assert_eq!(
        seen.lock().unwrap().clone(),
        vec!["detail|learning-assignment-1".to_owned()]
    );
}
