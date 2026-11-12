//! Actual Workflow PMS social projection over migrated Holding PostgreSQL.
//!
//! Proves the exact outbox/command-receipt join, business-day post identity,
//! body, post/source/receipt/notification rows and receipt idempotency. The
//! owner ports below are a test-local composition; the production owner
//! functions live in each owner crate.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::job_projector::{
    EventMessage, EventPayload, PmsSocialFuture, PmsSocialNotice, PmsSocialOriginal,
    PmsSocialPorts, PmsSocialPost, PmsSocialSource, pms_social_original, project_pms_social,
};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

const PMS_SOURCES_SQL: &str = "SELECT r.id,r.property_id,r.kind,r.data_json::jsonb data,r.version,r.updated_at,\n    p.company_id,p.branch_id,p.name property_name,p.business_date\n  FROM pms_records r JOIN pms_properties p ON p.id=r.property_id";

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}

fn read_failed(_error: sqlx::Error) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TEST_PMS_SOCIAL_READ_FAILED",
        "The test PMS social fact could not be read.",
    )
}

#[derive(Default)]
struct Ports {
    recipients: Vec<String>,
    last_review: Option<String>,
    last_recipient: Option<String>,
}

impl PmsSocialPorts for Ports {
    fn original<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        event_id: &'a str,
        event_type: &'a str,
    ) -> PmsSocialFuture<'a, Option<PmsSocialOriginal>> {
        Box::pin(async move { pms_social_original(connection, event_id, event_type).await })
    }

    fn payout_source<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        id: &'a str,
        property_id: &'a str,
    ) -> PmsSocialFuture<'a, Option<String>> {
        Box::pin(async move {
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT data_json::jsonb->>'originalId' FROM pms_records \
                 WHERE id=$1 AND property_id=$2 AND kind='entry'",
            )
            .bind(id)
            .bind(property_id)
            .fetch_optional(&mut *connection)
            .await
            .map(Option::flatten)
            .map_err(read_failed)
        })
    }

    fn source<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        id: Option<&'a str>,
        company_id: &'a str,
        property_id: &'a str,
    ) -> PmsSocialFuture<'a, Option<PmsSocialSource>> {
        Box::pin(async move {
            let sql = format!(
                "SELECT pr.id,pr.kind,pr.data data_json,pr.version,pr.company_id,pr.branch_id,\
                 pr.property_name,pr.business_date FROM ({PMS_SOURCES_SQL}) pr \
                 WHERE pr.id=$1 AND pr.company_id=$2 AND pr.property_id=$3"
            );
            let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(id)
                .bind(company_id)
                .bind(property_id)
                .fetch_optional(&mut *connection)
                .await
                .map_err(read_failed)?;
            let Some(row) = row else {
                return Ok(None);
            };
            Ok(Some(PmsSocialSource {
                id: row.try_get("id").map_err(read_failed)?,
                kind: row.try_get("kind").map_err(read_failed)?,
                company_id: row.try_get("company_id").map_err(read_failed)?,
                branch_id: row.try_get("branch_id").map_err(read_failed)?,
                property_name: row.try_get("property_name").map_err(read_failed)?,
                business_date: row.try_get("business_date").map_err(read_failed)?,
                data_json: row.try_get::<Value, _>("data_json").map_err(read_failed)?,
                version: row.try_get("version").map_err(read_failed)?,
            }))
        })
    }

    fn lock<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        post_id: &'a str,
    ) -> PmsSocialFuture<'a, ()> {
        Box::pin(async move {
            sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
                .bind(post_id)
                .execute(&mut *connection)
                .await
                .map_err(read_failed)?;
            Ok(())
        })
    }

    fn received<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        event_id: &'a str,
    ) -> PmsSocialFuture<'a, bool> {
        Box::pin(async move {
            let row = sqlx::query("SELECT 1 FROM pms_social_event_receipts WHERE event_id=$1")
                .bind(event_id)
                .fetch_optional(&mut *connection)
                .await
                .map_err(read_failed)?;
            Ok(row.is_some())
        })
    }

    fn publish<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        post: PmsSocialPost,
        source_id: &'a str,
        event_id: &'a str,
        received_at: i64,
    ) -> PmsSocialFuture<'a, ()> {
        Box::pin(async move {
            sqlx::query(
                "INSERT INTO social_posts(id,kind,company_id,branch_id,author_id,author_name,body,\
                 audience_kind,purpose,last_command_id,created_at,updated_at)\n\
                 VALUES($1,'system',$2,$3,$4,'PMS',$5,'branch','update',$6,$7,$8) ON CONFLICT(id) DO UPDATE SET\n\
                 body=excluded.body,last_command_id=excluded.last_command_id,version=social_posts.version+1,\
                 updated_at=excluded.updated_at\n\
                 WHERE social_posts.updated_at<=excluded.updated_at",
            )
            .bind(&post.post_id)
            .bind(&post.company_id)
            .bind(&post.branch_id)
            .bind(&post.actor_id)
            .bind(&post.body)
            .bind(&post.command_id)
            .bind(post.at)
            .bind(post.at)
            .execute(&mut *connection)
            .await
            .map_err(read_failed)?;
            sqlx::query(
                "INSERT INTO social_post_sources(post_id,ordinal,source_kind,source_id) \
                 VALUES($1,0,'pms_record',$2) ON CONFLICT DO NOTHING",
            )
            .bind(&post.post_id)
            .bind(source_id)
            .execute(&mut *connection)
            .await
            .map_err(read_failed)?;
            sqlx::query(
                "INSERT INTO pms_social_event_receipts(event_id,post_id,created_at) VALUES($1,$2,$3)",
            )
            .bind(event_id)
            .bind(&post.post_id)
            .bind(received_at)
            .execute(&mut *connection)
            .await
            .map_err(read_failed)?;
            Ok(())
        })
    }

    fn recipients<'a>(
        &'a mut self,
        _connection: &'a mut PgConnection,
        _id: &'a str,
        _company_id: &'a str,
        _actor_id: &'a str,
        review: Option<&'a str>,
        recipient: Option<&'a str>,
    ) -> PmsSocialFuture<'a, Vec<String>> {
        self.last_review = review.map(str::to_owned);
        self.last_recipient = recipient.map(str::to_owned);
        let recipients = self.recipients.clone();
        Box::pin(async move { Ok(recipients) })
    }

    fn version<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        post_id: &'a str,
    ) -> PmsSocialFuture<'a, Option<i64>> {
        Box::pin(async move {
            sqlx::query_scalar("SELECT version FROM social_posts WHERE id=$1")
                .bind(post_id)
                .fetch_optional(&mut *connection)
                .await
                .map_err(read_failed)
        })
    }

    fn notify<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        notice: PmsSocialNotice,
    ) -> PmsSocialFuture<'a, ()> {
        Box::pin(async move {
            sqlx::query(
                "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
                 record_version,message,created_at)\n\
                 VALUES($1,$2,$3,'post',$4,$5,$6,$7) ON CONFLICT(id) DO UPDATE SET \
                 record_version=excluded.record_version,message=excluded.message,\n\
                 created_at=excluded.created_at,read_at=NULL \
                 WHERE staff_notifications.created_at<=excluded.created_at",
            )
            .bind(&notice.id)
            .bind(&notice.company_id)
            .bind(&notice.user_id)
            .bind(&notice.post_id)
            .bind(notice.version)
            .bind(&notice.body)
            .bind(notice.at)
            .execute(&mut *connection)
            .await
            .map_err(read_failed)?;
            Ok(())
        })
    }
}

