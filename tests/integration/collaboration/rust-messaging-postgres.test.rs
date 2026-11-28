//! Actual Holding PostgreSQL direct/group conversations with an explicit owner
//! receipt/audit/outbox harness. Membership below is a synthetic composition port.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_collaboration_server::public::messaging_native::*;
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::{BusinessError, CommandKey, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{PgConnection, Row};

const NOW: i64 = 1_700_000_000_000;
type Error = PgTransactionError<BusinessError>;

#[tokio::test]
async fn messaging_people_identity_sources_match_assignment_and_control_facts() {
    use erpx_identity_access_server::public::messaging_people::{active_assignment_users, control_account_names};
    fixture::with_database(2, |pool| async move {
        let mut c = pool.raw().acquire().await.unwrap();
        sqlx::raw_sql("CREATE TEMP TABLE assignments(company_id text,user_id text,revoked_at bigint,valid_from bigint,valid_until bigint); CREATE TEMP TABLE \"user\"(id text,name text,email text); INSERT INTO assignments VALUES ('c','u1',NULL,10,21),('c','u1',NULL,0,30),('c','u2',NULL,10,20),('c','u3',10,0,30),('other','u4',NULL,0,30),('c','missing',NULL,0,30),('c','orphan',NULL,0,30); INSERT INTO \"user\" VALUES ('u1','Zulu','z@example.test'),('missing','Alpha','a@example.test'),('u4','Other','o@example.test');")
            .execute(&mut *c).await.unwrap();
        let company = CompanyId::new("c").unwrap();
        let mut ids = active_assignment_users(&mut c, &company, 10, 20).await.unwrap();
        ids.sort();
        assert_eq!(ids, ["missing", "orphan", "u1"]);
        let names = control_account_names(&mut c, &ids).await.unwrap();
        assert_eq!(names.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["missing", "u1"]);
        assert_eq!(names[0].email, "a@example.test");
        assert!(control_account_names(&mut c, &[]).await.unwrap().is_empty());
    }).await;
}

fn operation(change: &Change) -> &'static str {
    match change {
        Change::CreateRoom { .. } => "messaging.room.create",
        Change::SendMessage { .. } => "messaging.send",
        Change::MarkRead { .. } => "messaging.read",
        Change::ManageRoom { .. } => "messaging.manage",
    }
}
fn record_id(key: &str, change: &Change) -> String {
    match change {
        Change::CreateRoom { .. } => format!("room-{key}"),
        Change::SendMessage { room_id, .. } => room_id.clone(),
        Change::MarkRead { room_id, .. } => room_id.clone(),
        Change::ManageRoom { room_id, .. } => room_id.clone(),
    }
}
fn fingerprint(change: &Change) -> String {
    format!("{change:?}")
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "MESSAGING_SOURCE_CHANGED",
        "Synthetic current source changed.",
    )
    .into()
}
fn decode(raw: &str) -> Result<Receipt, Error> {
    let value: Value = serde_json::from_str(raw).map_err(|_| changed())?;
    let status = match value["status"].as_str() {
        Some("created") => "created",
        Some("sent") => "sent",
        Some("read") => "read",
        _ => return Err(changed()),
    };
    Ok(Receipt {
        id: value["id"].as_str().ok_or_else(changed)?.into(),
        version: value["version"].as_i64().ok_or_else(changed)?,
        status,
        seq: value["seq"].as_i64().ok_or_else(changed)?,
    })
}

async fn membership(c: &mut PgConnection, user: &str, company: &str) -> Result<String, Error> {
    let row = sqlx::query_scalar::<_, String>(
        "SELECT display_name FROM messaging_test_members WHERE company_id=$1 AND user_id=$2",
    )
    .bind(company)
    .bind(user)
    .fetch_optional(&mut *c)
    .await?;
    row.ok_or_else(|| {
        BusinessError::new(
            FailureKind::Forbidden,
            "MEMBERSHIP_REQUIRED",
            "Current company membership is required.",
        )
        .into()
    })
}
fn actor(user: &str, company: &str) -> MessagingActor {
    MessagingActor {
        user_id: user.into(),
        display_name: format!("{user} name"),
        company_id: company.into(),
    }
}

