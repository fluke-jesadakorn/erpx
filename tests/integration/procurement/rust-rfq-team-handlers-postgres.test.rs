#[path = "../../support/rust-postgres.rs"]
mod fixture;
#[path = "../../../src/composition/src/procurement_rfq_approval.rs"]
mod procurement_rfq_approval;
#[path = "../../../src/composition/src/procurement_rfq_access.rs"]
mod request_access;

#[tokio::test]
async fn team_handlers_resolve_principals_dedupe_and_apply_scope_membership() {
    fixture::with_database(2, |pool| async move {
        let mut c=pool.raw().acquire().await.unwrap();
        // A null team must return before any source-table query, as in TS.
        assert!(request_access::rfq_request_team_handlers(&mut c,"c",None,None,None).await.unwrap().is_empty());
        sqlx::raw_sql("CREATE TABLE employee_request_events(id text,company_id text,request_id text,action text,actor_id text,note text,evidence_json text,request_version bigint,created_at bigint)").execute(&mut *c).await.unwrap();
        // TS event query references People even when there are no event rows.
        assert!(request_access::rfq_request_detail_events(&mut c,"c","r").await.is_err());
        sqlx::raw_sql("CREATE TABLE service_teams(id text,company_id text,active bigint); CREATE TABLE service_team_members(team_id text,company_id text,user_id text,responsibility text); CREATE TABLE hr_people(company_id text,user_id text,legal_name text,merged_into_id text); CREATE TABLE assignments(company_id text,user_id text,scope_kind text,scope_node_id text,valid_from bigint,valid_until bigint,revoked_at bigint); CREATE TABLE iam_members(company_id text,user_id text,person_id text,status text); INSERT INTO service_teams VALUES('team','c',1),('inactive','c',0); INSERT INTO service_team_members VALUES('team','c','a-old','lead'),('team','c','b-current','member'),('team','c','c-revoked','member'),('inactive','c','b-current','lead'); INSERT INTO hr_people VALUES('c','a-old','Old Name',NULL),('c','b-current','Current Name',NULL); INSERT INTO iam_members VALUES('c','a-old','p','superseded'),('c','b-current','p','active'); INSERT INTO assignments VALUES('c','b-current','branch','b',0,9000000000000,NULL),('c','c-revoked','holding',NULL,0,9000000000000,1)").execute(&mut *c).await.unwrap();
        let rows=request_access::rfq_request_team_handlers(&mut c,"c",Some("b"),None,Some("team")).await.unwrap();
        assert_eq!(rows.len(),1);assert_eq!(rows[0].user_id,"b-current");assert_eq!(rows[0].responsibility,"member");assert_eq!(rows[0].name,"Current Name");
        assert!(request_access::rfq_request_team_handlers(&mut c,"c",Some("other"),None,Some("team")).await.unwrap().is_empty());
        assert!(request_access::rfq_request_team_handlers(&mut c,"c",Some("b"),None,Some("inactive")).await.unwrap().is_empty());
        assert!(request_access::rfq_request_team_handlers(&mut c,"other",Some("b"),None,Some("team")).await.unwrap().is_empty());
        assert!(request_access::rfq_request_detail_events(&mut c,"c","r").await.unwrap().is_empty());
        sqlx::raw_sql("INSERT INTO employee_request_events VALUES('b','c','r','edit','missing','B','{}',2,1),('a','c','r','create','b-current','A','{}',1,1),('foreign','other','r','edit','b-current','X','{}',1,0)").execute(&mut *c).await.unwrap();
        let events=request_access::rfq_request_detail_events(&mut c,"c","r").await.unwrap();
        assert_eq!(events.iter().map(|e|e.id.as_str()).collect::<Vec<_>>(),["a","b"]);
        assert_eq!(events[0].actor_name,"Current Name");assert_eq!(events[1].actor_name,"Company member");
    }).await;
}
