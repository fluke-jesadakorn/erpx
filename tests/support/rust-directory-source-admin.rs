//! Actual two-account sign-in, SSO-child MFA and published Company IAM policy.
//! No synthetic source approval policy is used by this administration fixture.
#[allow(dead_code, reason = "shared owned LDAPS/private-profile fixture")]
#[path = "rust-directory-ldaps.rs"]
pub mod ldap;
#[allow(dead_code, reason = "shared actual Account and MFA fixture")]
#[path = "rust-account-security.rs"]
pub mod security;
use erpx_server_identity_access::public::{
    account_security::*, directory::*, effective_authority::HoldingWorkAuthorizer, work_context::*,
    *,
};
use erpx_server_shared::public::{AssignmentId, Clock, CommandKey, CompanyId, HoldingId, RecordId};
use erpx_service::directory_source_administration::*;
pub use security::fixture as account;
use std::{collections::BTreeMap, sync::Arc};
pub const HOST: &str = "acme.example.test";
pub struct Prepared {
    pub system: account::System,
    pub central_a: PresentedSecrets,
    pub central_b: PresentedSecrets,
    pub child_a: PresentedSecrets,
    pub child_b: PresentedSecrets,
    pub id_a: String,
    pub id_b: String,
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub keys: Arc<AccountMfaKeyring>,
    pub directory: Arc<PostgresDirectory>,
    pub profile: ldap::Fixture,
    pub server: ldap::Server,
}
impl Prepared {
    pub fn service(&self) -> DirectorySourceAdministration<account::fixture::MutableClock> {
        DirectorySourceAdministration::new(
            self.directory.clone(),
            self.authorizer.clone(),
            self.keys.clone(),
            self.system.scenario.clock.clone(),
        )
    }
    pub fn proof(&self, reviewer: bool) -> DeviceProof {
        if reviewer {
            &self.child_b
        } else {
            &self.child_a
        }
        .proof(&SystemAuthenticationSecrets)
    }
    pub fn workspace(&self, reviewer: bool) -> HoldingWorkspaceRequest {
        HoldingWorkspaceRequest {
            company_id: CompanyId::new("company").unwrap(),
            branch_slug: None,
            department_slug: None,
            expected: Some(ExpectedWorkSelection {
                revision: 7,
                assignment_id: AssignmentId::new(if reviewer {
                    "source-admin-b"
                } else {
                    "source-admin-a"
                })
                .unwrap(),
                assignment_version: 3,
            }),
            mutation: true,
        }
    }
    pub fn preview(
        &self,
        key: &str,
        revision: Option<i64>,
        key_id: &str,
    ) -> DirectorySourcePreview {
        DirectorySourcePreview {
            request: DirectorySourceChangeRequest::new(
                RecordId::new(ldap::CONNECTION).unwrap(),
                key_id.into(),
                1,
                revision
                    .map(provenance::SourceRevision::checked)
                    .transpose()
                    .unwrap(),
            )
            .unwrap(),
            key: CommandKey::new(key).unwrap(),
            reason: "Review this exact selected Directory source and shared key.".into(),
        }
    }
    pub fn decision(
        &self,
        result: &DirectorySourceReviewResult,
        key: &str,
    ) -> DirectorySourceDecision {
        DirectorySourceDecision {
            connection_id: RecordId::new(ldap::CONNECTION).unwrap(),
            review_id: RecordId::new(&result.id).unwrap(),
            expected_version: result.version,
            key: CommandKey::new(key).unwrap(),
        }
    }
}
pub fn rebuilt(p: &Prepared, profiles: Arc<DirectoryProfileStore>) -> Arc<PostgresDirectory> {
    Arc::new(
        PostgresDirectory::new(
            DirectoryDatabaseBindings {
                control: p.system.control.clone(),
                holdings: BTreeMap::from([
                    (
                        HoldingId::new("h001").unwrap(),
                        DirectoryHoldingDatabase {
                            audience: account::ACME.into(),
                            write_binding: "HOLDING_H001_DB".into(),
                            pool: p.system.holdings[0].clone(),
                        },
                    ),
                    (
                        HoldingId::new("h002").unwrap(),
                        DirectoryHoldingDatabase {
                            audience: account::NORTHSTAR.into(),
                            write_binding: "HOLDING_H002_DB".into(),
                            pool: p.system.holdings[1].clone(),
                        },
                    ),
                ]),
                registry: account::fixture::account::holding_registry(),
                environment: "development".into(),
            },
            profiles,
            p.system.scenario.clock.clone(),
            Arc::new(SystemAuthenticationSecrets),
            account::invalidator(&p.system.holdings, p.system.scenario.clock.clone()),
        )
        .unwrap(),
    )
}
pub async fn prepare(system: account::System, verify_reviewer: bool) -> Prepared {
    let central_a = account::login(&system, "a@example.test", None).await;
    let central_b = account::login(&system, "b@example.test", None).await;
    let a = account::child(&system, &central_a, account::ACME).await;
    let b = account::child(&system, &central_b, account::ACME).await;
    let id_a = a.result.session_id;
    let id_b = b.result.session_id;
    let child_a = PresentedSecrets {
        browser: None,
        active: Some(a.token),
        saved: vec![],
    };
    let child_b = PresentedSecrets {
        browser: None,
        active: Some(b.token),
        saved: vec![],
    };
    let services = security::services(&system);
    let keys = services.keys.clone();
    for (user, proof, verify) in [
        ("fixture-a", &child_a, true),
        ("fixture-b", &child_b, verify_reviewer),
    ] {
        services
            .service
            .enroll(
                proof,
                RequestAudience::Holding {
                    audience: account::ACME.into(),
                },
            )
            .await
            .unwrap();
        if verify {
            let code = security::code(&system, &keys, user, 0).await;
            services
                .service
                .verify(
                    proof,
                    RequestAudience::Holding {
                        audience: account::ACME.into(),
                    },
                    code,
                )
                .await
                .unwrap();
        }
    }
    let now = system.scenario.clock.now().get();
    let holding = &system.holdings[0];
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('company','company','acme','Actual source administration fixture',NULL,'company',NULL,'active');INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','custom_source_admin','Published Directory administration',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_source_admin',1,'[\"iam.directory.manage\",\"iam.review\"]','[]','[\"holding\"]','synthetic-fixture',0)").execute(holding.raw()).await.unwrap();
    for (user, session, assignment) in [
        ("fixture-a", &id_a, "source-admin-a"),
        ("fixture-b", &id_b, "source-admin-b"),
    ] {
        sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES($1,$2,'custom_source_admin','[]','company',$3,$4,3,'holding')").bind(assignment).bind(user).bind(now-1000).bind(now+3_600_000).execute(holding.raw()).await.unwrap();
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,$2,'company',$3,3,7,$4,$5)").bind(session).bind(user).bind(assignment).bind(now+3_600_000).bind(now).execute(holding.raw()).await.unwrap();
    }
    let server = ldap::Server::start(ldap::Scenario::Complete, false, None).await;
    let profile = ldap::Fixture::new(
        server.port,
        false,
        Some(&ldap::material().ca),
        "ldap.fixture.test",
    )
    .await;
    sqlx::query("INSERT INTO iam_connection_routes(connection_id,holding_id,company_id)VALUES($1,'h001','company')").bind(ldap::CONNECTION).execute(system.control.raw()).await.unwrap();
    sqlx::query("INSERT INTO iam_connections(id,company_id,name,provider,config,status,version,tested_version,created_by,created_at)VALUES($1,'company','Actual admin source','univention',$2,'active',1,1,'fixture-a',$3)").bind(ldap::CONNECTION).bind(serde_json::to_value(&profile.descriptor.config).unwrap()).bind(now).execute(holding.raw()).await.unwrap();
    let authorizer = Arc::new(
        HoldingWorkAuthorizer::new(
            system.control.clone(),
            holding.clone(),
            HoldingId::new("h001").unwrap(),
            account::fixture::account::policy(),
            system.scenario.clock.clone(),
            account::fixture::account::holding_registry(),
        )
        .unwrap(),
    );
    let directory = Arc::new(
        PostgresDirectory::new(
            DirectoryDatabaseBindings {
                control: system.control.clone(),
                holdings: BTreeMap::from([
                    (
                        HoldingId::new("h001").unwrap(),
                        DirectoryHoldingDatabase {
                            audience: account::ACME.into(),
                            write_binding: "HOLDING_H001_DB".into(),
                            pool: system.holdings[0].clone(),
                        },
                    ),
                    (
                        HoldingId::new("h002").unwrap(),
                        DirectoryHoldingDatabase {
                            audience: account::NORTHSTAR.into(),
                            write_binding: "HOLDING_H002_DB".into(),
                            pool: system.holdings[1].clone(),
                        },
                    ),
                ]),
                registry: account::fixture::account::holding_registry(),
                environment: "development".into(),
            },
            profile.profiles.clone(),
            system.scenario.clock.clone(),
            Arc::new(SystemAuthenticationSecrets),
            account::invalidator(&system.holdings, system.scenario.clock.clone()),
        )
        .unwrap(),
    );
    Prepared {
        system,
        central_a,
        central_b,
        child_a,
        child_b,
        id_a,
        id_b,
        authorizer,
        keys,
        directory,
        profile,
        server,
    }
}
pub async fn counts(p: &Prepared) -> (i64, i64, i64, i64) {
    sqlx::query_as("SELECT(SELECT count(*)FROM iam_directory_source_selections),(SELECT count(*)FROM iam_reviews),(SELECT count(*)FROM iam_receipts),(SELECT count(*)FROM iam_audit)").fetch_one(p.system.holdings[0].raw()).await.unwrap()
}
