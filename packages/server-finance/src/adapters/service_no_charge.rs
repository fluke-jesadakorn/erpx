//! Finance-owned zero-demand decision. Only the outer selected native unit may
//! supply IAM, Field lineage, purchase and invoice routing authority.
use super::field_service_invoice_read::{
    CurrentFinanceServiceInvoiceWorkspace, read_service_invoice_workspace,
};
use crate::{
    application::journal::FinanceActor, domain::service_no_charge::require_no_charge_decision,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::commands::{CommandActor, CommandJson, command_fingerprint};
use serde_json::{Value, json};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_NO_CHARGE_SOURCE_CHANGED",
        "Review the original fully covered Service billing source.",
    )
}
pub async fn lock_service_no_charge_sources(c: &mut PgConnection) -> Result<(), Error> {
    // The outer composition coordinates Commercial/Field/IAM/receipt sources.
    // Finance-only locks protect every negative monetary-artifact assertion.
    sqlx::query("LOCK TABLE finance_invoices,finance_service_no_charge_confirmations IN SHARE ROW EXCLUSIVE MODE").execute(&mut *c).await.map_err(service_no_charge_persistence_error)?;
    sqlx::query("LOCK TABLE finance_journals,finance_journal_lines,finance_allocations,finance_refund_obligations,finance_billing_corrections,finance_business_billing_runs,money_transaction_snapshots IN SHARE MODE").execute(c).await.map_err(service_no_charge_persistence_error)?;
    Ok(())
}
async fn source(c: &mut PgConnection, actor: &FinanceActor, id: &str) -> Result<Value, Error> {
    let row: Option<Value> = sqlx::query_scalar("SELECT to_jsonb(s)-'source_image' FROM finance_service_no_charge_candidates s WHERE invoice_id=$1 AND company_id=$2")
        .bind(id).bind(actor.company_id.as_str()).fetch_optional(c).await.map_err(service_no_charge_persistence_error)?;
    row.ok_or_else(|| changed().into())
}
/// Private constructor retains the actual Finance workspace on its original
/// SERIALIZABLE transaction; no client source label or HTTP guard is accepted.
pub struct CurrentServiceNoCharge {
    workspace: CurrentFinanceServiceInvoiceWorkspace,
    source: Value,
    actor: FinanceActor,
    id: String,
}
pub async fn read_service_no_charge(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
) -> Result<CurrentServiceNoCharge, Error> {
    let workspace = read_service_invoice_workspace(c, actor, id).await?;
    let source = source(c, actor, id).await?;
    Ok(CurrentServiceNoCharge {
        workspace,
        source,
        actor: actor.clone(),
        id: id.into(),
    })
}
impl CurrentServiceNoCharge {
    pub fn scope_matches(&self, branch: Option<&str>, department: Option<&str>) -> bool {
        branch.is_none_or(|b| self.source["branch_id"].as_str() == Some(b))
            && department.is_none_or(|d| self.source["department_id"].as_str() == Some(d))
    }
    pub fn creator(&self) -> Result<&str, BusinessError> {
        self.source["created_by"].as_str().ok_or_else(changed)
    }
    pub fn releaser(&self) -> Result<&str, BusinessError> {
        self.source["released_by"].as_str().ok_or_else(changed)
    }
    pub fn version(&self) -> Result<i64, BusinessError> {
        self.source["invoice_version"]
            .as_i64()
            .filter(|n| (1..=9_007_199_254_740_991).contains(n))
            .ok_or_else(changed)
    }
    pub fn confirmed(&self) -> bool {
        self.source["invoice_status"] == "no_charge_confirmed"
    }
    pub fn record(&self) -> Value {
        json!(self.workspace.data().invoice)
    }
    /// Current selected IAM/session/scope and the original immutable source are
    /// both bound to the review. This opaque token is never an authority grant.
    pub fn context_key(&self, actor: &CommandActor) -> Result<String, BusinessError> {
        command_fingerprint(
            actor,
            "finance.service-no-charge.review",
            &self.id,
            &CommandJson::parse(&format!(
                "{{\"authority\":{},\"version\":{},\"sourceHash\":{}}}",
                actor.receipt_authority()?.as_str(),
                self.version()?,
                self.source["source_hash"]
            ))?,
        )
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.workspace.require_current(c).await?;
        if source(c, &self.actor, &self.id).await? != self.source {
            return Err(changed().into());
        }
        Ok(())
    }
    /// Only fresh decisions apply version/context/independence. Exact historical
    /// replay is checked separately against its retained terminal proof.
    pub fn require_decision(
        &self,
        expected: i64,
        key: &str,
        actor: &CommandActor,
        independent: bool,
    ) -> Result<(), BusinessError> {
        require_no_charge_decision(
            self.source["invoice_status"] == "draft",
            self.version()?,
            expected,
            key == self.context_key(actor)?,
            independent,
        )
    }
    pub async fn apply(
        self,
        c: &mut PgConnection,
        command: &str,
        reason: &str,
        now: i64,
    ) -> Result<RetainedServiceNoCharge, Error> {
        self.require_current(c).await?;
        let version = self.version()?;
        let confirmation = format!("service-no-charge-{command}");
        let mut expected = self.record();
        let map = expected.as_object_mut().ok_or_else(changed)?;
        map.insert("status".into(), json!("no_charge_confirmed"));
        map.insert("approved_by".into(), json!(self.actor.user_id.as_str()));
        map.insert("version".into(), json!(version + 1));
        map.insert("updated_at".into(), json!(now));
        let text = |key: &str| self.source[key].as_str().ok_or_else(changed);
        sqlx::query("INSERT INTO finance_service_no_charge_confirmations(id,company_id,invoice_id,service_order_id,billing_release_id,invoice_version_before,invoice_version_after,source_hash,status,independence_checked,confirmed_by,reason,command_id,confirmed_at)VALUES($1,$2,$3,$4,$5,$6,$7,$8,'no_charge_confirmed',true,$9,$10,$11,$12)")
            .bind(&confirmation).bind(self.actor.company_id.as_str()).bind(&self.id)
            .bind(text("service_order_id")?).bind(text("billing_release_id")?).bind(version).bind(version+1)
            .bind(text("source_hash")?).bind(self.actor.user_id.as_str()).bind(reason).bind(command).bind(now).execute(&mut *c).await.map_err(service_no_charge_persistence_error)?;
        let before: Value = sqlx::query_scalar(
            "SELECT to_jsonb(i) FROM finance_invoices i WHERE i.id=$1 AND i.company_id=$2",
        )
        .bind(&self.id)
        .bind(self.actor.company_id.as_str())
        .fetch_one(&mut *c)
        .await
        .map_err(service_no_charge_persistence_error)?;
        let mut final_image = before.clone();
        for key in ["status", "approved_by", "version", "updated_at"] {
            final_image[key] = expected[key].clone();
        }
        let previous: Option<String> =
            sqlx::query_scalar("SELECT current_setting('erpx.service_invoice_post_runtime',true)")
                .fetch_one(&mut *c)
                .await
                .map_err(service_no_charge_persistence_error)?;
        sqlx::query("SELECT set_config('erpx.service_invoice_post_runtime',jsonb_build_object('database',current_database(),'pid',pg_backend_pid()::text,'xid',pg_current_xact_id()::text,'started',pg_postmaster_start_time()::text,'invoice',$1::jsonb)::text,true)").bind(&final_image).execute(&mut *c).await.map_err(service_no_charge_persistence_error)?;
        let affected=sqlx::query("UPDATE finance_invoices SET status='no_charge_confirmed',approved_by=$1,version=version+1,updated_at=$2 WHERE id=$3 AND company_id=$4 AND status='draft' AND version=$5 AND to_jsonb(finance_invoices)=$6")
            .bind(self.actor.user_id.as_str()).bind(now).bind(&self.id).bind(self.actor.company_id.as_str()).bind(version).bind(before).execute(&mut *c).await.map_err(service_no_charge_persistence_error)?.rows_affected();
        sqlx::query("SELECT set_config('erpx.service_invoice_post_runtime',$1,true)")
            .bind(previous.as_deref().unwrap_or(""))
            .execute(&mut *c)
            .await
            .map_err(service_no_charge_persistence_error)?;
        if affected != 1 {
            return Err(changed().into());
        }
        let actual: Value = sqlx::query_scalar(
            "SELECT to_jsonb(i) FROM finance_invoices i WHERE id=$1 AND company_id=$2",
        )
        .bind(&self.id)
        .bind(self.actor.company_id.as_str())
        .fetch_one(&mut *c)
        .await
        .map_err(service_no_charge_persistence_error)?;
        if actual != final_image {
            return Err(changed().into());
        }
        let after = read_service_no_charge(c, &self.actor, &self.id).await?;
        if after.record() != expected || after.source["source_hash"] != self.source["source_hash"] {
            return Err(changed().into());
        }
        super::service_receipt_runtime::mark_invoice(c, self.actor.company_id.as_str(), &self.id)
            .await?;
        Ok(RetainedServiceNoCharge {
            current: after,
            command: command.into(),
        })
    }
    pub async fn require_terminal(&self, c: &mut PgConnection, command: &str) -> Result<(), Error> {
        self.require_current(c).await?;
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_service_no_charge_completed WHERE id=$1 AND company_id=$2 AND command_id=$3)")
            .bind(&self.id).bind(self.actor.company_id.as_str()).bind(command).fetch_one(c).await.map_err(service_no_charge_persistence_error)?;
        if !self.confirmed() || !exists {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SERVICE_NO_CHARGE_PROOF_CHANGED",
                "The exact independent no-charge confirmation is unavailable.",
            )
            .into());
        }
        Ok(())
    }
}
pub struct RetainedServiceNoCharge {
    current: CurrentServiceNoCharge,
    command: String,
}
impl RetainedServiceNoCharge {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.current.require_terminal(c, &self.command).await
    }
}

