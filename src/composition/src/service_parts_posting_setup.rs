//! Exact retained Service parts setup context encoding. Owner witnesses and
//! selected authority remain required before this data can be used for a write.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::commands::CommandJson;
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
/// Already owner-sourced PostgreSQL jsonb array text. The original query uses
/// SQL NULL for empty families; `None` and the text `null` both bind as JSON null.
pub struct PostingHistoryImages<'a> {
    pub orders: &'a str,
    pub stock: &'a str,
    pub ledger: &'a str,
    pub counts: &'a str,
    pub consumptions: &'a str,
    pub invoices: Option<&'a str>,
    pub payments: Option<&'a str>,
    pub journals: Option<&'a str>,
    pub service: &'a str,
}
/// Reconstruct PostgreSQL's actual `jsonb_build_object(...)::text` rendering
/// from owner-provided values; no owner relation is read by composition.
pub async fn retained_history_image(
    c: &mut PgConnection,
    h: &PostingHistoryImages<'_>,
) -> Result<String, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("SERVICE_POSTING_TRANSACTION_REQUIRED").into());
    }
    let values = [
        Some(h.orders),
        Some(h.stock),
        Some(h.ledger),
        Some(h.counts),
        Some(h.consumptions),
        h.invoices,
        h.payments,
        h.journals,
        Some(h.service),
    ];
    if values
        .iter()
        .flatten()
        .try_fold(0usize, |n, v| n.checked_add(v.len()))
        .is_none_or(|n| n > 2 * 1024 * 1024)
    {
        return Err(invalid());
    }
    let image:Option<String>=sqlx::query_scalar("WITH image AS (SELECT jsonb_build_object('orders',$1::jsonb,'stock',$2::jsonb,'ledger',$3::jsonb,'counts',$4::jsonb,'consumptions',$5::jsonb,'invoices',$6::jsonb,'payments',$7::jsonb,'journals',$8::jsonb,'service',$9::jsonb)::text value) SELECT CASE WHEN octet_length(value)<=2097152 THEN value END FROM image")
        .bind(h.orders).bind(h.stock).bind(h.ledger).bind(h.counts).bind(h.consumptions).bind(h.invoices).bind(h.payments).bind(h.journals).bind(h.service).fetch_one(c).await?;
    image.ok_or_else(invalid)
}
/// Encode the original `JSON.stringify([identity, state, refs.image])`. This
/// function hashes data only; a sealed current IAM witness supplies identity.
pub fn retained_context_key(
    identity: &CommandJson,
    mapping: Option<&str>,
    proof: Option<&str>,
    history: bool,
    history_image: &str,
    finance_reference_image: &str,
) -> Result<String, BusinessError> {
    let json = |value: Option<&str>| -> Result<String, BusinessError> {
        match value {
            Some(value) => Ok(CommandJson::parse(value)?.as_str().to_owned()),
            None => Ok("null".into()),
        }
    };
    let quote = |value: &str| serde_json::to_string(value).map_err(|_| invalid_business());
    let state = format!(
        "{{\"mapping\":{},\"proof\":{},\"history\":{},\"image\":{}}}",
        json(mapping)?,
        json(proof)?,
        history,
        quote(history_image)?
    );
    let value = format!(
        "[{},{},{}]",
        quote(identity.as_str())?,
        state,
        quote(finance_reference_image)?
    );
    let canonical = CommandJson::parse(&value)?;
    Ok(hex::encode(Sha256::digest(canonical.as_str().as_bytes())))
}
fn invalid() -> Error {
    invalid_business().into()
}
fn invalid_business() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SERVICE_PARTS_POSTING_CONTEXT_UNAVAILABLE",
        "The original Service setup context is unavailable.",
    )
}

use erpx_server_commerce::public::service_parts_posting::{
    lock_service_parts_posting_sources, read_service_parts_posting,
    service_posting_schema_available,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, lock_commercial_authority_sources,
};
use erpx_server_field_service::public::service_parts_posting::{
    lock_service_parts_field_history, read_service_parts_field_history,
};
use erpx_server_finance::public::service_parts_posting::{
    ServicePartsPostingMapping, lock_service_parts_posting_references,
    read_service_parts_posting_references,
};
use erpx_server_identity_access::public::{
    effective_authority::{AuthorizedWorkContext, HoldingWorkSourcePreparation},
    selected_command_authority::{
        ServicePartsAuthorityPurpose, capture_service_parts_command_authority,
    },
};
use erpx_server_inventory::public::service_parts_posting::{
    lock_service_parts_inventory_history, read_service_parts_inventory_history,
};
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::{Clock, CompanyId, HoldingId, TimestampMillis};
use erpx_server_workflow::public::service_parts_posting::{
    lock_service_posting_receipts, read_service_posting_receipt,
};
use serde::Serialize;