struct Harness<'a> {
    c: &'a mut PgConnection,
    user: String,
    company: String,
    key: CommandKey,
    operation: String,
    record_id: String,
    fingerprint: String,
    retained: Option<CurrentMessagingEffects>,
    tamper: bool,
}
impl Harness<'_> {
    async fn receipt(&mut self) -> Result<Option<Receipt>, Error> {
        let row = sqlx::query(
            "SELECT operation,actor_id,record_id,request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR SHARE",
        )
        .bind(self.key.as_str())
        .fetch_optional(&mut *self.c)
        .await?;
        let Some(row) = row else { return Ok(None) };
        if row.try_get::<String, _>(0)? != self.operation
            || row.try_get::<String, _>(1)? != self.user
            || row.try_get::<String, _>(2)? != self.record_id
            || row.try_get::<String, _>(3)? != self.fingerprint
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to another request.",
            )
            .into());
        }
        decode(&row.try_get::<String, _>(4)?).map(Some)
    }
    async fn write(&mut self, receipt: &Receipt) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(self.key.as_str())
        .bind(&self.operation)
        .bind(&self.user)
        .bind(&self.fingerprint)
        .bind(&self.record_id)
        .bind(serde_json::to_string(receipt).map_err(|_| changed())?)
        .bind(NOW)
        .execute(&mut *self.c)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES($1,$2,$3,$4,$5,'messaging_room',$6,$7,'messaging',$8)",
        )
        .bind(format!("audit-{}", self.key.as_str()))
        .bind(self.key.as_str())
        .bind(&self.user)
        .bind(format!("{}-assignment", self.user))
        .bind(&self.operation)
        .bind(&self.record_id)
        .bind(receipt.version)
        .bind(NOW)
        .execute(&mut *self.c)
        .await?;
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$2,$3,'messaging_room',$4,$5,$6,$7)",
        )
        .bind(format!("outbox-{}", self.key.as_str()))
        .bind(self.key.as_str())
        .bind(format!("{}.changed", self.operation))
        .bind(&self.record_id)
        .bind(receipt.version)
        .bind(serde_json::to_string(receipt).map_err(|_| changed())?)
        .bind(NOW)
        .execute(&mut *self.c)
        .await?;
        Ok(())
    }
}
impl MessagingUnit for Harness<'_> {
    type Error = Error;
    async fn authorize(&mut self, _: Access) -> Result<MessagingActor, Error> {
        let display_name = membership(self.c, &self.user, &self.company).await?;
        Ok(MessagingActor {
            user_id: self.user.clone(),
            display_name,
            company_id: self.company.clone(),
        })
    }
    async fn replay(&mut self, _: &Change) -> Result<Option<Receipt>, Error> {
        self.receipt().await
    }
    async fn room(&mut self, id: &str) -> Result<Option<RoomSnapshot>, Error> {
        read_room(self.c, &CompanyId::new(self.company.clone())?, id).await
    }
    async fn is_company_member(&mut self, user_id: &str) -> Result<bool, Error> {
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM messaging_test_members WHERE company_id=$1 AND user_id=$2)",
        )
        .bind(&self.company)
        .bind(user_id)
        .fetch_one(&mut *self.c)
        .await?)
    }
    async fn direct_room_id(&mut self, members: &[String]) -> Result<String, Error> {
        Ok(format!("dm-{}", members.join("-")))
    }
    async fn apply(
        &mut self,
        actor: &MessagingActor,
        key: &CommandKey,
        plan: OwnerPlan,
    ) -> Result<Receipt, Error> {
        let receipt = apply_plan(self.c, actor, key, &plan, NOW).await?;
        self.retained = Some(retain_effects(self.c, &plan, &receipt).await?);
        self.write(&receipt).await?;
        Ok(receipt)
    }
    async fn flush_deferred(&mut self) -> Result<(), Error> {
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *self.c)
            .await?;
        Ok(())
    }
    async fn require_unit_current(&mut self) -> Result<(), Error> {
        membership(self.c, &self.user, &self.company).await?;
        if let Some(retained) = &self.retained {
            retained.require_current(self.c).await?;
        }
        if self.receipt().await?.is_none() || self.tamper {
            return Err(changed());
        }
        Ok(())
    }
}

async fn run(
    pool: &DatabasePool,
    user: &str,
    company: &str,
    key: &str,
    change: Change,
    tamper: bool,
) -> Result<Receipt, Error> {
    let key = CommandKey::new(key)?;
    let operation = operation(&change).to_owned();
    let record_id = record_id(key.as_str(), &change);
    let fingerprint = fingerprint(&change);
    let user = user.to_owned();
    let company = company.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let key = key.clone();
            let change = change.clone();
            let user = user.clone();
            let company = company.clone();
            let operation = operation.clone();
            let record_id = record_id.clone();
            let fingerprint = fingerprint.clone();
            Box::pin(async move {
                lock_messaging_sources(c).await?;
                let mut harness = Harness {
                    c,
                    user,
                    company,
                    key: key.clone(),
                    operation,
                    record_id,
                    fingerprint,
                    retained: None,
                    tamper,
                };
                execute(&mut harness, &key, change).await
            })
        })
        .await
}

