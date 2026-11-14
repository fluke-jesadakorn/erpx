//! Finance-owned Money snapshot for an operational Asset acquisition.
//!
//! This is only the Finance handoff. The caller retains the Asset, selected
//! Identity, Commercial and Workflow facts and commits all effects together.
use super::postgres_journal_facts::embedded_currency_rules;
use crate::domain::company_money::{FinanceCollation, convert_currency_units};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind};
use futures_util::TryStreamExt;
use num_bigint::BigInt;
use sqlx::{Connection, PgConnection, Row};
use std::cmp::Ordering;

type Error = PgTransactionError<BusinessError>;
type Unit = ((String, i32, String, String, String), String, Vec<i64>);
fn changed() -> Error { BusinessError::new(FailureKind::Conflict, "ASSET_ACQUISITION_MONEY_CHANGED", "The Asset acquisition Money source changed.").into() }

/// Finance utility-only locks. The coordinator must acquire every other owner
/// fence before its first snapshot, and retry the whole unit on contention.
pub async fn lock_asset_acquisition_money_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() { return Err(PersistenceError::new("FINANCE_OWNER_TRANSACTION_REQUIRED").into()); }
    for sql in ["LOCK TABLE public.company_currency_scales IN SHARE MODE NOWAIT", "LOCK TABLE public.company_fx_rates IN SHARE MODE NOWAIT", "LOCK TABLE public.money_transaction_snapshots IN SHARE ROW EXCLUSIVE MODE NOWAIT"] {
        sqlx::query(sql).execute(&mut *c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    }
    Ok(())
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_REQUIRED").into());
    }
    let (held, visible, path):(bool,bool,String)=sqlx::query_as("SELECT (SELECT count(*)=3 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND CASE WHEN c.oid='public.money_transaction_snapshots'::regclass THEN l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') ELSE l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') END)) FROM pg_class c WHERE c.oid IN ('public.company_currency_scales'::regclass,'public.company_fx_rates'::regclass,'public.money_transaction_snapshots'::regclass)), NOT (row_security_active('public.company_currency_scales'::regclass) OR row_security_active('public.company_fx_rates'::regclass) OR row_security_active('public.money_transaction_snapshots'::regclass)),current_setting('search_path')").fetch_one(&mut *c).await?;
    if !held { return Err(PersistenceError::new("ASSET_MONEY_LOCKS_REQUIRED").into()); }
    if !visible { return Err(PersistenceError::new("ASSET_MONEY_RLS_UNSUPPORTED").into()); }
    let oids:Vec<i64>=sqlx::query_scalar("SELECT unnest(ARRAY['public.company_currency_scales'::regclass::oid::bigint,'public.company_fx_rates'::regclass::oid::bigint,'public.money_transaction_snapshots'::regclass::oid::bigint])").fetch_all(&mut *c).await?;
    let transaction=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;
    Ok((transaction,path,oids))
}

async fn money_image(c:&mut PgConnection, company:&CompanyId, id:&str)->Result<Option<String>,Error> {
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(m)::text,'UTF8')),'hex') FROM public.money_transaction_snapshots m WHERE company_id=$1 AND source_type='operational_asset' AND source_id=$2 AND amount_kind='acquisition_cost' FOR SHARE OF m").bind(company.as_str()).bind(id).fetch_optional(c).await?)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SelectedRate { id:String, date:String, quote:String, confirmed_at:i64, fallback:bool }
#[derive(Clone, Debug, Eq, PartialEq)]
struct AssetQuote { scale:Option<i64>, selected:Option<SelectedRate> }

