#[path = "../../support/maintenance/parts-facts.rs"]
mod parts;
use parts::{
    PartsGuardClock, PartsWriteClock, issue_command, maintenance_request as maintenance,
    seed_parts as parts_fixture, seed_work as source, work_id as id,
};
#[allow(
    dead_code,
    reason = "Shared actual retained Account/Holding fixture also serves Identity tests"
)]
#[path = "../../support/rust-effective-authority.rs"]
mod auth;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use auth::*;
use erpx_server_maintenance::public::{MaintenanceTransition, work_source::*};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, RecordId, TimestampMillis};
use erpx_service::maintenance_parts::{
    MaintenancePartsSourcePreparation, PgMaintenancePartsSource, prepare_issue_part_inventory,
};
fn error<T>(result: Result<T, PgTransactionError<BusinessError>>, code: &str) {
    assert!(
        matches!(result,Err(PgTransactionError::Business(ref e))if e.code==code),
        "expected {code}"
    );
}
#[tokio::test]
async fn real_retained_work_reads_minimal_owner_source_and_preserves_policy_then_version_order() {
    fixture::with_database(2, |control| async move {
        fixture::with_database(2, move |holding| async move {
            let (authorizer, _) = setup(&control, &holding).await;
            source(&holding).await;
            authorizer
                .execute_with_sources(
                    "acme.example.test",
                    proof(),
                    input(),
                    maintenance(),
                    MaintenanceWorkSourcePreparation,
                    |c, current| {
                        Box::pin(async move {
                            let s = read_current_maintenance_work(
                                c,
                                current,
                                &id(),
                                TimestampMillis::new(NOW),
                            )
                            .await?
                            .unwrap();
                            assert_eq!(s.facts().id, id());
                            assert_eq!(s.facts().branch_id.as_str(), "branch");
                            assert!(!format!("{:?}", s.facts()).contains("PRIVATE_"));
                            s.require_action(&MaintenanceTransition::IssuePart)?;
                            s.require_expected_version(1)?;
                            assert_eq!(
                                s.require_expected_version(2).unwrap_err().code,
                                "MAINTENANCE_STATE_CHANGED"
                            );
                            s.require_current(c, TimestampMillis::new(NOW)).await?;
                            assert!(
                                read_current_maintenance_work(
                                    c,
                                    current,
                                    &RecordId::new("missing")?,
                                    TimestampMillis::new(NOW)
                                )
                                .await?
                                .is_none()
                            );
                            Ok(())
                        })
                    },
                )
                .await
                .unwrap();
            // An eligible independent reviewer may act for another owner, but only with
            // the selected current records.review AND that reviewer's People reference.
            sqlx::query(
                "UPDATE maintenance_work_orders SET owner_id='unlinked-owner'WHERE id='work'",
            )
            .execute(holding.raw())
            .await
            .unwrap();
            authorizer
                .execute_with_sources(
                    "acme.example.test",
                    proof(),
                    input(),
                    maintenance(),
                    MaintenanceWorkSourcePreparation,
                    |c, current| {
                        Box::pin(async move {
                            let s = read_current_maintenance_work(
                                c,
                                current,
                                &id(),
                                TimestampMillis::new(NOW),
                            )
                            .await?
                            .unwrap();
                            s.require_action(&MaintenanceTransition::IssuePart)?;
                            Ok(())
                        })
                    },
                )
                .await
                .unwrap();
        })
        .await;
    })
    .await;
}
#[tokio::test]
async fn current_worker_expiry_and_selected_review_permission_cannot_be_replaced_by_boolean_labels()
{
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
  let(authorizer,_)=setup(&control,&holding).await;source(&holding).await;
  sqlx::query("UPDATE maintenance_work_orders SET owner_id='unlinked-owner',status='open'WHERE id='work'").execute(holding.raw()).await.unwrap();
  sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_native_authority',3,'[\"records.write\",\"module.maintenance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);UPDATE iam_roles SET published_revision=3 WHERE id='custom_native_authority'").execute(holding.raw()).await.unwrap();
  error(authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance(),MaintenanceWorkSourcePreparation,|c,current|Box::pin(async move{let s=read_current_maintenance_work(c,current,&id(),TimestampMillis::new(NOW)).await?.unwrap();s.require_action(&MaintenanceTransition::IssuePart)?;Ok(())})).await,"MAINTENANCE_OWNER_REQUIRED");
  sqlx::raw_sql("UPDATE maintenance_work_orders SET owner_id='user',status='in_progress'WHERE id='work';UPDATE hr_engagements SET end_date='2026-01-02'WHERE id='engagement'").execute(holding.raw()).await.unwrap();
  error(authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance(),MaintenanceWorkSourcePreparation,|c,current|Box::pin(async move{let s=read_current_maintenance_work(c,current,&id(),TimestampMillis::new(NOW)).await?.unwrap();s.require_action(&MaintenanceTransition::IssuePart)?;Ok(())})).await,"MAINTENANCE_OWNER_REQUIRED");
 }).await;}).await;
}
#[tokio::test]
async fn final_source_check_refuses_local_mutation_backward_time_and_expired_retained_context() {
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
  let(authorizer,_)=setup(&control,&holding).await;source(&holding).await;
  authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance(),MaintenanceWorkSourcePreparation,move|c,current|{let holding=holding.clone();Box::pin(async move{
   let s=read_current_maintenance_work(c,current,&id(),TimestampMillis::new(NOW)).await?.unwrap();let mut other=holding.raw().begin().await?;sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await?;assert!(s.require_current(&mut other,TimestampMillis::new(NOW)).await.is_err());other.rollback().await?;assert!(s.require_current(c,TimestampMillis::new(NOW-1)).await.is_err());assert!(s.require_current(c,TimestampMillis::new(current.expires_at())).await.is_err());sqlx::query("UPDATE maintenance_work_orders SET version=version+1 WHERE id='work'").execute(&mut *c).await?;assert!(s.require_current(c,TimestampMillis::new(NOW)).await.is_err());Ok(())
  })}).await.unwrap();
 }).await;}).await;
}
#[tokio::test]
async fn actual_selected_branch_and_physical_asset_relationship_cannot_be_supplied_by_request() {
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
  let(authorizer,_)=setup(&control,&holding).await;source(&holding).await;
  sqlx::raw_sql("INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at)VALUES('foreign','company','other-branch','OTHER','Other','synthetic','THB','fixture',0,0),('foreign-2','company','other-branch','OTHER2','Other second','synthetic','THB','fixture',0,0);INSERT INTO maintenance_work_orders(id,company_id,branch_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at)VALUES('other-work','company','other-branch','foreign','Other','normal','in_progress','user','user',0,0),('mismatched','company','branch','foreign-2','Mismatch','normal','in_progress','user','user',0,0)").execute(holding.raw()).await.unwrap();
  let mut branch=input();branch.branch_slug=Some("north".into());
  authorizer.execute_with_sources("acme.example.test",proof(),branch.clone(),maintenance(),MaintenanceWorkSourcePreparation,|c,current|Box::pin(async move{assert!(read_current_maintenance_work(c,current,&RecordId::new("other-work")?,TimestampMillis::new(NOW)).await?.is_none());Ok(())})).await.unwrap();
  error(authorizer.execute_with_sources("acme.example.test",proof(),branch,maintenance(),MaintenanceWorkSourcePreparation,|c,current|Box::pin(async move{let _=read_current_maintenance_work(c,current,&RecordId::new("mismatched")?,TimestampMillis::new(NOW)).await?;Ok(())})).await,"MAINTENANCE_STATE_CHANGED");
 }).await;}).await;
}

