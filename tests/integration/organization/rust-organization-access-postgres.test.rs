#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_organization::public::organization_access::*;
use erpx_server_organization::public::organization_nodes::OrganizationScopeKind;
use erpx_server_runtime::public::postgres::{
    load_migrations, migrate_database, MigrationTrack, PgTransactionError, PgUnitOfWork,
    TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, TimestampMillis};

#[derive(Clone, Copy)]
struct TestClock;
impl Clock for TestClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1_800_000_000_000)
    }
}

fn actor(
    scope: OrganizationScopeKind,
    branch: Option<&str>,
    department: Option<&str>,
    people: bool,
    organization: bool,
) -> OrganizationAccessActor {
    OrganizationAccessActor {
        actor_id: "owner".into(),
        company_id: "company".into(),
        branch_id: branch.map(str::to_owned),
        department_id: department.map(str::to_owned),
        scope_kind: scope,
        assignment_scope: scope,
        may_read_iam: true,
        may_manage_people: people,
        may_manage_organization: organization,
    }
}

async fn seed(pool: &erpx_server_runtime::public::postgres::DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) \
         VALUES('company','company','acme','Synthetic Acme',NULL,'company',NULL,'active'), \
               ('branch','branch','north','North','company','company','branch','active'), \
               ('department','department','service','Service','branch','company','branch','active');\
         INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,active,created_at,updated_at) \
         VALUES('level-1','company','ceo','CEO',1,1,1,0,0), \
               ('level-2','company','director','Executive / Director',2,2,1,0,0);\
         INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,reports_to_position_id,is_ceo,active,version,created_by,created_at,updated_at) \
         VALUES('ceo','company','branch','department','Chief','level-1',NULL,1,1,1,'fixture',0,0), \
               ('director','company','branch','department','Director','level-2','ceo',0,1,3,'fixture',0,0), \
               ('foreign','foreign',NULL,NULL,'Foreign','level-1',NULL,0,1,1,'fixture',0,0);\
         INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,active,created_at,updated_at) \
         VALUES('foreign-level','foreign','ceo','CEO',1,1,1,0,0);\
         INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,reports_to_position_id,is_ceo,active,version,created_by,created_at,updated_at) \
         VALUES('foreign-position','foreign',NULL,NULL,'Foreign position','foreign-level',NULL,0,1,1,'fixture',0,0);\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,email,country_code,status,version,created_by,created_at,updated_at) \
         VALUES('ceo-person','ceo-user','company','Chief','chief@example.test','TH','active',1,'fixture',0,0), \
               ('holder','holder-user','company','Holder','holder@example.test','TH','active',1,'fixture',0,0), \
               ('foreign-person','foreign-user','foreign','Foreigner','foreign@example.test','TH','active',1,'fixture',0,0);\
         INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,manager_person_id,job_title,currency,version,created_by,created_at,updated_at,position_id,level_id) \
         VALUES('engagement-ceo','ceo-person','company','branch','department','employee','salaried','active','2024-01-01',NULL,'Chief','THB',1,'fixture',0,0,'ceo','level-1'), \
               ('engagement-holder','holder','company','branch','department','employee','salaried','active','2024-01-01','ceo-person','Director','THB',1,'fixture',0,0,'director','level-2'), \
               ('engagement-foreign','foreign-person','foreign',NULL,NULL,'employee','salaried','active','2024-01-01',NULL,'Foreign','THB',1,'fixture',0,0,'foreign-position','foreign-level');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn run<T, F>(
    pool: erpx_server_runtime::public::postgres::DatabasePool,
    f: F,
) -> Result<T, PgTransactionError<BusinessError>>
where
    F: for<'c> FnOnce(
            &'c mut sqlx::PgConnection,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<T, BusinessError>> + Send + 'c>,
        > + Send
        + 'static,
    T: Send + 'static,
{
    PgUnitOfWork::new(pool)
        .run_once(TransactionOptions::default(), move |connection| {
            Box::pin(async move { f(connection).await.map_err(PgTransactionError::Business) })
        })
        .await
}

async fn read(
    pool: erpx_server_runtime::public::postgres::DatabasePool,
    actor: OrganizationAccessActor,
) -> Result<OrganizationAccessData, PgTransactionError<BusinessError>> {
    run(pool, move |connection| {
        Box::pin(async move {
            let mut ports = PgOrganizationAccess::new(connection);
            ports
                .read(&actor, &TestClock)
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
        })
    })
    .await
}

fn business<T>(result: Result<T, PgTransactionError<BusinessError>>, code: &str) {
    match result {
        Err(PgTransactionError::Business(error)) => assert_eq!(error.code, code),
        Err(PgTransactionError::Persistence(error)) => {
            panic!("expected business denial {code}, got persistence {error:?}")
        }
        Ok(_) => panic!("expected business denial {code}, got success"),
    }
}

#[tokio::test]
async fn access_projection_is_bounded_to_the_company_and_actor_window() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let company = read(
            pool.clone(),
            actor(OrganizationScopeKind::Holding, None, None, true, true),
        )
        .await
        .unwrap();
        assert_eq!(company.positions.len(), 2);
        assert!(company
            .people
            .iter()
            .all(|person| person.company_id == "company"));
        assert_eq!(company.people.len(), 2);
        assert!(company.can_move_people);
        assert!(company.can_manage);
        assert_eq!(company.actor_id, "owner");
        assert!(company
            .people
            .iter()
            .all(|person| person.email.is_some() && !person.start_date.is_empty()));
        let holder = company
            .people
            .iter()
            .find(|person| person.person_id == "holder")
            .unwrap();
        assert!(holder.issues.is_empty(), "{:?}", holder.issues);

        let branch = read(
            pool.clone(),
            actor(
                OrganizationScopeKind::Branch,
                Some("branch"),
                None,
                false,
                true,
            ),
        )
        .await
        .unwrap();
        assert_eq!(branch.positions.len(), 2);
        assert_eq!(branch.people.len(), 2);
        assert!(!branch.can_move_people);
        assert!(!branch.can_manage);
        assert!(branch.people.iter().all(|person| person.email.is_none()
            && person.start_date.is_empty()
            && person.end_date.is_none()));

        let denied = read(
            pool.clone(),
            OrganizationAccessActor {
                may_read_iam: false,
                may_manage_people: false,
                may_manage_organization: false,
                ..actor(OrganizationScopeKind::Holding, None, None, false, false)
            },
        )
        .await;
        business(denied, "PERMISSION_REQUIRED");
    })
    .await;
}

#[tokio::test]
async fn access_projection_requires_the_owner_transaction() {
    fixture::with_database(1, |pool| async move {
        seed(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut ports = PgOrganizationAccess::new(&mut connection);
        let error = ports
            .read(
                &actor(OrganizationScopeKind::Holding, None, None, true, true),
                &TestClock,
            )
            .await
            .unwrap_err();
        match error {
            PgTransactionError::Persistence(error) => {
                assert_eq!(error.code, "ORGANIZATION_ACCESS_TRANSACTION_REQUIRED")
            }
            PgTransactionError::Business(error) => panic!("unexpected business {error:?}"),
        }
    })
    .await;
}
