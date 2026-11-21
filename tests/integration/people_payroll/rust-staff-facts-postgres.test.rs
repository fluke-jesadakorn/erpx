#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_people_payroll::public::staff_facts::{active_staff, person_name, ActiveStaffFact};
use erpx_server_runtime::public::postgres::{
    load_migrations, migrate_database, DatabasePool, MigrationTrack,
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
        "INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,merged_into_id,created_by,created_at,updated_at) VALUES
           ('survivor',NULL,'company','Survivor','TH','active',NULL,'fixture',0,0),
           ('person','user','company','Alpha','TH','active',NULL,'fixture',0,0),
           ('merged','merged-user','company','Merged','TH','active','survivor','fixture',0,0),
           ('inactive','inactive-user','company','Inactive','TH','inactive',NULL,'fixture',0,0),
           ('foreign','foreign-user','foreign','Foreign','TH','active',NULL,'fixture',0,0);
         INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,end_date,job_title,currency,created_by,created_at,updated_at) VALUES
           ('current','person','company','branch','department','employee','salaried','active','2000-01-01',NULL,'Current','THB','fixture',0,0),
           ('newer','person','company','newer-branch','newer-department','employee','salaried','active','2001-01-01',NULL,'Newer','THB','fixture',0,0),
           ('future','person','company','future-branch','future-department','employee','salaried','active','2999-01-01',NULL,'Future','THB','fixture',0,0),
           ('ended','person','company','old-branch','old-department','employee','salaried','active','2000-01-01','2000-12-31','Ended','THB','fixture',0,0),
           ('suspended','person','company','suspended-branch','suspended-department','employee','salaried','suspended','2000-01-01',NULL,'Suspended','THB','fixture',0,0),
           ('foreign-engagement','foreign','foreign','foreign-branch','foreign-department','employee','salaried','active','2000-01-01',NULL,'Foreign','THB','fixture',0,0)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn active_staff_preserves_current_company_utc_window_and_ordering() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        assert_eq!(
            active_staff(&mut connection, "company", "user")
                .await
                .unwrap(),
            Some(ActiveStaffFact {
                branch_id: Some("newer-branch".into()),
                department_id: Some("newer-department".into()),
            })
        );
        // Another Company, an unlinked user, an inactive or merged Person, and
        // a user whose only active engagement is outside the UTC day all miss.
        assert!(active_staff(&mut connection, "foreign", "user")
            .await
            .unwrap()
            .is_none());
        assert!(active_staff(&mut connection, "company", "missing")
            .await
            .unwrap()
            .is_none());
        assert!(active_staff(&mut connection, "company", "merged-user")
            .await
            .unwrap()
            .is_none());
        assert!(active_staff(&mut connection, "company", "inactive-user")
            .await
            .unwrap()
            .is_none());
        assert!(active_staff(&mut connection, "company", "foreign-user")
            .await
            .unwrap()
            .is_none());
        sqlx::raw_sql(
            "UPDATE hr_engagements SET end_date='2000-12-31' WHERE id='current';
             UPDATE hr_engagements SET end_date='2001-12-31' WHERE id='newer'",
        )
        .execute(&mut *connection)
        .await
        .unwrap();
        assert!(active_staff(&mut connection, "company", "user")
            .await
            .unwrap()
            .is_none());
    })
    .await;
}

#[tokio::test]
async fn person_name_requires_a_current_active_unmerged_person() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        assert_eq!(
            person_name(&mut connection, "company", "user")
                .await
                .unwrap()
                .as_deref(),
            Some("Alpha")
        );
        assert!(person_name(&mut connection, "company", "merged-user")
            .await
            .unwrap()
            .is_none());
        assert!(person_name(&mut connection, "company", "inactive-user")
            .await
            .unwrap()
            .is_none());
        assert!(person_name(&mut connection, "foreign", "user")
            .await
            .unwrap()
            .is_none());
        assert!(person_name(&mut connection, "company", "missing")
            .await
            .unwrap()
            .is_none());
    })
    .await;
}

#[tokio::test]
async fn identifiers_are_validated_before_any_read() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let error = active_staff(&mut connection, "", "user").await.unwrap_err();
        assert_eq!(error.code, "INVALID_IDENTIFIER");
        let error = person_name(&mut connection, "company", "")
            .await
            .unwrap_err();
        assert_eq!(error.code, "INVALID_IDENTIFIER");
    })
    .await;
}