/// This read is only mounted by trusted nonproduction assembly. Production
/// feature-release authority requires a separate real owner witness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServicePostingEnvironment {
    Development,
    Production,
}
#[derive(Clone, Copy)]
pub struct ServicePostingSources;
impl HoldingWorkSourcePreparation for ServicePostingSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_service_posting_receipts(c).await?;
            lock_service_parts_posting_sources(c).await?;
            lock_service_parts_inventory_history(c).await?;
            lock_service_parts_field_history(c).await?;
            lock_service_parts_posting_references(c).await?;
            Ok(())
        })
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RetainedWorkAuthority<'a> {
    pub(crate) session_id: &'a str,
    pub(crate) revision: i64,
    pub(crate) assignment_version: i64,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServicePostingView {
    pub company_id: String,
    pub available: bool,
    pub schema_available: bool,
    pub configured: Option<bool>,
    pub source_ready: bool,
    pub version: Option<u8>,
    pub can_configure: bool,
    pub can_confirm_existing: bool,
    pub context_key: Option<String>,
    #[cfg_attr(feature="contracts",ts(type="Record<string, string | number | null> | null"),schema(value_type=Option<Object>))]
    pub configuration: Option<serde_json::Value>,
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub charts: Vec<serde_json::Value>,
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub accounts: Vec<serde_json::Value>,
    #[cfg_attr(feature="contracts",ts(type="Record<string, string | number | null> | null"),schema(value_type=Option<Object>))]
    pub expected_sales_defaults: Option<serde_json::Value>,
    #[cfg_attr(feature="contracts",ts(type="Record<string, string | number | null> | null"),schema(value_type=Option<Object>))]
    pub approved_sales_defaults: Option<serde_json::Value>,
    pub unavailable_reason: Option<&'static str>,
}
fn unavailable(company: &CompanyId, reason: &'static str) -> ServicePostingView {
    ServicePostingView {
        company_id: company.as_str().into(),
        available: false,
        schema_available: false,
        configured: None,
        source_ready: false,
        version: None,
        can_configure: false,
        can_confirm_existing: false,
        context_key: None,
        configuration: None,
        charts: vec![],
        accounts: vec![],
        expected_sales_defaults: None,
        approved_sales_defaults: None,
        unavailable_reason: Some(reason),
    }
}
fn posting_mapping(value: &serde_json::Value) -> Option<ServicePartsPostingMapping> {
    let field = |key: &str| value.get(key)?.as_str().map(str::to_owned);
    Some(ServicePartsPostingMapping {
        chart_id: field("chart_id")?,
        currency: field("currency")?,
        receivable_account_id: field("receivable_account_id")?,
        payable_account_id: field("payable_account_id")?,
        revenue_account_id: field("revenue_account_id")?,
        inventory_account_id: field("inventory_account_id")?,
        clearing_account_id: field("clearing_account_id")?,
        cogs_account_id: field("cogs_account_id")?,
        input_tax_account_id: field("input_tax_account_id")?,
        output_tax_account_id: field("output_tax_account_id")?,
        cash_account_id: field("cash_account_id")?,
    })
}
pub(crate) struct PostingPurchases {
    holding: HoldingId,
    company: CompanyId,
    catalogue: CommercialCatalogue,
    history: PurchaseWitness,
    new_work: PurchaseWitness,
    pub(crate) history_allowed: bool,
    pub(crate) new_work_allowed: bool,
}
impl PostingPurchases {
    pub(crate) async fn capture(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        now: TimestampMillis,
    ) -> Result<Self, Error> {
        let holding = current.holding_id().clone();
        let company = current.selection().workspace().company_id.clone();
        let catalogue = erpx_server_commercial::public::purchases::current_package_catalogue()?;
        let mut reader =
            PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, &catalogue);
        let request = |purpose| PurchaseRequest {
            capabilities: vec![
                "service.orders".into(),
                "service.parts-assets".into(),
                "finance.service-invoice".into(),
            ],
            purpose,
            mode: PackageEnforcementMode::Enforced,
        };
        let history = reader
            .inspect(&request(CapabilityPurpose::History), now)
            .await?;
        let history_allowed = history.require_allowed().is_ok();
        let new_work = reader
            .inspect(&request(CapabilityPurpose::NewWork), now)
            .await?;
        let new_work_allowed = new_work.require_allowed().is_ok();
        Ok(Self {
            holding,
            company,
            catalogue,
            history: history.witness,
            new_work: new_work.witness,
            history_allowed,
            new_work_allowed,
        })
    }
    pub(crate) async fn require_current(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        let mut reader = PgCompanyPurchaseReader::new(
            self.holding.clone(),
            self.company.clone(),
            c,
            &self.catalogue,
        );
        let a = reader.revalidate(&self.history, now).await?;
        let b = reader.revalidate(&self.new_work, now).await?;
        if a.require_allowed().is_ok() != self.history_allowed
            || b.require_allowed().is_ok() != self.new_work_allowed
        {
            return Err(invalid());
        }
        Ok(())
    }
}
/// Caller must first use the selected Identity coordinator with `ServicePostingSources`.
/// The function captures and revalidates all actual owner witnesses in that unit.
pub async fn read_service_posting_setup<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    environment: ServicePostingEnvironment,
) -> Result<ServicePostingView, Error> {
    read_service_posting_setup_for(
        c,
        current,
        clock,
        environment,
        ServicePartsAuthorityPurpose::Read,
    )
    .await
}
async fn read_service_posting_setup_for<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    environment: ServicePostingEnvironment,
    purpose: ServicePartsAuthorityPurpose,
) -> Result<ServicePostingView, Error> {
    let company = &current.selection().workspace().company_id;
    if environment != ServicePostingEnvironment::Development {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "SERVICE_POSTING_ENVIRONMENT_UNAVAILABLE",
            "Service setup is unavailable.",
        )
        .into());
    }
    let scope = current.selection().workspace();
    if scope.branch_id.is_some()
        || scope.department_id.is_some()
        || !matches!(
            current.selection().assignment().scope_kind,
            WorkScopeKind::Holding
        )
        || !current.has_permission("finance.read")
        || !(current.has_permission("finance.write")
            || purpose == ServicePartsAuthorityPurpose::Read
                && current.has_permission("finance.approve"))
    {
        return Ok(unavailable(company, "authority-required"));
    }
    let actor = capture_service_parts_command_authority(c, current, purpose, clock).await?;
    let purchases = PostingPurchases::capture(c, current, clock.now()).await?;
    if purpose == ServicePartsAuthorityPurpose::Read && !purchases.history_allowed {
        purchases.require_current(c, clock.now()).await?;
        actor.require_current(c, clock).await?;
        return Ok(unavailable(company, "purchase-required"));
    }
    let commerce = read_service_parts_posting(c, company).await?;
    let inventory =
        read_service_parts_inventory_history(c, company, &commerce.facts().warehouse_ids).await?;
    let field = read_service_parts_field_history(c, company).await?;
    let receipt = read_service_posting_receipt(c, company).await?;
    let mapping = commerce
        .facts()
        .mapping_json
        .as_deref()
        .map(serde_json::from_str::<serde_json::Value>)
        .transpose()
        .map_err(|_| invalid_business())?;
    let typed = mapping.as_ref().and_then(posting_mapping);
    let finance = read_service_parts_posting_references(c, company, typed.as_ref()).await?;
    let proof = if commerce.facts().schema_available {
        receipt.facts().map(|f| f.proof_json.as_str())
    } else {
        None
    };
    let proof_value = proof
        .map(serde_json::from_str::<serde_json::Value>)
        .transpose()
        .map_err(|_| invalid_business())?;
    let source_ready = proof_value
        .as_ref()
        .is_some_and(|p| p.get("mapping") == mapping.as_ref());
    let valid = match (&mapping, &typed) {
        (None, _) => true,
        (Some(_), Some(m)) => finance.validate_mapping(m).is_ok(),
        _ => false,
    };
    let finance_data = finance.data();
    let inv = inventory.data();
    let fld = field.data();
    let fh = &finance_data.history_image;
    let history = commerce.facts().has_commerce_history
        || inv.has_history
        || fld.has_history
        || finance_data.finance_history;
    let image = retained_history_image(
        c,
        &PostingHistoryImages {
            orders: &commerce.facts().commerce_orders_json,
            stock: &inv.stock_json,
            ledger: &inv.ledger_json,
            counts: &inv.counts_json,
            consumptions: &inv.consumptions_json,
            invoices: fh.invoices.as_deref(),
            payments: fh.payments.as_deref(),
            journals: fh.journals.as_deref(),
            service: &fld.service_json,
        },
    )
    .await?;
    let selection = current.selection();
    let assignment = selection.assignment();
    let authority = RetainedWorkAuthority {
        session_id: current.session_id().as_str(),
        revision: selection.revision(),
        assignment_version: assignment.version,
    };
    let identity = CommandJson::from_serializable(&(
        company.as_str(),
        Option::<&str>::None,
        Option::<&str>::None,
        current.user_id().as_str(),
        assignment.id.as_str(),
        assignment.role.as_str(),
        authority,
        actor.iam(),
    ))?;
    let key = retained_context_key(
        &identity,
        commerce.facts().mapping_json.as_deref(),
        proof,
        history,
        &image,
        &finance_data.reference_image,
    )?;
    let active = current.has_permission("finance.write") && purchases.new_work_allowed;
    let reason = if !commerce.facts().schema_available {
        Some("schema-required")
    } else if mapping.is_some() && !valid || proof.is_some() && !source_ready {
        Some("invalid-existing-mapping")
    } else if mapping.is_none() && history {
        Some("history-requires-review")
    } else if mapping.is_none() && finance_data.defaults_blocked {
        Some("sales-defaults-required")
    } else {
        None
    };
    let charts=finance_data.charts.iter().filter(|r|r["status"]=="active").map(|r|serde_json::json!({"id":r["id"],"name":r["name"],"baseCurrency":r["base_currency"]})).collect();
    let accounts=finance_data.accounts.iter().filter(|r|r["status"]=="active"&&r["currency"].is_string()).map(|r|serde_json::json!({"id":r["id"],"chartId":r["chart_id"],"code":r["code"],"name":r["name"],"category":r["category"],"currency":r["currency"],"controlKind":r["control_kind"]})).collect();
    let expected = finance_data
        .expected_sales_defaults
        .as_ref()
        .map(|d| serde_json::json!({"id":d.id,"version":d.version}));
    let approved=finance_data.approved_sales_defaults.as_ref().map(|d|serde_json::json!({"id":d["id"],"version":d["version"],"chartId":d["chart_id"],"currency":d["currency"],"receivableAccountId":d["receivable_account_id"],"revenueAccountId":d["revenue_account_id"],"taxAccountId":d["tax_account_id"]}));
    let result = ServicePostingView {
        company_id: company.as_str().into(),
        available: true,
        schema_available: commerce.facts().schema_available,
        configured: Some(mapping.is_some()),
        source_ready,
        version: Some(if proof.is_some() { 1 } else { 0 }),
        can_configure: active
            && commerce.facts().schema_available
            && mapping.is_none()
            && !history
            && proof.is_none()
            && !finance_data.defaults_blocked,
        can_confirm_existing: active
            && commerce.facts().schema_available
            && mapping.is_some()
            && valid
            && proof.is_none(),
        context_key: Some(key),
        configuration: mapping.as_ref().map(|m| serde_json::json!({
            "chartId":m["chart_id"], "currency":m["currency"],
            "receivableAccountId":m["receivable_account_id"], "payableAccountId":m["payable_account_id"],
            "revenueAccountId":m["revenue_account_id"], "inventoryAccountId":m["inventory_account_id"],
            "clearingAccountId":m["clearing_account_id"], "cogsAccountId":m["cogs_account_id"],
            "inputTaxAccountId":m["input_tax_account_id"], "outputTaxAccountId":m["output_tax_account_id"],
            "cashAccountId":m["cash_account_id"]
        })),
        charts,
        accounts,
        expected_sales_defaults: expected,
        approved_sales_defaults: approved,
        unavailable_reason: reason,
    };
    commerce.require_current(c).await?;
    inventory.require_current(c).await?;
    field.require_current(c).await?;
    finance.require_current(c).await?;
    receipt.require_current(c).await?;
    purchases.require_current(c, clock.now()).await?;
    actor.require_current(c, clock).await?;
    Ok(result)
}

