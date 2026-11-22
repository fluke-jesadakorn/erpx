//! PostgreSQL adapter for the native Procurement supplier-bill command.
//!
//! Repository SQL stays on the caller's retained SERIALIZABLE Holding
//! transaction. All Procurement-owned reads and writes mirror the retained
//! TypeScript statements byte-for-byte in bind order, including the
//! registered native statement shapes (`procurement_stmt_1` through
//! `procurement_stmt_10`). The cross-owner staging hooks (Commerce
//! receipt-version and invoice links, Finance draft-void, Collaboration
//! notifications) are composition responsibilities and are NOT implemented
//! here.
use crate::application::supplier_bill_command::{
    BillHead, CaptureAttributes, CaptureLineFact, CaptureOrderFact, CapturePostingFact,
    CapturePreparedLine, CaptureReceiptFact, PolicyHead, PostingConfig, StoredSupplierBillReceipt,
    SupplierBillCapturePlan, SupplierBillCommandFuture, SupplierBillCommandRepository,
    SupplierBillCommitPlan, SupplierBillDecidePlan, SupplierBillPolicyPlan,
};
use crate::domain::supplier_bill_command::{
    SupplierBillCommandActor, SupplierBillReadback, SupplierBillRecordReceipt,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub struct PgSupplierBillCommand<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgSupplierBillCommand<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        require_serializable(self.connection).await
    }
}

async fn require_serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *connection)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("SUPPLIER_BILL_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

fn is_conflict(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|value| value.code())
        .as_deref()
        == Some("23505")
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SUPPLIER_BILL_SOURCE_CHANGED",
        "The actual Purchase invoice source changed.",
    )
    .into()
}

