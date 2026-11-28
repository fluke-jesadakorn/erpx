//! Native CRM identity-family owner effects over real migrated PostgreSQL.
//! These prove owner row writes, serializable-unit guards, CAS on the guest
//! link and persisted readback. PMS source, package authority, Workflow ledger
//! and browser journeys remain composition/A1 responsibilities.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_crm_server::public::accounts::{
    PgCrmIdentityFamily, change_guest_sharing, create_contact, link_guest,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
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
        "INSERT INTO crm_accounts(id,company_id,kind,name,created_by,version,created_at,updated_at) VALUES ('family-account','company','business','Family account','owner',1,1,1);\
         INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,version,created_by,updated_at) VALUES ('family-property','company','branch','Hotel','2026-12-01','open','{}',1,'owner',1);\
         INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES ('family-guest','family-property','guest','{}',1,1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn identity_family_persists_contact_guest_link_and_cas_sharing_in_one_unit() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;

        let contact = serde_json::json!({
            "action": "create-contact", "accountId": "family-account",
            "name": "  Front Desk  ", "detail": "  Reception contact  "
        });
        let guest = serde_json::json!({
            "action": "link-guest", "contactId": "contact-family-key", "guestId": "family-guest",
            "propertyId": "family-property", "evidence": "  Verified identity  ",
            "sharingEvidence": "  Guest consent  "
        });
        let sharing = serde_json::json!({
            "action": "guest-sharing", "linkId": "guest-link-family-key",
            "expectedVersion": 1, "state": "revoked", "evidence": "  Guest withdrew consent  "
        });

        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();

        // A non-serializable unit fails before any write.
        let mut plain = pool.raw().begin().await.unwrap();
        let mut plain_repo = PgCrmIdentityFamily::new(&mut plain);
        let denied = create_contact(
            "owner",
            "company",
            "plain-key",
            &serde_json::from_value(contact.clone()).unwrap(),
            &mut plain_repo,
            1,
        )
        .await
        .unwrap_err();
        assert!(matches!(denied, PgTransactionError::Persistence(_)));
        plain.rollback().await.unwrap();

        {
            let mut repo = PgCrmIdentityFamily::new(&mut tx);
            create_contact(
                "owner",
                "company",
                "family-key",
                &serde_json::from_value(contact).unwrap(),
                &mut repo,
                1,
            )
            .await
            .unwrap();
        }
        let (name, detail): (String, String) = sqlx::query_as(
            "SELECT name,detail FROM public.crm_contacts WHERE id='contact-family-key' AND company_id='company'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!((name.as_str(), detail.as_str()), ("Front Desk", "Reception contact"));

        {
            let mut repo = PgCrmIdentityFamily::new(&mut tx);
            link_guest(
                "owner",
                "company",
                "family-key",
                &serde_json::from_value(guest).unwrap(),
                &mut repo,
                1,
            )
            .await
            .unwrap();
        }
        let link: (String, String, String, i32) = sqlx::query_as(
            "SELECT sharing_state,sharing_evidence,sharing_by,version FROM public.crm_guest_links WHERE id='guest-link-family-key' AND company_id='company'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(link, ("granted".into(), "Guest consent".into(), "owner".into(), 1));
        let event: (String, i32, String) = sqlx::query_as(
            "SELECT state,version,evidence FROM public.crm_guest_sharing_events WHERE id='sharing-family-key'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(event, ("granted".into(), 1, "Guest consent".into()));

        {
            let mut repo = PgCrmIdentityFamily::new(&mut tx);
            change_guest_sharing(
                "owner",
                "company",
                "family-sharing-key",
                &serde_json::from_value(sharing.clone()).unwrap(),
                &mut repo,
                2,
            )
            .await
            .unwrap();
        }
        let (state, version): (String, i32) = sqlx::query_as(
            "SELECT sharing_state,version FROM public.crm_guest_links WHERE id='guest-link-family-key'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!((state.as_str(), version), ("revoked", 2));

        // A stale expected version is a conflict and writes no second event.
        let stale = {
            let mut repo = PgCrmIdentityFamily::new(&mut tx);
            change_guest_sharing(
                "owner",
                "company",
                "family-sharing-stale-key",
                &serde_json::from_value(sharing).unwrap(),
                &mut repo,
                3,
            )
            .await
            .unwrap_err()
        };
        let business = match stale {
            PgTransactionError::Business(error) => error,
            other => panic!("expected owner business conflict, got {other:?}"),
        };
        assert_eq!(business.code, "CONFLICT");
        let events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public.crm_guest_sharing_events WHERE link_id='guest-link-family-key'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(events, 2);

        tx.rollback().await.unwrap();
        let persisted: i64 = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM public.crm_contacts WHERE id='contact-family-key') \
             + (SELECT count(*) FROM public.crm_guest_links WHERE id='guest-link-family-key') \
             + (SELECT count(*) FROM public.crm_guest_sharing_events WHERE link_id='guest-link-family-key')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(persisted, 0);

        // Commit the same owner effects in a second unit and read them back.
        let mut committed = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *committed)
            .await
            .unwrap();
        {
            let mut repo = PgCrmIdentityFamily::new(&mut committed);
            create_contact(
                "owner",
                "company",
                "commit-key",
                &serde_json::from_value(serde_json::json!({
                    "action":"create-contact","accountId":"family-account",
                    "name":"Committed contact","detail":"Committed detail"
                }))
                .unwrap(),
                &mut repo,
                1,
            )
            .await
            .unwrap();
        }
        committed.commit().await.unwrap();
        let stored: (String, String) = sqlx::query_as(
            "SELECT name,detail FROM public.crm_contacts WHERE id='contact-commit-key' AND company_id='company'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(stored, ("Committed contact".into(), "Committed detail".into()));
    })
    .await;
}
