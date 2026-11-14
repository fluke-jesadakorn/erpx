#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_people_payroll::public::staff_facts::{StaffPersonFact, staff_people};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO staff_levels(id,company_id,code,name,rank,active,version,created_at,updated_at,level_number) VALUES
           ('l1','company','L1','Level 1',1,1,1,0,0,1),
           ('l2','company','L2','Level 2',2,1,1,0,0,2),
           ('l4','company','L4','Level 4',4,1,1,0,0,4);
         INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,is_ceo,active,version,created_by,created_at,updated_at) VALUES
           ('pos-head','company','branch','dept-a','Head','l4',0,1,1,'fixture',0,0),
           ('pos-inactive','company','branch','dept-c','Old','l1',0,0,1,'fixture',0,0);
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,merged_into_id,created_by,created_at,updated_at) VALUES
           ('person-a','user-a','company','Alpha','TH','active',NULL,'fixture',0,0),
           ('person-b','user-b','company','Bravo','TH','active',NULL,'fixture',0,0),
           ('person-c','user-c','company','Charlie','TH','active',NULL,'fixture',0,0),
           ('person-unassigned','user-unassigned','company','Delta','TH','active',NULL,'fixture',0,0),
           ('person-revoked','user-revoked','company','Echo','TH','active',NULL,'fixture',0,0),
           ('person-merged','merged-user','company','Merged','TH','active','person-a','fixture',0,0),
           ('person-inactive','inactive-user','company','Inactive','TH','inactive',NULL,'fixture',0,0),
           ('person-f','user-f','foreign','Foxtrot','TH','active',NULL,'fixture',0,0);
         INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,end_date,manager_person_id,level_id,position_id,job_title,currency,created_by,created_at,updated_at) VALUES
           ('eng-a-new','person-a','company','branch','dept-a','employee','salaried','active','2020-01-01',NULL,'person-b','l4','pos-head','Newest','THB','fixture',0,0),
           ('eng-a-old','person-a','company','branch','dept-a','employee','salaried','active','2000-01-01',NULL,NULL,'l1','pos-head','Old','THB','fixture',0,0),
           ('eng-b','person-b','company','branch','dept-b','employee','salaried','active','2000-01-01',NULL,NULL,'l2','pos-head','Bravo','THB','fixture',0,0),
           ('eng-c','person-c','company','branch','dept-c','employee','salaried','active','2000-01-01',NULL,NULL,NULL,'pos-inactive','Charlie','THB','fixture',0,0),
           ('eng-d','person-unassigned','company','branch','dept-a','employee','salaried','active','2000-01-01',NULL,NULL,NULL,NULL,'No account','THB','fixture',0,0),
           ('eng-e','person-revoked','company','branch','dept-a','employee','salaried','active','2000-01-01',NULL,NULL,NULL,NULL,'Revoked','THB','fixture',0,0),
           ('eng-merged','person-merged','company','branch','dept-a','employee','salaried','active','2000-01-01',NULL,NULL,NULL,NULL,'Merged','THB','fixture',0,0),
           ('eng-inactive','person-inactive','company','branch','dept-a','employee','salaried','active','2000-01-01',NULL,NULL,NULL,NULL,'Inactive','THB','fixture',0,0),
           ('eng-f','person-f','foreign','foreign-branch','foreign-department','employee','salaried','active','2000-01-01',NULL,NULL,NULL,NULL,'Foreign','THB','fixture',0,0),
           ('eng-future','person-b','company','branch','dept-b','employee','salaried','active','2999-01-01',NULL,NULL,NULL,NULL,'Future','THB','fixture',0,0),
           ('eng-ended','person-b','company','branch','dept-b','employee','salaried','active','2000-01-01','2000-12-31',NULL,NULL,NULL,'Ended','THB','fixture',0,0),
           ('eng-suspended','person-b','company','branch','dept-b','employee','salaried','suspended','2000-01-01',NULL,NULL,NULL,NULL,'Suspended','THB','fixture',0,0);
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,revoked_at,version,scope_kind) VALUES
           ('asg-a','user-a','employee','[]','company',0,9007199254740991,NULL,1,'holding'),
           ('asg-b','user-b','employee','[]','company',0,9007199254740991,NULL,1,'holding'),
           ('asg-c','user-c','employee','[]','company',0,9007199254740991,NULL,1,'holding'),
           ('asg-merged','merged-user','employee','[]','company',0,9007199254740991,NULL,1,'holding'),
           ('asg-inactive','inactive-user','employee','[]','company',0,9007199254740991,NULL,1,'holding'),
           ('asg-f','user-f','employee','[]','foreign',0,9007199254740991,NULL,1,'holding'),
           ('asg-revoked','user-revoked','employee','[]','company',0,9007199254740991,1,1,'holding')",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn staff_people_keeps_company_window_ordering_and_assignment() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let rows = staff_people(&mut connection, "company").await.unwrap();
        assert_eq!(
            rows,
            vec![
                StaffPersonFact {
                    user_id: "user-a".into(),
                    person_id: "person-a".into(),
                    legal_name: "Alpha".into(),
                    department_id: Some("dept-a".into()),
                    position_id: Some("pos-head".into()),
                    level_number: Some(4),
                    manager_person_id: Some("person-b".into()),
                    engagement_id: "eng-a-new".into(),
                    engagement_version: 1,
                },
                StaffPersonFact {
                    user_id: "user-a".into(),
                    person_id: "person-a".into(),
                    legal_name: "Alpha".into(),
                    department_id: Some("dept-a".into()),
                    position_id: Some("pos-head".into()),
                    level_number: Some(1),
                    manager_person_id: None,
                    engagement_id: "eng-a-old".into(),
                    engagement_version: 1,
                },
                StaffPersonFact {
                    user_id: "user-b".into(),
                    person_id: "person-b".into(),
                    legal_name: "Bravo".into(),
                    department_id: Some("dept-b".into()),
                    position_id: Some("pos-head".into()),
                    level_number: Some(2),
                    manager_person_id: None,
                    engagement_id: "eng-b".into(),
                    engagement_version: 1,
                },
                StaffPersonFact {
                    user_id: "user-c".into(),
                    person_id: "person-c".into(),
                    legal_name: "Charlie".into(),
                    department_id: Some("dept-c".into()),
                    // The inactive position projection is absent, but the
                    // retained read still reports the engagement position id.
                    position_id: Some("pos-inactive".into()),
                    level_number: None,
                    manager_person_id: None,
                    engagement_id: "eng-c".into(),
                    engagement_version: 1,
                },
            ]
        );
    })
    .await;
}

#[tokio::test]
async fn staff_people_rejects_other_companies_without_an_account() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let foreign = staff_people(&mut connection, "foreign").await.unwrap();
        assert_eq!(foreign.len(), 1);
        assert_eq!(foreign[0].user_id, "user-f");
        assert_eq!(foreign[0].person_id, "person-f");
        assert!(foreign[0].department_id.as_deref() == Some("foreign-department"));

        // A missing Company is an empty honest read.
        assert!(
            staff_people(&mut connection, "missing")
                .await
                .unwrap()
                .is_empty()
        );
    })
    .await;
}

#[tokio::test]
async fn staff_people_validates_identifiers_before_any_read() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let error = staff_people(&mut connection, "").await.unwrap_err();
        assert_eq!(error.code, "INVALID_IDENTIFIER");
        assert_eq!(error.kind, erpx_server_shared::public::FailureKind::InvalidInput);
    })
    .await;
}
