#[allow(
    dead_code,
    reason = "The retained Account fixture also serves other native owners"
)]
#[path = "../../support/rust-effective-authority.rs"]
mod auth;
#[allow(
    dead_code,
    reason = "Shared actual Maintenance source facts also serve the source-only component"
)]
#[path = "../../support/maintenance/parts-facts.rs"]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use auth::*;
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError};
use erpx_server_shared::public::{BusinessError, IdentifierSource, RecordId};
use erpx_service::maintenance_parts::{
    MaintenancePartsSourcePreparation, execute_maintenance_issue_part,
};
use facts::*;
struct Ids(u64);
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        self.0 += 1;
        RecordId::new(format!("synthetic-maintenance-event-{}", self.0)).unwrap()
    }
}
async fn counts(pool: &DatabasePool) -> (i64, i64, i64, i64, i64, i64, i64, i64) {
    sqlx::query_as("SELECT(SELECT count(*)FROM command_receipts),(SELECT count(*)FROM audit_events),(SELECT count(*)FROM outbox_events),(SELECT count(*)FROM finance_journals),(SELECT count(*)FROM operational_inventory_consumptions),(SELECT count(*)FROM staff_notifications),(SELECT count(*)FROM inbox_events),(SELECT count(*)FROM inbox_deliveries)").fetch_one(pool.raw()).await.unwrap()
}
fn denied<T>(result: Result<T, PgTransactionError<BusinessError>>, code: &str) {
    if code == "DATABASE_NUMBER_OUT_OF_RANGE" {
        assert!(
            matches!(result,Err(PgTransactionError::Persistence(ref e))if e.code==code),
            "expected persistence {code}"
        );
    } else {
        assert!(
            matches!(result,Err(PgTransactionError::Business(ref e))if e.code==code),
            "expected business {code}"
        );
    }
}
#[tokio::test]
async fn real_native_workflow_posts_one_owner_receipt_and_draft_parts_with_notification_inbox_and_exact_replay()
 {
    fixture::with_database(2,|control|async move {fixture::with_database(2,move |holding|async move {
        let(authorizer,_)=setup(&control,&holding).await;seed_work(&holding).await;seed_parts(&holding).await;
        authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance_request(),MaintenancePartsSourcePreparation,|c,current|Box::pin(async move {
            assert!(!current.has_permission("finance.write"));
            let command=issue_command();let write=PartsWriteClock(std::sync::atomic::AtomicI64::new(NOW));let guard=PartsGuardClock;let mut ids=Ids(0);
            let first=execute_maintenance_issue_part(c,current,&command,PackageEnforcementMode::Enforced,&write,&guard,&mut ids).await?;
            assert!(!first.replayed);assert_eq!(first.result.as_str(),r#"{"id":"work","version":2,"status":"in_progress"}"#);assert_eq!(ids.0,2);
            let second=execute_maintenance_issue_part(c,current,&command,PackageEnforcementMode::Enforced,&write,&guard,&mut ids).await?;
            assert!(second.replayed);assert_eq!(second.result,first.result);assert_eq!(ids.0,2);
            assert_eq!(write.0.load(std::sync::atomic::Ordering::SeqCst),NOW+5);
            Ok(())
        })).await.unwrap();
        assert_eq!(counts(&holding).await,(1,1,1,1,1,1,1,1));
        let outcome:(i64,i64,i64,String,i64,i64,i64,i64)=sqlx::query_as("SELECT s.quantity_milli,s.value_minor,w.version,j.status,c.created_at,j.created_at,m.captured_at,r.created_at FROM commerce_stock s CROSS JOIN maintenance_work_orders w CROSS JOIN operational_inventory_consumptions c JOIN finance_journals j ON j.id=c.journal_id JOIN money_transaction_snapshots m ON m.source_id=c.id AND m.source_type='operational_inventory_consumption' CROSS JOIN command_receipts r").fetch_one(holding.raw()).await.unwrap();
        assert_eq!(outcome,(4000,800,2,"draft".into(),NOW+1,NOW+2,NOW+3,NOW+4));
        let notice:(String,String,i64)=sqlx::query_as("SELECT user_id,message,record_version FROM staff_notifications").fetch_one(holding.raw()).await.unwrap();assert_eq!(notice,("user".into(),"Asset maintenance work updated: review current next action".into(),2));
    }).await;}).await;
}
#[tokio::test]
async fn full_workflow_keeps_stock_and_quote_refusals_before_maximum_successor_numeric_precompile()
{
    use erpx_server_shared::public::CommandKey;
    for kind in ["stock", "quote", "version", "version-nul"] {
        fixture::with_database(2,move |control|async move {fixture::with_database(2,move |holding|async move {
            let(authorizer,_)=setup(&control,&holding).await;seed_work(&holding).await;seed_parts_with_rate(&holding,kind!="quote").await;
            let result=authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance_request(),MaintenancePartsSourcePreparation,move |c,current|Box::pin(async move {
                let raw=format!(r#"{{"action":"issue-part","expectedVersion":9007199254740991,"reason":"Actual original maximum input{}","warehouseId":"warehouse","productId":"{}","quantityMilli":1000,"entryDate":"2026-01-10","allocations":[]}}"#,if kind=="version-nul" {"\\u0000"} else {""},if kind=="stock"{"missing-product"}else{"product"});
                let command=erpx_server_maintenance::public::work_wire::normalize_maintenance_change(raw.as_bytes()).unwrap().into_command(CommandKey::new("synthetic-max-request")?,work_id());
                execute_maintenance_issue_part(c,current,&command,PackageEnforcementMode::Enforced,&PartsWriteClock(std::sync::atomic::AtomicI64::new(NOW)),&PartsGuardClock,&mut Ids(0)).await
            })).await;
            denied(result,match kind {"stock"=>"PART_STOCK_UNAVAILABLE","quote"=>"MANUAL_RATE_REQUIRED",_=>"DATABASE_NUMBER_OUT_OF_RANGE"});
            assert_eq!(counts(&holding).await,(0,0,0,0,0,0,0,0));
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT quantity_milli FROM commerce_stock").fetch_one(holding.raw()).await.unwrap(),5000);
        }).await;}).await;
    }
}
#[tokio::test]
async fn deferred_outbox_trigger_cannot_rewrite_work_after_all_native_effects_and_leave_any_receipt_or_delivery()
 {
    fixture::with_database(2,|control|async move {fixture::with_database(2,move |holding|async move {
        let(authorizer,_)=setup(&control,&holding).await;seed_work(&holding).await;seed_parts(&holding).await;
        sqlx::raw_sql("CREATE FUNCTION synthetic_late_outbox_work()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN UPDATE maintenance_work_orders SET title='UNINTENDED' WHERE id=NEW.aggregate_id;RETURN NEW;END$$;CREATE CONSTRAINT TRIGGER synthetic_late_outbox_work AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_late_outbox_work()").execute(holding.raw()).await.unwrap();
        let result=authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance_request(),MaintenancePartsSourcePreparation,|c,current|Box::pin(async move {
            execute_maintenance_issue_part(c,current,&issue_command(),PackageEnforcementMode::Enforced,&PartsWriteClock(std::sync::atomic::AtomicI64::new(NOW)),&PartsGuardClock,&mut Ids(0)).await
        })).await;
        denied(result,"MAINTENANCE_STATE_CHANGED");assert_eq!(counts(&holding).await,(0,0,0,0,0,0,0,0));
        let unchanged:(i64,i64,bool)=sqlx::query_as("SELECT s.quantity_milli,w.version,w.title=repeat('PRIVATE_TITLE',200000) FROM commerce_stock s CROSS JOIN maintenance_work_orders w").fetch_one(holding.raw()).await.unwrap();assert_eq!(unchanged,(5000,1,true));
    }).await;}).await;
}

