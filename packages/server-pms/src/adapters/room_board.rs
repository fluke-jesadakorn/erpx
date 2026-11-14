//! PMS-owned property/room board reads. Only `pms_properties` and `pms_records`
//! are read here. Finance, People, Records and Workflow stay public ports owned
//! by their crates.
use crate::application::room_board::{PmsBoardFuture, PmsBoardRepository};
use crate::domain::room_board::{
    PAGE_SIZE, PmsBoardData, PmsBoardSelection, PmsBoardSetup, PmsBoardVisibility, PmsPropertyRow,
    PmsRecord, PmsRoomSummary, read_invalid, read_limit,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row, postgres::PgRow};

type Error = PgTransactionError<BusinessError>;

pub(crate) async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PMS_BOARD_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("PMS_BOARD_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub async fn lock_room_board_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query("LOCK TABLE pms_properties,pms_records IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}

fn error() -> BusinessError {
    read_invalid()
}

fn property_row(row: &PgRow) -> Result<PmsPropertyRow, Error> {
    Ok(PmsPropertyRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        name: row.try_get("name")?,
        business_date: row.try_get("business_date")?,
        state: row.try_get("state")?,
        config_json: row.try_get("config_json")?,
        version: row.try_get("version")?,
    })
}

pub(crate) fn record_row(row: &PgRow) -> Result<PmsRecord, Error> {
    let raw: String = row.try_get("data_json")?;
    let data: Value = serde_json::from_str(&raw).map_err(|_| Error::from(error()))?;
    Ok(PmsRecord {
        id: row.try_get("id")?,
        kind: row.try_get("kind")?,
        version: row.try_get("version")?,
        updated_at: row.try_get("updated_at")?,
        data,
    })
}

pub async fn read_room_properties(
    c: &mut PgConnection,
    company_id: &str,
    branch_id: Option<&str>,
) -> Result<Vec<PmsPropertyRow>, Error> {
    serializable(c).await?;
    let rows = sqlx::query(
        "SELECT id,company_id,branch_id,name,business_date,state,config_json,version FROM pms_properties WHERE company_id=$1 AND ($2::text IS NULL OR branch_id=$2) ORDER BY name,id LIMIT 101",
    )
    .bind(company_id)
    .bind(branch_id)
    .fetch_all(&mut *c)
    .await?;
    rows.iter().map(property_row).collect()
}

pub(crate) async fn property(
    c: &mut PgConnection,
    property_id: &str,
) -> Result<PmsPropertyRow, Error> {
    let row = sqlx::query(
        "SELECT id,company_id,branch_id,name,business_date,state,config_json,version FROM pms_properties WHERE id=$1",
    )
    .bind(property_id)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| Error::from(crate::domain::room_board::not_found()))?;
    property_row(&row)
}

async fn selected_room_ids(
    c: &mut PgConnection,
    property_id: &str,
    selection: &PmsBoardSelection,
) -> Result<(Vec<String>, bool), Error> {
    let rows = sqlx::query(
        "SELECT id FROM pms_records r WHERE property_id=$1 AND kind='room' \
         AND ($2='' OR COALESCE(data_json::jsonb->>'cleanliness',data_json::jsonb->>'state')=$2) \
         AND ($3='' OR position(lower($3) in lower(concat_ws(' ',r.id,r.data_json::jsonb->>'number',r.data_json::jsonb->>'building',r.data_json::jsonb->>'floor')))>0) \
         AND ($4='' OR data_json::jsonb->>'building'=$4) \
         AND ($5='' OR data_json::jsonb->>'floor'=$5) \
         AND ($6='' OR data_json::jsonb->>'typeId'=$6) \
         ORDER BY data_json::jsonb->>'number',id LIMIT 51 OFFSET $7",
    )
    .bind(property_id)
    .bind(&selection.status)
    .bind(&selection.q)
    .bind(&selection.building)
    .bind(&selection.floor)
    .bind(selection.type_id.clone().unwrap_or_default())
    .bind(i64::from(selection.page) * PAGE_SIZE as i64)
    .fetch_all(&mut *c)
    .await?;
    let has_next = rows.len() > PAGE_SIZE;
    let ids = rows
        .iter()
        .take(PAGE_SIZE)
        .map(|row| row.try_get("id"))
        .collect::<Result<Vec<String>, _>>()?;
    Ok((ids, has_next))
}

async fn room_records(c: &mut PgConnection, property_id: &str) -> Result<Vec<PmsRecord>, Error> {
    let rows = sqlx::query(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records WHERE property_id=$1 AND kind='room' ORDER BY data_json::jsonb->>'number',id LIMIT 501",
    )
    .bind(property_id)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > 500 {
        return Err(
            read_limit("The hotel has more rooms than this bounded board can show.").into(),
        );
    }
    rows.iter().map(record_row).collect()
}

