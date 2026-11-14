//! Finance-owned Money snapshot producer for cross-owner consumers.
//!
//! This is the reusable native equivalent of the retained TypeScript
//! `snapshotMoney` handoff. It resolves the Company currency scale and the
//! effective-date quote, converts the amount to USD, and writes one immutable
//! `money_transaction_snapshots` row inside the caller's serializable
//! transaction. The caller retains its own owner facts, commits all effects
//! together, and may revalidate this receipt with `require_current`.
//!
//! `asset_acquisition_money` keeps its specialized acquisition port; this
//! module is the generic producer for the remaining consumers such as the
//! Ecommerce listing unit price.
use std::cmp::Ordering;
use std::future::Future;
use std::pin::Pin;

use super::postgres_journal_facts::embedded_currency_rules;
use crate::domain::company_money::{FinanceCollation, convert_currency_units};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind};
use futures_util::TryStreamExt;
use num_bigint::BigInt;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
const SOURCE_TYPE_MAX: usize = 100;
const SOURCE_ID_MAX: usize = 200;
const AMOUNT_KIND_MAX: usize = 100;

/// Snapshot transaction identity captured before and after every Finance read.
type Unit = ((String, i32, String, String, String), String, Vec<i64>);

/// The exact `snapshotMoney` request for one immutable source row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoneySnapshotInput {
    pub source_type: String,
    pub source_id: String,
    pub amount_kind: String,
    pub transaction_date: String,
    pub currency: String,
    /// Retained callers may pin the configured scale; `None` uses the Company
    /// scale for ISO currencies without a fixed minor unit.
    pub currency_scale: Option<i64>,
    /// Signed minor units, bounded to the JavaScript safe-integer range.
    pub amount_units: i64,
}

/// One persisted `money_transaction_snapshots` image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoneySnapshotEvidence {
    pub source_type: String,
    pub source_id: String,
    pub amount_kind: String,
    pub transaction_date: String,
    pub currency: String,
    pub currency_scale: i32,
    pub amount_units: String,
    pub usd_units: String,
    pub rate_id: Option<String>,
    pub used_latest_fallback: bool,
    pub captured_at: i64,
}

/// One immutable rate row as read by the effective-date selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoneyRateCandidate {
    pub id: String,
    pub effective_date: String,
    pub currency_per_usd: String,
    pub confirmed_at: i64,
}

/// The rate selected by the retained `resolveRate` ordering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedMoneyRate {
    pub id: String,
    pub effective_date: String,
    pub currency_per_usd: String,
    pub confirmed_at: i64,
    pub used_latest_fallback: bool,
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "MONEY_SNAPSHOT_CHANGED",
        "The Money source changed.",
    )
    .into()
}

