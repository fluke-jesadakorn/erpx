//! Finance-owned fresh posting source. This retains the existing invoice,
//! captured Field money lineage and options on the caller's SERIALIZABLE unit.
//! Preparation does not authorize or commit. Application writes remain in the
//! caller-owned unit; an HTTP routing observation never substitutes its guard.
use super::field_service_invoice_read::{
    CurrentFinanceServiceInvoiceWorkspace, read_service_invoice_workspace,
};
use crate::{
    application::journal::FinanceActor,
    domain::{
        journal::JournalLineInput,
        service_invoice_posting::{
            ServiceInvoicePostFacts, service_invoice_post_lines,
            validate_service_invoice_post_source,
        },
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
pub struct CurrentServiceInvoicePostSource {
    current: CurrentFinanceServiceInvoiceWorkspace,
    lines: Vec<JournalLineInput>,
    version: i64,
    total: i64,
    actor: FinanceActor,
    id: String,
    chart_id: String,
}
impl CurrentServiceInvoicePostSource {
    pub fn lines(&self) -> &[JournalLineInput] {
        &self.lines
    }
    pub fn version(&self) -> i64 {
        self.version
    }
    pub fn chart_id(&self) -> &str {
        &self.chart_id
    }
    pub fn total(&self) -> i64 {
        self.total
    }
    /// Must run before applying the journal/invoice effects on the same unit.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.current.require_current(c).await
    }
    pub fn workspace(&self) -> &super::field_service_invoice_read::FinanceServiceInvoiceWorkspace {
        self.current.data()
    }
}
fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "INVOICE_SOURCE_CHANGED",
        "The retained Field invoice source changed.",
    )
}
/// Invoke only after current authority and immutable receipt lookup. The outer
/// IAM adapter supplies same-Person identity, never an inbound body flag.
pub async fn prepare_service_invoice_post_source(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
    expected_version: Option<i64>,
    same_person_as_creator: bool,
    has_review_attachments: bool,
) -> Result<CurrentServiceInvoicePostSource, Error> {
    let current = read_service_invoice_workspace(c, actor, id).await?;
    let invoice = &current.data().invoice;
    let text = |key: &str| {
        invoice
            .get(key)
            .and_then(|v| v.as_str())
            .ok_or_else(invalid)
    };
    if text("status")? != "draft"
        || text("kind")? != "receivable"
        || text("source_type")? != "field_service"
    {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "INVOICE_NOT_FOUND",
            "A postable draft invoice was not found.",
        )
        .into());
    }
    let integer = |key: &str| {
        invoice
            .get(key)
            .and_then(|v| {
                v.as_i64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
            .ok_or_else(invalid)
    };
    let tax_account = match invoice.get("tax_account_id") {
        Some(serde_json::Value::Null) | None => None,
        Some(v) => Some(v.as_str().ok_or_else(invalid)?),
    };
    let facts = ServiceInvoicePostFacts {
        version: integer("version")?,
        subtotal: integer("subtotal_minor")?,
        tax: integer("tax_minor")?,
        total: integer("total_minor")?,
        debit_account: text("debit_account_id")?,
        credit_account: text("credit_account_id")?,
        tax_account,
    };
    validate_service_invoice_post_source(&facts, expected_version, same_person_as_creator)?;
    let chart_id:Option<String>=sqlx::query_scalar("SELECT a.chart_id FROM finance_accounts a JOIN finance_charts chart ON chart.id=a.chart_id WHERE a.id=$1 AND a.company_id=$2").bind(facts.debit_account).bind(actor.company_id.as_str()).fetch_optional(&mut *c).await?;
    let chart_id = chart_id.ok_or_else(|| {
        BusinessError::invalid("ACCOUNT_INVALID", "Invoice account is unavailable.")
    })?;
    let lines = service_invoice_post_lines(
        &facts,
        expected_version,
        same_person_as_creator,
        has_review_attachments,
    )?;
    let version = facts.version;
    let total = facts.total;
    Ok(CurrentServiceInvoicePostSource {
        current,
        lines,
        version,
        total,
        actor: actor.clone(),
        id: id.into(),
        chart_id,
    })
}

/// Applied source image retained through Workflow audit/outbox/deferred effects.
/// Construction is private; the outer unit must revalidate before commit.
pub struct RetainedServiceInvoicePost {
    current: CurrentFinanceServiceInvoiceWorkspace,
    journal_image: String,
    journal_id: String,
    company: String,
}
impl RetainedServiceInvoicePost {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.current.require_current(c).await?;
        if journal_image(c, &self.company, &self.journal_id).await? != self.journal_image {
            return Err(invalid().into());
        }
        Ok(())
    }
}
async fn require_posted_journal(
    c: &mut PgConnection,
    plan: &crate::application::journal::JournalPreparation,
) -> Result<(), Error> {
    let d = plan.draft();
    let actual: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(j) FROM finance_journals j WHERE company_id=$1 AND id=$2 FOR SHARE",
    )
    .bind(plan.actor().company_id.as_str())
    .bind(plan.journal_id())
    .fetch_one(&mut *c)
    .await?;
    let expected = serde_json::json!({"id":plan.journal_id(),"company_id":plan.actor().company_id.as_str(),"chart_id":d.chart_id,"period_id":d.period_id,"journal_number":plan.number(),"entry_date":d.entry_date,"currency":d.currency,"fx_rate_micros":plan.fx(),"source_type":"invoice","source_id":d.source_id,"memo":d.memo,"is_opening":0,"status":"posted","created_by":plan.actor().user_id.as_str(),"posted_by":plan.actor().user_id.as_str(),"posted_at":plan.now().get(),"created_at":plan.now().get(),"updated_at":plan.now().get(),"version":1});
    if expected
        .as_object()
        .ok_or_else(invalid)?
        .iter()
        .any(|(k, v)| actual.get(k) != Some(v))
    {
        return Err(invalid().into());
    }
    let rows:Vec<serde_json::Value>=sqlx::query_scalar("SELECT to_jsonb(l) FROM finance_journal_lines l WHERE journal_id=$1 ORDER BY ordinal,id FOR SHARE").bind(plan.journal_id()).fetch_all(&mut *c).await?;
    if rows.len() != plan.amounts().lines.len() {
        return Err(invalid().into());
    }
    for (i, (actual, line)) in rows.iter().zip(&plan.amounts().lines).enumerate() {
        let dimensions = serde_json::to_string(&line.dimensions.clone().unwrap_or_default())
            .map_err(|_| invalid())?;
        let expected = serde_json::json!({"id":format!("{}-{i}",plan.journal_id()),"journal_id":plan.journal_id(),"account_id":line.account_id,"description":line.description.as_deref().unwrap_or(""),"debit_minor":line.debit,"credit_minor":line.credit,"base_debit_minor":line.base_debit,"base_credit_minor":line.base_credit,"dimension_json":dimensions,"ordinal":i});
        if actual != &expected {
            return Err(invalid().into());
        }
    }
    Ok(())
}
async fn journal_image(c: &mut PgConnection, company: &str, id: &str) -> Result<String, Error> {
    let image:Option<String>=sqlx::query_scalar("SELECT jsonb_build_object('journal',to_jsonb(j),'lines',COALESCE((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM finance_journal_lines l WHERE l.journal_id=j.id),'[]'::jsonb))::text FROM finance_journals j WHERE j.company_id=$1 AND j.id=$2").bind(company).bind(id).fetch_optional(c).await?;
    let image = image.ok_or_else(invalid)?;
    if image.len() > 2 * 1024 * 1024 {
        return Err(invalid().into());
    }
    Ok(image)
}
impl CurrentServiceInvoicePostSource {
    /// No commit and no receipt creation here. The outer Workflow unit retains
    /// current IAM/Field/Commercial witnesses and adds the one command outcome.
    pub async fn apply(
        self,
        c: &mut PgConnection,
        plan: &crate::application::journal::JournalPreparation,
    ) -> Result<RetainedServiceInvoicePost, Error> {
        use crate::{
            adapters::postgres_journal::PgJournalStore, application::journal::JournalStatus,
            domain::journal::convert_at_fx,
        };
        self.require_current(c).await?;
        let draft = plan.draft();
        let encoded =
            |lines: &[JournalLineInput]| serde_json::to_string(lines).map_err(|_| invalid());
        let invoice = &self.workspace().invoice;
        let text = |k: &str| invoice.get(k).and_then(|v| v.as_str()).ok_or_else(invalid);
        if plan.actor() != &self.actor
            || !matches!(plan.result().status, JournalStatus::Posted)
            || draft.source_type != "invoice"
            || draft.source_id.as_deref() != Some(self.id.as_str())
            || draft.is_opening
            || draft.currency != text("currency")?
            || encoded(&draft.lines)? != encoded(&self.lines)?
        {
            return Err(invalid().into());
        }
        let base_total = convert_at_fx(self.total, plan.fx()).map_err(|e| {
            BusinessError::new(e.kind, e.code, "The invoice journal could not be posted.")
        })?;
        // Retain every pre-existing column, not just a version counter, so an
        // immediate trigger cannot rewrite another invoice fact unnoticed.
        let before: serde_json::Value = sqlx::query_scalar(
            "SELECT to_jsonb(i) FROM finance_invoices i WHERE company_id=$1 AND id=$2",
        )
        .bind(self.actor.company_id.as_str())
        .bind(&self.id)
        .fetch_one(&mut *c)
        .await?;
        let mut expected = before.clone();
        let map = expected.as_object_mut().ok_or_else(invalid)?;
        for (key, value) in [
            ("status", serde_json::json!("posted")),
            ("journal_id", serde_json::json!(plan.journal_id())),
            ("fx_rate_micros", serde_json::json!(plan.fx())),
            ("base_total_minor", serde_json::json!(base_total)),
            ("base_outstanding_minor", serde_json::json!(base_total)),
            (
                "approved_by",
                serde_json::json!(self.actor.user_id.as_str()),
            ),
            ("posted_at", serde_json::json!(plan.now().get())),
            ("version", serde_json::json!(self.version + 1)),
            ("updated_at", serde_json::json!(plan.now().get())),
        ] {
            map.insert(key.into(), value);
        }
        PgJournalStore::new(
            self.actor.holding_id.clone(),
            self.actor.company_id.clone(),
            c,
        )
        .persist(plan)
        .await
        .map_err(|e| match e {
            PgTransactionError::Business(e) => PgTransactionError::Business(BusinessError::new(
                e.kind,
                e.code,
                "The invoice journal could not be posted.",
            )),
            PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
        })?;
        let previous: Option<String> =
            sqlx::query_scalar("SELECT current_setting('erpx.service_invoice_post_runtime',true)")
                .fetch_one(&mut *c)
                .await?;
        sqlx::query("SELECT set_config('erpx.service_invoice_post_runtime',jsonb_build_object('database',current_database(),'pid',pg_backend_pid()::text,'xid',pg_current_xact_id()::text,'started',pg_postmaster_start_time()::text,'invoice',$1::jsonb)::text,true)").bind(&expected).execute(&mut *c).await?;
        let changed=sqlx::query("UPDATE finance_invoices i SET status='posted',journal_id=$3,fx_rate_micros=$4,base_total_minor=$5,base_outstanding_minor=$5,approved_by=$6,posted_at=$7,version=version+1,updated_at=$7 WHERE company_id=$1 AND id=$2 AND status='draft' AND version=$8 AND to_jsonb(i)=$9")
            .bind(self.actor.company_id.as_str()).bind(&self.id).bind(plan.journal_id()).bind(plan.fx()).bind(base_total).bind(self.actor.user_id.as_str()).bind(plan.now().get()).bind(self.version).bind(before).execute(&mut *c).await?;
        sqlx::query("SELECT set_config('erpx.service_invoice_post_runtime',$1,true)")
            .bind(previous.as_deref().unwrap_or(""))
            .execute(&mut *c)
            .await?;
        if changed.rows_affected() != 1 {
            return Err(invalid().into());
        }
        let actual: serde_json::Value = sqlx::query_scalar(
            "SELECT to_jsonb(i) FROM finance_invoices i WHERE company_id=$1 AND id=$2",
        )
        .bind(self.actor.company_id.as_str())
        .bind(&self.id)
        .fetch_one(&mut *c)
        .await?;
        if actual != expected {
            return Err(invalid().into());
        }
        let revision = invoice
            .get("line_revision")
            .and_then(|v| v.as_i64())
            .ok_or_else(invalid)?;
        let money=sqlx::query("INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at,carried_source_type,carried_source_id,carried_amount_kind) SELECT s.company_id,'finance_invoice',$2,'total',$3::date,s.currency,s.currency_scale,s.amount_units,s.usd_units,s.fx_rate_id,s.used_latest_fallback,$4,s.source_type,s.source_id,s.amount_kind FROM money_transaction_snapshots s WHERE s.company_id=$1 AND s.amount_units=$5::numeric AND ((s.source_type='finance_invoice' AND s.source_id=$2 AND s.amount_kind=$6) OR (s.source_type='field_service_billing_release' AND s.source_id=$7 AND s.amount_kind='invoice_total' AND NOT EXISTS(SELECT 1 FROM money_transaction_snapshots d WHERE d.company_id=$1 AND d.source_type='finance_invoice' AND d.source_id=$2 AND d.amount_kind=$6)))")
            .bind(self.actor.company_id.as_str()).bind(&self.id).bind(text("invoice_date")?).bind(plan.now().get()).bind(self.total.to_string()).bind(format!("draft:v{revision}")).bind(&self.workspace().source.release_id).execute(&mut *c).await?;
        if money.rows_affected() != 1 {
            return Err(invalid().into());
        }
        let current = read_service_invoice_workspace(c, &self.actor, &self.id).await?;
        require_posted_journal(c, plan).await?;
        let image = journal_image(c, self.actor.company_id.as_str(), plan.journal_id()).await?;
        super::service_receipt_runtime::mark_invoice(c, self.actor.company_id.as_str(), &self.id)
            .await?;
        Ok(RetainedServiceInvoicePost {
            current,
            journal_image: image,
            journal_id: plan.journal_id().into(),
            company: self.actor.company_id.as_str().into(),
        })
    }
}

/// Routing observation only. The consuming native command proves Field lineage
/// again in its own transaction; this read never grants posting authority.
pub async fn service_invoice_post_runtime(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
) -> Result<bool, Error> {
    let source: Option<String> = sqlx::query_scalar(
        "SELECT source_type FROM finance_invoices WHERE company_id=$1 AND id=$2 FOR SHARE",
    )
    .bind(actor.company_id.as_str())
    .bind(id)
    .fetch_optional(&mut *c)
    .await?;
    source.map(|s| s == "field_service").ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "INVOICE_NOT_FOUND",
            "Invoice not found.",
        )
        .into()
    })
}
