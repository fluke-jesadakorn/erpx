//! PostgreSQL adapter for the native Commerce owner-write registry.
//! The caller owns one serializable Holding transaction; only Commerce-owned
//! rows plus the shared command ledger are written here.
use crate::application::owner_write::{
    LedgerEntry, OwnerWriteFuture, OwnerWriteRepository, StoredReceipt,
};
use crate::domain::owner_write::{OwnerWrite, invalid};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command was already used.",
    )
}

fn kind_sql(kind: &str) -> &'static str {
    match kind {
        "settings_initialize" => {
            "INSERT INTO commerce_settings\
            (company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,\
            inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,\
            output_tax_account_id,cash_account_id) \
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) \
            ON CONFLICT(company_id) DO NOTHING"
        }
        "billing_test_document" => {
            "INSERT INTO billing_test_finance_documents\
            (stripe_invoice_id,source_billing_invoice_id,customer_holding_id,company_id,currency,\
            total_satang,stripe_status,observed_at,created_at) VALUES($1,$2,$3,$4,'THB',$5,$6,$7,$8) \
            ON CONFLICT(stripe_invoice_id) DO NOTHING"
        }
        "billing_test_entry" => {
            "INSERT INTO billing_test_finance_entries\
            (source_key,stripe_invoice_id,entry_kind,amount_satang,effective_at,created_at) \
            VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(source_key) DO NOTHING"
        }
        "billing_test_line" => {
            "INSERT INTO billing_test_finance_lines\
            (source_key,ordinal,account_code,debit_satang,credit_satang) \
            VALUES($1,$2,$3,$4,0),($5,$6,$7,0,$8)"
        }
        "fulfillment_version" => "UPDATE commerce_fulfillments SET version=version+1 WHERE id=$1",
        "order_version" => "UPDATE commerce_orders SET version=version+1,updated_at=$1 WHERE id=$2",
        "goods_carrying" => {
            "INSERT INTO commerce_invoice_goods_carrying\
            (invoice_id,company_id,chart_id,stock_currency,clearing_account_id,invoice_fx_rate_micros,\
            invoice_rate_basis,invoice_source_rate_id,invoice_target_rate_id,invoice_used_latest_fallback,\
            source_digest,source_json,receipt_base_minor,invoice_clearing_base_minor,adjustment_minor,\
            gain_account_id,loss_account_id,fx_journal_id,reviewed_by,reviewed_at) \
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20) \
            ON CONFLICT(invoice_id) DO NOTHING"
        }
        "product_create" => {
            "INSERT INTO commerce_products\
            (id,company_id,sku,name,unit,tracking_mode,item_type,price_minor,tax_bps,status) \
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(id) DO NOTHING"
        }
        "product_update" => {
            "UPDATE commerce_products SET sku=$1,name=$2,unit=$3,tracking_mode=$4,\
            item_type=$5,price_minor=$6,tax_bps=$7,status=$8,version=version+1 WHERE id=$9"
        }
        "order_version_bump" => {
            "UPDATE commerce_orders SET version=version+1,updated_at=$1 WHERE id=$2"
        }
        "order_posted_revision" => {
            "UPDATE commerce_orders o SET version=o.version+1,updated_at=$1 \
             FROM commerce_fulfillments f JOIN finance_journals j ON j.id=f.journal_id \
             WHERE f.order_id=o.id AND j.id=$2 AND j.company_id=$3 AND j.status='posted'"
        }
        "return_create" => {
            "INSERT INTO commerce_returns(id,company_id,branch_id,order_id,fulfillment_id,invoice_id,kind,number,return_date,reason,subtotal_minor,tax_minor,total_minor,requested_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)"
        }
        "return_line" => {
            "INSERT INTO commerce_return_lines(id,return_id,fulfillment_line_id,product_id,quantity_milli,value_minor,subtotal_minor,tax_minor,ordinal,condition_code,condition_note,quality_due_date,expected_subtotal_minor) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"
        }
        "return_lot_allocation" => {
            "INSERT INTO commerce_return_lot_allocations(id,return_line_id,fulfillment_allocation_id,lot_id,quantity_milli,value_minor,snapshot_quantity_milli,snapshot_value_minor,snapshot_version) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"
        }
        "fulfillment_version_guarded" => {
            "UPDATE commerce_fulfillments SET version=version+1 WHERE id=$1 AND version=$2"
        }
        "return_reject" => {
            "UPDATE commerce_returns SET status='rejected',approved_by=$1,reason=reason||E'\\nRejected: '||$2,version=version+1,updated_at=$3 WHERE id=$4"
        }
        "return_stock_move" => {
            "INSERT INTO commerce_return_stock_moves(id,return_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"
        }
        "return_approve" => {
            "UPDATE commerce_returns SET status='approved',approved_by=$1,stock_journal_id=$2,credit_journal_id=$3,base_credit_minor=$4,goods_fx_journal_id=$5,version=version+1,updated_at=$6 WHERE id=$7"
        }
        "return_post" => {
            "UPDATE commerce_returns SET status='posted',credited_minor=$1,refund_minor=$2,base_credit_minor=$3,base_credited_minor=$4,base_refund_minor=$5,fx_journal_id=$6,version=version+1,updated_at=$7 WHERE id=$8"
        }
        "order_update_parties" => {
            "UPDATE commerce_orders SET party_id=$1,warehouse_id=$2,branch_id=$3,order_date=$4,due_date=$5,note=$6,subtotal_minor=$7,tax_minor=$8,price_list_id=$9,price_list_version=$10,discount_reason=$11,discount_approval_required=$12,discount_approved_by=NULL,discount_approved_at=NULL,version=version+1,updated_at=$13 WHERE id=$14"
        }
        "order_lines_delete" => "DELETE FROM commerce_order_lines WHERE order_id=$1",
        "order_create" => {
            "INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,subtotal_minor,tax_minor,price_list_id,price_list_version,discount_reason,discount_approval_required,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,CASE WHEN $6='quote' THEN erpx_sales_document_number($7,'QT',$8) ELSE $9 END,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24)"
        }
        "order_lines_create" => {
            "INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,list_price_minor,discount_bps,item_type) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"
        }
        "order_transition" => {
            "UPDATE commerce_orders SET state=$1,approved_by=$2,discount_approved_by=CASE WHEN $3 AND discount_approval_required=1 THEN $4 WHEN $5 THEN NULL ELSE discount_approved_by END,discount_approved_at=CASE WHEN $6 AND discount_approval_required=1 THEN $7 WHEN $8 THEN NULL ELSE discount_approved_at END,version=version+1,updated_at=$9 WHERE id=$10"
        }
        "order_convert" => {
            "INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,subtotal_minor,tax_minor,source_id,price_list_id,price_list_version,discount_reason,discount_approval_required,discount_approved_by,discount_approved_at,created_by,created_at,updated_at) SELECT $1,company_id,branch_id,department_id,'sales',$2,party_id,warehouse_id,currency,order_date,due_date,note,subtotal_minor,tax_minor,id,price_list_id,price_list_version,discount_reason,discount_approval_required,discount_approved_by,discount_approved_at,$3,$4,$5 FROM commerce_orders WHERE id=$6"
        }
        "order_lines_convert" => {
            "INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,list_price_minor,discount_bps,item_type) SELECT $1||'-'||ordinal,$2,product_id,description,quantity_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,list_price_minor,discount_bps,item_type FROM commerce_order_lines WHERE order_id=$3"
        }
        "fulfillment_invoice_link" => {
            "UPDATE commerce_fulfillments SET invoice_id=$1,version=version+1 WHERE id=$2"
        }
        "mfg_existing_posting_confirmation" => {
            "INSERT INTO commerce_mfg_existing_posting_confirmations(company_id,command_id,request_hash,chart_id,inventory_account_id,version,mapping_json,proof_json,created_by,created_at) SELECT $1,r.command_id,r.request_hash,$2,$3,1,$4::jsonb,$5::jsonb,$6,$7 FROM command_receipts r WHERE r.command_id=$8 AND r.operation=$9 AND r.record_id=$10"
        }
        "supplier_fulfillment_links" => "SUPPLIER_FULFILLMENT_LINKS",
        _ => "",
    }
}

