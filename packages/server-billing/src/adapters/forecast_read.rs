//! PostgreSQL read adapter for the Billing seat-forecast baseline. The caller
//! owns the Control connection; this adapter never commits, writes or reads a
//! Holding database.
use crate::application::forecast_read::SeatForecastRepository;
use crate::domain::forecast_read::{
    RetainedStorageTotalsRow, SeatForecastPriceRow, UsageSnapshotRow,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, HoldingId};
use sqlx::{PgConnection, Row};
use std::future::Future;

const PRICE_COLUMNS: &str = "id,effective_month,base_monthly_satang::text base_monthly_satang,\
 included_original_bytes::text included_original_bytes,\
 storage_satang_per_gb_month::text storage_satang_per_gb_month,\
 ai_markup_percent::bigint ai_markup_percent,seat_terms_status,\
 included_seats::bigint included_seats,seat_monthly_satang::text seat_monthly_satang,\
 storage_allowance_mode,rating_enabled";

fn price(
    row: sqlx::postgres::PgRow,
) -> Result<SeatForecastPriceRow, PgTransactionError<BusinessError>> {
    Ok(SeatForecastPriceRow {
        id: row.try_get("id").map_err(|_| source_invalid())?,
        effective_month: row
            .try_get("effective_month")
            .map_err(|_| source_invalid())?,
        base_monthly_satang: row
            .try_get("base_monthly_satang")
            .map_err(|_| source_invalid())?,
        included_original_bytes: row
            .try_get("included_original_bytes")
            .map_err(|_| source_invalid())?,
        storage_satang_per_gb_month: row
            .try_get("storage_satang_per_gb_month")
            .map_err(|_| source_invalid())?,
        ai_markup_percent: row
            .try_get("ai_markup_percent")
            .map_err(|_| source_invalid())?,
        seat_terms_status: row
            .try_get("seat_terms_status")
            .map_err(|_| source_invalid())?,
        included_seats: row
            .try_get("included_seats")
            .map_err(|_| source_invalid())?,
        seat_monthly_satang: row
            .try_get("seat_monthly_satang")
            .map_err(|_| source_invalid())?,
        storage_allowance_mode: row
            .try_get("storage_allowance_mode")
            .map_err(|_| source_invalid())?,
        rating_enabled: row
            .try_get("rating_enabled")
            .map_err(|_| source_invalid())?,
    })
}

fn source_invalid() -> BusinessError {
    crate::domain::forecast_read::source_invalid()
}

pub struct PgSeatForecastRepository<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgSeatForecastRepository<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
}

impl SeatForecastRepository for PgSeatForecastRepository<'_> {
    type Error = PgTransactionError<BusinessError>;

    fn current_price(
        &mut self,
        forecast_month: &str,
    ) -> impl Future<Output = Result<Option<SeatForecastPriceRow>, Self::Error>> + Send {
        let sql = format!(
            "SELECT {PRICE_COLUMNS} FROM billing_target_prices \
             WHERE rating_enabled=TRUE AND seat_terms_status='defined' \
             AND base_monthly_satang=0 AND included_seats=0 \
             AND seat_monthly_satang IS NOT NULL \
             AND storage_allowance_mode='per_paid_seat_time' \
             AND effective_month<=$1 ORDER BY effective_month DESC,id DESC LIMIT 1"
        );
        async move {
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(forecast_month)
                .fetch_optional(&mut *self.connection)
                .await?
                .map(price)
                .transpose()
        }
    }

    fn future_price(
        &mut self,
        forecast_month: &str,
    ) -> impl Future<Output = Result<Option<SeatForecastPriceRow>, Self::Error>> + Send {
        let sql = format!(
            "SELECT {PRICE_COLUMNS} FROM billing_target_prices \
             WHERE rating_enabled=TRUE AND seat_terms_status='defined' \
             AND base_monthly_satang=0 AND included_seats=0 \
             AND seat_monthly_satang IS NOT NULL \
             AND storage_allowance_mode='per_paid_seat_time' \
             AND effective_month>$1 ORDER BY effective_month ASC,id ASC LIMIT 1"
        );
        async move {
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(forecast_month)
                .fetch_optional(&mut *self.connection)
                .await?
                .map(price)
                .transpose()
        }
    }

    fn usage_snapshot(
        &mut self,
        holding: &HoldingId,
        source_month: &str,
    ) -> impl Future<Output = Result<Option<UsageSnapshotRow>, Self::Error>> + Send {
        async move {
            let row = sqlx::query(
                "SELECT snapshot_id,refreshed_at,source_count::bigint source_count,\
                 storage_coverage,active_seat_count::bigint active_seat_count,\
                 active_seat_observed_at FROM billing_usage_snapshots \
                 WHERE holding_id=$1 AND month=$2",
            )
            .bind(holding.as_str())
            .bind(source_month)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(UsageSnapshotRow {
                    snapshot_id: row.try_get("snapshot_id")?,
                    refreshed_at: row.try_get("refreshed_at")?,
                    source_count: i32::try_from(row.try_get::<i64, _>("source_count")?)
                        .map_err(|_| source_invalid())?,
                    storage_coverage: row.try_get("storage_coverage")?,
                    active_seat_count: row.try_get("active_seat_count")?,
                    active_seat_observed_at: row.try_get("active_seat_observed_at")?,
                })
            })
            .transpose()
        }
    }

    fn retained_storage_totals(
        &mut self,
        holding: &HoldingId,
        source_month: &str,
        snapshot_id: &str,
    ) -> impl Future<Output = Result<Option<RetainedStorageTotalsRow>, Self::Error>> + Send {
        async move {
            let row: (String, String) = sqlx::query_as(
                "SELECT COUNT(*)::text row_count,\
                 COALESCE(SUM(quantity) FILTER (WHERE meter_kind='storage' AND active_at_period_end),0)::text active_original_bytes \
                 FROM billing_usage_meters WHERE holding_id=$1 AND month=$2 AND snapshot_id=$3",
            )
            .bind(holding.as_str())
            .bind(source_month)
            .bind(snapshot_id)
            .fetch_one(&mut *self.connection)
            .await?;
            Ok(Some(RetainedStorageTotalsRow {
                row_count: row.0,
                active_original_bytes: row.1,
            }))
        }
    }
}

pub async fn read_seat_forecast_baseline(
    connection: &mut PgConnection,
    holding: &HoldingId,
    now: i64,
) -> Result<crate::domain::forecast_read::SeatForecastBaseline, PgTransactionError<BusinessError>> {
    crate::application::forecast_read::read_seat_forecast_baseline(
        &mut PgSeatForecastRepository::new(connection),
        holding,
        erpx_server_shared::public::TimestampMillis::new(now),
    )
    .await
}