use erpx_server_commerce::public::service_parts_posting::{
    PostingMode, ServicePartsPostingMapping as CommerceMapping,
};
use erpx_server_finance::public::service_parts_posting::SalesDefaultsVersion;
/// Retains the canonical original Zod input for Workflow fingerprint/replay.
#[derive(Clone)]
pub struct NormalizedServicePostingCommand {
    pub mode: PostingMode,
    pub context_key: String,
    pub mapping: Option<CommerceMapping>,
    pub expected_sales_defaults: Option<SalesDefaultsVersion>,
    pub workflow_input: CommandJson,
}
fn wire_invalid() -> BusinessError {
    BusinessError::invalid(
        "VALIDATION_FAILED",
        "Review the Service posting setup input.",
    )
}
pub(crate) fn trimmed_units(
    value: &CommandJson,
    min: usize,
    max: usize,
    trim: bool,
) -> Result<(String, CommandJson), BusinessError> {
    let raw = value.string_utf16_units()?.ok_or_else(wire_invalid)?;
    let whitespace = |u: &u16| matches!(*u,0x0009..=0x000d|0x0020|0x00a0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff);
    let start = if trim {
        raw.iter().position(|u| !whitespace(u)).unwrap_or(raw.len())
    } else {
        0
    };
    let end = if trim {
        raw.iter()
            .rposition(|u| !whitespace(u))
            .map_or(start, |n| n + 1)
    } else {
        raw.len()
    };
    let units = &raw[start..end];
    if !(min..=max).contains(&units.len()) {
        return Err(wire_invalid());
    }
    Ok((
        String::from_utf16_lossy(units),
        CommandJson::from_utf16_string(units)?,
    ))
}
/// Strict wire schema; logical UTF-16 and optional-field presence remain in the
/// saved Workflow input while storage strings use PostgreSQL's physical form.
pub fn normalize_service_posting_command(
    input: &CommandJson,
    mode: PostingMode,
) -> Result<NormalizedServicePostingCommand, BusinessError> {
    let base = ["expectedVersion", "contextKey", "reason"];
    let extras = [
        "chartId",
        "currency",
        "receivableAccountId",
        "payableAccountId",
        "revenueAccountId",
        "inventoryAccountId",
        "clearingAccountId",
        "cogsAccountId",
        "inputTaxAccountId",
        "outputTaxAccountId",
        "cashAccountId",
        "expectedSalesDefaults",
    ];
    let keys = input.object_keys()?.ok_or_else(wire_invalid)?;
    if keys.iter().any(|k| {
        !base.contains(&k.as_str())
            && (mode != PostingMode::Initialize || !extras.contains(&k.as_str()))
    }) {
        return Err(wire_invalid());
    }
    let property = |name: &str| input.property(name)?.ok_or_else(wire_invalid);
    if property("expectedVersion")?.as_str() != "0" {
        return Err(wire_invalid());
    }
    let (context, key_json) = trimmed_units(&property("contextKey")?, 64, 64, false)?;
    if !context
        .bytes()
        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(wire_invalid());
    }
    let (_, reason) = trimmed_units(&property("reason")?, 3, 1000, true)?;
    let mut fields = vec![
        "\"expectedVersion\":0".to_owned(),
        format!("\"contextKey\":{}", key_json.as_str()),
    ];
    let mut mapping = None;
    let mut expected_sales_defaults = None;
    if mode == PostingMode::Initialize {
        let mut physical = Vec::new();
        for name in [
            "chartId",
            "currency",
            "reason",
            "receivableAccountId",
            "payableAccountId",
            "revenueAccountId",
            "inventoryAccountId",
            "clearingAccountId",
            "cogsAccountId",
            "inputTaxAccountId",
            "outputTaxAccountId",
            "cashAccountId",
        ] {
            let (value, json) = if name == "reason" {
                (String::new(), reason.clone())
            } else {
                trimmed_units(
                    &property(name)?,
                    if name == "currency" { 3 } else { 1 },
                    if name == "currency" { 3 } else { 180 },
                    name != "currency",
                )?
            };
            if name == "currency" && !value.bytes().all(|c| c.is_ascii_uppercase()) {
                return Err(wire_invalid());
            }
            if name != "reason" {
                physical.push(value);
            }
            fields.push(format!(
                "{}:{}",
                serde_json::to_string(name).map_err(|_| wire_invalid())?,
                json.as_str()
            ));
        }
        mapping = Some(CommerceMapping {
            chart_id: physical[0].clone(),
            currency: physical[1].clone(),
            receivable_account_id: physical[2].clone(),
            payable_account_id: physical[3].clone(),
            revenue_account_id: physical[4].clone(),
            inventory_account_id: physical[5].clone(),
            clearing_account_id: physical[6].clone(),
            cogs_account_id: physical[7].clone(),
            input_tax_account_id: physical[8].clone(),
            output_tax_account_id: physical[9].clone(),
            cash_account_id: physical[10].clone(),
        });
        if let Some(value) = input.property("expectedSalesDefaults")? {
            if value.as_str() == "null" {
                fields.push("\"expectedSalesDefaults\":null".into());
            } else {
                let keys = value.object_keys()?.ok_or_else(wire_invalid)?;
                if keys.len() != 2 || keys.iter().any(|k| k != "id" && k != "version") {
                    return Err(wire_invalid());
                }
                let (id, id_json) = trimmed_units(
                    &value.property("id")?.ok_or_else(wire_invalid)?,
                    1,
                    180,
                    false,
                )?;
                let version = value
                    .property("version")?
                    .ok_or_else(wire_invalid)?
                    .as_str()
                    .parse::<i64>()
                    .map_err(|_| wire_invalid())?;
                if !(1..=9_007_199_254_740_991).contains(&version) {
                    return Err(wire_invalid());
                }
                expected_sales_defaults = Some(SalesDefaultsVersion { id, version });
                fields.push(format!(
                    "\"expectedSalesDefaults\":{{\"id\":{},\"version\":{version}}}",
                    id_json.as_str()
                ));
            }
        }
    } else {
        fields.push(format!("\"reason\":{}", reason.as_str()));
    }
    let workflow_input = CommandJson::parse(&format!("{{{}}}", fields.join(",")))?;
    Ok(NormalizedServicePostingCommand {
        mode,
        context_key: context,
        mapping,
        expected_sales_defaults,
        workflow_input,
    })
}

