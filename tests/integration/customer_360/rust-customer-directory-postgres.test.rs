#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_customer_360::public::customer_timeline::CustomerTimelineActor;
use erpx_server_customer_360::public::directory::{
    CustomerDirectoryEntry, PgCustomerDirectoryRead, customer_directory,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

#[allow(clippy::too_many_arguments)]
fn actor(
    user: &str,
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    role: &str,
    assignment: &str,
    permissions: &[&str],
    modules: &[&str],
) -> CustomerTimelineActor {
    CustomerTimelineActor {
        user_id: user.into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: branch.map(|v| erpx_server_shared::public::RecordId::new(v).unwrap()),
        department_id: department.map(|v| erpx_server_shared::public::RecordId::new(v).unwrap()),
        role: role.into(),
        permissions: permissions.iter().map(|v| (*v).to_owned()).collect(),
        modules: modules.iter().map(|v| (*v).to_owned()).collect(),
        assignment_id: assignment.into(),
        expires_at: NOW.get() + 60_000,
    }
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch','company','company',NULL,'active',1),\
         ('other-branch','branch','other-branch','Other branch','company','company',NULL,'active',1),\
         ('department','department','department','Department','branch','company','branch','active',1),\
         ('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active',1),\
         ('foreign-branch','branch','foreign-branch','Foreign branch','foreign','foreign',NULL,'active',1);").execute(pool.raw()).await.unwrap();
    sqlx::raw_sql("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES ('customer','company','customer','C-001','Acme Customer','THB','active',1,10,10), ('unlinked','company','customer','C-002','Unlinked Customer','THB','active',1,10,10), ('foreign-customer','foreign','customer','F-001','Foreign Customer','THB','active',1,10,10);").execute(pool.raw()).await.unwrap();
    sqlx::raw_sql("INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES ('order-main','company','branch',NULL,'sales','SO-100','customer',NULL,'THB','2026-09-20','2026-09-23','Main','confirmed',100,0,1,'owner',100,100), ('order-foreign','foreign','foreign-branch',NULL,'sales','FSO-1','foreign-customer',NULL,'THB','2026-09-20','2026-09-23','Foreign','confirmed',100,0,1,'owner',400,400);").execute(pool.raw()).await.unwrap();
}

async fn read_directory(
    pool: &DatabasePool,
    actor: &CustomerTimelineActor,
) -> Result<Vec<CustomerDirectoryEntry>, BusinessError> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            Box::pin(async move {
                let mut reader = PgCustomerDirectoryRead::new(connection);
                customer_directory(&actor, &mut reader, NOW).await
            })
        })
        .await
        .map_err(|error| match error {
            PgTransactionError::Business(error) => error,
            other => panic!("expected an owner business failure, got {other:?}"),
        })
}

#[tokio::test]
async fn directory_scope_and_serializable_guard() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;

        // serializable guard outside transaction
        let mut conn = pool.raw().acquire().await.unwrap();
        let mut repo = PgCustomerDirectoryRead::new(&mut conn);
        let a = actor(
            "user",
            "company",
            None,
            None,
            "sales_manager",
            "assignment",
            &["commerce.read"],
            &[],
        );
        let err = repo
            .customers(
                &a,
                erpx_server_customer_360::public::directory::CustomerDirectoryView {
                    field_service: false,
                    finance: false,
                    legal: false,
                },
            )
            .await
            .unwrap_err();
        match err {
            PgTransactionError::Persistence(_) => {}
            PgTransactionError::Business(_) => panic!("expected persistence guard"),
        }
        drop(conn);

        let a = actor(
            "user",
            "company",
            None,
            None,
            "sales_manager",
            "assignment",
            &["commerce.read"],
            &[],
        );
        let entries = read_directory(&pool, &a).await.unwrap();
        assert!(entries.iter().any(|e| e.id == "customer"));
        assert!(!entries.iter().any(|e| e.id == "foreign-customer"));
        assert!(!entries.iter().any(|e| e.id == "unlinked"));
    })
    .await;
}

#[tokio::test]
async fn directory_branch_scope() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let a = actor(
            "user",
            "company",
            Some("branch"),
            None,
            "sales_manager",
            "assignment",
            &["commerce.read"],
            &[],
        );
        let entries = read_directory(&pool, &a).await.unwrap();
        assert!(entries.iter().any(|e| e.id == "customer"));
    })
    .await;
}