#[derive(Clone,Debug,PartialEq,Eq,serde::Serialize)]
#[cfg_attr(feature="contracts",derive(ts_rs::TS,utoipa::ToSchema))]
#[serde(rename_all="camelCase")]
pub struct ServiceNoChargeInput {
    #[cfg_attr(feature="contracts",ts(type="number"))]
    pub expected_version:i64,
    pub context_key:String,
    pub reason:String,
}
#[derive(Clone)]
pub struct ServiceNoChargeWire { input:ServiceNoChargeInput, normalized:CommandJson }
impl ServiceNoChargeWire {
    pub fn input(&self)->&ServiceNoChargeInput{&self.input}
    pub fn normalized(&self)->&CommandJson{&self.normalized}
}
#[derive(Clone,Debug,PartialEq,Eq,serde::Serialize)]
#[cfg_attr(feature="contracts",derive(ts_rs::TS,utoipa::ToSchema))]
#[serde(rename_all="snake_case")]
pub enum ServiceNoChargeStatus { NoChargeConfirmed }
#[derive(Clone,Debug,PartialEq,Eq,serde::Serialize)]
#[cfg_attr(feature="contracts",derive(ts_rs::TS,utoipa::ToSchema))]
#[serde(rename_all="camelCase")]
pub struct ServiceNoChargeResult {
    pub id:String,
    pub status:ServiceNoChargeStatus,
    #[cfg_attr(feature="contracts",ts(type="number"))]
    pub version:i64,
    pub confirmation_id:String,
}
#[derive(Clone,Debug,PartialEq,Eq,serde::Serialize)]
#[cfg_attr(feature="contracts",derive(ts_rs::TS,utoipa::ToSchema))]
pub struct ServiceNoChargeResponse {pub data:ServiceNoChargeResult}
/// Keep the original schema order, JS UTF-16 length/trim and unknown-key strip.
/// The logical receipt payload must not be rebuilt from lossy physical strings.
pub fn normalize_service_no_charge(body: &[u8]) -> Result<ServiceNoChargeWire, BusinessError> {
    let invalid = || {
        BusinessError::invalid(
            "VALIDATION_FAILED",
            "Review the original no-charge confirmation.",
        )
    };
    let text = String::from_utf8_lossy(body);
    let doc = CommandJson::parse(text.strip_prefix('\u{feff}').unwrap_or(&text))
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Request body must be valid JSON."))?;
    if doc.object_keys()?.is_none() {
        return Err(invalid());
    }
    let member = |key: &str| doc.property(key)?.ok_or_else(invalid);
    let raw = member("expectedVersion")?;
    let n: Value = serde_json::from_str(raw.as_str()).map_err(|_| invalid())?;
    let n = n.as_f64().ok_or_else(invalid)?;
    if n.fract() != 0.0 || !(1.0..=9_007_199_254_740_990.0).contains(&n) {
        return Err(invalid());
    }
    let context = member("contextKey")?
        .string_utf8_storage()?
        .ok_or_else(invalid)?;
    if context.len() != 64
        || !context
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    let units = member("reason")?
        .string_utf16_units()?
        .ok_or_else(invalid)?;
    let whitespace = |u: &u16| matches!(*u,0x0009..=0x000d|0x0020|0x00a0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff);
    let start = units
        .iter()
        .position(|u| !whitespace(u))
        .unwrap_or(units.len());
    let end = units
        .iter()
        .rposition(|u| !whitespace(u))
        .map_or(start, |i| i + 1);
    let reason = &units[start..end];
    if !(10..=3000).contains(&reason.len()) {
        return Err(invalid());
    }
    let normalized = CommandJson::parse(&format!(
        "{{\"expectedVersion\":{},\"contextKey\":{},\"reason\":{}}}",
        n as i64,
        member("contextKey")?.as_str(),
        CommandJson::from_utf16_string(reason)?.as_str()
    ))?;
    Ok(ServiceNoChargeWire {
        input:ServiceNoChargeInput{expected_version:n as i64,context_key:context,reason:String::from_utf16_lossy(reason)},
        normalized,
    })
}