use erpx_server_commerce::public::service_parts_posting::{
    CurrentServicePartsPosting, PgServicePartsPostingSource, PreparedServicePartsPosting,
    ServicePartsPostingIntent,
};
use erpx_server_field_service::public::service_parts_posting::CurrentServicePartsFieldHistory;
use erpx_server_finance::public::service_parts_posting::CurrentServicePartsPostingReferences;
use erpx_server_identity_access::public::{
    command_metadata::project_command_iam,
    selected_command_authority::CurrentServicePartsCommandAuthority, work_context::WorkScopeKind,
};
use erpx_server_inventory::public::service_parts_posting::CurrentServicePartsInventoryHistory;
use erpx_server_shared::public::{IdentifierSource, RecordId};
use erpx_server_workflow::public::{
    commands::{
        CommandActor, CommandDisposition, CommandField, PgCommandAuthority, PgCommandLedger,
        PgOwnerCommandEffects, WorkAuthority, WorkflowOutput, WorkflowRequest,
    },
    record_origin::{
        RawRecordOriginRequest, read_raw_record_origin, read_raw_record_origin_result_with_limit,
        verify_raw_record_origin,
    },
    service_parts_posting::CurrentServicePostingReceipt,
};

fn finance_mapping(m: &CommerceMapping) -> ServicePartsPostingMapping {
    ServicePartsPostingMapping {
        chart_id: m.chart_id.clone(),
        currency: m.currency.clone(),
        receivable_account_id: m.receivable_account_id.clone(),
        payable_account_id: m.payable_account_id.clone(),
        revenue_account_id: m.revenue_account_id.clone(),
        inventory_account_id: m.inventory_account_id.clone(),
        clearing_account_id: m.clearing_account_id.clone(),
        cogs_account_id: m.cogs_account_id.clone(),
        input_tax_account_id: m.input_tax_account_id.clone(),
        output_tax_account_id: m.output_tax_account_id.clone(),
        cash_account_id: m.cash_account_id.clone(),
    }
}
fn commerce_mapping(m: &ServicePartsPostingMapping) -> CommerceMapping {
    CommerceMapping {
        chart_id: m.chart_id.clone(),
        currency: m.currency.clone(),
        receivable_account_id: m.receivable_account_id.clone(),
        payable_account_id: m.payable_account_id.clone(),
        revenue_account_id: m.revenue_account_id.clone(),
        inventory_account_id: m.inventory_account_id.clone(),
        clearing_account_id: m.clearing_account_id.clone(),
        cogs_account_id: m.cogs_account_id.clone(),
        input_tax_account_id: m.input_tax_account_id.clone(),
        output_tax_account_id: m.output_tax_account_id.clone(),
        cash_account_id: m.cash_account_id.clone(),
    }
}
fn posting_changed(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
pub(crate) fn posting_actor(
    current: &AuthorizedWorkContext,
) -> Result<CommandActor, BusinessError> {
    let s = current.selection();
    let a = s.assignment();
    let w = s.workspace();
    Ok(CommandActor {
        user_id: current.user_id().clone(),
        assignment_id: a.id.clone(),
        role: a.role.clone(),
        company_id: w.company_id.clone(),
        branch_id: CommandField::Null,
        department_id: CommandField::Null,
        authority: CommandField::Value(WorkAuthority {
            session_id: current.session_id().as_str().into(),
            revision: s.revision(),
            assignment_version: a.version,
        }),
        iam: CommandField::Value(CommandJson::from_serializable(&project_command_iam(
            current,
        )?)?),
        assignment_scope_kind: match a.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        assignment_scope_node_id: a.scope_node_id.as_ref().map(|v| v.as_str().into()),
    })
}
impl PostingPurchases {
    pub(crate) async fn require_for(
        &self,
        c: &mut PgConnection,
        purpose: CapabilityPurpose,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        let mut reader = PgCompanyPurchaseReader::new(
            self.holding.clone(),
            self.company.clone(),
            c,
            &self.catalogue,
        );
        let witness = if purpose == CapabilityPurpose::History {
            &self.history
        } else {
            &self.new_work
        };
        reader.revalidate(witness, now).await?.require_allowed()?;
        Ok(())
    }
}
struct PostingCommandAuthority<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    request: &'a WorkflowRequest,
    purchases: &'a PostingPurchases,
    purpose: CapabilityPurpose,
}
impl<C: Clock + Sync> PgCommandAuthority for PostingCommandAuthority<'_, C> {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        request: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if request.key != self.request.key
            || request.operation != self.request.operation
            || request.record_id != self.request.record_id
            || request.input != self.request.input
        {
            return Err(invalid());
        }
        let selected = capture_service_parts_command_authority(
            c,
            self.current,
            ServicePartsAuthorityPurpose::Write,
            self.clock,
        )
        .await?;
        selected.require_current(c, self.clock).await?;
        Ok(posting_actor(self.current)?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        request: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        self.purchases
            .require_for(c, self.purpose, self.clock.now())
            .await?;
        if disposition == CommandDisposition::Replay {
            let identity = RawRecordOriginRequest {
                command_key: request.key.clone(),
                operation: request.operation.clone(),
                record_id: request.record_id.clone(),
                creator_user_id: self.current.user_id().as_str().into(),
                company_id: self.current.selection().workspace().company_id.clone(),
            };
            let (origin, result) =
                read_raw_record_origin_result_with_limit(c, identity, 2 * 1024 * 1024)
                    .await?
                    .ok_or_else(|| {
                        posting_changed(
                            "IDEMPOTENCY_CONFLICT",
                            "The original Service posting receipt is unavailable.",
                        )
                    })?;
            let valid:bool=sqlx::query_scalar("SELECT $1::jsonb->>'id'=$2 AND $1::jsonb->>'version'='1' AND $1::jsonb->>'status'=CASE $3::text WHEN 'commerce.service-parts-posting-setup.initialize' THEN 'configured' ELSE 'confirmed-existing' END AND $1::jsonb->'_servicePartsPostingSetup'->>'kind'='service-parts-posting-setup' AND $1::jsonb->'_servicePartsPostingSetup'->>'companyId'=$2 AND $1::jsonb->'_servicePartsPostingSetup'->>'commandId'=$4 AND $1::jsonb->'_servicePartsPostingSetup'->'mapping'->>'company_id'=$2")
                .bind(result.as_str()).bind(&request.record_id).bind(&request.operation).bind(&request.key).fetch_one(&mut *c).await?;
            if !valid {
                return Err(posting_changed(
                    "IDEMPOTENCY_CONFLICT",
                    "The original Service posting receipt is unavailable.",
                )
                .into());
            }
            verify_raw_record_origin(c, &origin).await?;
        }
        Ok(())
    }
}
struct PostingCommandSources<'a, C> {
    intent: ServicePartsPostingIntent,
    clock: &'a C,
    authority: CurrentServicePartsCommandAuthority<'a>,
    commerce: CurrentServicePartsPosting,
    inventory: CurrentServicePartsInventoryHistory,
    field: CurrentServicePartsFieldHistory,
    finance: CurrentServicePartsPostingReferences,
    receipt: CurrentServicePostingReceipt,
    purchases: PostingPurchases,
    marker: Option<String>,
}
impl<C: Clock + Sync> PgServicePartsPostingSource for PostingCommandSources<'_, C> {
    fn intent(&self) -> &ServicePartsPostingIntent {
        &self.intent
    }
    async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.authority.require_current(c, self.clock).await?;
        self.commerce.require_current(c).await?;
        self.inventory.require_current(c).await?;
        self.field.require_current(c).await?;
        self.finance.require_current(c).await?;
        if self.marker.is_none() {
            self.receipt.require_current(c).await?;
        } else {
            let receipt = read_service_posting_receipt(c, &self.intent.company).await?;
            let facts = receipt.facts().ok_or_else(invalid_business)?;
            if facts.command_id != self.intent.command_id
                || Some(facts.proof_json.as_str()) != self.marker.as_deref()
            {
                return Err(invalid());
            }
            receipt.require_current(c).await?;
        }
        self.purchases
            .require_for(c, CapabilityPurpose::NewWork, self.clock.now())
            .await
    }
    async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.authority.require_current(c, self.clock).await?;
        self.inventory.require_current(c).await?;
        self.field.require_current(c).await?;
        self.finance.require_current(c).await?;
        self.purchases
            .require_for(c, CapabilityPurpose::NewWork, self.clock.now())
            .await?;
        let receipt = read_service_posting_receipt(c, &self.intent.company).await?;
        let facts = receipt.facts().ok_or_else(invalid_business)?;
        if facts.command_id != self.intent.command_id
            || Some(facts.proof_json.as_str()) != self.marker.as_deref()
        {
            return Err(invalid());
        }
        receipt.require_current(c).await
    }
}
struct PostingEffects<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    input: &'a NormalizedServicePostingCommand,
    plan: Option<(PostingCommandSources<'a, C>, PreparedServicePartsPosting)>,
}
impl<C: Clock + Sync> PgOwnerCommandEffects for PostingEffects<'_, C> {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        request: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let view = read_service_posting_setup_for(
            c,
            self.current,
            self.clock,
            ServicePostingEnvironment::Development,
            ServicePartsAuthorityPurpose::Write,
        )
        .await?;
        if !view.schema_available {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "SERVICE_PARTS_SCHEMA_REQUIRED",
                "Install the reviewed Service source schema before preparing parts.",
            )
            .into());
        }
        if view.version == Some(1) {
            return Err(posting_changed(
                "SERVICE_PARTS_ALREADY_CONFIGURED",
                "Retain the original immutable Service parts posting source.",
            )
            .into());
        }
        if view.context_key.as_deref() != Some(self.input.context_key.as_str()) {
            return Err(posting_changed(
                "SERVICE_PARTS_POSTING_CONTEXT_CHANGED",
                "The actual mapping, references or selected Finance assignment changed.",
            )
            .into());
        }
        let company = &self.current.selection().workspace().company_id;
        let commerce = read_service_parts_posting(c, company).await?;
        let inventory =
            read_service_parts_inventory_history(c, company, &commerce.facts().warehouse_ids)
                .await?;
        let field = read_service_parts_field_history(c, company).await?;
        let receipt = read_service_posting_receipt(c, company).await?;
        let existing = commerce
            .facts()
            .mapping_json
            .as_deref()
            .map(serde_json::from_str::<serde_json::Value>)
            .transpose()
            .map_err(|_| invalid_business())?;
        let existing_mapping = existing.as_ref().and_then(posting_mapping);
        let finance =
            read_service_parts_posting_references(c, company, existing_mapping.as_ref()).await?;
        let mapping = match self.input.mode {
            PostingMode::Initialize => {
                if existing.is_some() {
                    return Err(posting_changed(
                        "COMMERCE_POSTING_ALREADY_CONFIGURED",
                        "Use explicit existing-mapping confirmation.",
                    )
                    .into());
                }
                if commerce.facts().has_commerce_history
                    || inventory.data().has_history
                    || field.data().has_history
                    || finance.data().finance_history
                {
                    return Err(posting_changed(
                        "SERVICE_PARTS_HISTORY_REQUIRES_REVIEW",
                        "Historical posting sources require their original owner review.",
                    )
                    .into());
                }
                let m = self.input.mapping.as_ref().ok_or_else(wire_invalid)?;
                let f = finance_mapping(m);
                finance.validate_mapping(&f)?;
                finance.validate_defaults(&f, self.input.expected_sales_defaults.as_ref())?;
                m.clone()
            }
            PostingMode::ConfirmExisting => {
                let f = existing_mapping.ok_or_else(|| {
                    posting_changed(
                        "SERVICE_PARTS_MAPPING_REQUIRED",
                        "Choose the actual configured Company mapping.",
                    )
                })?;
                finance.validate_mapping(&f)?;
                commerce_mapping(&f)
            }
        };
        let authority = capture_service_parts_command_authority(
            c,
            self.current,
            ServicePartsAuthorityPurpose::Write,
            self.clock,
        )
        .await?;
        let purchases = PostingPurchases::capture(c, self.current, self.clock.now()).await?;
        purchases
            .require_for(c, CapabilityPurpose::NewWork, self.clock.now())
            .await?;
        let intent = ServicePartsPostingIntent {
            company: company.clone(),
            command_id: request.key.clone(),
            actor_id: self.current.user_id().as_str().into(),
            created_at: self.clock.now().get(),
            mode: self.input.mode,
            mapping,
        };
        let mut source = PostingCommandSources {
            intent,
            clock: self.clock,
            authority,
            commerce,
            inventory,
            field,
            finance,
            receipt,
            purchases,
            marker: None,
        };
        let plan = PreparedServicePartsPosting::prepare(c, &source).await?;
        source.marker = Some(plan.receipt_marker_json().into());
        let status = if self.input.mode == PostingMode::Initialize {
            "configured"
        } else {
            "confirmed-existing"
        };
        let result = CommandJson::parse(&format!(
            "{{\"id\":{},\"status\":\"{status}\",\"version\":1,\"_servicePartsPostingSetup\":{}}}",
            serde_json::to_string(company.as_str()).map_err(|_| invalid_business())?,
            plan.receipt_marker_json()
        ))?;
        self.plan = Some((source, plan));
        Ok(WorkflowOutput {
            version: 1,
            result,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let (source, plan) = self.plan.as_ref().ok_or_else(invalid_business)?;
        plan.apply(c, source).await
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let (source, plan) = self.plan.as_ref().ok_or_else(invalid_business)?;
        plan.require_current(c, source).await
    }
}
struct PostingIds;
impl IdentifierSource for PostingIds {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("UUID record identity")
    }
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServicePostingResult {
    pub id: String,
    pub status: String,
    pub version: u8,
}
pub struct ServicePostingOutcome {
    pub result: ServicePostingResult,
    pub replayed: bool,
}
/// Run only inside the genuine selected-Identity coordinator after its source
/// preparation. Workflow owns the canonical receipt and exact replay.
pub async fn execute_service_posting_setup<C: Clock + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    environment: ServicePostingEnvironment,
    key: &str,
    input: &NormalizedServicePostingCommand,
) -> Result<ServicePostingOutcome, Error> {
    if environment != ServicePostingEnvironment::Development {
        return Err(invalid());
    }
    let selected = capture_service_parts_command_authority(
        c,
        current,
        ServicePartsAuthorityPurpose::Write,
        clock,
    )
    .await?;
    if !matches!(
        current.selection().assignment().scope_kind,
        WorkScopeKind::Holding
    ) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "COMPANY_FINANCE_REQUIRED",
            "Use the original Company Finance assignment.",
        )
        .into());
    }
    selected.require_current(c, clock).await?;
    if !service_posting_schema_available(c).await? {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "SERVICE_PARTS_SCHEMA_REQUIRED",
            "Install the reviewed Service source schema before preparing parts.",
        )
        .into());
    }
    let operation = match input.mode {
        PostingMode::Initialize => "commerce.service-parts-posting-setup.initialize",
        PostingMode::ConfirmExisting => "commerce.service-parts-posting-setup.confirm-existing",
    };
    let request = WorkflowRequest {
        key: key.into(),
        operation: operation.into(),
        record_id: current.selection().workspace().company_id.as_str().into(),
        input: input.workflow_input.clone(),
    };
    let original = read_raw_record_origin(
        c,
        RawRecordOriginRequest {
            command_key: key.into(),
            operation: operation.into(),
            record_id: request.record_id.clone(),
            creator_user_id: current.user_id().as_str().into(),
            company_id: current.selection().workspace().company_id.clone(),
        },
    )
    .await?;
    let purpose = if original.is_some() {
        CapabilityPurpose::History
    } else {
        CapabilityPurpose::NewWork
    };
    let purchases = PostingPurchases::capture(c, current, clock.now()).await?;
    purchases.require_for(c, purpose, clock.now()).await?;
    let mut authority = PostingCommandAuthority {
        current,
        clock,
        request: &request,
        purchases: &purchases,
        purpose,
    };
    let mut effects = PostingEffects {
        current,
        clock,
        input,
        plan: None,
    };
    let outcome = PgCommandLedger::new(
        current.selection().workspace().company_id.clone(),
        c,
        &mut authority,
        clock,
        &mut PostingIds,
    )
    .execute(&request, &mut effects)
    .await?;
    let status = outcome
        .result
        .property("status")?
        .ok_or_else(invalid_business)?
        .string_utf8_storage()?
        .ok_or_else(invalid_business)?;
    let expected_status = match input.mode {
        PostingMode::Initialize => "configured",
        PostingMode::ConfirmExisting => "confirmed-existing",
    };
    if status != expected_status {
        return Err(invalid());
    }
    Ok(ServicePostingOutcome {
        result: ServicePostingResult {
            id: request.record_id,
            status,
            version: 1,
        },
        replayed: outcome.replayed,
    })
}

