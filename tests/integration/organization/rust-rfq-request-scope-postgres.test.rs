#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_organization::public::rfq_request_scope::*;

#[tokio::test]
async fn active_nodes_and_null_branch_match_the_source_predicates() {
    fixture::with_database(2, |pool| async move {
        let mut c = pool.raw().acquire().await.unwrap();
        sqlx::raw_sql("CREATE TABLE organization_nodes(id text,company_id text,kind text,status text,branch_id text); INSERT INTO organization_nodes VALUES('c','c','company','active',NULL),('b','c','branch','active','b'),('inactive','c','branch','inactive','inactive'),('d','c','department','active','b'),('rootd','c','department','active',NULL)").execute(&mut *c).await.unwrap();
        assert!(rfq_scope_company(&mut c,"c").await.unwrap());
        assert!(!rfq_scope_company(&mut c,"missing").await.unwrap());
        assert!(rfq_scope_branch(&mut c,"c","b").await.unwrap());
        assert!(!rfq_scope_branch(&mut c,"c","inactive").await.unwrap());
        assert!(!rfq_scope_branch(&mut c,"other","b").await.unwrap());
        assert!(rfq_scope_department(&mut c,"c",Some("b"),"d").await.unwrap());
        assert!(!rfq_scope_department(&mut c,"c",None,"d").await.unwrap());
        assert!(rfq_scope_department(&mut c,"c",None,"rootd").await.unwrap());
    }).await;
}
