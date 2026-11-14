//! Actual Holding PostgreSQL direct/group conversations with an explicit owner
//! receipt/audit/outbox harness. Membership below is a synthetic composition port.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::messaging_native::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{PgConnection, Row};

const NOW: i64 = 1_700_000_000_000;
type Error = PgTransactionError<BusinessError>;

fn operation(change: &Change) -> &'static str {
    match change {
        Change::CreateRoom { .. } => "messaging.room.create",
        Change::SendMessage { .. } => "messaging.send",
        Change::MarkRead { .. } => "messaging.read",
    }
}
fn record_id(key: &str, change: &Change) -> String {
    match change {
        Change::CreateRoom { .. } => format!("room-{key}"),
        Change::SendMessage { room_id, .. } => room_id.clone(),
        Change::MarkRead { room_id, .. } => room_id.clone(),
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
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
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
        assert_eq!(messages, 1);
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