// The legacy SQL orders all rates by date/confirmation/id before the stable
// JavaScript localeCompare sorts. Preserve that input order when ICU compares
// distinct IDs equally, and compare confirmed_at as JS Number does. Streaming
// retains only two candidates rather than imposing a new row-count cap.
async fn asset_quote(c:&mut PgConnection,company:&CompanyId,currency:&str,date:&str)->Result<AssetQuote,Error> {
    let scale:Option<i64>=sqlx::query_scalar("SELECT scale::bigint FROM public.company_currency_scales WHERE company_id=$1 AND currency=$2 FOR SHARE").bind(company.as_str()).bind(currency).fetch_optional(&mut *c).await?;
    let coll=FinanceCollation::english().map_err(|_|changed())?;
    let mut stream=sqlx::query("SELECT id,effective_date::text effective_date,currency_per_usd::text quote,confirmed_at::bigint confirmed_at FROM public.company_fx_rates WHERE company_id=$1 AND currency=$2 ORDER BY effective_date DESC,confirmed_at DESC,id DESC FOR SHARE").bind(company.as_str()).bind(currency).fetch(c);
    let mut dated:Option<SelectedRate>=None;let mut fallback:Option<SelectedRate>=None;
    while let Some(row)=stream.try_next().await? {
        let candidate=SelectedRate{id:row.try_get("id")?,date:row.try_get("effective_date")?,quote:row.try_get("quote")?,confirmed_at:row.try_get("confirmed_at")?,fallback:false};
        let number_order=|a:i64,b:i64| (a as f64).partial_cmp(&(b as f64)).unwrap_or(Ordering::Equal);
        let dated_order=|a:&SelectedRate,b:&SelectedRate| coll.compare(&a.date,&b.date).then_with(||number_order(a.confirmed_at,b.confirmed_at)).then_with(||coll.compare(&a.id,&b.id));
        let fallback_order=|a:&SelectedRate,b:&SelectedRate| number_order(a.confirmed_at,b.confirmed_at).then_with(||coll.compare(&a.id,&b.id)).then_with(||coll.compare(&a.date,&b.date));
        if candidate.date.as_str()<=date && dated.as_ref().is_none_or(|old|dated_order(&candidate,old)==Ordering::Greater) { dated=Some(candidate.clone()); }
        if fallback.as_ref().is_none_or(|old|fallback_order(&candidate,old)==Ordering::Greater) { fallback=Some(candidate); }
    }
    let selected=if currency=="USD" {Some(SelectedRate{id:"USD".into(),date:date.into(),quote:"1".into(),confirmed_at:0,fallback:false})} else if dated.is_some(){dated} else {fallback.map(|mut rate|{rate.fallback=true;rate})};
    Ok(AssetQuote{scale,selected})
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetAcquisitionMoneyEvidence {
    pub transaction_date:String, pub currency:String, pub currency_scale:i32,
    pub amount_units:String, pub usd_units:String, pub rate_id:Option<String>,
    pub used_latest_fallback:bool, pub captured_at:i64,
}
pub struct PreparedAssetAcquisitionMoney { unit:Unit, company:CompanyId, id:String, quote:AssetQuote, evidence:AssetAcquisitionMoneyEvidence }
impl PreparedAssetAcquisitionMoney {
    pub fn evidence(&self)->&AssetAcquisitionMoneyEvidence { &self.evidence }
    pub async fn require_before(&self,c:&mut PgConnection)->Result<(),Error> {
        if unit(&mut *c).await?!=self.unit || asset_quote(&mut *c,&self.company,&self.evidence.currency,&self.evidence.transaction_date).await?!=self.quote || money_image(&mut *c,&self.company,&self.id).await?.is_some() || unit(c).await?!=self.unit { return Err(changed()); } Ok(())
    }
    pub async fn persist(self,c:&mut PgConnection)->Result<RetainedAssetAcquisitionMoney,Error> {
        self.require_before(&mut *c).await?;
        let e=&self.evidence;
        let expected:String=sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('company_id',$1::text,'source_type','operational_asset','source_id',$2::text,'amount_kind','acquisition_cost','transaction_date',$3::date,'currency',$4::text,'currency_scale',$5::integer,'amount_units',$6::numeric,'usd_units',$7::numeric,'fx_rate_id',$8::text,'used_latest_fallback',$9::boolean,'captured_at',$10::bigint,'original_source_type',NULL,'original_source_id',NULL,'original_amount_kind',NULL,'carried_source_type',NULL,'carried_source_id',NULL,'carried_amount_kind',NULL,'carried_allocation_offset_units',NULL)::text,'UTF8')),'hex')").bind(self.company.as_str()).bind(&self.id).bind(&e.transaction_date).bind(&e.currency).bind(e.currency_scale).bind(&e.amount_units).bind(&e.usd_units).bind(&e.rate_id).bind(e.used_latest_fallback).bind(e.captured_at).fetch_one(&mut *c).await?;
        sqlx::query("INSERT INTO public.money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at) VALUES($1,'operational_asset',$2,'acquisition_cost',$3::date,$4,$5,$6::numeric,$7::numeric,$8,$9,$10)").bind(self.company.as_str()).bind(&self.id).bind(&e.transaction_date).bind(&e.currency).bind(e.currency_scale).bind(&e.amount_units).bind(&e.usd_units).bind(&e.rate_id).bind(e.used_latest_fallback).bind(e.captured_at).execute(&mut *c).await?;
        if money_image(&mut *c,&self.company,&self.id).await?.as_deref()!=Some(expected.as_str()) { return Err(changed()); }
        let retained=RetainedAssetAcquisitionMoney{plan:self,image:expected};retained.require_current(c).await?;Ok(retained)
    }
}
pub struct RetainedAssetAcquisitionMoney { plan:PreparedAssetAcquisitionMoney,image:String }
impl RetainedAssetAcquisitionMoney {
    pub fn evidence(&self)->&AssetAcquisitionMoneyEvidence { &self.plan.evidence }
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error> {
        if unit(&mut *c).await?!=self.plan.unit || asset_quote(&mut *c,&self.plan.company,&self.plan.evidence.currency,&self.plan.evidence.transaction_date).await?!=self.plan.quote || money_image(&mut *c,&self.plan.company,&self.plan.id).await?.as_deref()!=Some(self.image.as_str()) || unit(c).await?!=self.plan.unit { return Err(changed()); } Ok(())
    }
}

