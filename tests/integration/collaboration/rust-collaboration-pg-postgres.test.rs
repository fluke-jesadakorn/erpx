//! PostgreSQL integration proof for the production collaboration owner units.
//! The fixture creates an isolated `erpx_dev_rust_*` database; the harness is
//! skipped when `ERPX_TEST_ADMIN_URL` is not configured.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::inbox::{
    InboxNotificationQuery, InboxScope, PgInboxUnit, UnavailableInboxTargets,
};
use erpx_server_collaboration::public::messaging_native::{
    Change as MessagingChange, MessagingScope, PgMessagingUnit, RoomKind, execute as send_room,
};
use erpx_server_collaboration::public::messaging_teams::{
    Change as TeamChange, MessagingTeamUnit, PgMessagingTeamUnit, execute as execute_team,
};
use erpx_server_collaboration::public::social::{
    Audience, Change, FeedFilter, PgSocialUnit, SocialScope, UnavailableSocialRecords, execute,
    lock_social_sources,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CommandKey};

type Error = PgTransactionError<BusinessError>;

fn configured() -> bool {
    std::env::var("ERPX_TEST_ADMIN_URL").is_ok()
}

async fn setup(pool: &DatabasePool, now: i64) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) \
         VALUES('*','collaboration_pg','Collaboration pg',true,1,'active')",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','collaboration_pg',1,'[]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    for (id, user, branch) in [
        ("grant-author", "author", "branch"),
        ("grant-other", "other", "branch"),
        ("grant-reader", "reader", "branch"),
    ] {
        sqlx::query(
            "INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,version,scope_kind,scope_node_id) \
             VALUES($1,$2,'collaboration_pg','[\"collaboration\"]','company',$3,$4,$5,1,'branch',$3)",
        )
        .bind(id)
        .bind(user)
        .bind(branch)
        .bind(now - 60_000)
        .bind(now + 3_600_000)
        .execute(pool.raw())
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO hr_people(id,user_id,company_id,legal_name,preferred_name,country_code,status,created_by,created_at,updated_at) \
         VALUES('person-other','other','company','Other Person','Other',  'TH','active','fixture',$1,$1)",
    )
    .bind(now)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inbox_events(id,company_id,event_key,source_type,source_id,source_version,category,preview,created_at) \
         VALUES('event-1','company','notice:notice-1','notice','notice-1',1,'notice','Quarterly notice',$1)",
    )
    .bind(now)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inbox_deliveries(id,event_id,company_id,user_id,delivered_at) \
         VALUES('delivery-1','event-1','company','author',$1)",
    )
    .bind(now)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inbox_notices(id,company_id,sender_user_id,title,body,audience_json,recipient_count,created_at) \
         VALUES('notice-1','company','author','Quarterly notice','Body','{\"kind\":\"company\"}',1,$1)",
    )
    .bind(now)
    .execute(pool.raw())
    .await
    .unwrap();
}

fn assert_team_unit<T: MessagingTeamUnit>() {}

