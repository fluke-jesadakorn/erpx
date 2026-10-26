#[path = "../../support/legal/trust-facts.rs"]
#[allow(dead_code)]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_legal::public::{
    LegalTrustActor, LegalTrustChangePlan, LegalTrustCommand, LegalTrustCommandRequest,
    LegalTrustReceipt, LegalTrustRecord, PgLegalTrustCommitAuthority, PgLegalTrustStore,
    PgLegalTrustWorkflow, PreparedLegalTrustCommand, RustTrustMaterialInspector, TrustMaterialKind,
    TrustMaterialState, execute_legal_trust_command, prepare_legal_trust_command,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use sqlx::PgConnection;

struct FixtureAuthority {
    manage: bool,
    independent: bool,
}
impl PgLegalTrustCommitAuthority for FixtureAuthority {
    async fn revalidate(
        &mut self,
        _: &mut PgConnection,
        _: &LegalTrustActor,
        witness: &str,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        if self.manage && witness == "current-authority-1" {
            Ok(())
        } else {
            Err(PgTransactionError::Business(BusinessError::new(
                FailureKind::Forbidden,
                "LEGAL_AUTHORITY_CHANGED",
                "Authority changed.",
            )))
        }
    }
    async fn independent_person_review(
        &mut self,
        _: &mut PgConnection,
        _: &LegalTrustActor,
        _: &UserId,
    ) -> Result<bool, PgTransactionError<BusinessError>> {
        Ok(self.independent)
    }
}

/// Disposable owner bridges for proving transaction composition, not the
/// production Identity or Workflow implementations.
struct ProbeWorkflow {
    authority: FixtureAuthority,
}
impl PgLegalTrustCommitAuthority for ProbeWorkflow {
    async fn revalidate(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
        witness: &str,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        self.authority.revalidate(connection, actor, witness).await
    }
    async fn independent_person_review(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
        created_by: &UserId,
    ) -> Result<bool, PgTransactionError<BusinessError>> {
        self.authority
            .independent_person_review(connection, actor, created_by)
            .await
    }
}
impl PgLegalTrustWorkflow for ProbeWorkflow {
    async fn authorize_manage(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
    ) -> Result<String, PgTransactionError<BusinessError>> {
        self.revalidate(connection, actor, "current-authority-1")
            .await?;
        Ok("current-authority-1".to_owned())
    }
    async fn same_person(
        &mut self,
        _: &mut PgConnection,
        actor: &LegalTrustActor,
        other: &UserId,
    ) -> Result<bool, PgTransactionError<BusinessError>> {
        Ok(actor.user_id == *other || !self.authority.independent)
    }
    async fn replay(
        &mut self,
        _: &mut PgConnection,
        _: &LegalTrustActor,
        _: &CommandKey,
        _: &LegalTrustCommand,
    ) -> Result<Option<LegalTrustReceipt>, PgTransactionError<BusinessError>> {
        Ok(None)
    }
    async fn record_completed(
        &mut self,
        connection: &mut PgConnection,
        _: &LegalTrustActor,
        key: &CommandKey,
        _: &LegalTrustCommand,
        _: &LegalTrustReceipt,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        sqlx::query("INSERT INTO receipt_probe VALUES($1)")
            .bind(key.as_str())
            .execute(&mut *connection)
            .await?;
        sqlx::query("INSERT INTO audit_probe VALUES('audit')")
            .execute(connection)
            .await?;
        Ok(())
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}
fn addition() -> LegalTrustCommand {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/legal/pdf-signatures/conformance.v1.json"
    ))
    .unwrap();
    let item = corpus["cases"][0]["trust"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["path"] == "roots/root.pem")
        .unwrap();
    LegalTrustCommand::Add {
        kind: TrustMaterialKind::Root,
        name: "Reviewed synthetic root".to_owned(),
        source: "https://unreachable.invalid/operator-source".to_owned(),
        content: String::from_utf8(hex::decode(item["hex"].as_str().unwrap()).unwrap()).unwrap(),
    }
}

async fn add_plan(command_key: &str) -> Box<LegalTrustChangePlan> {
    let result = prepare_legal_trust_command(
        &mut facts::TrustFacts::authorized_company(),
        &RustTrustMaterialInspector,
        &facts::FixedClock,
        &facts::actor(),
        &key(command_key),
        &addition(),
    )
    .await
    .unwrap();
    let PreparedLegalTrustCommand::Change(plan) = result else {
        panic!("expected prepared trust addition");
    };
    plan
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}