/// Prepare snapshotMoney's exact currency scale, acquired-date quote and USD
/// conversion, including zero amounts. This does not itself write an Asset.
pub async fn prepare_asset_acquisition_money(c:&mut PgConnection,company:&CompanyId,id:&str,date:&str,currency:&str,amount:i64,clock:&impl Clock)->Result<PreparedAssetAcquisitionMoney,Error> {
    let transaction=unit(&mut *c).await?;
    if !id.starts_with("asset-") || id.encode_utf16().count()>200 || !(0..=9_007_199_254_740_991).contains(&amount) || currency.len()!=3 || !currency.bytes().all(|b|b.is_ascii_uppercase()) || time::Date::parse(date,&time::macros::format_description!("[year]-[month]-[day]")).is_err() { return Err(BusinessError::invalid("ASSET_ACQUISITION_MONEY_INVALID","Provide valid Asset acquisition Money facts.").into()); }
    // snapshotMoney validates the configured scale before reading rates.
    let configured_scale:Option<i64>=sqlx::query_scalar("SELECT scale::bigint FROM public.company_currency_scales WHERE company_id=$1 AND currency=$2 FOR SHARE").bind(company.as_str()).bind(currency).fetch_optional(&mut *c).await?;
    let rules=embedded_currency_rules().map_err(|_|BusinessError::invalid("CURRENCY_SCALE_REQUIRED","Use a configured ISO currency scale."))?;
    let scale=rules.scale(currency,configured_scale).map_err(|_|BusinessError::invalid("CURRENCY_SCALE_REQUIRED","Use a configured ISO currency scale."))?;
    let quote=asset_quote(&mut *c,company,currency,date).await?;
    if quote.scale!=configured_scale{return Err(changed());}
    let rate=quote.selected.as_ref().ok_or_else(||BusinessError::invalid("MANUAL_RATE_REQUIRED","A confirmed manual currency/USD rate is required."))?;
    let usd=convert_currency_units(&BigInt::from(amount),scale,2,&rate.quote,"1").map_err(|_|BusinessError::invalid("MONEY_OVERFLOW","Money conversion exceeds its exact bounds."))?.to_string();
    if usd.len()>38 {return Err(BusinessError::invalid("MONEY_OVERFLOW","Money evidence exceeds its numeric bounds.").into());}
    let captured_at=clock.now().get();if !(0..=9_007_199_254_740_991).contains(&captured_at){return Err(changed());}
    let evidence=AssetAcquisitionMoneyEvidence{transaction_date:date.into(),currency:currency.into(),currency_scale:i32::from(scale),amount_units:amount.to_string(),usd_units:usd,rate_id:(rate.id!="USD").then(||rate.id.clone()),used_latest_fallback:rate.fallback,captured_at};
    let prepared=PreparedAssetAcquisitionMoney{unit:transaction,company:company.clone(),id:id.into(),quote,evidence};prepared.require_before(c).await?;Ok(prepared)
}
