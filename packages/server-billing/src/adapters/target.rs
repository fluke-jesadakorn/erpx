//! PostgreSQL read adapter for the Billing target billing period. The caller
//! owns the Control connection and transaction; this adapter never writes,
//! commits, calls a provider or approves a period.
use crate::application::target::TargetBillingRepository;
use crate::domain::target::{
    ApprovedExtrasRow, ApprovedInvoiceRow, ApprovedRatingRow, AssignmentRow, CostRow,
    CurrentSeatsRow, EvidenceMeterRow, MeasuredSeatIntervalsRow, MeterRow, OperatingCostsRow,
    PaymentConsentRow, PriceRow, RatingSeatIntervalsRow, SnapshotRow, StatementRow,
    TargetBillingPeriodView,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, HoldingId, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};
use std::future::Future;

fn source_invalid() -> BusinessError {
    crate::domain::target::source_invalid()
}

const SNAPSHOT_COLUMNS: &str = "snapshot_id,refreshed_at::text refreshed_at,\
 source_count::bigint source_count,ai_coverage,storage_coverage,\
 unreconciled_originals::bigint unreconciled_originals,seat_coverage,\
 seat_source_count::bigint seat_source_count";

const METER_COLUMNS: &str = "source_key,meter_kind,company_id,branch_id,funding_source,\
 quantity::text quantity,input_tokens::text input_tokens,output_tokens::text output_tokens,\
 request_status,gateway_status,provider,model,units_json,cost_status,\
 verified_cost_satang::text verified_cost_satang,cost_evidence_key,\
 cost_reconciled_at::text cost_reconciled_at,active_from::text active_from,\
 active_until::text active_until";

fn snapshot(row: &sqlx::postgres::PgRow) -> Result<SnapshotRow, PgTransactionError<BusinessError>> {
    Ok(SnapshotRow {
        snapshot_id: row.try_get("snapshot_id")?,
        refreshed_at: row.try_get("refreshed_at").map_err(|_| source_invalid())?,
        source_count: row.try_get("source_count").map_err(|_| source_invalid())?,
        ai_coverage: row.try_get("ai_coverage")?,
        storage_coverage: row.try_get("storage_coverage")?,
        unreconciled_originals: row
            .try_get("unreconciled_originals")
            .map_err(|_| source_invalid())?,
        seat_coverage: row.try_get("seat_coverage")?,
        seat_source_count: row
            .try_get("seat_source_count")
            .map_err(|_| source_invalid())?,
    })
}

fn meter(row: &sqlx::postgres::PgRow) -> Result<MeterRow, PgTransactionError<BusinessError>> {
    Ok(MeterRow {
        source_key: row.try_get("source_key")?,
        meter_kind: row.try_get("meter_kind")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        funding_source: row.try_get("funding_source")?,
        quantity: row.try_get("quantity").map_err(|_| source_invalid())?,
        input_tokens: row.try_get("input_tokens").map_err(|_| source_invalid())?,
        output_tokens: row.try_get("output_tokens").map_err(|_| source_invalid())?,
        request_status: row.try_get("request_status")?,
        provider: row.try_get("provider")?,
        model: row.try_get("model")?,
        gateway_status: row.try_get("gateway_status")?,
        units_json: row.try_get("units_json")?,
        cost_status: row.try_get("cost_status")?,
        verified_cost_satang: row
            .try_get("verified_cost_satang")
            .map_err(|_| source_invalid())?,
        cost_evidence_key: row.try_get("cost_evidence_key")?,
        cost_reconciled_at: row
            .try_get("cost_reconciled_at")
            .map_err(|_| source_invalid())?,
        active_from: row.try_get("active_from").map_err(|_| source_invalid())?,
        active_until: row.try_get("active_until").map_err(|_| source_invalid())?,
    })
}

fn price(row: &sqlx::postgres::PgRow) -> Result<PriceRow, PgTransactionError<BusinessError>> {
    Ok(PriceRow {
        id: row.try_get("id")?,
        effective_month: row.try_get("effective_month")?,
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
        seat_terms_status: row.try_get("seat_terms_status")?,
        included_seats: row
            .try_get("included_seats")
            .map_err(|_| source_invalid())?,
        seat_monthly_satang: row
            .try_get("seat_monthly_satang")
            .map_err(|_| source_invalid())?,
        storage_allowance_mode: row.try_get("storage_allowance_mode")?,
        rating_enabled: row.try_get("rating_enabled")?,
    })
}