#[tokio::test]
async fn actual_work_effects_retain_original_authority_and_exact_predetermined_work_event_images() {
    fixture::with_database(2,|control|async move {fixture::with_database(2,move |holding|async move {
        let(authorizer,_)=setup(&control,&holding).await;source(&holding).await;
        authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance(),MaintenanceWorkSourcePreparation,|c,current|Box::pin(async move {
            let source=read_current_maintenance_work(c,current,&id(),TimestampMillis::new(NOW)).await?.unwrap();
            let command=issue_command();
            let effects=source.retain_issue_part_effects(c,&command,TimestampMillis::new(NOW),TimestampMillis::new(NOW)).await?;
            effects.require_before(c,TimestampMillis::new(NOW)).await?;
            effects.write(c,TimestampMillis::new(NOW)).await?;
            // Old Work reference deliberately remains stale, never rebaselined.
            assert!(source.require_current(c,TimestampMillis::new(NOW)).await.is_err());
            sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *c).await?;
            effects.require_after(c,TimestampMillis::new(NOW)).await
        })).await.unwrap();
        let work:(i64,String)=sqlx::query_as("SELECT version,status FROM maintenance_work_orders WHERE id='work'").fetch_one(holding.raw()).await.unwrap();assert_eq!(work,(2,"in_progress".into()));
        let event:(String,String,i64)=sqlx::query_as("SELECT actor_id,reason,version FROM maintenance_work_events WHERE work_order_id='work'").fetch_one(holding.raw()).await.unwrap();assert_eq!(event,("user".into(),"Synthetic issue evidence".into(),2));
    }).await;}).await;
}
#[tokio::test]
async fn work_after_image_refuses_immediate_and_deferred_unintended_title_rewrites_atomically() {
    for deferred in [false, true] {
        fixture::with_database(2,move |control|async move {fixture::with_database(2,move |holding|async move {
            let(authorizer,_)=setup(&control,&holding).await;source(&holding).await;
            let trigger=if deferred {
                "CREATE FUNCTION synthetic_work_rewrite()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN UPDATE maintenance_work_orders SET title='UNINTENDED' WHERE id=NEW.work_order_id;RETURN NEW;END$$;CREATE CONSTRAINT TRIGGER synthetic_work_rewrite AFTER INSERT ON maintenance_work_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_work_rewrite()"
            }else {
                "CREATE FUNCTION synthetic_work_rewrite()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN NEW.title:='UNINTENDED';RETURN NEW;END$$;CREATE TRIGGER synthetic_work_rewrite BEFORE UPDATE ON maintenance_work_orders FOR EACH ROW EXECUTE FUNCTION synthetic_work_rewrite()"
            };
            sqlx::raw_sql(trigger).execute(holding.raw()).await.unwrap();
            let result=authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance(),MaintenanceWorkSourcePreparation,|c,current|Box::pin(async move {
                let source=read_current_maintenance_work(c,current,&id(),TimestampMillis::new(NOW)).await?.unwrap();let command=issue_command();
                let effects=source.retain_issue_part_effects(c,&command,TimestampMillis::new(NOW),TimestampMillis::new(NOW)).await?;
                effects.write(c,TimestampMillis::new(NOW)).await?;
                sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *c).await?;
                effects.require_after(c,TimestampMillis::new(NOW)).await
            })).await;
            error(result,"MAINTENANCE_STATE_CHANGED");
            let source:(i64,bool)=sqlx::query_as("SELECT version,title=repeat('PRIVATE_TITLE',200000) FROM maintenance_work_orders WHERE id='work'").fetch_one(holding.raw()).await.unwrap();assert_eq!(source,(1,true));
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM maintenance_work_events").fetch_one(holding.raw()).await.unwrap(),0);
        }).await;}).await;
    }
}

