#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_collaboration_server::public::rfq_notifications::*;
use erpx_identity_access_server::public::staff_notifications::rfq_notification_recipients;
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;

struct Source {
    facts: RfqNoticeFacts,
}
impl RfqNoticeSource for Source {
    fn facts(&self) -> &RfqNoticeFacts {
        &self.facts
    }
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let version: Option<i64> =
            sqlx::query_scalar("SELECT version FROM rfq_test_source WHERE id=$1")
                .bind(&self.facts.id)
                .fetch_optional(c)
                .await?;
        if version != Some(self.facts.version) {
            return Err(
                BusinessError::new(FailureKind::Conflict, "SOURCE_CHANGED", "changed").into(),
            );
        }
        Ok(())
    }
    async fn recipients(&self, c: &mut PgConnection) -> Result<Vec<String>, Error> {
        Ok(rfq_notification_recipients(
            c,
            &self.facts.company_id,
            &self.facts.branch_id,
            self.facts.department_id.as_deref(),
        )
        .await?)
    }
}
#[tokio::test]
async fn rfq_notification_matches_identity_scope_and_rolls_back() {
    fixture::with_database(2,|pool|async move {
        let mut c=pool.raw().acquire().await.unwrap();
        sqlx::raw_sql("CREATE TABLE assignments(id text,user_id text,company_id text,role text,scope_kind text,scope_node_id text,active boolean); CREATE TABLE staff_notifications(id text primary key,company_id text,user_id text,record_type text,record_id text,record_version bigint,message text,created_at bigint); CREATE TABLE rfq_test_source(id text,version bigint); INSERT INTO rfq_test_source VALUES('rfq',2); INSERT INTO assignments VALUES('a','user1','c','buyer','holding',NULL,true),('b','user1','c','buyer','branch','b',true),('c','user2','c','buyer','department','d',true),('d','wrongbranch','c','buyer','branch','other',true),('e','wrongrole','c','viewer','holding',NULL,true),('f','inactive','c','buyer','holding',NULL,false),('g','othercompany','other','buyer','holding',NULL,true); CREATE FUNCTION iam_assignment_active(text,bigint) RETURNS boolean LANGUAGE sql STABLE AS $$ SELECT active FROM assignments WHERE id=$1 $$; CREATE FUNCTION iam_role_allows(text,text,text) RETURNS boolean LANGUAGE sql IMMUTABLE AS $$ SELECT $1='buyer' AND $3='commerce.purchase' $$;").execute(&mut *c).await.unwrap();
        let source=Source{facts:RfqNoticeFacts{id:"rfq".into(),company_id:"c".into(),branch_id:"b".into(),department_id:Some("d".into()),version:2,updated_at:20}};
        let mut tx=c.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        assert!(notify_rfq_staff(&mut tx,"key",&source).await.is_err());
        lock_rfq_notifications(&mut tx).await.unwrap();
        notify_rfq_staff(&mut tx,"key",&source).await.unwrap();notify_rfq_staff(&mut tx,"key",&source).await.unwrap();
        let rows:Vec<(String,String,String,i64,String,i64)>=sqlx::query_as("SELECT id,user_id,record_type,record_version,message,created_at FROM staff_notifications ORDER BY id").fetch_all(&mut *tx).await.unwrap();
        assert_eq!(rows,vec![("key-user1".into(),"user1".into(),"procurement_rfq".into(),2,"Supplier quotation review changed: review the next action".into(),20),("key-user2".into(),"user2".into(),"procurement_rfq".into(),2,"Supplier quotation review changed: review the next action".into(),20)]);
        sqlx::query("UPDATE rfq_test_source SET version=3 WHERE id='rfq'").execute(&mut *tx).await.unwrap();
        assert!(notify_rfq_staff(&mut tx,"other",&source).await.is_err());
        tx.rollback().await.unwrap();
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM staff_notifications").fetch_one(&mut *c).await.unwrap(),0);
    }).await;
}