async fn setup(pool: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "CREATE TABLE messaging_test_members(company_id text NOT NULL,user_id text NOT NULL,display_name text NOT NULL,PRIMARY KEY(company_id,user_id));
         INSERT INTO messaging_test_members VALUES
           ('company','author','Author'),('company','person-1','Person One'),('company','person-2','Person Two'),
           ('other','other-author','Other Author');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn direct_thread_messages_reads_replay_and_company_isolation() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let room = run(
            &pool,
            "author",
            "company",
            "messaging-room-0001",
            Change::CreateRoom {
                kind: RoomKind::Direct,
                name: "ignored".into(),
                members: vec!["person-1".into()],
            },
            false,
        )
        .await
        .unwrap();
        assert_eq!(room.status, "created");
        assert_eq!(room.id, "dm-author-person-1");
        let send = run(
            &pool,
            "author",
            "company",
            "messaging-message-0001",
            Change::SendMessage {
                room_id: room.id.clone(),
                body: "  hello  ".into(),
            },
            false,
        )
        .await
        .unwrap();
        assert_eq!((send.status, send.seq), ("sent", 1));
        let replay = run(
            &pool,
            "author",
            "company",
            "messaging-message-0001",
            Change::SendMessage {
                room_id: room.id.clone(),
                body: "  hello  ".into(),
            },
            false,
        )
        .await
        .unwrap();
        assert_eq!(replay, send);
        let counts: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM messaging_rooms),(SELECT count(*) FROM messaging_room_members),(SELECT count(*) FROM messaging_messages),(SELECT count(*) FROM command_receipts)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 2, 1, 2));

        let mut tx = pool.raw().begin().await.unwrap();
        let history = read_history(
            &mut tx,
            &actor("person-1", "company"),
            &room.id,
            None,
            None,
            30,
        )
        .await
        .unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].body, "hello");
        assert_eq!(history[0].seq, 1);
        tx.rollback().await.unwrap();

        let mut tx = pool.raw().begin().await.unwrap();
        let overview = read_overview(&mut tx, &actor("person-1", "company"))
            .await
            .unwrap();
        assert_eq!(overview.len(), 1);
        assert_eq!(overview[0].unread, 1);
        assert_eq!(overview[0].team_id, None);
        assert_eq!(overview[0].private, false);
        assert_eq!(overview[0].preview.as_deref(), Some("hello"));
        assert_eq!(overview[0].updated_at, overview[0].last_message_at);
        assert!(overview[0].updated_at.is_some());
        tx.rollback().await.unwrap();
        run(
            &pool,
            "person-1",
            "company",
            "messaging-read-0001",
            Change::MarkRead {
                room_id: room.id.clone(),
                seq: 1,
            },
            false,
        )
        .await
        .unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        let overview = read_overview(&mut tx, &actor("person-1", "company"))
            .await
            .unwrap();
        assert_eq!(overview[0].unread, 0);
        tx.rollback().await.unwrap();
        let own = run(
            &pool,
            "person-1",
            "company",
            "messaging-message-own",
            Change::SendMessage {
                room_id: room.id.clone(),
                body: "my own message".into(),
            },
            false,
        )
        .await
        .unwrap();
        assert_eq!((own.status, own.seq), ("sent", 2));
        let mut tx = pool.raw().begin().await.unwrap();
        let overview = read_overview(&mut tx, &actor("person-1", "company"))
            .await
            .unwrap();
        assert_eq!(overview[0].unread, 0, "own messages never count as unread");
        tx.rollback().await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        assert!(
            read_history(
                &mut tx,
                &actor("other-author", "other"),
                &room.id,
                None,
                None,
                30,
            )
            .await
            .is_err()
        );
        tx.rollback().await.unwrap();
        let denied = run(
            &pool,
            "person-2",
            "company",
            "messaging-message-0002",
            Change::SendMessage {
                room_id: room.id.clone(),
                body: "not a member".into(),
            },
            false,
        )
        .await
        .unwrap_err();
        assert!(matches!(denied, PgTransactionError::Business(e) if e.code == "ROOM_MEMBER_REQUIRED"));
        let messages: i64 = sqlx::query_scalar("SELECT count(*) FROM messaging_messages")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(messages, 2);
    })
    .await;
}

