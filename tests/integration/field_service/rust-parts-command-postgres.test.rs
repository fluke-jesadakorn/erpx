#[allow(
    dead_code,
    reason = "actual Account/Finance fixture is shared with other owner suites"
)]
#[path = "../../support/field_service/parts.rs"]
mod facts;
use erpx_server_shared::public::Clock;
use facts::*;

#[tokio::test]
async fn actual_reserve_uses_two_real_stock_versions_and_exact_order_support_inbox_images_with_one_history_replay()
 {
    account::with_system(|system|async move{
        let p=setup(system).await;let now=p.review.system.scenario.clock.now().get();
        let before:(i64,i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id='warehouse'AND product_id='product'").fetch_one(p.pool().raw()).await.unwrap();
        let raw=format!(r#"{{"action":"reserve-part","expectedVersion":1,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":{},"reason":"Synthetic genuine reservation"}}"#,now+3_600_000);
        let key="field-reserve-native-one";
        let first=p.execute("reserve-order",key,&raw).await.unwrap();assert!(!first.replayed);
        let after:(i64,i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id='warehouse'AND product_id='product'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(after,(before.0,before.1,before.2+2));
        let state:(String,i64)=sqlx::query_as("SELECT status,version::bigint FROM field_service_orders WHERE id='reserve-order'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(state,("scheduled".into(),2));
        let ticket:(i64,i64,String)=sqlx::query_as("SELECT version,service_order_version,status FROM support_tickets WHERE id='ticket-reserve-order'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(ticket,(2,2,"open".into()));
        let history:(String,serde_json::Value)=sqlx::query_as("SELECT action,evidence_json FROM support_ticket_events WHERE id=$1").bind(format!("ticket-service-event-service-order-event-{key}")).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(history.0,"service-reserve-part");assert_eq!(history.1,serde_json::json!({"serviceOrderId":"reserve-order","serviceOrderVersion":2,"serviceEventId":format!("service-order-event-{key}")}));
        let notice:(String,String,i64)=sqlx::query_as("SELECT record_type,message,record_version FROM staff_notifications WHERE id=$1").bind(format!("service-order-notice-{key}")).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(notice,("field_service_order".into(),"Field service order updated: review current next action".into(),2));
        assert_eq!(p.counts(key).await,(1,1,1,0,1));
        let replay=p.execute("reserve-order",key,&raw).await.unwrap();assert!(replay.replayed);assert_eq!(replay.result.as_str(),first.result.as_str());
        assert_eq!(p.counts(key).await,(1,1,1,0,1));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM inbox_events WHERE id=$1").bind(format!("legacy:service-order-notice-{key}")).fetch_one(p.pool().raw()).await.unwrap(),1);
    }).await;
}

#[tokio::test]
async fn actual_reserved_issue_without_finance_write_retains_original_allowance_trigger_version_and_distinct_money_clock()
 {
    account::with_system(|system|async move{
        let p=setup(system).await;let now=p.review.system.scenario.clock.now().get();let key="field-issue-native-one";
        let before:i64=sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse'AND product_id='product'").fetch_one(p.pool().raw()).await.unwrap();
        // Future-dated engagement remains eligible in the exact undated Service
        // rule, independently of Maintenance's current-UTC workforce predicate.
        let outcome=p.issue(key).await.unwrap();assert!(!outcome.replayed);
        let stock:(i64,i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id='warehouse'AND product_id='product'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(stock,(4000,800,before+2));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT consumed_milli FROM field_service_part_reservations WHERE id='reserved-part'").fetch_one(p.pool().raw()).await.unwrap(),1000);
        let journal=p.journal(key).await;
        let j:(String,String,String,i64)=sqlx::query_as("SELECT status,source_type,source_id,created_at FROM finance_journals WHERE id=$1").bind(&journal).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(j,("draft".into(),"field_service_parts".into(),format!("operational-part-{key}"),now+2));
        let times:(i64,i64)=sqlx::query_as("SELECT c.created_at,m.captured_at FROM operational_inventory_consumptions c JOIN money_transaction_snapshots m ON m.source_type='operational_inventory_consumption'AND m.source_id=c.id AND m.amount_kind='stock_cost' WHERE c.id=$1").bind(format!("operational-part-{key}")).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(times,(now+1,now+3));
        let event:(i64,serde_json::Value)=sqlx::query_as("SELECT created_at,details_json FROM field_service_order_events WHERE id=$1").bind(format!("service-order-event-{key}")).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(event.0,now);assert_eq!(event.1,serde_json::json!({"reservationId":"reserved-part","quantityMilli":1000,"consumptionId":format!("operational-part-{key}"),"costMinor":200,"journalId":journal}));
        assert_eq!(p.counts(key).await,(1,1,1,1,1));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM command_receipts WHERE operation='finance.journal.create'").fetch_one(p.pool().raw()).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT service_order_version FROM support_tickets WHERE id='ticket-issue-order'").fetch_one(p.pool().raw()).await.unwrap(),4);
    }).await;
}

#[tokio::test]
async fn independent_finance_reviewer_has_no_operational_hr_or_records_write_and_requires_real_original_parts_source()
 {
    account::with_system(|system|async move{
        let p=setup(system).await;p.issue("field-finance-source").await.unwrap();let journal=p.journal("field-finance-source").await;
        withdraw(&p,"finance").await;
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE p.user_id='fixture-b'").fetch_one(p.pool().raw()).await.unwrap(),0);
        // A blocked customer and an inactive operational worker do not become
        // new Finance reviewer requirements on this exact financial lineage.
        sqlx::raw_sql("UPDATE finance_parties SET status='blocked'WHERE id='customer';UPDATE hr_engagements SET status='suspended'WHERE id='field-engagement';INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at)VALUES('later-rate','company','THB','2026-01-09',4,'fixture-b',1)").execute(p.pool().raw()).await.unwrap();
        let result=p.review.review(&journal,"field-finance-post",post()).await.unwrap();assert!(matches!(result,erpx_server_finance::public::journal_review::JournalReviewResult::Posted{version:2,..}));
        let money:Vec<(String,String,String)>=sqlx::query_as("SELECT source_type,amount_units::text,usd_units::text FROM money_transaction_snapshots ORDER BY source_type").fetch_all(p.pool().raw()).await.unwrap();assert_eq!(money,vec![("finance_journal".into(),"200".into(),"50".into()),("operational_inventory_consumption".into(),"200".into(),"100".into())]);
        let iam:serde_json::Value=sqlx::query_scalar("SELECT authority_json::jsonb->'iam'FROM command_receipts WHERE command_id='field-finance-post'").fetch_one(p.pool().raw()).await.unwrap();assert!(!iam["permissions"].as_array().unwrap().iter().any(|p|p=="records.write"));
    }).await;
}

#[tokio::test]
async fn exact_history_replay_uses_current_iam_but_does_not_reapply_expired_state_or_inactive_settlement()
 {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let first = p.issue("field-history-source").await.unwrap();
        withdraw(&p, "service-parts-assets").await;
        let replay = p.issue("field-history-source").await.unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.result.as_str(), first.result.as_str());
        business(p.issue("field-new-inactive").await, "PACKAGE_INACTIVE");
        sqlx::query("UPDATE iam_roles SET status='archived'WHERE id='custom_field_parts'")
            .execute(p.pool().raw())
            .await
            .unwrap();
        assert!(p.issue("field-history-source").await.is_err());
        assert_eq!(p.counts("field-history-source").await, (1, 1, 1, 1, 1));
        assert_eq!(p.counts("field-new-inactive").await, (0, 0, 0, 0, 0));
    })
    .await;
}

#[tokio::test]
async fn original_quote_failure_precedes_late_reservation_and_stale_maximum_version_without_deleting_money_history()
 {
    account::with_system(|system|async move{
        let p=setup(system).await;
        sqlx::raw_sql("UPDATE finance_charts SET base_currency='EUR'WHERE id='chart';UPDATE finance_accounts SET currency='EUR'WHERE id IN('inventory','expense')").execute(p.pool().raw()).await.unwrap();
        let raw=r#"{"action":"issue-part","expectedVersion":9007199254740991,"reservationId":"reserved-part","quantityMilli":1000,"entryDate":"2026-01-10","reason":"Synthetic stale request with missing quote"}"#;
        business(p.execute("issue-order","field-max-no-quote",raw).await,"MANUAL_RATE_REQUIRED");assert_eq!(p.counts("field-max-no-quote").await,(0,0,0,0,0));
        sqlx::query("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at)VALUES('euro-rate','company','EUR','2026-01-01',1,'fixture-a',1)").execute(p.pool().raw()).await.unwrap();
        assert!(matches!(p.execute("issue-order","field-max-valid-quote",raw).await,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(ref e)) if e.code=="DATABASE_NUMBER_OUT_OF_RANGE"));assert_eq!(p.counts("field-max-valid-quote").await,(0,0,0,0,0));
    }).await;
}

