//! Postgres proof of the native calendar Inbox visibility delegate.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::inbox::{
    CalendarInboxTargets, InboxTarget, InboxTargetPort, PgInboxTargets,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::BusinessError;
use serde_json::json;

async fn seed(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES \
         ('person-1','user-1','co-1','Ada','TH','active','sys',1000,1000), \
         ('person-3','user-3','co-1','Bo','TH','active','sys',1000,1000)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO calendar_appointments(id,company_id,organizer_person_id,title,starts_at,ends_at,timezone,status,version,created_by,created_at,updated_at) VALUES \
         ('appt-1','co-1','person-1','Planning',2000,3000,'Asia/Bangkok','scheduled',7,'user-1',1000,1000), \
         ('appt-creator','co-1','person-3','Solo',2000,3000,'Asia/Bangkok','scheduled',3,'user-3',1000,1000), \
         ('seg/one two','co-1','person-1','Encoded',2000,3000,'Asia/Bangkok','scheduled',5,'user-9',1000,1000)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO calendar_attendees(company_id,appointment_id,user_id) VALUES \
         ('co-1','appt-1','user-1'), \
         ('co-1','seg/one two','user-1')",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn run<T, F>(
    pool: DatabasePool,
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

async fn composed_target(
    pool: DatabasePool,
    company_id: &str,
    user_id: &str,
    source_type: &str,
    source_id: &str,
) -> Option<InboxTarget> {
    let (company_id, user_id, source_type, source_id) = (
        company_id.to_owned(),
        user_id.to_owned(),
        source_type.to_owned(),
        source_id.to_owned(),
    );
    run(pool, move |connection| {
        Box::pin(async move {
            let mut targets =
                PgInboxTargets::with_delegate(None, None, CalendarInboxTargets::new());
            targets
                .target(connection, &company_id, &user_id, &source_type, &source_id, 0)
                .await
        })
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn invited_appointment_resolves_exact_href_and_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = composed_target(
            pool.clone(),
            "co-1",
            "user-1",
            "calendar_appointment",
            "appt-1",
        )
        .await
        .expect("invited appointment must resolve");
        assert_eq!(
            target.href.as_deref(),
            Some("/calendar?event=appointment:appt-1")
        );
        let source = target.source.expect("appointment must carry a source");
        assert_eq!(source, json!({
            "kind": "calendar_appointment",
            "id": "appt-1",
            "version": 7,
            "href": "/calendar?event=appointment:appt-1",
            "label": "Appointment",
        }));
    })
    .await;
}

#[tokio::test]
async fn appointment_href_url_encodes_the_source_id() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = composed_target(
            pool.clone(),
            "co-1",
            "user-1",
            "calendar_appointment",
            "seg/one two",
        )
        .await
        .expect("invited appointment must resolve");
        assert_eq!(
            target.href.as_deref(),
            Some("/calendar?event=appointment:seg%2Fone%20two")
        );
        let source = target.source.expect("appointment must carry a source");
        assert_eq!(source["id"], "seg/one two");
        assert_eq!(source["version"], json!(5));
        assert_eq!(
            source["href"],
            "/calendar?event=appointment:seg%2Fone%20two"
        );
    })
    .await;
}

#[tokio::test]
async fn recording_creator_without_an_attendee_row_stays_visible() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Mirrors the retained `calendarRecipient` UNION ALL branch: the
        // creator recorded the appointment even without an attendee row.
        let target = composed_target(
            pool.clone(),
            "co-1",
            "user-3",
            "calendar_appointment",
            "appt-creator",
        )
        .await
        .expect("recording creator must resolve");
        assert_eq!(
            target.href.as_deref(),
            Some("/calendar?event=appointment:appt-creator")
        );
        assert_eq!(
            target.source.expect("creator must carry a source")["version"],
            json!(3)
        );
    })
    .await;
}