fn statement(
    row: &sqlx::postgres::PgRow,
) -> Result<StatementRow, PgTransactionError<BusinessError>> {
    Ok(StatementRow {
        provider: row.try_get("provider")?,
        statement_id: row.try_get("statement_id")?,
        original_currency: row.try_get("original_currency")?,
        provider_total_minor: row
            .try_get("provider_total_minor")
            .map_err(|_| source_invalid())?,
        reconciled_total_minor: row
            .try_get("reconciled_total_minor")
            .map_err(|_| source_invalid())?,
        evidence_id: row.try_get("evidence_id")?,
        verified_at: row.try_get("verified_at").map_err(|_| source_invalid())?,
    })
}

pub struct PgTargetBillingRepository<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgTargetBillingRepository<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
}

impl TargetBillingRepository for PgTargetBillingRepository<'_> {
    type Error = PgTransactionError<BusinessError>;

    fn operating_costs(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<OperatingCostsRow>, Self::Error>> + Send {
        async move {
            let rows = sqlx::query(
                "SELECT o.category,o.amount_satang::text amount_satang,e.source_state \
                 FROM billing_target_operating_costs o \
                 JOIN billing_evidence e ON e.id=o.evidence_id \
                 WHERE o.holding_id=$1 AND o.month=$2",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(OperatingCostsRow {
                        category: row.try_get("category")?,
                        amount_satang: row
                            .try_get("amount_satang")
                            .map_err(|_| source_invalid())?,
                        source_state: row.try_get("source_state")?,
                    })
                })
                .collect()
        }
    }

    fn measured_snapshot(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Option<SnapshotRow>, Self::Error>> + Send {
        let sql = format!(
            "SELECT {SNAPSHOT_COLUMNS} FROM billing_usage_snapshots \
             WHERE holding_id=$1 AND month=$2"
        );
        async move {
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(holding.as_str())
                .bind(month)
                .fetch_optional(&mut *self.connection)
                .await?
                .as_ref()
                .map(snapshot)
                .transpose()
        }
    }

    fn current_seats(
        &mut self,
        holding: &HoldingId,
        billing_month: &str,
    ) -> impl Future<Output = Result<Option<CurrentSeatsRow>, Self::Error>> + Send {
        async move {
            let row = sqlx::query(
                "SELECT active_seat_count::bigint active_seat_count,\
                 active_seat_observed_at::text active_seat_observed_at \
                 FROM billing_usage_snapshots WHERE holding_id=$1 AND month=$2",
            )
            .bind(holding.as_str())
            .bind(billing_month)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(CurrentSeatsRow {
                    active_seat_count: row
                        .try_get("active_seat_count")
                        .map_err(|_| source_invalid())?,
                    active_seat_observed_at: row
                        .try_get("active_seat_observed_at")
                        .map_err(|_| source_invalid())?,
                })
            })
            .transpose()
        }
    }

    fn measured_meters(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<EvidenceMeterRow>, Self::Error>> + Send {
        let sql = format!(
            "SELECT {METER_COLUMNS},active_at_period_end,snapshot_id,\
             byte_milliseconds::text byte_milliseconds FROM billing_usage_meters \
             WHERE holding_id=$1 AND month=$2 ORDER BY source_key"
        );
        async move {
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(holding.as_str())
                .bind(month)
                .fetch_all(&mut *self.connection)
                .await?;
            rows.iter()
                .map(|row| {
                    Ok(EvidenceMeterRow {
                        meter: meter(row)?,
                        active_at_period_end: row.try_get("active_at_period_end")?,
                        byte_milliseconds: row
                            .try_get("byte_milliseconds")
                            .map_err(|_| source_invalid())?,
                        snapshot_id: row.try_get("snapshot_id")?,
                    })
                })
                .collect()
        }
    }

    fn measured_seat_intervals(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<MeasuredSeatIntervalsRow>, Self::Error>> + Send {
        async move {
            let rows = sqlx::query(
                "SELECT intervals_json,snapshot_id FROM billing_seat_usage \
                 WHERE holding_id=$1 AND month=$2",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter()
                .map(|row| {
                    Ok(MeasuredSeatIntervalsRow {
                        intervals_json: row.try_get("intervals_json")?,
                        snapshot_id: row.try_get("snapshot_id")?,
                    })
                })
                .collect()
        }
    }

    fn assignment(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Option<AssignmentRow>, Self::Error>> + Send {
        async move {
            let row = sqlx::query(
                "SELECT holding_id,effective_month,price_id,mode,\
                 service_started_at::text service_started_at \
                 FROM billing_target_assignments WHERE holding_id=$1 \
                 AND effective_month<=$2 ORDER BY effective_month DESC LIMIT 1",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(AssignmentRow {
                    holding_id: row.try_get("holding_id")?,
                    effective_month: row.try_get("effective_month")?,
                    price_id: row.try_get("price_id")?,
                    mode: row.try_get("mode")?,
                    service_started_at: row
                        .try_get("service_started_at")
                        .map_err(|_| source_invalid())?,
                })
            })
            .transpose()
        }
    }

    fn assigned_price(
        &mut self,
        price_id: &str,
        month: &str,
    ) -> impl Future<Output = Result<Option<PriceRow>, Self::Error>> + Send {
        async move {
            let row = sqlx::query(
                "SELECT id,effective_month,base_monthly_satang::text base_monthly_satang,\
                 included_original_bytes::text included_original_bytes,\
                 storage_satang_per_gb_month::text storage_satang_per_gb_month,\
                 ai_markup_percent::bigint ai_markup_percent,seat_terms_status,\
                 included_seats::bigint included_seats,\
                 seat_monthly_satang::text seat_monthly_satang,\
                 storage_allowance_mode,rating_enabled FROM billing_target_prices \
                 WHERE id=$1 AND effective_month<=$2",
            )
            .bind(price_id)
            .bind(month)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.as_ref().map(price).transpose()
        }
    }

    fn approved_rating(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Option<ApprovedRatingRow>, Self::Error>> + Send {
        async move {
            let row = sqlx::query(
                "SELECT state,missing_json,lines_json,total_satang::text total_satang,\
                 meter_snapshot_id,invoice_id FROM billing_target_rated_periods \
                 WHERE holding_id=$1 AND month=$2 AND state='approved'",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(ApprovedRatingRow {
                    state: row.try_get("state")?,
                    missing_json: row.try_get("missing_json")?,
                    lines_json: row.try_get("lines_json")?,
                    total_satang: row.try_get("total_satang").map_err(|_| source_invalid())?,
                    meter_snapshot_id: row.try_get("meter_snapshot_id")?,
                    invoice_id: row.try_get("invoice_id")?,
                })
            })
            .transpose()
        }
    }

    fn approved_invoice(
        &mut self,
        invoice_id: &str,
    ) -> impl Future<Output = Result<Option<ApprovedInvoiceRow>, Self::Error>> + Send {
        async move {
            let row = sqlx::query(
                "SELECT id,total_satang::text total_satang,status,stripe_id \
                 FROM billing_invoices WHERE id=$1",
            )
            .bind(invoice_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(ApprovedInvoiceRow {
                    id: row.try_get("id")?,
                    total_satang: row.try_get("total_satang").map_err(|_| source_invalid())?,
                    status: row.try_get("status")?,
                    stripe_id: row.try_get("stripe_id")?,
                })
            })
            .transpose()
        }
    }

    fn approved_statements(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<StatementRow>, Self::Error>> + Send {
        async move {
            let rows = sqlx::query(
                "SELECT provider,statement_id,original_currency,\
                 provider_total_minor::text provider_total_minor,\
                 reconciled_total_minor::text reconciled_total_minor,evidence_id,\
                 verified_at::text verified_at FROM billing_target_statements \
                 WHERE holding_id=$1 AND month=$2",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter().map(statement).collect()
        }
    }

    fn rollout_eligibility(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        async move {
            let row = sqlx::query(
                "SELECT 1 FROM billing_target_sandbox_holdings WHERE holding_id=$1 \
                 UNION ALL SELECT 1 FROM billing_target_rollout_approvals \
                 WHERE holding_id=$1 AND effective_month<=$2 LIMIT 1",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.is_some())
        }
    }

    fn legacy_enrollment_overlap(
        &mut self,
        holding: &HoldingId,
        start: i64,
        end: i64,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        async move {
            let row = sqlx::query(
                "SELECT 1 FROM billing_enrollments WHERE holding_id=$1 \
                 AND plan_id<>'platform_usage' AND started_at<$3 \
                 AND COALESCE(ended_at,effective_at,9223372036854775807)>$2 LIMIT 1",
            )
            .bind(holding.as_str())
            .bind(start)
            .bind(end)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.is_some())
        }
    }

    fn month_invoices(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        async move {
            let row = sqlx::query(
                "SELECT 1 FROM billing_invoices i JOIN billing_periods p ON p.id=i.period_id \
                 WHERE i.holding_id=$1 AND p.month=$2 AND i.status<>'void' LIMIT 1",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.is_some())
        }
    }

    fn payment_consent(
        &mut self,
        holding: &HoldingId,
    ) -> impl Future<Output = Result<Option<PaymentConsentRow>, Self::Error>> + Send {
        async move {
            let row = sqlx::query(
                "SELECT payment_mode,consent_at::text consent_at,card_json \
                 FROM billing_accounts WHERE holding_id=$1",
            )
            .bind(holding.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(PaymentConsentRow {
                    payment_mode: row.try_get("payment_mode")?,
                    consent_at: row.try_get("consent_at").map_err(|_| source_invalid())?,
                    card_json: row.try_get("card_json")?,
                })
            })
            .transpose()
        }
    }

    fn rating_snapshot(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Option<SnapshotRow>, Self::Error>> + Send {
        let sql = format!(
            "SELECT {SNAPSHOT_COLUMNS} FROM billing_usage_snapshots \
             WHERE holding_id=$1 AND month=$2"
        );
        async move {
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(holding.as_str())
                .bind(month)
                .fetch_optional(&mut *self.connection)
                .await?
                .as_ref()
                .map(snapshot)
                .transpose()
        }
    }

    fn rating_meters(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<MeterRow>, Self::Error>> + Send {
        let sql = format!(
            "SELECT {METER_COLUMNS} FROM billing_usage_meters \
             WHERE holding_id=$1 AND month=$2 ORDER BY source_key"
        );
        async move {
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(holding.as_str())
                .bind(month)
                .fetch_all(&mut *self.connection)
                .await?;
            rows.iter().map(meter).collect()
        }
    }

    fn provider_costs(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<CostRow>, Self::Error>> + Send {
        async move {
            let rows = sqlx::query(
                "SELECT source_key,provider,statement_id,original_currency,\
                 original_minor::text original_minor,\
                 provider_cost_satang::text provider_cost_satang,evidence_id \
                 FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter()
                .map(|row| {
                    Ok(CostRow {
                        source_key: row.try_get("source_key")?,
                        provider: row.try_get("provider")?,
                        statement_id: row.try_get("statement_id")?,
                        original_currency: row.try_get("original_currency")?,
                        original_minor: row
                            .try_get("original_minor")
                            .map_err(|_| source_invalid())?,
                        provider_cost_satang: row
                            .try_get("provider_cost_satang")
                            .map_err(|_| source_invalid())?,
                        evidence_id: row.try_get("evidence_id")?,
                    })
                })
                .collect()
        }
    }

    fn provider_statements(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<StatementRow>, Self::Error>> + Send {
        async move {
            let rows = sqlx::query(
                "SELECT provider,statement_id,original_currency,\
                 provider_total_minor::text provider_total_minor,\
                 reconciled_total_minor::text reconciled_total_minor,evidence_id,\
                 verified_at::text verified_at FROM billing_target_statements \
                 WHERE holding_id=$1 AND month=$2",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter().map(statement).collect()
        }
    }

    fn approved_extras(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<ApprovedExtrasRow>, Self::Error>> + Send {
        async move {
            let rows = sqlx::query(
                "SELECT id,kind,description,amount_satang::text amount_satang,evidence_id \
                 FROM billing_target_extras WHERE holding_id=$1 AND month=$2 \
                 ORDER BY approved_at,id",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter()
                .map(|row| {
                    Ok(ApprovedExtrasRow {
                        id: row.try_get("id")?,
                        kind: row.try_get("kind")?,
                        description: row.try_get("description")?,
                        amount_satang: row
                            .try_get("amount_satang")
                            .map_err(|_| source_invalid())?,
                        evidence_id: row.try_get("evidence_id")?,
                    })
                })
                .collect()
        }
    }

    fn rating_seat_intervals(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<RatingSeatIntervalsRow>, Self::Error>> + Send {
        async move {
            let rows = sqlx::query(
                "SELECT user_key,intervals_json,snapshot_id FROM billing_seat_usage \
                 WHERE holding_id=$1 AND month=$2 ORDER BY user_key",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter()
                .map(|row| {
                    Ok(RatingSeatIntervalsRow {
                        user_key: row.try_get("user_key")?,
                        intervals_json: row.try_get("intervals_json")?,
                        snapshot_id: row.try_get("snapshot_id")?,
                    })
                })
                .collect()
        }
    }
}

pub async fn read_target_billing_period(
    connection: &mut PgConnection,
    holding: &HoldingId,
    month: &str,
    now: TimestampMillis,
) -> Result<TargetBillingPeriodView, PgTransactionError<BusinessError>> {
    crate::application::target::read_target_billing_period(
        &mut PgTargetBillingRepository::new(connection),
        holding,
        month,
        now,
    )
    .await
}

async fn serializable(
    connection: &mut PgConnection,
) -> Result<(), PgTransactionError<BusinessError>> {
    if !connection.is_in_transaction() {
        return Err(
            erpx_server_runtime::public::postgres::PersistenceError::new(
                "BILLING_COMMAND_TRANSACTION_REQUIRED",
            )
            .into(),
        );
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(
            erpx_server_runtime::public::postgres::PersistenceError::new(
                "BILLING_COMMAND_SERIALIZABLE_REQUIRED",
            )
            .into(),
        );
    }
    Ok(())
}

impl crate::application::target_rate_command::RateTargetRepository
    for PgTargetBillingRepository<'_>
{
    fn record_rating_run<'a>(
        &'a mut self,
        draft: &'a crate::application::target_rate_command::RateTargetDraft,
        actor: &'a crate::domain::actor::BillingActor,
        reason: &'a str,
        now: TimestampMillis,
    ) -> crate::application::BillingFuture<'a, (), Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            sqlx::query(
                "INSERT INTO billing_target_rating_runs\
                 (id,holding_id,month,price_id,meter_snapshot_id,state,missing_json,\
                  lines_json,total_satang,actor,reason,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
            )
            .bind(&draft.id)
            .bind(&draft.holding_id)
            .bind(&draft.month)
            .bind(&draft.price_id)
            .bind(&draft.snapshot_id)
            .bind(&draft.state)
            .bind(&draft.missing_json)
            .bind(&draft.lines_json)
            .bind(draft.total_satang)
            .bind(actor.user_id.as_str())
            .bind(reason)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn retain_rated_period<'a>(
        &'a mut self,
        draft: &'a crate::application::target_rate_command::RateTargetDraft,
        now: TimestampMillis,
    ) -> crate::application::BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "INSERT INTO billing_target_rated_periods\
                 (holding_id,month,price_id,assignment_month,mode,meter_snapshot_id,\
                  service_start,service_end,state,missing_json,lines_json,total_satang,rated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) \
                 ON CONFLICT(holding_id,month) DO UPDATE SET \
                  price_id=excluded.price_id,assignment_month=excluded.assignment_month,\
                  mode=excluded.mode,meter_snapshot_id=excluded.meter_snapshot_id,\
                  service_start=excluded.service_start,service_end=excluded.service_end,\
                  state=excluded.state,missing_json=excluded.missing_json,\
                  lines_json=excluded.lines_json,total_satang=excluded.total_satang,\
                  rated_at=excluded.rated_at \
                 WHERE billing_target_rated_periods.state<>'approved' RETURNING holding_id",
            )
            .bind(&draft.holding_id)
            .bind(&draft.month)
            .bind(&draft.price_id)
            .bind(&draft.assignment_month)
            .bind(&draft.mode)
            .bind(&draft.snapshot_id)
            .bind(draft.service_start)
            .bind(draft.service_end)
            .bind(&draft.state)
            .bind(&draft.missing_json)
            .bind(&draft.lines_json)
            .bind(draft.total_satang)
            .bind(now.get())
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.is_some())
        })
    }

    fn digest<'a>(
        &'a mut self,
        value: &'a str,
    ) -> crate::application::BillingFuture<'a, String, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(value)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn lock_holding<'a>(
        &'a mut self,
        holding: &'a HoldingId,
    ) -> crate::application::BillingFuture<'a, (), Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("billing:{}", holding.as_str()))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn lock_command<'a>(
        &'a mut self,
        key: &'a str,
    ) -> crate::application::BillingFuture<'a, (), Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("billing-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> crate::application::BillingFuture<
        'a,
        Option<crate::application::cost::ReceiptSnapshot>,
        Self::Error,
    > {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query("SELECT digest,result_json FROM billing_receipts WHERE id=$1")
                .bind(key)
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(|row| {
                Ok::<_, PgTransactionError<BusinessError>>(
                    crate::application::cost::ReceiptSnapshot {
                        digest: row.try_get("digest")?,
                        result_json: row.try_get("result_json")?,
                    },
                )
            })
            .transpose()
        })
    }

    fn holding_exists<'a>(
        &'a mut self,
        holding: &'a HoldingId,
    ) -> crate::application::BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM holding_bindings WHERE holding_id=$1)",
            )
            .bind(holding.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn account_for_update<'a>(
        &'a mut self,
        holding: &'a HoldingId,
    ) -> crate::application::BillingFuture<
        'a,
        Option<crate::application::cost::AccountSnapshot>,
        Self::Error,
    > {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row =
                sqlx::query("SELECT version FROM billing_accounts WHERE holding_id=$1 FOR UPDATE")
                    .bind(holding.as_str())
                    .fetch_optional(&mut *self.connection)
                    .await?;
            row.map(|row| {
                Ok::<_, PgTransactionError<BusinessError>>(
                    crate::application::cost::AccountSnapshot {
                        version: i64::from(row.try_get::<i32, _>("version")?),
                    },
                )
            })
            .transpose()
        })
    }

    fn increment_account_version<'a>(
        &'a mut self,
        holding: &'a HoldingId,
        expected_version: i64,
    ) -> crate::application::BillingFuture<'a, Option<i64>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "UPDATE billing_accounts SET version=version+1 \
                 WHERE holding_id=$1 AND version=$2 RETURNING version",
            )
            .bind(holding.as_str())
            .bind(expected_version)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(|row| i64::from(row.get::<i32, _>("version"))))
        })
    }

    fn insert_audit<'a>(
        &'a mut self,
        entry: &'a crate::application::cost::AuditEntry<'a>,
        now: TimestampMillis,
    ) -> crate::application::BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let changed = sqlx::query(
                "INSERT INTO billing_audit(id,holding_id,actor,action,reason,details_json,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(format!("billing-audit-{}", entry.key))
            .bind(entry.target.as_str())
            .bind(entry.actor.user_id.as_str())
            .bind(entry.action)
            .bind(entry.reason)
            .bind(entry.details_json)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        key: &'a str,
        digest: &'a str,
        result_json: &'a str,
        now: TimestampMillis,
    ) -> crate::application::BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let changed = sqlx::query(
                "INSERT INTO billing_receipts(id,digest,result_json,created_at) \
                 VALUES($1,$2,$3,$4) ON CONFLICT(id) DO NOTHING",
            )
            .bind(key)
            .bind(digest)
            .bind(result_json)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn serialized_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> crate::application::BillingFuture<
        'a,
        Option<crate::application::cost::ReceiptSnapshot>,
        Self::Error,
    > {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query("SELECT digest,result_json FROM billing_receipts WHERE id=$1")
                .bind(key)
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(|row| {
                Ok::<_, PgTransactionError<BusinessError>>(
                    crate::application::cost::ReceiptSnapshot {
                        digest: row.try_get("digest")?,
                        result_json: row.try_get("result_json")?,
                    },
                )
            })
            .transpose()
        })
    }
}

pub async fn execute_rate_target(
    connection: &mut PgConnection,
    actor: &crate::domain::actor::BillingActor,
    target: &HoldingId,
    command: &crate::domain::target_rate_command::RateTarget,
    now: TimestampMillis,
) -> Result<crate::domain::command::CommandReceipt, PgTransactionError<BusinessError>> {
    crate::application::target_rate_command::execute_rate_target(
        actor,
        target,
        command,
        &mut PgTargetBillingRepository::new(connection),
        now,
    )
    .await
}
