//! Finance-owned current receipt and immutable requested Money. Current IAM,
//! Field creation lineage and Workflow decision/audit effects remain outer ports.
use crate::{application::journal::FinanceActor, domain::service_receipt::*};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[allow(clippy::cmp_owned)]
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_PAYMENT_SOURCE_CHANGED",
        "The controlled payment's original Service invoice changed. Refresh its source workflow.",
    )
    .into()
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
#[allow(clippy::cmp_owned)]
async fn image(c: &mut PgConnection, company: &str, id: &str) -> Result<(Value, String), Error> {
    let payment:Option<Option<Value>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(p)::text)<=65536 THEN to_jsonb(p) END FROM finance_payments p WHERE company_id=$1 AND id=$2 AND kind='receipt' FOR UPDATE")
        .bind(company).bind(id).fetch_optional(&mut *c).await?;
    let row = payment.flatten().ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "PAYMENT_NOT_FOUND",
            "Payment not found.",
        )
    })?;
    let money:Option<Option<String>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(m)::text)<=65536 THEN to_jsonb(m)::text END FROM money_transaction_snapshots m WHERE company_id=$1 AND source_type='finance_payment' AND source_id=$2 AND amount_kind='requested' FOR SHARE")
        .bind(company).bind(id).fetch_optional(&mut *c).await?;
    let money_text = money.flatten().ok_or_else(changed)?;
    let money: Value = serde_json::from_str(&money_text).map_err(|_| changed())?;
    if money.get("currency") != row.get("currency")
        || money["currency_scale"].as_i64() != Some(2)
        || money["amount_units"].to_string() != row["amount_minor"].to_string()
        || money["usd_units"].as_i64().is_some_and(|v| v < 0)
        || row["withheld_minor"].as_i64() != Some(0)
        || !row["withholding_assessment_id"].is_null()
    {
        return Err(changed());
    }
    Ok((row, money_text))
}
pub struct CurrentServiceReceiptReview {
    unit: Unit,
    company: String,
    id: String,
    before: (Value, String),
}
impl CurrentServiceReceiptReview {
    pub fn row(&self) -> &Value {
        &self.before.0
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || image(c, &self.company, &self.id).await? != self.before {
            return Err(changed());
        }
        Ok(())
    }
#[allow(clippy::too_many_arguments)]
    pub async fn apply(
        self,
        c: &mut PgConnection,
        actor: &FinanceActor,
        approve: bool,
        expected: i64,
        same_creator: bool,
        same_approver: bool,
        at: i64,
    ) -> Result<RetainedServiceReceiptReview, Error> {
        self.require_current(c).await?;
        if actor.company_id.as_str() != self.company || !(0..=9_007_199_254_740_991).contains(&at) {
            return Err(changed());
        }
        let row = &self.before.0;
        let facts = ServiceReceiptReviewFacts {
            status: row["status"].as_str().ok_or_else(changed)?,
            version: row["version"].as_i64().ok_or_else(changed)?,
            has_bank_account: row["bank_account"].as_str().is_some_and(|v| !v.is_empty()),
            same_creator,
            same_approver,
            same_releaser: false,
        };
        if approve {
            validate_service_receipt_approval(&facts, expected)?;
        } else {
            validate_service_receipt_release(&facts, expected)?;
        }
        let mut planned = self.before;
        planned.0["status"] = Value::from(if approve { "approved" } else { "released" });
        planned.0[if approve {
            "approved_by"
        } else {
            "released_by"
        }] = Value::from(actor.user_id.as_str());
        planned.0["version"] = Value::from(
            expected
                .checked_add(1)
                .filter(|v| *v <= 9_007_199_254_740_991)
                .ok_or_else(changed)?,
        );
        planned.0["updated_at"] = Value::from(at);
        let sql = if approve {
            "UPDATE finance_payments SET status='approved',approved_by=$1,version=version+1,updated_at=$2 WHERE id=$3 AND company_id=$4 AND version=$5"
        } else {
            "UPDATE finance_payments SET status='released',released_by=$1,version=version+1,updated_at=$2 WHERE id=$3 AND company_id=$4 AND version=$5"
        };
        if sqlx::query(sql)
            .bind(actor.user_id.as_str())
            .bind(at)
            .bind(&self.id)
            .bind(&self.company)
            .bind(expected)
            .execute(&mut *c)
            .await?
            .rows_affected()
            != 1
        {
            return Err(changed());
        }
        let retained = RetainedServiceReceiptReview {
            unit: self.unit,
            company: self.company,
            id: self.id,
            planned,
        };
        retained.require_current(c).await?;
        super::service_receipt_runtime::mark(c, &retained.company, &retained.id, &[]).await?;
        Ok(retained)
    }
}
pub struct RetainedServiceReceiptReview {
    unit: Unit,
    company: String,
    id: String,
    planned: (Value, String),
}
impl RetainedServiceReceiptReview {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || image(c, &self.company, &self.id).await? != self.planned {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_service_receipt_review(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
) -> Result<CurrentServiceReceiptReview, Error> {
    let unit = unit(c).await?;
    let before = image(c, actor.company_id.as_str(), id).await?;
    Ok(CurrentServiceReceiptReview {
        unit,
        company: actor.company_id.as_str().into(),
        id: id.into(),
        before,
    })
}

/// Current Finance half of Field receipt lineage. Workflow proves creation;
/// Field proves release. Every consuming command rechecks party/currency and
/// the complete allocation partition on its own retained transaction.
pub async fn require_service_receipt_finance_lineage(
    c: &mut PgConnection,
    actor: &FinanceActor,
    payment: &str,
    invoice: &str,
    creating: bool,
) -> Result<(), Error> {
    unit(c).await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_payments WHERE id=$1)")
            .bind(payment)
            .fetch_one(&mut *c)
            .await?;
    if creating && !exists {
        return Ok(());
    }
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_payments p JOIN finance_invoices original ON original.id=$2 AND original.company_id=p.company_id AND original.party_id=p.party_id AND original.currency=p.currency WHERE p.id=$1 AND p.company_id=$3 AND p.kind='receipt' AND original.source_type='field_service' AND original.kind='receivable' AND NOT EXISTS(SELECT 1 FROM finance_allocations a WHERE a.payment_id=p.id AND (a.company_id<>p.company_id OR a.invoice_id<>original.id)) AND (p.status NOT IN ('settled','voided') OR EXISTS(SELECT 1 FROM finance_allocations a WHERE a.payment_id=p.id AND a.company_id=p.company_id AND a.invoice_id=original.id HAVING sum(a.amount_minor)=p.amount_minor)))")
        .bind(payment).bind(invoice).bind(actor.company_id.as_str()).fetch_one(&mut *c).await?;
    if !valid {
        return Err(changed());
    }
    Ok(())
}