impl SupplierBillCommandRepository for PgSupplierBillCommand<'_> {
    type Error = Error;

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<StoredSupplierBillReceipt>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT operation,actor_id,record_id,request_hash,authority_json,result_json \
                 FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(StoredSupplierBillReceipt {
                    operation: row.try_get("operation")?,
                    actor_id: row.try_get("actor_id")?,
                    record_id: row.try_get("record_id")?,
                    request_fingerprint: row.try_get("request_hash")?,
                    authority_json: row.try_get("authority_json")?,
                    result_json: row.try_get("result_json")?,
                })
            })
            .transpose()
        })
    }

    fn capture_receipt<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
        fulfillment_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<CaptureReceiptFact>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT f.id,f.order_id,f.fulfillment_date,f.version,f.invoice_id,\
                  o.kind,o.party_id,o.currency,o.branch_id,o.department_id,\
                  j.id AS journal_id,j.status AS journal_status,j.currency AS journal_currency,j.chart_id AS journal_chart_id \
                 FROM commerce_fulfillments f \
                 JOIN commerce_orders o ON o.id=f.order_id AND o.company_id=$2 \
                 LEFT JOIN finance_journals j ON j.id=f.journal_id \
                 WHERE f.id=$1 \
                   AND ($3::text IS NULL OR o.branch_id=$3) \
                   AND ($4::text IS NULL OR o.department_id=$4)",
            )
            .bind(fulfillment_id)
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_deref())
            .bind(actor.department_id.as_deref())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(CaptureReceiptFact {
                    id: row.try_get("id")?,
                    order_id: row.try_get("order_id")?,
                    fulfillment_date: row.try_get("fulfillment_date")?,
                    version: row.try_get("version")?,
                    invoice_id: row.try_get("invoice_id")?,
                    order_kind: row.try_get("kind")?,
                    order_party_id: row.try_get("party_id")?,
                    order_currency: row.try_get("currency")?,
                    order_branch_id: row.try_get("branch_id")?,
                    order_department_id: row.try_get("department_id")?,
                    journal_id: row.try_get("journal_id")?,
                    journal_status: row.try_get("journal_status")?,
                    journal_currency: row.try_get("journal_currency")?,
                    journal_chart_id: row.try_get("journal_chart_id")?,
                })
            })
            .transpose()
        })
    }

    fn capture_order<'a>(
        &'a mut self,
        order_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<CaptureOrderFact>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,kind,party_id,currency,branch_id,department_id \
                 FROM commerce_orders WHERE id=$1",
            )
            .bind(order_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(CaptureOrderFact {
                    id: row.try_get("id")?,
                    kind: row.try_get("kind")?,
                    party_id: row.try_get("party_id")?,
                    currency: row.try_get("currency")?,
                    branch_id: row.try_get("branch_id")?,
                    department_id: row.try_get("department_id")?,
                })
            })
            .transpose()
        })
    }

    fn posting_config<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
    ) -> SupplierBillCommandFuture<'a, Option<PostingConfig>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row: Option<(String, String, String)> = sqlx::query_as(
                "SELECT chart_id,payable_account_id,input_tax_account_id \
                 FROM commerce_settings WHERE company_id=$1",
            )
            .bind(actor.company_id.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(|(chart_id, payable_account_id, input_tax_account_id)| PostingConfig {
                chart_id,
                payable_account_id,
                input_tax_account_id,
            }))
        })
    }

    fn receipt_postings<'a>(
        &'a mut self,
        journal_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Vec<CapturePostingFact>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT account_id,credit_minor FROM finance_journal_lines \
                 WHERE journal_id=$1 ORDER BY ordinal",
            )
            .bind(journal_id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter()
                .map(|row| {
                    Ok::<_, Error>(CapturePostingFact {
                        account_id: row.try_get("account_id")?,
                        credit_minor: row.try_get("credit_minor")?,
                    })
                })
                .collect()
        })
    }

    fn receipt_lines<'a>(
        &'a mut self,
        fulfillment_ids: &'a [String],
    ) -> SupplierBillCommandFuture<'a, Vec<CaptureLineFact>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            if fulfillment_ids.is_empty() {
                return Ok(Vec::new());
            }
            let mut sql = String::from(
                "SELECT fl.id,fl.fulfillment_id,fl.quantity_milli,fl.subtotal_minor \
                 FROM commerce_fulfillment_lines fl \
                 WHERE fl.fulfillment_id IN (",
            );
            for (index, _) in fulfillment_ids.iter().enumerate() {
                if index > 0 {
                    sql.push_str(", ");
                }
                sql.push_str(&format!("${}", index + 1));
            }
            sql.push_str(") ORDER BY fl.fulfillment_id,fl.id");
            let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
            for id in fulfillment_ids {
                query = query.bind(id);
            }
            let rows = query.fetch_all(&mut *self.connection).await?;
            rows.iter()
                .map(|row| {
                    Ok::<_, Error>(CaptureLineFact {
                        id: row.try_get("id")?,
                        fulfillment_id: row.try_get("fulfillment_id")?,
                        quantity_milli: row.try_get("quantity_milli")?,
                        subtotal_minor: row.try_get("subtotal_minor")?,
                    })
                })
                .collect()
        })
    }

    fn bill<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
        match_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<BillHead>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT m.id,m.created_by,i.created_by AS invoice_created_by,m.status,i.status AS invoice_status,\
                  m.quantity_matches,m.version,m.chart_id,m.variance_account_id,\
                  m.invoice_id,i.version AS invoice_version,m.branch_id,m.department_id \
                 FROM procurement_bill_matches m \
                 LEFT JOIN finance_invoices i ON i.id=m.invoice_id \
                 WHERE m.id=$1 AND m.company_id=$2 \
                   AND ($3::text IS NULL OR m.branch_id=$3) \
                   AND ($4::text IS NULL OR m.department_id=$4)",
            )
            .bind(match_id)
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_deref())
            .bind(actor.department_id.as_deref())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(BillHead {
                    id: row.try_get("id")?,
                    created_by: row.try_get("created_by")?,
                    invoice_created_by: row.try_get("invoice_created_by")?,
                    status: row.try_get("status")?,
                    invoice_status: row.try_get("invoice_status")?,
                    quantity_matches: row.try_get("quantity_matches")?,
                    version: row.try_get("version")?,
                    chart_id: row.try_get("chart_id")?,
                    variance_account_id: row.try_get("variance_account_id")?,
                    invoice_id: row.try_get("invoice_id")?,
                    invoice_version: row.try_get("invoice_version")?,
                    branch_id: row.try_get("branch_id")?,
                    department_id: row.try_get("department_id")?,
                })
            })
            .transpose()
        })
    }

    fn bill_variance<'a>(
        &'a mut self,
        match_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let hit: Option<String> = sqlx::query_scalar(
                "SELECT id FROM procurement_bill_match_lines \
                 WHERE match_id=$1 AND subtotal_minor<>expected_subtotal_minor LIMIT 1",
            )
            .bind(match_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(hit.is_some())
        })
    }

    fn variance_account_ok<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
        account_id: &'a str,
        chart_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let hit: Option<String> = sqlx::query_scalar(
                "SELECT a.id FROM finance_accounts a \
                 JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=a.company_id \
                 WHERE a.id=$1 AND a.company_id=$2 AND a.chart_id=$3 \
                   AND a.currency=c.base_currency \
                   AND a.category='expense' AND a.status='active'",
            )
            .bind(account_id)
            .bind(actor.company_id.as_str())
            .bind(chart_id)
            .fetch_optional(&mut *self.connection)
            .await
            .map_err(|_| storage())?;
            Ok(hit.is_some())
        })
    }

    fn policy<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
    ) -> SupplierBillCommandFuture<'a, Option<PolicyHead>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row: Option<(i64, String)> = sqlx::query_as(
                "SELECT version,matching_mode FROM procurement_invoice_policies WHERE company_id=$1",
            )
            .bind(actor.company_id.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(|(version, matching_mode)| PolicyHead {
                version,
                matching_mode,
            }))
        })
    }

    fn company_authority<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
        now: TimestampMillis,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            // Literal mirror of the retained `companyAuthority(db, actor,
            // 'finance.approve')` read: holding-scope assignment with the
            // Finance approve permission, active at `now`.
            let hit: Option<i32> = sqlx::query_scalar(
                "SELECT 1 FROM assignments WHERE id=$1 AND company_id=$2 \
                 AND iam_role_allows(role,company_id,$3) \
                 AND scope_kind='holding' AND revoked_at IS NULL \
                 AND iam_assignment_active(id,$4) \
                 AND iam_principal_active(user_id,company_id,$4) \
                 AND valid_from<=$4 AND valid_until>$4 LIMIT 1",
            )
            .bind(&actor.assignment_id)
            .bind(actor.company_id.as_str())
            .bind("finance.approve")
            .bind(now.get())
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(hit.is_some())
        })
    }

    fn bill_image<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
        match_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<serde_json::Value>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let image: Option<String> = sqlx::query_scalar(
                "SELECT jsonb_build_object('bill',to_jsonb(m),'receipts',(SELECT jsonb_agg(jsonb_build_object('binding',to_jsonb(br),'receipt',to_jsonb(f),'order',to_jsonb(o)) ORDER BY br.id) FROM procurement_bill_match_receipts br JOIN commerce_fulfillments f ON f.id=br.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id WHERE br.match_id=m.id))::text image \
                 FROM procurement_bill_matches m \
                 JOIN commerce_fulfillments primary_receipt ON primary_receipt.id=m.fulfillment_id AND primary_receipt.order_id=m.order_id \
                 JOIN commerce_orders primary_order ON primary_order.id=m.order_id AND primary_order.company_id=m.company_id AND primary_order.kind='purchase' \
                 WHERE m.id=$1 AND m.company_id=$2 \
                   AND ($3::text IS NULL OR m.branch_id=$3) \
                   AND ($4::text IS NULL OR m.department_id=$4) \
                   AND EXISTS(SELECT 1 FROM procurement_bill_match_receipts br WHERE br.match_id=m.id AND br.fulfillment_id=m.fulfillment_id) \
                   AND NOT EXISTS(SELECT 1 FROM procurement_bill_match_receipts br LEFT JOIN commerce_fulfillments f ON f.id=br.fulfillment_id LEFT JOIN commerce_orders o ON o.id=f.order_id WHERE br.match_id=m.id AND (o.id IS NULL OR o.kind<>'purchase' OR o.company_id<>m.company_id OR o.party_id<>m.supplier_id OR o.currency<>m.currency))",
            )
            .bind(match_id)
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_deref())
            .bind(actor.department_id.as_deref())
            .fetch_optional(&mut *self.connection)
            .await?;
            image
                .map(|text| serde_json::from_str(&text).map_err(|_| invalid_image()))
                .transpose()
                .map_err(Into::into)
        })
    }

    fn verify_capture_receipts<'a>(
        &'a mut self,
        receipts: &'a [(String, String, i64)],
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            // Literal mirror of the retained per-receipt capture assertion:
            // the fulfillment is still at the requested version, uninvoiced,
            // and its journal is posted.
            for (fulfillment_id, order_id, expected_version) in receipts {
                let hit: Option<i32> = sqlx::query_scalar(
                    "SELECT 1 FROM commerce_fulfillments f \
                     JOIN finance_journals j ON j.id=f.journal_id \
                     WHERE f.id=$1 AND f.order_id=$2 AND f.version=$3 \
                       AND f.invoice_id IS NULL AND j.status='posted'",
                )
                .bind(fulfillment_id)
                .bind(order_id)
                .bind(*expected_version)
                .fetch_optional(&mut *self.connection)
                .await?;
                if hit.is_none() {
                    return Ok(false);
                }
            }
            Ok(true)
        })
    }

    fn verify_decide_state<'a>(
        &'a mut self,
        plan: &'a SupplierBillDecidePlan,
        bill_status: &'a str,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let hit: Option<i32> = sqlx::query_scalar(
                "SELECT 1 FROM procurement_bill_matches m \
                 WHERE m.id=$1 AND m.version=$2 AND m.status=$3 \
                   AND m.company_id=$4 \
                   AND ($5::text IS NULL OR m.branch_id=$5) \
                   AND ($6::text IS NULL OR m.department_id=$6)",
            )
            .bind(&plan.match_id)
            .bind(plan.expected_version)
            .bind(bill_status)
            .bind(plan.commit.company_id.as_str())
            .bind(plan.commit.branch_id.as_deref())
            .bind(plan.commit.department_id.as_deref())
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(hit.is_some())
        })
    }

    fn verify_policy_version<'a>(
        &'a mut self,
        plan: &'a SupplierBillPolicyPlan,
        current_version: i64,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let hit: Option<i32> = if current_version == 0 {
                sqlx::query_scalar(
                    "SELECT 1 WHERE NOT EXISTS(SELECT 1 FROM procurement_invoice_policies WHERE company_id=$1)",
                )
                .bind(plan.commit.company_id.as_str())
                .fetch_optional(&mut *self.connection)
                .await?
            } else {
                sqlx::query_scalar(
                    "SELECT 1 FROM procurement_invoice_policies WHERE company_id=$1 AND version=$2",
                )
                .bind(plan.commit.company_id.as_str())
                .bind(current_version)
                .fetch_optional(&mut *self.connection)
                .await?
            };
            Ok(hit.is_some())
        })
    }

    fn insert_match<'a>(
        &'a mut self,
        plan: &'a SupplierBillCapturePlan,
        attributes: &'a CaptureAttributes,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            // procurement_stmt_4 bind order.
            let inserted = sqlx::query(
                "INSERT INTO procurement_bill_matches(id,company_id,branch_id,department_id,order_id,fulfillment_id,supplier_id,currency,chart_id,clearing_account_id,payable_account_id,tax_account_id,reference,invoice_date,due_date,evidence,expected_subtotal_minor,subtotal_minor,tax_minor,quantity_matches,status,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,'review',$21,$22,$23)",
            )
            .bind(&plan.match_id)
            .bind(plan.commit.company_id.as_str())
            .bind(&attributes.order_branch_id)
            .bind(attributes.order_department_id.as_deref())
            .bind(&attributes.order_id)
            .bind(&attributes.fulfillment_id)
            .bind(&attributes.supplier_id)
            .bind(&attributes.currency)
            .bind(&attributes.chart_id)
            .bind(&attributes.clearing_account_id)
            .bind(&attributes.payable_account_id)
            .bind(&attributes.tax_account_id)
            .bind(&attributes.reference)
            .bind(&attributes.invoice_date)
            .bind(&attributes.due_date)
            .bind(&attributes.evidence)
            .bind(attributes.expected_subtotal)
            .bind(attributes.subtotal)
            .bind(attributes.tax)
            .bind(attributes.quantity_matches)
            .bind(&attributes.created_by)
            .bind(plan.commit.now.get())
            .bind(plan.commit.now.get())
            .execute(&mut *self.connection)
            .await;
            match inserted {
                Ok(_) => Ok(()),
                Err(error) if is_conflict(&error) => Err(changed().into()),
                Err(error) => Err(error.into()),
            }
        })
    }

    fn insert_match_receipt<'a>(
        &'a mut self,
        plan: &'a SupplierBillCapturePlan,
        index: usize,
        fulfillment_id: &'a str,
        expected_version: i64,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            // procurement_stmt_5 bind order. The retained commit stores the
            // post-stage receipt version (`expectedVersion + 1`): the staging
            // step advances the Commerce fulfillment inside this same unit.
            let id = format!("{}-receipt-{index}", plan.match_id);
            let captured = expected_version.checked_add(1).ok_or_else(changed)?;
            let inserted = sqlx::query(
                "INSERT INTO procurement_bill_match_receipts(id,match_id,fulfillment_id,captured_version) VALUES($1,$2,$3,$4)",
            )
            .bind(&id)
            .bind(&plan.match_id)
            .bind(fulfillment_id)
            .bind(captured)
            .execute(&mut *self.connection)
            .await;
            match inserted {
                Ok(_) => Ok(()),
                Err(error) if is_conflict(&error) => Err(changed().into()),
                Err(error) => Err(error.into()),
            }
        })
    }

    fn insert_match_lines<'a>(
        &'a mut self,
        plan: &'a SupplierBillCapturePlan,
        lines: &'a [CapturePreparedLine],
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            // procurement_stmt_6 bind order.
            for (ordinal, line) in lines.iter().enumerate() {
                let id = format!("{}-{ordinal}", plan.match_id);
                let inserted = sqlx::query(
                    "INSERT INTO procurement_bill_match_lines VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
                )
                .bind(&id)
                .bind(&plan.match_id)
                .bind(&line.fulfillment_line_id)
                .bind(line.quantity)
                .bind(line.expected_quantity)
                .bind(line.price)
                .bind(line.tax_bps)
                .bind(line.expected_subtotal)
                .bind(line.subtotal)
                .bind(line.tax)
                .execute(&mut *self.connection)
                .await;
                match inserted {
                    Ok(_) => (),
                    Err(error) if is_conflict(&error) => return Err(changed().into()),
                    Err(error) => return Err(error.into()),
                }
            }
            Ok(())
        })
    }

    fn insert_event<'a>(
        &'a mut self,
        plan: &'a SupplierBillCommitPlan,
        match_id: &'a str,
        version: i64,
        action: &'a str,
        note: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            // procurement_stmt_1 bind order. The id is the retained
            // `bill-event-{key}` shape from the caller's command key.
            sqlx::query(
                "INSERT INTO procurement_bill_match_events VALUES($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(format!("bill-event-{}", plan.key.as_str()))
            .bind(match_id)
            .bind(version)
            .bind(action)
            .bind(&plan.actor_id)
            .bind(note)
            .bind(plan.now.get())
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn apply_decision<'a>(
        &'a mut self,
        plan: &'a SupplierBillDecidePlan,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            // procurement_stmt_10 bind order. The version CAS lives in the
            // `verify_decide_state` fence, exactly like the retained
            // assertion/update pair.
            let status = if plan.action == "approve" {
                "approved"
            } else {
                "returned"
            };
            sqlx::query(
                "UPDATE procurement_bill_matches SET status=$1,reviewed_by=$2,review_reason=$3,variance_account_id=$4,version=version+1,updated_at=$5 WHERE id=$6",
            )
            .bind(status)
            .bind(&plan.commit.actor_id)
            .bind(&plan.reason)
            .bind(plan.variance_account_id.as_deref())
            .bind(plan.commit.now.get())
            .bind(&plan.match_id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn deactivate_match_receipts<'a>(
        &'a mut self,
        plan: &'a SupplierBillDecidePlan,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            // procurement_stmt_9.
            sqlx::query(
                "UPDATE procurement_bill_match_receipts SET active=FALSE WHERE match_id=$1 AND active=TRUE",
            )
            .bind(&plan.match_id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn upsert_policy<'a>(
        &'a mut self,
        plan: &'a SupplierBillPolicyPlan,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            // procurement_stmt_7 bind order.
            sqlx::query(
                "INSERT INTO procurement_invoice_policies(company_id,matching_mode,version,updated_by,updated_at) VALUES($1,$2,$3,$4,$5) ON CONFLICT(company_id) DO UPDATE SET matching_mode=excluded.matching_mode,version=excluded.version,updated_by=excluded.updated_by,updated_at=excluded.updated_at",
            )
            .bind(plan.commit.company_id.as_str())
            .bind(&plan.matching_mode)
            .bind(plan.expected_version + 1)
            .bind(&plan.commit.actor_id)
            .bind(plan.commit.now.get())
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn insert_policy_event<'a>(
        &'a mut self,
        plan: &'a SupplierBillPolicyPlan,
        previous: &'a str,
        reason: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            // procurement_stmt_8 bind order.
            sqlx::query(
                "INSERT INTO procurement_invoice_policy_events(id,company_id,version,previous_mode,matching_mode,actor_id,reason,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(format!("supplier-policy-{}", plan.commit.key.as_str()))
            .bind(plan.commit.company_id.as_str())
            .bind(plan.expected_version + 1)
            .bind(previous)
            .bind(&plan.matching_mode)
            .bind(&plan.commit.actor_id)
            .bind(reason)
            .bind(plan.commit.now.get())
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        key: &'a str,
        operation: &'a str,
        actor: &'a SupplierBillCommandActor,
        request_fingerprint: &'a str,
        record_id: &'a str,
        result_json: &'a str,
        authority_json: &'a str,
        now: TimestampMillis,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "INSERT INTO command_receipts\
                 (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,\
                  authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
                 ON CONFLICT(command_id) DO NOTHING",
            )
            .bind(key)
            .bind(operation)
            .bind(&actor.user_id)
            .bind(request_fingerprint)
            .bind(record_id)
            .bind(result_json)
            .bind(now.get())
            .bind(authority_json)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn audit<'a>(
        &'a mut self,
        plan: &'a SupplierBillCommitPlan,
        receipt: &'a SupplierBillRecordReceipt,
        authority_json: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO audit_events\
                 (id,command_id,actor_id,assignment_id,action,record_type,record_id,\
                  record_version,purpose,created_at,authority_json) \
                 VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            )
            .bind(plan.key.as_str())
            .bind(&plan.actor_id)
            .bind(&plan.assignment_id)
            .bind(&plan.operation)
            .bind("supplier_bill")
            .bind(&receipt.id)
            .bind(receipt.version)
            .bind(&plan.operation)
            .bind(plan.now.get())
            .bind(authority_json)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn outbox<'a>(
        &'a mut self,
        plan: &'a SupplierBillCommitPlan,
        receipt: &'a SupplierBillRecordReceipt,
        authority_json: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let authority: serde_json::Value =
                serde_json::from_str(authority_json).map_err(|_| invalid_authority())?;
            let payload = serde_json::json!({
                "authority": authority,
                "id": receipt.id,
                "version": receipt.version,
                "companyId": plan.company_id.as_str(),
                "branchId": plan.branch_id,
                "departmentId": plan.department_id,
            });
            sqlx::query(
                "INSERT INTO outbox_events\
                 (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
                  payload_json,created_at) \
                 VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(plan.key.as_str())
            .bind(&plan.operation)
            .bind("supplier_bill")
            .bind(&receipt.id)
            .bind(receipt.version)
            .bind(payload.to_string())
            .bind(plan.now.get())
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn readback<'a>(
        &'a mut self,
        company: &'a CompanyId,
        match_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<SupplierBillReadback>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT m.id,m.version,m.status \
                 FROM procurement_bill_matches m \
                 WHERE m.id=$1 AND m.company_id=$2",
            )
            .bind(match_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(SupplierBillReadback {
                    id: row.try_get("id")?,
                    version: row.try_get("version")?,
                    status: row.try_get("status")?,
                })
            })
            .transpose()
        })
    }
}

fn invalid_image() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMAND_RECEIPT_INVALID",
        "The Procurement command acknowledgement could not be read back.",
    )
    .into()
}

fn invalid_authority() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMAND_RECEIPT_INVALID",
        "The current Procurement authority could not be retained.",
    )
    .into()
}

fn storage() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "SUPPLIER_BILL_QUERY_FAILED",
        "The supplier invoice command is unavailable.",
    )
    .into()
}
