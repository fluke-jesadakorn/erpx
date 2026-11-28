//! Native PMS home hotel-metrics read facts.
//!
//! This is a read projection, never a command, receipt or source purchase. It
//! ports the retained `hotelMetrics` SQL
//! (`modules/pms/server/src/adapters/pms/home.ts`) exactly: one row per PMS
//! property, with room/stay counts anchored on that property's `business_date`.
//!
//! Two retained rules are explicit here and copied verbatim into the adapter:
//!
//! * the **maintenance-block capacity rule** — `capacity` counts active rooms
//!   except those with an active `maintenance` block whose `from` is on or
//!   before the business date. Such a room still counts in `room_count`.
//! * the **ordering** — `ORDER BY p.name, p.id`.
//!
//! The in-memory projection mirrors the SQL so a unit test can pin the rules
//! without a database and a PostgreSQL test can assert parity against the same
//! facts. Money is intentionally absent: `hotelMoney` is a separate port and
//! this slice makes no Finance claim.
use erpx_platform_shared_server::public::{BusinessError, FailureKind};

/// Bounded scope of a hotel-metrics read. The source port bound the branch and
/// property facts twice; the native port carries each once and binds it at both
/// placeholders, which is the same selection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PmsHotelMetricsFacts {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub property_id: Option<String>,
}

/// One retained `hotelMetrics` row. Counts are PostgreSQL `count(*)` BIGINTs.
/// `currency` is `NULL` when `config_json` has no `currency` key, exactly as
/// `config_json::jsonb->>'currency'` yields `NULL`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmsHotelMetric {
    pub id: String,
    pub name: String,
    pub branch_id: String,
    pub business_date: String,
    pub state: String,
    pub currency: Option<String>,
    pub room_count: i64,
    pub inspected: i64,
    pub capacity: i64,
    pub sold: i64,
    pub arrivals: i64,
    pub departures: i64,
}

/// PMS-owned property input for the in-memory projection (the columns the SQL
/// selects or filters on).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PmsMetricPropertyRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub name: String,
    pub business_date: String,
    pub state: String,
    pub config_json: String,
}

/// PMS-owned record input for the in-memory projection (the columns the SQL
/// reads out of `data_json::jsonb`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PmsMetricRecordRow {
    pub id: String,
    pub property_id: String,
    pub kind: String,
    pub data_json: String,
}

fn field<'a>(data: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    data.get(key).and_then(serde_json::Value::as_str)
}

fn parse(data_json: &str) -> Option<serde_json::Value> {
    serde_json::from_str(data_json).ok()
}

/// `data_json::jsonb->>'active'='true'`.
fn active_room(record: &PmsMetricRecordRow, data: &serde_json::Value) -> bool {
    record.kind == "room" && field(data, "active") == Some("true")
}

/// `data_json::jsonb->>'cleanliness'='inspected'`.
fn inspected_room(data: &serde_json::Value) -> bool {
    field(data, "cleanliness") == Some("inspected")
}

/// The retained maintenance-block capacity exclusion: an active `maintenance`
/// block on this room, effective on or before the property's business date.
/// `from` is compared as text, exactly as `->>'from'<=p.business_date` does for
/// the stored `YYYY-MM-DD` values.
fn maintenance_blocked(
    property_id: &str,
    room_id: &str,
    business_date: &str,
    records: &[PmsMetricRecordRow],
) -> bool {
    records.iter().any(|block| {
        if block.property_id != property_id || block.kind != "block" {
            return false;
        }
        let Some(data) = parse(&block.data_json) else {
            return false;
        };
        field(&data, "state") == Some("active")
            && field(&data, "kind") == Some("maintenance")
            && field(&data, "from").is_some_and(|from| from <= business_date)
            && field(&data, "roomId") == Some(room_id)
    })
}

fn in_scope(property: &PmsMetricPropertyRow, facts: &PmsHotelMetricsFacts) -> bool {
    property.company_id == facts.company_id
        && facts
            .branch_id
            .as_deref()
            .is_none_or(|branch| property.branch_id == branch)
        && facts
            .property_id
            .as_deref()
            .is_none_or(|property_id| property.id == property_id)
}

/// In-memory mirror of the retained `hotelMetrics` SQL. Reads only PMS-owned
/// facts and returns rows in the retained `name, id` order.
pub fn project_hotel_metrics(
    properties: &[PmsMetricPropertyRow],
    records: &[PmsMetricRecordRow],
    facts: &PmsHotelMetricsFacts,
) -> Vec<PmsHotelMetric> {
    let mut rows: Vec<PmsHotelMetric> = properties
        .iter()
        .filter(|property| in_scope(property, facts))
        .map(|property| {
            let bd = property.business_date.as_str();
            let currency = parse(&property.config_json)
                .and_then(|config| field(&config, "currency").map(str::to_owned));

            let mut room_count = 0_i64;
            let mut inspected = 0_i64;
            let mut capacity = 0_i64;
            for room in records.iter().filter(|r| r.property_id == property.id) {
                let Some(data) = parse(&room.data_json) else {
                    continue;
                };
                if !active_room(room, &data) {
                    continue;
                }
                room_count += 1;
                if inspected_room(&data) {
                    inspected += 1;
                }
                if !maintenance_blocked(&property.id, &room.id, bd, records) {
                    capacity += 1;
                }
            }

            let (mut sold, mut arrivals, mut departures) = (0_i64, 0_i64, 0_i64);
            for stay in records.iter().filter(|r| r.property_id == property.id && r.kind == "stay") {
                let Some(data) = parse(&stay.data_json) else {
                    continue;
                };
                let state = field(&data, "state");
                let arrival = field(&data, "arrival");
                let departure = field(&data, "departure");
                if matches!(state, Some("in_house" | "checked_out"))
                    && arrival.is_some_and(|value| value <= bd)
                    && departure.is_some_and(|value| value > bd)
                {
                    sold += 1;
                }
                if state == Some("reserved") && arrival.is_some_and(|value| value <= bd) {
                    arrivals += 1;
                }
                if state == Some("in_house") && departure.is_some_and(|value| value <= bd) {
                    departures += 1;
                }
            }

            PmsHotelMetric {
                id: property.id.clone(),
                name: property.name.clone(),
                branch_id: property.branch_id.clone(),
                business_date: property.business_date.clone(),
                state: property.state.clone(),
                currency,
                room_count,
                inspected,
                capacity,
                sold,
                arrivals,
                departures,
            }
        })
        .collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    rows
}

/// Fail closed when a read does not describe the retained `name, id` order;
/// the native read port must not silently reorder the home overview.
pub fn ensure_ordered(rows: &[PmsHotelMetric]) -> Result<(), BusinessError> {
    let ordered = rows.windows(2).all(|pair| {
        pair[0].name < pair[1].name || (pair[0].name == pair[1].name && pair[0].id < pair[1].id)
    });
    if ordered {
        Ok(())
    } else {
        Err(read_invalid())
    }
}

/// Unavailable read error used for a malformed native hotel-metrics read.
pub fn read_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_HOTEL_METRICS_READ_INVALID",
        "The hotel metrics could not be read.",
    )
}
