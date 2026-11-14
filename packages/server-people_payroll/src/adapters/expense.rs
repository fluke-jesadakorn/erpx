//! PostgreSQL adapter for native expense claims.
//!
//! The caller owns one serializable Holding transaction and fresh Account/IAM
//! authority. This adapter writes only People-owned `finance_expenses`,
//! `expense_decisions`, `expense_payment_drafts` rows plus the standard command
//! receipt/audit/outbox ledger. Workflow approvals, Files evidence, Operations
//! staff facts, Organization routes and Finance postings/settlements are
//! supplied through composed ports; their SQL is never reached here.
use crate::application::expense::{
    ExpenseAccountingWrite, ExpenseCancelWrite, ExpenseCommandFuture, ExpenseCommandRepository,
    ExpenseDecisionInsert, ExpenseInsert, ExpenseLedgerEntry, ExpensePaymentCancelWrite,
    ExpensePaymentConfirmedWrite, ExpensePaymentDraftInsert, ExpensePaymentDraftWrite,
    ExpensePostWrite, ExpenseReverseWrite, ExpenseReviewWrite, ExpenseSettlementWrite,
    ExpenseSubmitWrite, ExpenseUpdate, ExpenseVersionBump, StoredExpenseReceipt,
};
use crate::domain::expense::{
    ExpensePaymentDraft, ExpenseRow, changed, idempotency_conflict, request_invalid,
};
use crate::domain::workforce::WorkforceCommandActor;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn write_error(error: sqlx::Error) -> Error {
    if error
        .as_database_error()
        .and_then(|value| value.code())
        .as_deref()
        == Some("23505")
    {
        conflict(
            "CONFLICT",
            "The expense record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
        )
        .into()
    } else {
        error.into()
    }
}

fn accounting_value(row: &sqlx::postgres::PgRow) -> Result<Value, sqlx::Error> {
    let raw: String = row.try_get("accounting_json")?;
    Ok(serde_json::from_str(&raw).unwrap_or(Value::Null))
}

pub struct PgExpenseCommand<'c> {
    connection: &'c mut PgConnection,
    company: CompanyId,
    user_id: String,
    assignment_id: String,
    authority_json: String,
}

impl<'c> PgExpenseCommand<'c> {
    pub fn new(connection: &'c mut PgConnection, actor: &WorkforceCommandActor) -> Self {
        Self {
            connection,
            company: actor.company_id.clone(),
            user_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            authority_json: actor.authority_json(),
        }
    }