async fn persist(
    connection: &mut PgConnection,
    plan: &LegalTrustChangePlan,
    authority: &mut FixtureAuthority,
) -> Result<LegalTrustReceipt, PgTransactionError<BusinessError>> {
    PgLegalTrustStore::new(
        HoldingId::new("holding-a").unwrap(),
        CompanyId::new("company-a").unwrap(),
        connection,
    )
    .persist(plan, authority)
    .await
}

#[tokio::test]
async fn trust_change_commits_and_rolls_back_with_the_existing_owner_transaction() {
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        sqlx::raw_sql("CREATE TABLE source_probe(id text PRIMARY KEY); CREATE TABLE receipt_probe(id text PRIMARY KEY)").execute(pool.raw()).await.unwrap();
        let uow=PgUnitOfWork::new(pool.clone());
        uow.run::<_,BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move {
            let plan=add_plan("legal-trust-first").await;
            sqlx::query("INSERT INTO source_probe VALUES('source')").execute(&mut *connection).await?;
            persist(connection,&plan,&mut FixtureAuthority{manage:true,independent:true}).await?;
            sqlx::query("INSERT INTO receipt_probe VALUES('receipt')").execute(&mut *connection).await?;
            Ok(())
        })).await.unwrap();
        let rollback=uow.run::<(),BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move {
            let plan=add_plan("legal-trust-rollback").await;
            persist(connection,&plan,&mut FixtureAuthority{manage:true,independent:true}).await?;
            sqlx::query("INSERT INTO source_probe VALUES('rollback')").execute(&mut *connection).await?;
            sqlx::query("INSERT INTO receipt_probe VALUES('receipt')").execute(&mut *connection).await?;
            Ok(())
        })).await;
        assert!(matches!(rollback,Err(PgTransactionError::Persistence(ref error)) if error.sql_state()==Some("23505")));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM legal_trust_material").fetch_one(pool.raw()).await.unwrap(),1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM source_probe").fetch_one(pool.raw()).await.unwrap(),1);
        let stored:(String,String,String)=sqlx::query_as("SELECT state,checksum,content FROM legal_trust_material WHERE id='legal-trust-first'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(stored.0,"proposed");
        use base64::{Engine,engine::general_purpose::STANDARD};use sha2::Digest;
        let decoded=STANDARD.decode(stored.2.replace('\n',"")).unwrap();
        assert_eq!(stored.1,hex::encode(sha2::Sha256::digest(&decoded)));
    }).await;
}

#[tokio::test]
async fn trust_store_requires_owner_scope_current_authority_and_an_independent_person() {
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        let plan=add_plan("legal-trust-review").await;
        let mut connection=pool.raw().acquire().await.unwrap();
        assert!(persist(&mut connection,&plan,&mut FixtureAuthority{manage:true,independent:true}).await.is_err());
        drop(connection);
        let uow=PgUnitOfWork::new(pool.clone());
        uow.run::<(),BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move {
            let plan=add_plan("legal-trust-review").await;
            persist(connection,&plan,&mut FixtureAuthority{manage:true,independent:true}).await?;Ok(())
        })).await.unwrap();
        for guard in ["authority","person","stale","scope"] {
            let result=uow.run::<(),BusinessError,_>(TransactionOptions::default(),move |connection|Box::pin(async move {
                let checksum=sqlx::query_scalar::<_,String>("SELECT checksum FROM legal_trust_material WHERE id='legal-trust-review'").fetch_one(&mut *connection).await?;
                let mut ports=facts::TrustFacts::authorized_company();
                ports.source=Some(LegalTrustRecord{id:RecordId::new("legal-trust-review").unwrap(),company_id:CompanyId::new("company-a").unwrap(),version:1,
                    kind:TrustMaterialKind::Root,state:TrustMaterialState::Proposed,created_by:facts::actor().user_id,checksum_sha256:checksum});
                let mut reviewer=facts::actor();reviewer.user_id=UserId::new("second-manager").unwrap();
                let command=LegalTrustCommand::Approve{id:RecordId::new("legal-trust-review").unwrap(),version:1};
                let result=prepare_legal_trust_command(&mut ports,&RustTrustMaterialInspector,&facts::FixedClock,&reviewer,&key("legal-trust-approve"),&command)
                    .await.map_err(PgTransactionError::Business)?;
                let PreparedLegalTrustCommand::Change(plan)=result else {panic!("expected approval plan");};
                if guard=="stale" {sqlx::query("UPDATE legal_trust_material SET version=2 WHERE id='legal-trust-review'").execute(&mut *connection).await?;}
                let company=if guard=="scope" {"other-company"} else {"company-a"};
                PgLegalTrustStore::new(HoldingId::new("holding-a").unwrap(),CompanyId::new(company).unwrap(),connection)
                    .persist(&plan,&mut FixtureAuthority{manage:guard!="authority",independent:guard!="person"}).await?;
                Ok(())
            })).await;
            assert!(matches!(result,Err(PgTransactionError::Business(_))),"{guard}");
        }
        let row:(String,i64,Option<String>)=sqlx::query_as("SELECT state,version::bigint,approved_by FROM legal_trust_material WHERE id='legal-trust-review'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(row,("proposed".to_owned(),1,None));
        for (version,state) in [(1,TrustMaterialState::Approved),(2,TrustMaterialState::Retired)] {
            uow.run::<(),BusinessError,_>(TransactionOptions::default(),move |connection|Box::pin(async move {
                let checksum=sqlx::query_scalar::<_,String>("SELECT checksum FROM legal_trust_material WHERE id='legal-trust-review'").fetch_one(&mut *connection).await?;
                let mut ports=facts::TrustFacts::authorized_company();
                ports.source=Some(LegalTrustRecord{id:RecordId::new("legal-trust-review").unwrap(),company_id:CompanyId::new("company-a").unwrap(),version,
                    kind:TrustMaterialKind::Root,state:if version==1 {TrustMaterialState::Proposed}else{TrustMaterialState::Approved},created_by:facts::actor().user_id,checksum_sha256:checksum});
                let mut reviewer=facts::actor();reviewer.user_id=UserId::new("second-manager").unwrap();
                let command=if state==TrustMaterialState::Approved {LegalTrustCommand::Approve{id:RecordId::new("legal-trust-review").unwrap(),version}}
                    else {LegalTrustCommand::Retire{id:RecordId::new("legal-trust-review").unwrap(),version}};
                let prepared=prepare_legal_trust_command(&mut ports,&RustTrustMaterialInspector,&facts::FixedClock,&reviewer,&key("legal-trust-decision"),&command)
                    .await.map_err(PgTransactionError::Business)?;
                let PreparedLegalTrustCommand::Change(plan)=prepared else {panic!("expected decision plan");};
                persist(connection,&plan,&mut FixtureAuthority{manage:true,independent:state==TrustMaterialState::Approved}).await?;Ok(())
            })).await.unwrap();
        }
        let row:(String,i64,String)=sqlx::query_as("SELECT state,version::bigint,approved_by FROM legal_trust_material WHERE id='legal-trust-review'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(row,("retired".to_owned(),3,"second-manager".to_owned()));
    }).await;
}