#[tokio::test]
async fn deferred_outbox_stock_mutation_rolls_back_reservation_journal_money_order_support_and_all_receipt_effects()
 {
    account::with_system(|system|async move{
        let p=setup(system).await;
        sqlx::raw_sql("CREATE SEQUENCE field_final_attempt;CREATE FUNCTION field_final_fault()RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('field_final_attempt');UPDATE commerce_stock SET quantity_milli=quantity_milli+1 WHERE warehouse_id='warehouse'AND product_id='product';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER field_final_fault AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='field-final-fault')EXECUTE FUNCTION field_final_fault()").execute(p.pool().raw()).await.unwrap();
        assert!(p.issue("field-final-fault").await.is_err());
        assert!(sqlx::query_scalar::<_,bool>("SELECT is_called FROM field_final_attempt").fetch_one(p.pool().raw()).await.unwrap());
        assert_eq!(p.counts("field-final-fault").await,(0,0,0,0,0));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT consumed_milli FROM field_service_part_reservations WHERE id='reserved-part'").fetch_one(p.pool().raw()).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version::bigint FROM field_service_orders WHERE id='issue-order'").fetch_one(p.pool().raw()).await.unwrap(),3);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT service_order_version FROM support_tickets WHERE id='ticket-issue-order'").fetch_one(p.pool().raw()).await.unwrap(),3);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM finance_journals").fetch_one(p.pool().raw()).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM money_transaction_snapshots").fetch_one(p.pool().raw()).await.unwrap(),0);
    }).await;
}

#[tokio::test]
async fn raw_utf16_event_encoding_stays_after_real_quote_stock_and_receipt_failure_positions() {
    account::with_system(|system|async move{
        let p=setup(system).await;
        // Match Node's text-parameter replacement while keeping distinct raw
        // UTF-16 receipt/event JSON. This row remains an actual owner reservation.
        sqlx::query("UPDATE field_service_part_reservations SET id=$1 WHERE id='reserved-part'").bind("reserved-\u{fffd}").execute(p.pool().raw()).await.unwrap();
        let raw=r#"{"action":"issue-part","expectedVersion":3,"reservationId":"reserved-\ud800","quantityMilli":1000,"entryDate":"2026-01-10","reason":"Synthetic raw Unicode event evidence"}"#;
        sqlx::raw_sql("UPDATE finance_charts SET base_currency='EUR'WHERE id='chart';UPDATE finance_accounts SET currency='EUR'WHERE id IN('inventory','expense')").execute(p.pool().raw()).await.unwrap();
        business(p.execute("issue-order","field-raw-quote-first",raw).await,"MANUAL_RATE_REQUIRED");
        let no_stock=raw.replace("\"quantityMilli\":1000","\"quantityMilli\":6000");
        business(p.execute("issue-order","field-raw-stock-first",&no_stock).await,"PART_STOCK_UNAVAILABLE");
        sqlx::raw_sql("UPDATE finance_charts SET base_currency='THB'WHERE id='chart';UPDATE finance_accounts SET currency='THB'WHERE id IN('inventory','expense');CREATE FUNCTION field_receipt_fault()RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'SYNTHETIC_RECEIPT_FIRST' USING ERRCODE='23514';END $$;CREATE TRIGGER field_receipt_fault BEFORE INSERT ON command_receipts FOR EACH ROW WHEN(NEW.command_id='field-raw-receipt-first')EXECUTE FUNCTION field_receipt_fault()").execute(p.pool().raw()).await.unwrap();
        let receipt=p.execute("issue-order","field-raw-receipt-first",raw).await;
        match receipt{Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(e))=>assert_eq!(e.sql_state(),Some("23514")),_=>panic!("actual known-result receipt must fail before the raw event cast")}
        let encoded=p.execute("issue-order","field-raw-event-last",raw).await;
        match encoded{Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(e))=>assert_eq!(e.sql_state(),Some("22P02")),_=>panic!("raw original event JSON must reach actual PostgreSQL Unicode validation")}
        for key in["field-raw-quote-first","field-raw-stock-first","field-raw-receipt-first","field-raw-event-last"]{assert_eq!(p.counts(key).await,(0,0,0,0,0));}
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT consumed_milli FROM field_service_part_reservations WHERE id=$1").bind("reserved-\u{fffd}").fetch_one(p.pool().raw()).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM finance_journals").fetch_one(p.pool().raw()).await.unwrap(),0);
    }).await;
}

#[tokio::test]
async fn generic_reversal_of_a_genuine_posted_field_service_parts_source_remains_at_its_owner() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        p.issue("field-reverse-source").await.unwrap();
        let journal = p.journal("field-reverse-source").await;
        p.review
            .review(&journal, "field-post-full-finance", post())
            .await
            .unwrap();
        business(
            p.review
                .review(&journal, "field-reverse-source-deny", reverse())
                .await,
            "REVERSE_AT_SOURCE",
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM finance_journals WHERE id=$1")
                .bind(&journal)
                .fetch_one(p.pool().raw())
                .await
                .unwrap(),
            "posted"
        );
        assert_eq!(p.counts("field-reverse-source-deny").await, (0, 0, 0, 0, 0));
    })
    .await;
}

