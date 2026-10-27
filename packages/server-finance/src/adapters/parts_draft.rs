//! Finance-owned parts DRAFT persistence on the current original owner unit.
//! This source does not grant records.write, finance.write, a purchase or stock.
//! Its required composition port must retain the real owner/Inventory proofs.
use crate::adapters::postgres_inventory_consumption::prepare_owned_parts_stock_cost_money;
use crate::application::journal::prepare_journal_with_seed;
use crate::application::parts_draft::{
    PartsDraftConfig, PartsDraftFacts, PartsDraftPlan, PartsOwnerKind, derive_parts_draft,
};

use crate::public::{
    FinanceActor, FinanceCollation, JournalAuthority, JournalDraft, JournalError,
    JournalPreparation, JournalStatus, PgJournalPreparation, PgJournalStore,
    embedded_currency_rules,
    inventory_consumption::{
        CurrentConsumptionFinance, PreparedConsumptionMoney, RetainedConsumptionMoney,
        lock_inventory_consumption_finance_sources,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, RecordId, TimestampMillis};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;

/// Utility-only write-intent order. The owner bundle calls this BEFORE the
/// Inventory planner helper (which already locks Money), and before any IAM
/// Holding snapshot. Match the existing Finance review journal→lines→Money order.
pub async fn lock_owned_parts_draft_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(changed());
    }
    sqlx::query("LOCK TABLE finance_journals,finance_journal_lines,money_transaction_snapshots IN ROW EXCLUSIVE MODE").execute(&mut*c).await?;
    lock_inventory_consumption_finance_sources(c).await
}

