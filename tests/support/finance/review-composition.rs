//! Actual Account/SSO, sealed IAM and Commercial fixture for the service seam.
//! Synthetic chart/source data lives only in exact leased databases. Journal
//! provenance is produced by the real Workflow ledger and Finance store.
#[path = "../rust-account-revocation.rs"]
pub mod account;
use account::System;
use erpx_server_commercial::public::purchases::*;
use erpx_server_finance::public::{journal_review::*, *};
use erpx_server_identity_access::public::{
    AuthenticationSecrets, DeviceProof, PresentedSecrets, SecretPurpose,
    SystemAuthenticationSecrets,
    command_metadata::project_command_iam,
    effective_authority::{AuthorizedWorkContext, HoldingWorkAuthorizer, WorkAuthorizationRequest},
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId,
    IdentifierSource, RecordId,
};
use erpx_server_workflow::public::commands::*;
use erpx_service::finance_journal_review::*;
use sqlx::PgConnection;
use std::sync::Arc;
pub type Error = PgTransactionError<BusinessError>;
pub type TestClock = account::fixture::MutableClock;
pub struct Prepared {
    pub system: System,
    pub maker: DeviceProof,
    pub reviewer: DeviceProof,
    pub reviewer_token: erpx_server_identity_access::public::OpaqueSecret,
    pub reviewer_central: PresentedSecrets,
    pub reviewer_selection: HoldingWorkspaceRequest,
    pub maker_selection: HoldingWorkspaceRequest,
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub configuration: FinanceJournalConfiguration,
}
impl Prepared {
    pub fn administration(&self) -> FinanceJournalReviewAdministration<TestClock> {
        FinanceJournalReviewAdministration::new(
            self.authorizer.clone(),
            self.configuration.clone(),
            self.system.scenario.clock.clone(),
        )
    }
    pub async fn create(&self, key: &str, opening: bool) -> Result<(), Error> {
        let key = key.to_owned();
        let clock = self.system.scenario.clock.clone();
        let config = self.configuration.clone();
        self.authorizer.execute_with_sources("acme.example.test",self.maker.clone(),self.maker_selection.clone(),WorkAuthorizationRequest{permission:"finance.write".into(),module:Some("finance".into()),destination:"finance/journals".into(),api:true},config,move|c,current|{
   let key=key.clone();let clock=clock.clone();Box::pin(async move{
    let actor=finance_actor(current);let draft=draft(opening);let input=CommandJson::from_serializable(&serde_json::json!({"chartId":"chart","periodId":"period","entryDate":"2026-01-01","currency":"THB","isOpening":opening,"sourceType":draft.source_type,"sourceId":draft.source_id,"lines":draft.lines}))?;
    let request=WorkflowRequest{key:key.clone(),operation:"finance.journal.create".into(),record_id:format!("journal-{key}"),input};
    let mut authority=CreationAuthority{current,clock:clock.as_ref()};let mut effects=CreationEffects{current,actor,draft,clock:clock.as_ref(),plan:None};let mut ids=Ids;
    let outcome=PgCommandLedger::new(current.selection().workspace().company_id.clone(),c,&mut authority,clock.as_ref(),&mut ids).execute(&request,&mut effects).await?;
    assert!(!outcome.replayed);Ok(())
   })
  }).await
    }
    pub async fn review(
        &self,
        id: &str,
        key: &str,
        input: JournalReviewInput,
    ) -> Result<JournalReviewResult, Error> {
        self.administration()
            .review(
                "acme.example.test",
                self.reviewer.clone(),
                self.reviewer_selection.clone(),
                command(id, key, input),
            )
            .await
    }
}
pub fn command(id: &str, key: &str, input: JournalReviewInput) -> JournalReviewCommand {
    JournalReviewCommand {
        key: CommandKey::new(key).unwrap(),
        journal_id: RecordId::new(id).unwrap(),
        input,
    }
}
pub fn post() -> JournalReviewInput {
    JournalReviewInput::Post {
        expected_version: 1,
    }
}
pub fn reverse() -> JournalReviewInput {
    JournalReviewInput::Reverse {
        period_id: "period".into(),
        entry_date: "2026-01-10".into(),
        memo: None,
    }
}
pub fn business<T>(result: Result<T, Error>, code: &str) {
    match result {
        Err(PgTransactionError::Business(e)) => assert_eq!(e.code, code),
        _ => panic!("expected current business denial {code}"),
    }
}
pub async fn setup(system: System) -> Prepared {
    setup_with_finance(system, true).await
}
pub async fn setup_with_finance(system: System, finance: bool) -> Prepared {
    let maker_central = account::login(&system, "a@example.test", None).await;
    let maker = account::child(&system, &maker_central, account::ACME).await;
    let reviewer_central = account::login(&system, "b@example.test", None).await;
    let reviewer = account::child(&system, &reviewer_central, account::ACME).await;
    let pool = &system.holdings[0];
    let now = system.scenario.clock.now().get();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('company','company','acme','Synthetic Acme',NULL,'company',NULL,'active');INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','custom_finance_review','Current finance reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_finance_review',1,'[\"finance.read\",\"finance.write\",\"finance.approve\",\"module.finance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',0)").execute(pool.raw()).await.unwrap();
    for (user, session) in [
        ("fixture-a", &maker.result.session_id),
        ("fixture-b", &reviewer.result.session_id),
    ] {
        let assignment = format!("finance-{user}");
        sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES($1,$2,'custom_finance_review','[\"finance\"]','company',$3,$4,3,'holding')").bind(&assignment).bind(user).bind(now-1000).bind(now+3_600_000).execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,$2,'company',$3,3,7,$4,$5)").bind(session).bind(user).bind(assignment).bind(now+3_600_000).bind(now).execute(pool.raw()).await.unwrap();
    }
    sqlx::raw_sql("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES('chart','company','Primary','THB','fixture-a',0,0),('book-chart','company','Secondary','THB','fixture-a',0,0);INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at)VALUES('period','company','January','2026-01-01','2026-01-31',0);INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,created_at,updated_at)VALUES('cash','company','chart','100','Cash','asset',0,0),('sales','company','chart','400','Sales','revenue',0,0),('book-cash','company','book-chart','100','Cash','asset',0,0),('book-sales','company','book-chart','400','Sales','revenue',0,0);INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at)VALUES('rate','company','THB','2026-01-01',2,'fixture-a',1)").execute(pool.raw()).await.unwrap();
    if finance {
        grant(pool, now).await;
    }
    let authorizer = Arc::new(
        HoldingWorkAuthorizer::new(
            system.control.clone(),
            pool.clone(),
            HoldingId::new("h001").unwrap(),
            account::fixture::account::policy(),
            system.scenario.clock.clone(),
            account::fixture::account::holding_registry(),
        )
        .unwrap(),
    );
    let configuration = FinanceJournalConfiguration::new(
        FinanceJournalEnvironment::Development,
        HoldingId::new("h001").unwrap(),
        CompanyId::new("company").unwrap(),
        Arc::new(current_package_catalogue().unwrap()),
    )
    .unwrap();
    Prepared {
        maker: proof(&maker.token),
        reviewer: proof(&reviewer.token),
        reviewer_token: reviewer.token,
        maker_selection: selection("fixture-a"),
        reviewer_selection: selection("fixture-b"),
        reviewer_central,
        system,
        authorizer,
        configuration,
    }
}
pub async fn grant(pool: &DatabasePool, now: i64) {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at)VALUES('company','finance',1,'active',0,$2,1,'Synthetic reviewed finance purchase','public synthetic evidence',$1)").bind(now-1000).bind(now+3_600_000).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'finance-origin-grant','company','finance','fixture-a','synthetic-request',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company'AND package_id='finance'").bind(now).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}
fn proof(raw: &erpx_server_identity_access::public::OpaqueSecret) -> DeviceProof {
    DeviceProof {
        active: Some(SystemAuthenticationSecrets.digest(SecretPurpose::Session, raw)),
        ..Default::default()
    }
}
fn selection(user: &str) -> HoldingWorkspaceRequest {
    HoldingWorkspaceRequest {
        company_id: CompanyId::new("company").unwrap(),
        branch_slug: None,
        department_slug: None,
        expected: Some(ExpectedWorkSelection {
            revision: 7,
            assignment_id: AssignmentId::new(format!("finance-{user}")).unwrap(),
            assignment_version: 3,
        }),
        mutation: true,
    }
}
fn draft(opening: bool) -> JournalDraft {
    JournalDraft{chart_id:"chart".into(),period_id:"period".into(),entry_date:"2026-01-01".into(),currency:"THB".into(),fx_rate:None,is_opening:opening,memo:"Actual native journal origin".into(),source_type:if opening{"opening_import"}else{"manual"}.into(),source_id:opening.then(||"chart".into()),lines:serde_json::from_value(serde_json::json!([{"accountId":"cash","debit":"100.00","credit":"0"},{"accountId":"sales","debit":"0","credit":"100.00"}])).unwrap()}
}
fn finance_actor(current: &AuthorizedWorkContext) -> FinanceActor {
    FinanceActor {
        holding_id: current.holding_id().clone(),
        company_id: current.selection().workspace().company_id.clone(),
        user_id: current.user_id().clone(),
        assignment_id: current.selection().assignment().id.clone(),
    }
}
fn current_actor(current: &AuthorizedWorkContext) -> Result<CommandActor, BusinessError> {
    let a = current.selection().assignment();
    let w = current.selection().workspace();
    Ok(CommandActor {
        user_id: current.user_id().clone(),
        assignment_id: a.id.clone(),
        role: a.role.clone(),
        company_id: w.company_id.clone(),
        branch_id: w
            .branch_id
            .as_ref()
            .map(|x| CommandField::Value(x.as_str().into()))
            .unwrap_or(CommandField::Null),
        department_id: w
            .department_id
            .as_ref()
            .map(|x| CommandField::Value(x.as_str().into()))
            .unwrap_or(CommandField::Null),
        authority: CommandField::Value(WorkAuthority {
            session_id: current.session_id().as_str().into(),
            revision: current.selection().revision(),
            assignment_version: a.version,
        }),
        iam: CommandField::Value(CommandJson::from_serializable(&project_command_iam(
            current,
        )?)?),
        assignment_scope_kind: match a.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        assignment_scope_node_id: a.scope_node_id.as_ref().map(|s| s.as_str().into()),
    })
}
struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).unwrap()
    }
}
fn convert(e: PgTransactionError<JournalError>) -> Error {
    match e {
        PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
        PgTransactionError::Business(e) => {
            BusinessError::new(e.kind, e.code, "Actual native Finance preparation failed.").into()
        }
    }
}
struct CreationAuthority<'a> {
    current: &'a AuthorizedWorkContext,
    clock: &'a TestClock,
}
impl PgCommandAuthority for CreationAuthority<'_> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        lock_commercial_authority_sources(c).await?;
        lock_journal_review_sources(c).await
    }
    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        self.current.require_permission("finance.write")?;
        if self.clock.now().get() >= self.current.expires_at() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "EXPIRED",
                "Actual source authority expired.",
            )
            .into());
        }
        Ok(current_actor(self.current)?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        PgCompanyPurchaseReader::new(
            self.current.holding_id().clone(),
            self.current.selection().workspace().company_id.clone(),
            c,
            &current_package_catalogue()?,
        )
        .inspect(
            &PurchaseRequest {
                capabilities: vec!["finance.accounting".into()],
                purpose: if disposition == CommandDisposition::Replay {
                    CapabilityPurpose::History
                } else {
                    CapabilityPurpose::NewWork
                },
                mode: PackageEnforcementMode::Enforced,
            },
            self.clock.now(),
        )
        .await?
        .require_allowed()?;
        Ok(())
    }
}
struct PreparationAuthority<'a>(&'a AuthorizedWorkContext);
impl JournalAuthority for PreparationAuthority<'_> {
    type Error = PgTransactionError<JournalError>;
    async fn authorize(
        &mut self,
        actor: &FinanceActor,
        _: &JournalDraft,
    ) -> Result<(), Self::Error> {
        if actor != &finance_actor(self.0) || !self.0.has_permission("finance.write") {
            return Err(JournalError {
                kind: FailureKind::Forbidden,
                code: "PREPARATION_CURRENT_AUTHORITY_REQUIRED",
                message: "Actual current Finance permission required.".into(),
            }
            .into());
        }
        Ok(())
    }
}
struct CreationEffects<'a> {
    current: &'a AuthorizedWorkContext,
    actor: FinanceActor,
    draft: JournalDraft,
    clock: &'a TestClock,
    plan: Option<JournalPreparation>,
}
impl PgOwnerCommandEffects for CreationEffects<'_> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        lock_journal_review_sources(c).await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        r: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let rules = embedded_currency_rules().unwrap();
        let collation = FinanceCollation::english().unwrap();
        let mut authority = PreparationAuthority(self.current);
        let mut ports = PgJournalPreparation::new(
            c,
            &self.actor,
            &mut authority,
            self.clock,
            &rules,
            &collation,
        )
        .map_err(convert)?;
        let plan = prepare_journal(
            &mut ports,
            &self.actor,
            &self.draft,
            &CommandKey::new(&r.key)?,
            JournalStatus::Draft,
        )
        .await
        .map_err(convert)?;
        let output = WorkflowOutput {
            version: 1,
            result: CommandJson::from_serializable(plan.result())?,
            event_scope: None,
        };
        self.plan = Some(plan);
        Ok(output)
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        PgJournalStore::new(
            self.actor.holding_id.clone(),
            self.actor.company_id.clone(),
            c,
        )
        .persist(self.plan.as_ref().unwrap())
        .await
        .map_err(convert)
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        r: &WorkflowRequest,
    ) -> Result<(), Error> {
        let row: (String, String, i64) = sqlx::query_as(
            "SELECT status,created_by,version::bigint FROM finance_journals WHERE id=$1",
        )
        .bind(&r.record_id)
        .fetch_one(c)
        .await?;
        assert_eq!(
            row,
            ("draft".into(), self.current.user_id().as_str().into(), 1)
        );
        Ok(())
    }
}
