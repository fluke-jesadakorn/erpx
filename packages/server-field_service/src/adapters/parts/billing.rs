//! Original completed-visit pricing and immutable billing-release witness.
use super::source::*;
use erpx_server_shared::public::{CommandKey, TimestampMillis};
use serde::Serialize;
use sqlx::{PgConnection, Row};
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceBillingRelease {
    pub id: String,
    pub order_id: String,
    pub agreement_id: String,
    pub customer_id: String,
    pub currency: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub amount_minor: i64,
    pub warranty_applied: bool,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub warranty_discount_bps: i64,
    pub warranty_end_snapshot: Option<String>,
    pub pricing_date: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub source_order_version: i64,
    pub released_by: String,
    pub reason: String,
}
#[derive(Clone, PartialEq, Eq)]
struct Image {
    agreement_status: String,
    rate: i64,
    discount: i64,
    currency: String,
    completion_evidence: String,
    completed_at: Option<i64>,
    completion_actors: Vec<String>,
    digest: String,
}
pub struct CurrentFieldBilling {
    unit: Unit,
    order_id: String,
    company_id: String,
    before: Image,
    release: FieldServiceBillingRelease,
    due_date: String,
    at: i64,
    expected_release: String,
}
impl CurrentFieldBilling {
    pub async fn prepare<R: PgFieldServiceOrderReferences>(
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        key: &CommandKey,
        expected_version: i64,
        input: (&str, Option<String>),
        at: TimestampMillis,
        now: TimestampMillis,
    ) -> Result<Self, Error> {
        let (reason, warranty_end) = input;
        source
            .require_visit_action(c, crate::domain::visit::VisitAction::ReleaseBilling, now)
            .await?;
        if source.facts().version != expected_version {
            return Err(changed());
        }
        let unit = unit(c).await?;
        let order_id = source.facts().id.as_str().to_owned();
        let company_id = source.facts().company_id.as_str().to_owned();
        let before = image(c, &order_id, &company_id)
            .await?
            .ok_or_else(changed)?;
        if before.completion_actors.is_empty() {
            return Err(changed());
        }
        let dates:(String,String)=sqlx::query_as("SELECT to_char(to_timestamp($1::double precision/1000) AT TIME ZONE 'UTC','YYYY-MM-DD'),to_char(to_timestamp(($1::double precision+2592000000)/1000) AT TIME ZONE 'UTC','YYYY-MM-DD')").bind(at.get()).fetch_one(&mut *c).await?;
        let discount = before.discount.clamp(0, 10000);
        let applied = warranty_end.as_ref().is_some_and(|v| v >= &dates.0);
        // Retained JS Number rounds the multiplication before Math.floor.
        let amount = if applied {
            ((before.rate as f64 * (10000 - discount) as f64) / 10000.0).floor() as i64
        } else {
            before.rate
        };
        let version = expected_version
            .checked_add(1)
            .filter(|v| *v <= 9_007_199_254_740_991)
            .ok_or_else(changed)?;
        let release = FieldServiceBillingRelease {
            id: format!("field-billing-{}", key.as_str()),
            order_id: order_id.clone(),
            agreement_id: source.facts().agreement_id.clone(),
            customer_id: source.facts().customer_id.clone(),
            currency: before.currency.clone(),
            amount_minor: amount,
            warranty_applied: applied,
            warranty_discount_bps: discount,
            warranty_end_snapshot: warranty_end,
            pricing_date: dates.0,
            source_order_version: version,
            released_by: source.actor_user_id().as_str().into(),
            reason: reason.into(),
        };
        let expected_release:String=sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('id',$1::text,'order_id',$2::text,'agreement_id',$3::text,'customer_id',$4::text,'currency',$5::text,'amount_minor',$6::bigint,'source_order_version',$7::bigint,'status','released','released_by',$8::text,'reason',$9::text,'created_at',$10::bigint,'warranty_applied',$11::boolean,'warranty_discount_bps',$12::int,'warranty_end_snapshot',$13::text,'pricing_date',$14::text)::text,'UTF8')),'hex')").bind(&release.id).bind(&release.order_id).bind(&release.agreement_id).bind(&release.customer_id).bind(&release.currency).bind(amount).bind(version).bind(&release.released_by).bind(&release.reason).bind(at.get()).bind(applied).bind(discount).bind(&release.warranty_end_snapshot).bind(&release.pricing_date).fetch_one(&mut *c).await?;
        let out = Self {
            unit,
            order_id,
            company_id,
            before,
            release,
            due_date: dates.1,
            at: at.get(),
            expected_release,
        };
        out.require_before(c, source, now).await?;
        Ok(out)
    }
    pub fn release(&self) -> &FieldServiceBillingRelease {
        &self.release
    }
    pub fn due_date(&self) -> &str {
        &self.due_date
    }
    pub fn completion_actors(&self) -> &[String] {
        &self.before.completion_actors
    }
    pub fn completion_evidence(&self) -> &str {
        &self.before.completion_evidence
    }
    pub fn completed_at(&self) -> Option<i64> {
        self.before.completed_at
    }
    pub async fn require_before<R: PgFieldServiceOrderReferences>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || source.facts().id.as_str() != self.order_id
            || source.facts().company_id.as_str() != self.company_id
            || image(c, &self.order_id, &self.company_id).await?.as_ref() != Some(&self.before)
        {
            return Err(changed());
        }
        source
            .require_visit_action(c, crate::domain::visit::VisitAction::ReleaseBilling, now)
            .await?;
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM field_service_billing_releases WHERE id=$1 OR order_id=$2)").bind(&self.release.id).bind(&self.order_id).fetch_one(c).await?;
        if exists {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn apply<R: PgFieldServiceOrderReferences>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.require_before(c, source, now).await?;
        if self.before.agreement_status != "active" {
            return Err(changed());
        }
        sqlx::query("INSERT INTO field_service_billing_releases(id,order_id,agreement_id,customer_id,currency,amount_minor,warranty_applied,warranty_discount_bps,warranty_end_snapshot,pricing_date,source_order_version,released_by,reason,created_at)VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)").bind(&self.release.id).bind(&self.release.order_id).bind(&self.release.agreement_id).bind(&self.release.customer_id).bind(&self.release.currency).bind(self.release.amount_minor).bind(self.release.warranty_applied).bind(self.release.warranty_discount_bps).bind(&self.release.warranty_end_snapshot).bind(&self.release.pricing_date).bind(self.release.source_order_version).bind(&self.release.released_by).bind(&self.release.reason).bind(self.at).execute(c).await?;
        Ok(())
    }
    pub async fn require_after<R: PgFieldServiceOrderReferences>(
        &self,
        c: &mut PgConnection,
        source: &CurrentFieldServiceOrder<'_, R>,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        source
            .require_original_identity_and_references(c, now)
            .await?;
        let current = image(c, &self.order_id, &self.company_id)
            .await?
            .ok_or_else(changed)?;
        if current != self.before {
            return Err(changed());
        }
        let digest:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex') FROM field_service_billing_releases r WHERE id=$1 AND order_id=$2").bind(&self.release.id).bind(&self.order_id).fetch_optional(c).await?;
        if digest.as_ref() != Some(&self.expected_release) {
            return Err(changed());
        }
        Ok(())
    }
}
async fn image(c: &mut PgConnection, order: &str, company: &str) -> Result<Option<Image>, Error> {
    let row=sqlx::query("SELECT CASE WHEN b.bounded THEN g.status END agreement_status,g.visit_rate_minor::bigint rate,g.warranty_discount_bps::bigint discount,CASE WHEN b.bounded THEN g.currency END currency,CASE WHEN b.bounded THEN o.completion_evidence END evidence,o.visit_completed_at::bigint completed_at,CASE WHEN b.bounded THEN actors.ids END actors,b.bounded,encode(sha256(convert_to(jsonb_build_array(to_jsonb(g),actors.images)::text,'UTF8')),'hex') digest FROM field_service_orders o JOIN field_service_agreements g ON g.id=o.agreement_id AND g.company_id=o.company_id CROSS JOIN LATERAL (SELECT coalesce(jsonb_agg(e.actor_id ORDER BY e.version DESC),'[]'::jsonb) ids,coalesce(jsonb_agg(to_jsonb(e) ORDER BY e.version DESC),'[]'::jsonb) images,coalesce(sum(octet_length(e.actor_id)::bigint+16),0) bytes FROM field_service_order_events e WHERE e.order_id=o.id AND e.action='complete') actors CROSS JOIN LATERAL (SELECT octet_length(g.status)::bigint+octet_length(g.currency)::bigint+coalesce(octet_length(o.completion_evidence),0)::bigint+actors.bytes<=2097152 bounded) b WHERE o.id=$1 AND o.company_id=$2 FOR SHARE OF o,g")
  .bind(order).bind(company).fetch_optional(c).await?;
    let Some(row) = row else { return Ok(None) };
    if !row.try_get::<bool, _>("bounded")? {
        return Err(invalid());
    }
    let actors: serde_json::Value = row
        .try_get::<Option<serde_json::Value>, _>("actors")?
        .ok_or_else(changed)?;
    let completion_actors = actors
        .as_array()
        .ok_or_else(changed)?
        .iter()
        .map(|v| v.as_str().map(str::to_owned).ok_or_else(changed))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(Image {
        agreement_status: row
            .try_get::<Option<String>, _>("agreement_status")?
            .ok_or_else(changed)?,
        rate: row.try_get("rate")?,
        discount: row.try_get("discount")?,
        currency: row
            .try_get::<Option<String>, _>("currency")?
            .ok_or_else(changed)?,
        completion_evidence: row
            .try_get::<Option<String>, _>("evidence")?
            .ok_or_else(changed)?,
        completed_at: row.try_get("completed_at")?,
        completion_actors,
        digest: row.try_get("digest")?,
    }))
}
