//! Native Work Projection navigation read on real owner public reads.
//!
//! Proves the navigation projection composes the same native snapshot and
//! capability reads on the caller's retained serializable Holding transaction,
//! keeps missing sources explicit, and never invents the Home or routing facts.
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
use erpx_server_work_projection::public::work_capability_probes::InventoryPreparation;
use erpx_server_work_projection::public::work_navigation_read::{
    NativeWorkNavigationRequest, read_native_work_navigation,
};
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
         INSERT INTO maintenance_work_events(id,work_order_id,version,action,actor_id,reason,status,owner_id,scheduled_at,downtime_started_at,downtime_ended_at,created_at) \
         VALUES('work-event-1','work',1,'create','user','Synthetic work opened','in_progress','user',NULL,NULL,NULL,0)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn request(include_attention: bool) -> NativeWorkNavigationRequest {
    NativeWorkNavigationRequest {
        beta: false,
        inventory: InventoryPreparation {
            can_prepare_counts: true,
            can_prepare_transfers: true,
        },
        expense_available: true,
        home_unread: 4,
        include_attention,
        routing_attention: true,
    }
}

#[tokio::test]
async fn composes_native_navigation_with_explicit_unknown_sources() {
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
                            let full = read_native_work_navigation(
                                connection,
                                current,
                                request(true),
                                now,
                            )
                            .await?;
                            assert_eq!(full.home, 4);
                            let attention = full.attention.expect("attention");
                            assert!(attention.incomplete);
                            assert!(!attention.items.is_empty());
                            assert!(attention.total >= attention.items.len());
                            assert!(full.work.is_none());
                            assert!(full.work_view_availability.routing);

                            let compact = read_native_work_navigation(
                                connection,
                                current,
                                request(false),
                                now,
                            )
                            .await?;
                            assert_eq!(compact.attention, None);
                            assert!(compact.work.is_none());
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
