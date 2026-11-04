//! Actual bounded Maintenance reads on retained Account/Holding authorization.
//! These are owner read projections, not command, purchase or browser evidence.
#![allow(dead_code)]
#[path = "../../support/rust-effective-authority.rs"]
mod auth;
#[path = "../../support/maintenance/parts-facts.rs"]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use auth::*;
use erpx_server_identity_access::public::effective_authority::{
    HoldingWorkSourcePreparation, WorkAuthorizationRequest,
};
use erpx_server_maintenance::public::read::{
    read_maintenance_detail, read_maintenance_plan_detail, read_maintenance_plans,
    read_maintenance_workspace,
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, TimestampMillis};
use erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest;
use facts::*;
use sqlx::PgConnection;

#[derive(Clone, Copy)]
struct ReadSources;
impl HoldingWorkSourcePreparation for ReadSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

fn scoped_input() -> HoldingWorkspaceRequest {
    let mut request = input();
    request.branch_slug = Some("north".into());
    request
}

fn read_request() -> WorkAuthorizationRequest {
    WorkAuthorizationRequest {
        permission: "records.read".into(),
        module: Some("maintenance".into()),
        destination: "assets/maintenance".into(),
        api: false,
    }
}
async fn seed(pool: &DatabasePool) {
    seed_work(pool).await;
    sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_native_authority',3,'[\"records.read\",\"records.write\",\"records.review\",\"module.maintenance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);UPDATE iam_roles SET published_revision=3 WHERE id='custom_native_authority';INSERT INTO maintenance_work_events(id,work_order_id,version,action,actor_id,reason,status,owner_id,scheduled_at,downtime_started_at,downtime_ended_at,created_at)VALUES('work-event-1','work',1,'create','user','Synthetic work opened','in_progress','user',NULL,NULL,NULL,0)").execute(pool.raw()).await.unwrap();
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("INSERT INTO maintenance_preventive_plans(id,company_id,branch_id,department_id,asset_id,title,priority,owner_id,interval_days,time_zone,first_due_local,next_due_local,next_due_at,lead_days,created_by,edited_by,created_at,updated_at)VALUES('plan','company','branch',NULL,'asset','Synthetic preventive plan','normal','user',30,'Asia/Bangkok','2026-11-10T09:00','2026-11-10T09:00',$1,7,'user','user',0,0)").bind(NOW).execute(&mut *tx).await.unwrap();
    sqlx::raw_sql("INSERT INTO maintenance_preventive_events(id,plan_id,version,action,actor_id,reason,snapshot_json,created_at)SELECT 'plan-event-1',id,version,'create','user','Synthetic plan created',to_jsonb(p),0 FROM maintenance_preventive_plans p WHERE id='plan'").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn workspace_detail_and_preventive_reads_are_company_scope_bound_and_preserve_source_facts() {
    fixture::with_database(2, |control| async move {
        fixture::with_database(2, move |holding| async move {
            let (authorizer, clock) = setup(&control, &holding).await;
            seed(&holding).await;
            let now = clock.now();
            authorizer
                .execute_with_sources(
                    "acme.example.test",
                    proof(),
                    input(),
                    read_request(),
                    ReadSources,
                    move |connection, current| {
                        Box::pin(async move {
                            let workspace =
                                read_maintenance_workspace(connection, current, now).await?;
                            assert_eq!(workspace.items.len(), 1);
                            let work = &workspace.items[0];
                            assert_eq!(work.id, "work");
                            assert_eq!(work.asset_code, "SYNTHETIC");
                            assert_eq!(work.asset_name, "PRIVATE_ASSET");
                            assert_eq!(work.owner_name.as_deref(), Some("PRIVATE_WORKER"));
                            assert_eq!(work.owner_eligible, 1);
                            assert_eq!(work.can_review, 1);
                            assert!(workspace.can_create);
                            assert_eq!(workspace.assets.len(), 1);
                            assert_eq!(workspace.people.len(), 1);
                            assert!(workspace.nodes.iter().any(|node| node.id == "branch"));

                            let detail = read_maintenance_detail(
                                connection, current, "work", now,
                            )
                            .await?;
                            assert_eq!(detail.history.len(), 1);
                            assert_eq!(detail.history[0].action, "create");
                            assert!(detail.consumptions.is_empty());
                            assert_eq!(detail.value()["work"]["status"], "in_progress");
                            assert!(
                                read_maintenance_detail(connection, current, "missing", now)
                                    .await
                                    .is_err()
                            );

                            let plans = read_maintenance_plans(connection, current, now).await?;
                            assert_eq!(plans.len(), 1);
                            assert_eq!(plans[0].asset_code, "SYNTHETIC");
                            assert_eq!(plans[0].owner_eligible, 1);
                            let plan = read_maintenance_plan_detail(
                                connection, current, "plan", now,
                            )
                            .await?;
                            assert_eq!(plan.asset_version, Some(1));
                            assert_eq!(plan.people.len(), 1);
                            assert_eq!(plan.history.len(), 1);
                            assert!(plan.occurrences.is_empty());
                            assert_eq!(plan.value()["plan"]["status"], "draft");
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
async fn reads_refuse_missing_read_permission_expired_context_and_foreign_record_ids() {
    fixture::with_database(2, |control| async move {
        fixture::with_database(2, move |holding| async move {
            let (authorizer, clock) = setup(&control, &holding).await;
            seed(&holding).await;
            // Scope a foreign work row to another branch and company.
            sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('branch-b','branch','south','South','company','company',NULL,'active');INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at)VALUES('foreign-asset','company','branch-b','FOREIGN','Foreign asset','synthetic','THB','fixture',0,0);INSERT INTO maintenance_work_orders(id,company_id,branch_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at)VALUES('foreign-work','company','branch-b','foreign-asset','Foreign work','normal','open','user','user',0,0);UPDATE assignments SET scope_kind='branch',scope_node_id='branch' WHERE id='grant'").execute(holding.raw()).await.unwrap();
            let now = clock.now();
            authorizer
                .execute_with_sources(
                    "acme.example.test",
                    proof(),
                    scoped_input(),
                    read_request(),
                    ReadSources,
                    move |connection, current| {
                        Box::pin(async move {
                            let workspace =
                                read_maintenance_workspace(connection, current, now).await?;
                            assert!(workspace.items.iter().all(|item| item.id != "foreign-work"));
                            let error = read_maintenance_detail(
                                connection,
                                current,
                                "foreign-work",
                                now,
                            )
                            .await
                            .unwrap_err();
                            assert!(
                                matches!(
                                    error,
                                    erpx_server_runtime::public::postgres::PgTransactionError::Business(ref e)
                                        if e.kind == FailureKind::NotFound
                                ),
                                "foreign work must be a scoped not-found"
                            );
                            Ok(())
                        })
                    },
                )
                .await
                .unwrap();

            // Remove the read permission while retaining an otherwise valid
            // session and selected assignment.
            sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_native_authority',4,'[\"module.maintenance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);UPDATE iam_roles SET published_revision=4 WHERE id='custom_native_authority'").execute(holding.raw()).await.unwrap();
            let denied = authorizer
                .execute_with_sources(
                    "acme.example.test",
                    proof(),
                    scoped_input(),
                    read_request(),
                    ReadSources,
                    move |connection, current| {
                        Box::pin(async move { read_maintenance_workspace(connection, current, now).await })
                    },
                )
                .await
                .unwrap_err();
            assert!(
                matches!(
                    denied,
                    erpx_server_runtime::public::postgres::PgTransactionError::Business(ref e)
                        if e.kind == FailureKind::Forbidden
                ),
                "missing records.read must fail closed"
            );

            // Restore read, then observe beyond the selected context expiry.
            sqlx::raw_sql("UPDATE iam_roles SET published_revision=3 WHERE id='custom_native_authority'").execute(holding.raw()).await.unwrap();
            let expired_now = TimestampMillis::new(NOW + 3_600_001);
            let expired = authorizer
                .execute_with_sources(
                    "acme.example.test",
                    proof(),
                    scoped_input(),
                    read_request(),
                    ReadSources,
                    move |connection, current| {
                        Box::pin(async move {
                            read_maintenance_workspace(connection, current, expired_now).await
                        })
                    },
                )
                .await;
            assert!(expired.is_err(), "expired selected context must fail closed");
        })
        .await;
    })
    .await;
}