#[tokio::test]
async fn collaboration_pg_units_mount_over_real_owner_rows() {
    assert_team_unit::<PgMessagingTeamUnit<'static>>();
    if !configured() {
        eprintln!("skipping collaboration_pg_postgres: ERPX_TEST_ADMIN_URL is not configured");
        return;
    }
    fixture::with_database(3, |pool| async move {
        let now: i64 =
            sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        setup(&pool, now).await;

        let key = CommandKey::new("social-pg-0001").unwrap();
        let scope = SocialScope::new("company", "author", "grant-author", "Author", now)
            .unwrap()
            .branch(Some("branch"))
            .unwrap()
            .share_company(true);
        let change = Change::PublishPost {
            body: "Hello #pg".into(),
            audience: Audience::Branch,
            tags: vec![],
            mentions: vec![erpx_server_collaboration::public::social::MentionInput {
                kind: erpx_server_collaboration::public::social::MentionKind::Person,
                target_id: "other".into(),
            }],
            linked_record: None,
        };
        let publish = {
            let pool = pool.clone();
            let key = key.clone();
            let change = change.clone();
            let scope = scope.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |connection| {
                    let key = key.clone();
                    let change = change.clone();
                    let scope = scope.clone();
                    Box::pin(async move {
                        lock_social_sources(connection).await?;
                        let mut records = UnavailableSocialRecords;
                        let mut unit = PgSocialUnit::new(connection, scope, key.clone(), &mut records);
                        execute(&mut unit, &key, change).await
                    })
                })
                .await
                .unwrap()
        };
        assert_eq!(publish.id, "post-social-pg-0001");
        assert_eq!(publish.status, "published");

        let replay = {
            let pool = pool.clone();
            let key = key.clone();
            let change = change.clone();
            let scope = scope.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |connection| {
                    let key = key.clone();
                    let change = change.clone();
                    let scope = scope.clone();
                    Box::pin(async move {
                        lock_social_sources(connection).await?;
                        let mut records = UnavailableSocialRecords;
                        let mut unit = PgSocialUnit::new(connection, scope, key.clone(), &mut records);
                        execute(&mut unit, &key, change).await
                    })
                })
                .await
                .unwrap()
        };
        assert_eq!(replay, publish);

        let feed = {
            let pool = pool.clone();
            let scope = scope.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |connection| {
                    let scope = scope.clone();
                    Box::pin(async move {
                        let mut records = UnavailableSocialRecords;
                        let mut unit = PgSocialUnit::new(
                            connection,
                            scope,
                            CommandKey::new("social-pg-read").unwrap(),
                            &mut records,
                        );
                        unit.feed(FeedFilter::All, None, 30).await
                    })
                })
                .await
                .unwrap()
        };
        assert_eq!(feed.len(), 1);
        assert_eq!(feed[0].topics, vec!["pg".to_owned()]);

        let counts: (i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM social_posts),(SELECT count(*) FROM social_mentions),\
             (SELECT count(*) FROM command_receipts),(SELECT count(*) FROM audit_events),(SELECT count(*) FROM outbox_events)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 1, 1, 1));

        let denied_record = {
            let pool = pool.clone();
            let scope = scope.clone();
            let key = CommandKey::new("social-pg-record-0001").unwrap();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |connection| {
                    let scope = scope.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        lock_social_sources(connection).await?;
                        let mut records = UnavailableSocialRecords;
                        let mut unit = PgSocialUnit::new(connection, scope, key.clone(), &mut records);
                        execute(
                            &mut unit,
                            &key,
                            Change::PublishPost {
                                body: "Record mention".into(),
                                audience: Audience::Branch,
                                tags: vec![],
                                mentions: vec![
                                    erpx_server_collaboration::public::social::MentionInput {
                                        kind:
                                            erpx_server_collaboration::public::social::MentionKind::Record,
                                        target_id: "people_payroll/leave-1".into(),
                                    },
                                ],
                                linked_record: None,
                            },
                        )
                        .await
                    })
                })
                .await
        };
        assert!(matches!(
            denied_record,
            Err(PgTransactionError::Business(error))
                if error.code == "SOCIAL_RECORD_PORT_UNAVAILABLE"
        ));

        let room_key = CommandKey::new("messaging-pg-0001").unwrap();
        let messaging_scope =
            MessagingScope::new("company", "author", "grant-author", "Author", now).unwrap();
        let room = {
            let pool = pool.clone();
            let key = room_key.clone();
            let scope = messaging_scope.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |connection| {
                    let key = key.clone();
                    let scope = scope.clone();
                    Box::pin(async move {
                        let mut unit = PgMessagingUnit::new(connection, scope, key.clone());
                        send_room(
                            &mut unit,
                            &key,
                            MessagingChange::CreateRoom {
                                kind: RoomKind::Direct,
                                name: String::new(),
                                members: vec!["other".into()],
                            },
                        )
                        .await
                    })
                })
                .await
                .unwrap()
        };
        assert!(room.id.starts_with("dm-"));

        let send_key = CommandKey::new("messaging-pg-0002").unwrap();
        let sent = {
            let pool = pool.clone();
            let key = send_key.clone();
            let scope = messaging_scope.clone();
            let room_id = room.id.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |connection| {
                    let key = key.clone();
                    let scope = scope.clone();
                    let room_id = room_id.clone();
                    Box::pin(async move {
                        let mut unit = PgMessagingUnit::new(connection, scope, key.clone());
                        send_room(
                            &mut unit,
                            &key,
                            MessagingChange::SendMessage {
                                room_id,
                                body: "Hello".into(),
                            },
                        )
                        .await
                    })
                })
                .await
                .unwrap()
        };
        assert_eq!(sent.seq, 1);

        let closed = {
            let pool = pool.clone();
            let scope = messaging_scope.clone();
            let room_id = room.id.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |connection| {
                    let scope = scope.clone();
                    let room_id = room_id.clone();
                    Box::pin(async move {
                        let mut unit = PgMessagingUnit::new(
                            connection,
                            scope,
                            CommandKey::new("messaging-pg-files").unwrap(),
                        );
                        let headers =
                            erpx_server_runtime::public::http_support::HeaderMap::new();
                        match unit
                            .bind_attachment(
                                "acme.example.test",
                                "acme",
                                &headers,
                                &room_id,
                                &["set-1".into()],
                                None,
                                None,
                            )
                            .await
                        {
                            Err(PgTransactionError::Business(error))
                                if error.code == "FILES_PORT_UNAVAILABLE" =>
                            {
                                Ok::<(), PgTransactionError<BusinessError>>(())
                            }
                            other => panic!("attachments must fail closed: {other:?}"),
                        }
                    })
                })
                .await
                .unwrap()
        };
        assert_eq!(closed, ());

        let team_key = CommandKey::new("messaging-team-0001").unwrap();
        let team = {
            let pool = pool.clone();
            let key = team_key.clone();
            let scope = messaging_scope.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |connection| {
                    let key = key.clone();
                    let scope = scope.clone();
                    Box::pin(async move {
                        let mut unit = PgMessagingTeamUnit::new(connection, scope, key.clone());
                        execute_team(
                            &mut unit,
                            &key,
                            TeamChange::CreateTeam {
                                name: "Ops".into(),
                                members: vec!["other".into()],
                            },
                        )
                        .await
                    })
                })
                .await
                .unwrap()
        };
        assert_eq!(team.id, "team-messaging-team-0001");

        let channels = {
            let pool = pool.clone();
            let scope = messaging_scope.clone();
            let team_id = team.id.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |connection| {
                    let scope = scope.clone();
                    let team_id = team_id.clone();
                    Box::pin(async move {
                        let mut unit = PgMessagingTeamUnit::read_only(connection, scope);
                        unit.channels(&team_id, 10).await
                    })
                })
                .await
                .unwrap()
        };
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].name, "General");

        let inbox = {
            let pool = pool.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |connection| {
                    Box::pin(async move {
                        let scope =
                            InboxScope::new("company", "author", "grant-author", now).unwrap();
                        let mut targets = UnavailableInboxTargets;
                        let mut unit = PgInboxUnit::new(connection, scope, &mut targets);
                        let summary = unit.summary().await?;
                        let page = unit
                            .notifications(InboxNotificationQuery {
                                view: Some("unread".into()),
                                category: None,
                                search: String::new(),
                                cursor: None,
                                limit: 10,
                            })
                            .await?;
                        let notices = unit.notices().await?;
                        let notice = unit.notice("notice-1").await?;
                        let people = unit.people("").await?;
                        Ok::<_, Error>((summary, page, notices, notice, people))
                    })
                })
                .await
                .unwrap()
        };
        assert_eq!(inbox.0.unread, 1);
        assert_eq!(inbox.0.categories.notice, 1);
        assert_eq!(inbox.1.items.len(), 1);
        assert_eq!(inbox.1.items[0].source_type, "notice");
        assert_eq!(inbox.2.len(), 1);
        assert_eq!(inbox.2[0]["id"], "notice-1");
        assert_eq!(inbox.3.as_ref().unwrap()["completedCount"], 1);
        assert_eq!(inbox.4.len(), 1);

        sqlx::query(
            "INSERT INTO inbox_events(id,company_id,event_key,source_type,source_id,source_version,category,preview,created_at) \
             VALUES('event-2','company','support:1','support_ticket','ticket-1',1,'work','Ticket assigned',$1)",
        )
        .bind(now + 1)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO inbox_deliveries(id,event_id,company_id,user_id,delivered_at) \
             VALUES('delivery-2','event-2','company','author',$1)",
        )
        .bind(now + 1)
        .execute(pool.raw())
        .await
        .unwrap();
        let missing_target = {
            let pool = pool.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |connection| {
                    Box::pin(async move {
                        let scope =
                            InboxScope::new("company", "author", "grant-author", now).unwrap();
                        let mut targets = UnavailableInboxTargets;
                        let mut unit = PgInboxUnit::new(connection, scope, &mut targets);
                        unit.notifications(InboxNotificationQuery {
                            view: Some("all".into()),
                            category: None,
                            search: String::new(),
                            cursor: None,
                            limit: 10,
                        })
                        .await
                    })
                })
                .await
        };
        assert!(matches!(
            missing_target,
            Err(PgTransactionError::Business(error))
                if error.code == "INBOX_TARGET_PORT_UNAVAILABLE"
        ));
    })
    .await;
}
