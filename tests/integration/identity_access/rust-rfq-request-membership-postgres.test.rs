#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_identity_access::public::rfq_request_membership::{
    rfq_request_current_principals, rfq_request_service_member, rfq_request_service_owner,
};

#[tokio::test]
async fn assignment_scope_and_liveness_match_service_member_sql() {
    fixture::with_database(2, |pool| async move {
        let mut c=pool.raw().acquire().await.unwrap();
        sqlx::raw_sql("CREATE TABLE assignments(company_id text,user_id text,scope_kind text,scope_node_id text,valid_from bigint,valid_until bigint,revoked_at bigint); INSERT INTO assignments VALUES('c','holding','holding',NULL,0,9000000000000,NULL),('c','branch','branch','b',0,9000000000000,NULL),('c','dept','department','d',0,9000000000000,NULL),('c','revoked','holding',NULL,0,9000000000000,1),('c','expired','holding',NULL,0,1,NULL),('c','future','holding',NULL,9000000000000,9000000000001,NULL)").execute(&mut *c).await.unwrap();
        assert!(rfq_request_service_member(&mut c,"c","holding",None,None).await.unwrap());
        assert!(rfq_request_service_member(&mut c,"c","branch",Some("b"),None).await.unwrap());
        assert!(!rfq_request_service_member(&mut c,"c","branch",Some("other"),None).await.unwrap());
        assert!(rfq_request_service_member(&mut c,"c","dept",None,Some("d")).await.unwrap());
        for user in ["revoked","expired","future"] {assert!(!rfq_request_service_member(&mut c,"c",user,None,None).await.unwrap());}
        assert!(!rfq_request_service_member(&mut c,"other","holding",None,None).await.unwrap());
        sqlx::raw_sql("CREATE TABLE iam_members(company_id text,user_id text,person_id text,status text); INSERT INTO iam_members VALUES('c','old','p1','superseded'),('c','new','p1','active'),('c','ambiguous','p2','superseded'),('c','a','p2','active'),('c','b','p2','active'),('other','foreign','p1','active')").execute(&mut *c).await.unwrap();
        let ids=vec!["old".to_owned(),"ambiguous".to_owned(),"missing".to_owned(),"old".to_owned()];
        assert_eq!(rfq_request_current_principals(&mut c,"c",&ids).await.unwrap(),vec![("old".into(),"new".into()),("ambiguous".into(),"ambiguous".into()),("missing".into(),"missing".into()),("old".into(),"new".into())]);
        assert!(rfq_request_current_principals(&mut c,"c",&[]).await.unwrap().is_empty());
        sqlx::raw_sql("ALTER TABLE assignments ADD COLUMN id text, ADD COLUMN role text; UPDATE assignments SET id=user_id,role='owner'; CREATE FUNCTION iam_role_allows(text,text,text) RETURNS boolean LANGUAGE sql AS $$ SELECT $1='owner' AND $3='manage_erpx.access' $$; CREATE FUNCTION iam_assignment_active(text,bigint) RETURNS boolean LANGUAGE sql AS $$ SELECT true $$; CREATE FUNCTION iam_principal_active(text,text,bigint) RETURNS boolean LANGUAGE sql AS $$ SELECT true $$").execute(&mut *c).await.unwrap();
        assert!(rfq_request_service_owner(&mut c,"holding","c",1000).await.unwrap());
        assert!(!rfq_request_service_owner(&mut c,"branch","c",1000).await.unwrap());
        assert!(!rfq_request_service_owner(&mut c,"holding","c",9000000000001).await.unwrap());
    }).await;
}