    async fn read_ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("PEOPLE_EXPENSE_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }

    async fn ready(&mut self) -> Result<(), Error> {
        self.read_ready().await?;
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("PEOPLE_EXPENSE_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn insert_ledger(&mut self, entry: &ExpenseLedgerEntry<'_>) -> Result<(), Error> {
        let result = serde_json::to_string(entry.result).map_err(|_| request_invalid())?;
        let inserted = sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
             result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
             ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(entry.key)
        .bind(entry.operation)
        .bind(&self.user_id)
        .bind(entry.request_hash)
        .bind(entry.record_id)
        .bind(result)
        .bind(entry.now)
        .bind(&self.authority_json)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if inserted != 1 {
            return Err(idempotency_conflict().into());
        }
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
             record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$5,$9,$10) ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("people-expense-audit-{}", entry.key))
        .bind(entry.key)
        .bind(&self.user_id)
        .bind(&self.assignment_id)
        .bind(entry.operation)
        .bind(entry.record_type)
        .bind(entry.record_id)
        .bind(entry.version)
        .bind(entry.now)
        .bind(&self.authority_json)
        .execute(&mut *self.connection)
        .await?;
        let authority: Value = serde_json::from_str(&self.authority_json).unwrap_or(Value::Null);
        let payload = serde_json::json!({
            "authority": authority,
            "id": entry.record_id,
            "state": entry.state,
            "companyId": self.company.as_str(),
            "version": entry.version,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
             aggregate_version,payload_json,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("people-expense-outbox-{}", entry.key))
        .bind(entry.key)
        .bind(entry.operation)
        .bind(entry.record_type)
        .bind(entry.record_id)
        .bind(entry.version)
        .bind(payload)
        .bind(entry.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

const EXPENSE_COLUMNS: &str = "x.id,x.company_id,x.request_kind,x.payee_party_id,x.employee_id,\
 x.engagement_id,x.branch_id,x.department_id,x.claimant_user_id,x.claimant_name,p.legal_name,\
 x.expense_date,x.merchant,x.currency,x.amount_minor,x.tax_minor,x.category_account_id,\
 x.receipt_reference,x.business_purpose,x.prepared_by,x.approved_by,x.status,x.workflow_version,\
 x.workflow_stage,x.accounting_json::text accounting_json,x.invoice_id,x.journal_id,\
 x.rejection_reason,x.reversal_journal_id,x.cancelled_at,x.submitted_at,x.posted_at,\
 x.reimbursed_at,x.version,x.created_by,x.created_at,x.updated_at";

const EXPENSE_FROM: &str = "FROM finance_expenses x \
 LEFT JOIN hr_people p ON p.id=x.employee_id AND p.company_id=x.company_id AND p.merged_into_id IS NULL";

fn map_expense_row(row: sqlx::postgres::PgRow) -> Result<ExpenseRow, sqlx::Error> {
    Ok(ExpenseRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        request_kind: row.try_get("request_kind")?,
        payee_party_id: row.try_get("payee_party_id")?,
        employee_id: row.try_get("employee_id")?,
        engagement_id: row.try_get("engagement_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        claimant_user_id: row.try_get("claimant_user_id")?,
        claimant_name: row.try_get("claimant_name")?,
        legal_name: row.try_get("legal_name")?,
        expense_date: row.try_get("expense_date")?,
        merchant: row.try_get("merchant")?,
        currency: row.try_get("currency")?,
        amount_minor: row.try_get("amount_minor")?,
        tax_minor: row.try_get("tax_minor")?,
        category_account_id: row.try_get("category_account_id")?,
        receipt_reference: row.try_get("receipt_reference")?,
        business_purpose: row.try_get("business_purpose")?,
        prepared_by: row.try_get("prepared_by")?,
        approved_by: row.try_get("approved_by")?,
        status: row.try_get("status")?,
        workflow_version: row.try_get("workflow_version")?,
        workflow_stage: row.try_get("workflow_stage")?,
        accounting_json: accounting_value(&row)?,
        invoice_id: row.try_get("invoice_id")?,
        journal_id: row.try_get("journal_id")?,
        rejection_reason: row.try_get("rejection_reason")?,
        reversal_journal_id: row.try_get("reversal_journal_id")?,
        cancelled_at: row.try_get("cancelled_at")?,
        submitted_at: row.try_get("submitted_at")?,
        posted_at: row.try_get("posted_at")?,
        reimbursed_at: row.try_get("reimbursed_at")?,
        version: row.try_get("version")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn map_payment_draft(row: sqlx::postgres::PgRow) -> Result<ExpensePaymentDraft, sqlx::Error> {
    Ok(ExpensePaymentDraft {
        id: row.try_get("id")?,
        expense_id: row.try_get("expense_id")?,
        company_id: row.try_get("company_id")?,
        created_by: row.try_get("created_by")?,
        payment_date: row.try_get("payment_date")?,
        amount_minor: row.try_get("amount_minor")?,
        reference: row.try_get("reference")?,
        status: row.try_get("status")?,
        payment_id: row.try_get("payment_id")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

impl ExpenseCommandRepository for PgExpenseCommand<'_> {
    type Error = Error;

    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("people-expense-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> ExpenseCommandFuture<'a, String, Self::Error> {
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
    ) -> ExpenseCommandFuture<'a, Option<StoredExpenseReceipt>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            let request_hash = row.try_get::<String, _>("request_hash")?;
            let result = match row.try_get::<Option<String>, _>("result_json")? {
                None => None,
                Some(value) => Some(serde_json::from_str(&value).map_err(|_| {
                    Error::Business(BusinessError::new(
                        FailureKind::Unavailable,
                        "PEOPLE_EXPENSE_RECEIPT_INVALID",
                        "The expense command receipt is invalid.",
                    ))
                })?),
            };
            Ok(Some(StoredExpenseReceipt {
                request_hash,
                result,
            }))
        })
    }

    fn ledger<'a>(
        &'a mut self,
        entry: &'a ExpenseLedgerEntry<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.insert_ledger(entry).await
        })
    }

    fn expense_rows<'a>(
        &'a mut self,
        company: &'a CompanyId,
        user_id: &'a str,
        reviewer_read: bool,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
    ) -> ExpenseCommandFuture<'a, Vec<ExpenseRow>, Self::Error> {
        Box::pin(async move {
            self.read_ready().await?;
            let sql = format!(
                "SELECT {EXPENSE_COLUMNS} {EXPENSE_FROM} WHERE x.company_id=$1 \
                 AND (x.claimant_user_id=$2 OR x.created_by=$2 OR x.prepared_by=$2 OR x.approved_by=$2 \
                   OR $3::boolean OR ($4::text IS NOT NULL AND x.branch_id=$4) \
                   OR ($5::text IS NOT NULL AND x.department_id=$5)) \
                 ORDER BY x.created_at DESC LIMIT 250"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(company.as_str())
                .bind(user_id)
                .bind(reviewer_read)
                .bind(branch_id)
                .bind(department_id)
                .fetch_all(&mut *self.connection)
                .await?;
            rows.into_iter()
                .map(map_expense_row)
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Error::from)
        })
    }

    fn expense_row<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
        user_id: &'a str,
        reviewer_read: bool,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseRow>, Self::Error> {
        Box::pin(async move {
            self.read_ready().await?;
            let sql = format!(
                "SELECT {EXPENSE_COLUMNS} {EXPENSE_FROM} WHERE x.id=$1 AND x.company_id=$2 \
                 AND (x.claimant_user_id=$3 OR x.created_by=$3 OR x.prepared_by=$3 OR x.approved_by=$3 \
                   OR $4::boolean OR ($5::text IS NOT NULL AND x.branch_id=$5) \
                   OR ($6::text IS NOT NULL AND x.department_id=$6))"
            );
            let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(id)
                .bind(company.as_str())
                .bind(user_id)
                .bind(reviewer_read)
                .bind(branch_id)
                .bind(department_id)
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(map_expense_row).transpose().map_err(Error::from)
        })
    }

    fn expense_row_for_update<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseRow>, Self::Error> {
        Box::pin(async move {
            self.read_ready().await?;
            let sql = format!(
                "SELECT {EXPENSE_COLUMNS} {EXPENSE_FROM} \
                 WHERE x.id=$1 AND x.company_id=$2 FOR UPDATE OF x"
            );
            let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(id)
                .bind(company.as_str())
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(map_expense_row).transpose().map_err(Error::from)
        })
    }

    fn claimant<'a>(
        &'a mut self,
        company: &'a CompanyId,
        person_id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<crate::application::expense::ExpenseClaimant>, Self::Error>
    {
        Box::pin(async move {
            self.read_ready().await?;
            let row = sqlx::query(
                "SELECT id,user_id,legal_name FROM hr_people WHERE id=$1 AND company_id=$2 \
                 AND status='active' AND merged_into_id IS NULL",
            )
            .bind(person_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, sqlx::Error>(crate::application::expense::ExpenseClaimant {
                    id: row.try_get("id")?,
                    user_id: row.try_get("user_id")?,
                    legal_name: row.try_get("legal_name")?,
                })
            })
            .transpose()
            .map_err(Error::from)
        })
    }

