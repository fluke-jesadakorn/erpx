//! PostgreSQL adapter for native POS register-session, bill, void and refund
//! commands. All SQL is POS-owned; cross-owner journals, parties, stock and
//! hotel facts remain caller-supplied ports.
use crate::application::commands::{PosCommandRepository, PosFuture};
use crate::domain::commands::{
    CashApproveInput, PosActor, PosCashMovement, PosCommand, PosFulfillment, PosFulfillmentInsert,
    PosFulfillmentSplitFacts, PosLine, PosLineDetail, PosLineDiscountRow, PosLineInsert,
    PosFulfillmentStock, PosLineSplitRow, PosPayment, PosPaymentInsert, PosReceipt, PosRefund,
    PosRegister, PosSale, PosSettings,
    PosShift, RefundRequestInput, SaleCreateInput,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
type Fut<'a, T> = PosFuture<'a, T, Error>;

pub struct PgPosCommandRepository<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgPosCommandRepository<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction()
            || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
                .fetch_one(&mut *self.connection)
                .await?
                != "serializable"
        {
            return Err(PersistenceError::new("POS_COMMAND_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }
}

impl PosCommandRepository for PgPosCommandRepository<'_> {
    type Error = Error;

    fn digest<'a>(&'a mut self, value: &'a str) -> Fut<'a, String> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(value)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }
    fn receipt<'a>(&'a mut self, key: &'a str) -> Fut<'a, Option<PosReceipt>> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query("SELECT request_hash,result_json FROM pos_receipts WHERE id=$1")
                .bind(key)
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(|row| {
                Ok::<_, Error>(PosReceipt {
                    request_hash: row.try_get("request_hash")?,
                    result_json: row.try_get("result_json")?,
                })
            })
            .transpose()
        })
    }
    fn insert_receipt<'a>(
        &'a mut self,
        key: &'a str,
        company: &'a CompanyId,
        request_hash: &'a str,
        result_json: &'a str,
        now: TimestampMillis,
    ) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "INSERT INTO pos_receipts(id,company_id,request_hash,result_json,created_at) \
                 VALUES($1,$2,$3,$4,$5) ON CONFLICT(id) DO NOTHING",
            )
            .bind(key)
            .bind(company.as_str())
            .bind(request_hash)
            .bind(result_json)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }
    fn audit<'a>(
        &'a mut self,
        actor: &'a PosActor,
        operation: &'a str,
        record_id: &'a str,
        input: &'a str,
        result: &'a str,
        now: TimestampMillis,
    ) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            let details = serde_json::json!({
                "input": serde_json::from_str::<serde_json::Value>(input).unwrap_or(serde_json::Value::Null),
                "result": serde_json::from_str::<serde_json::Value>(result).unwrap_or(serde_json::Value::Null),
                "assignmentId": actor.assignment_id.as_str(),
            })
            .to_string();
            sqlx::query(
                "INSERT INTO pos_audit(id,company_id,branch_id,actor_id,operation,record_id,details_json,created_at) \
                 VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.user_id.as_str())
            .bind(operation)
            .bind(record_id)
            .bind(details)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
    fn new_id(&mut self) -> Fut<'_, String> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar("SELECT gen_random_uuid()::text")
                .fetch_one(&mut *self.connection)
                .await?)
        })
    }
    fn business_date(&mut self, now: TimestampMillis) -> Fut<'_, String> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT to_char(to_timestamp($1::double precision/1000.0) AT TIME ZONE 'Asia/Bangkok','YYYY-MM-DD')",
            )
            .bind(now.get())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }
    fn same_person<'a>(
        &'a mut self,
        company: &'a CompanyId,
        left: &'a str,
        right: &'a str,
    ) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT COALESCE(iam_same_person($1,$2,$3),false)")
                    .bind(left)
                    .bind(right)
                    .bind(company.as_str())
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn settings<'a>(&'a mut self, company: &'a CompanyId) -> Fut<'a, Option<PosSettings>> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT enabled,currency,cashier_discount_bps,accounts_json,accounting_approved_by \
                 FROM pos_settings WHERE company_id=$1",
            )
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(PosSettings {
                    enabled: row.try_get("enabled")?,
                    currency: row.try_get("currency")?,
                    cashier_discount_bps: row.try_get("cashier_discount_bps")?,
                    accounts_json: row.try_get("accounts_json")?,
                    accounting_approved_by: row.try_get("accounting_approved_by")?,
                })
            })
            .transpose()
        })
    }
    fn register<'a>(
        &'a mut self,
        company: &'a CompanyId,
        register_id: &'a str,
    ) -> Fut<'a, Option<PosRegister>> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,company_id,branch_id,warehouse_id,name,mode,active,property_id \
                 FROM pos_registers WHERE id=$1 AND company_id=$2",
            )
            .bind(register_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(PosRegister {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    warehouse_id: row.try_get("warehouse_id")?,
                    name: row.try_get("name")?,
                    mode: row.try_get("mode")?,
                    active: row.try_get("active")?,
                    property_id: row.try_get("property_id")?,
                })
            })
            .transpose()
        })
    }
    fn shift<'a>(
        &'a mut self,
        company: &'a CompanyId,
        shift_id: &'a str,
    ) -> Fut<'a, Option<PosShift>> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,register_id,company_id,branch_id,business_date,opened_by,opening_minor,state,\
                 counted_minor,expected_minor,variance_minor,closed_by,version \
                 FROM pos_shifts WHERE id=$1 AND company_id=$2 FOR UPDATE",
            )
            .bind(shift_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(PosShift {
                    id: row.try_get("id")?,
                    register_id: row.try_get("register_id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    business_date: row.try_get("business_date")?,
                    opened_by: row.try_get("opened_by")?,
                    opening_minor: row.try_get("opening_minor")?,
                    state: row.try_get("state")?,
                    counted_minor: row.try_get("counted_minor")?,
                    expected_minor: row.try_get("expected_minor")?,
                    variance_minor: row.try_get("variance_minor")?,
                    closed_by: row.try_get("closed_by")?,
                    version: row.try_get("version")?,
                })
            })
            .transpose()
        })
    }
    fn shift_open_exists<'a>(&'a mut self, register_id: &'a str) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM pos_shifts WHERE register_id=$1 AND state='open')",
            )
            .bind(register_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }
    fn day_writable<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: &'a str,
        business_date: &'a str,
    ) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT NOT EXISTS(SELECT 1 FROM pos_day_closes \
                 WHERE company_id=$1 AND branch_id=$2 AND business_date=$3 AND state='prepared')",
            )
            .bind(company.as_str())
            .bind(branch_id)
            .bind(business_date)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }
    fn insert_shift<'a>(
        &'a mut self,
        id: &'a str,
        register: &'a PosRegister,
        actor: &'a PosActor,
        opening_minor: i64,
        business_date: &'a str,
        now: TimestampMillis,
    ) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,business_date) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(id)
            .bind(&register.id)
            .bind(&register.company_id)
            .bind(&register.branch_id)
            .bind(actor.user_id.as_str())
            .bind(now.get())
            .bind(opening_minor)
            .bind(business_date)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
    fn open_sales<'a>(&'a mut self, shift_id: &'a str) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM pos_sales WHERE shift_id=$1 AND state IN ('open','held','payment_pending'))",
            )
            .bind(shift_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }
    fn unapproved_cash<'a>(&'a mut self, shift_id: &'a str) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM pos_cash_movements WHERE shift_id=$1 AND approved_by IS NULL)",
            )
            .bind(shift_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }
    fn cash_sales<'a>(&'a mut self, shift_id: &'a str) -> Fut<'a, i64> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT COALESCE(SUM(p.amount_minor),0)::bigint FROM pos_payments p \
                 JOIN pos_sales s ON s.id=p.sale_id \
                 WHERE s.shift_id=$1 AND p.method='cash' AND p.state='succeeded'",
            )
            .bind(shift_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }
    fn cash_refunds<'a>(&'a mut self, shift_id: &'a str) -> Fut<'a, i64> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT COALESCE(SUM(rp.amount_minor),0)::bigint FROM pos_refund_payments rp \
                 JOIN pos_payments p ON p.id=rp.payment_id JOIN pos_refunds r ON r.id=rp.refund_id \
                 WHERE r.payout_shift_id=$1 AND p.method='cash' AND rp.state='succeeded'",
            )
            .bind(shift_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }
    fn approved_movements<'a>(&'a mut self, shift_id: &'a str) -> Fut<'a, i64> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT COALESCE(SUM(amount_minor),0)::bigint FROM pos_cash_movements \
                 WHERE shift_id=$1 AND approved_by IS NOT NULL",
            )
            .bind(shift_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }
    fn close_shift<'a>(
        &'a mut self,
        shift_id: &'a str,
        expected_version: i64,
        counted_minor: i64,
        expected_minor: i64,
        variance_minor: i64,
        denominations: Option<&'a str>,
        actor: &'a PosActor,
        now: TimestampMillis,
    ) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE pos_shifts SET state='closed',counted_minor=$3,expected_minor=$4,\
                 variance_minor=$5,closed_at=$6,closed_by=$7,denominations_json=($8::text)::jsonb,version=version+1 \
                 WHERE id=$1 AND version=$2 AND state='open'",
            )
            .bind(shift_id)
            .bind(expected_version)
            .bind(counted_minor)
            .bind(expected_minor)
            .bind(variance_minor)
            .bind(now.get())
            .bind(actor.user_id.as_str())
            .bind(denominations)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }
    fn cash_movement<'a>(
        &'a mut self,
        shift_id: &'a str,
        movement_id: &'a str,
    ) -> Fut<'a, Option<PosCashMovement>> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,shift_id,amount_minor,created_by,approved_by \
                 FROM pos_cash_movements WHERE id=$1 AND shift_id=$2 FOR UPDATE",
            )
            .bind(movement_id)
            .bind(shift_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(PosCashMovement {
                    id: row.try_get("id")?,
                    shift_id: row.try_get("shift_id")?,
                    amount_minor: row.try_get("amount_minor")?,
                    created_by: row.try_get("created_by")?,
                    approved_by: row.try_get("approved_by")?,
                })
            })
            .transpose()
        })
    }
    fn approve_cash<'a>(
        &'a mut self,
        movement_id: &'a str,
        expected_version: i64,
        actor: &'a PosActor,
    ) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE pos_cash_movements SET approved_by=$3 \
                 WHERE id=$1 AND approved_by IS NULL AND shift_id IN \
                 (SELECT id FROM pos_shifts WHERE id=(SELECT shift_id FROM pos_cash_movements WHERE id=$1) AND version=$2 AND state='open')",
            )
            .bind(movement_id)
            .bind(expected_version)
            .bind(actor.user_id.as_str())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }
    fn bump_shift<'a>(&'a mut self, shift_id: &'a str, expected_version: i64) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE pos_shifts SET version=version+1 WHERE id=$1 AND version=$2 AND state='open'",
            )
            .bind(shift_id)
            .bind(expected_version)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }
    fn review_shift<'a>(
        &'a mut self,
        shift_id: &'a str,
        expected_version: i64,
        actor: &'a PosActor,
        reason: &'a str,
    ) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE pos_shifts SET state='reviewed',reviewed_by=$3,review_reason=$4,version=version+1 \
                 WHERE id=$1 AND version=$2 AND state='closed'",
            )
            .bind(shift_id)
            .bind(expected_version)
            .bind(actor.user_id.as_str())
            .bind(reason)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }
    fn insert_posting<'a>(
        &'a mut self,
        source_key: &'a str,
        company: &'a CompanyId,
        journal_id: &'a str,
    ) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO pos_postings(source_key,company_id,journal_id) VALUES($1,$2,$3) \
                 ON CONFLICT(source_key) DO NOTHING",
            )
            .bind(source_key)
            .bind(company.as_str())
            .bind(journal_id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn sale<'a>(
        &'a mut self,
        company: &'a CompanyId,
        sale_id: &'a str,
    ) -> Fut<'a, Option<PosSale>> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,company_id,branch_id,register_id,shift_id,number,party_id,issuer_json,currency,business_date,service_mode,state,\
                 subtotal_minor,tax_minor,discount_minor,total_minor,created_by,version,accounts_json,discount_approved_by,invoice_id \
                 FROM pos_sales WHERE id=$1 AND company_id=$2 FOR UPDATE",
            )
            .bind(sale_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(PosSale {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    register_id: row.try_get("register_id")?,
                    shift_id: row.try_get("shift_id")?,
                    number: row.try_get("number")?,
                    party_id: row.try_get("party_id")?,
                    issuer_json: row.try_get("issuer_json")?,
                    currency: row.try_get("currency")?,
                    business_date: row.try_get("business_date")?,
                    service_mode: row.try_get("service_mode")?,
                    state: row.try_get("state")?,
                    subtotal_minor: row.try_get("subtotal_minor")?,
                    tax_minor: row.try_get("tax_minor")?,
                    discount_minor: row.try_get("discount_minor")?,
                    total_minor: row.try_get("total_minor")?,
                    created_by: row.try_get("created_by")?,
                    version: row.try_get("version")?,
                    accounts_json: row.try_get("accounts_json")?,
                    discount_approved_by: row.try_get("discount_approved_by")?,
                    invoice_id: row.try_get("invoice_id")?,
                })
            })
            .transpose()
        })
    }
    fn sale_lines<'a>(&'a mut self, sale_id: &'a str) -> Fut<'a, Vec<PosLine>> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT id,sale_id,quantity_milli,subtotal_minor,tax_minor,returned_milli,price_minor \
                 FROM pos_lines WHERE sale_id=$1 ORDER BY id",
            )
            .bind(sale_id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(PosLine {
                        id: row.try_get("id")?,
                        sale_id: row.try_get("sale_id")?,
                        quantity_milli: row.try_get("quantity_milli")?,
                        subtotal_minor: row.try_get("subtotal_minor")?,
                        tax_minor: row.try_get("tax_minor")?,
                        returned_milli: row.try_get("returned_milli")?,
                        price_minor: row.try_get("price_minor")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }
    fn payment_started<'a>(&'a mut self, sale_id: &'a str) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM pos_payments WHERE sale_id=$1 AND \
                 (state IN ('requested','pending','exception') OR (state='succeeded' AND amount_minor>refunded_minor)))",
            )
            .bind(sale_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }
    fn set_sale_state<'a>(
        &'a mut self,
        sale_id: &'a str,
        expected_version: i64,
        state: &'a str,
        now: TimestampMillis,
    ) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE pos_sales SET state=$3,\
                 subtotal_minor=(SELECT COALESCE(SUM(subtotal_minor),0) FROM pos_lines WHERE sale_id=$1),\
                 tax_minor=(SELECT COALESCE(SUM(tax_minor),0) FROM pos_lines WHERE sale_id=$1),\
                 discount_minor=(SELECT COALESCE(SUM(discount_minor),0) FROM pos_lines WHERE sale_id=$1),\
                 total_minor=(SELECT COALESCE(SUM(subtotal_minor+tax_minor),0) FROM pos_lines WHERE sale_id=$1),\
                 version=version+1,updated_at=$4 WHERE id=$1 AND version=$2",
            )
            .bind(sale_id)
            .bind(expected_version)
            .bind(state)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }
    fn sale_fulfillments<'a>(&'a mut self, sale_id: &'a str) -> Fut<'a, Vec<PosFulfillment>> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT f.id,f.state,f.kind FROM pos_fulfillments f \
                 WHERE f.id IN (SELECT fulfillment_id FROM pos_lines WHERE sale_id=$1) FOR UPDATE OF f",
            )
            .bind(sale_id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(PosFulfillment {
                        id: row.try_get("id")?,
                        state: row.try_get("state")?,
                        kind: row.try_get("kind")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }
    fn fulfillment_shared<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
        sale_id: &'a str,
    ) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM pos_lines WHERE fulfillment_id=$1 AND sale_id<>$2)",
            )
            .bind(fulfillment_id)
            .bind(sale_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }
    fn cancel_fulfillment<'a>(&'a mut self, fulfillment_id: &'a str) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("DELETE FROM pos_reservations WHERE fulfillment_id=$1")
                .bind(fulfillment_id)
                .execute(&mut *self.connection)
                .await?;
            sqlx::query(
                "UPDATE pos_fulfillments SET state='cancelled',reserved=false,version=version+1 WHERE id=$1",
            )
            .bind(fulfillment_id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
    fn cancel_sale<'a>(
        &'a mut self,
        sale_id: &'a str,
        expected_version: i64,
        now: TimestampMillis,
    ) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE pos_sales SET state='cancelled',table_id=NULL,version=version+1,updated_at=$3 \
                 WHERE id=$1 AND version=$2",
            )
            .bind(sale_id)
            .bind(expected_version)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }
    fn table_exists<'a>(&'a mut self, register_id: &'a str, table_id: &'a str) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM pos_tables WHERE id=$1 AND register_id=$2)",
            )
            .bind(table_id)
            .bind(register_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }
    fn insert_sale<'a>(
        &'a mut self,
        id: &'a str,
        shift: &'a PosShift,
        register: &'a PosRegister,
        settings: &'a PosSettings,
        input: &'a SaleCreateInput,
        number: &'a str,
        issuer_json: &'a str,
        actor: &'a PosActor,
        business_date: &'a str,
        now: TimestampMillis,
    ) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,\
                 party_id,table_id,guests,service_mode,created_by,created_at,updated_at,accounts_json,issuer_json) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$14,$15,$16)",
            )
            .bind(id)
            .bind(&shift.company_id)
            .bind(&shift.branch_id)
            .bind(&register.id)
            .bind(&shift.id)
            .bind(number)
            .bind(&settings.currency)
            .bind(business_date)
            .bind(input.party_id.as_deref())
            .bind(input.table_id.as_deref())
            .bind(input.guests)
            .bind(&input.service_mode)
            .bind(actor.user_id.as_str())
            .bind(now.get())
            .bind(&settings.accounts_json)
            .bind(issuer_json)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
    fn insert_fulfillment<'a>(
        &'a mut self,
        fulfillment: &'a PosFulfillmentInsert,
    ) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO pos_fulfillments(id,company_id,branch_id,warehouse_id,product_id,quantity_milli,kind,station,note,recipe_json,accounts_json,currency,business_date,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,(extract(epoch from clock_timestamp())*1000)::bigint)",
            )
            .bind(&fulfillment.id)
            .bind(&fulfillment.company_id)
            .bind(&fulfillment.branch_id)
            .bind(&fulfillment.warehouse_id)
            .bind(&fulfillment.product_id)
            .bind(fulfillment.quantity_milli)
            .bind(&fulfillment.kind)
            .bind(&fulfillment.station)
            .bind(&fulfillment.note)
            .bind(&fulfillment.recipe_json)
            .bind(&fulfillment.accounts_json)
            .bind(&fulfillment.currency)
            .bind(&fulfillment.business_date)
            .bind(erpx_server_shared::public::TimestampMillis::new(0).get())
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
    fn insert_line<'a>(&'a mut self, line: &'a PosLineInsert) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO pos_lines(id,sale_id,fulfillment_id,product_id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,subtotal_minor,tax_minor,modifiers_json,note) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
            )
            .bind(&line.id)
            .bind(&line.sale_id)
            .bind(&line.fulfillment_id)
            .bind(&line.product_id)
            .bind(&line.name)
            .bind(line.quantity_milli)
            .bind(line.price_minor)
            .bind(line.tax_bps)
            .bind(line.tax_inclusive)
            .bind(line.discount_minor)
            .bind(line.subtotal_minor)
            .bind(line.tax_minor)
            .bind(&line.modifiers_json)
            .bind(&line.note)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
    fn refresh_sale<'a>(&'a mut self, sale_id: &'a str, now: TimestampMillis) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "UPDATE pos_sales SET subtotal_minor=(SELECT COALESCE(SUM(subtotal_minor),0) FROM pos_lines WHERE sale_id=$1),tax_minor=(SELECT COALESCE(SUM(tax_minor),0) FROM pos_lines WHERE sale_id=$2),discount_minor=(SELECT COALESCE(SUM(discount_minor),0) FROM pos_lines WHERE sale_id=$3),total_minor=(SELECT COALESCE(SUM(subtotal_minor+tax_minor),0) FROM pos_lines WHERE sale_id=$4),version=version+1,updated_at=$5,discount_approved_by=NULL,discount_reason=NULL WHERE id=$6",
            )
            .bind(sale_id)
            .bind(sale_id)
            .bind(sale_id)
            .bind(sale_id)
            .bind(now.get())
            .bind(sale_id)
            .execute(&mut *self.connection)
            .await?;
            Ok(rows.rows_affected() == 1)
        })
    }
    fn sale_line<'a>(&'a mut self, sale_id: &'a str, line_id: &'a str) -> Fut<'a, Option<PosLineDetail>> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT l.id,l.fulfillment_id,l.price_minor,l.tax_bps,l.tax_inclusive,l.discount_minor,l.bill_discount_minor,l.note,f.state fulfillment_state FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.id=$1 AND l.sale_id=$2 FOR UPDATE OF l,f",
            )
            .bind(line_id)
            .bind(sale_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(PosLineDetail {
                    id: row.try_get("id")?,
                    fulfillment_id: row.try_get("fulfillment_id")?,
                    price_minor: row.try_get("price_minor")?,
                    tax_bps: row.try_get("tax_bps")?,
                    tax_inclusive: row.try_get("tax_inclusive")?,
                    discount_minor: row.try_get("discount_minor")?,
                    bill_discount_minor: row.try_get("bill_discount_minor")?,
                    note: row.try_get("note")?,
                    fulfillment_state: row.try_get("fulfillment_state")?,
                })
            })
            .transpose()
        })
    }
    fn update_fulfillment<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
        quantity_milli: i64,
        note: &'a str,
    ) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("UPDATE pos_fulfillments SET quantity_milli=$1,note=$2,version=version+1 WHERE id=$3")
                .bind(quantity_milli)
                .bind(note)
                .bind(fulfillment_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn update_line<'a>(
        &'a mut self,
        line_id: &'a str,
        quantity_milli: i64,
        discount_minor: i64,
        subtotal_minor: i64,
        tax_minor: i64,
        note: &'a str,
    ) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("UPDATE pos_lines SET quantity_milli=$1,discount_minor=$2,subtotal_minor=$3,tax_minor=$4,note=$5 WHERE id=$6")
                .bind(quantity_milli)
                .bind(discount_minor)
                .bind(subtotal_minor)
                .bind(tax_minor)
                .bind(note)
                .bind(line_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn set_sale_party<'a>(&'a mut self, sale_id: &'a str, party_id: Option<&'a str>) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("UPDATE pos_sales SET party_id=$1 WHERE id=$2")
                .bind(party_id)
                .bind(sale_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn set_sale_table<'a>(&'a mut self, sale_id: &'a str, table_id: &'a str) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("UPDATE pos_sales SET table_id=$1 WHERE id=$2")
                .bind(table_id)
                .bind(sale_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn delete_reservations<'a>(&'a mut self, fulfillment_id: &'a str) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("DELETE FROM pos_reservations WHERE fulfillment_id=$1")
                .bind(fulfillment_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn delete_line<'a>(&'a mut self, line_id: &'a str) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("DELETE FROM pos_lines WHERE id=$1")
                .bind(line_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn sale_lines_discount<'a>(&'a mut self, sale_id: &'a str) -> Fut<'a, Vec<PosLineDiscountRow>> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT id,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,bill_discount_minor FROM pos_lines WHERE sale_id=$1 ORDER BY id FOR UPDATE",
            )
            .bind(sale_id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(PosLineDiscountRow {
                        id: row.try_get("id")?,
                        quantity_milli: row.try_get("quantity_milli")?,
                        price_minor: row.try_get("price_minor")?,
                        tax_bps: row.try_get("tax_bps")?,
                        tax_inclusive: row.try_get("tax_inclusive")?,
                        discount_minor: row.try_get("discount_minor")?,
                        bill_discount_minor: row.try_get("bill_discount_minor")?,
                    })
                })
                .collect()
        })
    }
    fn update_line_discount<'a>(
        &'a mut self,
        line_id: &'a str,
        discount_minor: i64,
        bill_discount_minor: i64,
        subtotal_minor: i64,
        tax_minor: i64,
    ) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("UPDATE pos_lines SET discount_minor=$1,bill_discount_minor=$2,subtotal_minor=$3,tax_minor=$4 WHERE id=$5")
                .bind(discount_minor)
                .bind(bill_discount_minor)
                .bind(subtotal_minor)
                .bind(tax_minor)
                .bind(line_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn move_lines<'a>(&'a mut self, target_id: &'a str, source_id: &'a str) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("UPDATE pos_lines SET sale_id=$1 WHERE sale_id=$2")
                .bind(target_id)
                .bind(source_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn mark_merged<'a>(&'a mut self, sale_id: &'a str) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("UPDATE pos_sales SET state='merged',table_id=NULL WHERE id=$1")
                .bind(sale_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn sale_line_split<'a>(&'a mut self, sale_id: &'a str, line_id: &'a str) -> Fut<'a, Option<PosLineSplitRow>> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,fulfillment_id,quantity_milli,subtotal_minor,tax_minor,discount_minor,bill_discount_minor FROM pos_lines WHERE id=$1 AND sale_id=$2 FOR UPDATE",
            )
            .bind(line_id)
            .bind(sale_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(PosLineSplitRow {
                    id: row.try_get("id")?,
                    fulfillment_id: row.try_get("fulfillment_id")?,
                    quantity_milli: row.try_get("quantity_milli")?,
                    subtotal_minor: row.try_get("subtotal_minor")?,
                    tax_minor: row.try_get("tax_minor")?,
                    discount_minor: row.try_get("discount_minor")?,
                    bill_discount_minor: row.try_get("bill_discount_minor")?,
                })
            })
            .transpose()
        })
    }
    fn fulfillment_split<'a>(&'a mut self, fulfillment_id: &'a str) -> Fut<'a, Option<PosFulfillmentSplitFacts>> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query("SELECT kind,state,reserved FROM pos_fulfillments WHERE id=$1 FOR UPDATE")
                .bind(fulfillment_id)
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(|row| {
                Ok::<_, Error>(PosFulfillmentSplitFacts {
                    kind: row.try_get("kind")?,
                    state: row.try_get("state")?,
                    reserved: row.try_get("reserved")?,
                })
            })
            .transpose()
        })
    }
    fn create_split_sale<'a>(
        &'a mut self,
        target_id: &'a str,
        source_id: &'a str,
        number: &'a str,
        user: &'a str,
        now: TimestampMillis,
    ) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,party_id,guests,service_mode,created_by,created_at,updated_at,accounts_json,issuer_json,discount_requested_by) SELECT $1,company_id,branch_id,register_id,shift_id,$2,currency,business_date,party_id,guests,service_mode,$3,$4,$4,accounts_json,issuer_json,$3 FROM pos_sales WHERE id=$5",
            )
            .bind(target_id)
            .bind(number)
            .bind(user)
            .bind(now.get())
            .bind(source_id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
    fn move_line_to_sale<'a>(&'a mut self, target_id: &'a str, line_id: &'a str) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("UPDATE pos_lines SET sale_id=$1 WHERE id=$2")
                .bind(target_id)
                .bind(line_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn create_split_fulfillment<'a>(
        &'a mut self,
        new_id: &'a str,
        source_id: &'a str,
        quantity_milli: i64,
        now: TimestampMillis,
    ) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO pos_fulfillments(id,company_id,branch_id,warehouse_id,product_id,quantity_milli,kind,station,note,recipe_json,accounts_json,currency,business_date,created_at) SELECT $1,company_id,branch_id,warehouse_id,product_id,$2,kind,station,note,recipe_json,accounts_json,currency,business_date,$3 FROM pos_fulfillments WHERE id=$4",
            )
            .bind(new_id)
            .bind(quantity_milli)
            .bind(now.get())
            .bind(source_id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
    fn reduce_fulfillment<'a>(&'a mut self, source_id: &'a str, quantity_milli: i64) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("UPDATE pos_fulfillments SET quantity_milli=quantity_milli-$1,version=version+1 WHERE id=$2")
                .bind(quantity_milli)
                .bind(source_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn create_split_line<'a>(
        &'a mut self,
        new_line_id: &'a str,
        target_id: &'a str,
        new_fulfillment_id: &'a str,
        source_line_id: &'a str,
        quantity_milli: i64,
        discount_minor: i64,
        bill_discount_minor: i64,
        subtotal_minor: i64,
        tax_minor: i64,
    ) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO pos_lines(id,sale_id,fulfillment_id,product_id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,bill_discount_minor,subtotal_minor,tax_minor,modifiers_json,note) SELECT $1,$2,$3,product_id,name,$4,price_minor,tax_bps,tax_inclusive,$5,$6,$7,$8,modifiers_json,note FROM pos_lines WHERE id=$9",
            )
            .bind(new_line_id)
            .bind(target_id)
            .bind(new_fulfillment_id)
            .bind(quantity_milli)
            .bind(discount_minor)
            .bind(bill_discount_minor)
            .bind(subtotal_minor)
            .bind(tax_minor)
            .bind(source_line_id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
    fn reduce_line<'a>(
        &'a mut self,
        line_id: &'a str,
        quantity_milli: i64,
        subtotal_minor: i64,
        tax_minor: i64,
        discount_minor: i64,
        bill_discount_minor: i64,
    ) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("UPDATE pos_lines SET quantity_milli=quantity_milli-$1,subtotal_minor=subtotal_minor-$2,tax_minor=tax_minor-$3,discount_minor=discount_minor-$4,bill_discount_minor=bill_discount_minor-$5 WHERE id=$6")
                .bind(quantity_milli)
                .bind(subtotal_minor)
                .bind(tax_minor)
                .bind(discount_minor)
                .bind(bill_discount_minor)
                .bind(line_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn payment_reserved<'a>(&'a mut self, sale_id: &'a str) -> Fut<'a, i64> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT COALESCE(SUM(amount_minor-refunded_minor),0)::bigint FROM pos_payments WHERE sale_id=$1 AND state IN ('requested','pending','succeeded','exception')",
            )
            .bind(sale_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }
    fn sale_gross<'a>(&'a mut self, sale_id: &'a str) -> Fut<'a, i64> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT COALESCE(SUM(subtotal_minor+tax_minor+discount_minor),0)::bigint FROM pos_lines WHERE sale_id=$1",
            )
            .bind(sale_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }
    fn sale_fulfillment_stock<'a>(&'a mut self, sale_id: &'a str) -> Fut<'a, Vec<PosFulfillmentStock>> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT f.id,f.warehouse_id,f.product_id,f.quantity_milli,f.kind,f.recipe_json,f.state,f.reserved FROM pos_fulfillments f WHERE f.id IN (SELECT fulfillment_id FROM pos_lines WHERE sale_id=$1) FOR UPDATE OF f",
            )
            .bind(sale_id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(PosFulfillmentStock {
                        id: row.try_get("id")?,
                        warehouse_id: row.try_get("warehouse_id")?,
                        product_id: row.try_get("product_id")?,
                        quantity_milli: row.try_get("quantity_milli")?,
                        kind: row.try_get("kind")?,
                        recipe_json: row.try_get("recipe_json")?,
                        state: row.try_get("state")?,
                        reserved: row.try_get("reserved")?,
                    })
                })
                .collect()
        })
    }
    fn insert_payment<'a>(&'a mut self, payment: &'a PosPaymentInsert) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            let state = if payment.method == "transfer" { "pending" } else { "requested" };
            sqlx::query(
                "INSERT INTO pos_payments(id,sale_id,company_id,method,amount_minor,tendered_minor,reference,state,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10)",
            )
            .bind(&payment.id)
            .bind(&payment.sale_id)
            .bind(&payment.company_id)
            .bind(&payment.method)
            .bind(payment.amount_minor)
            .bind(payment.tendered_minor)
            .bind(&payment.reference)
            .bind(state)
            .bind(&payment.created_by)
            .bind(payment.now.get())
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
    fn settle_payment<'a>(&'a mut self, payment_id: &'a str, now: TimestampMillis) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("UPDATE pos_payments SET state='succeeded',version=version+1,updated_at=$1 WHERE id=$2")
                .bind(now.get())
                .bind(payment_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn attach_finance_payment<'a>(&'a mut self, payment_id: &'a str, finance_payment_id: &'a str) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("UPDATE pos_payments SET finance_payment_id=$1 WHERE id=$2")
                .bind(finance_payment_id)
                .bind(payment_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn reserve_stock<'a>(&'a mut self, fulfillment_id: &'a str, warehouse_id: &'a str, product_id: &'a str, quantity_milli: i64) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("INSERT INTO pos_reservations(fulfillment_id,warehouse_id,product_id,quantity_milli) VALUES($1,$2,$3,$4)")
                .bind(fulfillment_id)
                .bind(warehouse_id)
                .bind(product_id)
                .bind(quantity_milli)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn mark_fulfillment_reserved<'a>(&'a mut self, fulfillment_id: &'a str) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("UPDATE pos_fulfillments SET reserved=true WHERE id=$1")
                .bind(fulfillment_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn bump_payment_pending<'a>(&'a mut self, sale_id: &'a str, now: TimestampMillis) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query("UPDATE pos_sales SET state='payment_pending',version=version+1,updated_at=$1 WHERE id=$2")
                .bind(now.get())
                .bind(sale_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(rows.rows_affected() == 1)
        })
    }
    fn sale_paid_total<'a>(&'a mut self, sale_id: &'a str) -> Fut<'a, i64> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT COALESCE(SUM(amount_minor-refunded_minor),0)::bigint FROM pos_payments WHERE sale_id=$1 AND state='succeeded'",
            )
            .bind(sale_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }
    fn sale_pending_payments<'a>(&'a mut self, sale_id: &'a str) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pos_payments WHERE sale_id=$1 AND state IN ('requested','pending'))",
            )
            .bind(sale_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }
    fn delete_reservation<'a>(&'a mut self, fulfillment_id: &'a str, product_id: &'a str) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("DELETE FROM pos_reservations WHERE fulfillment_id=$1 AND product_id=$2")
                .bind(fulfillment_id)
                .bind(product_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn insert_stock_move<'a>(
        &'a mut self,
        id: &'a str,
        company_id: &'a str,
        fulfillment_id: &'a str,
        warehouse_id: &'a str,
        product_id: &'a str,
        quantity_milli: i64,
        value_minor: i64,
        movement_date: &'a str,
        source_number: &'a str,
        now: TimestampMillis,
    ) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO pos_stock_moves(id,company_id,warehouse_id,product_id,fulfillment_id,source_id,source_number,kind,quantity_milli,value_minor,movement_date,created_at) VALUES($1,$2,$3,$4,$5,$5,$6,'sale',$7,$8,$9,$10)",
            )
            .bind(id)
            .bind(company_id)
            .bind(warehouse_id)
            .bind(product_id)
            .bind(fulfillment_id)
            .bind(source_number)
            .bind(-quantity_milli)
            .bind(-value_minor)
            .bind(movement_date)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
    fn mark_fulfillment_delivered<'a>(&'a mut self, fulfillment_id: &'a str, cost_minor: i64) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("UPDATE pos_fulfillments SET state='delivered',cost_minor=$1,version=version+1 WHERE id=$2")
                .bind(cost_minor)
                .bind(fulfillment_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn complete_sale<'a>(&'a mut self, sale_id: &'a str, now: TimestampMillis) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query("UPDATE pos_sales SET state='completed',version=version+1,updated_at=$1 WHERE id=$2")
                .bind(now.get())
                .bind(sale_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(rows.rows_affected() == 1)
        })
    }
    fn attach_invoice<'a>(&'a mut self, sale_id: &'a str, invoice_id: &'a str) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("UPDATE pos_sales SET invoice_id=$1 WHERE id=$2")
                .bind(invoice_id)
                .bind(sale_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn payment<'a>(
        &'a mut self,
        company: &'a CompanyId,
        payment_id: &'a str,
    ) -> Fut<'a, Option<PosPayment>> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,sale_id,method,state,amount_minor,reference,created_by,version,finance_payment_id FROM pos_payments WHERE id=$1 AND company_id=$2 FOR UPDATE",
            )
            .bind(payment_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(PosPayment {
                    id: row.try_get("id")?,
                    sale_id: row.try_get("sale_id")?,
                    method: row.try_get("method")?,
                    state: row.try_get("state")?,
                    amount_minor: row.try_get("amount_minor")?,
                    reference: row.try_get("reference")?,
                    created_by: row.try_get("created_by")?,
                    version: row.try_get("version")?,
                    finance_payment_id: row.try_get("finance_payment_id")?,
                })
            })
            .transpose()
        })
    }
    fn cancel_payment<'a>(&'a mut self, payment_id: &'a str, now: TimestampMillis) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("UPDATE pos_payments SET state='cancelled',version=version+1,updated_at=$1 WHERE id=$2")
                .bind(now.get())
                .bind(payment_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn reopen_sale<'a>(&'a mut self, sale_id: &'a str, state: &'a str, now: TimestampMillis) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query("UPDATE pos_sales SET state=$1,version=version+1,updated_at=$2 WHERE id=$3")
                .bind(state)
                .bind(now.get())
                .bind(sale_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(rows.rows_affected() == 1)
        })
    }
    fn prepared_day_close<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: &'a str,
        business_date: &'a str,
    ) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM pos_day_closes \
                 WHERE company_id=$1 AND branch_id=$2 AND business_date=$3 AND state='prepared')",
            )
            .bind(company.as_str())
            .bind(branch_id)
            .bind(business_date)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }
    fn insert_refund<'a>(
        &'a mut self,
        id: &'a str,
        sale: &'a PosSale,
        input: &'a RefundRequestInput,
        kind: &'a str,
        lines_json: &'a str,
        amount_minor: i64,
        tax_minor: i64,
        accounting_date: &'a str,
        actor: &'a PosActor,
        now: TimestampMillis,
    ) -> Fut<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO pos_refunds(id,sale_id,company_id,branch_id,payout_shift_id,kind,reason,lines_json,\
                 amount_minor,requested_by,created_at,accounting_date,tax_minor) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
            )
            .bind(id)
            .bind(&sale.id)
            .bind(&sale.company_id)
            .bind(&sale.branch_id)
            .bind(input.payout_shift_id.as_deref())
            .bind(kind)
            .bind(&input.reason)
            .bind(lines_json)
            .bind(amount_minor)
            .bind(actor.user_id.as_str())
            .bind(now.get())
            .bind(accounting_date)
            .bind(tax_minor)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
    fn refund<'a>(
        &'a mut self,
        company: &'a CompanyId,
        refund_id: &'a str,
    ) -> Fut<'a, Option<PosRefund>> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,sale_id,company_id,branch_id,state,requested_by,version \
                 FROM pos_refunds WHERE id=$1 AND company_id=$2 FOR UPDATE",
            )
            .bind(refund_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(PosRefund {
                    id: row.try_get("id")?,
                    sale_id: row.try_get("sale_id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    state: row.try_get("state")?,
                    requested_by: row.try_get("requested_by")?,
                    version: row.try_get("version")?,
                })
            })
            .transpose()
        })
    }
    fn reject_refund<'a>(
        &'a mut self,
        refund_id: &'a str,
        expected_version: i64,
        actor: &'a PosActor,
    ) -> Fut<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE pos_refunds SET state='rejected',approved_by=$3,version=version+1 \
                 WHERE id=$1 AND version=$2 AND state='requested'",
            )
            .bind(refund_id)
            .bind(expected_version)
            .bind(actor.user_id.as_str())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }
}

// Keep the imported command type visible for public adapter documentation.
const _: Option<PosCommand> = None;
const _: Option<CashApproveInput> = None;