/// Mandatory owner-local implementation; no default or personal-rights flag.
/// require_before retains the actual Work/actor/Inventory/Quality/purchase
/// inputs. It must not move final command-version/availability/date business
/// guards ahead of the original preparation error order. require_apply checks
/// the owner and Inventory guards at their canonical effect position.
/// require_after verifies its EXACT allowed owner/stock/lot/event/receipt
/// after-image, not broad acceptance of any changed row or a success boolean.
pub trait PgPartsDraftOwnerSource: Sync {
    fn facts(&self) -> &PartsDraftFacts;
    fn finance_source(&self) -> &CurrentConsumptionFinance;
    fn require_before(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn require_apply(
        &self,
        c: &mut PgConnection,
        expected: &PartsDraftExpected,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn require_after(
        &self,
        c: &mut PgConnection,
        expected: &PartsDraftExpected,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

/// Finance-derived output facts for owner-owned expected-after-image checks.
/// No public constructor, Deserialize, transaction or permission is exposed.
pub struct PartsDraftExpected {
    consumption_id: RecordId,
    journal_id: String,
    journal_number: String,
    currency: String,
    period_id: String,
    value_minor: i64,
    consumption_created_at: i64,
    journal_created_at: i64,
}
impl PartsDraftExpected {
    pub fn consumption_id(&self) -> &RecordId {
        &self.consumption_id
    }
    pub fn journal_id(&self) -> &str {
        &self.journal_id
    }
    pub fn journal_number(&self) -> &str {
        &self.journal_number
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub fn period_id(&self) -> &str {
        &self.period_id
    }
    pub fn value_minor(&self) -> i64 {
        self.value_minor
    }
    pub fn consumption_created_at(&self) -> i64 {
        self.consumption_created_at
    }
    pub fn journal_created_at(&self) -> i64 {
        self.journal_created_at
    }
}

struct BoundDraftAuthority {
    actor: FinanceActor,
    draft: JournalDraft,
}
impl JournalAuthority for BoundDraftAuthority {
    type Error = PgTransactionError<JournalError>;
    async fn authorize(
        &mut self,
        actor: &FinanceActor,
        draft: &JournalDraft,
    ) -> Result<(), Self::Error> {
        let lines = draft.lines.len() == self.draft.lines.len()
            && draft.lines.iter().zip(&self.draft.lines).all(|(a, b)| {
                a.account_id == b.account_id
                    && a.description == b.description
                    && a.debit == b.debit
                    && a.credit == b.credit
                    && a.dimensions == b.dimensions
            });
        if actor != &self.actor
            || !lines
            || draft.chart_id != self.draft.chart_id
            || draft.period_id != self.draft.period_id
            || draft.entry_date != self.draft.entry_date
            || draft.currency != self.draft.currency
            || draft.fx_rate != self.draft.fx_rate
            || draft.is_opening
            || draft.memo != self.draft.memo
            || draft.source_type != self.draft.source_type
            || draft.source_id != self.draft.source_id
        {
            return Err(JournalError {
                kind: FailureKind::Forbidden,
                code: "PARTS_DRAFT_BINDING_REQUIRED",
                message: "Use the exact currently authorized owner parts plan.".into(),
            }
            .into());
        }
        Ok(())
    }
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "PARTS_DRAFT_SOURCE_CHANGED",
        "The original owned parts preparation changed.",
    )
    .into()
}
fn journal_error(e: PgTransactionError<JournalError>) -> Error {
    match e {
        PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
        PgTransactionError::Business(e) => BusinessError::new(
            e.kind,
            e.code,
            "The current Finance parts preparation is unavailable.",
        )
        .into(),
    }
}
fn input_binding<S: PgPartsDraftOwnerSource>(
    source: &S,
    facts: &PartsDraftFacts,
) -> Result<PartsDraftPlan, Error> {
    let finance = source.finance_source();
    let config = finance.config().ok_or_else(changed)?;
    let [period] = finance.period_ids() else {
        return Err(changed());
    };
    Ok(derive_parts_draft(
        facts,
        &PartsDraftConfig {
            holding_id: finance.holding_id().clone(),
            company_id: finance.mapping().company_id.clone(),
            chart_id: config.chart_id.clone(),
            period_id: period.clone(),
            entry_date: finance.entry_date().into(),
            currency: config.currency.clone(),
            base_currency: config.base_currency.clone(),
            inventory_account_id: config.inventory_account_id.clone(),
            expense_account_id: config.cogs_account_id.clone(),
        },
    )?)
}

#[derive(PartialEq, Eq)]
struct Unit(String, i32, String, String);
impl Unit {
    async fn current(c: &mut PgConnection) -> Result<Self, Error> {
        if !c.is_in_transaction() {
            return Err(changed());
        }
        let value:(String,i32,String,String)=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?;
        Ok(Self(value.0, value.1, value.2, value.3))
    }
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if &Self::current(c).await? != self {
            return Err(changed());
        }
        Ok(())
    }
}

pub struct PreparedPartsDraft<'a, S: PgPartsDraftOwnerSource> {
    owner: &'a S,
    unit: Unit,
    facts: PartsDraftFacts,
    expected: PartsDraftExpected,
    journal: JournalPreparation,
    money: PreparedConsumptionMoney,
}
/// Owned original preparation for the gap between Workflow prepare and apply.
/// Its facts are a binding, not authority. Persistence requires the actual
/// original owner source again and cannot mint a replacement source proof.
pub struct OwnedPreparedPartsDraft {
    unit: Unit,
    facts: PartsDraftFacts,
    expected: PartsDraftExpected,
    journal: JournalPreparation,
    money: PreparedConsumptionMoney,
}
pub struct JournalWrittenPartsDraft<'a, S: PgPartsDraftOwnerSource> {
    owner: &'a S,
    unit: Unit,
    expected: PartsDraftExpected,
    image: JournalImage,
    money: PreparedConsumptionMoney,
}
pub struct RetainedPartsDraft<'a, S: PgPartsDraftOwnerSource> {
    owner: &'a S,
    unit: Unit,
    expected: PartsDraftExpected,
    image: JournalImage,
    money: RetainedConsumptionMoney,
}
/// Owned evidence of only the predetermined Finance and Money after-images.
/// This is not owner authority or evidence of Work/Inventory/purchase completion.
/// The enclosing coordinator must retain and recheck those original sources.
pub struct PartsDraftFinanceReceipt {
    unit: Unit,
    expected: PartsDraftExpected,
    image: JournalImage,
    money: RetainedConsumptionMoney,
}
/// The write clock is sampled only by canonical Journal and Money preparation.
/// The separate current-observation clock never consumes a domain timestamp.
pub async fn prepare_owned_parts_draft<
    'a,
    S: PgPartsDraftOwnerSource,
    C: Clock + Sync,
    G: Clock + Sync,