    fn latest_engagement<'a>(
        &'a mut self,
        company: &'a CompanyId,
        person_id: &'a str,
    ) -> ExpenseCommandFuture<
        'a,
        Option<crate::application::expense::ExpenseEngagementScope>,
        Self::Error,
    > {
        Box::pin(async move {
            self.read_ready().await?;
            let row = sqlx::query(
                "SELECT branch_id,department_id FROM hr_engagements \
                 WHERE person_id=$1 AND company_id=$2 ORDER BY start_date DESC LIMIT 1",
            )
            .bind(person_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, sqlx::Error>(crate::application::expense::ExpenseEngagementScope {
                    branch_id: row.try_get("branch_id")?,
                    department_id: row.try_get("department_id")?,
                })
            })
            .transpose()
            .map_err(Error::from)
        })
    }

    fn create_expense<'a>(
        &'a mut self,
        command: &'a ExpenseInsert<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO finance_expenses(id,company_id,request_kind,payee_party_id,employee_id,\
                 engagement_id,branch_id,department_id,claimant_user_id,claimant_name,expense_date,\
                 merchant,currency,amount_minor,tax_minor,category_account_id,receipt_reference,\
                 business_purpose,prepared_by,status,workflow_version,workflow_stage,version,\
                 created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,\
                 'draft',2,'draft',1,$20,$21,$21)",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.request_kind)
            .bind(command.payee_party_id)
            .bind(command.employee_id)
            .bind(command.engagement_id)
            .bind(command.branch_id)
            .bind(command.department_id)
            .bind(command.claimant_user_id)
            .bind(command.claimant_name)
            .bind(command.expense_date)
            .bind(command.merchant)
            .bind(command.currency)
            .bind(command.amount_minor)
            .bind(command.tax_minor)
            .bind(command.category_account_id)
            .bind(command.receipt_reference)
            .bind(command.business_purpose)
            .bind(command.prepared_by)
            .bind(command.created_by)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            Ok(())
        })
    }

    fn update_expense<'a>(
        &'a mut self,
        command: &'a ExpenseUpdate<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let _ = command.key;
            let changed_rows = sqlx::query(
                "UPDATE finance_expenses SET request_kind=$3,payee_party_id=$4,employee_id=$5,\
                 engagement_id=$6,branch_id=$7,department_id=$8,claimant_user_id=$9,\
                 claimant_name=$10,expense_date=$11,merchant=$12,currency=$13,amount_minor=$14,\
                 tax_minor=$15,category_account_id=$16,receipt_reference=$17,business_purpose=$18,\
                 prepared_by=$19,status='draft',workflow_version=2,workflow_stage='draft',\
                 version=version+1,updated_at=$20 \
                 WHERE id=$1 AND company_id=$2 AND version=$21 AND cancelled_at IS NULL \
                 AND status IN ('draft','rejected')",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.request_kind)
            .bind(command.payee_party_id)
            .bind(command.employee_id)
            .bind(command.engagement_id)
            .bind(command.branch_id)
            .bind(command.department_id)
            .bind(command.claimant_user_id)
            .bind(command.claimant_name)
            .bind(command.expense_date)
            .bind(command.merchant)
            .bind(command.currency)
            .bind(command.amount_minor)
            .bind(command.tax_minor)
            .bind(command.category_account_id)
            .bind(command.receipt_reference)
            .bind(command.business_purpose)
            .bind(command.prepared_by)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed_rows != 1 {
                return Err(changed("EXPENSE_CHANGED").into());
            }
            Ok(())
        })
    }

    fn submit_expense<'a>(
        &'a mut self,
        command: &'a ExpenseSubmitWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let _ = command.key;
            let changed_rows = sqlx::query(
                "UPDATE finance_expenses SET employee_id=$3,engagement_id=$4,branch_id=$5,\
                 department_id=$6,workflow_version=2,workflow_stage=$7,status='in_review',\
                 accounting_json='{}',approved_by=NULL,rejection_reason=NULL,submitted_at=$8,\
                 version=version+1,updated_at=$8 \
                 WHERE id=$1 AND company_id=$2 AND version=$9 AND cancelled_at IS NULL \
                 AND status IN ('draft','rejected')",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.employee_id)
            .bind(command.engagement_id)
            .bind(command.branch_id)
            .bind(command.department_id)
            .bind(command.stage)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed_rows != 1 {
                return Err(changed("EXPENSE_CHANGED").into());
            }
            Ok(())
        })
    }

    fn review_expense<'a>(
        &'a mut self,
        command: &'a ExpenseReviewWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let _ = command.key;
            let changed_rows = sqlx::query(
                "UPDATE finance_expenses SET status=$4,workflow_stage=$5,rejection_reason=$6,\
                 approved_by=CASE WHEN $7::boolean THEN NULL \
                   WHEN $8::text IS NOT NULL THEN $8 ELSE approved_by END,\
                 version=version+1,updated_at=$9 \
                 WHERE id=$1 AND company_id=$2 AND version=$10 AND cancelled_at IS NULL \
                 AND status=$3",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.from_status)
            .bind(command.status)
            .bind(command.stage)
            .bind(command.reason)
            .bind(command.clear_approved)
            .bind(command.approved_by)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed_rows != 1 {
                return Err(changed("EXPENSE_CHANGED").into());
            }
            Ok(())
        })
    }

    fn cancel_expense<'a>(
        &'a mut self,
        command: &'a ExpenseCancelWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let _ = command.key;
            let changed_rows = sqlx::query(
                "UPDATE finance_expenses SET status='rejected',workflow_stage='cancelled',\
                 cancelled_at=$4,rejection_reason=$3,version=version+1,updated_at=$4 \
                 WHERE id=$1 AND company_id=$2 AND version=$5 AND cancelled_at IS NULL \
                 AND status IN ('draft','rejected','in_review')",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.reason)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed_rows != 1 {
                return Err(changed("EXPENSE_CHANGED").into());
            }
            Ok(())
        })
    }

    fn save_accounting<'a>(
        &'a mut self,
        command: &'a ExpenseAccountingWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let _ = command.key;
            let changed_rows = sqlx::query(
                "UPDATE finance_expenses SET category_account_id=$3,amount_minor=$4,tax_minor=$5,\
                 accounting_json=CAST($6 AS text)::jsonb,version=version+1,updated_at=$7 \
                 WHERE id=$1 AND company_id=$2 AND version=$8 AND cancelled_at IS NULL \
                 AND status='in_review' AND workflow_stage='accounts_review'",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.category_account_id)
            .bind(command.amount_minor)
            .bind(command.tax_minor)
            .bind(command.accounting_json)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed_rows != 1 {
                return Err(changed("EXPENSE_CHANGED").into());
            }
            Ok(())
        })
    }

    fn post_expense<'a>(
        &'a mut self,
        command: &'a ExpensePostWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let _ = command.key;
            let changed_rows = sqlx::query(
                "UPDATE finance_expenses SET status='posted',workflow_stage='payment',\
                 invoice_id=$3,journal_id=$4,posted_at=$5,version=version+1,updated_at=$5 \
                 WHERE id=$1 AND company_id=$2 AND version=$6 AND cancelled_at IS NULL \
                 AND status='approved' AND workflow_stage='gl_posting'",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.invoice_id)
            .bind(command.journal_id)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed_rows != 1 {
                return Err(changed("EXPENSE_CHANGED").into());
            }
            Ok(())
        })
    }

    fn settle_expense<'a>(
        &'a mut self,
        command: &'a ExpenseSettlementWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let _ = command.key;
            let changed_rows = sqlx::query(
                "UPDATE finance_expenses SET \
                 status=CASE WHEN $3=0 THEN 'reimbursed' ELSE 'posted' END,\
                 workflow_stage=CASE WHEN $3=0 THEN 'completed' ELSE 'payment' END,\
                 reimbursed_at=CASE WHEN $3=0 THEN $4 ELSE reimbursed_at END,\
                 version=version+1,updated_at=$4 \
                 WHERE id=$1 AND company_id=$2 AND version=$5 AND cancelled_at IS NULL",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.outstanding_minor)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed_rows != 1 {
                return Err(changed("EXPENSE_CHANGED").into());
            }
            Ok(())
        })
    }

    fn reverse_expense<'a>(
        &'a mut self,
        command: &'a ExpenseReverseWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let _ = command.key;
            let changed_rows = sqlx::query(
                "UPDATE finance_expenses SET status='rejected',cancelled_at=$3,\
                 rejection_reason=$4,reversal_journal_id=$5,version=version+1,updated_at=$3 \
                 WHERE id=$1 AND company_id=$2 AND version=$6 AND status='posted' \
                 AND cancelled_at IS NULL",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.now)
            .bind(command.reason)
            .bind(command.reversal_journal_id)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed_rows != 1 {
                return Err(changed("EXPENSE_CHANGED").into());
            }
            Ok(())
        })
    }

    fn insert_decision<'a>(
        &'a mut self,
        command: &'a ExpenseDecisionInsert<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO expense_decisions(id,expense_id,command_id,action,actor_id,\
                 reviewed_version,reason,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
                 ON CONFLICT(id) DO NOTHING",
            )
            .bind(command.id)
            .bind(command.expense_id)
            .bind(command.command_id)
            .bind(command.action)
            .bind(command.actor_id)
            .bind(command.reviewed_version)
            .bind(command.reason)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            Ok(())
        })
    }

    fn payment_draft<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpensePaymentDraft>, Self::Error> {
        Box::pin(async move {
            self.read_ready().await?;
            let row = sqlx::query(
                "SELECT id,expense_id,company_id,created_by,payment_date,amount_minor,reference,\
                 status,payment_id,version,created_at,updated_at FROM expense_payment_drafts \
                 WHERE id=$1 AND company_id=$2",
            )
            .bind(id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(map_payment_draft).transpose().map_err(Error::from)
        })
    }

    fn payment_draft_for_update<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpensePaymentDraft>, Self::Error> {
        Box::pin(async move {
            self.read_ready().await?;
            let row = sqlx::query(
                "SELECT id,expense_id,company_id,created_by,payment_date,amount_minor,reference,\
                 status,payment_id,version,created_at,updated_at FROM expense_payment_drafts \
                 WHERE id=$1 AND company_id=$2 FOR UPDATE",
            )
            .bind(id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(map_payment_draft).transpose().map_err(Error::from)
        })
    }

    fn payment_drafts<'a>(
        &'a mut self,
        company: &'a CompanyId,
        expense_id: &'a str,
    ) -> ExpenseCommandFuture<'a, Vec<ExpensePaymentDraft>, Self::Error> {
        Box::pin(async move {
            self.read_ready().await?;
            let rows = sqlx::query(
                "SELECT id,expense_id,company_id,created_by,payment_date,amount_minor,reference,\
                 status,payment_id,version,created_at,updated_at FROM expense_payment_drafts \
                 WHERE expense_id=$1 AND company_id=$2 ORDER BY created_at,id",
            )
            .bind(expense_id)
            .bind(company.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(map_payment_draft)
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Error::from)
        })
    }

    fn native_payment_prepared<'a>(
        &'a mut self,
        company: &'a CompanyId,
        expense_id: &'a str,
        draft_id: &'a str,
    ) -> ExpenseCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.read_ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM command_receipts c \
                 WHERE c.operation='expense.payment.prepare' AND c.record_id=$2 \
                 AND c.authority_json::jsonb->>'companyId'=$1 \
                 AND c.result_json::jsonb->>'draftId'=$3 \
                 AND c.result_json::jsonb->>'id'=$2)",
            )
            .bind(company.as_str())
            .bind(expense_id)
            .bind(draft_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn insert_payment_draft<'a>(
        &'a mut self,
        command: &'a ExpensePaymentDraftInsert<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO expense_payment_drafts(id,expense_id,company_id,created_by,status,\
                 version,created_at,updated_at) VALUES($1,$2,$3,$4,'draft',1,$5,$5)",
            )
            .bind(command.id)
            .bind(command.expense_id)
            .bind(command.company_id)
            .bind(command.created_by)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            Ok(())
        })
    }

    fn save_payment_draft<'a>(
        &'a mut self,
        command: &'a ExpensePaymentDraftWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed_rows = sqlx::query(
                "UPDATE expense_payment_drafts SET payment_date=$3,amount_minor=$4,reference=$5,\
                 version=version+1,updated_at=$6 \
                 WHERE id=$1 AND company_id=$2 AND version=$7 AND status='draft' \
                 AND payment_id IS NULL",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.payment_date)
            .bind(command.amount_minor)
            .bind(command.reference)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed_rows != 1 {
                return Err(changed("EXPENSE_PAYMENT_CHANGED").into());
            }
            Ok(())
        })
    }

    fn confirm_payment_draft<'a>(
        &'a mut self,
        command: &'a ExpensePaymentConfirmedWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed_rows = sqlx::query(
                "UPDATE expense_payment_drafts SET status='confirmed',payment_id=$3,\
                 payment_date=$4,amount_minor=$5,reference=$6,version=version+1,updated_at=$7 \
                 WHERE id=$1 AND company_id=$2 AND version=$8 AND status='draft' \
                 AND payment_id IS NULL",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.payment_id)
            .bind(command.payment_date)
            .bind(command.amount_minor)
            .bind(command.reference)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed_rows != 1 {
                return Err(changed("EXPENSE_PAYMENT_CHANGED").into());
            }
            Ok(())
        })
    }

    fn cancel_payment_draft<'a>(
        &'a mut self,
        command: &'a ExpensePaymentCancelWrite<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed_rows = sqlx::query(
                "UPDATE expense_payment_drafts SET status='cancelled',version=version+1,\
                 updated_at=$3 WHERE id=$1 AND company_id=$2 AND version=$4 \
                 AND status='draft' AND payment_id IS NULL",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed_rows != 1 {
                return Err(changed("EXPENSE_PAYMENT_CHANGED").into());
            }
            Ok(())
        })
    }

    fn bump_expense_version<'a>(
        &'a mut self,
        command: &'a ExpenseVersionBump<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed_rows = sqlx::query(
                "UPDATE finance_expenses SET version=version+1,updated_at=$3 \
                 WHERE id=$1 AND company_id=$2 AND version=$4 AND cancelled_at IS NULL",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed_rows != 1 {
                return Err(changed("EXPENSE_CHANGED").into());
            }
            Ok(())
        })
    }

    fn bump_payment_version<'a>(
        &'a mut self,
        command: &'a ExpenseVersionBump<'a>,
    ) -> ExpenseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed_rows = sqlx::query(
                "UPDATE expense_payment_drafts SET version=version+1,updated_at=$3 \
                 WHERE id=$1 AND company_id=$2 AND version=$4 AND status='draft'",
            )
            .bind(command.id)
            .bind(command.company_id)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed_rows != 1 {
                return Err(changed("EXPENSE_PAYMENT_CHANGED").into());
            }
            Ok(())
        })
    }
}