#[tokio::test]
async fn ordinary_stale_issue_part_against_synthetic_maximum_stored_version_refuses_at_work_cas() {
    fixture::with_database(2, |control| async move {
        fixture::with_database(2, move |holding| async move {
            let (authorizer, _) = setup(&control, &holding).await;
            seed_work(&holding).await;
            seed_parts(&holding).await;
            // Synthetic edge fixture, not evidence of a real Work history.
            sqlx::query(
                "UPDATE maintenance_work_orders SET version=9007199254740991 WHERE id='work'",
            )
            .execute(holding.raw())
            .await
            .unwrap();
            let result = authorizer
                .execute_with_sources(
                    "acme.example.test",
                    proof(),
                    input(),
                    maintenance_request(),
                    MaintenancePartsSourcePreparation,
                    |c, current| {
                        Box::pin(async move {
                            execute_maintenance_issue_part(
                                c,
                                current,
                                &issue_command(),
                                PackageEnforcementMode::Enforced,
                                &PartsWriteClock(std::sync::atomic::AtomicI64::new(NOW)),
                                &PartsGuardClock,
                                &mut Ids(1900),
                            )
                            .await
                        })
                    },
                )
                .await;
            denied(result, "CONFLICT");
            assert_eq!(counts(&holding).await, (0, 0, 0, 0, 0, 0, 0, 0));
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT version FROM maintenance_work_orders WHERE id='work'"
                )
                .fetch_one(holding.raw())
                .await
                .unwrap(),
                9_007_199_254_740_991
            );
        })
        .await;
    })
    .await;
}

