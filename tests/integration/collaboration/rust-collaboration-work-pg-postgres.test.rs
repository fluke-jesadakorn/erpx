//! PostgreSQL proof for the production Inbox destination port and the native
//! Collaboration command unit. The fixture creates an isolated
//! `erpx_dev_rust_*` database and is skipped when `ERPX_TEST_ADMIN_URL` is not
//! configured.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::collaboration_work::{
    AuthorizedActor, CardAction, Change, CollaborationScope, CollaborationSourcePort,
    PgCollaborationUnit, RelationKind, execute_scope, read_scope_card,
};
use erpx_server_collaboration::public::inbox::{
    InboxNotificationQuery, InboxScope, InboxTarget, InboxTargetFuture, InboxTargetPort,
    PgInboxTargets, PgInboxUnit,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::record_source_contract::{
    SourceKind, SourceRecord, SourceReference,
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
         VALUES('*','collaboration_pg_unit','Collaboration unit',true,1,'active')",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','collaboration_pg_unit',1,'[\"records.read\",\"records.write\",\"records.review\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) \
         VALUES('*','collaboration_pg_reader','Collaboration reader',true,1,'active')",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','collaboration_pg_reader',1,'[\"records.read\",\"records.write\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    for (assignment, user, role, branch) in [
        ("grant-owner", "owner", "collaboration_pg_unit", "branch"),
        (
            "grant-reviewer",
            "reviewer",
            "collaboration_pg_unit",
            "branch",
        ),
        (
            "grant-reader",
            "reader",
            "collaboration_pg_reader",
            "branch",
        ),
    ] {
        sqlx::query(
            "INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,version,scope_kind,scope_node_id) \
             VALUES($1,$2,$3,'[\"collaboration\"]','company',$4,$5,$6,1,'branch',$4)",
        )
        .bind(assignment)
        .bind(user)
        .bind(role)
        .bind(branch)
        .bind(now - 60_000)
        .bind(now + 3_600_000)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) \
             VALUES($1,$2,'company',$3,'TH','active','fixture',$4,$4)",
        )
        .bind(format!("person-{user}"))
        .bind(user)
        .bind(user)
        .bind(now)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO hr_engagements(id,person_id,company_id,branch_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) \
             VALUES($1,$2,'company','branch','employee','salaried','active','2025-01-01','Member','THB','fixture',$3,$3)",
        )
        .bind(format!("engagement-{user}"))
        .bind(format!("person-{user}"))
        .bind(now)
        .execute(pool.raw())
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO collaboration_action_cards(id,company_id,branch_id,department_id,source_type,source_id,source_version,title,owner_id,due_at,created_by,created_at,updated_at) \
         VALUES('card-inbox-1','company','branch',NULL,'support_ticket','ticket-1',3,'Review inbox card','owner',$1,'owner',$2,$2)",
    )
    .bind(now + 60_000)
    .bind(now)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inbox_notices(id,company_id,sender_user_id,title,body,audience_json,recipient_count,created_at) \
         VALUES('notice-inbox-1','company','owner','Notice','Body','{\"kind\":\"company\"}',1,$1)",
    )
    .bind(now)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inbox_events(id,company_id,event_key,source_type,source_id,source_version,category,preview,created_at) \
         VALUES('event-inbox-card','company','collaboration:card-inbox-1','collaboration_action','card-inbox-1',1,'work','Review inbox card',$1)",
    )
    .bind(now + 1)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inbox_deliveries(id,event_id,company_id,user_id,delivered_at) \
         VALUES('delivery-inbox-card','event-inbox-card','company','owner',$1)",
    )
    .bind(now + 1)
    .execute(pool.raw())
    .await
    .unwrap();
}

struct Sources;

impl CollaborationSourcePort for Sources {
    type Error = Error;
    type Witness = String;

    async fn prepare(
        &mut self,
        reference: &SourceReference,
    ) -> Result<(SourceRecord, String), Error> {
        Ok((
            SourceRecord {
                kind: reference.kind,
                id: reference.id.clone(),
                company_id: "company".into(),
                branch_id: Some("branch".into()),
                department_id: None,
                version: 3,
                title: "Synthetic source".into(),
                status: "open".into(),
            },
            reference.id.clone(),
        ))
    }

    async fn require_current(&mut self, _: &String) -> Result<(), Error> {
        Ok(())
    }
}

struct Delegated;

impl InboxTargetPort for Delegated {
    fn target<'a>(
        &'a mut self,
        _connection: &'a mut sqlx::PgConnection,
        _company_id: &'a str,
        _user_id: &'a str,
        source_type: &'a str,
        source_id: &'a str,
        source_version: i64,
    ) -> InboxTargetFuture<'a> {
        Box::pin(async move {
            Ok(Some(InboxTarget {
                href: Some(format!("/delegated/{source_type}/{source_id}")),
                source: Some(serde_json::json!({
                    "kind": source_type,
                    "id": source_id,
                    "version": source_version,
                    "href": format!("/delegated/{source_type}/{source_id}"),
                })),
            }))
        })
    }
}

