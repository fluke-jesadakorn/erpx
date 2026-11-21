//! Owner-local Organization setup HTTP ingress over the actual PostgreSQL
//! owner unit. The test backend supplies the same actor shape the composition
//! root will build from the normal Account session; transport still enforces
//! exact Host/Holding/origin/Idempotency-Key/body policy.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_organization::public::organization_nodes::{
    OrganizationAuthority, OrganizationCommand, OrganizationCommandActor,
};
use erpx_server_organization::public::organization_setup::*;
use erpx_server_organization::public::organization_setup_http::*;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, Response, StatusCode, body::to_bytes, header, serde_json,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CompanyId, FailureKind, HoldingId, TimestampMillis, UserId,
};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone, Copy)]
struct TestClock;
impl Clock for TestClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1_800_000_000_000)
    }
}

fn actor(permissions: &[&str], assignment: &str, role: &str) -> OrganizationCommandActor {
    OrganizationCommandActor {
        holding_id: HoldingId::new("h001").unwrap(),
        command: OrganizationCommand {
            user_id: UserId::new("owner").unwrap(),
            assignment_id: AssignmentId::new(assignment).unwrap(),
            role: role.into(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: None,
            department_id: None,
            authority: Some(OrganizationAuthority {
                session_id: "session-owner".into(),
                revision: 7,
                assignment_version: 1,
            }),
            iam_json: json!({
                "principalId": "principal-owner",
                "permissions": permissions,
                "grantablePermissions": [],
                "personId": "person-owner",
                "personUserIds": ["owner"],
                "freshUntil": null
            })
            .to_string(),
            assignment_scope_kind: "holding".into(),
            assignment_scope_node_id: None,
        },
        modules: vec!["people".into()],
        permissions: permissions.iter().map(|value| (*value).into()).collect(),
    }
}

fn owner() -> OrganizationCommandActor {
    actor(
        &["organization.manage"],
        "assignment-owner",
        "custom_organization",
    )
}

fn limited() -> OrganizationCommandActor {
    actor(
        &["organization.manage"],
        "assignment-limited",
        "custom_people_only",
    )
}

fn failure(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "ORGANIZATION_TEST_UNAVAILABLE",
            "The test organization unit is unavailable.",
        ),
    }
}

struct PgBackend {
    pool: DatabasePool,
    actor: OrganizationCommandActor,
}

impl PgBackend {
    fn new(pool: DatabasePool, actor: OrganizationCommandActor) -> Self {
        Self { pool, actor }
    }
}

impl OrganizationSetupHttpBackend for PgBackend {
    fn install_levels<'a>(
        &'a self,
        request: InstallLevelsHttpRequest<'a>,
    ) -> OrganizationSetupHttpFuture<'a> {
        let pool = self.pool.clone();
        let actor = self.actor.clone();
        let key = request.key.to_owned();
        Box::pin(async move {
            PgUnitOfWork::new(pool)
                .run_once(TransactionOptions::default(), move |connection| {
                    Box::pin(async move {
                        let mut ports = PgOrganizationSetup::new(connection);
                        install_levels(&mut ports, actor, &key, &TestClock).await
                    })
                })
                .await
                .map_err(failure)
        })
    }

    fn save_template<'a>(
        &'a self,
        request: SaveTemplateHttpRequest<'a>,
    ) -> OrganizationSetupHttpFuture<'a> {
        let pool = self.pool.clone();
        let actor = self.actor.clone();
        let key = request.key.to_owned();
        let template_id = request.template_id.map(str::to_owned);
        let input = request.input.clone();
        Box::pin(async move {
            PgUnitOfWork::new(pool)
                .run_once(TransactionOptions::default(), move |connection| {
                    Box::pin(async move {
                        let mut ports = PgOrganizationSetup::new(connection);
                        save_template(
                            &mut ports,
                            actor,
                            &key,
                            template_id.as_deref(),
                            input,
                            &TestClock,
                        )
                        .await
                    })
                })
                .await
                .map_err(failure)
        })
    }

    fn apply_template<'a>(
        &'a self,
        request: ApplyTemplateHttpRequest<'a>,
    ) -> OrganizationSetupHttpFuture<'a> {
        let pool = self.pool.clone();
        let actor = self.actor.clone();
        let key = request.key.to_owned();
        let template_id = request.template_id.to_owned();
        let input = request.input.clone();
        Box::pin(async move {
            PgUnitOfWork::new(pool)
                .run_once(TransactionOptions::default(), move |connection| {
                    Box::pin(async move {
                        let mut ports = PgOrganizationSetup::new(connection);
                        apply_template(&mut ports, actor, &key, &template_id, input, &TestClock)
                            .await
                    })
                })
                .await
                .map_err(failure)
        })
    }
}