fn message(id: &str, event_type: &str, aggregate_id: &str) -> EventMessage {
    EventMessage {
        holding_id: "holding".into(),
        id: id.into(),
        event_type: event_type.into(),
        aggregate_id: aggregate_id.into(),
        aggregate_version: 1,
        payload: EventPayload {
            id: aggregate_id.into(),
            state: None,
            version: 1,
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
        },
    }
}

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,\
         version,created_by,updated_at) VALUES\
         ('property','company','branch','Grand Hotel','2026-10-05','open','{}',1,'creator',100);\
         INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES\
         ('reservation','property','reservation','{\"state\":\"confirmed\",\"date\":\"2026-10-06\"}',4,100),\
         ('entry-1','property','entry','{\"originalId\":\"refund-source\"}',1,100),\
         ('refund-source','property','refund','{\"state\":\"pending\",\"date\":\"2026-10-07\"}',2,100);\
         INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
         aggregate_version,payload_json,created_at) VALUES\
         ('event-1','command-1','pms.reservation.create','pms','reservation',1,'{}',1790000000000),\
         ('event-refund','command-2','pms.refund.pay','pms','property',1,'{}',1790000000000);\
         INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES\
         ('command-1','pms.reservation.create','creator','hash','reservation',\
          '{\"id\":\"reservation\",\"propertyId\":\"property\"}',1790000000000),\
         ('command-2','pms.refund.pay','creator','hash','entry-1',\
          '{\"id\":\"entry-1\",\"propertyId\":\"property\"}',1790000000000);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn original_joins_the_outbox_event_to_its_command_receipt() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut tx = connection.begin().await.unwrap();
        let original = pms_social_original(&mut tx, "event-1", "pms.reservation.create")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(original.command_id, "command-1");
        assert_eq!(original.created_at, 1_790_000_000_000);
        assert_eq!(original.actor_id, "creator");
        assert_eq!(
            original.result_json,
            r#"{"id":"reservation","propertyId":"property"}"#
        );
        assert!(
            pms_social_original(&mut tx, "event-1", "pms.refund.pay")
                .await
                .unwrap()
                .is_none()
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn projects_post_receipt_notification_and_is_receipt_idempotent() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut tx = connection.begin().await.unwrap();
        let now = 1_790_000_100_000_i64;
        let mut ports = Ports {
            recipients: vec!["manager".into()],
            ..Ports::default()
        };
        let event = message("event-1", "pms.reservation.create", "reservation");

        assert_eq!(
            project_pms_social(&mut tx, &mut ports, &event, now)
                .await
                .unwrap(),
            Some(())
        );
        assert_eq!(ports.last_review, None);
        assert_eq!(ports.last_recipient, None);

        let post = sqlx::query(
            "SELECT kind,company_id,branch_id,author_id,author_name,body,audience_kind,purpose,\
             last_command_id,version,created_at,updated_at FROM social_posts WHERE id=$1",
        )
        .bind("pms-post-83b92a95fc4e224578285f30e46e124571b5820b")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(post.get::<String, _>("kind"), "system");
        assert_eq!(post.get::<String, _>("company_id"), "company");
        assert_eq!(post.get::<String, _>("branch_id"), "branch");
        assert_eq!(post.get::<String, _>("author_id"), "creator");
        assert_eq!(post.get::<String, _>("author_name"), "PMS");
        assert_eq!(
            post.get::<String, _>("body"),
            "Grand Hotel · ยืนยันการจอง / Reservation confirmed\nconfirmed · วันที่ธุรกิจ / Business date 2026-10-06"
        );
        assert_eq!(post.get::<String, _>("audience_kind"), "branch");
        assert_eq!(post.get::<String, _>("purpose"), "update");
        assert_eq!(post.get::<String, _>("last_command_id"), "command-1");
        assert_eq!(post.get::<i64, _>("version"), 1);
        assert_eq!(post.get::<i64, _>("created_at"), 1_790_000_000_000);
        assert_eq!(post.get::<i64, _>("updated_at"), 1_790_000_000_000);

        assert_eq!(
            sqlx::query_as::<_, (String, i32, String, String)>(
                "SELECT post_id,ordinal,source_kind,source_id FROM social_post_sources",
            )
            .fetch_one(&mut *tx)
            .await
            .unwrap(),
            (
                "pms-post-83b92a95fc4e224578285f30e46e124571b5820b".into(),
                0,
                "pms_record".into(),
                "reservation".into()
            )
        );
        assert_eq!(
            sqlx::query_as::<_, (String, i64)>(
                "SELECT post_id,created_at FROM pms_social_event_receipts WHERE event_id='event-1'",
            )
            .fetch_one(&mut *tx)
            .await
            .unwrap(),
            (
                "pms-post-83b92a95fc4e224578285f30e46e124571b5820b".into(),
                now
            )
        );
        let notice = sqlx::query(
            "SELECT user_id,record_type,record_id,record_version,message,created_at,read_at \
             FROM staff_notifications",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(notice.get::<String, _>("user_id"), "manager");
        assert_eq!(notice.get::<String, _>("record_type"), "post");
        assert_eq!(
            notice.get::<String, _>("record_id"),
            "pms-post-83b92a95fc4e224578285f30e46e124571b5820b"
        );
        assert_eq!(notice.get::<i64, _>("record_version"), 1);
        assert_eq!(
            notice.get::<String, _>("message"),
            "Grand Hotel · ยืนยันการจอง / Reservation confirmed\nconfirmed · วันที่ธุรกิจ / Business date 2026-10-06"
        );
        assert_eq!(notice.get::<i64, _>("created_at"), 1_790_000_000_000);
        assert_eq!(notice.get::<Option<i64>, _>("read_at"), None);

        // A redelivered event takes the advisory lock, sees the retained
        // receipt and never publishes or notifies again.
        assert_eq!(
            project_pms_social(&mut tx, &mut ports, &event, now + 1)
                .await
                .unwrap(),
            Some(())
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM social_posts")
                .fetch_one(&mut *tx)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM staff_notifications")
                .fetch_one(&mut *tx)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT version FROM social_posts")
                .fetch_one(&mut *tx)
                .await
                .unwrap(),
            1
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn refund_pay_resolves_the_payout_source_and_review_parameters() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut tx = connection.begin().await.unwrap();
        let mut ports = Ports {
            recipients: vec!["finance".into(), "cashier".into()],
            ..Ports::default()
        };
        let event = message("event-refund", "pms.refund.pay", "property");
        assert_eq!(
            project_pms_social(&mut tx, &mut ports, &event, 1_790_000_200_000)
                .await
                .unwrap(),
            Some(())
        );
        // pms.refund.pay carries no review and falls back to recordedBy /
        // requestedBy / assigneeId / userId, none of which exist here.
        assert_eq!(ports.last_review, None);
        assert_eq!(ports.last_recipient, None);
        let post = sqlx::query("SELECT body,last_command_id FROM social_posts")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
        assert_eq!(post.len(), 1);
        assert_eq!(
            post[0].get::<String, _>("body"),
            "Grand Hotel · จ่ายเงินคืนแล้ว / Refund paid\npending · วันที่ธุรกิจ / Business date 2026-10-07"
        );
        assert_eq!(post[0].get::<String, _>("last_command_id"), "command-2");
        assert!(
            sqlx::query("SELECT 1 FROM social_post_sources WHERE source_id='refund-source'",)
                .fetch_optional(&mut *tx)
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM staff_notifications")
                .fetch_one(&mut *tx)
                .await
                .unwrap(),
            2
        );
        tx.rollback().await.unwrap();
    })
    .await;
}