use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        HoldingWorkAuthorizer, SelectedContextRead, SelectedIngressFuture,
        SelectedTransportCallbacks, SelectedWorkIngress, WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use std::sync::Arc;
struct ServicePostingReadIngress {
    selected: SelectedContextRead,
    holding: HoldingId,
    company: CompanyId,
}
enum ServicePostingReadDenial {
    View(Box<ServicePostingReadResponse>),
    Ingress(BusinessError),
}
fn service_posting_read_denied(
    input: &ServicePostingReadIngress,
    error: BusinessError,
) -> Result<ServicePostingReadDenial, BusinessError> {
    if error.code != "PERMISSION_REQUIRED" {
        return Err(error);
    }
    if input.selected.holding_id() != &input.holding
        || input.selected.selection().workspace().company_id != input.company
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "SERVICE_POSTING_SCOPE_REQUIRED",
            "Use the current Company.",
        ));
    }
    Ok(ServicePostingReadDenial::View(Box::new(
        ServicePostingReadResponse {
            data: unavailable(&input.company, "authority-required"),
            context: crate::field_service_reservation_read::selected_read_context(&input.selected)?,
        },
    )))
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServicePostingReadResponse {
    pub data: ServicePostingView,
    pub context: crate::field_service_reservation_read::FieldServiceRenderedContext,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServicePostingCommandResponse {
    pub data: ServicePostingResult,
    pub context: crate::field_service_reservation_read::FieldServiceRenderedContext,
}
/// Normal selected Account ingress, including the original typed unavailable
/// view when an otherwise current actor lacks Finance read permission.
pub struct ServicePostingTarget<C> {
    pub holding: HoldingId,
    pub company: CompanyId,
    pub clock: Arc<C>,
    pub environment: ServicePostingEnvironment,
}

pub async fn read_selected_service_posting_setup<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    request: Input,
    target: ServicePostingTarget<C>,
) -> Result<ServicePostingReadResponse, Error>
where
    C: Clock + Send + Sync + 'static,
    Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
{
    let ServicePostingTarget {
        holding,
        company,
        clock,
        environment,
    } = target;
    if environment != ServicePostingEnvironment::Development {
        return Err(invalid());
    }
    let expected_holding = holding.clone();
    let expected_company = company.clone();
    let callbacks=SelectedTransportCallbacks::<_,ServicePostingReadIngress,ServicePostingReadDenial,_>::new::<ServicePostingReadResponse>(
        move|selected:&SelectedWorkIngress<'_>|->SelectedIngressFuture<ServicePostingReadIngress,ServicePostingReadDenial>{
            let result=selected.read_context().map(|selected|ServicePostingReadIngress{selected,holding:expected_holding.clone(),company:expected_company.clone()}).map_err(ServicePostingReadDenial::Ingress);
            Box::pin(async move{result})
        },service_posting_read_denied,
        move|c,current,input:ServicePostingReadIngress|{let clock=clock.clone();Box::pin(async move{
            if current.holding_id()!=&input.holding||current.selection().workspace().company_id!=input.company {return Err(BusinessError::new(FailureKind::Forbidden,"SERVICE_POSTING_SCOPE_REQUIRED","Use the current Company.").into());}
            let context=crate::field_service_reservation_read::context(current)?;
            let data=read_service_posting_setup(c,current,clock.as_ref(),environment).await?;
            Ok(Ok(ServicePostingReadResponse { data, context }))
        })});
    match authorizer
        .execute_with_selected_transport_ingress_deferred(
            host,
            proof,
            move || {
                let mut request = request()?;
                request.mutation = false;
                Ok(request)
            },
            (
                WorkAuthorizationRequest {
                    permission: "finance.read".into(),
                    module: None,
                    destination: "services/field-service".into(),
                    api: true,
                },
                ServicePostingSources,
            ),
            callbacks,
        )
        .await?
    {
        Ok(view) => Ok(view),
        Err(ServicePostingReadDenial::View(view)) => Ok(*view),
        Err(ServicePostingReadDenial::Ingress(error)) => Err(error.into()),
    }
}

