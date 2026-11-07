//! PMS-owned front-desk queue reads. Only `pms_properties`, `pms_records` and
//! `pms_documents` are read here. People, Commercial, Finance and Records stay
//! public ports owned by their crates.
use crate::adapters::room_board::{property, record_row, serializable};
use crate::application::front_desk::PmsFrontDeskRepository;
use crate::application::room_board::PmsBoardFuture;
use crate::domain::front_desk::{PmsFrontDeskData, PmsFrontDeskSelection, read_limit};
use crate::domain::room_board::{PAGE_SIZE, PmsBoardVisibility, PmsRecord};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub async fn lock_front_desk_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query("LOCK TABLE pms_properties,pms_records,pms_documents IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}

async fn bounded_kind(
    c: &mut PgConnection,
    property_id: &str,
    kind: &str,
    limit: usize,
) -> Result<Vec<PmsRecord>, Error> {
    let rows = sqlx::query(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind=$2 ORDER BY id LIMIT $3",
    )
    .bind(property_id)
    .bind(kind)
    .bind(limit as i64 + 1)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > limit {
        return Err(read_limit("The hotel front desk exceeds its bounded read.").into());
    }
    rows.iter().map(record_row).collect()
}

async fn page_stays(
    c: &mut PgConnection,
    property_id: &str,
    selection: &PmsFrontDeskSelection,
    from: &str,
    to: &str,
    business_date: &str,
) -> Result<Vec<PmsRecord>, Error> {
    let rows = sqlx::query(
        "SELECT s.id,s.kind,s.version,s.updated_at,s.data_json \
         FROM pms_records s \
         LEFT JOIN pms_records g ON g.property_id=s.property_id AND g.kind='guest' AND g.id=s.data_json::jsonb->>'guestId' \
         LEFT JOIN pms_records r ON r.property_id=s.property_id AND r.kind='reservation' AND r.id=s.data_json::jsonb->>'reservationId' \
         WHERE s.property_id=$1 AND s.kind='stay' AND (\
           ($2='' AND ((s.data_json::jsonb->>'arrival'<=$4 AND s.data_json::jsonb->>'departure'>=$3) OR s.data_json::jsonb->>'state'='in_house')) \
           OR ($2='arrivals' AND s.data_json::jsonb->>'state'='reserved' AND s.data_json::jsonb->>'arrival'=$5) \
           OR ($2='departures' AND s.data_json::jsonb->>'state'='in_house' AND s.data_json::jsonb->>'departure'<=$5) \
           OR ($2 NOT IN ('','arrivals','departures') AND s.data_json::jsonb->>'state'=$2 \
             AND ((s.data_json::jsonb->>'arrival'<=$4 AND s.data_json::jsonb->>'departure'>=$3) OR s.data_json::jsonb->>'state'='in_house'))) \
         AND ($6='' OR position(lower($6) in lower(concat_ws(' ',g.data_json::jsonb->>'name',s.id,r.data_json::jsonb->>'number')))>0) \
         ORDER BY COALESCE(s.data_json::jsonb->>'arrival',s.updated_at::text),s.id \
         LIMIT 51 OFFSET $7",
    )
    .bind(property_id)
    .bind(&selection.status)
    .bind(from)
    .bind(to)
    .bind(business_date)
    .bind(&selection.q)
    .bind(i64::from(selection.page) * PAGE_SIZE as i64)
    .fetch_all(&mut *c)
    .await?;
    rows.iter().map(record_row).collect()
}

async fn selected_stay(
    c: &mut PgConnection,
    property_id: &str,
    id: &str,
) -> Result<Option<PmsRecord>, Error> {
    let row = sqlx::query(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind='stay' AND id=$2",
    )
    .bind(property_id)
    .bind(id)
    .fetch_optional(&mut *c)
    .await?;
    row.as_ref().map(record_row).transpose()
}

async fn linked_records(
    c: &mut PgConnection,
    property_id: &str,
    kind: &str,
    link: &str,
    stay_ids: &[String],
) -> Result<Vec<PmsRecord>, Error> {
    if stay_ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind=$2 AND id IN (\
           SELECT data_json::jsonb->>'{link}' FROM pms_records \
           WHERE property_id=$1 AND kind='stay' AND id=ANY($3)) \
         ORDER BY id LIMIT 501"
    );
    let rows = sqlx::query(AssertSqlSafe(sql))
        .bind(property_id)
        .bind(kind)
        .bind(stay_ids)
        .fetch_all(&mut *c)
        .await?;
    if rows.len() > 500 {
        return Err(read_limit("The hotel front desk exceeds its bounded read.").into());
    }
    rows.iter().map(record_row).collect()
}