>(
    c: &mut PgConnection,
    source: &'a S,
    write_clock: &C,
    guard_clock: &G,
) -> Result<PreparedPartsDraft<'a, S>, Error> {
    if source.facts().kind != PartsOwnerKind::Maintenance {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "PARTS_OWNER_SOURCE_UNAVAILABLE",
            "This native parts writer requires the actual Maintenance owner source.",
        )
        .into());
    }
    source.require_before(c, guard_clock.now()).await?;
    source.finance_source().require_current(c).await?;
    let unit = Unit::current(c).await?;
    let facts = source.facts().clone();
    let (draft, key, consumption_id, consumption_at) = input_binding(source, &facts)?.into_parts();
    let actor = &facts.actor;
    let mut authority = BoundDraftAuthority {
        actor: actor.clone(),
        draft: draft.clone(),
    };
    let rules = embedded_currency_rules().map_err(|_| changed())?;
    let collation = FinanceCollation::english().map_err(|_| changed())?;
    let mut preparation =
        PgJournalPreparation::new(c, actor, &mut authority, write_clock, &rules, &collation)
            .map_err(journal_error)?;
    let journal =
        prepare_journal_with_seed(&mut preparation, actor, &draft, &key, JournalStatus::Draft)
            .await
            .map_err(journal_error)?;
    // Preserve B0 quote/scale errors before the later mapping effect guard.
    // The Money proof still checks the mapping at require_apply/persistence.
    let money = prepare_owned_parts_stock_cost_money(
        c,
        source.finance_source(),
        consumption_id.clone(),
        facts.value_minor,
        write_clock,
    )
    .await?;
    let expected = PartsDraftExpected {
        consumption_id,
        journal_id: journal.journal_id().into(),
        journal_number: journal.number().into(),
        currency: draft.currency,
        period_id: draft.period_id,
        value_minor: facts.value_minor,
        consumption_created_at: consumption_at.get(),
        journal_created_at: journal.now().get(),
    };
    source.require_before(c, guard_clock.now()).await?;
    if source.facts() != &facts {
        return Err(changed());
    }
    Ok(PreparedPartsDraft {
        owner: source,
        unit,
        facts,
        expected,
        journal,
        money,
    })
}
impl<'a, S: PgPartsDraftOwnerSource> PreparedPartsDraft<'a, S> {
    pub fn expected(&self) -> &PartsDraftExpected {
        &self.expected
    }
    /// Release the source borrow while moving only the original preparation.
    /// No SQL, authority refresh, new quote or domain Clock read occurs here.
    pub fn into_owned(self) -> OwnedPreparedPartsDraft {
        OwnedPreparedPartsDraft {
            unit: self.unit,
            facts: self.facts,
            expected: self.expected,
            journal: self.journal,
            money: self.money,
        }
    }
    pub async fn persist_journal<C: Clock + Sync>(
        self,
        c: &mut PgConnection,
        clock: &C,
    ) -> Result<JournalWrittenPartsDraft<'a, S>, Error> {
        let owner = self.owner;
        self.into_owned().persist_journal(c, owner, clock).await
    }
}
impl OwnedPreparedPartsDraft {
    pub fn expected(&self) -> &PartsDraftExpected {
        &self.expected
    }
    /// Reborrow the actual current owner source after Workflow has inserted the
    /// known-result receipt. All Journal/quote preparation has already finished;
    /// the original late owner/version/mapping gate still precedes effects.
    pub async fn persist_journal<'a, S: PgPartsDraftOwnerSource, C: Clock + Sync>(
        self,
        c: &mut PgConnection,
        owner: &'a S,
        clock: &C,
    ) -> Result<JournalWrittenPartsDraft<'a, S>, Error> {
        self.unit.require_current(c).await?;
        if owner.facts() != &self.facts {
            return Err(changed());
        }
        owner.require_apply(c, &self.expected, clock.now()).await?;
        if owner.facts() != &self.facts {
            return Err(changed());
        }
        self.money.require_current(c).await?;
        PgJournalStore::new(
            self.journal.actor().holding_id.clone(),
            self.journal.actor().company_id.clone(),
            c,
        )
        .persist(&self.journal)
        .await
        .map_err(journal_error)?;
        let image = JournalImage::expected(&self.journal)?;
        image.require_current(c).await?;
        Ok(JournalWrittenPartsDraft {
            owner,
            unit: self.unit,
            expected: self.expected,
            image,
            money: self.money,
        })
    }
}
impl<'a, S: PgPartsDraftOwnerSource> JournalWrittenPartsDraft<'a, S> {
    pub fn expected(&self) -> &PartsDraftExpected {
        &self.expected
    }
    /// Called after consumption/source+lot evidence and before stock/Work/event.
    pub async fn persist_stock_cost(
        self,
        c: &mut PgConnection,
    ) -> Result<RetainedPartsDraft<'a, S>, Error> {
        self.unit.require_current(c).await?;
        self.image.require_current(c).await?;
        let money = self.money.persist(c).await?;
        self.image.require_current(c).await?;
        Ok(RetainedPartsDraft {
            owner: self.owner,
            unit: self.unit,
            expected: self.expected,
            image: self.image,
            money,
        })
    }
}
impl<S: PgPartsDraftOwnerSource> RetainedPartsDraft<'_, S> {
    pub fn expected(&self) -> &PartsDraftExpected {
        &self.expected
    }
    pub async fn require_after<C: Clock + Sync>(
        &self,
        c: &mut PgConnection,
        clock: &C,
    ) -> Result<(), Error> {
        self.unit.require_current(c).await?;
        self.image.require_current(c).await?;
        self.money.require_current(c).await?;
        self.owner
            .require_after(c, &self.expected, clock.now())
            .await?;
        self.image.require_current(c).await?;
        self.money.require_current(c).await
    }

    /// Check the original owner after-image before releasing its borrow. Move
    /// the original proofs without cloning, querying a new baseline or granting
    /// permission. The owned receipt can survive local owner assembly until the
    /// enclosing Workflow's final Audit/outbox/deferred-constraint checks.
    pub async fn into_finance_receipt<C: Clock + Sync>(
        self,
        c: &mut PgConnection,
        guard_clock: &C,
    ) -> Result<PartsDraftFinanceReceipt, Error> {
        self.require_after(c, guard_clock).await?;
        Ok(PartsDraftFinanceReceipt {
            unit: self.unit,
            expected: self.expected,
            image: self.image,
            money: self.money,
        })
    }
}
impl PartsDraftFinanceReceipt {
    pub fn expected(&self) -> &PartsDraftExpected {
        &self.expected
    }
    /// Recheck the same original unit and Finance/Money images only. Actual
    /// owner/IAM/purchase/stock source guards remain the coordinator's duty.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.unit.require_current(c).await?;
        self.image.require_current(c).await?;
        self.money.require_current(c).await
    }
}

