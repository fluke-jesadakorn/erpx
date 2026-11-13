//! PostgreSQL adapter for the native PMS booking-refund slice.
//!
//! Only PMS-owned rows are read or written: `pms_booking_payments`,
//! `pms_booking_refund_requests`, `pms_booking_allocations`, `pms_booking_*`
//! attempts and `pms_records`/`pms_properties`. The caller owns the
//! transaction; this adapter never commits, retries, calls a provider or mints
//! authority.
use crate::application::refund::{PmsRefundProperty, PmsRefundRepository};
use crate::application::room_board::PmsBoardFuture;
use crate::domain::refund::{
    MAX_REFUND_ROWS, PmsRefundFinanceRow, PmsRefundPayment, PmsRefundPrior, PmsRefundRequest,
    PmsRefundSource, record_invalid_for_adapter, unavailable,
};
use crate::domain::room_board::PmsRecord;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use sqlx::{PgConnection, Row, postgres::PgRow};

type Error = PgTransactionError<BusinessError>;

fn payment_row(row: &PgRow) -> Result<PmsRefundPayment, Error> {
    Ok(PmsRefundPayment {
        id: row.try_get("id")?,
        property_id: row.try_get("property_id")?,
        reservation_id: row.try_get("reservation_id")?,
        company_id: row.try_get("company_id")?,
        holding_slug: row.try_get("holding_slug")?,
        account_id: row.try_get("account_id")?,
        livemode: row.try_get("livemode")?,
        amount_minor: row.try_get("amount_minor")?,
        status: row.try_get("status")?,
        checkout_id: row.try_get("checkout_id")?,
        clearing_account: row.try_get::<Option<String>, _>("clearing_account")?,
        methods_json: row.try_get("methods_json")?,
        created_at: row.try_get("created_at")?,
        provider_succeeded_at: row.try_get("provider_succeeded_at")?,
    })
}

fn request_row(row: &PgRow) -> Result<PmsRefundRequest, Error> {
    Ok(PmsRefundRequest {
        id: row.try_get("id")?,
        payment_id: row.try_get("payment_id")?,
        amount_minor: row.try_get("amount_minor")?,
        allocations_json: row.try_get("allocations_json")?,
        reason: row.try_get("reason")?,
        requested_by: row.try_get("requested_by")?,
        approved_by: row.try_get("approved_by")?,
        provider_id: row.try_get("provider_id")?,
        state: row.try_get("state")?,
        generation: row.try_get("generation")?,
        legacy_review: row.try_get("legacy_review").unwrap_or(false),
    })
}

fn record_row(row: &PgRow) -> Result<PmsRecord, Error> {
    let raw: String = row.try_get("data_json")?;
    let data: Value = serde_json::from_str(&raw).map_err(|_| Error::from(record_invalid()))?;
    Ok(PmsRecord {
        id: row.try_get("id")?,
        kind: row.try_get("kind")?,
        version: row.try_get("version")?,
        updated_at: row.try_get("updated_at")?,
        data,
    })
}

fn record_json(record: &PmsRecord) -> Result<String, Error> {
    serde_json::to_string(&record.data).map_err(|_| Error::from(record_invalid()))
}

fn record_invalid() -> BusinessError {
    record_invalid_for_adapter()
}

/// PMS-owned refund repository bound to one caller-owned connection.
pub struct PgPmsRefundRepository<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgPmsRefundRepository<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

