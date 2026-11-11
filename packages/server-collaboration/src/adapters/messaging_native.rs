//! Messaging owner rows. No IAM/People query is made here; composition supplies
//! current Company membership and the stable direct-conversation key.
use crate::domain::messaging_native::{
    HistoryCursor, MessagingActor, OwnerPlan, Receipt, RoomKind, RoomSnapshot, plan_receipt,
    require_member, room_not_found,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, FailureKind};
use serde::Serialize;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("MESSAGING_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as(
        "SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text",
    )
    .fetch_one(c)
    .await?)
}

pub async fn lock_messaging_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("MESSAGING_TRANSACTION_REQUIRED").into());
    }
    sqlx::query(
        "LOCK TABLE messaging_rooms,messaging_room_members,messaging_messages,messaging_reads IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "MESSAGING_SOURCE_CHANGED",
        "The conversation changed. Refresh and retry.",
    )
    .into()
}

async fn room_members(c: &mut PgConnection, id: &str) -> Result<Vec<String>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT user_id FROM messaging_room_members WHERE room_id=$1 ORDER BY user_id",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?)
}

async fn team_members(c: &mut PgConnection, id: &str) -> Result<Vec<String>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT user_id FROM messaging_team_members WHERE team_id=$1 ORDER BY user_id",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?)
}