#[tokio::test]
async fn actual_maintenance_inventory_finance_purchase_composition_keeps_draft_and_original_write_clocks()
 {
    use erpx_server_commercial::public::purchases::PackageEnforcementMode;
    use erpx_server_finance::public::parts_draft::prepare_owned_parts_draft;
    use erpx_server_shared::public::Clock;
    fixture::with_database(2,|control|async move {fixture::with_database(2,move |holding|async move {
        let(authorizer,_)=setup(&control,&holding).await;source(&holding).await;parts_fixture(&holding).await;
        let receipt=authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance(),MaintenancePartsSourcePreparation,|c,current|Box::pin(async move {
            assert!(!current.has_permission("finance.write"));
            let source=read_current_maintenance_work(c,current,&id(),TimestampMillis::new(NOW)).await?.unwrap();
            let command=issue_command();let write_clock=PartsWriteClock(std::sync::atomic::AtomicI64::new(NOW));let guard_clock=PartsGuardClock;
            source.require_action(&MaintenanceTransition::IssuePart)?;
            let application_at=write_clock.now();
            let inventory=prepare_issue_part_inventory(c, &source,&command,&write_clock,&guard_clock).await?;
            let work=source.retain_issue_part_effects(c,&command,application_at,guard_clock.now()).await?.into_plan();
            let owner=PgMaintenancePartsSource::retain(c,&source,&work,&inventory,PackageEnforcementMode::Enforced,guard_clock.now()).await?;
            let finance=prepare_owned_parts_draft(c,&owner,&write_clock,&guard_clock).await?;
            assert_eq!(finance.expected().consumption_created_at(),NOW+1);
            assert_eq!(finance.expected().journal_created_at(),NOW+2);
            let finance=finance.persist_journal(c,&guard_clock).await?;
            inventory.write_consumption_and_lots(c).await?;
            let finance=finance.persist_stock_cost(c).await?;
            inventory.write_stock(c).await?;
            work.write(c,&source,guard_clock.now()).await?;
            sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *c).await?;
            let receipt=finance.into_finance_receipt(c,&guard_clock).await?;
            receipt.require_current(c).await?;
            // A detached receipt keeps predetermined rows, never a new baseline.
            sqlx::query("SAVEPOINT synthetic_finance_after_image").execute(&mut *c).await?;
            sqlx::query("UPDATE finance_journals SET memo='Unintended after extraction'").execute(&mut *c).await?;
            assert!(matches!(receipt.require_current(c).await,Err(PgTransactionError::Business(ref e))if e.code=="PARTS_DRAFT_SOURCE_CHANGED"));
            sqlx::query("ROLLBACK TO SAVEPOINT synthetic_finance_after_image").execute(&mut *c).await?;
            receipt.require_current(c).await?;
            inventory.require_inventory_after(c).await?;
            work.require_after(c,&source,guard_clock.now()).await?;
            drop(owner);
            let work_receipt=work.into_retention(c,&source,guard_clock.now()).await?;
            work_receipt.require_after(c,&source,guard_clock.now()).await?;
            let timestamps:(i64,i64,i64)=sqlx::query_as("SELECT c.created_at,j.created_at,m.captured_at FROM operational_inventory_consumptions c JOIN finance_journals j ON j.id=c.journal_id JOIN money_transaction_snapshots m ON m.source_type='operational_inventory_consumption'AND m.source_id=c.id").fetch_one(&mut *c).await?;
            assert_eq!(timestamps,(NOW+1,NOW+2,NOW+3));
            Ok(receipt)
        })).await.unwrap();
        let committed:(i64,i64,i64,i64,String,String)=sqlx::query_as("SELECT s.quantity_milli,s.value_minor,s.version,w.version,j.status,j.source_type FROM commerce_stock s CROSS JOIN maintenance_work_orders w CROSS JOIN finance_journals j WHERE w.id='work'").fetch_one(holding.raw()).await.unwrap();
        assert_eq!(committed,(4000,800,2,2,"draft".into(),"maintenance_parts".into()));
        let receipt=std::sync::Arc::new(receipt);
        let detached=erpx_server_runtime::public::postgres::PgUnitOfWork::new(holding.clone()).run::<(),BusinessError,_>(erpx_server_runtime::public::postgres::TransactionOptions::default(),move |c| {
            let receipt=receipt.clone();Box::pin(async move {receipt.require_current(c).await})
        }).await;
        error(detached,"PARTS_DRAFT_SOURCE_CHANGED");
        // This is still component assembly: no fictitious second Finance receipt,
        // native notification, outer Workflow receipt/audit/outbox or HTTP cutover.
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM command_receipts").fetch_one(holding.raw()).await.unwrap(),0);
    }).await;}).await;
}
#[tokio::test]
async fn maximum_safe_requested_version_does_not_move_stock_or_quote_denial_before_original_late_cas()
 {
    use erpx_server_commercial::public::purchases::PackageEnforcementMode;
    use erpx_server_finance::public::parts_draft::prepare_owned_parts_draft;
    use erpx_server_shared::public::{Clock, CommandKey};
    for kind in ["stock", "quote", "version"] {
        fixture::with_database(2,move |control|async move {fixture::with_database(2,move |holding|async move {
            let(authorizer,_)=setup(&control,&holding).await;source(&holding).await;parts::seed_parts_with_rate(&holding,kind!="quote").await;
            let result=authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance(),MaintenancePartsSourcePreparation,move |c,current|Box::pin(async move {
                let source=read_current_maintenance_work(c,current,&id(),TimestampMillis::new(NOW)).await?.unwrap();
                let raw=format!(r#"{{"action":"issue-part","expectedVersion":9007199254740991,"reason":"Maximum safe original input","warehouseId":"warehouse","productId":"{}","quantityMilli":1000,"entryDate":"2026-01-10","allocations":[]}}"#,if kind=="stock"{"missing-product"}else{"product"});
                let command=erpx_server_maintenance::public::work_wire::normalize_maintenance_change(raw.as_bytes()).unwrap().into_command(CommandKey::new("synthetic-max-version")?,id());
                let write_clock=PartsWriteClock(std::sync::atomic::AtomicI64::new(NOW));let guard_clock=PartsGuardClock;let at=write_clock.now();
                let inventory=prepare_issue_part_inventory(c, &source,&command,&write_clock,&guard_clock).await?;
                let work=source.retain_issue_part_effects(c,&command,at,guard_clock.now()).await?.into_plan();
                assert_eq!(work.version_after(),9_007_199_254_740_992);
                let owner=PgMaintenancePartsSource::retain(c,&source,&work,&inventory,PackageEnforcementMode::Enforced,guard_clock.now()).await?;
                let finance=prepare_owned_parts_draft(c,&owner,&write_clock,&guard_clock).await?;
                finance.persist_journal(c,&guard_clock).await?;
                Ok(())
            })).await;
            error(result,match kind {"stock"=>"PART_STOCK_UNAVAILABLE","quote"=>"MANUAL_RATE_REQUIRED",_=>"MAINTENANCE_STATE_CHANGED"});
            let effects:(i64,i64,i64)=sqlx::query_as("SELECT(SELECT count(*)FROM finance_journals),(SELECT count(*)FROM operational_inventory_consumptions),(SELECT count(*)FROM maintenance_work_events)").fetch_one(holding.raw()).await.unwrap();assert_eq!(effects,(0,0,0));
        }).await;}).await;
    }
}

