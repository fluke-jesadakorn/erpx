//! Native Work Projection snapshot composition on real owner public reads.
//!
//! This proves the snapshot adapter composes the Maintenance owner read on the
//! caller's retained serializable Holding transaction. Sources without a
//! native owner read remain explicitly unavailable and are not replaced with
//! empty results.
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
use erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest;
use erpx_server_runtime::public::postgres::{DatabasePool, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock};
use erpx_server_work_projection::public::work_card_availability::WorkQueue;
use erpx_server_work_projection::public::work_snapshot_read::read_native_work_snapshot;
use sqlx::PgConnection;

#[derive(Clone, Copy)]
struct ReadSources;
impl HoldingWorkSourcePreparation for ReadSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

fn read_request() -> WorkAuthorizationRequest {
    WorkAuthorizationRequest {
        permission: "records.read".into(),
        module: Some("maintenance".into()),
        destination: "assets/maintenance".into(),
        api: false,
    }
}

fn input() -> HoldingWorkspaceRequest {
    let mut request = auth::input();
    request.mutation = false;
    request
}

async fn seed(pool: &DatabasePool) {
    facts::seed_work(pool).await;
    sqlx::raw_sql(
        "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','custom_native_authority',4,'[\"records.read\",\"records.write\",\"records.review\",\"module.maintenance.read\",\"organization.manage\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);\
         UPDATE iam_roles SET published_revision=4 WHERE id='custom_native_authority';\
         UPDATE assignments SET modules_json='[\"maintenance\",\"organization\"]' WHERE id='grant';\
         INSERT INTO organization_node_changes(id,company_id,action,node_kind,slug,display_name,state,version,requested_by,reason,created_at,updated_at) VALUES('change-1','company','create','branch','newbranch','New Branch','submitted',1,'other','Synthetic change',0,0);\
         INSERT INTO maintenance_work_events(id,work_order_id,version,action,actor_id,reason,status,owner_id,scheduled_at,downtime_started_at,downtime_ended_at,created_at) \
         VALUES('work-event-1','work',1,'create','user','Synthetic work opened','in_progress','user',NULL,NULL,NULL,0);\
         INSERT INTO maintenance_preventive_plans(id,company_id,branch_id,department_id,asset_id,title,priority,owner_id,interval_days,time_zone,first_due_local,next_due_local,next_due_at,lead_days,created_by,edited_by,created_at,updated_at) \
         VALUES('plan','company','branch',NULL,'asset','Synthetic preventive plan','normal','user',30,'Asia/Bangkok','2026-11-10T09:00','2026-11-10T09:00',1800000000000,7,'user','user',0,0);\
         INSERT INTO maintenance_preventive_events(id,plan_id,version,action,actor_id,reason,snapshot_json,created_at) \
         SELECT 'plan-event-1',id,version,'create','user','Synthetic plan created',to_jsonb(p),0 FROM maintenance_preventive_plans p WHERE id='plan'",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn composes_real_maintenance_reads_and_reports_unavailable_sources() {
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
                            let snapshot =
                                read_native_work_snapshot(connection, current, now).await?;
                            let maintenance = snapshot
                                .items
                                .iter()
                                .find(|item| item.id == "maintenance:work")
                                .expect("maintenance work item");
                            assert_eq!(maintenance.kind, "maintenance_work");
                            assert_eq!(maintenance.queue, WorkQueue::Todo);
                            assert_eq!(maintenance.can_act, Some(true));
                            assert_eq!(maintenance.relationships, vec!["assigned".to_owned()]);
                            assert_eq!(maintenance.href, "/maintenance/work");
                            assert!(
                                snapshot
                                    .items
                                    .iter()
                                    .any(|item| item.id == "maintenance_plan:plan")
                            );
                            let change = snapshot
                                .items
                                .iter()
                                .find(|item| item.id == "organization_change:change-1")
                                .expect("organization change item");
                            assert_eq!(change.kind, "organization_change");
                            assert_eq!(change.queue, WorkQueue::Todo);
                            assert_eq!(change.can_act, Some(true));
                            assert_eq!(change.relationships, vec!["approval".to_owned()]);
                            assert!(snapshot.unavailable_sources.contains(&"supportWork"));
                            assert!(
                                !snapshot
                                    .unavailable_sources
                                    .contains(&"organizationChangeWork")
                            );
                            assert!(!snapshot.unavailable_sources.contains(&"maintenanceWork"));
                            assert!(!snapshot.unavailable_sources.contains(&"preventivePlanWork"));
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