#[tokio::test]
async fn reserved_lot_and_serial_issues_retain_real_depletion_trigger_and_all_exact_cost_evidence()
{
    for tracking in ["lot", "serial"] {
        account::with_system(move |system|async move{
            let p=setup_tracking(system,tracking).await;
            let(lot,quantity,cost,stock_quantity,stock_value)=if tracking=="lot"{("lot-1",3000,600,2000,400)}else{("serial-0",1000,200,4000,800)};
            let before:i64=sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse'AND product_id='product'").fetch_one(p.pool().raw()).await.unwrap();
            let raw=format!(r#"{{"action":"issue-part","expectedVersion":3,"reservationId":"reserved-part","quantityMilli":{quantity},"entryDate":"2026-01-10","allocations":[{{"lotId":"{lot}","quantityMilli":{quantity},"balanceVersion":1}}],"reason":"Synthetic tracked reserved issue"}}"#);
            p.execute("issue-order","field-tracked-source",&raw).await.unwrap();
            let stock:(i64,i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id='warehouse'AND product_id='product'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(stock,(stock_quantity,stock_value,before+2));
            let physical:(String,i64)=sqlx::query_as("SELECT status,version FROM inventory_lots WHERE id=$1").bind(lot).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(physical,("depleted".into(),2));
            let balance:(i64,i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor,version FROM inventory_lot_balances WHERE lot_id=$1 AND warehouse_id='warehouse'").bind(lot).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(balance,(0,0,2));
            let evidence:(i64,i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor,source_balance_version FROM operational_inventory_consumption_lots WHERE consumption_id='operational-part-field-tracked-source'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(evidence,(quantity,cost,1));
            let movement:(String,i64,i64)=sqlx::query_as("SELECT source_type,quantity_milli,value_minor FROM inventory_lot_moves WHERE source_id='operational-part-field-tracked-source'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(movement,("field_service".into(),-quantity,-cost));
        }).await;
    }
}

#[tokio::test]
async fn late_original_reservation_expiry_and_remaining_assertions_follow_successful_quote_preparation()
 {
    for change in ["expiry", "remaining"] {
        account::with_system(move |system|async move{
            let p=setup(system).await;let now=p.review.system.scenario.clock.now().get();
            if change=="expiry"{sqlx::query("UPDATE field_service_part_reservations SET expires_at=$1 WHERE id='reserved-part'").bind(now-1).execute(p.pool().raw()).await.unwrap();}
            else{sqlx::query("UPDATE field_service_part_reservations SET released_milli=2500 WHERE id='reserved-part'").execute(p.pool().raw()).await.unwrap();}
            sqlx::raw_sql("UPDATE finance_charts SET base_currency='EUR'WHERE id='chart';UPDATE finance_accounts SET currency='EUR'WHERE id IN('inventory','expense')").execute(p.pool().raw()).await.unwrap();
            business(p.issue("field-reservation-quote-first").await,"MANUAL_RATE_REQUIRED");
            sqlx::query("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at)VALUES('euro-rate','company','EUR','2026-01-01',1,'fixture-a',1)").execute(p.pool().raw()).await.unwrap();
            business(p.issue("field-reservation-late-deny").await,"CONFLICT");
            assert_eq!(p.counts("field-reservation-quote-first").await,(0,0,0,0,0));assert_eq!(p.counts("field-reservation-late-deny").await,(0,0,0,0,0));
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM finance_journals").fetch_one(p.pool().raw()).await.unwrap(),0);
        }).await;
    }
}

#[tokio::test]
async fn guarded_administration_requires_actual_mutation_selection_before_wire_and_has_zero_effects_on_refusal()
 {
    use erpx_server_commercial::public::purchases::PackageEnforcementMode;
    use erpx_server_shared::public::{Clock, CompanyId, HoldingId};
    use erpx_service::field_service_parts::{
        FieldServicePartsAdministration, FieldServicePartsAdministrationError,
        FieldServicePartsConfiguration, FieldServicePartsEnvironment,
    };
    account::with_system(|system|async move {
        let p=setup(system).await;
        let guard=p.review.system.scenario.clock.clone();
        let make = |holding: &str,company: &str| {
            let config=FieldServicePartsConfiguration::new(FieldServicePartsEnvironment::Development,HoldingId::new(holding).unwrap(),CompanyId::new(company).unwrap(),PackageEnforcementMode::Enforced).unwrap();
            FieldServicePartsAdministration::new(p.review.authorizer.clone(),config,std::sync::Arc::new(WriteClock(std::sync::atomic::AtomicI64::new(guard.now().get()))),guard.clone())
        };
        let admin=make("h001","company");
        let mut no_expected=p.selection.clone(); no_expected.expected=None; no_expected.mutation=false;
        let missing=admin.execute("acme.example.test",p.review.maker.clone(),no_expected,"reserve-order".into(),b"{".to_vec(),"guarded-missing".into()).await;
        assert!(matches!(missing,Err(FieldServicePartsAdministrationError::Transaction(erpx_server_runtime::public::postgres::PgTransactionError::Business(ref e))) if e.code=="WORK_CONTEXT_CHANGED"));
        let mut stale=p.selection.clone();stale.expected.as_mut().unwrap().revision-=1;stale.mutation=false;
        let stale=admin.execute("acme.example.test",p.review.maker.clone(),stale,"reserve-order".into(),b"{".to_vec(),"guarded-stale".into()).await;
        assert!(matches!(stale,Err(FieldServicePartsAdministrationError::Transaction(erpx_server_runtime::public::postgres::PgTransactionError::Business(ref e))) if e.code=="WORK_CONTEXT_CHANGED"));
        let invalid=admin.execute("acme.example.test",p.review.maker.clone(),p.selection.clone(),"reserve-order".into(),b"{".to_vec(),"guarded-invalid".into()).await;
        assert!(matches!(invalid,Err(FieldServicePartsAdministrationError::Wire(ref e)) if e.invalid_json));
        let schema=admin.execute("acme.example.test",p.review.maker.clone(),p.selection.clone(),"reserve-order".into(),b"{}".to_vec(),"guarded-schema".into()).await;
        assert!(matches!(schema,Err(FieldServicePartsAdministrationError::Wire(ref e)) if !e.invalid_json && !e.issues.is_empty()));
        assert!(FieldServicePartsConfiguration::new(FieldServicePartsEnvironment::Production,HoldingId::new("h001").unwrap(),CompanyId::new("company").unwrap(),PackageEnforcementMode::Enforced).is_err());
        let wrong_holding=make("other","company").execute("acme.example.test",p.review.maker.clone(),p.selection.clone(),"reserve-order".into(),b"{".to_vec(),"guarded-holding".into()).await;
        assert!(matches!(wrong_holding,Err(FieldServicePartsAdministrationError::Transaction(erpx_server_runtime::public::postgres::PgTransactionError::Business(ref e))) if e.code=="FIELD_SERVICE_CONFIGURATION_SCOPE_CHANGED"));
        let wrong_company=make("h001","other").execute("acme.example.test",p.review.maker.clone(),p.selection.clone(),"reserve-order".into(),b"{".to_vec(),"guarded-company".into()).await;
        assert!(matches!(wrong_company,Err(FieldServicePartsAdministrationError::Transaction(erpx_server_runtime::public::postgres::PgTransactionError::Business(ref e))) if e.code=="FIELD_SERVICE_CONFIGURATION_SCOPE_CHANGED"));
        for key in ["guarded-missing","guarded-stale","guarded-invalid","guarded-schema","guarded-holding","guarded-company"] { assert_eq!(p.counts(key).await,(0,0,0,0,0)); }
        let mut selected=p.selection.clone();selected.mutation=false;
        let result=admin.execute("acme.example.test",p.review.maker.clone(),selected,"reserve-order".into(),br#"{"action":"reserve-part","expectedVersion":1,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":2000000000000,"reason":"Synthetic guarded reserve"}"#.to_vec(),"guarded-valid".into()).await;
        assert!(result.is_ok());
    }).await;
}

#[tokio::test]
async fn reserve_raw_warehouse_product_references_reach_original_cas_and_scoped_stock() {
    for scenario in [
        "stale-nul",
        "missing-long",
        "long-control",
        "foreign-warehouse",
        "foreign-product",
    ] {
        account::with_system(move|system|async move {
            let p=setup(system).await; let now=p.review.system.scenario.clock.now().get();
            let warehouse=match scenario {"stale-nul"=>"missing\0warehouse".to_owned(),"missing-long"=>format!("missing-{}","x".repeat(220)),"long-control"=>format!("warehouse-{}\n","x".repeat(220)),"foreign-warehouse"=>"foreign-warehouse".into(),_=>"warehouse".into()};
            let product=if scenario=="long-control" {format!("product-{}\t","y".repeat(220))} else if scenario=="foreign-product" {"foreign-product".into()} else {"product".into()};
            if scenario=="long-control" {
                sqlx::query("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES($1,'company','branch','RAW-LONG','Synthetic raw warehouse')").bind(&warehouse).execute(p.pool().raw()).await.unwrap();
                sqlx::query("INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode)VALUES($1,'company','RAW-LONG','Synthetic raw product',100,'none')").bind(&product).execute(p.pool().raw()).await.unwrap();
                sqlx::query("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor)VALUES($1,$2,5000,1000)").bind(&warehouse).bind(&product).execute(p.pool().raw()).await.unwrap();
            } else if scenario=="foreign-warehouse" {
                sqlx::query("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('foreign-warehouse','foreign','branch','FOREIGN','Synthetic foreign')").execute(p.pool().raw()).await.unwrap();
                sqlx::query("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor)VALUES('foreign-warehouse','product',5000,1000)").execute(p.pool().raw()).await.unwrap();
            } else if scenario=="foreign-product" {
                sqlx::query("INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode)VALUES('foreign-product','foreign','FOREIGN','Synthetic foreign product',100,'none')").execute(p.pool().raw()).await.unwrap();
                sqlx::query("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor)VALUES('warehouse','foreign-product',5000,1000)").execute(p.pool().raw()).await.unwrap();
            }
            let raw=serde_json::json!({"action":"reserve-part","expectedVersion":if scenario=="stale-nul"{2}else{1},"warehouseId":warehouse,"productId":product,"quantityMilli":1000,"expiresAt":now+3600000,"reason":"Synthetic raw reserve"}).to_string();
            let key=format!("raw-reserve-{scenario}"); let result=p.execute("reserve-order",&key,&raw).await;
            if ["long-control","foreign-product"].contains(&scenario) {
                result.unwrap();assert_eq!(p.counts(&key).await,(1,1,1,0,1));
                let stock:(i64,i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2").bind(&warehouse).bind(&product).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(stock,(5000,1000,3));
                let details:serde_json::Value=sqlx::query_scalar("SELECT details_json FROM field_service_order_events WHERE id=$1").bind(format!("service-order-event-{key}")).fetch_one(p.pool().raw()).await.unwrap();
                assert_eq!(details["warehouseId"],warehouse);assert_eq!(details["productId"],product);
            } else { business(result,"CONFLICT");assert_eq!(p.counts(&key).await,(0,0,0,0,0)); }
        }).await;
    }
}

#[tokio::test]
async fn issue_raw_long_reservation_reference_uses_original_owner_lookup() {
    account::with_system(|system|async move {
        let p=setup(system).await; let reservation=format!("reserved-{}\n","x".repeat(220));
        sqlx::query("UPDATE field_service_part_reservations SET id=$1 WHERE id='reserved-part'").bind(&reservation).execute(p.pool().raw()).await.unwrap();
        let raw=serde_json::json!({"action":"issue-part","expectedVersion":3,"reservationId":reservation,"quantityMilli":1000,"entryDate":"2026-01-10","allocations":[],"reason":"Synthetic raw issue"}).to_string();
        let result=p.execute("issue-order","raw-long-reservation",&raw).await;result.unwrap();
        let details:serde_json::Value=sqlx::query_scalar("SELECT details_json FROM field_service_order_events WHERE id='service-order-event-raw-long-reservation'").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(details["reservationId"],reservation);assert_eq!(p.counts("raw-long-reservation").await,(1,1,1,1,1));
    }).await;
}

#[tokio::test]
async fn reserve_lone_surrogate_uses_replacement_lookup_but_preserves_original_details_for_storage()
{
    account::with_system(|system|async move {
        let p=setup(system).await;let now=p.review.system.scenario.clock.now().get();
        sqlx::query("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('warehouse-�','company','branch','SURROGATE','Synthetic replacement')").execute(p.pool().raw()).await.unwrap();
        sqlx::query("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor)VALUES('warehouse-�','product',5000,1000)").execute(p.pool().raw()).await.unwrap();
        let raw=format!(r#"{{"action":"reserve-part","expectedVersion":1,"warehouseId":"warehouse-\ud800","productId":"product","quantityMilli":1000,"expiresAt":{},"reason":"Synthetic surrogate"}}"#,now+3600000);
        let result=p.execute("reserve-order","raw-surrogate",&raw).await;
        assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(_))));
        assert_eq!(p.counts("raw-surrogate").await,(0,0,0,0,0));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse-�' AND product_id='product'").fetch_one(p.pool().raw()).await.unwrap(),1);
    }).await;
}

#[tokio::test]
async fn eligible_reviewer_can_issue_for_raw_long_stored_technician() {
    account::with_system(|system|async move {
        let p=setup(system).await;let technician=format!("technician-{}\n","x".repeat(220));
        sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_field_parts',2,'[\"records.read\",\"records.write\",\"records.review\",\"module.field_service.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic',0);UPDATE iam_roles SET published_revision=2 WHERE id='custom_field_parts'").execute(p.pool().raw()).await.unwrap();
        assign_historical_technician(&p,&technician,&["issue-order"]).await;
        let issue=r#"{"action":"issue-part","expectedVersion":4,"reservationId":"reserved-part","quantityMilli":1000,"entryDate":"2026-01-10","reason":"Synthetic long technician"}"#;
        p.execute("issue-order","long-technician",issue).await.unwrap();
        let actual:(String,String,String)=sqlx::query_as("SELECT o.technician_id,n.user_id,t.owner_id FROM field_service_orders o JOIN staff_notifications n ON n.record_id=o.id JOIN support_tickets t ON t.id=o.ticket_id WHERE o.id='issue-order'").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(actual,(technician.clone(),technician.clone(),technician));
        assert_eq!(p.counts("long-technician").await,(1,1,1,1,1));
    }).await;
}

#[tokio::test]
async fn parts_nul_reason_cannot_preempt_original_stale_order_cas_and_current_storage_rolls_back() {
    for action in ["reserve-part", "issue-part"] {
        for stale in [true, false] {
            account::with_system(move|system|async move {
                let p=setup(system).await;let now=p.review.system.scenario.clock.now().get();
                let value=if action=="reserve-part" {serde_json::json!({"action":action,"expectedVersion":if stale{2}else{1},"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":now+3600000,"reason":"Synthetic\u{0}reason"})} else {serde_json::json!({"action":action,"expectedVersion":if stale{4}else{3},"reservationId":"reserved-part","quantityMilli":1000,"entryDate":"2026-01-10","allocations":[],"reason":"Synthetic\u{0}reason"})};
                let key=format!("nul-{action}-{stale}");let order=if action=="reserve-part"{"reserve-order"}else{"issue-order"};let result=p.execute(order,&key,&value.to_string()).await;
                if stale {business(result,"CONFLICT");} else {assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(_))));}
                assert_eq!(p.counts(&key).await,(0,0,0,0,0));
                assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM finance_journals").fetch_one(p.pool().raw()).await.unwrap(),0);
            }).await;
        }
    }
}

#[tokio::test]
async fn missing_raw_reservation_reaches_lookup_and_nul_reaches_actual_database_encoding() {
    for nul in [false, true] {
        account::with_system(move|system|async move {
            let p=setup(system).await;
            let reservation=if nul {"missing\0reservation".to_owned()} else {format!("missing-{}","x".repeat(220))};
            let raw=serde_json::json!({"action":"issue-part","expectedVersion":3,"reservationId":reservation,"quantityMilli":1000,"entryDate":"2026-01-10","reason":"Synthetic missing reservation"}).to_string();
            let key=if nul{"raw-nul-reservation"}else{"raw-missing-reservation"};let result=p.execute("issue-order",key,&raw).await;
            if nul {assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(_))));} else {business(result,"FIELD_SERVICE_PART_NOT_FOUND");}
            assert_eq!(p.counts(key).await,(0,0,0,0,0));
        }).await;
    }
}