#[tokio::test]
async fn uninvited_reader_and_foreign_company_see_nothing() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        assert!(
            composed_target(pool.clone(), "co-1", "user-2", "calendar_appointment", "appt-1")
                .await
                .is_none(),
            "stranger without an attendee row must stay invisible"
        );
        assert!(
            composed_target(pool.clone(), "co-2", "user-1", "calendar_appointment", "appt-1")
                .await
                .is_none(),
            "foreign company must stay invisible"
        );
        assert!(
            composed_target(
                pool.clone(),
                "co-1",
                "user-1",
                "calendar_appointment",
                "missing",
            )
            .await
            .is_none(),
            "missing appointment must stay invisible"
        );
    })
    .await;
}

#[tokio::test]
async fn removed_appointment_is_visible_without_href_or_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = composed_target(pool.clone(), "co-1", "user-1", "calendar_removed", "appt-1")
            .await
            .expect("existing appointment must resolve as removed");
        assert_eq!(target.href, None);
        assert_eq!(target.source, None);
        // Removal visibility ignores invitation, exactly like the retained read.
        let stranger =
            composed_target(pool.clone(), "co-1", "user-2", "calendar_removed", "appt-1")
                .await
                .expect("removed visibility ignores invitation");
        assert_eq!(stranger.href, None);
        assert_eq!(stranger.source, None);
        assert!(
            composed_target(pool.clone(), "co-1", "user-1", "calendar_removed", "missing")
                .await
                .is_none(),
            "missing appointment must stay invisible"
        );
    })
    .await;
}

#[tokio::test]
async fn unknown_sources_fail_closed_without_inventing_links() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let code = run(pool.clone(), |connection| {
            Box::pin(async move {
                let mut delegate = CalendarInboxTargets::new();
                let error = delegate
                    .target(connection, "co-1", "user-1", "future_source", "x", 0)
                    .await
                    .expect_err("unknown source must fail closed");
                Ok(error.code)
            })
        })
        .await
        .unwrap();
        assert_eq!(code, "INBOX_TARGET_SOURCE_UNSUPPORTED");
        // Through composition a retained but unowned source still reports its
        // missing owner instead of inventing a link.
        let code = run(pool.clone(), |connection| {
            Box::pin(async move {
                let mut targets =
                    PgInboxTargets::with_delegate(None, None, CalendarInboxTargets::new());
                let error = targets
                    .target(connection, "co-1", "user-1", "support_ticket", "t-1", 0)
                    .await
                    .expect_err("unowned source must fail closed");
                Ok(error.code)
            })
        })
        .await
        .unwrap();
        assert_eq!(code, "INBOX_TARGET_PORT_UNAVAILABLE");
    })
    .await;
}

#[tokio::test]
async fn port_commits_nothing_and_rolls_back_with_the_caller() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let rolled_back: Result<Option<InboxTarget>, _> =
            PgUnitOfWork::new(pool.clone())
                .run_once(TransactionOptions::default(), move |connection| {
                    Box::pin(async move {
                        let mut targets =
                            PgInboxTargets::with_delegate(None, None, CalendarInboxTargets::new());
                        let visible = targets
                            .target(
                                connection,
                                "co-1",
                                "user-1",
                                "calendar_appointment",
                                "appt-1",
                                0,
                            )
                            .await?;
                        assert!(visible.is_some());
                        Err(BusinessError::new(
                            erpx_server_shared::public::FailureKind::Conflict,
                            "SYNTHETIC_ROLLBACK",
                            "Synthetic caller failure.",
                        )
                        .into())
                    })
                })
                .await;
        assert!(matches!(
            rolled_back,
            Err(PgTransactionError::Business(ref error)) if error.code == "SYNTHETIC_ROLLBACK"
        ));
        // The read-only port staged no rows: the seed image is unchanged.
        let appointments: i64 =
            sqlx::query_scalar("SELECT count(*) FROM calendar_appointments WHERE company_id='co-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        let attendees: i64 =
            sqlx::query_scalar("SELECT count(*) FROM calendar_attendees WHERE company_id='co-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!((appointments, attendees), (3, 2));
    })
    .await;
}
