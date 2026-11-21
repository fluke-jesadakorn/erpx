//! Finance-owned physical customer references and creation delta. Composition
//! retains IAM, both Commercial purchases and the Workflow ledger in this unit.
use super::field_service_customer::unit;
use crate::domain::service_customer_preparation::{ServiceCustomerParty, ServiceCustomerResult};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, PartialEq, Eq)]
struct Image {
    parties: Vec<(String, String)>,
    money: Vec<(String, String)>,
}
#[derive(Clone, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServiceCustomerReference {
    pub id: String,
    pub code: String,
    pub name: String,
    pub currency: String,
}
#[derive(Clone, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServiceCustomerCurrency {
    pub code: String,
    pub name: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceCustomerCurrent {
    pub company_id: String,
    pub id: String,
    pub kind: String,
    pub status: String,
    pub code: String,
    pub name: String,
    pub currency: String,
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_CUSTOMER_SOURCE_CHANGED",
        "The original Service customer references changed.",
    )
    .into()
}
async fn reference_images(
    c: &mut PgConnection,
    company: &CompanyId,
    sql: &'static str,
    key: &str,
    bytes: &mut usize,
) -> Result<Vec<(String, String)>, Error> {
    let mut stream = sqlx::query(sql).bind(company.as_str()).fetch(c);
    let mut images = Vec::new();
    while let Some(row) = stream.try_next().await? {
        let image: Option<String> = row.try_get("image")?;
        let image = image.ok_or_else(changed)?;
        *bytes = bytes.checked_add(image.len()).ok_or_else(changed)?;
        if images.len() >= 20000 || *bytes > 16 * 1024 * 1024 {
            return Err(changed());
        }
        images.push((row.try_get(key)?, image));
    }
    Ok(images)
}
async fn capture(c: &mut PgConnection, company: &CompanyId) -> Result<Image, Error> {
    let mut bytes = 0;
    let parties=reference_images(&mut *c,company,"SELECT id,CASE WHEN octet_length(to_jsonb(p)::text)<=65536 THEN to_jsonb(p)::text END image FROM finance_parties p WHERE company_id=$1 ORDER BY id LIMIT 20001 FOR SHARE OF p","id",&mut bytes).await?;
    let mut money=reference_images(&mut *c,company,"SELECT 'settings'::text kind,CASE WHEN octet_length(to_jsonb(s)::text)<=65536 THEN to_jsonb(s)::text END image FROM company_money_settings s WHERE company_id=$1 FOR SHARE OF s","kind",&mut bytes).await?;
    money.extend(reference_images(&mut *c,company,"SELECT 'scale:'||currency kind,CASE WHEN octet_length(to_jsonb(s)::text)<=65536 THEN to_jsonb(s)::text END image FROM company_currency_scales s WHERE company_id=$1 ORDER BY currency LIMIT 20001 FOR SHARE OF s","kind",&mut bytes).await?);
    if money.len() > 20000 {
        return Err(changed());
    }
    money.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(Image { parties, money })
}