/// Selected Identity is admitted before command body normalization. The caller
/// still owns the public Origin check and supplies the original idempotency key.
pub struct ServicePostingTransportCommand {
    pub key: Result<String, BusinessError>,
    pub body: axum::body::Body,
    pub mode: PostingMode,
}

pub async fn execute_selected_service_posting_setup<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    request: Input,
    target: ServicePostingTarget<C>,
    command: ServicePostingTransportCommand,
) -> Result<ServicePostingCommandResponse, Error>
where
    C: Clock + Send + Sync + 'static,
    Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
{
    let ServicePostingTarget {
        holding,
        company,
        clock,
        environment,
    } = target;
    let ServicePostingTransportCommand { key, body, mode } = command;
    if environment != ServicePostingEnvironment::Development {
        return Err(invalid());
    }
    let body = Arc::new(tokio::sync::Mutex::new(Some(body)));
    let cached = Arc::new(tokio::sync::OnceCell::new());
    let callbacks = SelectedTransportCallbacks::<
        _,
        (String, NormalizedServicePostingCommand),
        BusinessError,
        _,
    >::new::<ServicePostingCommandResponse>(
        move |_: &SelectedWorkIngress<'_>| -> SelectedIngressFuture<
            (String, NormalizedServicePostingCommand),
            BusinessError,
        > {
            let body = body.clone();
            let cached = cached.clone();
            let key = key.clone();
            Box::pin(async move {
                let key = key?;
                if key.is_empty() {
                    return Err(BusinessError::invalid(
                        "COMMAND_KEY_REQUIRED",
                        "Idempotency-Key is required.",
                    ));
                }
                let bytes = cached
                    .get_or_init(|| async {
                        let body = body.lock().await.take()?;
                        axum::body::to_bytes(body, 2 * 1024 * 1024)
                            .await
                            .ok()
                            .map(|v| v.to_vec())
                    })
                    .await
                    .as_ref()
                    .ok_or_else(invalid_business)?;
                let text = String::from_utf8_lossy(bytes);
                let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
                let json = CommandJson::parse(text)
                    .map_err(|_| BusinessError::invalid("INVALID_JSON", "Use valid JSON."))?;
                let input = normalize_service_posting_command(&json, mode)?;
                Ok((key, input))
            })
        },
        |_, error| Err(error),
        move |c, current, (key, input): (String, NormalizedServicePostingCommand)| {
            let clock = clock.clone();
            let holding = holding.clone();
            let company = company.clone();
            Box::pin(async move {
                if current.holding_id() != &holding
                    || current.selection().workspace().company_id != company
                {
                    return Err(BusinessError::new(
                        FailureKind::Forbidden,
                        "SERVICE_POSTING_SCOPE_REQUIRED",
                        "Use the current Company.",
                    )
                    .into());
                }
                let context = crate::field_service_reservation_read::context(current)?;
                let outcome = execute_service_posting_setup(
                    c,
                    current,
                    clock.as_ref(),
                    environment,
                    &key,
                    &input,
                )
                .await?;
                Ok(Ok(ServicePostingCommandResponse {
                    data: outcome.result,
                    context,
                }))
            })
        },
    );
    match authorizer
        .execute_with_selected_transport_ingress_deferred(
            host,
            proof,
            move || {
                let mut request = request()?;
                request.mutation = true;
                Ok(request)
            },
            (
                WorkAuthorizationRequest {
                    permission: "finance.write".into(),
                    module: None,
                    destination: "services/field-service".into(),
                    api: true,
                },
                ServicePostingSources,
            ),
            callbacks,
        )
        .await?
    {
        Ok(value) => Ok(value),
        Err(error) => Err(error.into()),
    }
}