#[tokio::test]
async fn pg_inbox_targets_resolve_owned_sources_and_fail_closed() {
    if !configured() {
        eprintln!("skipping collaboration_work_pg: ERPX_TEST_ADMIN_URL is not configured");
        return;
    }
    fixture::with_database(2, |pool| async move {
        let now: i64 =
            sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        setup(&pool, now).await;

        let (target, missing, unsupported, unknown, delegated, notice) = {
            let pool = pool.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |c| {
                    Box::pin(async move {
                        let mut targets = PgInboxTargets::new(Some("branch"), None)
                            .assignment(Some("grant-owner"));
                        let target = targets
                            .target(
                                c,
                                "company",
                                "owner",
                                "collaboration_action",
                                "card-inbox-1",
                                1,
                            )
                            .await?;
                        let missing = targets
                            .target(
                                c,
                                "company",
                                "owner",
                                "collaboration_action",
                                "card-inbox-missing",
                                1,
                            )
                            .await?;
                        let unsupported = targets
                            .target(c, "company", "owner", "support_ticket", "ticket-1", 3)
                            .await;
                        let unknown = targets
                            .target(c, "company", "owner", "future_source", "future-1", 1)
                            .await;
                        let mut delegated_targets =
                            PgInboxTargets::with_delegate(Some("branch"), None, Delegated);
                        let delegated = delegated_targets
                            .target(c, "company", "owner", "support_ticket", "ticket-1", 3)
                            .await?;
                        let mut notice_targets = PgInboxTargets::new(Some("branch"), None);
                        let notice = notice_targets
                            .target(c, "company", "owner", "notice", "notice-inbox-1", 1)
                            .await?;
                        Ok::<_, Error>((target, missing, unsupported, unknown, delegated, notice))
                    })
                })
                .await
                .unwrap()
        };
        let target = target.expect("owned collaboration action target");
        assert_eq!(target.href.as_deref(), Some("/collaboration"));
        assert_eq!(target.source.as_ref().unwrap()["version"], 1);
        assert!(missing.is_none());
        assert!(matches!(
            unsupported,
            Err(BusinessError {
                code: "INBOX_TARGET_PORT_UNAVAILABLE",
                ..
            })
        ));
        assert!(matches!(
            unknown,
            Err(BusinessError {
                code: "INBOX_TARGET_SOURCE_UNSUPPORTED",
                ..
            })
        ));
        assert_eq!(
            delegated.unwrap().href.as_deref(),
            Some("/delegated/support_ticket/ticket-1")
        );
        assert_eq!(notice.expect("notice target").href, None);

        // Branch-scoped target resolution hides a card outside the selected frame.
        let hidden = {
            let pool = pool.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |c| {
                    Box::pin(async move {
                        let mut targets = PgInboxTargets::new(Some("other-branch"), None);
                        targets
                            .target(
                                c,
                                "company",
                                "owner",
                                "collaboration_action",
                                "card-inbox-1",
                                1,
                            )
                            .await
                            .map_err(Error::from)
                    })
                })
                .await
                .unwrap()
        };
        assert!(hidden.is_none());

        let page = {
            let pool = pool.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |c| {
                    Box::pin(async move {
                        let scope =
                            InboxScope::new("company", "owner", "grant-owner", now).unwrap();
                        let mut targets = PgInboxTargets::new(Some("branch"), None)
                            .assignment(Some("grant-owner"));
                        let mut unit = PgInboxUnit::new(c, scope, &mut targets);
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
                .unwrap()
        };
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].source_type, "collaboration_action");
        assert_eq!(page.items[0].href.as_deref(), Some("/collaboration"));
        assert_eq!(
            page.items[0].source.as_ref().unwrap()["kind"],
            "collaboration_action"
        );
    })
    .await;
}

