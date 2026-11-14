//! Real Auth/SSO + published IAM fixture for the retirement/compute boundary.
//! No manually adopted session, default Actor or permission-bearing JSON input.
#[path = "rust-account-revocation.rs"]
pub mod account;
use account::{ACME, NORTHSTAR, System};
use erpx_server_identity_access::public::{
    AccountAuthentication, AuthenticationSecrets, DeviceProof, PresentedSecrets, RequestAudience,
    SecretPurpose, SystemAuthenticationSecrets,
    effective_authority::{
        HoldingWorkAuthorizer, PgQueuedWorkAuthorizer, QueuedWorkBinding, WorkAuthorizationRequest,
    },
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest},
};
use erpx_server_shared::public::{AssignmentId, BusinessError, Clock, CompanyId, HoldingId};
use std::sync::Arc;

pub struct PreparedAccountQueuedAuthority {
    pub system: System,
    pub account_auth: Arc<AccountAuthentication>,
    pub central: PresentedSecrets,
    pub proof: DeviceProof,
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub queued: Arc<PgQueuedWorkAuthorizer>,
    pub selection: HoldingWorkspaceRequest,
    pub access: WorkAuthorizationRequest,
    pub binding: QueuedWorkBinding,
    pub child_id: String,
    pub second_holding_child_id: String,
}
impl PreparedAccountQueuedAuthority {
    /// The same normal Account command as the current browser, with its real
    /// saved-cookie proof. Both Holding fences precede its Control acknowledgement.
    pub async fn sign_out(&self) -> Result<(), BusinessError> {
        self.account_auth
            .sign_out(&self.central, RequestAudience::Account)
            .await
    }
}

pub async fn prepare(system: System) -> PreparedAccountQueuedAuthority {
    let central = account::login(&system, "a@example.test", None).await;
    let issued = account::child(&system, &central, ACME).await;
    let second = account::child(&system, &central, NORTHSTAR).await;
    account::context(&system.holdings[1], &second, "fixture-a").await;
    let now = system.scenario.clock.now().get();
    let holding = &system.holdings[0];
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('company','company','acme','Synthetic Acme',NULL,'company',NULL,'active'); INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','custom_queued_legal','Published queued Legal reader',false,1,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_queued_legal',1,'[\"legal.read\",\"module.legal.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',0)").execute(holding.raw()).await.unwrap();
    sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('queued-account-grant','fixture-a','custom_queued_legal','[\"legal\"]','company',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(holding.raw()).await.unwrap();
    sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-a','company','queued-account-grant',3,7,$2,$3)").bind(&issued.result.session_id).bind(now+3_600_000).bind(now).execute(holding.raw()).await.unwrap();
    let proof = DeviceProof {
        active: Some(SystemAuthenticationSecrets.digest(SecretPurpose::Session, &issued.token)),
        ..Default::default()
    };
    let selection = HoldingWorkspaceRequest {
        company_id: CompanyId::new("company").unwrap(),
        branch_slug: None,
        department_slug: None,
        expected: Some(ExpectedWorkSelection {
            revision: 7,
            assignment_id: AssignmentId::new("queued-account-grant").unwrap(),
            assignment_version: 3,
        }),
        mutation: true,
    };
    let access = WorkAuthorizationRequest {
        permission: "legal.read".into(),
        module: Some("legal".into()),
        destination: "legal/processing".into(),
        api: true,
    };
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
    // Capture the original queue witness through the actual current-authority
    // coordinator and retained Control proof, rather than constructing an Actor.
    let binding = authorizer
        .execute(
            "acme.example.test",
            proof.clone(),
            selection.clone(),
            access.clone(),
            |_, context| Box::pin(async move { Ok(QueuedWorkBinding::from_current(context)) }),
        )
        .await
        .unwrap();
    assert_eq!(binding.session_id.as_str(), issued.result.session_id);
    assert_eq!(binding.user_id.as_str(), "fixture-a");
    let queued = Arc::new(PgQueuedWorkAuthorizer::new(
        system.control.clone(),
        holding.clone(),
        HoldingId::new("h001").unwrap(),
        system.scenario.clock.clone(),
        account::fixture::account::holding_registry(),
    ));
    PreparedAccountQueuedAuthority {
        account_auth: system.scenario.auth.clone(),
        system,
        central,
        proof,
        authorizer,
        queued,
        selection,
        access,
        binding,
        child_id: issued.result.session_id,
        second_holding_child_id: second.result.session_id,
    }
}
