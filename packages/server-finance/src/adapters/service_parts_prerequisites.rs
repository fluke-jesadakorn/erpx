//! Finance-only prerequisite rows. These witnesses provide facts, not authority.
use crate::domain::service_parts_prerequisites::{
    ServicePartsMoneyReference, ServicePartsPeriodReference, ServicePartsPrerequisiteReferences,
    ServicePartsRateReference,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
pub struct CurrentServicePartsPrerequisiteReferences {
    company: CompanyId,
    unit: Unit,
    data: ServicePartsPrerequisiteReferences,
}
impl CurrentServicePartsPrerequisiteReferences {
    pub fn data(&self) -> &ServicePartsPrerequisiteReferences {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || capture(c, &self.company).await? != self.data {
            return Err(changed());
        }
        Ok(())
    }
    /// Revalidate all earlier Finance facts while admitting exactly one new
    /// Finance-owned period or rate; the writer verifies its inserted fields.
    pub async fn require_after_insert(
        &self,
        c: &mut PgConnection,
        kind: &str,
        id: &str,
    ) -> Result<(), Error> {
        if !matches!(kind, "period" | "rate") || unit(c).await? != self.unit {
            return Err(changed());
        }
        let mut actual = capture(c, &self.company).await?;
        let count = match kind {
            "period" => {
                if self.data.periods.iter().any(|v| v.id == id) {
                    return Err(changed());
                }
                let before = actual.periods.len();
                actual.periods.retain(|v| v.id != id);
                before - actual.periods.len()
            }
            "rate" => {
                if self.data.rates.iter().any(|v| v.id == id) {
                    return Err(changed());
                }
                let before = actual.rates.len();
                actual.rates.retain(|v| v.id != id);
                before - actual.rates.len()
            }
            _ => unreachable!(),
        };
        if count != 1 || actual != self.data {
            return Err(changed());
        }
        Ok(())
    }
}
/// Acquire all participating owners' locks before any SELECT establishes a
/// serializable snapshot. The caller composes the other owners' locks first.
pub async fn lock_service_parts_prerequisite_references(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE finance_periods,company_fx_rates,company_money_settings,company_currency_scales,finance_commands IN SHARE MODE").execute(c).await?;
    Ok(())
}
/// The retained chart/account/period creators reject a legacy Finance command
/// when no current Workflow receipt can replay it. This is source data only.
pub struct CurrentLegacyFinanceCommand {
    key: String,
    unit: Unit,
    digest: Option<String>,
}
impl CurrentLegacyFinanceCommand {
    pub fn exists(&self) -> bool {
        self.digest.is_some()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || legacy_digest(c, &self.key).await? != self.digest {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_legacy_finance_command(
    c: &mut PgConnection,
    key: &str,
) -> Result<CurrentLegacyFinanceCommand, Error> {
    let unit = unit(c).await?;
    let digest = legacy_digest(c, key).await?;
    Ok(CurrentLegacyFinanceCommand {
        key: key.into(),
        unit,
        digest,
    })
}
async fn legacy_digest(c: &mut PgConnection, key: &str) -> Result<Option<String>, Error> {
    let row=sqlx::query("SELECT CASE WHEN octet_length(to_jsonb(f)::text)<=2097152 THEN encode(sha256(convert_to(to_jsonb(f)::text,'UTF8')),'hex') END digest FROM finance_commands f WHERE command_id=$1").bind(key).fetch_optional(c).await?;
    match row {
        None => Ok(None),
        Some(row) => Ok(Some(
            row.try_get::<Option<String>, _>("digest")?
                .ok_or_else(limited)?,
        )),
    }
}
pub async fn read_service_parts_prerequisite_references(
    c: &mut PgConnection,
    company: &CompanyId,
) -> Result<CurrentServicePartsPrerequisiteReferences, Error> {
    let unit = unit(c).await?;
    let data = capture(c, company).await?;
    Ok(CurrentServicePartsPrerequisiteReferences {
        company: company.clone(),
        unit,
        data,
    })
}
async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
) -> Result<ServicePartsPrerequisiteReferences, Error> {
    // Limit before transferring rows; return a single sentinel on overflow.
    let rows=sqlx::query("WITH source AS MATERIALIZED (SELECT 'period' kind,p.id key,to_jsonb(p)::text body,NULL::text rate FROM finance_periods p WHERE p.company_id=$1 UNION ALL SELECT 'rate',r.id,to_jsonb(r)::text,r.currency_per_usd::text FROM company_fx_rates r WHERE r.company_id=$1 UNION ALL SELECT 'money','settings',to_jsonb(s)::text,NULL FROM company_money_settings s WHERE s.company_id=$1 UNION ALL SELECT 'money','scale:'||s.currency,to_jsonb(s)::text,NULL FROM company_currency_scales s WHERE s.company_id=$1), measured AS (SELECT *,sum(octet_length(body)+octet_length(key)+coalesce(octet_length(rate),0)) OVER () bytes,count(*) OVER () count,row_number() OVER (ORDER BY kind,key) ordinal FROM source) SELECT CASE WHEN bytes<=2097152 AND count<=10000 THEN kind END kind,CASE WHEN bytes<=2097152 AND count<=10000 THEN key END key,CASE WHEN bytes<=2097152 AND count<=10000 THEN body END body,CASE WHEN bytes<=2097152 AND count<=10000 THEN rate END rate FROM measured WHERE (bytes<=2097152 AND count<=10000) OR ordinal=1 ORDER BY kind,key").bind(company.as_str()).fetch_all(&mut *c).await?;
    let mut result = ServicePartsPrerequisiteReferences {
        periods: vec![],
        rates: vec![],
        money: vec![],
    };
    for row in rows {
        let kind: Option<String> = row.try_get("kind")?;
        let key: Option<String> = row.try_get("key")?;
        let source: Option<String> = row.try_get("body")?;
        let (kind, key, source) = (
            kind.ok_or_else(limited)?,
            key.ok_or_else(limited)?,
            source.ok_or_else(limited)?,
        );
        let body: serde_json::Value = serde_json::from_str(&source).map_err(|_| limited())?;
        let string = |field: &str| -> Result<String, Error> {
            body.get(field)
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .ok_or_else(limited)
        };
        let integer = |field: &str| -> Result<i64, Error> {
            body.get(field).and_then(|v| v.as_i64()).ok_or_else(limited)
        };
        match kind.as_str() {
            "period" => result.periods.push(ServicePartsPeriodReference {
                id: key,
                name: string("name")?,
                start_date: string("start_date")?,
                end_date: string("end_date")?,
                state: string("state")?,
                version: integer("close_version")?,
                source,
            }),
            "rate" => result.rates.push(ServicePartsRateReference {
                id: key,
                currency: string("currency")?,
                effective_date: string("effective_date")?,
                currency_per_usd: row
                    .try_get::<Option<String>, _>("rate")?
                    .ok_or_else(limited)?,
                confirmed_at: integer("confirmed_at")?,
                source,
            }),
            "money" => result
                .money
                .push(ServicePartsMoneyReference { kind: key, source }),
            _ => return Err(limited()),
        }
    }
    Ok(result)
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
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_PARTS_POSTING_SOURCE_CHANGED",
        "The Finance prerequisite source changed.",
    )
    .into()
}
fn limited() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "SERVICE_PARTS_POSTING_SOURCE_LIMIT",
        "The Finance prerequisite source exceeds its bounded read.",
    )
    .into()
}