async fn seed(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) \
         VALUES('company','company','acme','Synthetic Acme',NULL,'company',NULL,'active'), \
               ('branch','branch','north','North','company','company','branch','active'), \
               ('department','department','service','Service','branch','company','branch','active');\
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) \
         VALUES('*','custom_organization','Synthetic organization authority',false,1,'active'), \
               ('*','custom_people_only','Synthetic people-only authority',false,1,'active');\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','custom_organization',1,'[\"organization.manage\"]','[]','[\"holding\"]','fixture',0), \
               ('*','custom_people_only',1,'[\"people.manage\"]','[]','[\"holding\"]','fixture',0);\
         UPDATE iam_roles SET published_revision=1 WHERE company_id='*' AND id IN ('custom_organization','custom_people_only');\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at) \
         VALUES('person-owner','owner','company','Owner','TH','active',1,'fixture',0,0);\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) \
         VALUES('company','owner','local:owner','person-owner','active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) \
         VALUES('assignment-owner','owner','custom_organization','[\"people\"]','company',0,9999999999999,1,'holding'), \
               ('assignment-limited','owner','custom_people_only','[\"people\"]','company',0,9999999999999,1,'holding');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn state(pool: DatabasePool, actor: OrganizationCommandActor) -> OrganizationSetupHttpState {
    OrganizationSetupHttpState::new(vec![OrganizationSetupHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(PgBackend::new(pool, actor)),
    }])
    .unwrap()
}

fn headers(key: Option<&str>, origin: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("host", HeaderValue::from_static("acme.example.test"));
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    if let Some(key) = key {
        headers.insert("idempotency-key", HeaderValue::from_str(key).unwrap());
    }
    if let Some(origin) = origin {
        headers.insert("origin", HeaderValue::from_str(origin).unwrap());
    }
    headers
}

fn template_body(name: &str, expected_version: i64) -> Value {
    json!({
        "name": name,
        "positions": [
            {"key":"ceo","title":"CEO","levelNumber":1,"reportsToKey":null,"isCeo":true},
            {"key":"manager","title":"Manager","levelNumber":3,"reportsToKey":"ceo","isCeo":false,"rosterRequired":true}
        ],
        "expectedVersion": expected_version
    })
}

async fn body(response: Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn http_install_levels_persists_replays_and_rejects_origin_or_key_abuse() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let api = state(pool.clone(), owner());
        let response = api
            .install_levels(
                &headers(
                    Some("organization-levels-http-0001"),
                    Some("https://acme.example.test"),
                ),
                "acme",
                None,
                b"{}",
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        assert_eq!(body(response).await, json!({"data": {"id": "company"}}));
        let levels: i64 =
            sqlx::query_scalar("SELECT count(*) FROM staff_levels WHERE company_id='company'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(levels, 6);

        let replay = api
            .install_levels(
                &headers(
                    Some("organization-levels-http-0001"),
                    Some("https://acme.example.test"),
                ),
                "acme",
                None,
                b"{}",
            )
            .await;
        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(body(replay).await, json!({"data": {"id": "company"}}));

        let wrong_origin = api
            .install_levels(
                &headers(
                    Some("organization-levels-http-0002"),
                    Some("https://attacker.example.test"),
                ),
                "acme",
                None,
                b"{}",
            )
            .await;
        assert_eq!(wrong_origin.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            body(wrong_origin).await["error"]["code"],
            "ORIGIN_FORBIDDEN"
        );

        let missing_key = api
            .install_levels(
                &headers(None, Some("https://acme.example.test")),
                "acme",
                None,
                b"{}",
            )
            .await;
        assert_eq!(missing_key.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            body(missing_key).await["error"]["code"],
            "COMMAND_KEY_REQUIRED"
        );

        let unknown_holding = api
            .install_levels(
                &headers(
                    Some("organization-levels-http-0003"),
                    Some("https://acme.example.test"),
                ),
                "northstar",
                None,
                b"{}",
            )
            .await;
        assert_eq!(unknown_holding.status(), StatusCode::NOT_FOUND);
    })
    .await;
}