fn bind_all<'q>(
    mut query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    values: &'q [Value],
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    for value in values {
        query = match value {
            Value::String(text) => query.bind(text.as_str()),
            Value::Number(number) => query.bind(number.as_i64().unwrap_or(0)),
            Value::Bool(flag) => query.bind(*flag),
            Value::Null => query.bind(Option::<i64>::None),
            _ => query.bind(value.to_string()),
        };
    }
    query
}

pub struct PgOwnerWrite<'c> {
    connection: &'c mut PgConnection,
    company: String,
}
impl<'c> PgOwnerWrite<'c> {
    pub fn new(connection: &'c mut PgConnection, scope: &str) -> Self {
        Self {
            connection,
            company: scope.to_owned(),
        }
    }
    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("COMMERCE_OWNER_WRITE_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("COMMERCE_OWNER_WRITE_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }
    async fn insert_ledger(&mut self, entry: &LedgerEntry<'_>) -> Result<(), Error> {
        let authority = serde_json::json!({"companyId": &self.company});
        let changed = sqlx::query(
            "INSERT INTO command_receipts\
             (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(format!("owner-write:{}", entry.key)).bind(entry.operation).bind(entry.actor_id)
        .bind(entry.request_hash).bind(entry.record_id).bind(entry.result).bind(entry.now).bind(&authority)
        .execute(&mut *self.connection).await?.rows_affected();
        if changed != 1 {
            return Err(conflict().into());
        }
        sqlx::query(
            "INSERT INTO audit_events\
             (id,command_id,actor_id,assignment_id,action,record_type,record_id,\
             record_version,purpose,created_at,authority_json) \
             VALUES(gen_random_uuid()::text,$1,$2,$2,$3,$4,$5,1,$3,$6,$7)",
        )
        .bind(entry.key)
        .bind(entry.actor_id)
        .bind(entry.operation)
        .bind(entry.record_type)
        .bind(entry.record_id)
        .bind(entry.now)
        .bind(&authority)
        .execute(&mut *self.connection)
        .await?;
        let payload = serde_json::json!({"companyId": &self.company, "recordId": entry.record_id, "result": entry.result});
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,1,$5,$6)",
        )
        .bind(entry.key).bind(entry.operation).bind(entry.record_type).bind(entry.record_id)
        .bind(&payload).bind(entry.now).execute(&mut *self.connection).await?;
        Ok(())
    }
    async fn apply_owner(&mut self, write: &OwnerWrite) -> Result<(), Error> {
        let sql = kind_sql(write.kind);
        if sql.is_empty() {
            return Err(invalid().into());
        }
        if write.kind == "supplier_fulfillment_links" {
            let invoice = &write.values[0];
            let ids: Vec<String> =
                serde_json::from_value(write.values[1].clone()).map_err(|_| invalid())?;
            for id in ids {
                match invoice {
                    Value::String(value) => {
                        sqlx::query("UPDATE commerce_fulfillments SET invoice_id=$1,version=version+1 WHERE id=$2")
                            .bind(value.as_str())
                            .bind(id.as_str())
                            .execute(&mut *self.connection).await?;
                    }
                    _ => {
                        sqlx::query("UPDATE commerce_fulfillments SET invoice_id=NULL,version=version+1 WHERE id=$1")
                            .bind(id.as_str())
                            .execute(&mut *self.connection).await?;
                    }
                }
            }
            return Ok(());
        }
        bind_all(sqlx::query(sql), &write.values)
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }
}

impl OwnerWriteRepository for PgOwnerWrite<'_> {
    type Error = Error;
    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }
    fn lock_command<'a>(&'a mut self, key: &'a str) -> OwnerWriteFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("commerce-owner-write:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }
    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> OwnerWriteFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(identity)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> OwnerWriteFuture<'a, Option<StoredReceipt>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1",
            )
            .bind(format!("owner-write:{key}"))
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else { return Ok(None) };
            Ok(Some(StoredReceipt {
                request_hash: row.try_get("request_hash")?,
                result: Some(row.try_get("result_json")?),
            }))
        })
    }
    fn ledger<'a>(
        &'a mut self,
        entry: &'a LedgerEntry<'a>,
    ) -> OwnerWriteFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.insert_ledger(entry).await
        })
    }
    fn apply<'a>(&'a mut self, write: &'a OwnerWrite) -> OwnerWriteFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.apply_owner(write).await
        })
    }
}