#[tokio::test]
async fn group_thread_order_tamper_rollback_and_participant_authority() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let room = run(
            &pool,
            "author",
            "company",
            "messaging-room-0101",
            Change::CreateRoom {
                kind: RoomKind::Group,
                name: "  Operations  ".into(),
                members: vec!["person-1".into(), "person-2".into()],
            },
            false,
        )
        .await
        .unwrap();
        assert_eq!(room.id, "room-messaging-room-0101");
        for (key, body) in [("messaging-message-0101", "one"), ("messaging-message-0102", "two")] {
            run(
                &pool,
                "author",
                "company",
                key,
                Change::SendMessage {
                    room_id: room.id.clone(),
                    body: body.into(),
                },
                false,
            )
            .await
            .unwrap();
        }
        let mut tx = pool.raw().begin().await.unwrap();
        let history = read_history(
            &mut tx,
            &actor("person-2", "company"),
            &room.id,
            None,
            None,
            30,
        )
        .await
        .unwrap();
        assert_eq!(
            history.iter().map(|row| row.body.as_str()).collect::<Vec<_>>(),
            vec!["one", "two"]
        );
        let after = read_history(
            &mut tx,
            &actor("person-2", "company"),
            &room.id,
            None,
            Some(HistoryCursor { seq: 1 }),
            30,
        )
        .await
        .unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].body, "two");
        assert!(
            read_history(
                &mut tx,
                &actor("other-author", "other"),
                &room.id,
                None,
                None,
                30,
            )
            .await
            .is_err()
        );
        tx.rollback().await.unwrap();

        let refused = run(
            &pool,
            "author",
            "company",
            "messaging-room-0102",
            Change::CreateRoom {
                kind: RoomKind::Group,
                name: "External".into(),
                members: vec!["outsider".into()],
            },
            false,
        )
        .await
        .unwrap_err();
        assert!(matches!(refused, PgTransactionError::Business(e) if e.code == "MESSAGING_MEMBER_REQUIRED"));
        let before: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM messaging_messages),(SELECT count(*) FROM command_receipts)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let tampered = run(
            &pool,
            "author",
            "company",
            "messaging-message-0103",
            Change::SendMessage {
                room_id: room.id.clone(),
                body: "tampered".into(),
            },
            true,
        )
        .await;
        assert!(matches!(tampered, Err(PgTransactionError::Business(e)) if e.code == "MESSAGING_SOURCE_CHANGED"));
        let after: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM messaging_messages),(SELECT count(*) FROM command_receipts)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(before, after);
    })
    .await;
}

#[tokio::test]
async fn overview_includes_organization_channel_for_node_members_only() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut c = pool.raw().acquire().await.unwrap();
        sqlx::raw_sql(
            "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES \
             ('company','company','company','Company',NULL,'company',NULL,'active'), \
             ('node-1','company','announcements','Announcements',NULL,'company',NULL,'active'), \
             ('node-2','company','suspended','Suspended',NULL,'company',NULL,'suspended'); \
             INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES \
             ('person-org','org-user','company','Org User','TH','seed',1,1), \
             ('person-rev','revoked-user','company','Revoked User','TH','seed',1,1); \
             INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES \
             ('eng-org','person-org','company','employee','salaried','active','2020-01-01','Member','THB','seed',1,1), \
             ('eng-rev','person-rev','company','employee','salaried','active','2020-01-01','Member','THB','seed',1,1); \
             INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
             ('*','employee','Employee',true,1,'active') \
             ON CONFLICT(company_id,id) DO UPDATE SET published_revision=excluded.published_revision,status='active'; \
             INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
             ('*','employee',1,'[\"iam.self\"]','[]','[\"holding\",\"branch\",\"department\"]','system',0) \
             ON CONFLICT(company_id,role_id,revision) DO NOTHING; \
             INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,revoked_at) VALUES \
             ('assign-org','org-user','employee','{}','company',0,9999999999999,NULL), \
             ('assign-rev','revoked-user','employee','{}','company',0,9999999999999,1); \
             INSERT INTO messaging_teams(id,company_id,name,owner_id,created_at) VALUES ('team-org','company','Org','org-user',1700000000000); \
             INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,team_id,private,version,last_seq,created_at,organization_node_id) VALUES \
             ('room-org','company','channel','Announcements','org-user','team-org',false,1,0,1700000000000,'node-1'), \
             ('room-suspended','company','channel','Suspended','org-user','team-org',false,1,0,1700000000000,'node-2');",
        )
        .execute(&mut *c)
        .await
        .unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        let overview = read_overview(&mut tx, &actor("org-user", "company"))
            .await
            .unwrap();
        assert_eq!(overview.len(), 1);
        assert_eq!(overview[0].id, "room-org");
        assert_eq!(overview[0].private, false);
        assert_eq!(overview[0].preview, None);
        assert_eq!(overview[0].updated_at, Some(1700000000000));
        tx.rollback().await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        let hidden = read_overview(&mut tx, &actor("person-2", "company"))
            .await
            .unwrap();
        assert!(hidden.is_empty());
        tx.rollback().await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        let revoked = read_overview(&mut tx, &actor("revoked-user", "company"))
            .await
            .unwrap();
        assert!(revoked.is_empty());
        tx.rollback().await.unwrap();
    })
    .await;
}