#[tokio::test]
async fn two_reservation_requests_observe_one_order_transition_and_exact_key_replay() {
    for same_key in [false, true] {
        account::with_system(move|system|async move {
            let p=setup(system).await;let now=p.review.system.scenario.clock.now().get();
            let raw=serde_json::json!({"action":"reserve-part","expectedVersion":1,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":now+3600000,"reason":"Synthetic concurrent reservation"}).to_string();
            let before:i64=sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(p.pool().raw()).await.unwrap();
            let (left,right)=tokio::join!(p.execute("reserve-order","concurrent-service-a",&raw),p.execute("reserve-order",if same_key{"concurrent-service-a"}else{"concurrent-service-b"},&raw));
            if same_key {let left=left.unwrap();let right=right.unwrap();assert_ne!(left.replayed,right.replayed);assert_eq!(left.result,right.result);} else {
                match (left,right) {
                    (Ok(ok),Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)))|(Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)),Ok(ok))=>{assert!(!ok.replayed);assert_eq!(e.code,"CONFLICT");},
                    _=>panic!("expected one committed reservation and one order CAS conflict"),
                }
            }
            let rows:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT version FROM field_service_orders WHERE id='reserve-order'),(SELECT count(*) FROM field_service_part_reservations WHERE order_id='reserve-order'),(SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(rows,(2,1,before+2));
        }).await;
    }
}

