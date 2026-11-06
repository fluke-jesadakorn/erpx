//! Owner effect test with explicit synthetic source; this does not grant IAM.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
#[path = "../../support/quality/quality-facts.rs"]
#[allow(dead_code)]
mod quality_facts;
use erpx_server_collaboration::public::service_count_flow_notification::*;
use erpx_server_runtime::public::postgres::{
    MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions, load_migrations,
    migrate_database,
};
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;
use erpx_server_collaboration::public::crm_activity_notification::*;
type Error = PgTransactionError<BusinessError>;
use erpx_server_collaboration::public::quality_notification::*;
struct QualitySource(QualityNotificationFacts);

struct JoinedQualitySource {
    facts:QualityNotificationFacts,
    hold:erpx_server_quality::public::quality_http::CurrentQualityDetail,
    effect:erpx_server_quality::public::quality_http::RetainedQualityCancellation,
    recipients:erpx_server_identity_access::public::quality_recipients::CurrentQualityRecipients,
}

#[tokio::test]
async fn quality_cancellation_writer_intent_serializes_competing_commands() {
    use erpx_server_quality::public::quality_http::{lock_quality_cancellation_writes,lock_quality_read_sources,prepare_quality_cancellation};
    use erpx_server_quality::public::quality_read::{CancellationAction,QualityCancellationCommand};
    use erpx_server_shared::public::{CommandKey,CompanyId,RecordId,UserId};
    use sqlx::Connection;
    fixture::with_database(3,|pool|async move {
        quality_facts::setup(&pool).await;quality_facts::hold(&pool,"race-hold").await;
        let mut first=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *first).await.unwrap();
        lock_quality_cancellation_writes(&mut first).await.unwrap();lock_quality_read_sources(&mut first).await.unwrap();
        let command=QualityCancellationCommand { command_key:CommandKey::new("quality-race-first").unwrap(),hold_id:RecordId::new("race-hold").unwrap(),expected_version:1,action:CancellationAction::Request,reason:"Review duplicate hold".into() };
        let effect=prepare_quality_cancellation(&mut first,&command,&CompanyId::new("company").unwrap(),Some(&RecordId::new("branch").unwrap()),&UserId::new("maker").unwrap(),1_800_000_000_000).await.unwrap();
        effect.apply(&mut first).await.unwrap();
        let first_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *first).await.unwrap();
        let other_pool=pool.clone();let (send,receive)=tokio::sync::oneshot::channel();
        let second=tokio::spawn(async move {
            let mut connection=other_pool.raw().acquire().await.unwrap();
            let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *connection).await.unwrap();
            let mut tx=connection.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();send.send(pid).unwrap();
            lock_quality_cancellation_writes(&mut tx).await.unwrap();lock_quality_read_sources(&mut tx).await.unwrap();
            let command=QualityCancellationCommand { command_key:CommandKey::new("quality-race-second").unwrap(),hold_id:RecordId::new("race-hold").unwrap(),expected_version:1,action:CancellationAction::Request,reason:"Review duplicate hold".into() };
            let outcome=prepare_quality_cancellation(&mut tx,&command,&CompanyId::new("company").unwrap(),Some(&RecordId::new("branch").unwrap()),&UserId::new("maker").unwrap(),1_800_000_000_000).await;
            assert!(outcome.is_err());tx.rollback().await.unwrap();
        });
        let second_pid=receive.await.unwrap();let mut blocked=false;
        for _ in 0..200 { blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(first_pid).bind(second_pid).fetch_one(pool.raw()).await.unwrap();if blocked {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await; }
        assert!(blocked,"competing Quality writer must wait before its first snapshot");first.commit().await.unwrap();second.await.unwrap();
        let state:(i64,i64)=sqlx::query_as("SELECT h.version,(SELECT count(*) FROM inventory_quality_cancellations e WHERE e.hold_id=h.id) FROM inventory_quality_holds h WHERE h.id='race-hold'").fetch_one(pool.raw()).await.unwrap();assert_eq!(state,(2,1));
    }).await;
}
impl PgQualityNotificationSource for JoinedQualitySource {
    fn facts(&self)->&QualityNotificationFacts { &self.facts }
    async fn require_current(&self,c:&mut PgConnection)->Result<(),Error> {
        self.effect.require_current(c).await?;self.hold.require_current(c).await?;self.recipients.require_current(c).await?;
        let h=&self.hold.data().hold;
        if h.status!=erpx_server_quality::public::quality_read::HoldStatus::CancelPending || h.id!=self.facts.hold_id || h.company_id!=self.facts.company_id || h.version!=self.facts.version || h.updated_at!=self.facts.updated_at || self.recipients.recipients()!=self.facts.recipients {
            return Err(BusinessError::invalid("TEST_SOURCE_CHANGED","The joined owner sources changed.").into());
        }
        Ok(())
    }
}

