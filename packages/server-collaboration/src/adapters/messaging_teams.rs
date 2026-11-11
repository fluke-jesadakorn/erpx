//! Messaging team/channel owner rows. No IAM query is made here; composition
//! supplies current Company membership and team ownership is stored on the row.
use crate::domain::messaging_teams::{
    ChannelSnapshot, OwnerPlan, Receipt, TeamSnapshot, plan_receipt,
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

pub async fn lock_messaging_team_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("MESSAGING_TRANSACTION_REQUIRED").into());
    }
    sqlx::query(
        "LOCK TABLE messaging_teams,messaging_team_members,messaging_rooms,messaging_room_members IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "MESSAGING_SOURCE_CHANGED",
        "The team changed. Refresh and retry.",
    )
    .into()
}

async fn team_members(c: &mut PgConnection, id: &str) -> Result<Vec<String>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT user_id FROM messaging_team_members WHERE team_id=$1 ORDER BY user_id",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?)
}

async fn room_members(c: &mut PgConnection, id: &str) -> Result<Vec<String>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT user_id FROM messaging_room_members WHERE room_id=$1 ORDER BY user_id",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?)
}

pub async fn read_team(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<TeamSnapshot>, Error> {
    let row = sqlx::query(
        "SELECT id,company_id,name,owner_id,version,created_at FROM messaging_teams WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    Ok(Some(TeamSnapshot {
        id: row.try_get(0)?,
        company_id: row.try_get(1)?,
        name: row.try_get(2)?,
        owner_id: row.try_get(3)?,
        version: row.try_get(4)?,
        members: team_members(c, id).await?,
        created_at: row.try_get(5)?,
    }))
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelView {
    pub id: String,
    pub team_id: String,
    pub name: String,
    pub owner_id: String,
    pub version: i64,
    pub last_seq: i64,
    pub private: bool,
    pub members: Vec<String>,
    pub created_at: i64,
}