#[tokio::test]
async fn field_service_numeric_boundary_preserves_ordinary_stale_against_synthetic_maximum_source()
{
    for action in ["reserve-part", "issue-part"] {
        account::with_system(move|system|async move {
            let p=setup(system).await;let now=p.review.system.scenario.clock.now().get();let order=if action=="reserve-part"{"reserve-order"}else{"issue-order"};
            // Synthetic bounded state for failure ordering, not business history.
            let mut fixture=p.pool().raw().begin().await.unwrap();
            sqlx::query("UPDATE field_service_orders SET version=9007199254740991 WHERE id=$1").bind(order).execute(&mut *fixture).await.unwrap();
            sqlx::query("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at) SELECT 'synthetic-max-event',id,version,'dispatch','fixture-a','Synthetic maximum source',status,technician_id,scheduled_at,'{}'::jsonb,updated_at FROM field_service_orders WHERE id=$1").bind(order).execute(&mut *fixture).await.unwrap();
            fixture.commit().await.unwrap();
            let raw=if action=="reserve-part" {serde_json::json!({"action":action,"expectedVersion":1,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":now+3600000,"reason":"Synthetic stale"})} else {serde_json::json!({"action":action,"expectedVersion":1,"reservationId":"reserved-part","quantityMilli":1000,"entryDate":"2026-01-10","reason":"Synthetic stale"})};
            let key=format!("max-source-{action}");business(p.execute(order,&key,&raw.to_string()).await,"CONFLICT");assert_eq!(p.counts(&key).await,(0,0,0,0,0));
        }).await;
    }
}

#[tokio::test]
async fn field_service_maximum_successor_refuses_before_nul_reason_encoding() {
    for action in ["reserve-part", "issue-part"] {
        account::with_system(move|system|async move {
            let p=setup(system).await;let now=p.review.system.scenario.clock.now().get();let order=if action=="reserve-part"{"reserve-order"}else{"issue-order"};
            let raw=if action=="reserve-part" {serde_json::json!({"action":action,"expectedVersion":9007199254740991_i64,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":now+3600000,"reason":"Synthetic\u{0}reason"})} else {serde_json::json!({"action":action,"expectedVersion":9007199254740991_i64,"reservationId":"reserved-part","quantityMilli":1000,"entryDate":"2026-01-10","reason":"Synthetic\u{0}reason"})};
            let key=format!("max-nul-{action}");let result=p.execute(order,&key,&raw.to_string()).await;
            assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(ref e)) if e.code=="DATABASE_NUMBER_OUT_OF_RANGE"));assert_eq!(p.counts(&key).await,(0,0,0,0,0));
        }).await;
    }
}

#[tokio::test]
async fn maximum_command_key_preserves_full_derived_event_and_support_history_identifiers() {
    account::with_system(|system|async move {
        let p=setup(system).await;let now=p.review.system.scenario.clock.now().get();let key="k".repeat(180);
        let raw=serde_json::json!({"action":"reserve-part","expectedVersion":1,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":now+3600000,"reason":"Synthetic maximum key"}).to_string();
        p.execute("reserve-order",&key,&raw).await.unwrap();
        let event=format!("service-order-event-{key}");let history=format!("ticket-service-event-{event}");assert_eq!(event.len(),200);assert_eq!(history.len(),221);
        let actual:(String,String)=sqlx::query_as("SELECT e.id,h.id FROM field_service_order_events e JOIN support_ticket_events h ON h.id=$2 WHERE e.id=$1").bind(&event).bind(&history).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(actual,(event,history));
        assert_eq!(p.counts(&key).await,(1,1,1,0,1));
    }).await;
}

#[tokio::test]
async fn synthetic_long_historical_lineage_supports_independent_finance_review() {
    account::with_system(|system|async move {
        let p=setup(system).await;p.issue("field-history-seed").await.unwrap();let seed_journal=p.journal("field-history-seed").await;
        // Insert a complete synthetic historical graph with its final raw identities;
        // it proves factual reading, not that a native producer issued these values.
        let order=format!("historical-order-{}\n","o".repeat(220));let creator=format!("historical-creator-{}\t","u".repeat(220));
        let key="field-historical-raw";let consumption=format!("operational-part-{key}");let journal="journal-historical-raw";let reservation="historical-reservation";let event=format!("service-order-event-{key}");
        let mut tx=p.pool().raw().begin().await.unwrap();
        sqlx::query("INSERT INTO support_tickets SELECT (jsonb_populate_record(NULL::support_tickets,to_jsonb(t)||jsonb_build_object('id','ticket-historical-raw','source_kind','general','customer_id',NULL,'order_id',NULL,'service_agreement_id',NULL,'service_order_id',NULL,'service_order_version',NULL))).* FROM support_tickets t WHERE id='ticket-issue-order'").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO field_service_orders SELECT (jsonb_populate_record(NULL::field_service_orders,to_jsonb(o)||jsonb_build_object('id',$1::text,'ticket_id','ticket-historical-raw'))).* FROM field_service_orders o WHERE id='issue-order'").bind(&order).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO field_service_part_reservations SELECT (jsonb_populate_record(NULL::field_service_part_reservations,to_jsonb(r)||jsonb_build_object('id',$1::text,'order_id',$2::text))).* FROM field_service_part_reservations r WHERE id='reserved-part'").bind(reservation).bind(&order).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO field_service_order_events SELECT (jsonb_populate_record(NULL::field_service_order_events,to_jsonb(e)||jsonb_build_object('id',$1::text,'order_id',$2::text,'actor_id',$3::text,'details_json',e.details_json||jsonb_build_object('reservationId',$4::text,'consumptionId',$5::text,'journalId',$6::text)))).* FROM field_service_order_events e WHERE id='service-order-event-field-history-seed'").bind(&event).bind(&order).bind(&creator).bind(reservation).bind(&consumption).bind(journal).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO finance_journals SELECT (jsonb_populate_record(NULL::finance_journals,to_jsonb(j)||jsonb_build_object('id',$1::text,'journal_number','SYNTHETIC-RAW-HISTORY','source_id',$2::text))).* FROM finance_journals j WHERE id=$3").bind(journal).bind(&consumption).bind(&seed_journal).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO finance_journal_lines SELECT (jsonb_populate_record(NULL::finance_journal_lines,to_jsonb(l)||jsonb_build_object('id','historical-line-'||l.ordinal::text,'journal_id',$1::text))).* FROM finance_journal_lines l WHERE journal_id=$2").bind(journal).bind(&seed_journal).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO money_transaction_snapshots SELECT (jsonb_populate_record(NULL::money_transaction_snapshots,to_jsonb(m)||jsonb_build_object('source_id',$1::text))).* FROM money_transaction_snapshots m WHERE source_type='operational_inventory_consumption' AND source_id='operational-part-field-history-seed'").bind(&consumption).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO operational_inventory_consumptions SELECT (jsonb_populate_record(NULL::operational_inventory_consumptions,to_jsonb(c)||jsonb_build_object('id',$1::text,'source_id',$2::text,'journal_id',$3::text,'created_by',$4::text))).* FROM operational_inventory_consumptions c WHERE id='operational-part-field-history-seed'").bind(&consumption).bind(&order).bind(journal).bind(&creator).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO command_receipts SELECT (jsonb_populate_record(NULL::command_receipts,to_jsonb(r)||jsonb_build_object('command_id',$1::text,'record_id',$2::text,'actor_id',$3::text))).* FROM command_receipts r WHERE command_id='field-history-seed'").bind(key).bind(&order).bind(&creator).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        let result=p.review.review(journal,"field-historical-raw-post",post()).await.unwrap();assert!(matches!(result,erpx_server_finance::public::journal_review::JournalReviewResult::Posted{version:2,..}));
    }).await;
}

#[tokio::test]
async fn field_service_reservation_waits_for_real_stock_withdrawal_before_first_snapshot() {
    account::with_system(|system|async move {
        let p=std::sync::Arc::new(setup(system).await);let now=p.review.system.scenario.clock.now().get();
        let before:i64=sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(p.pool().raw()).await.unwrap();
        let mut writer=p.pool().raw().begin().await.unwrap();
        sqlx::query("UPDATE commerce_stock SET quantity_milli=3000,value_minor=600,version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'").execute(&mut *writer).await.unwrap();
        let other=p.clone();let raw=serde_json::json!({"action":"reserve-part","expectedVersion":1,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":now+3600000,"reason":"Synthetic source withdrawal"}).to_string();
        let task=tokio::spawn(async move {other.execute("reserve-order","field-stock-withdrawal",&raw).await});
        let waiting=tokio::time::timeout(std::time::Duration::from_secs(3),async {loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE NOT granted AND mode='ShareRowExclusiveLock' AND relation='commerce_stock'::regclass AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(p.pool().raw()).await.unwrap();
            if waiting {break;} tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }}).await.is_ok();
        writer.commit().await.unwrap();let result=task.await.unwrap();assert!(waiting,"actual stock writer must block the source fence");business(result,"CONFLICT");
        assert_eq!(p.counts("field-stock-withdrawal").await,(0,0,0,0,0));
        let stock:(i64,i64)=sqlx::query_as("SELECT quantity_milli,version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(stock,(3000,before+1));
        let reservation:(i64,i64)=sqlx::query_as("SELECT quantity_milli,consumed_milli FROM field_service_part_reservations WHERE id='reserved-part'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(reservation,(3000,0));
    }).await;
}

#[tokio::test]
async fn raw_long_stock_references_reserve_then_issue_with_tracking_and_independent_finance() {
    for tracking in ["none", "lot", "serial"] {
        account::with_system(move|system|async move {
            let p=setup(system).await;let now=p.review.system.scenario.clock.now().get();
            let warehouse=format!("raw-warehouse-{}\n","w".repeat(220));let product=format!("raw-product-{}\t","p".repeat(220));
            sqlx::query("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES($1,'company','branch','RAW-ISSUE','Synthetic raw stock')").bind(&warehouse).execute(p.pool().raw()).await.unwrap();
            sqlx::query("INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode)VALUES($1,'company','RAW-ISSUE','Synthetic raw stock',100,$2)").bind(&product).bind(tracking).execute(p.pool().raw()).await.unwrap();
            let mut stock_fixture=p.pool().raw().begin().await.unwrap();
            sqlx::query("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor)VALUES($1,$2,1000,200)").bind(&warehouse).bind(&product).execute(&mut *stock_fixture).await.unwrap();
            if tracking!="none" {
                sqlx::query("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,serial_number,created_by,created_at)VALUES('raw-lot','company',$1,$2,$3,$4,'fixture-a',0)").bind(&product).bind(tracking).bind((tracking=="lot").then_some("RAW-LOT")).bind((tracking=="serial").then_some("RAW-SERIAL")).execute(&mut *stock_fixture).await.unwrap();
                sqlx::query("INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor)VALUES('raw-lot',$1,1000,200)").bind(&warehouse).execute(&mut *stock_fixture).await.unwrap();
            }
            stock_fixture.commit().await.unwrap();
            let reserve=serde_json::json!({"action":"reserve-part","expectedVersion":1,"warehouseId":warehouse,"productId":product,"quantityMilli":1000,"expiresAt":now+3600000,"reason":"Synthetic raw reserve"}).to_string();
            p.execute("reserve-order","raw-chain-reserve",&reserve).await.unwrap();
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2").bind(&warehouse).bind(&product).fetch_one(p.pool().raw()).await.unwrap(),3);
            // Synthetic retained start transition: this batch does not claim to migrate start.
            let mut tx=p.pool().raw().begin().await.unwrap();
            sqlx::query("UPDATE field_service_orders SET status='in_visit',visit_started_at=$1,version=3,updated_at=$1 WHERE id='reserve-order'").bind(now+10).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at) SELECT 'synthetic-raw-start',id,3,'start','fixture-a','Synthetic retained start','in_visit',technician_id,scheduled_at,'{}'::jsonb,$1 FROM field_service_orders WHERE id='reserve-order'").bind(now+10).execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
            let allocations=if tracking=="none" {serde_json::json!([])} else {serde_json::json!([{"lotId":"raw-lot","quantityMilli":1000,"balanceVersion":1}])};
            let issue=serde_json::json!({"action":"issue-part","expectedVersion":3,"reservationId":"field-part-raw-chain-reserve","quantityMilli":1000,"entryDate":"2026-01-10","allocations":allocations,"reason":"Synthetic raw issue"}).to_string();
            p.execute("reserve-order","raw-chain-issue",&issue).await.unwrap();
            let stock:(i64,i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2").bind(&warehouse).bind(&product).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(stock,(0,0,5));
            let actual:(String,String)=sqlx::query_as("SELECT warehouse_id,product_id FROM operational_inventory_consumptions WHERE id='operational-part-raw-chain-issue'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(actual,(warehouse.clone(),product.clone()));
            if tracking!="none" {let lot:(String,i64,i64)=sqlx::query_as("SELECT l.status,b.quantity_milli,b.value_minor FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id WHERE l.id='raw-lot'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(lot,("depleted".into(),0,0));}
            let journal=p.journal("raw-chain-issue").await;let result=p.review.review(&journal,"raw-chain-finance",post()).await.unwrap();assert!(matches!(result,erpx_server_finance::public::journal_review::JournalReviewResult::Posted{version:2,..}));
        }).await;
    }
}

#[tokio::test]
async fn raw_issue_planning_preserves_active_and_company_scope_stock_refusals() {
    for scenario in ["warehouse-inactive", "product-inactive", "product-company"] {
        account::with_system(move |system| async move {
            let p = setup(system).await;
            match scenario {
                "warehouse-inactive" => sqlx::query(
                    "UPDATE commerce_warehouses SET status='archived' WHERE id='warehouse'",
                )
                .execute(p.pool().raw())
                .await
                .unwrap(),
                "product-inactive" => {
                    sqlx::query("UPDATE commerce_products SET status='archived' WHERE id='product'")
                        .execute(p.pool().raw())
                        .await
                        .unwrap()
                }
                _ => sqlx::query(
                    "UPDATE commerce_products SET company_id='foreign' WHERE id='product'",
                )
                .execute(p.pool().raw())
                .await
                .unwrap(),
            };
            let key = format!("raw-issue-{scenario}");
            business(p.issue(&key).await, "PART_STOCK_UNAVAILABLE");
            assert_eq!(p.counts(&key).await, (0, 0, 0, 0, 0));
        })
        .await;
    }
}

#[tokio::test]
async fn eligible_control_heavy_technician_notification_images_and_late_tamper_are_exact() {
    for tamper in [false, true] {
        account::with_system(move|system|async move {
            let p=setup(system).await;let now=p.review.system.scenario.clock.now().get();
            // Fits factual TEXT/index storage while JSON escaping alone exceeds 8192 bytes.
            let technician=format!("target-{}-end","\u{1}".repeat(1500));
            sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_field_parts',2,'[\"records.read\",\"records.write\",\"records.review\",\"module.field_service.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic',0);UPDATE iam_roles SET published_revision=2 WHERE id='custom_field_parts'").execute(p.pool().raw()).await.unwrap();
            sqlx::query("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at)VALUES('long-tech-person',$1,'company','Synthetic eligible worker','TH','fixture-a',0,0)").bind(&technician).execute(p.pool().raw()).await.unwrap();
            sqlx::query("INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('long-tech-engagement','long-tech-person','company','employee','salaried','active','2100-01-01','Synthetic','THB','fixture-a',0,0)").execute(p.pool().raw()).await.unwrap();
            sqlx::query("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy)VALUES('company',$1,$2,'long-tech-person','active','local')").bind(&technician).bind(format!("local:{technician}")).execute(p.pool().raw()).await.unwrap();
            sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES('long-tech-grant',$1,'custom_field_parts','[\"field_service\"]','company',$2,$3,1,'holding')").bind(&technician).bind(now-1000).bind(now+3600000).execute(p.pool().raw()).await.unwrap();
            assign_historical_technician(&p,&technician,&["reserve-order","issue-order"]).await;
            let reserve=serde_json::json!({"action":"reserve-part","expectedVersion":2,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":now+3600000,"reason":"Synthetic eligible worker"}).to_string();
            p.execute("reserve-order","control-tech-reserve",&reserve).await.unwrap();
            let (actual,bytes):(String,i64)=sqlx::query_as("SELECT user_id,octet_length(to_jsonb(n)::text)::bigint FROM staff_notifications n WHERE id='service-order-notice-control-tech-reserve'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(actual,technician);assert!(bytes>8192);
            if tamper {
                sqlx::raw_sql("CREATE SEQUENCE control_notice_fault_seen;CREATE FUNCTION control_notice_fault()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN PERFORM nextval('control_notice_fault_seen');UPDATE inbox_deliveries SET read_at=123 WHERE id='service-order-notice-control-tech-issue';RETURN NEW;END$$;CREATE CONSTRAINT TRIGGER control_notice_fault AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='control-tech-issue') EXECUTE FUNCTION control_notice_fault()").execute(p.pool().raw()).await.unwrap();
            }
            let issue=r#"{"action":"issue-part","expectedVersion":4,"reservationId":"reserved-part","quantityMilli":1000,"entryDate":"2026-01-10","reason":"Synthetic control technician"}"#;
            let result=p.execute("issue-order","control-tech-issue",issue).await;
            if tamper {
                business(result,"FIELD_SERVICE_NOTIFICATION_SOURCE_CHANGED");assert!(sqlx::query_scalar::<_,bool>("SELECT is_called FROM control_notice_fault_seen").fetch_one(p.pool().raw()).await.unwrap());assert_eq!(p.counts("control-tech-issue").await,(0,0,0,0,0));
                let state:(i64,i64)=sqlx::query_as("SELECT (SELECT version FROM field_service_orders WHERE id='issue-order'),(SELECT consumed_milli FROM field_service_part_reservations WHERE id='reserved-part')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(state,(4,0));
            } else {
                result.unwrap();assert_eq!(p.counts("control-tech-issue").await,(1,1,1,1,1));
                let actual:String=sqlx::query_scalar("SELECT user_id FROM inbox_deliveries WHERE id='service-order-notice-control-tech-issue'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(actual,technician);
            }
        }).await;
    }
}

#[tokio::test]
async fn current_read_and_write_denials_precede_invalid_body_without_effects() {
    use erpx_server_commercial::public::purchases::PackageEnforcementMode;
    use erpx_server_shared::public::{CompanyId, HoldingId};
    use erpx_service::field_service_parts::{
        FieldServicePartsAdministration, FieldServicePartsAdministrationError,
        FieldServicePartsConfiguration, FieldServicePartsEnvironment,
    };
    for missing in ["read", "write"] {
        account::with_system(move|system|async move {
            let p=setup(system).await;let guard=p.review.system.scenario.clock.clone();
            let permissions=if missing=="read"{r#"["records.write","module.field_service.read"]"#}else{r#"["records.read","module.field_service.read"]"#};
            sqlx::query("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_field_parts',2,$1::jsonb,'[]','[\"holding\",\"branch\",\"department\"]','synthetic',0)").bind(permissions).execute(p.pool().raw()).await.unwrap();
            sqlx::query("UPDATE iam_roles SET published_revision=2 WHERE id='custom_field_parts'").execute(p.pool().raw()).await.unwrap();
            let config=FieldServicePartsConfiguration::new(FieldServicePartsEnvironment::Development,HoldingId::new("h001").unwrap(),CompanyId::new("company").unwrap(),PackageEnforcementMode::Enforced).unwrap();
            let admin=FieldServicePartsAdministration::new(p.review.authorizer.clone(),config,std::sync::Arc::new(WriteClock(std::sync::atomic::AtomicI64::new(guard.now().get()))),guard);
            let key=format!("missing-{missing}-invalid");let result=admin.execute("acme.example.test",p.review.maker.clone(),p.selection.clone(),"reserve-order".into(),b"{".to_vec(),key.clone()).await;
            assert!(matches!(result,Err(FieldServicePartsAdministrationError::Transaction(erpx_server_runtime::public::postgres::PgTransactionError::Business(ref e))) if e.code=="PERMISSION_REQUIRED"));
            assert_eq!(p.counts(&key).await,(0,0,0,0,0));
        }).await;
    }
}

#[tokio::test]
async fn long_raw_lot_and_more_than_500_other_balances_preserve_physical_depletion_rule() {
    account::with_system(|system|async move {
        let p=setup_tracking(system,"lot").await;
        let lot=format!("raw-lot-{}\n","l".repeat(220));let code=format!("lot-code-{}","z".repeat(220));
        // Keep each warehouse's stock and lot-control totals consistent at commit.
        let mut fixture=p.pool().raw().begin().await.unwrap();
        sqlx::query("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at)VALUES($1,'company','product','lot',$2,'fixture-a',0)").bind(&lot).bind(&code).execute(&mut *fixture).await.unwrap();
        sqlx::query("UPDATE inventory_lot_balances SET quantity_milli=quantity_milli-1000,value_minor=value_minor-200 WHERE lot_id='lot-1' AND warehouse_id='warehouse'").execute(&mut *fixture).await.unwrap();
        sqlx::query("INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor)VALUES($1,'warehouse',1000,200)").bind(&lot).execute(&mut *fixture).await.unwrap();
        sqlx::query("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) SELECT 'other-'||n,'company','branch','OTHER-'||n,'Synthetic other' FROM generate_series(1,501) n").execute(&mut *fixture).await.unwrap();
        sqlx::query("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor)VALUES('other-501','product',1,1)").execute(&mut *fixture).await.unwrap();
        sqlx::query("INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) SELECT $1,'other-'||n,CASE WHEN n=501 THEN 1 ELSE 0 END,CASE WHEN n=501 THEN 1 ELSE 0 END FROM generate_series(1,501) n").bind(&lot).execute(&mut *fixture).await.unwrap();
        fixture.commit().await.unwrap();
        let raw=serde_json::json!({"action":"issue-part","expectedVersion":3,"reservationId":"reserved-part","quantityMilli":1000,"entryDate":"2026-01-10","allocations":[{"lotId":lot,"quantityMilli":1000,"balanceVersion":1}],"reason":"Synthetic raw lot"}).to_string();
        p.execute("issue-order","field-long-lot",&raw).await.unwrap();
        let actual:(String,i64,i64)=sqlx::query_as("SELECT l.status,b.quantity_milli,b.value_minor FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id WHERE l.id=$1 AND b.warehouse_id='warehouse'").bind(&lot).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(actual,("active".into(),0,0));
        let evidence:(String,i64,i64)=sqlx::query_as("SELECT lot_id,quantity_milli,value_minor FROM operational_inventory_consumption_lots WHERE consumption_id='operational-part-field-long-lot'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(evidence,(lot,1000,200));
        assert_eq!(p.counts("field-long-lot").await,(1,1,1,1,1));
        let journal=p.journal("field-long-lot").await;
        let result=p.review.review(&journal,"field-long-lot-finance",post()).await.unwrap();
        assert!(matches!(result,erpx_server_finance::public::journal_review::JournalReviewResult::Posted{version:2,..}));
    }).await;
}

#[tokio::test]
async fn raw_issue_retains_quality_quantity_lot_and_future_movement_guards_after_quote() {
    for scenario in [
        "whole",
        "partial",
        "blocked-lot",
        "future-movement",
        "quote-first",
    ] {
        account::with_system(move|system|async move {
            let p=if scenario=="blocked-lot"{setup_tracking(system,"lot").await}else{setup(system).await};
            if scenario=="future-movement" {
                sqlx::raw_sql("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,created_by,prepared_by,created_at,updated_at)VALUES('field-future-count','company','branch','warehouse','FIELD-FUTURE','2026-01-11','Synthetic future','fixture-a','fixture-a',0,0);INSERT INTO inventory_adjustment_moves(id,count_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at)VALUES('field-future-move','field-future-count','warehouse','product',0,0,'2026-01-11',0)").execute(p.pool().raw()).await.unwrap();
            } else {
                let scope=if scenario=="whole"||scenario=="quote-first"{"whole"}else{"partial"};let lot=if scenario=="blocked-lot"{Some("lot-1")}else{None};let qty=if scenario=="partial"{Some(4500_i64)}else if scenario=="blocked-lot"{Some(1000)}else{None};
                sqlx::query("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at,quantity_scope,lot_id,held_quantity_milli)VALUES('field-hold','company','branch','warehouse','product','Synthetic hold','2026-12-31','fixture-a',0,0,$1,$2,$3)").bind(scope).bind(lot).bind(qty).execute(p.pool().raw()).await.unwrap();
            }
            if scenario=="quote-first" {sqlx::raw_sql("UPDATE finance_charts SET base_currency='EUR' WHERE id='chart';UPDATE finance_accounts SET currency='EUR' WHERE id IN ('inventory','expense')").execute(p.pool().raw()).await.unwrap();}
            let allocations=if scenario=="blocked-lot"{serde_json::json!([{"lotId":"lot-1","quantityMilli":1000,"balanceVersion":1}])}else{serde_json::json!([])};
            let raw=serde_json::json!({"action":"issue-part","expectedVersion":3,"reservationId":"reserved-part","quantityMilli":1000,"entryDate":"2026-01-10","allocations":allocations,"reason":"Synthetic quality guard"}).to_string();
            let key=format!("field-quality-{scenario}");let result=p.execute("issue-order",&key,&raw).await;
            business(result,if scenario=="quote-first"{"MANUAL_RATE_REQUIRED"}else{"CONFLICT"});assert_eq!(p.counts(&key).await,(0,0,0,0,0));
        }).await;
    }
}

#[tokio::test]
async fn independent_finance_review_rejects_missing_or_wrong_company_receipt_lineage() {
    for scenario in ["missing", "foreign-company"] {
        account::with_system(move|system|async move {
            let p=setup(system).await;p.issue("field-lineage-source").await.unwrap();let journal=p.journal("field-lineage-source").await;
            if scenario=="missing" {sqlx::query("DELETE FROM command_receipts WHERE command_id='field-lineage-source'").execute(p.pool().raw()).await.unwrap();}
            else {sqlx::query("UPDATE command_receipts SET authority_json=jsonb_set(authority_json::jsonb,'{companyId}','\"foreign\"'::jsonb)::text WHERE command_id='field-lineage-source'").execute(p.pool().raw()).await.unwrap();}
            let key=format!("field-lineage-deny-{scenario}");business(p.review.review(&journal,&key,post()).await,"FINANCE_FIELD_SERVICE_SOURCE_UNAVAILABLE");
            assert_eq!(p.counts(&key).await,(0,0,0,0,0));assert_eq!(sqlx::query_scalar::<_,String>("SELECT status FROM finance_journals WHERE id=$1").bind(&journal).fetch_one(p.pool().raw()).await.unwrap(),"draft");
        }).await;
    }
}

#[tokio::test]
async fn deferred_owner_finance_inbox_and_lot_faults_are_observed_and_atomically_rolled_back() {
    for scenario in [
        "order",
        "event",
        "support",
        "journal",
        "stock-cost",
        "inbox",
        "lot",
        "other-balance",
    ] {
        account::with_system(move|system|async move {
            let tracked=["lot","other-balance"].contains(&scenario);
            let p=if tracked {setup_tracking(system,"lot").await}else{setup(system).await};
            if scenario=="other-balance" {
                sqlx::raw_sql("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('fault-other','company','branch','FAULT-OTHER','Synthetic other');INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor)VALUES('lot-1','fault-other',0,0)").execute(p.pool().raw()).await.unwrap();
            }
            let body=match scenario {
                "order"=>"UPDATE field_service_orders SET site='UNINTENDED' WHERE id='issue-order'",
                "event"=>"UPDATE field_service_order_events SET reason='UNINTENDED' WHERE id='service-order-event-field-final-matrix'",
                "support"=>"INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at) SELECT 'field-fault-event',id,version+1,'dispatch','fixture-a','Synthetic fault',status,technician_id,scheduled_at,'{}'::jsonb,updated_at FROM field_service_orders WHERE id='issue-order';PERFORM set_config('erpx.service_case_projection','field-fault-event',true);UPDATE support_tickets SET title='UNINTENDED',version=version+1,service_order_version=service_order_version+1 WHERE id='ticket-issue-order'",
                "journal"=>"UPDATE finance_journals SET memo='UNINTENDED' WHERE source_id='operational-part-field-final-matrix'",
                "stock-cost"=>"UPDATE money_transaction_snapshots SET amount_units=amount_units+1 WHERE source_type='operational_inventory_consumption' AND source_id='operational-part-field-final-matrix' AND amount_kind='stock_cost'",
                "inbox"=>"UPDATE inbox_deliveries SET read_at=123 WHERE id='service-order-notice-field-final-matrix'",
                "lot"=>"UPDATE inventory_lots SET lot_code='UNINTENDED' WHERE id='lot-1'",
                _=>"UPDATE inventory_lot_balances SET version=version+1 WHERE lot_id='lot-1' AND warehouse_id='fault-other'",
            };
            let sql=format!("CREATE SEQUENCE field_matrix_seen;CREATE FUNCTION field_matrix_fault() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN PERFORM nextval('field_matrix_seen');{body};RETURN NEW;END$$;CREATE CONSTRAINT TRIGGER field_matrix_fault AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='field-final-matrix') EXECUTE FUNCTION field_matrix_fault()");// `body` is selected solely from the fixed literal match above.
            sqlx::raw_sql(sqlx::AssertSqlSafe(sql)).execute(p.pool().raw()).await.unwrap();
            let raw=if tracked {r#"{"action":"issue-part","expectedVersion":3,"reservationId":"reserved-part","quantityMilli":1000,"entryDate":"2026-01-10","allocations":[{"lotId":"lot-1","quantityMilli":1000,"balanceVersion":1}],"reason":"Synthetic deferred fault"}"#}else{r#"{"action":"issue-part","expectedVersion":3,"reservationId":"reserved-part","quantityMilli":1000,"entryDate":"2026-01-10","reason":"Synthetic deferred fault"}"#};
            let result=p.execute("issue-order","field-final-matrix",raw).await;
            assert!(sqlx::query_scalar::<_,bool>("SELECT is_called FROM field_matrix_seen").fetch_one(p.pool().raw()).await.unwrap(),"fault was not reached: {scenario}");
            match scenario {
                "event"=>assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(ref e)) if e.sql_state()==Some("23514")),"{scenario}"),
                "stock-cost"=>assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(ref e)) if e.sql_state()==Some("23514")),"{scenario}"),
                _=>business(result,match scenario {"order"=>"FIELD_SERVICE_SOURCE_CHANGED","support"=>"SUPPORT_SERVICE_SOURCE_CHANGED","journal"=>"PARTS_DRAFT_SOURCE_CHANGED","inbox"=>"FIELD_SERVICE_NOTIFICATION_SOURCE_CHANGED",_=>"INVENTORY_CONSUMPTION_SOURCE_CHANGED"}),
            }
            assert_eq!(p.counts("field-final-matrix").await,(0,0,0,0,0));
            let state:(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT version FROM field_service_orders WHERE id='issue-order'),(SELECT consumed_milli FROM field_service_part_reservations WHERE id='reserved-part'),(SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),(SELECT service_order_version FROM support_tickets WHERE id='ticket-issue-order'),(SELECT count(*) FROM finance_journals),(SELECT count(*) FROM money_transaction_snapshots)").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(state,(3,0,5000,3,0,0));
            let remaining:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_order_events WHERE id IN ('service-order-event-field-final-matrix','field-fault-event')),(SELECT count(*) FROM support_ticket_events WHERE id='ticket-service-event-service-order-event-field-final-matrix'),(SELECT count(*) FROM inbox_events WHERE id='legacy:service-order-notice-field-final-matrix'),(SELECT count(*) FROM inbox_deliveries WHERE id='service-order-notice-field-final-matrix')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(remaining,(0,0,0,0));
            if tracked {let lot:(String,i64,i64,i64)=sqlx::query_as("SELECT l.status,b.quantity_milli,b.value_minor,b.version FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id WHERE l.id='lot-1' AND b.warehouse_id='warehouse'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(lot,("active".into(),3000,600,1));}
            if scenario=="other-balance" {assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version FROM inventory_lot_balances WHERE lot_id='lot-1' AND warehouse_id='fault-other'").fetch_one(p.pool().raw()).await.unwrap(),1);}

        }).await;
    }
}