/// Fail-closed reader used before a command exists. It requires only an open
/// caller-owned transaction, never a serializable one.
pub struct PgExpenseReader<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgExpenseReader<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn expense_row(
        &mut self,
        company: &CompanyId,
        id: &str,
        user_id: &str,
        reviewer_read: bool,
        branch_id: Option<&str>,
        department_id: Option<&str>,
    ) -> Result<Option<ExpenseRow>, Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("PEOPLE_EXPENSE_TRANSACTION_REQUIRED").into());
        }
        let sql = format!(
            "SELECT {EXPENSE_COLUMNS} {EXPENSE_FROM} WHERE x.id=$1 AND x.company_id=$2 \
             AND (x.claimant_user_id=$3 OR x.created_by=$3 OR x.prepared_by=$3 OR x.approved_by=$3 \
               OR $4::boolean OR ($5::text IS NOT NULL AND x.branch_id=$5) \
               OR ($6::text IS NOT NULL AND x.department_id=$6))"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .bind(company.as_str())
            .bind(user_id)
            .bind(reviewer_read)
            .bind(branch_id)
            .bind(department_id)
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(map_expense_row).transpose().map_err(Error::from)
    }

    pub async fn payment_draft(
        &mut self,
        company: &CompanyId,
        id: &str,
    ) -> Result<Option<ExpensePaymentDraft>, Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("PEOPLE_EXPENSE_TRANSACTION_REQUIRED").into());
        }
        let row = sqlx::query(
            "SELECT id,expense_id,company_id,created_by,payment_date,amount_minor,reference,\
             status,payment_id,version,created_at,updated_at FROM expense_payment_drafts \
             WHERE id=$1 AND company_id=$2",
        )
        .bind(id)
        .bind(company.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(map_payment_draft).transpose().map_err(Error::from)
    }
}