#[tokio::test]
async fn quality_notice_joins_actual_quality_and_identity_witnesses_atomically() {
    use erpx_server_identity_access::public::quality_recipients::*;
    use erpx_server_quality::public::quality_http::{lock_quality_cancellation_writes,lock_quality_read_sources,prepare_quality_cancellation,read_quality_detail};
    use erpx_server_quality::public::quality_read::{CancellationAction,QualityCancellationCommand};
    use erpx_server_shared::public::{CommandKey,CompanyId,RecordId,UserId};
    fixture::with_database(3,|pool|async move {
        quality_facts::setup(&pool).await;quality_facts::hold(&pool,"joined-hold").await;
        sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','quality_notice_test','Quality test',false,1,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','quality_notice_test',1,'[\"quality.read\",\"quality.release\"]','[]','[\"holding\",\"branch\"]','fixture',0); INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy) VALUES('company','manager','local:manager','manager','active','local'); INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind,scope_node_id) VALUES('quality-notice-manager','manager','quality_notice_test','[]','company',0,9007199254740991,1,'branch','branch')").execute(pool.raw()).await.unwrap();
        for rollback in [true,false] {
            let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move {
                lock_quality_cancellation_writes(c).await?;
                lock_quality_read_sources(c).await?;
                let mut prep=lock_quality_recipient_sources(c).await?;
                lock_quality_notifications(prep.connection()).await?;
                let locks=prep.bind().await?;
                let company=CompanyId::new("company")?;let branch=RecordId::new("branch")?;
                let command=QualityCancellationCommand { command_key:CommandKey::new("joined-quality-key")?,hold_id:RecordId::new("joined-hold")?,expected_version:1,action:CancellationAction::Request,reason:"Reviewed incorrect hold".into() };
                let prepared=prepare_quality_cancellation(c,&command,&company,Some(&branch),&UserId::new("maker")?,1_800_000_000_000).await?;
                let effect=prepared.apply(c).await?;
                let hold=read_quality_detail(c,&company,Some(&branch),"joined-hold").await?.ok_or_else(||BusinessError::invalid("TEST_MISSING","Missing hold."))?;
                let h=&hold.data().hold;
                let recipients=inspect_quality_recipients(c,&locks,QualityRecipientRequest { company_id:company.clone(),branch_id:branch.clone(),creator:h.created_by.clone(),cancellation_requester:h.cancel_requested_by.clone(),status:QualityRecipientStatus::CancellationPending }).await?;
                let facts=QualityNotificationFacts { command_key:command.command_key.as_str().into(),company_id:h.company_id.clone(),hold_id:h.id.clone(),version:h.version,updated_at:h.updated_at,recipients:recipients.recipients().to_vec() };
                assert_eq!(facts.recipients,vec!["manager"]);
                let source=JoinedQualitySource { facts,hold,effect,recipients };
                let notifications=stage_quality_notifications(c,&source).await?;notifications.require_current(c,&source).await?;
                if rollback { return Err(BusinessError::invalid("TEST_ROLLBACK","Rollback joined effects.").into()); }
                Ok(())
            })).await;
            if rollback { assert!(result.is_err()); } else { result.unwrap(); }
            let expected=i64::from(!rollback);
            let counts:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM inventory_quality_cancellations WHERE id='quality-cancel-joined-quality-key'),(SELECT count(*) FROM staff_notifications WHERE id='joined-quality-key-manager'),(SELECT count(*) FROM inbox_events WHERE id='legacy:joined-quality-key-manager'),(SELECT count(*) FROM inbox_deliveries WHERE id='joined-quality-key-manager'),(SELECT version FROM inventory_quality_holds WHERE id='joined-hold')").fetch_one(pool.raw()).await.unwrap();
            assert_eq!(counts,(expected,expected,expected,expected,1+expected));
        }
    }).await;
}
impl PgQualityNotificationSource for QualitySource {
    fn facts(&self)->&QualityNotificationFacts { &self.0 }
    async fn require_current(&self,c:&mut PgConnection)->Result<(),Error> {
        let f=&self.0;
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM quality_notification_test_source WHERE company_id=$1 AND hold_id=$2 AND version=$3 AND updated_at=$4 AND recipients=$5)")
            .bind(&f.company_id).bind(&f.hold_id).bind(f.version).bind(f.updated_at).bind(&f.recipients).fetch_one(c).await?;
        if !valid { return Err(BusinessError::invalid("TEST_SOURCE_CHANGED","Test source changed.").into()); } Ok(())
    }
}
#[tokio::test]
async fn quality_notifications_preserve_conflicts_bridge_and_rollback() {
    fixture::with_database(2,|pool|async move {
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let m=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&m).await.unwrap();
        sqlx::raw_sql("CREATE TABLE quality_notification_test_source(company_id text,hold_id text,version bigint,updated_at bigint,recipients text[]); INSERT INTO quality_notification_test_source VALUES('co','hold',2,7,ARRAY['manager','reviewer','owner']),('co','empty-hold',2,7,ARRAY[]::text[]); INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES('quality-command-manager','other','other','other','other',1,'other',1),('existing-unique','co','owner','quality_hold','hold',2,'Quality handoff: review the source record',7)").execute(pool.raw()).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move {
            lock_quality_notifications(c).await?;
            let source=QualitySource(QualityNotificationFacts { command_key:"quality-command".into(),company_id:"co".into(),hold_id:"hold".into(),version:2,updated_at:7,recipients:vec!["manager".into(),"reviewer".into(),"owner".into()] });
            let retained=stage_quality_notifications(c,&source).await?;retained.require_current(c,&source).await?;
            sqlx::query("UPDATE quality_notification_test_source SET version=3").execute(&mut *c).await?;
            assert!(retained.require_current(c,&source).await.is_err());
            Err(BusinessError::invalid("TEST_ROLLBACK","Rollback owner effects.").into())
        })).await.unwrap_err();
        let counts:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM staff_notifications WHERE id='quality-command-manager'),(SELECT count(*) FROM staff_notifications WHERE id='quality-command-reviewer'),(SELECT count(*) FROM inbox_events WHERE id='legacy:quality-command-reviewer'),(SELECT count(*) FROM inbox_deliveries WHERE id='quality-command-reviewer')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(counts,(1,0,0,0));
        PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move {
            lock_quality_notifications(c).await?;
            let source=QualitySource(QualityNotificationFacts { command_key:"quality-command".into(),company_id:"co".into(),hold_id:"hold".into(),version:2,updated_at:7,recipients:vec!["manager".into(),"reviewer".into(),"owner".into()] });
            let retained=stage_quality_notifications(c,&source).await?;retained.require_current(c,&source).await?;Ok(())
        })).await.unwrap();
        let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM staff_notifications WHERE id='quality-command-reviewer'),(SELECT count(*) FROM inbox_events WHERE id='legacy:quality-command-reviewer'),(SELECT count(*) FROM inbox_deliveries WHERE id='quality-command-reviewer')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(counts,(1,1,1));
        let inbox:(String,String,String,String,i64,String)=sqlx::query_as("SELECT e.id,e.source_type,e.source_id,d.user_id,e.source_version,d.id FROM inbox_events e JOIN inbox_deliveries d ON d.event_id=e.id WHERE d.id='quality-command-reviewer'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(inbox,("legacy:quality-command-reviewer".into(),"quality_hold".into(),"hold".into(),"reviewer".into(),2,"quality-command-reviewer".into()));
        let no_op:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM staff_notifications WHERE id='quality-command-owner'),(SELECT count(*) FROM inbox_events WHERE id='legacy:quality-command-owner'),(SELECT count(*) FROM inbox_deliveries WHERE id='quality-command-owner'),(SELECT count(*) FROM staff_notifications WHERE id='existing-unique')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(no_op,(0,0,0,1));
        PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move {
            lock_quality_notifications(c).await?;
            let source=QualitySource(QualityNotificationFacts { command_key:"empty-command".into(),company_id:"co".into(),hold_id:"empty-hold".into(),version:2,updated_at:7,recipients:vec![] });
            let retained=stage_quality_notifications(c,&source).await?;retained.require_current(c,&source).await?;Ok(())
        })).await.unwrap();
        let empty:i64=sqlx::query_scalar("SELECT count(*) FROM staff_notifications WHERE record_id='empty-hold'").fetch_one(pool.raw()).await.unwrap();assert_eq!(empty,0);
        let empty_bridge:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM inbox_events WHERE source_id='empty-hold')+(SELECT count(*) FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id WHERE e.source_id='empty-hold')").fetch_one(pool.raw()).await.unwrap();assert_eq!(empty_bridge,0);
    }).await;
}
struct SyntheticSource(CountFlowNotificationFacts);

