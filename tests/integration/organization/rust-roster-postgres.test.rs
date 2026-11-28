#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_organization_server::public::organization_access::OrganizationAccessActor;
use erpx_organization_server::public::organization_nodes::OrganizationScopeKind;
use erpx_organization_server::public::roster::{OrganizationRosterRead, PgOrganizationRoster};
use erpx_platform_runtime_server::public::postgres::{
    load_migrations, migrate_database, MigrationTrack, PgTransactionError, PgUnitOfWork,
    TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;

fn actor(people: bool, organization: bool) -> OrganizationAccessActor {
    OrganizationAccessActor {
        actor_id: "owner".into(),
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        scope_kind: OrganizationScopeKind::Holding,
        assignment_scope: OrganizationScopeKind::Holding,
        may_read_iam: true,
        may_manage_people: people,
        may_manage_organization: organization,
    }
}

async fn seed(pool: &erpx_platform_runtime_server::public::postgres::DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,active,created_at,updated_at) \
         VALUES('level-1','company','ceo','CEO',1,1,1,0,0), \
               ('level-2','company','director','Executive / Director',2,2,1,0,0), \
               ('level-9','company','dormant','Dormant',9,3,0,0,0);\
         INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,active,created_at,updated_at) \
         VALUES('foreign-level','foreign','ceo','CEO',1,1,1,0,0);\
         INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,reports_to_position_id,is_ceo,active,version,created_by,created_at,updated_at) \
         VALUES('ceo','company','branch','department','Chief','level-1',NULL,1,1,1,'fixture',0,0), \
               ('director','company','branch','department','Director','level-2','ceo',0,1,3,'fixture',0,0), \
               ('foreign-position','foreign',NULL,NULL,'Foreign position','foreign-level',NULL,0,1,1,'fixture',0,0);\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,email,country_code,status,version,created_by,created_at,updated_at,merged_into_id) \
         VALUES('ceo-person','ceo-user','company','Chief','chief@example.test','TH','active',1,'fixture',0,0,NULL), \
               ('holder','holder-user','company','Holder','holder@example.test','TH','active',1,'fixture',0,0,NULL), \
               ('aide-person','aide-user','company','Aide','aide@example.test','TH','active',1,'fixture',0,0,NULL), \
               ('archived-person','archived-user','company','Archived','archived@example.test','TH','archived',1,'fixture',0,0,NULL), \
               ('merged-person','merged-user','company','Merged','merged@example.test','TH','active',1,'fixture',0,0,'ceo-person'), \
               ('foreign-person','foreign-user','foreign','Foreigner','foreign@example.test','TH','active',1,'fixture',0,0,NULL);\
         INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,manager_person_id,job_title,currency,version,created_by,created_at,updated_at,position_id,level_id) \
         VALUES('engagement-ceo','ceo-person','company','branch','department','employee','salaried','active','2024-01-01',NULL,'Chief','THB',1,'fixture',0,0,'ceo','level-1'), \
               ('engagement-holder','holder','company','branch','department','employee','salaried','active','2024-01-01','ceo-person','Director','THB',1,'fixture',0,0,'director','level-2'), \
               ('engagement-aide','aide-person','company','branch','department','employee','salaried','active','2024-01-01','ceo-person','Aide','THB',1,'fixture',0,0,'director','level-9'), \
               ('engagement-archived','archived-person','company','branch','department','employee','salaried','active','2024-01-01',NULL,'Archived','THB',1,'fixture',0,0,'ceo','level-1'), \
               ('engagement-ended','holder','company','branch','department','employee','salaried','ended','2024-01-01',NULL,'Ended','THB',1,'fixture',0,0,'director','level-2'), \
               ('engagement-merged','merged-person','company','branch','department','employee','salaried','active','2024-01-01',NULL,'Merged','THB',1,'fixture',0,0,'director','level-2'), \
               ('engagement-contractor','holder','company','branch','department','contractor','contractor','active','2024-01-01',NULL,'Contractor','THB',1,'fixture',0,0,'director','level-2'), \
               ('engagement-foreign','foreign-person','foreign',NULL,NULL,'employee','salaried','active','2024-01-01',NULL,'Foreign','THB',1,'fixture',0,0,'foreign-position','foreign-level');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn run<T, F>(
    pool: erpx_platform_runtime_server::public::postgres::DatabasePool,
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
    pool: erpx_platform_runtime_server::public::postgres::DatabasePool,
    actor: OrganizationAccessActor,
) -> Result<OrganizationRosterRead, PgTransactionError<BusinessError>> {
    run(pool, move |connection| {
        Box::pin(async move {
            let mut ports = PgOrganizationRoster::new(connection);
            ports.read(&actor).await.map_err(|error| match error {
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

/// The native roster pair matches the retained `organizationRoster` family:
/// Company-scoped people and positions in retained order, with the retained
/// exclusions (archived, merged, ended, non-employee) and the inactive-level
/// projection preserved.
#[tokio::test]
async fn roster_read_matches_the_retained_family_pair() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let roster = read(pool.clone(), actor(true, true)).await.unwrap();
        let people: Vec<&str> = roster
            .people
            .iter()
            .map(|person| person.person_id.as_str())
            .collect();
        assert_eq!(people, ["aide-person", "ceo-person", "holder"]);
        assert!(roster
            .people
            .iter()
            .all(|person| person.company_id == "company"));
        let chief = roster
            .people
            .iter()
            .find(|person| person.person_id == "ceo-person")
            .unwrap();
        assert_eq!(chief.level_number, Some(1));
        assert_eq!(chief.level_name.as_deref(), Some("CEO"));
        assert_eq!(chief.engagement_id, "engagement-ceo");
        let aide = roster
            .people
            .iter()
            .find(|person| person.person_id == "aide-person")
            .unwrap();
        assert_eq!(aide.level_number, None);
        assert_eq!(aide.level_name, None);
        let positions: Vec<&str> = roster
            .positions
            .iter()
            .map(|position| position.id.as_str())
            .collect();
        assert_eq!(positions, ["ceo", "director"]);
        assert!(roster
            .positions
            .iter()
            .all(|position| position.company_id == "company"));
        let director = roster
            .positions
            .iter()
            .find(|position| position.id == "director")
            .unwrap();
        assert_eq!(director.level_number, 2);
        assert_eq!(director.reports_to_position_id.as_deref(), Some("ceo"));

        business(
            read(
                pool.clone(),
                OrganizationAccessActor {
                    may_read_iam: false,
                    may_manage_people: false,
                    may_manage_organization: false,
                    ..actor(false, false)
                },
            )
            .await,
            "PERMISSION_REQUIRED",
        );
    })
    .await;
}

#[tokio::test]
async fn roster_read_requires_the_owner_transaction() {
    fixture::with_database(1, |pool| async move {
        seed(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut ports = PgOrganizationRoster::new(&mut connection);
        let error = ports.read(&actor(true, true)).await.unwrap_err();
        match error {
            PgTransactionError::Persistence(error) => {
                assert_eq!(error.code, "ORGANIZATION_ROSTER_TRANSACTION_REQUIRED")
            }
            PgTransactionError::Business(error) => panic!("unexpected business {error:?}"),
        }
    })
    .await;
}