pub async fn lock_service_customer_sources(c: &mut PgConnection, write: bool) -> Result<(), Error> {
    if !sqlx::Connection::is_in_transaction(c) {
        return Err(changed());
    }
    if write {
        sqlx::query("LOCK TABLE finance_parties IN SHARE ROW EXCLUSIVE MODE")
            .execute(&mut *c)
            .await?;
    } else {
        sqlx::query("LOCK TABLE finance_parties IN SHARE MODE")
            .execute(&mut *c)
            .await?;
    }
    sqlx::query("LOCK TABLE company_money_settings,company_currency_scales IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub struct CurrentServiceCustomerReferences {
    unit: Unit,
    company: CompanyId,
    image: Image,
}
impl CurrentServiceCustomerReferences {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || capture(c, &self.company).await? != self.image {
            return Err(changed());
        }
        Ok(())
    }
    /// Canonical database images feed the opaque preparation context digest;
    /// callers expose only the typed reference projection, never these images.
    pub fn context_image(&self) -> Value {
        json!([
            self.image
                .parties
                .iter()
                .map(|(_, s)| s)
                .collect::<Vec<_>>(),
            self.image.money
        ])
    }
    pub fn customers(&self) -> Result<Vec<ServiceCustomerReference>, Error> {
        self.image
            .parties
            .iter()
            .filter_map(|(id, image)| {
                let parsed = serde_json::from_str::<Value>(image).map_err(|_| changed());
                match parsed {
                    Err(e) => Some(Err(e)),
                    Ok(v) if v["kind"] == "customer" && v["status"] == "active" => Some((|| {
                        let string =
                            |key: &str| v[key].as_str().map(str::to_owned).ok_or_else(changed);
                        Ok(ServiceCustomerReference {
                            id: id.clone(),
                            code: string("code")?,
                            name: string("name")?,
                            currency: string("currency")?,
                        })
                    })(
                    )),
                    _ => None,
                }
            })
            .collect()
    }
    /// Current confirmation is intentionally distinct from the immutable
    /// creation receipt, including a later blocked customer or changed label.
    pub fn customer(&self, id: &str) -> Result<Option<ServiceCustomerCurrent>, Error> {
        let Some((_, image)) = self.image.parties.iter().find(|(key, _)| key == id) else {
            return Ok(None);
        };
        let v: Value = serde_json::from_str(image).map_err(|_| changed())?;
        if v["company_id"] != self.company.as_str()
            || v["kind"] != "customer"
            || !matches!(v["status"].as_str(), Some("active" | "blocked"))
        {
            return Err(changed());
        }
        let string = |key: &str| v[key].as_str().map(str::to_owned).ok_or_else(changed);
        Ok(Some(ServiceCustomerCurrent {
            company_id: self.company.as_str().into(),
            id: id.into(),
            kind: "customer".into(),
            status: string("status")?,
            code: string("code")?,
            name: string("name")?,
            currency: string("currency")?,
        }))
    }
    fn configured_scales(&self) -> Result<std::collections::BTreeMap<String, i64>, Error> {
        let mut scales = std::collections::BTreeMap::new();
        for (kind, image) in &self.image.money {
            if kind.starts_with("scale:") {
                let v: Value = serde_json::from_str(image).map_err(|_| changed())?;
                scales.insert(
                    v["currency"].as_str().ok_or_else(changed)?.to_owned(),
                    v["scale"].as_i64().ok_or_else(changed)?,
                );
            }
        }
        Ok(scales)
    }
    pub fn currency_scale(&self, currency: &str) -> Result<u8, Error> {
        let rules =
            super::postgres_journal_facts::embedded_currency_rules().map_err(|_| changed())?;
        let scales = self.configured_scales()?;
        rules
            .scale(currency, scales.get(currency).copied())
            .map_err(|_| {
                BusinessError::invalid(
                    "CURRENCY_SCALE_REQUIRED",
                    "Configure a current ISO currency scale.",
                )
                .into()
            })
    }
    pub fn currencies(&self) -> Result<Vec<ServiceCustomerCurrency>, Error> {
        #[derive(Deserialize)]
        struct Catalogue {
            currencies: Vec<Definition>,
        }
        #[derive(Deserialize)]
        struct Definition {
            code: String,
            name: String,
        }
        let catalogue: Catalogue =
            serde_json::from_str(include_str!("../../../catalogue/src/iso-4217-current.json"))
                .map_err(|_| changed())?;
        let rules =
            super::postgres_journal_facts::embedded_currency_rules().map_err(|_| changed())?;
        let scales = self.configured_scales()?;
        Ok(catalogue
            .currencies
            .into_iter()
            .filter(|d| rules.scale(&d.code, scales.get(&d.code).copied()).ok() == Some(2))
            .map(|d| ServiceCustomerCurrency {
                code: d.code,
                name: d.name,
            })
            .collect())
    }
}
pub async fn read_service_customer_references(
    c: &mut PgConnection,
    company: &CompanyId,
) -> Result<CurrentServiceCustomerReferences, Error> {
    Ok(CurrentServiceCustomerReferences {
        unit: unit(c).await?,
        company: company.clone(),
        image: capture(c, company).await?,
    })
}
pub struct PreparedServiceCustomer {
    before: CurrentServiceCustomerReferences,
    id: String,
    party: ServiceCustomerParty,
    minor: i64,
    now: i64,
}
impl PreparedServiceCustomer {
    pub fn result(&self) -> ServiceCustomerResult {
        ServiceCustomerResult {
            id: self.id.clone(),
            party: self.party.clone(),
            credit_limit_minor: self.minor,
        }
    }
    pub async fn persist(
        self,
        c: &mut PgConnection,
    ) -> Result<RetainedServiceCustomerCreation, Error> {
        self.before.require_current(c).await?;
        let expected = json!({"id":self.id,"company_id":self.before.company.as_str(),"kind":"customer","code":self.party.code,"name":self.party.name,"currency":self.party.currency,"tax_registration":self.party.tax_registration,"payment_terms_days":self.party.payment_terms_days,"credit_limit_minor":self.minor,"status":"active","version":1,"created_at":self.now,"updated_at":self.now,"person_id":null,"phone":null,"address_json":{},"notification_email":null,"notification_consent":"unknown","merged_into_id":null,"merged_at":null,"merged_by":null});
        let expected: String = sqlx::query_scalar(
            "SELECT to_jsonb(jsonb_populate_record(NULL::finance_parties,$1::jsonb))::text",
        )
        .bind(expected.to_string())
        .fetch_one(&mut *c)
        .await?;
        sqlx::query("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,tax_registration,payment_terms_days,credit_limit_minor,created_at,updated_at)VALUES($1,$2,'customer',$3,$4,$5,$6,$7,$8,$9,$9)").bind(&self.id).bind(self.before.company.as_str()).bind(&self.party.code).bind(&self.party.name).bind(&self.party.currency).bind(&self.party.tax_registration).bind(i64::from(self.party.payment_terms_days)).bind(self.minor).bind(self.now).execute(&mut *c).await?;
        let image = capture(c, &self.before.company).await?;
        if image.money != self.before.image.money
            || image
                .parties
                .iter()
                .filter(|(id, _)| id != &self.id)
                .cloned()
                .collect::<Vec<_>>()
                != self.before.image.parties
            || image
                .parties
                .iter()
                .find(|(id, _)| id == &self.id)
                .map(|(_, raw)| raw)
                != Some(&expected)
        {
            return Err(changed());
        }
        let result = self.result();
        let retained = RetainedServiceCustomerCreation {
            unit: self.before.unit,
            company: self.before.company,
            image,
            result,
        };
        retained.require_current(c).await?;
        Ok(retained)
    }
}
pub struct RetainedServiceCustomerCreation {
    unit: Unit,
    company: CompanyId,
    image: Image,
    result: ServiceCustomerResult,
}
impl RetainedServiceCustomerCreation {
    pub fn result(&self) -> &ServiceCustomerResult {
        &self.result
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || capture(c, &self.company).await? != self.image {
            return Err(changed());
        }
        Ok(())
    }
}
pub fn prepare_service_customer(
    before: CurrentServiceCustomerReferences,
    id: String,
    mut party: ServiceCustomerParty,
    now: i64,
) -> Result<PreparedServiceCustomer, Error> {
    party.validate_fields().map_err(|_| {
        BusinessError::invalid(
            "SERVICE_CUSTOMER_INPUT_INVALID",
            "Use a valid customer input.",
        )
    })?;
    if !id.starts_with("party-")
        || id.len() > 200
        || !(0..=9_007_199_254_740_991).contains(&now)
        || before.image.parties.iter().any(|(old, _)| old == &id)
    {
        return Err(changed());
    }
    party.currency = party.currency.to_uppercase();
    if before.currency_scale(&party.currency)? != 2 {
        return Err(BusinessError::invalid(
            "FINANCE_CURRENCY_SCALE_UNSUPPORTED",
            "Service customers require a current two-decimal currency.",
        )
        .into());
    }

    for (_, raw) in &before.image.parties {
        let row: Value = serde_json::from_str(raw).map_err(|_| changed())?;
        if row["kind"] == "customer" && row["code"] == party.code {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SERVICE_CUSTOMER_CODE_EXISTS",
                "This customer code is already in use.",
            )
            .into());
        }
    }
    let minor = party.credit_minor().map_err(|_| {
        BusinessError::invalid(
            "INVALID_MONEY",
            "Use an exact, bounded two-decimal credit limit.",
        )
    })?;
    Ok(PreparedServiceCustomer {
        before,
        id,
        party,
        minor,
        now,
    })
}