#[tokio::test]
async fn trust_limit_is_rechecked_against_actual_locked_company_rows() {
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        sqlx::query("INSERT INTO legal_trust_material(id,company_id,kind,name,source,content,checksum,state,created_by,created_at) SELECT 'existing-'||n,'company-a','root','Synthetic root','https://unreachable.invalid','MAA=',repeat('a',64),'proposed','creator',0 FROM generate_series(1,100) n")
            .execute(pool.raw()).await.unwrap();
        let uow=PgUnitOfWork::new(pool.clone());
        let result=uow.run::<(),BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move {
            let plan=add_plan("legal-trust-over-limit").await;
            persist(connection,&plan,&mut FixtureAuthority{manage:true,independent:true}).await?;Ok(())
        })).await;
        assert!(matches!(result,Err(PgTransactionError::Business(BusinessError{code:"LEGAL_TRUST_LIMIT",..}))));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM legal_trust_material").fetch_one(pool.raw()).await.unwrap(),100);
    }).await;
}

#[tokio::test]
async fn concrete_command_repository_and_cross_owner_receipt_share_one_commit() {
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        sqlx::raw_sql("CREATE TABLE receipt_probe(id text PRIMARY KEY);CREATE TABLE audit_probe(id text PRIMARY KEY)")
            .execute(pool.raw()).await.unwrap();
        let uow=PgUnitOfWork::new(pool.clone());
        for (id,success) in [("legal-workflow-first",true),("legal-workflow-late-failure",false)] {
            let result=uow.run::<(),BusinessError,_>(TransactionOptions::default(),move |connection|Box::pin(async move {
                let mut workflow=ProbeWorkflow{authority:FixtureAuthority{manage:true,independent:true}};
                execute_legal_trust_command(connection,HoldingId::new("holding-a").unwrap(),CompanyId::new("company-a").unwrap(),
                    &mut workflow,&RustTrustMaterialInspector,&facts::FixedClock,
                    LegalTrustCommandRequest{actor:&facts::actor(),key:&key(id),command:&addition()}).await?;
                Ok(())
            })).await;
            assert_eq!(result.is_ok(),success);
        }
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM legal_trust_material").fetch_one(pool.raw()).await.unwrap(),1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM receipt_probe").fetch_one(pool.raw()).await.unwrap(),1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM audit_probe").fetch_one(pool.raw()).await.unwrap(),1);
    }).await;
}