fn simple_command(
    action: &str,
    version: i64,
) -> erpx_server_maintenance::public::work_commands::MaintenanceWorkCommand {
    let bytes=serde_json::to_vec(&serde_json::json!({"action":action,"expectedVersion":version,"reason":"Synthetic transition evidence"})).unwrap();
    erpx_server_maintenance::public::work_wire::normalize_maintenance_change(&bytes)
        .unwrap()
        .into_command(
            erpx_server_shared::public::CommandKey::new(format!("synthetic-{action}-effects"))
                .unwrap(),
            id(),
        )
}
#[tokio::test]
async fn actual_start_block_resume_effects_keep_work_and_physical_receipts_separate() {
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
  let(authorizer,_)=setup(&control,&holding).await;source(&holding).await;
  sqlx::query("UPDATE maintenance_work_orders SET status='open'WHERE id='work'").execute(holding.raw()).await.unwrap();
  for(action,version,status)in [("start",1,"in_progress"),("block",2,"blocked"),("resume",3,"in_progress")]{
   authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance(),MaintenanceWorkSourcePreparation,move|c,current|Box::pin(async move{
    let source=read_current_maintenance_work(c,current,&id(),TimestampMillis::new(NOW)).await?.unwrap();let command=simple_command(action,version);let at=TimestampMillis::new(NOW+version);
    let plan=source.retain_simple_transition(c,&command,at,at).await?;
    let physical=match plan.physical_change(){Some(change)=>Some(source.physical_asset_source().prepare_physical_change(c,change.clone()).await?),None=>None};
    plan.write_work(c,&source,at).await?;
    let physical=match physical{Some(prepared)=>Some(prepared.apply(c,source.physical_asset_source()).await?),None=>None};
    let receipt=plan.finish(c,&source,physical.as_ref(),at).await?;
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut*c).await?;receipt.require_after(c,&source,physical.as_ref(),at).await?;
    let actual:(String,i64,Option<String>,Option<i64>,Option<i64>)=sqlx::query_as("SELECT status,version,blocked_reason,downtime_started_at,downtime_ended_at FROM maintenance_work_orders WHERE id='work'").fetch_one(&mut*c).await?;
    assert_eq!(actual,(status.into(),version+1,if action=="block"{Some("Synthetic transition evidence".into())}else{None},Some(NOW+1),None));
    let asset:(i64,String)=sqlx::query_as("SELECT version,lifecycle_status FROM operational_assets WHERE id='asset'").fetch_one(&mut*c).await?;assert_eq!(asset,(2,"maintenance".into()));
    let events:i64=sqlx::query_scalar("SELECT count(*)FROM operational_asset_events").fetch_one(&mut*c).await?;assert_eq!(events,1);
    assert!(source.require_current(c,at).await.is_err());Ok(())
   })).await.unwrap();
  }
  assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM maintenance_work_events").fetch_one(holding.raw()).await.unwrap(),3);
 }).await;}).await;
}
#[tokio::test]
async fn start_work_cas_precedes_original_late_asset_lifecycle_denial() {
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
  let(authorizer,_)=setup(&control,&holding).await;source(&holding).await;
  sqlx::raw_sql("UPDATE maintenance_work_orders SET status='open'WHERE id='work';UPDATE operational_assets SET lifecycle_status='retirement_pending',retirement_reason='Synthetic retirement'WHERE id='asset'").execute(holding.raw()).await.unwrap();
  for(stale,code)in [(true,"MAINTENANCE_STATE_CHANGED"),(false,"CONFLICT")]{
   error(authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance(),MaintenanceWorkSourcePreparation,move|c,current|Box::pin(async move{
    let source=read_current_maintenance_work(c,current,&id(),TimestampMillis::new(NOW)).await?.unwrap();let command=simple_command("start",if stale{2}else{1});let now=TimestampMillis::new(NOW);
    let plan=source.retain_simple_transition(c,&command,now,now).await?;let physical=source.physical_asset_source().prepare_physical_change(c,plan.physical_change().unwrap().clone()).await?;
    plan.write_work(c,&source,now).await?;let physical=physical.apply(c,source.physical_asset_source()).await?;let _=plan.finish(c,&source,Some(&physical),now).await?;Ok(())
   })).await,code);
   assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version FROM maintenance_work_orders WHERE id='work'").fetch_one(holding.raw()).await.unwrap(),1);
   assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM operational_asset_events").fetch_one(holding.raw()).await.unwrap(),0);
  }
 }).await;}).await;
}
#[tokio::test]
async fn start_cannot_substitute_another_command_physical_receipt_or_skip_it() {
    fixture::with_database(2, |control| async move {
        fixture::with_database(2, move |holding| async move {
            let (authorizer, _) = setup(&control, &holding).await;
            source(&holding).await;
            sqlx::query("UPDATE maintenance_work_orders SET status='open'WHERE id='work'")
                .execute(holding.raw())
                .await
                .unwrap();
            for (skip, code) in [
                (false, "ASSET_MAINTENANCE_SOURCE_CHANGED"),
                (true, "MAINTENANCE_STATE_CHANGED"),
            ] {
                error(
                    authorizer
                        .execute_with_sources(
                            "acme.example.test",
                            proof(),
                            input(),
                            maintenance(),
                            MaintenanceWorkSourcePreparation,
                            move |c, current| {
                                Box::pin(async move {
                                    let source = read_current_maintenance_work(
                                        c,
                                        current,
                                        &id(),
                                        TimestampMillis::new(NOW),
                                    )
                                    .await?
                                    .unwrap();
                                    let command = simple_command("start", 1);
                                    let now = TimestampMillis::new(NOW);
                                    let plan = source
                                        .retain_simple_transition(c, &command, now, now)
                                        .await?;
                                    let mut change = plan.physical_change().unwrap().clone();
                                    change.command_key =
                                        erpx_server_shared::public::CommandKey::new(
                                            "different-command-start",
                                        )?;
                                    let physical = source
                                        .physical_asset_source()
                                        .prepare_physical_change(c, change)
                                        .await?;
                                    plan.write_work(c, &source, now).await?;
                                    let physical = if skip {
                                        None
                                    } else {
                                        Some(
                                            physical
                                                .apply(c, source.physical_asset_source())
                                                .await?,
                                        )
                                    };
                                    let _ = plan.finish(c, &source, physical.as_ref(), now).await?;
                                    Ok(())
                                })
                            },
                        )
                        .await,
                    code,
                );
                assert_eq!(
                    sqlx::query_scalar::<_, i64>(
                        "SELECT version FROM operational_assets WHERE id='asset'"
                    )
                    .fetch_one(holding.raw())
                    .await
                    .unwrap(),
                    1
                );
                assert_eq!(
                    sqlx::query_scalar::<_, i64>(
                        "SELECT version FROM maintenance_work_orders WHERE id='work'"
                    )
                    .fetch_one(holding.raw())
                    .await
                    .unwrap(),
                    1
                );
            }
        })
        .await;
    })
    .await;
}
#[tokio::test]
async fn final_transition_receipt_refuses_deferred_work_rewrite_and_rolls_back_asset_start() {
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
  let(authorizer,_)=setup(&control,&holding).await;source(&holding).await;
  sqlx::raw_sql("UPDATE maintenance_work_orders SET status='open'WHERE id='work';CREATE FUNCTION synthetic_transition_rewrite()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN UPDATE maintenance_work_orders SET title='deferred title'WHERE id=NEW.work_order_id;RETURN NEW;END$$;CREATE CONSTRAINT TRIGGER synthetic_transition_rewrite AFTER INSERT ON maintenance_work_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_transition_rewrite()").execute(holding.raw()).await.unwrap();
  error(authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance(),MaintenanceWorkSourcePreparation,|c,current|Box::pin(async move{
   let source=read_current_maintenance_work(c,current,&id(),TimestampMillis::new(NOW)).await?.unwrap();let command=simple_command("start",1);let now=TimestampMillis::new(NOW);let plan=source.retain_simple_transition(c,&command,now,now).await?;let physical=source.physical_asset_source().prepare_physical_change(c,plan.physical_change().unwrap().clone()).await?;
   plan.write_work(c,&source,now).await?;let physical=physical.apply(c,source.physical_asset_source()).await?;let receipt=plan.finish(c,&source,Some(&physical),now).await?;sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut*c).await?;receipt.require_after(c,&source,Some(&physical),now).await?;Ok(())
  })).await,"MAINTENANCE_STATE_CHANGED");
  assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM maintenance_work_events").fetch_one(holding.raw()).await.unwrap(),0);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM operational_asset_events").fetch_one(holding.raw()).await.unwrap(),0);
  assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version FROM operational_assets WHERE id='asset'").fetch_one(holding.raw()).await.unwrap(),1);
 }).await;}).await;
}