async fn channel(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<ChannelSnapshot>, Error> {
    let row = sqlx::query(
        "SELECT id,company_id,team_id,name,owner_id,version,last_seq,private,created_at FROM messaging_rooms WHERE id=$1 AND company_id=$2 AND kind='channel'",
    )
    .bind(id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let private: bool = row.try_get(7)?;
    let team_id: String = row.try_get(2)?;
    let members = if private {
        room_members(c, id).await?
    } else {
        team_members(c, &team_id).await?
    };
    Ok(Some(ChannelSnapshot {
        id: row.try_get(0)?,
        company_id: row.try_get(1)?,
        team_id,
        name: row.try_get(3)?,
        owner_id: row.try_get(4)?,
        version: row.try_get(5)?,
        last_seq: row.try_get(6)?,
        private,
        members,
        created_at: row.try_get(8)?,
    }))
}

pub async fn read_channel(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<ChannelSnapshot>, Error> {
    channel(c, company, id).await
}

pub async fn read_team_channels(
    c: &mut PgConnection,
    company: &CompanyId,
    team_id: &str,
    limit: usize,
) -> Result<Vec<ChannelView>, Error> {
    let limit = limit.clamp(1, 100);
    let rows = sqlx::query(
        "SELECT id,name,owner_id,version,last_seq,private,created_at FROM messaging_rooms WHERE company_id=$1 AND team_id=$2 AND kind='channel' ORDER BY created_at,id LIMIT $3",
    )
    .bind(company.as_str())
    .bind(team_id)
    .bind(limit as i64)
    .fetch_all(&mut *c)
    .await?;
    let mut channels = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.try_get(0)?;
        let private: bool = row.try_get(5)?;
        let members = if private {
            room_members(c, &id).await?
        } else {
            team_members(c, team_id).await?
        };
        channels.push(ChannelView {
            id,
            team_id: team_id.into(),
            name: row.try_get(1)?,
            owner_id: row.try_get(2)?,
            version: row.try_get(3)?,
            last_seq: row.try_get(4)?,
            private,
            members,
            created_at: row.try_get(6)?,
        });
    }
    Ok(channels)
}

pub async fn apply_plan(
    c: &mut PgConnection,
    actor: &crate::domain::messaging_native::MessagingActor,
    key: &CommandKey,
    plan: &OwnerPlan,
    now: i64,
) -> Result<Receipt, Error> {
    unit(c).await?;
    let expected = plan_receipt(key, plan)?;
    let result: Result<Receipt, Error> = match plan {
        OwnerPlan::CreateTeam {
            id,
            general_room_id,
            company_id,
            name,
            owner_id,
            members,
        } => {
            sqlx::query(
                "INSERT INTO messaging_teams(id,company_id,name,owner_id,version,created_at) VALUES($1,$2,$3,$4,1,$5)",
            )
            .bind(id)
            .bind(company_id)
            .bind(name)
            .bind(owner_id)
            .bind(now)
            .execute(&mut *c)
            .await?;
            for member in members {
                sqlx::query("INSERT INTO messaging_team_members(team_id,user_id) VALUES($1,$2)")
                    .bind(id)
                    .bind(member)
                    .execute(&mut *c)
                    .await?;
            }
            sqlx::query(
                "INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,team_id,private,version,last_seq,created_at) VALUES($1,$2,'channel','General',$3,$4,false,1,0,$5)",
            )
            .bind(general_room_id)
            .bind(company_id)
            .bind(owner_id)
            .bind(id)
            .bind(now)
            .execute(&mut *c)
            .await?;
            Ok(expected.clone())
        }
        OwnerPlan::CreateChannel {
            id,
            company_id,
            team_id,
            name,
            owner_id,
            private,
            members,
        } => {
            sqlx::query(
                "INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,team_id,private,version,last_seq,created_at) VALUES($1,$2,'channel',$3,$4,$5,$6,1,0,$7)",
            )
            .bind(id)
            .bind(company_id)
            .bind(name)
            .bind(owner_id)
            .bind(team_id)
            .bind(private)
            .bind(now)
            .execute(&mut *c)
            .await?;
            if *private {
                for member in members {
                    sqlx::query(
                        "INSERT INTO messaging_room_members(room_id,user_id) VALUES($1,$2)",
                    )
                    .bind(id)
                    .bind(member)
                    .execute(&mut *c)
                    .await?;
                }
            }
            Ok(expected.clone())
        }
    };
    let receipt = result?;
    if receipt != expected {
        return Err(changed());
    }
    let _ = actor;
    Ok(receipt)
}

pub struct CurrentTeamEffects {
    unit: Unit,
    images: Vec<(String, String)>,
}
impl CurrentTeamEffects {
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
        "team" => "SELECT to_jsonb(p)::text FROM messaging_teams p WHERE p.id=$1",
        "room" => "SELECT to_jsonb(p)::text FROM messaging_rooms p WHERE p.id=$1",
        _ => return Err(changed()),
    };
    Ok(sqlx::query_scalar(sql).bind(id).fetch_optional(c).await?)
}

pub async fn retain_effects(
    c: &mut PgConnection,
    plan: &OwnerPlan,
    receipt: &Receipt,
) -> Result<CurrentTeamEffects, Error> {
    let current_unit = unit(c).await?;
    let mut images = Vec::new();
    match plan {
        OwnerPlan::CreateTeam {
            id,
            general_room_id,
            ..
        } => {
            if receipt.id != *id {
                return Err(changed());
            }
            for key in [format!("team:{id}"), format!("room:{general_room_id}")] {
                let raw = image(c, &key).await?.ok_or_else(changed)?;
                images.push((key, raw));
            }
        }
        OwnerPlan::CreateChannel { id, .. } => {
            if receipt.id != *id {
                return Err(changed());
            }
            let key = format!("room:{id}");
            let raw = image(c, &key).await?.ok_or_else(changed)?;
            images.push((key, raw));
        }
    }
    Ok(CurrentTeamEffects {
        unit: current_unit,
        images,
    })
}
