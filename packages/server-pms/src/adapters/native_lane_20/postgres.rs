//! Pg-backed execution of the property-readiness family. The SQL below is the
//! byte-exact expansion of the retired TypeScript `setupFacts` and `hasStays`
//! statements (`pms-read-tail-r31` EXISTS probes), plus an explicit
//! `pms_properties.company_id` scope so the axum owner, not the caller, owns
//! company isolation. No TS SQL and no TS calculation survive in the consumer.

use super::application::{
    PropertyReadinessFuture, PropertyReadinessRead, RoomSummaryFuture, RoomSummaryQuery,
    RoomSummaryRead,
};
use super::domain::{PropertyReadinessFacts, RoomSummaryFacts};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use sqlx::Row;

/// Scope witness: the property must belong to the company carried on the
/// request. A foreign property produces `None` (404), never empty facts.
pub const PROPERTY_SCOPE_SQL: &str =
    "SELECT 1 FROM pms_properties WHERE id=$1 AND company_id=$2";

/// Combined readiness projection. The four EXISTS arms are the retired TS
/// fragments, with the company scope pushed into each arm.
pub const READINESS_SQL: &str = r#"SELECT EXISTS(SELECT 1 FROM pms_records r JOIN pms_properties p ON p.id=r.property_id AND p.company_id=$1 WHERE r.property_id=$2 AND r.kind='room_type') AS has_room_types, EXISTS(SELECT 1 FROM pms_records r JOIN pms_properties p ON p.id=r.property_id AND p.company_id=$1 WHERE r.property_id=$2 AND r.kind='room' AND r.data_json::jsonb->>'active'='true') AS has_active_rooms, EXISTS(SELECT 1 FROM pms_records r JOIN pms_properties p ON p.id=r.property_id AND p.company_id=$1 WHERE r.property_id=$2 AND r.kind='rate' AND r.data_json::jsonb->>'active'='true') AS has_active_rates, EXISTS(SELECT 1 FROM pms_records r JOIN pms_properties p ON p.id=r.property_id AND p.company_id=$1 WHERE r.property_id=$2 AND r.kind='stay') AS has_stays"#;

pub struct PgPropertyReadinessRead {
    pool: DatabasePool,
}

impl PgPropertyReadinessRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PropertyReadinessRead for PgPropertyReadinessRead {
    fn load<'a>(
        &'a self,
        company: &'a str,
        property: &'a str,
    ) -> PropertyReadinessFuture<'a, Option<PropertyReadinessFacts>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let property = property.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let property = property.clone();
                Box::pin(async move {
                    let scoped = sqlx::query_scalar::<_, i32>(PROPERTY_SCOPE_SQL)
                        .bind(property.as_str())
                        .bind(company.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| super::domain::configuration_unavailable())?;
                    if scoped.is_none() {
                        return Ok(None);
                    }
                    let row = sqlx::query(READINESS_SQL)
                        .bind(company.as_str())
                        .bind(property.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| super::domain::configuration_unavailable())?;
                    Ok(Some(PropertyReadinessFacts {
                        has_room_types: row.try_get::<bool, _>("has_room_types").map_err(|_| super::domain::configuration_unavailable())?,
                        has_active_rooms: row.try_get::<bool, _>("has_active_rooms").map_err(|_| super::domain::configuration_unavailable())?,
                        has_active_rates: row.try_get::<bool, _>("has_active_rates").map_err(|_| super::domain::configuration_unavailable())?,
                        has_stays: row.try_get::<bool, _>("has_stays").map_err(|_| super::domain::configuration_unavailable())?,
                    }))
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => super::domain::configuration_unavailable(),
            })
        })
    }
}

/// Room-summary projection: the retired TS `roomSummary` statement, with the
/// Company scope pushed into a `pms_properties` join and each optional filter
/// bound once instead of twice. The stay-existence witness is unchanged.
pub const ROOM_SUMMARY_SQL: &str = r#"SELECT count(*)::integer total, count(*) FILTER (WHERE r.data_json::jsonb->>'active'='true')::integer active, count(*) FILTER (WHERE r.data_json::jsonb->>'cleanliness'='dirty')::integer dirty, count(*) FILTER (WHERE r.data_json::jsonb->>'cleanliness'='clean')::integer clean, count(*) FILTER (WHERE r.data_json::jsonb->>'cleanliness'='inspected')::integer inspected, count(*) FILTER (WHERE r.data_json::jsonb->>'dnd'='true')::integer dnd, count(*) FILTER (WHERE EXISTS(SELECT 1 FROM pms_records st WHERE st.property_id=r.property_id AND st.kind='stay' AND st.data_json::jsonb->>'roomId'=r.id AND st.data_json::jsonb->>'state'='in_house'))::integer occupied FROM pms_records r JOIN pms_properties p ON p.id=r.property_id AND p.company_id=$1 WHERE r.property_id=$2 AND r.kind='room' AND ($3='' OR r.data_json::jsonb->>'cleanliness'=$3) AND ($4='' OR position(lower($4) in lower(concat_ws(' ',r.id,r.data_json::jsonb->>'number',r.data_json::jsonb->>'building',r.data_json::jsonb->>'floor')))>0) AND ($5='' OR r.data_json::jsonb->>'building'=$5) AND ($6='' OR r.data_json::jsonb->>'floor'=$6) AND ($7='' OR r.data_json::jsonb->>'typeId'=$7)"#;

pub struct PgRoomSummaryRead {
    pool: DatabasePool,
}

impl PgRoomSummaryRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl RoomSummaryRead for PgRoomSummaryRead {
    fn load<'a>(
        &'a self,
        company: &'a str,
        query: &'a RoomSummaryQuery,
    ) -> RoomSummaryFuture<'a, Option<RoomSummaryFacts>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let owned = RoomSummaryQuery {
            property: query.property.clone(),
            status: query.status.clone(),
            q: query.q.clone(),
            building: query.building.clone(),
            floor: query.floor.clone(),
            type_id: query.type_id.clone(),
        };
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let owned = owned.clone();
                Box::pin(async move {
                    let scoped = sqlx::query_scalar::<_, i32>(PROPERTY_SCOPE_SQL)
                        .bind(owned.property.as_str())
                        .bind(company.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| super::domain::configuration_unavailable())?;
                    if scoped.is_none() {
                        return Ok(None);
                    }
                    let row = sqlx::query(ROOM_SUMMARY_SQL)
                        .bind(company.as_str())
                        .bind(owned.property.as_str())
                        .bind(owned.status.as_str())
                        .bind(owned.q.as_str())
                        .bind(owned.building.as_str())
                        .bind(owned.floor.as_str())
                        .bind(owned.type_id.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| super::domain::configuration_unavailable())?;
                    Ok(Some(RoomSummaryFacts {
                        total: row.try_get::<i32, _>("total").map_err(|_| super::domain::configuration_unavailable())?,
                        active: row.try_get::<i32, _>("active").map_err(|_| super::domain::configuration_unavailable())?,
                        dirty: row.try_get::<i32, _>("dirty").map_err(|_| super::domain::configuration_unavailable())?,
                        clean: row.try_get::<i32, _>("clean").map_err(|_| super::domain::configuration_unavailable())?,
                        inspected: row.try_get::<i32, _>("inspected").map_err(|_| super::domain::configuration_unavailable())?,
                        dnd: row.try_get::<i32, _>("dnd").map_err(|_| super::domain::configuration_unavailable())?,
                        occupied: row.try_get::<i32, _>("occupied").map_err(|_| super::domain::configuration_unavailable())?,
                    }))
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => super::domain::configuration_unavailable(),
            })
        })
    }
}