async fn kind_records(
    c: &mut PgConnection,
    property_id: &str,
    kind: &str,
    limit: i64,
) -> Result<Vec<PmsRecord>, Error> {
    let rows = sqlx::query(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records WHERE property_id=$1 AND kind=$2 ORDER BY id LIMIT $3",
    )
    .bind(property_id)
    .bind(kind)
    .bind(limit + 1)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() as i64 > limit {
        return Err(read_limit("The hotel room board exceeds its bounded read.").into());
    }
    rows.iter().map(record_row).collect()
}

async fn in_house_stays(c: &mut PgConnection, property_id: &str) -> Result<Vec<PmsRecord>, Error> {
    let rows = sqlx::query(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records WHERE property_id=$1 AND kind='stay' AND data_json::jsonb->>'state'='in_house' ORDER BY id LIMIT 501",
    )
    .bind(property_id)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > 500 {
        return Err(read_limit(
            "The hotel has more in-house stays than this bounded board can show.",
        )
        .into());
    }
    rows.iter().map(record_row).collect()
}

async fn active_tasks(c: &mut PgConnection, property_id: &str) -> Result<Vec<PmsRecord>, Error> {
    let rows = sqlx::query(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records WHERE property_id=$1 AND kind='task' AND data_json::jsonb->>'state' NOT IN ('done','superseded') ORDER BY updated_at DESC,id LIMIT 501",
    )
    .bind(property_id)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > 500 {
        return Err(read_limit(
            "The hotel has more open room tasks than this bounded board can show.",
        )
        .into());
    }
    rows.iter().map(record_row).collect()
}

async fn active_blocks(c: &mut PgConnection, property_id: &str) -> Result<Vec<PmsRecord>, Error> {
    let rows = sqlx::query(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records WHERE property_id=$1 AND kind='block' AND data_json::jsonb->>'state'='active' AND data_json::jsonb->>'kind'='maintenance' ORDER BY id LIMIT 201",
    )
    .bind(property_id)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > 200 {
        return Err(read_limit(
            "The hotel has more maintenance blocks than this bounded board can show.",
        )
        .into());
    }
    rows.iter().map(record_row).collect()
}

async fn related_records(
    c: &mut PgConnection,
    property_id: &str,
    kind: &str,
    link: &str,
) -> Result<Vec<PmsRecord>, Error> {
    let sql = format!(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records WHERE property_id=$1 AND kind=$2 AND id IN (SELECT DISTINCT data_json::jsonb->>'{link}' FROM pms_records WHERE property_id=$1 AND kind='stay' AND data_json::jsonb->>'state'='in_house') ORDER BY id LIMIT 501"
    );
    let rows = sqlx::query(AssertSqlSafe(sql))
        .bind(property_id)
        .bind(kind)
        .fetch_all(&mut *c)
        .await?;
    if rows.len() > 500 {
        return Err(read_limit(
            "The hotel has more related guest records than this bounded board can show.",
        )
        .into());
    }
    rows.iter().map(record_row).collect()
}

async fn setup_facts(c: &mut PgConnection, property_id: &str) -> Result<PmsBoardSetup, Error> {
    let row = sqlx::query(
        "SELECT EXISTS(SELECT 1 FROM pms_records WHERE property_id=$1 AND kind='room_type') has_room_types, \
         EXISTS(SELECT 1 FROM pms_records WHERE property_id=$1 AND kind='room' AND data_json::jsonb->>'active'='true') has_active_rooms, \
         EXISTS(SELECT 1 FROM pms_records WHERE property_id=$1 AND kind='rate' AND data_json::jsonb->>'active'='true') has_active_rates",
    )
    .bind(property_id)
    .fetch_one(&mut *c)
    .await?;
    Ok(PmsBoardSetup {
        has_room_types: row.try_get("has_room_types")?,
        has_active_rooms: row.try_get("has_active_rooms")?,
        has_active_rates: row.try_get("has_active_rates")?,
    })
}