pub async fn read_room(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<RoomSnapshot>, Error> {
    let row = sqlx::query(
        "SELECT id,company_id,kind,name,owner_id,version,last_seq,created_at FROM messaging_rooms WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let kind = RoomKind::parse(&row.try_get::<String, _>(2)?)?;
    // Public team channels take membership from the owning team; private
    // channels and direct/group rooms use explicit room membership.
    let members = match kind {
        RoomKind::Channel => {
            let (team_id, private): (String, bool) =
                sqlx::query_as("SELECT team_id,private FROM messaging_rooms WHERE id=$1")
                    .bind(id)
                    .fetch_one(&mut *c)
                    .await?;
            if private {
                room_members(c, id).await?
            } else {
                team_members(c, &team_id).await?
            }
        }
        _ => room_members(c, id).await?,
    };
    Ok(Some(RoomSnapshot {
        id: row.try_get(0)?,
        company_id: row.try_get(1)?,
        kind,
        name: row.try_get(3)?,
        owner_id: row.try_get(4)?,
        version: row.try_get(5)?,
        last_seq: row.try_get(6)?,
        members,
        created_at: row.try_get(7)?,
    }))
}

pub async fn read_message(
    c: &mut PgConnection,
    id: &str,
) -> Result<Option<crate::domain::messaging_native::MessageSnapshot>, Error> {
    let row = sqlx::query(
        "SELECT id,room_id,seq,sender_id,sender_name,body,created_at FROM messaging_messages WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(&mut *c)
    .await?;
    row.map(|row| {
        Ok(crate::domain::messaging_native::MessageSnapshot {
            id: row.try_get(0)?,
            room_id: row.try_get(1)?,
            seq: row.try_get(2)?,
            sender_id: row.try_get(3)?,
            sender_name: row.try_get(4)?,
            body: row.try_get(5)?,
            created_at: row.try_get(6)?,
        })
    })
    .transpose()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageRow {
    pub id: String,
    pub room_id: String,
    pub seq: i64,
    pub sender_id: String,
    pub sender_name: String,
    pub body: String,
    pub created_at: i64,
}

fn message_row(row: sqlx::postgres::PgRow) -> Result<MessageRow, sqlx::Error> {
    Ok(MessageRow {
        id: row.try_get(0)?,
        room_id: row.try_get(1)?,
        seq: row.try_get(2)?,
        sender_id: row.try_get(3)?,
        sender_name: row.try_get(4)?,
        body: row.try_get(5)?,
        created_at: row.try_get(6)?,
    })
}

pub async fn read_history(
    c: &mut PgConnection,
    actor: &MessagingActor,
    room_id: &str,
    before: Option<HistoryCursor>,
    after: Option<HistoryCursor>,
    limit: usize,
) -> Result<Vec<MessageRow>, Error> {
    let room = read_room(c, &CompanyId::new(actor.company_id.clone())?, room_id)
        .await?
        .ok_or_else(room_not_found)?;
    require_member(actor, &room)?;
    if before.is_some() && after.is_some() {
        return Err(BusinessError::invalid("CURSOR_INVALID", "Choose one message cursor.").into());
    }
    let limit = limit.clamp(1, 100) as i64;
    let reverse = after.is_none();
    let mut rows = if let Some(before) = before {
        sqlx::query(
            "SELECT id,room_id,seq,sender_id,sender_name,body,created_at FROM messaging_messages WHERE room_id=$1 AND seq<$2 ORDER BY seq DESC LIMIT $3",
        )
        .bind(room_id)
        .bind(before.seq)
        .bind(limit)
        .fetch_all(&mut *c)
        .await?
    } else if let Some(after) = after {
        sqlx::query(
            "SELECT id,room_id,seq,sender_id,sender_name,body,created_at FROM messaging_messages WHERE room_id=$1 AND seq>$2 ORDER BY seq ASC LIMIT $3",
        )
        .bind(room_id)
        .bind(after.seq)
        .bind(limit)
        .fetch_all(&mut *c)
        .await?
    } else {
        sqlx::query(
            "SELECT id,room_id,seq,sender_id,sender_name,body,created_at FROM messaging_messages WHERE room_id=$1 ORDER BY seq DESC LIMIT $2",
        )
        .bind(room_id)
        .bind(limit)
        .fetch_all(&mut *c)
        .await?
    };
    if reverse {
        rows.reverse();
    }
    rows.into_iter()
        .map(message_row)
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(Into::into)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomView {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub owner_id: String,
    pub version: i64,
    pub last_seq: i64,
    pub unread: i64,
    pub members: Vec<String>,
    pub last_message_at: Option<i64>,
}

pub async fn read_overview(
    c: &mut PgConnection,
    actor: &MessagingActor,
) -> Result<Vec<RoomView>, Error> {
    let rows = sqlx::query(
        "SELECT r.id,r.kind,r.name,r.owner_id,r.version,r.last_seq,
           (SELECT count(*) FROM messaging_messages m WHERE m.room_id=r.id AND m.seq>COALESCE((SELECT mr.seq FROM messaging_reads mr WHERE mr.room_id=r.id AND mr.user_id=$2),0)) unread,
           (SELECT max(m.created_at) FROM messaging_messages m WHERE m.room_id=r.id) last_message_at
         FROM messaging_rooms r
         WHERE r.company_id=$1 AND (
           EXISTS(SELECT 1 FROM messaging_room_members rm WHERE rm.room_id=r.id AND rm.user_id=$2)
           OR (r.kind='channel' AND r.private=false AND EXISTS(SELECT 1 FROM messaging_team_members tm WHERE tm.team_id=r.team_id AND tm.user_id=$2))
         )
         ORDER BY COALESCE((SELECT max(m.created_at) FROM messaging_messages m WHERE m.room_id=r.id),r.created_at) DESC,r.id LIMIT 100",
    )
    .bind(&actor.company_id)
    .bind(&actor.user_id)
    .fetch_all(&mut *c)
    .await?;
    let mut rooms = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.try_get(0)?;
        rooms.push(RoomView {
            members: room_members(c, &id).await?,
            id,
            kind: row.try_get(1)?,
            name: row.try_get(2)?,
            owner_id: row.try_get(3)?,
            version: row.try_get(4)?,
            last_seq: row.try_get(5)?,
            unread: row.try_get(6)?,
            last_message_at: row.try_get(7)?,
        });
    }
    Ok(rooms)
}

pub async fn apply_plan(
    c: &mut PgConnection,
    actor: &MessagingActor,
    key: &CommandKey,
    plan: &OwnerPlan,
    now: i64,
) -> Result<Receipt, Error> {
    unit(c).await?;
    let expected = plan_receipt(key, plan)?;
    let result: Result<Receipt, Error> = match plan {
        OwnerPlan::CreateRoom {
            id,
            company_id,
            kind,
            name,
            owner_id,
            members,
        } => {
            sqlx::query(
                "INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,team_id,private,direct_key,version,last_seq,created_at) VALUES($1,$2,$3,$4,$5,NULL,false,$6,1,0,$7)",
            )
            .bind(id)
            .bind(company_id)
            .bind(kind.as_str())
            .bind(name)
            .bind(owner_id)
            .bind((*kind == RoomKind::Direct).then_some(id.as_str()))
            .bind(now)
            .execute(&mut *c)
            .await?;
            for member in members {
                sqlx::query("INSERT INTO messaging_room_members(room_id,user_id) VALUES($1,$2)")
                    .bind(id)
                    .bind(member)
                    .execute(&mut *c)
                    .await?;
            }
            Ok(expected.clone())
        }
        OwnerPlan::SendMessage {
            id,
            room_id,
            seq,
            sender_id,
            sender_name,
            body,
        } => {
            sqlx::query(
                "INSERT INTO messaging_messages(id,room_id,seq,sender_id,sender_name,body,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(id)
            .bind(room_id)
            .bind(seq)
            .bind(sender_id)
            .bind(sender_name)
            .bind(body)
            .bind(now)
            .execute(&mut *c)
            .await?;
            let updated = sqlx::query(
                "UPDATE messaging_rooms SET last_seq=$1 WHERE id=$2 AND company_id=$3 AND last_seq=$4",
            )
            .bind(seq)
            .bind(room_id)
            .bind(&actor.company_id)
            .bind(seq - 1)
            .execute(&mut *c)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(changed());
            }
            Ok(expected.clone())
        }
        OwnerPlan::MarkRead {
            room_id,
            user_id,
            seq,
        } => {
            sqlx::query(
                "INSERT INTO messaging_reads(room_id,user_id,seq) VALUES($1,$2,$3) ON CONFLICT(room_id,user_id) DO UPDATE SET seq=GREATEST(messaging_reads.seq,EXCLUDED.seq)",
            )
            .bind(room_id)
            .bind(user_id)
            .bind(seq)
            .execute(&mut *c)
            .await?;
            Ok(expected.clone())
        }
    };
    let receipt = result?;
    if receipt != expected {
        return Err(changed());
    }
    Ok(receipt)
}

pub struct CurrentMessagingEffects {
    unit: Unit,
    images: Vec<(String, String)>,
}
impl CurrentMessagingEffects {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        for (key, expected) in &self.images {
            if image(c, key).await?.as_deref() != Some(expected.as_str()) {
                return Err(changed());
            }
        }
        Ok(())
    }
}

async fn image(c: &mut PgConnection, key: &str) -> Result<Option<String>, Error> {
    let (table, id) = key.split_once(':').ok_or_else(changed)?;
    let sql = match table {
        "room" => "SELECT to_jsonb(p)::text FROM messaging_rooms p WHERE p.id=$1",
        "message" => "SELECT to_jsonb(p)::text FROM messaging_messages p WHERE p.id=$1",
        "read" => "SELECT to_jsonb(p)::text FROM messaging_reads p WHERE p.room_id=$1",
        _ => return Err(changed()),
    };
    Ok(sqlx::query_scalar(sql).bind(id).fetch_optional(c).await?)
}

pub async fn retain_effects(
    c: &mut PgConnection,
    plan: &OwnerPlan,
    receipt: &Receipt,
) -> Result<CurrentMessagingEffects, Error> {
    let current_unit = unit(c).await?;
    let mut images = Vec::new();
    match plan {
        OwnerPlan::CreateRoom { id, .. } => {
            if receipt.id != *id {
                return Err(changed());
            }
            let raw = image(c, &format!("room:{id}")).await?.ok_or_else(changed)?;
            images.push((format!("room:{id}"), raw));
        }
        OwnerPlan::SendMessage { id, room_id, .. } => {
            if receipt.id != *id {
                return Err(changed());
            }
            let raw = image(c, &format!("message:{id}"))
                .await?
                .ok_or_else(changed)?;
            images.push((format!("message:{id}"), raw));
            let raw = image(c, &format!("room:{room_id}"))
                .await?
                .ok_or_else(changed)?;
            images.push((format!("room:{room_id}"), raw));
        }
        OwnerPlan::MarkRead { room_id, .. } => {
            if receipt.id != *room_id {
                return Err(changed());
            }
            let raw = image(c, &format!("read:{room_id}"))
                .await?
                .ok_or_else(changed)?;
            images.push((format!("read:{room_id}"), raw));
        }
    }
    Ok(CurrentMessagingEffects {
        unit: current_unit,
        images,
    })
}