struct ActualCrmSource(CrmActivityNotificationFacts);
impl PgCrmActivityNotificationSource for ActualCrmSource {
    fn facts(&self) -> &CrmActivityNotificationFacts { &self.0 }
    async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        let f = &self.0;
        let actual: Option<(String,String,i64,String,i64)> = sqlx::query_as("SELECT company_id,owner_id,version,status,updated_at FROM crm_lead_activities WHERE id=$1")
            .bind(&f.activity_id).fetch_optional(c).await?;
        if actual != Some((f.company_id.clone(),f.recipient.clone(),f.version,f.status.clone(),f.updated_at)) {
            return Err(BusinessError::invalid("CRM_SOURCE_CHANGED", "CRM source changed.").into());
        }
        Ok(())
    }
    async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> { self.require_before(c).await }
}

#[tokio::test]
async fn crm_activity_notification_retains_source_and_both_conflict_types() {
    fixture::with_database(2, |pool| async move {
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('co','company','co','Co',NULL,'co',NULL,'active'),('branch','branch','branch','Branch','co','co',NULL,'active'); INSERT INTO crm_leads(id,company_id,branch_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,version,created_by,created_at,updated_at) VALUES('lead','co','branch','Lead','Customer','Contact','Staff','Brief','owner','2026-12-01','open',1,'owner',0,0); INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,created_by,created_at,updated_at) VALUES('activity','co','lead','call','Call customer','Discuss scope','owner','2026-12-01','owner',7,7)").execute(pool.raw()).await.unwrap();
        for key in ["one", "two"] {
            PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move {
                lock_crm_activity_notifications(c).await?;
                let source=ActualCrmSource(CrmActivityNotificationFacts { command_key:key.into(),company_id:"co".into(),recipient:"owner".into(),activity_id:"activity".into(),version:1,status:"open".into(),updated_at:7 });
                let retained=prepare_crm_activity_notification(c,&source).await?.persist(c).await?;
                retained.require_current(c).await?;
                Ok(())
            })).await.unwrap();
        }
        let rows: Vec<(String,String)> = sqlx::query_as("SELECT id,message FROM staff_notifications WHERE record_type='crm_activity' ORDER BY id").fetch_all(pool.raw()).await.unwrap();
        assert_eq!(rows, vec![("crm-activity-notice-one".into(),"CRM activity assigned: review the due date and next action".into())]);
        let inbox: (String,String,String,String,i64,String) = sqlx::query_as("SELECT e.id,e.source_type,e.source_id,d.user_id,e.source_version,d.id FROM inbox_events e JOIN inbox_deliveries d ON d.event_id=e.id WHERE d.id='crm-activity-notice-one'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(inbox,("legacy:crm-activity-notice-one".into(),"crm_activity".into(),"activity".into(),"owner".into(),1,"crm-activity-notice-one".into()));
        let no_op:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM inbox_events WHERE id='legacy:crm-activity-notice-two')+(SELECT count(*) FROM inbox_deliveries WHERE id='crm-activity-notice-two')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(no_op,0);
        // A different row occupying the command ID is also a no-op, as in TS.
        sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES('crm-activity-notice-collision','co','other','other','other',1,'other',1)").execute(pool.raw()).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move {
            lock_crm_activity_notifications(c).await?;
            let source=ActualCrmSource(CrmActivityNotificationFacts { command_key:"collision".into(),company_id:"co".into(),recipient:"owner".into(),activity_id:"activity".into(),version:1,status:"open".into(),updated_at:7 });
            prepare_crm_activity_notification(c,&source).await?.persist(c).await?.require_current(c).await?;
            Ok(())
        })).await.unwrap();
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM staff_notifications WHERE record_type='crm_activity'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(count,1);
        sqlx::query("UPDATE crm_lead_activities SET status='completed',version=2,updated_at=8 WHERE id='activity'").execute(pool.raw()).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move {
            lock_crm_activity_notifications(c).await?;
            let source=ActualCrmSource(CrmActivityNotificationFacts { command_key:"completion".into(),company_id:"co".into(),recipient:"owner".into(),activity_id:"activity".into(),version:2,status:"completed".into(),updated_at:8 });
            prepare_crm_activity_notification(c,&source).await?.persist(c).await?.require_current(c).await?;
            Ok(())
        })).await.unwrap();
        let message:String=sqlx::query_scalar("SELECT message FROM staff_notifications WHERE id='crm-activity-notice-completion'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(message,"CRM activity outcome recorded");
        let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move {
            lock_crm_activity_notifications(c).await?;
            let stale=ActualCrmSource(CrmActivityNotificationFacts { command_key:"stale".into(),company_id:"co".into(),recipient:"owner".into(),activity_id:"activity".into(),version:1,status:"open".into(),updated_at:7 });
            prepare_crm_activity_notification(c,&stale).await?.persist(c).await?;
            Ok(())
        })).await;
        assert!(matches!(result,Err(PgTransactionError::Business(error)) if error.code=="CRM_SOURCE_CHANGED"));
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM staff_notifications WHERE id='crm-activity-notice-stale'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(count,0);
        let rollback=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move {
            lock_crm_activity_notifications(c).await?;
            let source=ActualCrmSource(CrmActivityNotificationFacts { command_key:"rollback".into(),company_id:"co".into(),recipient:"owner".into(),activity_id:"activity".into(),version:2,status:"completed".into(),updated_at:8 });
            // Temporarily remove the existing unique-tuple conflict so this
            // unit inserts a notice and triggers its downstream Inbox rows.
            sqlx::query("DELETE FROM staff_notifications WHERE id='crm-activity-notice-completion'").execute(&mut *c).await?;
            prepare_crm_activity_notification(c,&source).await?.persist(c).await?;
            Err(BusinessError::invalid("TEST_ROLLBACK", "Force rollback.").into())
        })).await;
        assert!(matches!(rollback,Err(PgTransactionError::Business(error)) if error.code=="TEST_ROLLBACK"));
        let images:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM staff_notifications WHERE id='crm-activity-notice-rollback'),(SELECT count(*) FROM inbox_events WHERE id='legacy:crm-activity-notice-rollback'),(SELECT count(*) FROM inbox_deliveries WHERE id='crm-activity-notice-rollback'),(SELECT count(*) FROM staff_notifications WHERE id='crm-activity-notice-completion')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(images,(0,0,0,1));
    }).await;
}
impl PgCountFlowNotificationSource for SyntheticSource {
    fn facts(&self) -> &CountFlowNotificationFacts {
        &self.0
    }
    async fn require_before(&self, _: &mut PgConnection) -> Result<(), Error> {
        Ok(())
    }
    async fn require_after(&self, _: &mut PgConnection) -> Result<(), Error> {
        Ok(())
    }
}
#[tokio::test]
async fn notification_planned_image_is_retained_and_existing_unique_conflict_is_preserved() {
    fixture::with_database(2,|pool|async move{
 let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let m=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&m).await.unwrap();
 for id in ["notice-a","notice-b"] {PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move{lock_count_flow_notifications(c).await?;let source=SyntheticSource(CountFlowNotificationFacts{id:id.into(),company_id:"co".into(),recipient:"reviewer".into(),count_id:"count".into(),version:2,message:"Review".into(),created_at:1});let p=prepare_count_flow_notification(c,&source).await?;let retained=p.persist(c).await?;retained.require_current(c).await?;Ok(())})).await.unwrap();}
 let rows:(i64,String)=sqlx::query_as("SELECT count(*),min(id) FROM staff_notifications WHERE record_type='stock_count' AND record_id='count'").fetch_one(pool.raw()).await.unwrap();assert_eq!(rows,(1,"notice-a".into()));
}).await
}