async fn room_summary(
    c: &mut PgConnection,
    property_id: &str,
    selection: &PmsBoardSelection,
) -> Result<PmsRoomSummary, Error> {
    let row = sqlx::query(
        "SELECT count(*)::bigint total, \
         count(*) FILTER (WHERE data_json::jsonb->>'active'='true')::bigint active, \
         count(*) FILTER (WHERE data_json::jsonb->>'cleanliness'='dirty')::bigint dirty, \
         count(*) FILTER (WHERE data_json::jsonb->>'cleanliness'='clean')::bigint clean, \
         count(*) FILTER (WHERE data_json::jsonb->>'cleanliness'='inspected')::bigint inspected, \
         count(*) FILTER (WHERE data_json::jsonb->>'dnd'='true')::bigint dnd, \
         count(*) FILTER (WHERE EXISTS (SELECT 1 FROM pms_records st WHERE st.property_id=r.property_id AND st.kind='stay' AND st.data_json::jsonb->>'roomId'=r.id AND st.data_json::jsonb->>'state'='in_house'))::bigint occupied \
         FROM pms_records r WHERE r.property_id=$1 AND r.kind='room' \
         AND ($2='' OR COALESCE(r.data_json::jsonb->>'cleanliness',r.data_json::jsonb->>'state')=$2) \
         AND ($3='' OR position(lower($3) in lower(concat_ws(' ',r.id,r.data_json::jsonb->>'number',r.data_json::jsonb->>'building',r.data_json::jsonb->>'floor')))>0) \
         AND ($4='' OR r.data_json::jsonb->>'building'=$4) \
         AND ($5='' OR r.data_json::jsonb->>'floor'=$5) \
         AND ($6='' OR r.data_json::jsonb->>'typeId'=$6)",
    )
    .bind(property_id)
    .bind(&selection.status)
    .bind(&selection.q)
    .bind(&selection.building)
    .bind(&selection.floor)
    .bind(selection.type_id.clone().unwrap_or_default())
    .fetch_one(&mut *c)
    .await?;
    Ok(PmsRoomSummary {
        total: row.try_get("total")?,
        active: row.try_get("active")?,
        dirty: row.try_get("dirty")?,
        clean: row.try_get("clean")?,
        inspected: row.try_get("inspected")?,
        dnd: row.try_get("dnd")?,
        occupied: row.try_get("occupied")?,
    })
}

fn push_unique(records: &mut Vec<PmsRecord>, rows: Vec<PmsRecord>) {
    let mut seen: std::collections::BTreeSet<String> =
        records.iter().map(|row| row.id.clone()).collect();
    for row in rows {
        if seen.insert(row.id.clone()) {
            records.push(row);
        }
    }
}

pub async fn read_room_board(
    c: &mut PgConnection,
    property_id: &str,
    selection: &PmsBoardSelection,
    visibility: PmsBoardVisibility,
) -> Result<PmsBoardData, Error> {
    serializable(c).await?;
    let property = property(c, property_id).await?;
    let (from, to) = match (&selection.from, &selection.to) {
        (Some(from), Some(to)) => (from.clone(), to.clone()),
        _ => {
            let dates: (String, String) =
                sqlx::query_as("SELECT ($1::date - 30)::text,($1::date + 60)::text")
                    .bind(&property.business_date)
                    .fetch_one(&mut *c)
                    .await?;
            dates
        }
    };
    let (ids, has_next) = selected_room_ids(c, property_id, selection).await?;
    let summary = room_summary(c, property_id, selection).await?;
    let setup = setup_facts(c, property_id).await?;
    let mut records = Vec::new();
    push_unique(&mut records, room_records(c, property_id).await?);
    push_unique(
        &mut records,
        kind_records(c, property_id, "room_type", 500).await?,
    );
    if visibility.pricing {
        push_unique(
            &mut records,
            kind_records(c, property_id, "rate", 500).await?,
        );
    }
    push_unique(&mut records, in_house_stays(c, property_id).await?);
    push_unique(&mut records, active_tasks(c, property_id).await?);
    push_unique(&mut records, active_blocks(c, property_id).await?);
    if visibility.guest_details {
        push_unique(
            &mut records,
            related_records(c, property_id, "guest", "guestId").await?,
        );
        push_unique(
            &mut records,
            related_records(c, property_id, "reservation", "reservationId").await?,
        );
        push_unique(
            &mut records,
            related_records(c, property_id, "guest_account", "guestAccountId").await?,
        );
    }
    Ok(PmsBoardData {
        property,
        records,
        ids,
        has_next,
        room_summary: summary,
        setup,
        from,
        to,
    })
}

/// SQL repository adapter bound to the caller's retained serializable unit.
pub struct PgPmsBoardRead<'a> {
    connection: &'a mut PgConnection,
}
impl<'a> PgPmsBoardRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}
impl PmsBoardRepository for PgPmsBoardRead<'_> {
    type Error = Error;
    fn properties<'a>(
        &'a mut self,
        company_id: &'a str,
        branch_id: Option<&'a str>,
    ) -> PmsBoardFuture<'a, Vec<PmsPropertyRow>, Self::Error> {
        Box::pin(async move { read_room_properties(self.connection, company_id, branch_id).await })
    }
    fn board<'a>(
        &'a mut self,
        property_id: &'a str,
        _business_date: &'a str,
        selection: &'a PmsBoardSelection,
        visibility: PmsBoardVisibility,
    ) -> PmsBoardFuture<'a, PmsBoardData, Self::Error> {
        Box::pin(async move {
            read_room_board(self.connection, property_id, selection, visibility).await
        })
    }
}