impl PmsRefundRepository for PgPmsRefundRepository<'_> {
    type Error = Error;

    fn payment<'a>(
        &'a mut self,
        payment_id: &'a str,
        property_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<PmsRefundPayment>, Self::Error> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT * FROM pms_booking_payments WHERE id=$1 AND property_id=$2 FOR UPDATE",
            )
            .bind(payment_id)
            .bind(property_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.as_ref().map(payment_row).transpose()
        })
    }

    fn payment_by_id<'a>(
        &'a mut self,
        payment_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<PmsRefundPayment>, Self::Error> {
        Box::pin(async move {
            let row = sqlx::query("SELECT * FROM pms_booking_payments WHERE id=$1")
                .bind(payment_id)
                .fetch_optional(&mut *self.connection)
                .await?;
            row.as_ref().map(payment_row).transpose()
        })
    }

    fn request<'a>(
        &'a mut self,
        request_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<PmsRefundRequest>, Self::Error> {
        Box::pin(async move {
            let row = sqlx::query("SELECT * FROM pms_booking_refund_requests WHERE id=$1")
                .bind(request_id)
                .fetch_optional(&mut *self.connection)
                .await?;
            row.as_ref().map(request_row).transpose()
        })
    }

    fn locked_request<'a>(
        &'a mut self,
        request_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<PmsRefundRequest>, Self::Error> {
        Box::pin(async move {
            let row =
                sqlx::query("SELECT * FROM pms_booking_refund_requests WHERE id=$1 FOR UPDATE")
                    .bind(request_id)
                    .fetch_optional(&mut *self.connection)
                    .await?;
            row.as_ref().map(request_row).transpose()
        })
    }

    fn sources<'a>(
        &'a mut self,
        payment_id: &'a str,
    ) -> PmsBoardFuture<'a, Vec<PmsRefundSource>, Self::Error> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT stay_id,guest_account_id,entry_id,amount_minor \
                 FROM pms_booking_allocations WHERE payment_id=$1 ORDER BY stay_id",
            )
            .bind(payment_id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter()
                .map(|row| {
                    Ok(PmsRefundSource {
                        stay_id: row.try_get("stay_id")?,
                        guest_account_id: row.try_get("guest_account_id")?,
                        entry_id: row.try_get("entry_id")?,
                        amount_minor: row.try_get("amount_minor")?,
                    })
                })
                .collect()
        })
    }

    fn previous<'a>(
        &'a mut self,
        payment_id: &'a str,
    ) -> PmsBoardFuture<'a, Vec<PmsRefundPrior>, Self::Error> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT amount_minor,allocations_json FROM pms_booking_refund_requests \
                 WHERE payment_id=$1 ORDER BY created_at LIMIT 101",
            )
            .bind(payment_id)
            .fetch_all(&mut *self.connection)
            .await?;
            if rows.len() > 100 {
                return Err(Error::from(unavailable(
                    "PMS_REFUND_READ_LIMIT",
                    "There are more refunds than this view can read.",
                )));
            }
            rows.iter()
                .map(|row| {
                    Ok(PmsRefundPrior {
                        amount_minor: row.try_get("amount_minor")?,
                        allocations_json: row.try_get("allocations_json")?,
                    })
                })
                .collect()
        })
    }

    fn account<'a>(
        &'a mut self,
        property_id: &'a str,
        account_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<PmsRecord>, Self::Error> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT id,kind,data_json,version,updated_at FROM pms_records \
                 WHERE property_id=$1 AND id=$2 AND kind='guest_account' FOR UPDATE",
            )
            .bind(property_id)
            .bind(account_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.as_ref().map(record_row).transpose()
        })
    }

    fn entries<'a>(
        &'a mut self,
        property_id: &'a str,
        guest_account_id: &'a str,
    ) -> PmsBoardFuture<'a, Vec<PmsRecord>, Self::Error> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT id,kind,data_json,version,updated_at FROM pms_records \
                 WHERE property_id=$1 AND kind='entry' \
                   AND data_json::jsonb->>'guestAccountId'=$2 \
                 ORDER BY updated_at,id LIMIT 501",
            )
            .bind(property_id)
            .bind(guest_account_id)
            .fetch_all(&mut *self.connection)
            .await?;
            if rows.len() > 500 {
                return Err(Error::from(unavailable(
                    "PMS_REFUND_READ_LIMIT",
                    "The guest account has more entries than this command can read.",
                )));
            }
            rows.iter().map(record_row).collect()
        })
    }

    fn property<'a>(
        &'a mut self,
        property_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<PmsRefundProperty>, Self::Error> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT id,company_id,branch_id,business_date,config_json FROM pms_properties WHERE id=$1",
            )
            .bind(property_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.as_ref()
                .map(|row| {
                    Ok(PmsRefundProperty {
                        id: row.try_get("id")?,
                        company_id: row.try_get("company_id")?,
                        branch_id: row.try_get("branch_id")?,
                        business_date: row.try_get("business_date")?,
                        config_json: row.try_get("config_json")?,
                    })
                })
                .transpose()
        })
    }

    fn save_account<'a>(
        &'a mut self,
        account: &'a PmsRecord,
    ) -> PmsBoardFuture<'a, (), Self::Error> {
        Box::pin(async move {
            if account.version < 2 {
                return Err(Error::from(record_invalid()));
            }
            let data = record_json(account)?;
            let changed = sqlx::query(
                "UPDATE pms_records SET data_json=$1::jsonb,version=$2,updated_at=$3 \
                 WHERE id=$4 AND kind='guest_account' AND version=$5",
            )
            .bind(data)
            .bind(account.version)
            .bind(account.updated_at)
            .bind(&account.id)
            .bind(account.version - 1)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(Error::from(crate::domain::refund::conflict(
                    "PMS_REFUND_STATE",
                    "Refund state changed",
                )));
            }
            Ok(())
        })
    }

    fn insert_entry<'a>(
        &'a mut self,
        property_id: &'a str,
        entry: &'a PmsRecord,
    ) -> PmsBoardFuture<'a, (), Self::Error> {
        Box::pin(async move {
            if entry.kind != "entry" || entry.version != 1 {
                return Err(Error::from(record_invalid()));
            }
            let data = record_json(entry)?;
            let changed = sqlx::query(
                "INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) \
                 VALUES($1,$2,'entry',$3::jsonb,$4,$5) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&entry.id)
            .bind(property_id)
            .bind(data)
            .bind(entry.version)
            .bind(entry.updated_at)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed == 0 {
                // A replayed settle attempt keeps the first retained entry.
                return Ok(());
            }
            Ok(())
        })
    }

    fn insert_request<'a>(
        &'a mut self,
        request: &'a PmsRefundRequest,
        now: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error> {
        Box::pin(async move {
            sqlx::query(
                "INSERT INTO pms_booking_refund_requests(\
                   id,payment_id,amount_minor,allocations_json,reason,requested_by,state,\
                   created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,'requested',$7,$7) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&request.id)
            .bind(&request.payment_id)
            .bind(request.amount_minor)
            .bind(&request.allocations_json)
            .bind(&request.reason)
            .bind(&request.requested_by)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn mark_payment_refund<'a>(
        &'a mut self,
        payment_id: &'a str,
        state: &'a str,
        requested_at: Option<i64>,
        approved_by: Option<&'a str>,
        refund_id: Option<&'a str>,
        now: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error> {
        Box::pin(async move {
            sqlx::query(
                "UPDATE pms_booking_payments SET \
                   refund_state=COALESCE($2,refund_state),\
                   refund_requested_at=COALESCE($3,refund_requested_at),\
                   refund_approved_by=COALESCE($4,refund_approved_by),\
                   refund_id=COALESCE($5,refund_id),updated_at=$6 \
                 WHERE id=$1",
            )
            .bind(payment_id)
            .bind(state)
            .bind(requested_at)
            .bind(approved_by)
            .bind(refund_id)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn update_request<'a>(
        &'a mut self,
        request_id: &'a str,
        state: &'a str,
        approved_by: Option<&'a str>,
        provider_id: Option<&'a str>,
        last_error: Option<&'a str>,
        now: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error> {
        Box::pin(async move {
            let retrying = state == "approved" && last_error.is_some();
            sqlx::query(
                "UPDATE pms_booking_refund_requests SET \
                   state=$2,\
                   approved_by=COALESCE($3,approved_by),\
                   provider_id=CASE WHEN $7 THEN NULL ELSE COALESCE($4,provider_id) END,\
                   generation=CASE WHEN $7 THEN generation+1 ELSE generation END,\
                   last_error=CASE WHEN $2='succeeded' THEN NULL ELSE COALESCE($5,last_error) END,\
                   legacy_review=CASE WHEN $2='approved' THEN false ELSE legacy_review END,\
                   updated_at=$6 \
                 WHERE id=$1",
            )
            .bind(request_id)
            .bind(state)
            .bind(approved_by)
            .bind(provider_id)
            .bind(last_error)
            .bind(now)
            .bind(retrying)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn insert_attempt<'a>(
        &'a mut self,
        request_id: &'a str,
        generation: i64,
        provider_id: &'a str,
        now: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error> {
        Box::pin(async move {
            sqlx::query(
                "INSERT INTO pms_booking_refund_attempts(request_id,generation,provider_id,created_at) \
                 VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
            )
            .bind(request_id)
            .bind(generation)
            .bind(provider_id)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn bind_provider_refund<'a>(
        &'a mut self,
        request_id: &'a str,
        payment_id: &'a str,
        provider_id: &'a str,
        now: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error> {
        Box::pin(async move {
            sqlx::query(
                "UPDATE pms_booking_refund_requests SET provider_id=$2,state='processing',updated_at=$3 \
                 WHERE id=$1 AND provider_id IS NULL",
            )
            .bind(request_id)
            .bind(provider_id)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            sqlx::query(
                "UPDATE pms_booking_payments SET refund_id=$2,refund_state='processing',updated_at=$3 \
                 WHERE id=$1",
            )
            .bind(payment_id)
            .bind(provider_id)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn finance_rows<'a>(
        &'a mut self,
        company_id: &'a str,
    ) -> PmsBoardFuture<'a, Vec<PmsRefundFinanceRow>, Self::Error> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT r.id,p.id payment_id,p.property_id,r.amount_minor,p.status,\
                        r.state refund_state,r.last_error,r.reason,r.created_at,\
                        (p.status='refund_review' AND r.state='requested' \
                         AND r.allocations_json::jsonb='[]'::jsonb \
                         AND NOT EXISTS(SELECT 1 FROM pms_records e WHERE e.property_id=p.property_id \
                           AND e.kind='entry' \
                           AND e.data_json::jsonb->>'sourceKey'='online-payment:'||p.id)) needs_receipt_recovery \
                 FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id \
                 WHERE p.company_id=$1 ORDER BY r.created_at DESC LIMIT $2",
            )
            .bind(company_id)
            .bind(MAX_REFUND_ROWS as i64 + 1)
            .fetch_all(&mut *self.connection)
            .await?;
            if rows.len() > MAX_REFUND_ROWS {
                return Err(Error::from(unavailable(
                    "PMS_REFUND_READ_LIMIT",
                    "There are more refunds than this view can read.",
                )));
            }
            rows.iter()
                .map(|row| {
                    Ok(PmsRefundFinanceRow {
                        id: row.try_get("id")?,
                        payment_id: row.try_get("payment_id")?,
                        property_id: row.try_get("property_id")?,
                        amount_minor: row.try_get("amount_minor")?,
                        status: row.try_get("status")?,
                        refund_state: row.try_get("refund_state")?,
                        last_error: row.try_get("last_error")?,
                        reason: row.try_get("reason")?,
                        created_at: row.try_get("created_at")?,
                        needs_receipt_recovery: row.try_get("needs_receipt_recovery")?,
                    })
                })
                .collect()
        })
    }

    fn refundable_requests<'a>(
        &'a mut self,
        request_id: &'a str,
    ) -> PmsBoardFuture<'a, Vec<PmsRefundRequest>, Self::Error> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT * FROM pms_booking_refund_requests \
                 WHERE (id=$1 OR payment_id=$1) \
                   AND state IN ('approved','processing','requires_action') \
                 ORDER BY created_at LIMIT 21",
            )
            .bind(request_id)
            .fetch_all(&mut *self.connection)
            .await?;
            if rows.len() > 20 {
                return Err(Error::from(unavailable(
                    "PMS_REFUND_READ_LIMIT",
                    "There are more refunds than this view can read.",
                )));
            }
            rows.iter().map(request_row).collect()
        })
    }

    fn approval_target<'a>(
        &'a mut self,
        id: &'a str,
        company_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<(String, String)>, Self::Error> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT r.id,p.property_id FROM pms_booking_refund_requests r \
                 JOIN pms_booking_payments p ON p.id=r.payment_id \
                 WHERE (r.id=$1 OR p.id=$1) AND p.company_id=$2 AND r.state='requested' \
                 ORDER BY r.created_at LIMIT 1",
            )
            .bind(id)
            .bind(company_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.as_ref()
                .map(|row| Ok((row.try_get("id")?, row.try_get("property_id")?)))
                .transpose()
        })
    }

    fn recovery_payment<'a>(
        &'a mut self,
        request_id: &'a str,
        company_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<PmsRefundPayment>, Self::Error> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT p.* FROM pms_booking_payments p \
                 JOIN pms_booking_refund_requests r ON r.payment_id=p.id \
                 WHERE r.id=$1 AND p.company_id=$2 AND p.status='refund_review' \
                   AND r.state='requested'",
            )
            .bind(request_id)
            .bind(company_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.as_ref().map(payment_row).transpose()
        })
    }

    fn stay_account<'a>(
        &'a mut self,
        property_id: &'a str,
        reservation_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT data_json::jsonb->>'guestAccountId' account_id FROM pms_records \
                 WHERE property_id=$1 AND kind='stay' \
                   AND data_json::jsonb->>'reservationId'=$2 LIMIT 1",
            )
            .bind(property_id)
            .bind(reservation_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.and_then(|row| {
                row.try_get::<Option<String>, _>("account_id")
                    .ok()
                    .flatten()
            }))
        })
    }

    fn receipt_entry<'a>(
        &'a mut self,
        property_id: &'a str,
        payment_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<PmsRecord>, Self::Error> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT id,kind,data_json,version,updated_at FROM pms_records \
                 WHERE property_id=$1 AND kind='entry' \
                   AND data_json::jsonb->>'sourceKey'='online-payment:'||$2 LIMIT 1",
            )
            .bind(property_id)
            .bind(payment_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.as_ref().map(record_row).transpose()
        })
    }

    fn mark_receipt_recovered<'a>(
        &'a mut self,
        payment_id: &'a str,
        provider_intent_id: &'a str,
        succeeded_at: i64,
        now: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error> {
        Box::pin(async move {
            sqlx::query(
                "UPDATE pms_booking_payments SET paid_at=$2,provider_succeeded_at=$2,\
                   payment_intent_id=$3,last_error=NULL,updated_at=$4 WHERE id=$1",
            )
            .bind(payment_id)
            .bind(succeeded_at)
            .bind(provider_intent_id)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn append_reservation_history<'a>(
        &'a mut self,
        property_id: &'a str,
        reservation_id: &'a str,
        actor_id: &'a str,
        action: &'a str,
        now: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT data_json FROM pms_records \
                 WHERE property_id=$1 AND id=$2 AND kind='reservation' FOR UPDATE",
            )
            .bind(property_id)
            .bind(reservation_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else {
                return Ok(());
            };
            let raw: String = row.try_get("data_json")?;
            let mut data: Value =
                serde_json::from_str(&raw).map_err(|_| Error::from(record_invalid()))?;
            let Some(object) = data.as_object_mut() else {
                return Err(Error::from(record_invalid()));
            };
            let mut history = object
                .get("history")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            history.push(serde_json::json!({"at": now, "actor": actor_id, "action": action}));
            object.insert("history".into(), Value::Array(history));
            sqlx::query(
                "UPDATE pms_records SET data_json=$1::jsonb,version=version+1,updated_at=$2 \
                 WHERE property_id=$3 AND id=$4 AND kind='reservation'",
            )
            .bind(serde_json::to_string(&data).map_err(|_| Error::from(record_invalid()))?)
            .bind(now)
            .bind(property_id)
            .bind(reservation_id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn write_context<'a>(&'a mut self) -> PmsBoardFuture<'a, (), Self::Error> {
        Box::pin(async move {
            sqlx::query("SELECT set_config('erpx.pms_write','on',true)")
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
}
