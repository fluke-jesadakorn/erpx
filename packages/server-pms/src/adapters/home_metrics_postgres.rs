//! PostgreSQL owner adapter for the native PMS home hotel-metrics read.
//!
//! The SQL below is the retained `hotelMetrics` query
//! (`packages/server-pms/src/adapters/pms/home.ts`) ported verbatim: only the
//! `?` placeholders are numbered `$1..$5`. The branch and property facts are
//! bound twice each, matching the source port's duplicated bind, so the
//! selection is identical. The maintenance-block capacity exclusion and the
//! `ORDER BY p.name,p.id` ordering are preserved character for character.
use crate::application::home_metrics::{PmsHotelMetricsFuture, PmsHotelMetricsRepository};
use crate::domain::home_metrics::{PmsHotelMetric, PmsHotelMetricsFacts};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sqlx::{PgConnection, Row, postgres::PgRow};

type Error = PgTransactionError<BusinessError>;

/// Retained `hotelMetrics` SQL, exact port with numbered placeholders.
const HOTEL_METRICS_SQL: &str = "SELECT p.id,p.name,p.branch_id,p.business_date,p.state,p.config_json::jsonb->>'currency' currency,
    (SELECT count(*) FROM pms_records r WHERE r.property_id=p.id AND r.kind='room' AND r.data_json::jsonb->>'active'='true') room_count,
    (SELECT count(*) FROM pms_records r WHERE r.property_id=p.id AND r.kind='room' AND r.data_json::jsonb->>'active'='true'
      AND r.data_json::jsonb->>'cleanliness'='inspected') inspected,
    (SELECT count(*) FROM pms_records r WHERE r.property_id=p.id AND r.kind='room' AND r.data_json::jsonb->>'active'='true'
      AND NOT EXISTS(SELECT 1 FROM pms_records b WHERE b.property_id=p.id AND b.kind='block' AND b.data_json::jsonb->>'state'='active'
        AND b.data_json::jsonb->>'kind'='maintenance' AND b.data_json::jsonb->>'from'<=p.business_date AND b.data_json::jsonb->>'roomId'=r.id)) capacity,
    (SELECT count(*) FROM pms_records s WHERE s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'state' IN ('in_house','checked_out')
      AND s.data_json::jsonb->>'arrival'<=p.business_date AND s.data_json::jsonb->>'departure'>p.business_date) sold,
    (SELECT count(*) FROM pms_records s WHERE s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'state'='reserved' AND s.data_json::jsonb->>'arrival'<=p.business_date) arrivals,
    (SELECT count(*) FROM pms_records s WHERE s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'state'='in_house' AND s.data_json::jsonb->>'departure'<=p.business_date) departures
    FROM pms_properties p WHERE p.company_id=$1 AND ($2::text IS NULL OR p.branch_id=$3) AND ($4::text IS NULL OR p.id=$5)
    ORDER BY p.name,p.id";

fn metric_row(row: &PgRow) -> Result<PmsHotelMetric, Error> {
    Ok(PmsHotelMetric {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        branch_id: row.try_get("branch_id")?,
        business_date: row.try_get("business_date")?,
        state: row.try_get("state")?,
        currency: row.try_get("currency")?,
        room_count: row.try_get("room_count")?,
        inspected: row.try_get("inspected")?,
        capacity: row.try_get("capacity")?,
        sold: row.try_get("sold")?,
        arrivals: row.try_get("arrivals")?,
        departures: row.try_get("departures")?,
    })
}

async fn read(
    connection: &mut PgConnection,
    facts: &PmsHotelMetricsFacts,
) -> Result<Vec<PmsHotelMetric>, Error> {
    let rows = sqlx::query(HOTEL_METRICS_SQL)
        .bind(&facts.company_id)
        .bind(facts.branch_id.as_deref())
        .bind(facts.branch_id.as_deref())
        .bind(facts.property_id.as_deref())
        .bind(facts.property_id.as_deref())
        .fetch_all(&mut *connection)
        .await?;
    rows.iter().map(metric_row).collect()
}

/// Read-only PMS hotel-metrics repository bound to the caller's connection.
pub struct PgPmsHotelMetricsRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgPmsHotelMetricsRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

impl PmsHotelMetricsRepository for PgPmsHotelMetricsRead<'_> {
    type Error = Error;

    fn hotel_metrics<'a>(
        &'a mut self,
        facts: &'a PmsHotelMetricsFacts,
    ) -> PmsHotelMetricsFuture<'a, Vec<PmsHotelMetric>, Self::Error> {
        Box::pin(async move { read(self.connection, facts).await })
    }
}