async fn issued_documents(
    c: &mut PgConnection,
    property_id: &str,
    account_ids: &[String],
) -> Result<Vec<Value>, Error> {
    if account_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        "SELECT id,guest_account_id,kind,created_at FROM pms_documents \
         WHERE property_id=$1 AND guest_account_id=ANY($2) \
         ORDER BY created_at DESC,id LIMIT 201",
    )
    .bind(property_id)
    .bind(account_ids)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > 200 {
        return Err(read_limit("The hotel front desk has too many issued documents.").into());
    }
    rows.iter()
        .map(|row| {
            Ok(json!({
                "id": row.try_get::<String, _>("id")?,
                "guest_account_id": row.try_get::<String, _>("guest_account_id")?,
                "kind": row.try_get::<String, _>("kind")?,
                "created_at": row.try_get::<i64, _>("created_at")?,
            }))
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(Error::from)
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

async fn dates(
    c: &mut PgConnection,
    business_date: &str,
    selection: &PmsFrontDeskSelection,
) -> Result<(String, String), Error> {
    match (&selection.from, &selection.to) {
        (Some(from), Some(to)) => Ok((from.clone(), to.clone())),
        _ => {
            let dates: (String, String) =
                sqlx::query_as("SELECT ($1::date - 30)::text,($1::date + 60)::text")
                    .bind(business_date)
                    .fetch_one(&mut *c)
                    .await?;
            Ok(dates)
        }
    }
}

pub async fn read_front_desk(
    c: &mut PgConnection,
    property_id: &str,
    selection: &PmsFrontDeskSelection,
    visibility: PmsBoardVisibility,
) -> Result<PmsFrontDeskData, Error> {
    serializable(c).await?;
    let property = property(c, property_id).await?;
    let (from, to) = dates(c, &property.business_date, selection).await?;
    let paged = page_stays(
        c,
        property_id,
        selection,
        &from,
        &to,
        &property.business_date,
    )
    .await?;
    let has_next = paged.len() > PAGE_SIZE;
    let page: Vec<PmsRecord> = paged.into_iter().take(PAGE_SIZE).collect();
    let ids: Vec<String> = page.iter().map(|row| row.id.clone()).collect();
    let mut stay_ids = ids.clone();
    let selected = match selection.selected_id.as_deref() {
        Some(id) if !stay_ids.iter().any(|value| value == id) => {
            let row = selected_stay(c, property_id, id).await?;
            if let Some(row) = &row {
                stay_ids.push(row.id.clone());
            }
            row
        }
        _ => None,
    };

    let mut records = Vec::new();
    push_unique(
        &mut records,
        bounded_kind(c, property_id, "room_type", 500).await?,
    );
    push_unique(
        &mut records,
        bounded_kind(c, property_id, "room", 500).await?,
    );
    if visibility.pricing {
        push_unique(
            &mut records,
            bounded_kind(c, property_id, "rate", 500).await?,
        );
    }
    push_unique(&mut records, page);
    if let Some(selected) = selected {
        push_unique(&mut records, vec![selected]);
    }
    push_unique(
        &mut records,
        linked_records(c, property_id, "reservation", "reservationId", &stay_ids).await?,
    );
    push_unique(
        &mut records,
        linked_records(c, property_id, "guest", "guestId", &stay_ids).await?,
    );
    let accounts =
        linked_records(c, property_id, "guest_account", "guestAccountId", &stay_ids).await?;
    let account_ids: Vec<String> = accounts.iter().map(|row| row.id.clone()).collect();
    push_unique(&mut records, accounts);
    let documents = if visibility.financial {
        let known: std::collections::BTreeSet<&str> =
            records.iter().map(|row| row.id.as_str()).collect();
        let visible_accounts: Vec<String> = account_ids
            .into_iter()
            .filter(|id| known.contains(id.as_str()))
            .collect();
        issued_documents(c, property_id, &visible_accounts).await?
    } else {
        Vec::new()
    };
    if records.len() > crate::domain::room_board::MAX_RECORDS {
        return Err(read_limit("The hotel front desk exceeds its bounded read.").into());
    }
    Ok(PmsFrontDeskData {
        property,
        records,
        ids,
        has_next,
        documents,
        from,
        to,
    })
}

/// SQL repository adapter bound to the caller's retained serializable unit.
pub struct PgPmsFrontDeskRead<'a> {
    connection: &'a mut PgConnection,
}
impl<'a> PgPmsFrontDeskRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}
impl PmsFrontDeskRepository for PgPmsFrontDeskRead<'_> {
    type Error = Error;
    fn properties<'a>(
        &'a mut self,
        company_id: &'a str,
        branch_id: Option<&'a str>,
    ) -> PmsBoardFuture<'a, Vec<crate::domain::room_board::PmsPropertyRow>, Self::Error> {
        Box::pin(async move {
            crate::adapters::room_board::read_room_properties(
                self.connection,
                company_id,
                branch_id,
            )
            .await
        })
    }
    fn front_desk<'a>(
        &'a mut self,
        property_id: &'a str,
        _business_date: &'a str,
        selection: &'a PmsFrontDeskSelection,
        visibility: PmsBoardVisibility,
    ) -> PmsBoardFuture<'a, PmsFrontDeskData, Self::Error> {
        Box::pin(async move {
            read_front_desk(self.connection, property_id, selection, visibility).await
        })
    }
}