/// A noncandidate is not an error for the shared invoice workspace. A database
/// error still propagates, so missing schema never silently selects TS.
pub async fn read_optional_service_no_charge(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
) -> Result<Option<CurrentServiceNoCharge>, Error> {
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_service_no_charge_candidates WHERE invoice_id=$1 AND company_id=$2)").bind(id).bind(actor.company_id.as_str()).fetch_one(&mut *c).await.map_err(service_no_charge_persistence_error)?;
    if exists {
        Ok(Some(read_service_no_charge(c, actor, id).await?))
    } else {
        Ok(None)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServiceNoChargeControls {
    pub invoice_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub version: i64,
    pub context_key: String,
    pub can_confirm: bool,
    pub confirmed: bool,
}

/// Preserve the finite reviewed public errors without exposing database details.
pub fn service_no_charge_persistence_error(error: sqlx::Error) -> Error {
    if let Some(db) = error.as_database_error() {
        if db.code().as_deref() == Some("42P01") {
            return BusinessError::new(
                FailureKind::Unavailable,
                "SERVICE_NO_CHARGE_SCHEMA_REQUIRED",
                "The reviewed nonmonetary confirmation schema is not installed.",
            )
            .into();
        }
        if db.code().as_deref() == Some("23514") {
            let code = match db.message() {
                "SERVICE_NO_CHARGE_EVIDENCE_IMMUTABLE" => {
                    Some("SERVICE_NO_CHARGE_EVIDENCE_IMMUTABLE")
                }
                "SERVICE_NO_CHARGE_INDEPENDENT_REVIEW_REQUIRED" => {
                    Some("SERVICE_NO_CHARGE_INDEPENDENT_REVIEW_REQUIRED")
                }
                "SERVICE_NO_CHARGE_PROOF_REQUIRED" => Some("SERVICE_NO_CHARGE_PROOF_REQUIRED"),
                "SERVICE_NO_CHARGE_TRANSITION_INVALID" => {
                    Some("SERVICE_NO_CHARGE_TRANSITION_INVALID")
                }
                _ => None,
            };
            if let Some(code) = code {
                return BusinessError::new(
                    FailureKind::Conflict,
                    code,
                    "The original no-charge confirmation source or immutable decision changed.",
                )
                .into();
            }
        }
    }
    error.into()
}
