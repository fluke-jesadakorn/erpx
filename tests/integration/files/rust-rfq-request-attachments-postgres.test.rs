#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_files::public::rfq_request_attachments::read_rfq_request_attachments;

#[tokio::test]
async fn attachment_rows_match_records_source_filter_and_order() {
    fixture::with_database(2, |pool| async move {
        let mut c = pool.raw().acquire().await.unwrap();
        sqlx::raw_sql("CREATE TABLE attachments(id text primary key, owner_type text NOT NULL, owner_id text NOT NULL, original_name text NOT NULL, media_type text NOT NULL, bytes bigint NOT NULL, checksum_sha256 text NOT NULL, purpose text NOT NULL, revision bigint NOT NULL, created_at bigint NOT NULL, revoked_at bigint); INSERT INTO attachments VALUES ('b','employee_request','r','B','image/png',5,'bb','request_evidence',2,1,NULL),('a','employee_request','r','A','image/png',4,'aa','request_evidence',1,1,NULL),('later','employee_request','r','Later','image/png',6,'cc','fulfillment',3,2,NULL),('revoked','employee_request','r','Gone','image/png',7,'dd','request_evidence',1,0,2),('foreign','employee_request','other','Foreign','image/png',1,'ee','request_evidence',1,0,NULL),('type','other','r','Other','image/png',1,'ff','request_evidence',1,0,NULL)").execute(&mut *c).await.unwrap();
        let rows = read_rfq_request_attachments(&mut c, "r").await.unwrap();
        assert_eq!(rows.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["a", "b", "later"]);
        assert_eq!((rows[0].original_name.as_str(), rows[0].bytes, rows[0].revision), ("A", 4, 1));
        assert!(read_rfq_request_attachments(&mut c, "missing").await.unwrap().is_empty());
    }).await;
}