const fn invalid_error(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn invalid(code: &'static str, message: &'static str) -> Error {
    invalid_error(code, message).into()
}

fn scale_required() -> BusinessError {
    invalid_error(
        "CURRENCY_SCALE_REQUIRED",
        "Use a configured ISO currency scale.",
    )
}

/// Zod `.trim()` then `.min(1).max(limit)` in UTF-16 units, without silently
/// rewriting the caller's identity.
fn bounded_label(value: &str, limit: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && trimmed.encode_utf16().count() <= limit
}

fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn valid_date(value: &str) -> bool {
    time::Date::parse(
        value,
        &time::macros::format_description!("[year]-[month]-[day]"),
    )
    .is_ok()
}

/// Validate the retained `moneyIdentitySchema` shape plus the native
/// safe-integer amount bound. Scale and quote rules stay in the caller-aware
/// functions below because they depend on Company configuration.
pub fn validate_money_snapshot_input(input: &MoneySnapshotInput) -> Result<(), BusinessError> {
    if !bounded_label(&input.source_type, SOURCE_TYPE_MAX)
        || !bounded_label(&input.source_id, SOURCE_ID_MAX)
        || !bounded_label(&input.amount_kind, AMOUNT_KIND_MAX)
        || !valid_date(&input.transaction_date)
        || !valid_currency(&input.currency)
        || !(-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&input.amount_units)
    {
        return Err(invalid_error(
            "MONEY_SNAPSHOT_INVALID",
            "Provide valid Money snapshot facts.",
        ));
    }
    Ok(())
}

/// Resolve the ISO scale or the Company scale for an ISO currency without a
/// fixed minor unit.
pub fn money_snapshot_scale(
    currency: &str,
    configured_scale: Option<i64>,
) -> Result<u8, BusinessError> {
    embedded_currency_rules()
        .map_err(|_| scale_required())?
        .scale(currency, configured_scale)
        .map_err(|_| scale_required())
}

/// Reject a caller-pinned scale that differs from the configured scale.
pub fn require_money_snapshot_scale(
    requested_scale: Option<i64>,
    configured_scale: u8,
) -> Result<(), BusinessError> {
    match requested_scale {
        Some(requested) if requested != i64::from(configured_scale) => Err(invalid_error(
            "CURRENCY_SCALE_MISMATCH",
            "A transaction must use the configured currency decimal places.",
        )),
        _ => Ok(()),
    }
}

/// Convert signed minor units to USD cents with the retained half-away
/// rounding and numeric bounds.
pub fn money_snapshot_usd(
    amount_units: i64,
    source_scale: u8,
    currency_per_usd: &str,
) -> Result<String, BusinessError> {
    let usd = convert_currency_units(&BigInt::from(amount_units), source_scale, 2, currency_per_usd, "1")
        .map_err(|_| {
            invalid_error(
                "MONEY_OVERFLOW",
                "Money conversion exceeds its exact bounds.",
            )
        })?
        .to_string();
    if usd.len() > 38 {
        return Err(invalid_error(
            "MONEY_OVERFLOW",
            "Money evidence exceeds its numeric bounds.",
        ));
    }
    Ok(usd)
}

/// JavaScript `Number` ordering for `confirmed_at` without losing large values
/// to an integer-only comparison.
fn number_order(left: i64, right: i64) -> Ordering {
    (left as f64)
        .partial_cmp(&(right as f64))
        .unwrap_or(Ordering::Equal)
}

fn dated_order(
    collation: &FinanceCollation,
    left: &MoneyRateCandidate,
    right: &MoneyRateCandidate,
) -> Ordering {
    collation
        .compare(&left.effective_date, &right.effective_date)
        .then_with(|| number_order(left.confirmed_at, right.confirmed_at))
        .then_with(|| collation.compare(&left.id, &right.id))
}

fn fallback_order(
    collation: &FinanceCollation,
    left: &MoneyRateCandidate,
    right: &MoneyRateCandidate,
) -> Ordering {
    number_order(left.confirmed_at, right.confirmed_at)
        .then_with(|| collation.compare(&left.id, &right.id))
        .then_with(|| collation.compare(&left.effective_date, &right.effective_date))
}

/// Streaming accumulator for the retained legacy ordering. It keeps only the
/// two candidates the SQL ordered stream can select, so no new row cap is
/// imposed. Strict `Greater` comparison preserves the first row on a canonical
/// collation tie exactly as the stable JavaScript sort does.
pub struct MoneyRateSelector {
    collation: FinanceCollation,
    dated: Option<MoneyRateCandidate>,
    fallback: Option<MoneyRateCandidate>,
}

impl MoneyRateSelector {
    pub fn new() -> Result<Self, BusinessError> {
        Ok(Self {
            collation: FinanceCollation::english().map_err(|_| {
                invalid_error(
                    "MONEY_SNAPSHOT_CHANGED",
                    "The Money source changed.",
                )
            })?,
            dated: None,
            fallback: None,
        })
    }

    pub fn observe(&mut self, candidate: MoneyRateCandidate, on: &str) {
        let better_dated = candidate.effective_date.as_str() <= on
            && self.dated.as_ref().is_none_or(|old| {
                dated_order(&self.collation, &candidate, old) == Ordering::Greater
            });
        if better_dated {
            self.dated = Some(candidate.clone());
        }
        let better_fallback = self.fallback.as_ref().is_none_or(|old| {
            fallback_order(&self.collation, &candidate, old) == Ordering::Greater
        });
        if better_fallback {
            self.fallback = Some(candidate);
        }
    }

    pub fn select(self, currency: &str, on: &str) -> Option<SelectedMoneyRate> {
        if currency == "USD" {
            return Some(SelectedMoneyRate {
                id: "USD".into(),
                effective_date: on.into(),
                currency_per_usd: "1".into(),
                confirmed_at: 0,
                used_latest_fallback: false,
            });
        }
        if self.dated.is_some() {
            return self.dated.map(|rate| SelectedMoneyRate {
                id: rate.id,
                effective_date: rate.effective_date,
                currency_per_usd: rate.currency_per_usd,
                confirmed_at: rate.confirmed_at,
                used_latest_fallback: false,
            });
        }
        self.fallback.map(|rate| SelectedMoneyRate {
            id: rate.id,
            effective_date: rate.effective_date,
            currency_per_usd: rate.currency_per_usd,
            confirmed_at: rate.confirmed_at,
            used_latest_fallback: true,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Quote {
    scale: Option<i64>,
    selected: Option<SelectedMoneyRate>,
}

/// Read the Company scale and the effective-date quote under the retained
/// share locks. The SQL order is the legacy `companyRates` order; the
/// accumulator applies the stable JavaScript comparator.
async fn quote(
    c: &mut PgConnection,
    company: &CompanyId,
    currency: &str,
    date: &str,
) -> Result<Quote, Error> {
    let scale: Option<i64> = sqlx::query_scalar(
        "SELECT scale::bigint FROM public.company_currency_scales \
         WHERE company_id=$1 AND currency=$2 FOR SHARE",
    )
    .bind(company.as_str())
    .bind(currency)
    .fetch_optional(&mut *c)
    .await?;
    let mut selector = MoneyRateSelector::new()?;
    let mut stream = sqlx::query(
        "SELECT id,effective_date::text effective_date,currency_per_usd::text quote,\
         confirmed_at::bigint confirmed_at FROM public.company_fx_rates \
         WHERE company_id=$1 AND currency=$2 \
         ORDER BY effective_date DESC,confirmed_at DESC,id DESC FOR SHARE",
    )
    .bind(company.as_str())
    .bind(currency)
    .fetch(c);
    while let Some(row) = stream.try_next().await? {
        selector.observe(
            MoneyRateCandidate {
                id: row.try_get("id")?,
                effective_date: row.try_get("effective_date")?,
                currency_per_usd: row.try_get("quote")?,
                confirmed_at: row.try_get("confirmed_at")?,
            },
            date,
        );
    }
    Ok(Quote {
        scale,
        selected: selector.select(currency, date),
    })
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
    let (held, visible, path): (bool, bool, String) = sqlx::query_as(
        "SELECT (SELECT count(*)=3 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND CASE WHEN c.oid='public.money_transaction_snapshots'::regclass THEN l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') ELSE l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') END)) FROM pg_class c WHERE c.oid IN ('public.company_currency_scales'::regclass,'public.company_fx_rates'::regclass,'public.money_transaction_snapshots'::regclass)), NOT (row_security_active('public.company_currency_scales'::regclass) OR row_security_active('public.company_fx_rates'::regclass) OR row_security_active('public.money_transaction_snapshots'::regclass)),current_setting('search_path')",
    )
    .fetch_one(&mut *c)
    .await?;
    if !held {
        return Err(PersistenceError::new("MONEY_SNAPSHOT_LOCKS_REQUIRED").into());
    }
    if !visible {
        return Err(PersistenceError::new("MONEY_SNAPSHOT_RLS_UNSUPPORTED").into());
    }
    let oids: Vec<i64> = sqlx::query_scalar(
        "SELECT unnest(ARRAY['public.company_currency_scales'::regclass::oid::bigint,\
         'public.company_fx_rates'::regclass::oid::bigint,\
         'public.money_transaction_snapshots'::regclass::oid::bigint])",
    )
    .fetch_all(&mut *c)
    .await?;
    let transaction = sqlx::query_as(
        "SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,\
         pg_postmaster_start_time()::text,current_user::text",
    )
    .fetch_one(c)
    .await?;
    Ok((transaction, path, oids))
}

/// Acquire the Finance utility locks in the retained order and modes. The
/// coordinator acquires every other owner fence before its first snapshot and
/// retries the whole unit on contention.
pub async fn lock_money_snapshot_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_OWNER_TRANSACTION_REQUIRED").into());
    }
    for sql in [
        "LOCK TABLE public.company_currency_scales IN SHARE MODE NOWAIT",
        "LOCK TABLE public.company_fx_rates IN SHARE MODE NOWAIT",
        "LOCK TABLE public.money_transaction_snapshots IN SHARE ROW EXCLUSIVE MODE NOWAIT",
    ] {
        sqlx::query(sql)
            .execute(&mut *c)
            .await
            .map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Image {
    evidence: MoneySnapshotEvidence,
    digest: String,
}

/// Read this exact source row and its immutable whole-row digest. Any
/// malformed row is a source failure rather than evidence.
async fn image(
    c: &mut PgConnection,
    company: &CompanyId,
    source_type: &str,
    source_id: &str,
    amount_kind: &str,
) -> Result<Option<Image>, Error> {
    let rows = sqlx::query(
        "SELECT CASE WHEN b.bounded THEN m.transaction_date::text END transaction_date,\
         CASE WHEN b.bounded THEN m.currency END currency,m.currency_scale::int currency_scale,\
         CASE WHEN b.bounded THEN m.amount_units::text END amount_units,\
         CASE WHEN b.bounded THEN m.usd_units::text END usd_units,\
         CASE WHEN b.bounded THEN m.fx_rate_id END fx_rate_id,\
         CASE WHEN b.bounded THEN m.used_latest_fallback END used_latest_fallback,\
         m.captured_at,b.bounded,(m.currency_scale BETWEEN 0 AND 12 AND \
         m.captured_at BETWEEN 0 AND 9007199254740991 AND \
         m.original_source_type IS NULL AND m.original_source_id IS NULL AND \
         m.original_amount_kind IS NULL AND m.carried_source_type IS NULL AND \
         m.carried_source_id IS NULL AND m.carried_amount_kind IS NULL AND \
         m.carried_allocation_offset_units IS NULL) ordinary,\
         encode(sha256(convert_to(to_jsonb(m)::text,'UTF8')),'hex') digest \
         FROM public.money_transaction_snapshots m CROSS JOIN LATERAL \
         (SELECT coalesce(octet_length(m.fx_rate_id),0)+octet_length(m.currency)+\
         octet_length(m.amount_units::text)+octet_length(m.usd_units::text)<=65536 bounded) b \
         WHERE m.company_id=$1 AND m.source_type=$2 AND m.source_id=$3 AND m.amount_kind=$4 \
         LIMIT 2 FOR SHARE OF m",
    )
    .bind(company.as_str())
    .bind(source_type)
    .bind(source_id)
    .bind(amount_kind)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > 1 {
        return Err(changed());
    }
    let Some(row) = rows.into_iter().next() else {
        return Ok(None);
    };
    if !row.try_get::<bool, _>("bounded")? || !row.try_get::<bool, _>("ordinary")? {
        return Err(changed());
    }
    let text = |key: &str| -> Result<String, Error> {
        row.try_get::<Option<String>, _>(key)?.ok_or_else(changed)
    };
    Ok(Some(Image {
        evidence: MoneySnapshotEvidence {
            source_type: source_type.into(),
            source_id: source_id.into(),
            amount_kind: amount_kind.into(),
            transaction_date: text("transaction_date")?,
            currency: text("currency")?,
            currency_scale: row.try_get("currency_scale")?,
            amount_units: text("amount_units")?,
            usd_units: text("usd_units")?,
            rate_id: row.try_get("fx_rate_id")?,
            used_latest_fallback: row
                .try_get::<Option<bool>, _>("used_latest_fallback")?
                .ok_or_else(changed)?,
            captured_at: row.try_get("captured_at")?,
        },
        digest: row.try_get("digest")?,
    }))
}

/// A prepared snapshot is bound to the current serializable unit, quote and an
/// absent source row. The caller owns rollback and all later owner effects.
pub struct PreparedMoneySnapshot {
    unit: Unit,
    company: CompanyId,
    input: MoneySnapshotInput,
    quote: Quote,
    evidence: MoneySnapshotEvidence,
}

impl PreparedMoneySnapshot {
    pub fn input(&self) -> &MoneySnapshotInput {
        &self.input
    }
    pub fn evidence(&self) -> &MoneySnapshotEvidence {
        &self.evidence
    }
    pub async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(&mut *c).await? != self.unit
            || quote(
                &mut *c,
                &self.company,
                &self.input.currency,
                &self.input.transaction_date,
            )
            .await?
                != self.quote
            || image(
                &mut *c,
                &self.company,
                &self.input.source_type,
                &self.input.source_id,
                &self.input.amount_kind,
            )
            .await?
            .is_some()
            || unit(c).await? != self.unit
        {
            return Err(changed());
        }
        Ok(())
    }
    /// Insert the exact retained row and return its whole-row image.
    pub async fn persist(
        self,
        c: &mut PgConnection,
    ) -> Result<RetainedMoneySnapshot, Error> {
        self.require_before(&mut *c).await?;
        let expected: String = sqlx::query_scalar(
            "SELECT encode(sha256(convert_to(jsonb_build_object(\
             'company_id',$1::text,'source_type',$2::text,'source_id',$3::text,\
             'amount_kind',$4::text,'transaction_date',$5::date,'currency',$6::text,\
             'currency_scale',$7::integer,'amount_units',$8::numeric,'usd_units',$9::numeric,\
             'fx_rate_id',$10::text,'used_latest_fallback',$11::boolean,'captured_at',$12::bigint,\
             'original_source_type',NULL,'original_source_id',NULL,'original_amount_kind',NULL,\
             'carried_source_type',NULL,'carried_source_id',NULL,'carried_amount_kind',NULL,\
             'carried_allocation_offset_units',NULL)::text,'UTF8')),'hex')",
        )
        .bind(self.company.as_str())
        .bind(&self.input.source_type)
        .bind(&self.input.source_id)
        .bind(&self.input.amount_kind)
        .bind(&self.evidence.transaction_date)
        .bind(&self.evidence.currency)
        .bind(self.evidence.currency_scale)
        .bind(&self.evidence.amount_units)
        .bind(&self.evidence.usd_units)
        .bind(&self.evidence.rate_id)
        .bind(self.evidence.used_latest_fallback)
        .bind(self.evidence.captured_at)
        .fetch_one(&mut *c)
        .await?;
        sqlx::query(
            "INSERT INTO public.money_transaction_snapshots\
             (company_id,source_type,source_id,amount_kind,transaction_date,currency,\
             currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at) \
             VALUES($1,$2,$3,$4,$5::date,$6,$7,$8::numeric,$9::numeric,$10,$11,$12)",
        )
        .bind(self.company.as_str())
        .bind(&self.input.source_type)
        .bind(&self.input.source_id)
        .bind(&self.input.amount_kind)
        .bind(&self.evidence.transaction_date)
        .bind(&self.evidence.currency)
        .bind(self.evidence.currency_scale)
        .bind(&self.evidence.amount_units)
        .bind(&self.evidence.usd_units)
        .bind(&self.evidence.rate_id)
        .bind(self.evidence.used_latest_fallback)
        .bind(self.evidence.captured_at)
        .execute(&mut *c)
        .await?;
        let captured = image(
            &mut *c,
            &self.company,
            &self.input.source_type,
            &self.input.source_id,
            &self.input.amount_kind,
        )
        .await?
        .ok_or_else(changed)?;
        if captured.evidence != self.evidence || captured.digest != expected {
            return Err(changed());
        }
        let retained = RetainedMoneySnapshot {
            plan: self,
            image: captured,
        };
        retained.require_current(c).await?;
        Ok(retained)
    }
}

/// Owner-retained Money effect. Keep it until every later write and deferred
/// effect has run, then call `require_current` in the same unit.
pub struct RetainedMoneySnapshot {
    plan: PreparedMoneySnapshot,
    image: Image,
}

impl RetainedMoneySnapshot {
    pub fn input(&self) -> &MoneySnapshotInput {
        &self.plan.input
    }
    pub fn evidence(&self) -> &MoneySnapshotEvidence {
        &self.plan.evidence
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(&mut *c).await? != self.plan.unit
            || quote(
                &mut *c,
                &self.plan.company,
                &self.plan.input.currency,
                &self.plan.input.transaction_date,
            )
            .await?
                != self.plan.quote
            || image(
                &mut *c,
                &self.plan.company,
                &self.plan.input.source_type,
                &self.plan.input.source_id,
                &self.plan.input.amount_kind,
            )
            .await?
                .as_ref()
                != Some(&self.image)
            || unit(c).await? != self.plan.unit
        {
            return Err(changed());
        }
        Ok(())
    }
}

/// Prepare `snapshotMoney`'s exact scale, effective-date quote and USD
/// conversion for one source row. This does not write the snapshot; `persist`
/// records it after all caller preconditions.
pub async fn prepare_money_snapshot(
    c: &mut PgConnection,
    company: &CompanyId,
    input: MoneySnapshotInput,
    clock: &impl Clock,
) -> Result<PreparedMoneySnapshot, Error> {
    let transaction = unit(&mut *c).await?;
    validate_money_snapshot_input(&input)?;
    // `snapshotMoney` validates the configured scale before reading rates.
    let configured_scale: Option<i64> = sqlx::query_scalar(
        "SELECT scale::bigint FROM public.company_currency_scales \
         WHERE company_id=$1 AND currency=$2 FOR SHARE",
    )
    .bind(company.as_str())
    .bind(&input.currency)
    .fetch_optional(&mut *c)
    .await?;
    let scale = money_snapshot_scale(&input.currency, configured_scale)?;
    require_money_snapshot_scale(input.currency_scale, scale)?;
    let quote = quote(
        &mut *c,
        company,
        &input.currency,
        &input.transaction_date,
    )
    .await?;
    if quote.scale != configured_scale {
        return Err(changed());
    }
    let rate = quote.selected.as_ref().ok_or_else(|| {
        invalid(
            "MANUAL_RATE_REQUIRED",
            "A confirmed manual currency/USD rate is required.",
        )
    })?;
    let usd_units = money_snapshot_usd(input.amount_units, scale, &rate.currency_per_usd)?;
    let captured_at = clock.now().get();
    if !(0..=MAX_SAFE_INTEGER).contains(&captured_at) {
        return Err(changed());
    }
    let evidence = MoneySnapshotEvidence {
        source_type: input.source_type.clone(),
        source_id: input.source_id.clone(),
        amount_kind: input.amount_kind.clone(),
        transaction_date: input.transaction_date.clone(),
        currency: input.currency.clone(),
        currency_scale: i32::from(scale),
        amount_units: input.amount_units.to_string(),
        usd_units,
        rate_id: (rate.id != "USD").then(|| rate.id.clone()),
        used_latest_fallback: rate.used_latest_fallback,
        captured_at,
    };
    let prepared = PreparedMoneySnapshot {
        unit: transaction,
        company: company.clone(),
        input,
        quote,
        evidence,
    };
    prepared.require_before(c).await?;
    Ok(prepared)
}

/// Cross-owner port for a composition that owns the caller's serializable
/// connection. The returned preparation is still persisted by the caller so
/// its own prerequisites and locks can run first.
pub trait PgMoneySnapshotProducer: Send + Sync {
    fn prepare<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        company: &'a CompanyId,
        input: &'a MoneySnapshotInput,
    ) -> Pin<Box<dyn Future<Output = Result<PreparedMoneySnapshot, Error>> + Send + 'a>>;
}

/// Default producer bound to a caller-owned clock.
pub struct PgMoneySnapshot<C> {
    clock: C,
}

impl<C> PgMoneySnapshot<C> {
    pub const fn new(clock: C) -> Self {
        Self { clock }
    }
}

impl<C: Clock + Send + Sync + 'static> PgMoneySnapshotProducer for PgMoneySnapshot<C> {
    fn prepare<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        company: &'a CompanyId,
        input: &'a MoneySnapshotInput,
    ) -> Pin<Box<dyn Future<Output = Result<PreparedMoneySnapshot, Error>> + Send + 'a>> {
        let input = input.clone();
        Box::pin(async move {
            prepare_money_snapshot(connection, company, input, &self.clock).await
        })
    }
}
