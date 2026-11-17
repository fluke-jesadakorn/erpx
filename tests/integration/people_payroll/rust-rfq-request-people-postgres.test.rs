#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_people_payroll::public::rfq_request_people::*;

#[tokio::test]
async fn event_and_handler_name_fallbacks_are_owner_scoped() {
    fixture::with_database(2, |pool| async move {
        let mut c=pool.raw().acquire().await.unwrap();
        sqlx::raw_sql("CREATE TABLE hr_people(company_id text,user_id text,legal_name text,merged_into_id text); INSERT INTO hr_people VALUES('c','a','Alice',NULL),('c','old','Old','a'),('other','foreign','Foreign',NULL)").execute(&mut *c).await.unwrap();
        assert_eq!(rfq_request_actor_name(&mut c,"c","a").await.unwrap(),"Alice");
        assert_eq!(rfq_request_actor_name(&mut c,"c","old").await.unwrap(),"Company member");
        assert_eq!(rfq_request_actor_name(&mut c,"c","foreign").await.unwrap(),"Company member");
        assert_eq!(rfq_request_handler_name(&mut c,"c","old").await.unwrap(),"old");
    }).await;
}