use crate::adapters::service_parts_posting::CurrentServicePartsPostingReferences;
use crate::domain::service_parts_prerequisites::{
    ServicePrerequisiteInput, ServicePrerequisiteKind,
};
use erpx_server_shared::public::UserId;
/// Source preparation must acquire this writer lease before any snapshot and
/// before other owners' source leases, matching the original transaction.
pub async fn lock_service_prerequisite_writer(
    c: &mut PgConnection,
    kind: ServicePrerequisiteKind,
) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_TRANSACTION_REQUIRED").into());
    }
    let sql = match kind {
        ServicePrerequisiteKind::Chart => "LOCK TABLE finance_charts IN SHARE ROW EXCLUSIVE MODE",
        ServicePrerequisiteKind::Account => {
            "LOCK TABLE finance_accounts IN SHARE ROW EXCLUSIVE MODE"
        }
        ServicePrerequisiteKind::Period => "LOCK TABLE finance_periods IN SHARE ROW EXCLUSIVE MODE",
        ServicePrerequisiteKind::Rate => "LOCK TABLE company_fx_rates IN SHARE ROW EXCLUSIVE MODE",
    };
    sqlx::query(sql).execute(c).await?;
    Ok(())
}
#[derive(Clone)]
pub struct ServicePrerequisiteIntent {
    pub company: CompanyId,
    pub actor: UserId,
    pub id: String,
    pub at: i64,
    pub input: ServicePrerequisiteInput,
}
pub struct PreparedServicePrerequisite {
    intent: ServicePrerequisiteIntent,
    currency: Option<String>,
}
/// Materialize the same Finance deferred boundary used by the retained
/// transaction before its final currentness checks.
pub async fn flush_service_prerequisite_effects(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("SET CONSTRAINTS finance_book_projection IMMEDIATE")
        .execute(&mut *c)
        .await?;
    sqlx::query("SET CONSTRAINTS finance_book_projection DEFERRED")
        .execute(&mut *c)
        .await?;
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(c)
        .await?;
    Ok(())
}
impl PreparedServicePrerequisite {
    pub async fn prepare(
        c: &mut PgConnection,
        intent: ServicePrerequisiteIntent,
        finance: &CurrentServicePartsPostingReferences,
        references: &CurrentServicePartsPrerequisiteReferences,
    ) -> Result<Self, Error> {
        finance.require_current(c).await?;
        references.require_current(c).await?;
        if finance.company() != &intent.company || references.company != intent.company {
            return Err(changed());
        }
        let currency =
            match &intent.input {
                ServicePrerequisiteInput::Account {
                    chart_id, currency, ..
                } => {
                    let chart = finance
                        .data()
                        .charts
                        .iter()
                        .find(|v| {
                            v.get("id").and_then(|x| x.as_str()) == Some(chart_id.as_str())
                                && v.get("status").and_then(|x| x.as_str()) == Some("active")
                        })
                        .ok_or_else(|| {
                            BusinessError::invalid(
                                "SERVICE_PARTS_CURRENCY_MISMATCH",
                                "Use the actual retained active Service parts chart.",
                            )
                        })?;
                    Some(currency.clone().unwrap_or_else(|| {
                        chart["base_currency"].as_str().unwrap_or("").to_owned()
                    }))
                }
                ServicePrerequisiteInput::Period {
                    start_date,
                    end_date,
                    ..
                } if end_date < start_date => {
                    return Err(BusinessError::invalid(
                        "INVALID_PERIOD",
                        "Period end must be on or after its start.",
                    )
                    .into());
                }
                ServicePrerequisiteInput::Rate {
                    currency,
                    currency_per_usd,
                    ..
                } => {
                    let valid = currency != "USD"
                        && crate::domain::company_money::rate_decimal_units(currency_per_usd, 18)
                            .is_ok_and(|v| v > num_bigint::BigInt::from(0));
                    if !valid {
                        return Err(BusinessError::invalid(
                            "INVALID_FX_RATE",
                            "The manual rate must be positive.",
                        )
                        .into());
                    }
                    None
                }
                _ => None,
            };
        Ok(Self { intent, currency })
    }
    pub fn resolved_currency(&self) -> Option<&str> {
        self.currency.as_deref()
    }
    pub async fn inserted_digest(&self, c: &mut PgConnection) -> Result<String, Error> {
        let sql = match &self.intent.input {
            ServicePrerequisiteInput::Chart { .. } => {
                "SELECT CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 THEN encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex') END FROM finance_charts r WHERE id=$1 AND company_id=$2"
            }
            ServicePrerequisiteInput::Account { .. } => {
                "SELECT CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 THEN encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex') END FROM finance_accounts r WHERE id=$1 AND company_id=$2"
            }
            ServicePrerequisiteInput::Period { .. } => {
                "SELECT CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 THEN encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex') END FROM finance_periods r WHERE id=$1 AND company_id=$2"
            }
            ServicePrerequisiteInput::Rate { .. } => {
                "SELECT CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 THEN encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex') END FROM company_fx_rates r WHERE id=$1 AND company_id=$2"
            }
        };
        sqlx::query_scalar::<_, Option<String>>(sql)
            .bind(&self.intent.id)
            .bind(self.intent.company.as_str())
            .fetch_optional(c)
            .await?
            .flatten()
            .ok_or_else(changed)
    }
    pub async fn apply(
        &self,
        c: &mut PgConnection,
        finance: &CurrentServicePartsPostingReferences,
        references: &CurrentServicePartsPrerequisiteReferences,
    ) -> Result<(), Error> {
        finance.require_current(c).await?;
        references.require_current(c).await?;
        let i = &self.intent;
        match &i.input {
            ServicePrerequisiteInput::Chart {
                name,
                base_currency,
                accounting_basis,
            } => {
                sqlx::query("INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$7)").bind(&i.id).bind(i.company.as_str()).bind(name).bind(base_currency).bind(accounting_basis).bind(i.actor.as_str()).bind(i.at).execute(&mut *c).await?;
            }
            ServicePrerequisiteInput::Account {
                chart_id,
                code,
                name,
                category,
                parent_id,
                is_control,
                control_kind,
                ..
            } => {
                let currency = self.currency.as_deref().ok_or_else(limited)?;
                sqlx::query("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,parent_id,currency,is_control,control_kind,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$11)").bind(&i.id).bind(chart_id).bind(i.company.as_str()).bind(code).bind(name).bind(category).bind(parent_id).bind(currency).bind(i64::from(*is_control||control_kind!="none")).bind(control_kind).bind(i.at).execute(&mut *c).await?;
            }
            ServicePrerequisiteInput::Period {
                name,
                start_date,
                end_date,
            } => {
                sqlx::query("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES($1,$2,$3,$4,$5,$6)").bind(&i.id).bind(i.company.as_str()).bind(name).bind(start_date).bind(end_date).bind(i.at).execute(&mut *c).await?;
            }
            ServicePrerequisiteInput::Rate {
                currency,
                effective_date,
                currency_per_usd,
                note,
            } => {
                sqlx::query("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES($1,$2,$3,$4::date,$5::numeric,$6,$7,$8)").bind(&i.id).bind(i.company.as_str()).bind(currency).bind(effective_date).bind(currency_per_usd).bind(i.actor.as_str()).bind(i.at).bind(note).execute(&mut *c).await?;
            }
        }
        Ok(())
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        finance: &CurrentServicePartsPostingReferences,
        references: &CurrentServicePartsPrerequisiteReferences,
    ) -> Result<(), Error> {
        let i = &self.intent;
        match &i.input {
            ServicePrerequisiteInput::Chart { .. } | ServicePrerequisiteInput::Account { .. } => {
                finance
                    .require_after_prerequisite_insert(c, i.input.kind(), &i.id)
                    .await?;
                references.require_current(c).await?;
            }
            _ => {
                finance.require_current(c).await?;
                references
                    .require_after_insert(c, i.input.kind(), &i.id)
                    .await?;
            }
        }
        let matched:bool=match &i.input {
            ServicePrerequisiteInput::Chart{name,base_currency,accounting_basis}=>sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_charts WHERE id=$1 AND company_id=$2 AND name=$3 AND base_currency=$4 AND accounting_basis=$5 AND created_by=$6 AND status='active' AND version=1)").bind(&i.id).bind(i.company.as_str()).bind(name).bind(base_currency).bind(accounting_basis).bind(i.actor.as_str()).fetch_one(&mut *c).await?,
            ServicePrerequisiteInput::Account{chart_id,code,name,category,parent_id,is_control,control_kind,..}=>sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_accounts WHERE id=$1 AND company_id=$2 AND chart_id=$3 AND code=$4 AND name=$5 AND category=$6 AND parent_id IS NOT DISTINCT FROM $7 AND currency=$8 AND is_control=$9 AND control_kind=$10 AND status='active' AND version=1)").bind(&i.id).bind(i.company.as_str()).bind(chart_id).bind(code).bind(name).bind(category).bind(parent_id).bind(self.currency.as_deref()).bind(i64::from(*is_control||control_kind!="none")).bind(control_kind).fetch_one(&mut *c).await?,
            ServicePrerequisiteInput::Period{name,start_date,end_date}=>sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_periods WHERE id=$1 AND company_id=$2 AND name=$3 AND start_date=$4 AND end_date=$5 AND state='open' AND close_version=0)").bind(&i.id).bind(i.company.as_str()).bind(name).bind(start_date).bind(end_date).fetch_one(&mut *c).await?,
            ServicePrerequisiteInput::Rate{currency,effective_date,currency_per_usd,note}=>sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM company_fx_rates WHERE id=$1 AND company_id=$2 AND currency=$3 AND effective_date=$4::date AND currency_per_usd=$5::numeric AND confirmed_by=$6 AND note=$7)").bind(&i.id).bind(i.company.as_str()).bind(currency).bind(effective_date).bind(currency_per_usd).bind(i.actor.as_str()).bind(note).fetch_one(&mut *c).await?,
        };
        if !matched {
            return Err(changed());
        }
        Ok(())
    }
}
