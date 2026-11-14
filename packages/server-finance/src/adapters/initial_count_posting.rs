//! Original physical-count Finance journal preparation on the owner's unit.
//! This cannot authorize Count, purchase, Flow or the captured Commerce mapping.
use crate::adapters::initial_count_finance::CurrentInitialCountFinance;
use crate::application::journal::{
    FinanceActor, JournalAuthority, JournalDraft, JournalPreparation, JournalStatus,
    prepare_journal,
};
use crate::domain::initial_count_variance::variance_lines;
use crate::domain::journal::{JournalError, JournalLineInput};
use crate::domain::money::units_to_decimal;
use crate::public::{
    FinanceCollation, PgJournalPreparation, PgJournalStore, embedded_currency_rules,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use num_bigint::BigInt;
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::{future::Future, sync::OnceLock};
type Error = PgTransactionError<BusinessError>;

/// Facts are supplied from the retained Inventory source and Workflow command,
/// never directly from request data. They do not themselves grant authority.
#[derive(Clone, PartialEq, Eq)]
pub struct InitialCountPostingFacts {
    pub actor: FinanceActor,
    pub command_key: CommandKey,
    pub count_id: String,
    pub count_number: String,
    pub reason: String,
    pub actual_cost_delta_minor: i64,
}
/// Implemented by the outer owner coordinator with actual original Count,
/// identity, purchase, Flow and Commerce proofs; no default implementation.
pub trait PgInitialCountPostingSource: Sync {
    fn facts(&self) -> &InitialCountPostingFacts;
    fn finance(&self) -> &CurrentInitialCountFinance;
    fn require_before(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn require_after(
        &self,
        c: &mut PgConnection,
        journal_id: Option<&str>,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
struct BoundAuthority {
    actor: FinanceActor,
    draft: JournalDraft,
}
impl JournalAuthority for BoundAuthority {
    type Error = PgTransactionError<JournalError>;
    async fn authorize(
        &mut self,
        actor: &FinanceActor,
        draft: &JournalDraft,
    ) -> Result<(), Self::Error> {
        let lines = draft.lines.len() == self.draft.lines.len()
            && draft.lines.iter().zip(&self.draft.lines).all(|(a, b)| {
                a.account_id == b.account_id
                    && a.debit == b.debit
                    && a.credit == b.credit
                    && a.description == b.description
                    && a.dimensions == b.dimensions
            });
        if actor != &self.actor
            || !lines
            || draft.chart_id != self.draft.chart_id
            || draft.period_id != self.draft.period_id
            || draft.entry_date != self.draft.entry_date
            || draft.currency != self.draft.currency
            || draft.fx_rate != self.draft.fx_rate
            || draft.memo != self.draft.memo
            || draft.source_type != self.draft.source_type
            || draft.source_id != self.draft.source_id
            || draft.is_opening
        {
            return Err(JournalError {
                kind: FailureKind::Forbidden,
                code: "COUNT_FINANCE_BINDING_REQUIRED",
                message: "Use the currently authorized Count posting.".into(),
            }
            .into());
        }
        Ok(())
    }
}
fn journal_error(e: PgTransactionError<JournalError>) -> Error {
    match e {
        PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
        PgTransactionError::Business(e) => {
            BusinessError::new(e.kind, e.code, "The Count Finance posting is unavailable.").into()
        }
    }
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "COUNT_FINANCE_SOURCE_CHANGED",
        "The original Count Finance posting changed.",
    )
    .into()
}
/// Preparation uses only the owner's retained transaction and does not commit.
pub struct PreparedInitialCountPosting<'a, S: PgInitialCountPostingSource> {
    source: &'a S,
    plan: Option<JournalPreparation>,
    facts: InitialCountPostingFacts,
}
pub async fn prepare_initial_count_posting<'a, S: PgInitialCountPostingSource, C: Clock + Sync>(
    c: &mut PgConnection,
    source: &'a S,
    clock: &C,
) -> Result<PreparedInitialCountPosting<'a, S>, Error> {
    source.require_before(c).await?;
    let finance = source.finance();
    finance.require_current(c).await?;
    let facts = source.facts().clone();
    if &facts.actor.holding_id != finance.holding_id()
        || facts.actor.company_id != finance.basis().company_id
        || facts.count_id.is_empty()
        || facts.count_number.is_empty()
        || facts.reason.trim().is_empty()
    {
        return Err(changed());
    }
    let lines = variance_lines(
        &finance.basis().inventory_account_id,
        &finance.basis().variance_account_id,
        facts.actual_cost_delta_minor,
    )?;
    let plan = if lines.is_empty() {
        None
    } else {
        let lines = lines
            .into_iter()
            .map(|line| {
                Ok(JournalLineInput {
                    account_id: line.account_id,
                    description: None,
                    debit: units_to_decimal(&BigInt::from(line.debit_minor), 2)
                        .map_err(|_| changed())?,
                    credit: units_to_decimal(&BigInt::from(line.credit_minor), 2)
                        .map_err(|_| changed())?,
                    dimensions: None,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let draft = JournalDraft {
            chart_id: finance.basis().chart_id.clone(),
            period_id: finance.period_id().into(),
            entry_date: finance.count_date().into(),
            currency: finance.basis().currency.clone(),
            fx_rate: None,
            is_opening: false,
            memo: format!("{} · {}", facts.count_number, facts.reason),
            source_type: "stock_count".into(),
            source_id: Some(facts.count_id.clone()),
            lines,
        };
        let mut authority = BoundAuthority {
            actor: facts.actor.clone(),
            draft: draft.clone(),
        };
        let rules = embedded_currency_rules().map_err(|_| changed())?;
        let collation = FinanceCollation::english().map_err(|_| changed())?;
        let mut ports =
            PgJournalPreparation::new(c, &facts.actor, &mut authority, clock, &rules, &collation)
                .map_err(journal_error)?;
        Some(
            prepare_journal(
                &mut ports,
                &facts.actor,
                &draft,
                &facts.command_key,
                JournalStatus::Posted,
            )
            .await
            .map_err(journal_error)?,
        )
    };
    source.require_before(c).await?;
    finance.require_current(c).await?;
    if source.facts() != &facts {
        return Err(changed());
    }
    Ok(PreparedInitialCountPosting {
        source,
        plan,
        facts,
    })
}
impl<'a, S: PgInitialCountPostingSource> PreparedInitialCountPosting<'a, S> {
    pub fn journal_id(&self) -> Option<&str> {
        self.plan.as_ref().map(|p| p.journal_id())
    }
    /// Workflow first performs replay/receipt and its guarded owner effects.
    /// This phase writes only Finance journal/lines; deferred PostgreSQL triggers
    /// own book projections, dimensions and financial event evidence.
    pub async fn persist(
        self,
        c: &mut PgConnection,
    ) -> Result<RetainedInitialCountPosting<'a, S>, Error> {
        self.source.require_before(c).await?;
        self.source.finance().require_current(c).await?;
        if self.source.facts() != &self.facts {
            return Err(changed());
        }
        let existing: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_journals WHERE company_id=$1 AND source_type='stock_count' AND source_id=$2)").bind(self.facts.actor.company_id.as_str()).bind(&self.facts.count_id).fetch_one(&mut *c).await?;
        if existing {
            return Err(changed());
        }
        if let Some(plan) = &self.plan {
            PgJournalStore::new(
                self.facts.actor.holding_id.clone(),
                self.facts.actor.company_id.clone(),
                c,
            )
            .persist(plan)
            .await
            .map_err(journal_error)?;
        }
        let receipt = RetainedInitialCountPosting {
            source: self.source,
            plan: self.plan,
            facts: self.facts,
            flush_window: OnceLock::new(),
        };
        receipt.require_finance(c).await?;
        Ok(receipt)
    }
}
pub struct RetainedInitialCountPosting<'a, S: PgInitialCountPostingSource> {
    source: &'a S,
    plan: Option<JournalPreparation>,
    facts: InitialCountPostingFacts,
    flush_window: OnceLock<(i64, i64)>,
}
impl<S: PgInitialCountPostingSource> RetainedInitialCountPosting<'_, S> {
    pub fn journal_id(&self) -> Option<&str> {
        self.plan.as_ref().map(|p| p.journal_id())
    }
    pub async fn require_finance(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.source.finance().require_current(c).await?;
        if self.source.facts() != &self.facts {
            return Err(changed());
        }
        let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(j) FROM finance_journals j WHERE company_id=$1 AND source_type='stock_count' AND source_id=$2 ORDER BY id FOR SHARE").bind(self.facts.actor.company_id.as_str()).bind(&self.facts.count_id).fetch_all(&mut *c).await?;
        let Some(p) = &self.plan else {
            if !rows.is_empty() {
                return Err(changed());
            }
            return Ok(());
        };
        let d = p.draft();
        let a = p.actor();
        let expected = json!({"id":p.journal_id(),"company_id":a.company_id.as_str(),"chart_id":d.chart_id,"period_id":d.period_id,"journal_number":p.number(),"entry_date":d.entry_date,"currency":d.currency,"fx_rate_micros":p.fx(),"source_type":"stock_count","source_id":d.source_id,"memo":d.memo,"is_opening":0,"status":"posted","created_by":a.user_id.as_str(),"posted_by":a.user_id.as_str(),"posted_at":p.now().get(),"reversed_by":null,"reversed_at":null,"created_at":p.now().get(),"updated_at":p.now().get(),"version":1});
        if rows != [expected] {
            return Err(changed());
        }
        let actual:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(l) FROM finance_journal_lines l WHERE journal_id=$1 ORDER BY ordinal,id LIMIT 3 FOR SHARE").bind(p.journal_id()).fetch_all(&mut *c).await?;
        let expected=p.amounts().lines.iter().enumerate().map(|(i,l)| Ok(json!({"id":format!("{}-{i}",p.journal_id()),"journal_id":p.journal_id(),"account_id":l.account_id,"description":l.description.as_deref().unwrap_or(""),"debit_minor":l.debit,"credit_minor":l.credit,"base_debit_minor":l.base_debit,"base_credit_minor":l.base_credit,"dimension_json":serde_json::to_string(&l.dimensions.clone().unwrap_or_default()).map_err(|_|changed())?,"ordinal":i}))).collect::<Result<Vec<_>,Error>>()?;
        if actual != expected {
            return Err(changed());
        }
        Ok(())
    }
    /// Flush in the same order as Finance journal review. The owner invokes this
    /// after its Count, receipt, audit and Flow writes; failures roll back all.
    pub async fn flush_and_require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_finance(c).await?;
        self.source.require_after(c, self.journal_id()).await?;
        if self.flush_window.get().is_some() {
            return Err(changed());
        }
        let before = database_millis(c).await?;
        sqlx::query("SET CONSTRAINTS finance_book_projection IMMEDIATE")
            .execute(&mut *c)
            .await?;
        sqlx::query("SET CONSTRAINTS finance_book_projection DEFERRED")
            .execute(&mut *c)
            .await?;
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *c)
            .await?;
        let after = database_millis(c).await?;
        if after < before {
            return Err(changed());
        }
        self.validate_after(c, before, after).await?;
        self.flush_window
            .set((before, after))
            .map_err(|_| changed())?;
        Ok(())
    }
    /// Recheck against the original database-clock window, never a new baseline.
    pub async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        let &(before, after) = self.flush_window.get().ok_or_else(changed)?;
        self.validate_after(c, before, after).await
    }
    async fn validate_after(
        &self,
        c: &mut PgConnection,
        before: i64,
        after: i64,
    ) -> Result<(), Error> {
        self.require_finance(c).await?;
        self.source.require_after(c, self.journal_id()).await?;
        if let Some(p) = &self.plan {
            let valid: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_financial_events e JOIN finance_journals j ON j.id=e.journal_id JOIN finance_charts c ON c.id=j.chart_id WHERE j.id=$1 AND e.id='journal-event-'||j.id AND e.company_id=j.company_id AND e.chart_id=j.chart_id AND e.period_id=j.period_id AND e.source_type='stock_count' AND e.source_id=j.source_id AND e.source_version=j.version AND e.effective_date=j.entry_date AND e.currency=j.currency AND e.base_currency=c.base_currency AND e.event_kind='recognition' AND e.legacy_snapshot=0 AND e.recorded_at BETWEEN $2 AND $3 AND e.recorded_at BETWEEN 0 AND 9007199254740991 AND to_jsonb(e)=jsonb_build_object('id','journal-event-'||j.id,'company_id',j.company_id,'chart_id',j.chart_id,'period_id',j.period_id,'journal_id',j.id,'source_type',j.source_type,'source_id',j.source_id,'source_version',j.version,'effective_date',j.entry_date,'currency',j.currency,'base_currency',c.base_currency,'event_kind','recognition','evidence_json',jsonb_build_object('journal',to_jsonb(j),'lines',COALESCE((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.ordinal) FROM finance_journal_lines l WHERE l.journal_id=j.id),'[]'::jsonb))::text,'legacy_snapshot',0,'recorded_at',e.recorded_at))").bind(p.journal_id()).bind(before).bind(after).fetch_one(&mut *c).await?;
            if !valid {
                return Err(changed());
            }
            // The deferred trigger selects the latest approved rule per secondary
            // chart. Verify the complete selected set and the projected line and
            // event evidence after the caller has flushed the constraints.
            let books_valid: bool = sqlx::query_scalar(r#"WITH primary_j AS (SELECT * FROM finance_journals WHERE id=$1), selected AS (
                SELECT DISTINCT ON (r.secondary_chart_id) r.* FROM finance_book_rules r,primary_j p
                WHERE r.company_id=p.company_id AND r.primary_chart_id=p.chart_id AND r.status='approved' AND r.effective_date<=p.entry_date
                ORDER BY r.secondary_chart_id,r.effective_date DESC
              ), actual AS (SELECT b.* FROM finance_book_postings b WHERE b.primary_journal_id=$1)
              SELECT (SELECT count(*) FROM selected)=(SELECT count(*) FROM actual)
                AND NOT EXISTS(SELECT 1 FROM selected r CROSS JOIN primary_j p LEFT JOIN actual b ON b.rule_id=r.id
                  LEFT JOIN finance_journals j ON j.id=b.secondary_journal_id
                  WHERE b.rule_id IS NULL OR b.company_id<>p.company_id OR b.source_version<>p.version
                    OR b.created_at NOT BETWEEN $2 AND $3 OR b.created_at NOT BETWEEN 0 AND 9007199254740991
                    OR to_jsonb(b) IS DISTINCT FROM jsonb_build_object('primary_journal_id',p.id,'secondary_journal_id','book-'||r.id||'-'||p.id,'rule_id',r.id,'company_id',p.company_id,'source_version',p.version,'created_at',b.created_at)
                    OR to_jsonb(j) IS DISTINCT FROM (to_jsonb(p)||jsonb_build_object('id','book-'||r.id||'-'||p.id,'chart_id',r.secondary_chart_id,'journal_number','book-'||r.id||'-'||p.id,'source_type','book_projection','source_id',p.id))
                    OR j.id IS DISTINCT FROM ('book-'||r.id||'-'||p.id) OR j.company_id IS DISTINCT FROM p.company_id
                    OR j.chart_id IS DISTINCT FROM r.secondary_chart_id OR j.period_id IS DISTINCT FROM p.period_id
                    OR j.journal_number IS DISTINCT FROM j.id OR j.entry_date IS DISTINCT FROM p.entry_date
                    OR j.currency IS DISTINCT FROM p.currency OR j.fx_rate_micros IS DISTINCT FROM p.fx_rate_micros
                    OR j.source_type IS DISTINCT FROM 'book_projection' OR j.source_id IS DISTINCT FROM p.id
                    OR j.memo IS DISTINCT FROM p.memo OR j.status IS DISTINCT FROM p.status OR j.is_opening IS DISTINCT FROM p.is_opening
                    OR j.version IS DISTINCT FROM p.version OR j.created_by IS DISTINCT FROM p.created_by
                    OR j.posted_by IS DISTINCT FROM p.posted_by OR j.posted_at IS DISTINCT FROM p.posted_at
                    OR j.reversed_by IS DISTINCT FROM p.reversed_by OR j.reversed_at IS DISTINCT FROM p.reversed_at
                    OR j.created_at IS DISTINCT FROM p.created_at OR j.updated_at IS DISTINCT FROM p.updated_at
                    OR (SELECT count(*) FROM finance_journal_lines l WHERE l.journal_id=j.id)<>(SELECT count(*) FROM finance_journal_lines l WHERE l.journal_id=p.id)
                    OR EXISTS(SELECT 1 FROM finance_journal_lines l LEFT JOIN finance_book_account_mappings m ON m.rule_id=r.id AND m.primary_account_id=l.account_id
                      LEFT JOIN finance_journal_lines x ON x.journal_id=j.id AND x.ordinal=l.ordinal
                      WHERE l.journal_id=p.id AND (to_jsonb(x) IS DISTINCT FROM (to_jsonb(l)||jsonb_build_object('id',j.id||'-'||l.ordinal,'journal_id',j.id,'account_id',m.secondary_account_id)) OR x.id IS DISTINCT FROM (j.id||'-'||l.ordinal) OR x.account_id IS DISTINCT FROM m.secondary_account_id
                        OR (x.description,x.debit_minor,x.credit_minor,x.base_debit_minor,x.base_credit_minor,x.dimension_json) IS DISTINCT FROM (l.description,l.debit_minor,l.credit_minor,l.base_debit_minor,l.base_credit_minor,l.dimension_json)))
                    OR NOT EXISTS(SELECT 1 FROM finance_financial_events e JOIN finance_charts chart ON chart.id=j.chart_id WHERE e.journal_id=j.id AND e.id='journal-event-'||j.id
                      AND e.company_id=j.company_id AND e.chart_id=j.chart_id AND e.period_id=j.period_id AND e.source_type=j.source_type AND e.source_id=j.source_id
                      AND e.source_version=j.version AND e.effective_date=j.entry_date AND e.currency=j.currency AND e.base_currency=chart.base_currency
                      AND e.event_kind='recognition' AND e.legacy_snapshot=0 AND e.recorded_at BETWEEN $2 AND $3 AND e.recorded_at BETWEEN 0 AND 9007199254740991 AND to_jsonb(e)=jsonb_build_object('id','journal-event-'||j.id,'company_id',j.company_id,'chart_id',j.chart_id,'period_id',j.period_id,'journal_id',j.id,'source_type',j.source_type,'source_id',j.source_id,'source_version',j.version,'effective_date',j.entry_date,'currency',j.currency,'base_currency',chart.base_currency,'event_kind','recognition','evidence_json',jsonb_build_object('journal',to_jsonb(j),'lines',COALESCE((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.ordinal) FROM finance_journal_lines l WHERE l.journal_id=j.id),'[]'::jsonb))::text,'legacy_snapshot',0,'recorded_at',e.recorded_at)))"#).bind(p.journal_id()).bind(before).bind(after).fetch_one(&mut *c).await?;
            if !books_valid {
                return Err(changed());
            }
        }
        self.require_finance(c).await
    }
}

async fn database_millis(c: &mut PgConnection) -> Result<i64, Error> {
    let value: i64 =
        sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint")
            .fetch_one(c)
            .await?;
    if !(0..=9_007_199_254_740_991).contains(&value) {
        return Err(changed());
    }
    Ok(value)
}
