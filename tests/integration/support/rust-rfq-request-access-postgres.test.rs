#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_support::public::rfq_request_access::*;

#[tokio::test]
async fn workspace_and_team_source_match_original_scope() {
    assert!(rfq_in_service_workspace(
        "c",
        None,
        None,
        "c",
        Some("b"),
        Some("d")
    ));
    assert!(rfq_in_service_workspace(
        "c",
        Some(""),
        Some("d"),
        "c",
        Some("b"),
        Some("d")
    ));
    assert!(!rfq_in_service_workspace(
        "c",
        Some("b"),
        Some("d"),
        "c",
        Some("other"),
        Some("d")
    ));
    assert!(!rfq_in_service_workspace(
        "c",
        None,
        Some("d"),
        "other",
        None,
        Some("d")
    ));
    fixture::with_database(2, |pool| async move {
        let mut c=pool.raw().acquire().await.unwrap();
        sqlx::raw_sql("CREATE TABLE service_teams(id text,company_id text,name text,active int); INSERT INTO service_teams VALUES('a','c','Inactive team',0),('b','other','Foreign',1)").execute(&mut *c).await.unwrap();
        assert_eq!(rfq_request_team_name(&mut c,"c","a").await.unwrap().as_deref(),Some("Inactive team"));
        assert!(rfq_request_team_name(&mut c,"c","b").await.unwrap().is_none());
    }).await;
}