#[tokio::test]
async fn pg_collaboration_unit_applies_reads_and_replays_owner_commands() {
    if !configured() {
        eprintln!("skipping collaboration_work_pg: ERPX_TEST_ADMIN_URL is not configured");
        return;
    }
    fixture::with_database(2, |pool| async move {
        let now: i64 =
            sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        setup(&pool, now).await;
        let scope = CollaborationScope::new("company", "owner", "grant-owner", now)
            .unwrap()
            .branch(Some("branch"));
        let key = CommandKey::new("collaboration-unit-0001").unwrap();
        let change = Change::CreateCard {
            source: SourceReference {
                kind: SourceKind::FieldServiceOrder,
                id: "field-1".into(),
                expected_version: None,
            },
            title: "Review visit".into(),
            owner_id: "owner".into(),
            due_at: now + 60_000,
            reason: "Review the visit".into(),
        };
        let first = {
            let pool = pool.clone();
            let scope = scope.clone();
            let key = key.clone();
            let change = change.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |c| {
                    let scope = scope.clone();
                    let key = key.clone();
                    let change = change.clone();
                    Box::pin(async move {
                        erpx_server_collaboration::public::collaboration_work::lock_collaboration_work_sources(c).await?;
                        let mut sources = Sources;
                        execute_scope(c, scope, key, &mut sources, change).await
                    })
                })
                .await
                .unwrap()
        };
        assert_eq!(first.id, "action-card-collaboration-unit-0001");
        assert_eq!(first.version, 1);
        assert_eq!(first.status, "open");

        let replay = {
            let pool = pool.clone();
            let scope = scope.clone();
            let key = key.clone();
            let change = change.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |c| {
                    let scope = scope.clone();
                    let key = key.clone();
                    let change = change.clone();
                    Box::pin(async move {
                        erpx_server_collaboration::public::collaboration_work::lock_collaboration_work_sources(c).await?;
                        let mut sources = Sources;
                        execute_scope(c, scope, key, &mut sources, change).await
                    })
                })
                .await
                .unwrap()
        };
        assert_eq!(replay, first);

        let card = {
            let pool = pool.clone();
            let scope = scope.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |c| {
                    let scope = scope.clone();
                    Box::pin(async move {
                        let mut sources = Sources;
                        read_scope_card(c, scope, &mut sources, "action-card-collaboration-unit-0001")
                            .await
                    })
                })
                .await
                .unwrap()
        };
        assert_eq!(card.version, 1);
        assert!(card.open);
        assert_eq!(card.owner_id, "owner");
        assert!(card.owner_eligible);

        let complete_key = CommandKey::new("collaboration-unit-complete").unwrap();
        let complete = {
            let pool = pool.clone();
            let scope = scope.clone();
            let key = complete_key.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |c| {
                    let scope = scope.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        erpx_server_collaboration::public::collaboration_work::lock_collaboration_work_sources(c).await?;
                        let mut sources = Sources;
                        execute_scope(
                            c,
                            scope,
                            key,
                            &mut sources,
                            Change::ChangeCard {
                                id: "action-card-collaboration-unit-0001".into(),
                                expected_version: 1,
                                action: CardAction::Complete,
                                owner_id: None,
                                due_at: None,
                                evidence: Some("Delivered the visit evidence".into()),
                                reason: "Complete the visit review".into(),
                            },
                        )
                        .await
                    })
                })
                .await
                .unwrap()
        };
        assert_eq!(complete.version, 2);
        assert_eq!(complete.status, "completed");

        let relation = {
            let pool = pool.clone();
            let scope = scope.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |c| {
                    let scope = scope.clone();
                    Box::pin(async move {
                        let mut sources = Sources;
                        execute_scope(
                            c,
                            scope,
                            CommandKey::new("collaboration-unit-relation").unwrap(),
                            &mut sources,
                            Change::CreateRelation {
                                left: SourceReference {
                                    kind: SourceKind::FieldServiceOrder,
                                    id: "field-1".into(),
                                    expected_version: None,
                                },
                                right: SourceReference {
                                    kind: SourceKind::SupportTicket,
                                    id: "ticket-1".into(),
                                    expected_version: None,
                                },
                                relation: RelationKind::Related,
                                reason: "Same customer issue".into(),
                            },
                        )
                        .await
                    })
                })
                .await
                .unwrap()
        };
        assert_eq!(relation.id, "record-relation-collaboration-unit-relation");
        assert_eq!(relation.status, "active");

        let denied = {
            let pool = pool.clone();
            let reviewer = CollaborationScope::new("company", "reviewer", "grant-reviewer", now)
                .unwrap()
                .branch(Some("branch"));
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |c| {
                    let reviewer = reviewer.clone();
                    Box::pin(async move {
                        erpx_server_collaboration::public::collaboration_work::lock_collaboration_work_sources(c).await?;
                        let mut sources = Sources;
                        execute_scope(
                            c,
                            reviewer,
                            CommandKey::new("collaboration-unit-denied").unwrap(),
                            &mut sources,
                            Change::ChangeCard {
                                id: "action-card-collaboration-unit-0001".into(),
                                expected_version: 2,
                                action: CardAction::Complete,
                                owner_id: None,
                                due_at: None,
                                evidence: Some("Not the assigned owner".into()),
                                reason: "Attempted completion".into(),
                            },
                        )
                        .await
                    })
                })
                .await
        };
        assert!(matches!(
            denied,
            Err(PgTransactionError::Business(error))
                if error.code == "ACTION_CARD_OWNER_REQUIRED"
                || error.code == "ACTION_CARD_STATE_CHANGED"
        ));

        let counts: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM collaboration_action_cards),\
             (SELECT count(*) FROM collaboration_record_relations),\
             (SELECT count(*) FROM command_receipts),\
             (SELECT count(*) FROM outbox_events)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (2, 1, 3, 3));
    })
    .await;
}

#[test]
fn collaboration_unit_is_composition_ready() {
    fn assert_unit<S>()
    where
        S: CollaborationSourcePort<Error = Error, Witness = String> + 'static,
    {
        let _ = std::mem::size_of::<PgCollaborationUnit<'static, S>>();
    }
    assert_unit::<Sources>();
    let _ = std::mem::size_of::<PgInboxTargets>();
    let _ = AuthorizedActor {
        user_id: "user".into(),
        company_id: "company".into(),
        can_review: true,
    };
}
