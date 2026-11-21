//! PostgreSQL read adapter for the Billing target assignment/price slice.
use crate::application::target_assignment::TargetAssignmentRepository;
use crate::domain::target_assignment::{TargetAssignmentRow, TargetPriceRow};
use erpx_server_shared::public::{BusinessError, HoldingId};
use sqlx::{PgConnection, Row};
use std::future::Future;

pub struct PgTargetAssignmentRepository<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgTargetAssignmentRepository<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
}

impl TargetAssignmentRepository for PgTargetAssignmentRepository<'_> {
    type Error = erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>;

    fn assignment(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Option<TargetAssignmentRow>, Self::Error>> + Send {
        async move {
            let row = sqlx::query(
                "SELECT holding_id,effective_month,price_id,mode,service_started_at::text service_started_at \
                 FROM billing_target_assignments WHERE holding_id=$1 AND effective_month<=$2 \
                 ORDER BY effective_month DESC LIMIT 1",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(TargetAssignmentRow {
                    holding_id: row.try_get("holding_id")?,
                    effective_month: row.try_get("effective_month")?,
                    price_id: row.try_get("price_id")?,
                    mode: row.try_get("mode")?,
                    service_started_at: row.try_get("service_started_at")?,
                })
            })
            .transpose()
        }
    }

    fn assigned_price(
        &mut self,
        price_id: &str,
        month: &str,
    ) -> impl Future<Output = Result<Option<TargetPriceRow>, Self::Error>> + Send {
        async move {
            let row = sqlx::query(
                "SELECT id,effective_month,base_monthly_satang::text base_monthly_satang,\
                 included_original_bytes::text included_original_bytes,\
                 storage_satang_per_gb_month::text storage_satang_per_gb_month,\
                 ai_markup_percent,seat_terms_status,included_seats,\
                 seat_monthly_satang::text seat_monthly_satang,storage_allowance_mode,rating_enabled \
                 FROM billing_target_prices WHERE id=$1 AND effective_month<=$2",
            )
            .bind(price_id)
            .bind(month)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(TargetPriceRow {
                    id: row.try_get("id")?,
                    effective_month: row.try_get("effective_month")?,
                    base_monthly_satang: row.try_get("base_monthly_satang")?,
                    included_original_bytes: row.try_get("included_original_bytes")?,
                    storage_satang_per_gb_month: row.try_get("storage_satang_per_gb_month")?,
                    ai_markup_percent: row.try_get("ai_markup_percent")?,
                    seat_terms_status: row.try_get("seat_terms_status")?,
                    included_seats: row.try_get("included_seats")?,
                    seat_monthly_satang: row.try_get("seat_monthly_satang")?,
                    storage_allowance_mode: row.try_get("storage_allowance_mode")?,
                    rating_enabled: row.try_get("rating_enabled")?,
                })
            })
            .transpose()
        }
    }
}

pub async fn read_target_assignment(
    connection: &mut PgConnection,
    holding: &HoldingId,
    month: &str,
) -> Result<
    crate::domain::target_assignment::TargetAssignmentRead,
    erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>,
> {
    crate::application::target_assignment::read_target_assignment(
        &mut PgTargetAssignmentRepository::new(connection),
        holding,
        month,
    )
    .await
}