#[tokio::test]
async fn http_template_save_and_apply_persist_and_fail_closed() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let api = state(pool.clone(), owner());
        let install = api
            .install_levels(
                &headers(
                    Some("organization-levels-http-apply-0001"),
                    Some("https://acme.example.test"),
                ),
                "acme",
                None,
                b"{}",
            )
            .await;
        assert_eq!(install.status(), StatusCode::OK);

        let saved = api
            .save_template(
                &headers(
                    Some("organization-template-http-0001"),
                    Some("https://acme.example.test"),
                ),
                "acme",
                None,
                None,
                template_body("Standard organization", 0)
                    .to_string()
                    .as_bytes(),
            )
            .await;
        assert_eq!(saved.status(), StatusCode::OK);
        let saved = body(saved).await;
        assert_eq!(saved["data"]["version"], 1);
        let template_id = saved["data"]["id"].as_str().unwrap().to_owned();

        let applied = api
            .apply_template(
                &headers(
                    Some("organization-template-http-apply-0001"),
                    Some("https://acme.example.test"),
                ),
                "acme",
                &template_id,
                None,
                json!({
                    "expectedVersion": 1,
                    "branchId": "branch",
                    "departmentId": "department",
                    "reason": "Apply the reviewed organization template"
                })
                .to_string()
                .as_bytes(),
            )
            .await;
        assert_eq!(applied.status(), StatusCode::OK);
        let applied = body(applied).await;
        assert_eq!(applied["data"]["id"], template_id);
        assert_eq!(applied["data"]["positionIds"].as_array().unwrap().len(), 2);
        let positions: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM organization_positions WHERE company_id='company'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(positions, 2);

        let stale = api
            .apply_template(
                &headers(
                    Some("organization-template-http-stale-0001"),
                    Some("https://acme.example.test"),
                ),
                "acme",
                &template_id,
                None,
                json!({
                    "expectedVersion": 2,
                    "branchId": null,
                    "departmentId": null,
                    "reason": "Apply the reviewed organization template"
                })
                .to_string()
                .as_bytes(),
            )
            .await;
        assert_eq!(stale.status(), StatusCode::CONFLICT);
        assert_eq!(body(stale).await["error"]["code"], "CONFLICT");

        let malformed = api
            .save_template(
                &headers(
                    Some("organization-template-http-malformed-0001"),
                    Some("https://acme.example.test"),
                ),
                "acme",
                None,
                None,
                b"{\"name\":\"x\"}",
            )
            .await;
        assert_eq!(malformed.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body(malformed).await["error"]["code"], "VALIDATION_FAILED");

        let limited = state(pool.clone(), limited());
        let denied = limited
            .install_levels(
                &headers(
                    Some("organization-levels-http-limited-0001"),
                    Some("https://acme.example.test"),
                ),
                "acme",
                None,
                b"{}",
            )
            .await;
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            body(denied).await["error"]["code"],
            "ORGANIZATION_AUTHORITY_CHANGED"
        );
    })
    .await;
}

#[tokio::test]
async fn http_rejects_non_json_and_extra_scope_without_touching_the_owner() {
    fixture::with_database(1, |pool| async move {
        seed(&pool).await;
        let api = state(pool.clone(), owner());
        let mut wrong_type = headers(
            Some("organization-levels-http-type-0001"),
            Some("https://acme.example.test"),
        );
        wrong_type.insert("content-type", HeaderValue::from_static("text/plain"));
        let response = api.install_levels(&wrong_type, "acme", None, b"{}").await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let extra = api
            .install_levels(
                &headers(
                    Some("organization-levels-http-extra-0001"),
                    Some("https://acme.example.test"),
                ),
                "acme",
                None,
                b"{\"unexpected\":true}",
            )
            .await;
        assert_eq!(extra.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let levels: i64 =
            sqlx::query_scalar("SELECT count(*) FROM staff_levels WHERE company_id='company'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(levels, 0);
    })
    .await;
}

#[tokio::test]
async fn router_configuration_rejects_ambiguous_holding_targets() {
    fixture::with_database(1, |pool| async move {
        let target = |host: &str| OrganizationSetupHttpTarget {
            host: host.into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(PgBackend::new(pool.clone(), owner())),
        };
        assert!(organization_setup_router(vec![target("acme.example.test")]).is_ok());
        assert!(
            organization_setup_router(vec![
                target("acme.example.test"),
                target("acme.example.test")
            ])
            .is_err()
        );
        assert!(
            organization_setup_router(vec![OrganizationSetupHttpTarget {
                host: "acme.example.test".into(),
                holding_slug: "Acme".into(),
                origin: "https://acme.example.test".into(),
                backend: Arc::new(PgBackend::new(pool, owner())),
            }])
            .is_err()
        );
    })
    .await;
}
