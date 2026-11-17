#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_customer_360::public::customer_portal::{
    PgCustomerPortal, PortalAction, PortalCommandFacts, PortalCommandInput, PortalCustomerActor,
    PortalPurchaseScope,
};
use erpx_server_customer_360::public::portal_action_source::{lock_portal_action_sources,read_portal_action_source};
use erpx_server_runtime::public::postgres::{load_migrations, migrate_database, MigrationTrack, PgTransactionError};
use sqlx::Connection;

#[tokio::test]
async fn postgres_portal_requires_serializable_and_preserves_record_replay_and_rollback() {
    fixture::with_database(4, |pool| async move {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let migrations = load_migrations(root, MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &migrations).await.unwrap();
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES ('company','company','company','Company',NULL,'company',NULL,'active',1),('branch','branch','branch','Branch','company','company',NULL,'active',1);\
          INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES ('customer','company','customer','C-1','Customer','THB','active',1,10,10);\
          INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES ('order','company','branch',NULL,'sales','SO-1','customer',NULL,'THB','2026-09-20','2026-09-23','','confirmed',100,0,3,'owner',100,100);\
          INSERT INTO customer_access_bindings(id,company_id,order_id,customer_id,email,expires_at,status,reason,created_by,approved_by,version,created_at,updated_at) VALUES ('binding','company','order','customer','customer@example.test',2000,'approved','approved','creator','reviewer',2,100,100);\
          INSERT INTO customer_access_identities(binding_id,user_id,email,bound_at) VALUES ('binding','user','customer@example.test',100);")
            .execute(pool.raw()).await.unwrap();
        let actor = PortalCustomerActor::new("user", "customer@example.test", None).unwrap();
        let other = PortalCustomerActor::new("other", "customer@example.test", None).unwrap();
        let mut conn = pool.raw().acquire().await.unwrap();
        assert!(PgCustomerPortal::new(&mut conn).replay("request-key").await.is_err());
        let mut tx = conn.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let mut repo = PgCustomerPortal::new(&mut tx);
        let binding = repo.binding("binding", &actor.email).await.unwrap().unwrap();
        assert_eq!((binding.binding_version, binding.order_version), (2, 3));
        assert!(repo.identity("binding", &actor).await.unwrap());
        assert!(!repo.identity("binding", &other).await.unwrap());
        assert!(repo.binding("binding", "wrong@example.test").await.unwrap().is_none());
        assert!(repo.replay("request-key").await.unwrap().is_none());
        let facts = PortalCommandFacts {
            input: PortalCommandInput { binding_id: "binding".into(), expected_binding_version: 2, action: PortalAction::RequestSupport, note: "Help".into(), attachment_set_ids: vec![] },
            key: "request-key".into(), id: "action-1".into(), now: 500,
            request_hash: "a".repeat(64), actor: actor.clone(), binding,
        };
        repo.record(&facts).await.unwrap();
        let prior = repo.replay("request-key").await.unwrap().unwrap();
        assert_eq!((prior.id.as_str(), prior.user_id.as_str(), prior.request_hash.as_str()), ("action-1", "user", facts.request_hash.as_str()));
        assert_eq!(repo.existing("binding", PortalAction::RequestSupport).await.unwrap().unwrap().id, "action-1");
        assert!(repo.existing("binding", PortalAction::AcknowledgeDelivery).await.unwrap().is_none());
        let rows = repo.projection(&actor, &PortalPurchaseScope::disabled(), 500, "read-1").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].binding_id, "binding");
        assert_eq!(rows[0].actions.len(), 1);
        repo.revalidate_source(&actor, &facts.binding, 500).await.unwrap();
        let error = repo.revalidate_source(&other, &facts.binding, 500).await.unwrap_err();
        assert!(matches!(error, PgTransactionError::Business(error) if error.code == "CUSTOMER_PORTAL_BINDING_UNAVAILABLE"));
        assert!(repo.revalidate_source(&actor, &facts.binding, 2000).await.is_err());
        drop(repo);
        // An evidence bind may advance the binding version without changing
        // the original scope checked by the legacy finish boundary.
        sqlx::query("UPDATE customer_access_bindings SET version=version+1 WHERE id='binding'").execute(&mut *tx).await.unwrap();
        PgCustomerPortal::new(&mut tx).revalidate_source(&actor, &facts.binding, 500).await.unwrap();
        sqlx::query("UPDATE commerce_orders SET version=version+1 WHERE id='order'").execute(&mut *tx).await.unwrap();
        assert!(PgCustomerPortal::new(&mut tx).revalidate_source(&actor, &facts.binding, 500).await.is_err());
        sqlx::query("UPDATE commerce_orders SET version=3 WHERE id='order'").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE commerce_orders SET state='fulfilled' WHERE id='order'").execute(&mut *tx).await.unwrap();
        assert!(PgCustomerPortal::new(&mut tx).revalidate_source(&actor, &facts.binding, 500).await.is_err());
        sqlx::query("UPDATE commerce_orders SET state='confirmed' WHERE id='order'").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE customer_access_bindings SET status='revoked' WHERE id='binding'").execute(&mut *tx).await.unwrap();
        assert!(PgCustomerPortal::new(&mut tx).revalidate_source(&actor, &facts.binding, 500).await.is_err());
        tx.rollback().await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM customer_portal_actions").fetch_one(&mut *conn).await.unwrap();
        let reads: i64 = sqlx::query_scalar("SELECT count(*) FROM customer_access_reads").fetch_one(&mut *conn).await.unwrap();
        assert_eq!((count, reads), (0, 0));
        let mut tx = conn.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        PgCustomerPortal::new(&mut tx).record(&facts).await.unwrap();
        tx.commit().await.unwrap();
        let mut tx = conn.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT before_action_lock").execute(&mut *tx).await.unwrap();
        lock_portal_action_sources(&mut tx).await.unwrap();
        assert!(read_portal_action_source(&mut tx,"missing").await.unwrap().is_none());
        let witness=read_portal_action_source(&mut tx,"action-1").await.unwrap().unwrap();
        assert_eq!((witness.facts().company_id.as_str(),witness.facts().binding_id.as_str(),witness.facts().order_id.as_str(),witness.facts().customer_id.as_str(),witness.facts().user_id.as_str(),witness.facts().action.as_str(),witness.facts().source_version), ("company","binding","order","customer","user","request_support",3));
        witness.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT before_action_lock").execute(&mut *tx).await.unwrap();
        assert!(witness.require_current(&mut tx).await.is_err());
        let saved = PgCustomerPortal::new(&mut tx).replay("request-key").await.unwrap().unwrap();
        assert_eq!((saved.id.as_str(), saved.action.as_str(), saved.status.as_str()), ("action-1", "request_support", "recorded"));
        tx.rollback().await.unwrap();
        let mut tx = conn.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_portal_action_sources(&mut tx).await.unwrap();
        assert!(witness.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();

        // The trigger models a deferred owner effect that changes the original
        // order after the portal action was recorded, on this same connection.
        sqlx::raw_sql("CREATE FUNCTION portal_test_deferred_source_change() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE commerce_orders SET version=version+1 WHERE id=NEW.order_id; RETURN NEW; END $$;\
          CREATE CONSTRAINT TRIGGER portal_test_deferred_source_change AFTER INSERT ON customer_portal_actions DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION portal_test_deferred_source_change();")
            .execute(&mut *conn).await.unwrap();
        let mut tx = conn.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let mut deferred = facts.clone();
        deferred.id = "action-2".into();
        deferred.key = "request-key-2".into();
        deferred.input.action = PortalAction::AcknowledgeDelivery;
        PgCustomerPortal::new(&mut tx).record(&deferred).await.unwrap();
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *tx).await.unwrap();
        let error = PgCustomerPortal::new(&mut tx).revalidate_source(&actor, &facts.binding, 500).await.unwrap_err();
        assert!(matches!(error, PgTransactionError::Business(error) if error.code == "CUSTOMER_PORTAL_BINDING_UNAVAILABLE"));
        tx.rollback().await.unwrap();
        let order_version: i64 = sqlx::query_scalar("SELECT version FROM commerce_orders WHERE id='order'").fetch_one(&mut *conn).await.unwrap();
        let action_count: i64 = sqlx::query_scalar("SELECT count(*) FROM customer_portal_actions WHERE request_key='request-key-2'").fetch_one(&mut *conn).await.unwrap();
        assert_eq!((order_version, action_count), (3, 0));
    }).await;
}