#[tokio::test]
async fn issue_part_nul_reason_preserves_apply_guards_and_rolls_back_storage_failure() {
    use erpx_server_maintenance::public::work_wire::normalize_maintenance_change;
    use erpx_server_shared::public::CommandKey;
    for scenario in [
        "stale",
        "current",
        "future-movement",
        "missing-stock",
        "missing-quote",
    ] {
        fixture::with_database(2,move |control| async move { fixture::with_database(2,move |holding| async move {
            let(authorizer,_)=setup(&control,&holding).await; seed_work(&holding).await;
            seed_parts_with_rate(&holding,scenario!="missing-quote").await;
            if scenario=="future-movement" {
                // Same actual parent/movement pair as Inventory's planning regression.
                sqlx::raw_sql("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,created_by,prepared_by,created_at,updated_at)VALUES('future-count','company','branch','warehouse','SYNTHETIC-FUTURE','2026-01-11','Synthetic date source','maker','maker',0,0);INSERT INTO inventory_adjustment_moves(id,count_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at)VALUES('future-move','future-count','warehouse','product',0,0,'2026-01-11',0)").execute(holding.raw()).await.unwrap();
            }
            let result=authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance_request(),MaintenancePartsSourcePreparation,move|c,current|Box::pin(async move {
                let value=serde_json::json!({"action":"issue-part","expectedVersion":if scenario=="stale"{2}else{1},"reason":"Synthetic\u{0000}reason","warehouseId":"warehouse","productId":if scenario=="missing-stock"{"missing-product"}else{"product"},"quantityMilli":1000,"entryDate":"2026-01-10","allocations":[]});
                let command=normalize_maintenance_change(&serde_json::to_vec(&value).unwrap()).unwrap().into_command(CommandKey::new(format!("issue-nul-{scenario}"))?,work_id());
                execute_maintenance_issue_part(c,current,&command,PackageEnforcementMode::Enforced,&PartsWriteClock(std::sync::atomic::AtomicI64::new(NOW)),&PartsGuardClock,&mut Ids(2100)).await
            })).await;
            if scenario=="current" {
                assert!(matches!(result,Err(PgTransactionError::Persistence(ref e)) if e.code=="POSTGRES_OPERATION_FAILED"));
            } else {
                denied(result,match scenario {"missing-stock"=>"PART_STOCK_UNAVAILABLE","missing-quote"=>"MANUAL_RATE_REQUIRED",_=>"CONFLICT"});
            }
            assert_eq!(counts(&holding).await,(0,0,0,0,0,0,0,0));
            let retained:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT w.version,s.quantity_milli,s.value_minor,(SELECT count(*) FROM maintenance_work_events),(SELECT count(*) FROM money_transaction_snapshots) FROM maintenance_work_orders w CROSS JOIN commerce_stock s WHERE w.id='work'").fetch_one(holding.raw()).await.unwrap();
            assert_eq!(retained,(1,5000,1000,0,0));
        }).await; }).await;
    }
}