/// Exact closed image of the canonical Finance writes, not a whole book or
/// unrelated names. Additional lines and any rewritten owned column deny.
struct JournalImage {
    journal: Value,
    lines: Vec<Value>,
}
impl JournalImage {
    fn expected(p: &JournalPreparation) -> Result<Self, Error> {
        let d = p.draft();
        let a = p.actor();
        let journal = json!({"id":p.journal_id(),"company_id":a.company_id.as_str(),
            "chart_id":d.chart_id,"period_id":d.period_id,"journal_number":p.number(),
            "entry_date":d.entry_date,"currency":d.currency,"fx_rate_micros":p.fx(),
            "source_type":d.source_type,"source_id":d.source_id,"memo":d.memo,
            "is_opening":0,"status":"draft","created_by":a.user_id.as_str(),
            "posted_by":null,"posted_at":null,"reversed_by":null,"reversed_at":null,"created_at":p.now().get(),
            "updated_at":p.now().get(),"version":1});
        let lines=p.amounts().lines.iter().enumerate().map(|(index,l)| {
            Ok(json!({"id":format!("{}-{index}",p.journal_id()),"journal_id":p.journal_id(),
                "account_id":l.account_id,"description":l.description.as_deref().unwrap_or(""),
                "debit_minor":l.debit,"credit_minor":l.credit,"base_debit_minor":l.base_debit,
                "base_credit_minor":l.base_credit,"dimension_json":serde_json::to_string(&l.dimensions.clone().unwrap_or_default()).map_err(|_|changed())?,
                "ordinal":index}))
        }).collect::<Result<Vec<_>,Error>>()?;
        Ok(Self { journal, lines })
    }
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let id = self.journal["id"].as_str().ok_or_else(changed)?;
        let company = self.journal["company_id"].as_str().ok_or_else(changed)?;
        let journal:Option<Option<String>>=sqlx::query_scalar(
            "SELECT CASE WHEN octet_length(image)<=65536 THEN image END FROM(SELECT jsonb_build_object('id',j.id,'company_id',j.company_id,'chart_id',j.chart_id,'period_id',j.period_id,'journal_number',j.journal_number,'entry_date',j.entry_date,'currency',j.currency,'fx_rate_micros',j.fx_rate_micros,'source_type',j.source_type,'source_id',j.source_id,'memo',j.memo,'is_opening',j.is_opening,'status',j.status,'created_by',j.created_by,'posted_by',j.posted_by,'posted_at',j.posted_at,'reversed_by',j.reversed_by,'reversed_at',j.reversed_at,'created_at',j.created_at,'updated_at',j.updated_at,'version',j.version)::text image FROM finance_journals j WHERE j.id=$1 AND j.company_id=$2 FOR SHARE OF j) bounded"
        ).bind(id).bind(company).fetch_optional(&mut*c).await?;
        let actual = journal
            .flatten()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .ok_or_else(changed)?;
        if actual != self.journal {
            return Err(changed());
        }
        let lines:Vec<Option<String>>=sqlx::query_scalar(
            "SELECT CASE WHEN octet_length(image)<=65536 THEN image END FROM(SELECT jsonb_build_object('id',l.id,'journal_id',l.journal_id,'account_id',l.account_id,'description',l.description,'debit_minor',l.debit_minor,'credit_minor',l.credit_minor,'base_debit_minor',l.base_debit_minor,'base_credit_minor',l.base_credit_minor,'dimension_json',l.dimension_json,'ordinal',l.ordinal)::text image FROM finance_journal_lines l WHERE journal_id=$1 ORDER BY ordinal LIMIT 3 FOR SHARE OF l) bounded"
        ).bind(id).fetch_all(&mut*c).await?;
        if lines.len() != self.lines.len() {
            return Err(changed());
        }
        for (raw, expected) in lines.into_iter().zip(&self.lines) {
            let actual = raw
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                .ok_or_else(changed)?;
            if &actual != expected {
                return Err(changed());
            }
        }
        Ok(())
    }
}
