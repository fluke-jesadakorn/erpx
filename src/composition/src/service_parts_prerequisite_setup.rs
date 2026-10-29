//! Native Service parts prerequisite read composition. Owner witnesses retain
//! all current sources in the selected Account transaction.
use crate::service_parts_posting_setup::{
    PostingHistoryImages, PostingPurchases, RetainedWorkAuthority, ServicePostingEnvironment,
    retained_history_image,
};
use erpx_server_commerce::public::service_parts_posting::read_service_parts_posting;
use erpx_server_field_service::public::service_parts_posting::read_service_parts_field_history;
use erpx_server_finance::public::{
    service_parts_posting::read_service_parts_prerequisite_finance,
    service_parts_prerequisites::{
        lock_service_parts_prerequisite_references, read_service_parts_prerequisite_references,
    },
};
use erpx_server_identity_access::public::{
    effective_authority::{AuthorizedWorkContext, HoldingWorkSourcePreparation},
    selected_command_authority::{
        ServicePartsAuthorityPurpose, capture_service_parts_command_authority,
    },
    work_context::WorkScopeKind,
};
use erpx_server_inventory::public::service_parts_posting::read_service_parts_inventory_history;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use erpx_server_workflow::public::{
    commands::CommandJson, service_parts_posting::read_service_posting_receipt,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
#[derive(Clone, Copy)]
pub struct ServicePrerequisiteSources;
impl HoldingWorkSourcePreparation for ServicePrerequisiteSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_service_posting_receipts(c).await?;
            lock_service_parts_posting_sources(c).await?;
            lock_service_parts_inventory_history(c).await?;
            lock_service_parts_field_history(c).await?;
            lock_service_parts_prerequisite_finance(c).await?;
            lock_service_parts_prerequisite_references(c).await?;
            Ok(())
        })
    }
}
#[derive(Serialize)]
struct ChartRow {
    id: String,
    name: String,
    base_currency: String,
    status: String,
    version: i64,
    source: String,
}
#[derive(Serialize)]
struct AccountRow {
    id: String,
    chart_id: String,
    code: String,
    name: String,
    category: String,
    currency: Option<String>,
    control_kind: String,
    status: String,
    version: i64,
    source: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PostingState {
    configured: bool,
    history: bool,
    chart_id: Option<String>,
    mapping_image: String,
    proof_image: String,
    image: String,
    unsupported_existing_mapping: bool,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServicePrerequisitesView {
    pub company_id: String,
    pub available: bool,
    pub configured: Option<bool>,
    pub context_key: Option<String>,
    pub can_create_chart: bool,
    pub can_create_account: bool,
    pub can_create_period: bool,
    pub can_confirm_rate: bool,
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub charts: Vec<serde_json::Value>,
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub accounts: Vec<serde_json::Value>,
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub periods: Vec<serde_json::Value>,
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub rates: Vec<serde_json::Value>,
    pub unavailable_reason: Option<&'static str>,
}
fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SERVICE_PARTS_POSTING_CONTEXT_UNAVAILABLE",
        "The original Service setup context is unavailable.",
    )
}
fn unavailable(company: &str, reason: &'static str) -> ServicePrerequisitesView {
    ServicePrerequisitesView {
        company_id: company.into(),
        available: false,
        configured: None,
        context_key: None,
        can_create_chart: false,
        can_create_account: false,
        can_create_period: false,
        can_confirm_rate: false,
        charts: vec![],
        accounts: vec![],
        periods: vec![],
        rates: vec![],
        unavailable_reason: Some(reason),
    }
}
fn str_field(row: &serde_json::Value, key: &str) -> Result<String, BusinessError> {
    row.get(key)
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .ok_or_else(invalid)
}
fn int_field(row: &serde_json::Value, key: &str) -> Result<i64, BusinessError> {
    safe_integer(row.get(key).and_then(|v| v.as_i64()).ok_or_else(invalid)?)
}
fn safe_integer(value: i64) -> Result<i64, BusinessError> {
    const JS_MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
    if (-JS_MAX_SAFE_INTEGER..=JS_MAX_SAFE_INTEGER).contains(&value) {
        Ok(value)
    } else {
        Err(invalid())
    }
}
fn charts_and_accounts(image: &str) -> Result<(Vec<ChartRow>, Vec<AccountRow>), BusinessError> {
    let (inner, _, _): (String, String, String) =
        serde_json::from_str(image).map_err(|_| invalid())?;
    let (charts, accounts): (Vec<String>, Vec<String>) =
        serde_json::from_str(&inner).map_err(|_| invalid())?;
    let charts = charts
        .into_iter()
        .map(|source| {
            let row: serde_json::Value = serde_json::from_str(&source).map_err(|_| invalid())?;
            Ok(ChartRow {
                id: str_field(&row, "id")?,
                name: str_field(&row, "name")?,
                base_currency: str_field(&row, "base_currency")?,
                status: str_field(&row, "status")?,
                version: int_field(&row, "version")?,
                source,
            })
        })
        .collect::<Result<Vec<_>, BusinessError>>()?;
    let accounts = accounts
        .into_iter()
        .map(|source| {
            let row: serde_json::Value = serde_json::from_str(&source).map_err(|_| invalid())?;
            Ok(AccountRow {
                id: str_field(&row, "id")?,
                chart_id: str_field(&row, "chart_id")?,
                code: str_field(&row, "code")?,
                name: str_field(&row, "name")?,
                category: str_field(&row, "category")?,
                currency: row
                    .get("currency")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
                control_kind: str_field(&row, "control_kind")?,
                status: str_field(&row, "status")?,
                version: int_field(&row, "version")?,
                source,
            })
        })
        .collect::<Result<Vec<_>, BusinessError>>()?;
    Ok((charts, accounts))
}
/// Caller must first acquire `ServicePrerequisiteSources` in the selected unit.
pub async fn read_service_prerequisites<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    environment: ServicePostingEnvironment,
) -> Result<ServicePrerequisitesView, Error> {
    read_service_prerequisites_for(c, current, clock, environment, true).await
}
async fn read_service_prerequisites_for<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    environment: ServicePostingEnvironment,
    require_history: bool,
) -> Result<ServicePrerequisitesView, Error> {
    let company = &current.selection().workspace().company_id;
    if environment != ServicePostingEnvironment::Development {
        return Err(invalid().into());
    }
    let scope = current.selection().workspace();
    if scope.branch_id.is_some()
        || scope.department_id.is_some()
        || !matches!(
            current.selection().assignment().scope_kind,
            WorkScopeKind::Holding
        )
        || !current.has_permission("finance.read")
        || !(current.has_permission("finance.write") || current.has_permission("finance.approve"))
    {
        return Ok(unavailable(company.as_str(), "authority-required"));
    }
    let actor = capture_service_parts_command_authority(
        c,
        current,
        ServicePartsAuthorityPurpose::Read,
        clock,
    )
    .await?;
    let purchases = PostingPurchases::capture(c, current, clock.now()).await?;
    if require_history && !purchases.history_allowed {
        purchases.require_current(c, clock.now()).await?;
        actor.require_current(c, clock).await?;
        return Ok(unavailable(company.as_str(), "purchase-required"));
    }
    let commerce = read_service_parts_posting(c, company).await?;
    let inventory =
        read_service_parts_inventory_history(c, company, &commerce.facts().warehouse_ids).await?;
    let field = read_service_parts_field_history(c, company).await?;
    let receipt = read_service_posting_receipt(c, company).await?;
    let finance = read_service_parts_prerequisite_finance(c, company).await?;
    let references = read_service_parts_prerequisite_references(c, company).await?;
    let (charts, accounts) = charts_and_accounts(&finance.data().reference_image)?;
    let (inv, fld, fin, refs) = (
        inventory.data(),
        field.data(),
        finance.data(),
        references.data(),
    );
    let h = &fin.history_image;
    let history = commerce.facts().has_commerce_history
        || inv.has_history
        || fld.has_history
        || fin.finance_history;
    let image = retained_history_image(
        c,
        &PostingHistoryImages {
            orders: &commerce.facts().commerce_orders_json,
            stock: &inv.stock_json,
            ledger: &inv.ledger_json,
            counts: &inv.counts_json,
            consumptions: &inv.consumptions_json,
            invoices: h.invoices.as_deref(),
            payments: h.payments.as_deref(),
            journals: h.journals.as_deref(),
            service: &fld.service_json,
        },
    )
    .await?;
    let mapping = commerce.facts().mapping_json.as_deref();
    let proof = if commerce.facts().schema_available {
        receipt.facts().map(|v| v.proof_json.as_str())
    } else {
        None
    };
    let mapping_value: Option<serde_json::Value> = mapping
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| invalid())?;
    let chart_id = mapping_value
        .as_ref()
        .and_then(|v| v.get("chart_id"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let unsupported = mapping_value.as_ref().is_some_and(|m| {
        charts
            .iter()
            .find(|ch| Some(&ch.id) == chart_id.as_ref())
            .is_none_or(|ch| {
                Some(ch.base_currency.as_str()) != m.get("currency").and_then(|v| v.as_str())
            })
    });
    let json_image = |v: Option<&str>| -> Result<String, BusinessError> {
        match v {
            Some(s) => Ok(CommandJson::parse(s)?.as_str().into()),
            None => Ok("null".into()),
        }
    };
    let state = PostingState {
        configured: mapping.is_some(),
        history,
        chart_id,
        mapping_image: json_image(mapping)?,
        proof_image: json_image(proof)?,
        image,
        unsupported_existing_mapping: unsupported,
    };
    // These numeric projections participate in the retained JavaScript context
    // key as well as the response. Reject values JavaScript cannot represent
    // exactly while preserving the original PostgreSQL source JSON unchanged.
    for period in &refs.periods {
        safe_integer(period.version)?;
    }
    for rate in &refs.rates {
        safe_integer(rate.confirmed_at)?;
    }
    let rows = CommandJson::from_serializable(&(
        &charts,
        &accounts,
        &refs.periods,
        &refs.rates,
        &refs.money,
    ))?;
    let selection = current.selection();
    let grant = selection.assignment();
    let iam = serde_json::to_value(actor.iam()).map_err(|_| invalid())?;
    let authority = RetainedWorkAuthority {
        session_id: current.session_id().as_str(),
        revision: selection.revision(),
        assignment_version: grant.version,
    };
    let key_input = CommandJson::from_serializable(&(
        current.holding_id().as_str(),
        company.as_str(),
        current.user_id().as_str(),
        grant.id.as_str(),
        grant.role.as_str(),
        authority,
        iam.get("queuedAssignmentVersion"),
        iam.get("identityVersion"),
        iam.get("roleRevision"),
        iam.get("hrAppointmentVersion"),
        rows.as_str(),
        &state,
    ))?;
    if key_input.as_str().len() > 2 * 1024 * 1024 {
        return Err(invalid().into());
    }
    let key = hex::encode(Sha256::digest(key_input.as_str().as_bytes()));
    let active = purchases.new_work_allowed;
    let write = current.has_permission("finance.write");
    let view=ServicePrerequisitesView{company_id:company.as_str().into(),available:true,configured:Some(state.configured),context_key:Some(key),can_create_chart:active&&write&&!state.configured&&!state.history,can_create_account:active&&write&&!state.unsupported_existing_mapping,can_create_period:active&&write,can_confirm_rate:active&&current.has_permission("finance.approve"),charts:charts.iter().filter(|x|x.status=="active").map(|x|serde_json::json!({"id":x.id,"name":x.name,"baseCurrency":x.base_currency})).collect(),accounts:accounts.iter().filter(|x|x.status=="active"&&x.currency.is_some()).map(|x|serde_json::json!({"id":x.id,"chartId":x.chart_id,"code":x.code,"name":x.name,"category":x.category,"currency":x.currency,"controlKind":x.control_kind})).collect(),periods:refs.periods.iter().map(|x|serde_json::json!({"id":x.id,"name":x.name,"startDate":x.start_date.chars().take(10).collect::<String>(),"endDate":x.end_date.chars().take(10).collect::<String>(),"state":x.state,"version":x.version})).collect(),rates:refs.rates.iter().map(|x|{let value=if x.currency_per_usd.contains('.') {x.currency_per_usd.trim_end_matches('0').trim_end_matches('.')}else{&x.currency_per_usd};serde_json::json!({"id":x.id,"currency":x.currency,"effectiveDate":x.effective_date.chars().take(10).collect::<String>(),"currencyPerUsd":value,"confirmedAt":x.confirmed_at})}).collect(),unavailable_reason:if !state.configured&&state.history{Some("history-requires-review")}else{None}};
    serde_json::to_writer(
        &mut crate::field_service_reservation_read::ReplyBudget(0),
        &view,
    )
    .map_err(|_| invalid())?;
    commerce.require_current(c).await?;
    inventory.require_current(c).await?;
    field.require_current(c).await?;
    receipt.require_current(c).await?;
    finance.require_current(c).await?;
    references.require_current(c).await?;
    purchases.require_current(c, clock.now()).await?;
    actor.require_current(c, clock).await?;
    Ok(view)
}

use crate::service_parts_posting_setup::trimmed_units;
use erpx_server_finance::public::service_parts_prerequisites::{
    ServicePrerequisiteInput, ServicePrerequisiteKind,
};
#[derive(Clone)]
pub struct NormalizedPrerequisiteCommand {
    pub context_key: String,
    pub input: ServicePrerequisiteInput,
    pub input_json: CommandJson,
    pub workflow_input: CommandJson,
}
fn wire_invalid() -> BusinessError {
    BusinessError::invalid(
        "VALIDATION_FAILED",
        "Review the Service prerequisite input.",
    )
}
fn string_field(
    value: &CommandJson,
    name: &str,
    min: usize,
    max: usize,
    trim: bool,
) -> Result<(String, CommandJson), BusinessError> {
    trimmed_units(
        &value.property(name)?.ok_or_else(wire_invalid)?,
        min,
        max,
        trim,
    )
    .map_err(|_| wire_invalid())
}
fn strict_keys(value: &CommandJson, allowed: &[&str]) -> Result<(), BusinessError> {
    if value
        .object_keys()?
        .ok_or_else(wire_invalid)?
        .iter()
        .any(|k| !allowed.contains(&k.as_str()))
    {
        return Err(wire_invalid());
    }
    Ok(())
}
fn member(name: &str, value: &CommandJson) -> String {
    format!("\"{name}\":{}", value.as_str())
}
fn upper_currency(value: &CommandJson, name: &str) -> Result<(String, CommandJson), BusinessError> {
    let raw = value
        .property(name)?
        .ok_or_else(wire_invalid)?
        .string_utf16_units()?
        .ok_or_else(wire_invalid)?;
    if raw.len() != 3 {
        return Err(wire_invalid());
    }
    let mut out = Vec::new();
    for c in char::decode_utf16(raw) {
        match c {
            Ok(c) => {
                for upper in c.to_uppercase() {
                    let mut pair = [0; 2];
                    out.extend_from_slice(upper.encode_utf16(&mut pair));
                }
            }
            Err(e) => out.push(e.unpaired_surrogate()),
        }
    }
    Ok((
        String::from_utf16_lossy(&out),
        CommandJson::from_utf16_string(&out)?,
    ))
}
fn valid_date(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || b.iter()
            .enumerate()
            .any(|(i, c)| i != 4 && i != 7 && !c.is_ascii_digit())
    {
        return false;
    }
    let number = |range: std::ops::Range<usize>| -> u32 {
        range.fold(0, |n, i| n * 10 + u32::from(b[i] - b'0'))
    };
    let year = number(0..4);
    let month = number(5..7);
    let day = number(8..10);
    let limit = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=limit).contains(&day)
}
/// Original strict Zod schemas, including defaults and omitted optional fields.
pub fn normalize_service_prerequisite(
    input: &CommandJson,
    kind: &str,
) -> Result<NormalizedPrerequisiteCommand, BusinessError> {
    strict_keys(input, &["expectedVersion", "contextKey", "input"])?;
    if input
        .property("expectedVersion")?
        .ok_or_else(wire_invalid)?
        .as_str()
        != "0"
    {
        return Err(wire_invalid());
    }
    let (context_key, context_json) = string_field(input, "contextKey", 64, 64, false)?;
    if !context_key
        .bytes()
        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(wire_invalid());
    }
    let body = input.property("input")?.ok_or_else(wire_invalid)?;
    let mut parts = Vec::new();
    let parsed = match kind {
        "chart" => {
            strict_keys(&body, &["name", "baseCurrency", "accountingBasis"])?;
            let (name, n) = string_field(&body, "name", 1, 160, true)?;
            let (base_currency, b) = upper_currency(&body, "baseCurrency")?;
            let (accounting_basis, a) = if body.property("accountingBasis")?.is_some() {
                string_field(&body, "accountingBasis", 0, 40, true)?
            } else {
                ("IFRS".into(), CommandJson::parse("\"IFRS\"")?)
            };
            parts.extend([
                member("name", &n),
                member("baseCurrency", &b),
                member("accountingBasis", &a),
            ]);
            ServicePrerequisiteInput::Chart {
                name,
                base_currency,
                accounting_basis,
            }
        }
        "account" => {
            strict_keys(
                &body,
                &[
                    "chartId",
                    "code",
                    "name",
                    "category",
                    "parentId",
                    "currency",
                    "isControl",
                    "controlKind",
                ],
            )?;
            let (chart_id, ch) = string_field(&body, "chartId", 1, 2 * 1024 * 1024, false)?;
            let (code, co) = string_field(&body, "code", 1, 40, true)?;
            let (name, n) = string_field(&body, "name", 1, 160, true)?;
            let (category, cat) = string_field(&body, "category", 1, 32, false)?;
            if ![
                "asset",
                "liability",
                "equity",
                "revenue",
                "expense",
                "other_income",
                "other_expense",
            ]
            .contains(&category.as_str())
            {
                return Err(wire_invalid());
            }
            parts.extend([
                member("chartId", &ch),
                member("code", &co),
                member("name", &n),
                member("category", &cat),
            ]);
            let parent_id = match body.property("parentId")? {
                None => None,
                Some(v) if v.as_str() == "null" => {
                    parts.push("\"parentId\":null".into());
                    None
                }
                Some(v) => {
                    let (p, j) = trimmed_units(&v, 0, 128, false).map_err(|_| wire_invalid())?;
                    parts.push(member("parentId", &j));
                    Some(p)
                }
            };
            let currency = if body.property("currency")?.is_some() {
                let (v, j) = upper_currency(&body, "currency")?;
                parts.push(member("currency", &j));
                Some(v)
            } else {
                None
            };
            let is_control = match body.property("isControl")? {
                None => false,
                Some(v) if v.as_str() == "true" => true,
                Some(v) if v.as_str() == "false" => false,
                _ => return Err(wire_invalid()),
            };
            parts.push(format!("\"isControl\":{is_control}"));
            let (control_kind, k) = if body.property("controlKind")?.is_some() {
                string_field(&body, "controlKind", 1, 20, false)?
            } else {
                ("none".into(), CommandJson::parse("\"none\"")?)
            };
            if !["none", "receivable", "payable", "bank"].contains(&control_kind.as_str()) {
                return Err(wire_invalid());
            }
            parts.push(member("controlKind", &k));
            ServicePrerequisiteInput::Account {
                chart_id,
                code,
                name,
                category,
                parent_id,
                currency,
                is_control,
                control_kind,
            }
        }
        "period" => {
            strict_keys(&body, &["name", "startDate", "endDate"])?;
            let (name, n) = string_field(&body, "name", 1, 100, true)?;
            let (start_date, s) = string_field(&body, "startDate", 10, 10, false)?;
            let (end_date, e) = string_field(&body, "endDate", 10, 10, false)?;
            if !valid_date(&start_date) || !valid_date(&end_date) {
                return Err(wire_invalid());
            }
            parts.extend([
                member("name", &n),
                member("startDate", &s),
                member("endDate", &e),
            ]);
            ServicePrerequisiteInput::Period {
                name,
                start_date,
                end_date,
            }
        }
        "rate" => {
            strict_keys(
                &body,
                &["currency", "effectiveDate", "currencyPerUsd", "note"],
            )?;
            let (currency, cu) = string_field(&body, "currency", 3, 3, false)?;
            let rules = erpx_server_finance::public::embedded_currency_rules()
                .map_err(|_| wire_invalid())?;
            if currency == "USD"
                || !currency.bytes().all(|x| x.is_ascii_uppercase())
                || !rules.contains(&currency)
            {
                return Err(wire_invalid());
            }
            let (effective_date, date) = string_field(&body, "effectiveDate", 10, 10, false)?;
            if !valid_date(&effective_date) {
                return Err(wire_invalid());
            }
            let (currency_per_usd, rate) = string_field(&body, "currencyPerUsd", 1, 39, false)?;
            let mut split = currency_per_usd.split('.');
            let whole = split.next().unwrap_or("");
            let fraction = split.next();
            if split.next().is_some()
                || whole.is_empty()
                || whole.len() > 20
                || !whole.bytes().all(|b| b.is_ascii_digit())
                || fraction.is_some_and(|f| {
                    f.is_empty() || f.len() > 18 || !f.bytes().all(|b| b.is_ascii_digit())
                })
            {
                return Err(wire_invalid());
            }
            let (note, n) = if body.property("note")?.is_some() {
                string_field(&body, "note", 0, 500, true)?
            } else {
                ("".into(), CommandJson::parse("\"\"")?)
            };
            parts.extend([
                member("currency", &cu),
                member("effectiveDate", &date),
                member("currencyPerUsd", &rate),
                member("note", &n),
            ]);
            ServicePrerequisiteInput::Rate {
                currency,
                effective_date,
                currency_per_usd,
                note,
            }
        }
        _ => return Err(wire_invalid()),
    };
    let input_json = CommandJson::parse(&format!("{{{}}}", parts.join(",")))?;
    let workflow_input = CommandJson::parse(&format!(
        "{{\"expectedVersion\":0,\"contextKey\":{},\"input\":{}}}",
        context_json.as_str(),
        input_json.as_str()
    ))?;
    Ok(NormalizedPrerequisiteCommand {
        context_key,
        input: parsed,
        input_json,
        workflow_input,
    })
}

use crate::service_parts_posting_setup::posting_actor;
use erpx_server_commerce::public::service_parts_posting::{
    CurrentServicePartsPosting, lock_service_parts_posting_sources,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, lock_commercial_authority_sources,
};
use erpx_server_field_service::public::service_parts_posting::{
    CurrentServicePartsFieldHistory, lock_service_parts_field_history,
};
use erpx_server_finance::public::{
    service_parts_posting::{
        CurrentServicePartsPostingReferences, lock_service_parts_prerequisite_finance,
    },
    service_parts_prerequisites::{
        CurrentLegacyFinanceCommand, CurrentServicePartsPrerequisiteReferences,
        PreparedServicePrerequisite, ServicePrerequisiteIntent, flush_service_prerequisite_effects,
        lock_service_prerequisite_writer, read_legacy_finance_command,
    },
};
use erpx_server_identity_access::public::selected_command_authority::CurrentServicePartsCommandAuthority;
use erpx_server_inventory::public::service_parts_posting::{
    CurrentServicePartsInventoryHistory, lock_service_parts_inventory_history,
};
use erpx_server_shared::public::{IdentifierSource, RecordId};
use erpx_server_workflow::public::{
    commands::{
        CommandActor, CommandDisposition, PgCommandAuthority, PgCommandLedger,
        PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
    },
    service_parts_posting::{
        CurrentServicePostingReceipt, lock_service_posting_receipts,
        read_service_prerequisite_receipt,
    },
};
#[derive(Clone)]
pub struct ServicePrerequisiteWriteSources(pub ServicePrerequisiteKind);
impl HoldingWorkSourcePreparation for ServicePrerequisiteWriteSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_service_posting_receipts(c).await?;
            lock_service_prerequisite_writer(c, self.0).await?;
            lock_service_parts_posting_sources(c).await?;
            lock_service_parts_inventory_history(c).await?;
            lock_service_parts_field_history(c).await?;
            lock_service_parts_prerequisite_finance(c).await?;
            lock_service_parts_prerequisite_references(c).await?;
            Ok(())
        })
    }
}
fn operation(input: &ServicePrerequisiteInput) -> &'static str {
    match input {
        ServicePrerequisiteInput::Chart { .. } => "finance.chart.create",
        ServicePrerequisiteInput::Account { .. } => "finance.account.create",
        ServicePrerequisiteInput::Period { .. } => "finance.period.create",
        ServicePrerequisiteInput::Rate { .. } => "money.rate.confirm",
    }
}
fn required(
    current: &AuthorizedWorkContext,
    input: &ServicePrerequisiteInput,
) -> Result<ServicePartsAuthorityPurpose, BusinessError> {
    let scope = current.selection().workspace();
    if scope.branch_id.is_some()
        || scope.department_id.is_some()
        || !matches!(
            current.selection().assignment().scope_kind,
            WorkScopeKind::Holding
        )
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "COMPANY_FINANCE_REQUIRED",
            "Use the original Company Finance assignment.",
        ));
    }
    if !current.has_permission("finance.read") {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "PERMISSION_REQUIRED",
            "Finance read is required.",
        ));
    }
    let rate = matches!(input, ServicePrerequisiteInput::Rate { .. });
    if !current.has_permission(if rate {
        "finance.approve"
    } else {
        "finance.write"
    }) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "PERMISSION_REQUIRED",
            "Current Finance authority is required.",
        ));
    }
    Ok(if rate {
        ServicePartsAuthorityPurpose::Read
    } else {
        ServicePartsAuthorityPurpose::Write
    })
}
struct PrerequisiteAuthority<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    input: &'a NormalizedPrerequisiteCommand,
    purchases: &'a PostingPurchases,
    purpose: CapabilityPurpose,
}
impl<C: Clock + Sync> PgCommandAuthority for PrerequisiteAuthority<'_, C> {
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
        _: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        let purpose = required(self.current, &self.input.input)?;
        let actor =
            capture_service_parts_command_authority(c, self.current, purpose, self.clock).await?;
        actor.require_current(c, self.clock).await?;
        Ok(posting_actor(self.current)?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        self.purchases
            .require_for(c, self.purpose, self.clock.now())
            .await?;
        if disposition == CommandDisposition::Replay {
            let view = read_service_prerequisites(
                c,
                self.current,
                self.clock,
                ServicePostingEnvironment::Development,
            )
            .await?;
            if !view.available {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "PACKAGE_REQUIRED",
                    "Current Service purchase history is required.",
                )
                .into());
            }
        }
        Ok(())
    }
}
struct PrerequisitePlan<'a, C> {
    clock: &'a C,
    authority: CurrentServicePartsCommandAuthority<'a>,
    commerce: CurrentServicePartsPosting,
    inventory: CurrentServicePartsInventoryHistory,
    field: CurrentServicePartsFieldHistory,
    finance: CurrentServicePartsPostingReferences,
    references: CurrentServicePartsPrerequisiteReferences,
    receipt: CurrentServicePostingReceipt,
    legacy: CurrentLegacyFinanceCommand,
    purchases: PostingPurchases,
    plan: PreparedServicePrerequisite,
}
impl<C: Clock + Sync> PrerequisitePlan<'_, C> {
    async fn before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.authority.require_current(c, self.clock).await?;
        self.commerce.require_current(c).await?;
        self.inventory.require_current(c).await?;
        self.field.require_current(c).await?;
        self.receipt.require_current(c).await?;
        self.legacy.require_current(c).await?;
        self.purchases
            .require_for(c, CapabilityPurpose::NewWork, self.clock.now())
            .await?;
        Ok(())
    }
    async fn after(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.before(c).await?;
        self.plan
            .require_current(c, &self.finance, &self.references)
            .await
    }
}
struct PrerequisiteEffects<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    input: &'a NormalizedPrerequisiteCommand,
    plan: Option<PrerequisitePlan<'a, C>>,
}
fn quote(value: &str) -> Result<String, BusinessError> {
    serde_json::to_string(value).map_err(|_| invalid())
}
fn result_json(
    request: &WorkflowRequest,
    input: &NormalizedPrerequisiteCommand,
    company: &str,
    plan: &PreparedServicePrerequisite,
    at: i64,
) -> Result<CommandJson, BusinessError> {
    let mut fields = vec![format!("\"id\":{}", quote(&request.record_id)?)];
    if matches!(input.input, ServicePrerequisiteInput::Rate { .. }) {
        for name in ["currency", "effectiveDate", "currencyPerUsd"] {
            fields.push(member(
                name,
                &input.input_json.property(name)?.ok_or_else(invalid)?,
            ));
        }
        fields.push(format!("\"confirmedAt\":{at}"));
    } else {
        let keys = input.input_json.object_keys()?.ok_or_else(invalid)?;
        for name in &keys {
            if name == "currency" && matches!(input.input, ServicePrerequisiteInput::Account { .. })
            {
                fields.push(format!(
                    "\"currency\":{}",
                    quote(plan.resolved_currency().ok_or_else(invalid)?)?
                ));
            } else {
                fields.push(member(
                    name,
                    &input.input_json.property(name)?.ok_or_else(invalid)?,
                ));
            }
        }
        if matches!(input.input, ServicePrerequisiteInput::Period { .. }) {
            fields.push("\"state\":\"open\"".into());
        }
        fields.push(format!("\"companyId\":{}", quote(company)?));
        if matches!(input.input, ServicePrerequisiteInput::Account { .. })
            && !keys.iter().any(|x| x == "currency")
        {
            fields.push(format!(
                "\"currency\":{}",
                quote(plan.resolved_currency().ok_or_else(invalid)?)?
            ));
        }
        fields.push(format!(
            "\"version\":{}",
            if matches!(input.input, ServicePrerequisiteInput::Period { .. }) {
                0
            } else {
                1
            }
        ));
    }
    CommandJson::parse(&format!("{{{}}}", fields.join(",")))
}
impl<C: Clock + Sync> PgOwnerCommandEffects for PrerequisiteEffects<'_, C> {
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
        let view = read_service_prerequisites_for(
            c,
            self.current,
            self.clock,
            ServicePostingEnvironment::Development,
            false,
        )
        .await?;
        if !view.available {
            return Err(invalid().into());
        }
        if matches!(self.input.input, ServicePrerequisiteInput::Chart { .. })
            && (view.configured == Some(true)
                || view.unavailable_reason == Some("history-requires-review"))
        {
            return Err(BusinessError::new(FailureKind::Conflict,"SERVICE_PARTS_POSTING_HISTORY_REQUIRES_REVIEW","Initial chart and account setup cannot replace an existing mapping or historical source.").into());
        }
        let company = &self.current.selection().workspace().company_id;
        let commerce = read_service_parts_posting(c, company).await?;
        let inventory =
            read_service_parts_inventory_history(c, company, &commerce.facts().warehouse_ids)
                .await?;
        let field = read_service_parts_field_history(c, company).await?;
        let receipt = read_service_posting_receipt(c, company).await?;
        let finance = read_service_parts_prerequisite_finance(c, company).await?;
        let references = read_service_parts_prerequisite_references(c, company).await?;
        if let ServicePrerequisiteInput::Account { chart_id, .. } = &self.input.input {
            let actual = finance.data().charts.iter().find(|v| {
                v["id"].as_str() == Some(chart_id) && v["status"].as_str() == Some("active")
            });
            let mapped = commerce
                .facts()
                .mapping_json
                .as_deref()
                .map(serde_json::from_str::<serde_json::Value>)
                .transpose()
                .map_err(|_| invalid())?;
            if actual.is_none()
                || mapped
                    .as_ref()
                    .is_some_and(|v| v["chart_id"].as_str() != Some(chart_id))
            {
                return Err(BusinessError::invalid(
                    "SERVICE_PARTS_CURRENCY_MISMATCH",
                    "Use the actual retained active Service parts chart.",
                )
                .into());
            }
        }
        if view.context_key.as_deref() != Some(&self.input.context_key) {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SERVICE_PARTS_POSTING_CONTEXT_CHANGED",
                "The selected work context or posting references changed.",
            )
            .into());
        }
        let legacy = read_legacy_finance_command(c, &request.key).await?;
        if !matches!(self.input.input, ServicePrerequisiteInput::Rate { .. }) && legacy.exists() {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This legacy command has no recorded work context.",
            )
            .into());
        }
        let authority = capture_service_parts_command_authority(
            c,
            self.current,
            required(self.current, &self.input.input)?,
            self.clock,
        )
        .await?;
        let purchases = PostingPurchases::capture(c, self.current, self.clock.now()).await?;
        purchases
            .require_for(c, CapabilityPurpose::NewWork, self.clock.now())
            .await?;
        let at = self.clock.now().get();
        let plan = PreparedServicePrerequisite::prepare(
            c,
            ServicePrerequisiteIntent {
                company: company.clone(),
                actor: self.current.user_id().clone(),
                id: request.record_id.clone(),
                at,
                input: self.input.input.clone(),
            },
            &finance,
            &references,
        )
        .await?;
        let result = result_json(request, self.input, company.as_str(), &plan, at)?;
        // Bound the actual reply envelope before the ledger writes a receipt.
        ensure_command_reply_bound(&result, &context(self.current)?)?;
        let version = if matches!(self.input.input, ServicePrerequisiteInput::Period { .. }) {
            0
        } else {
            1
        };
        self.plan = Some(PrerequisitePlan {
            clock: self.clock,
            authority,
            commerce,
            inventory,
            field,
            finance,
            references,
            receipt,
            legacy,
            purchases,
            plan,
        });
        Ok(WorkflowOutput {
            version,
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
        let p = self.plan.as_ref().ok_or_else(invalid)?;
        p.before(c).await?;
        p.plan.apply(c, &p.finance, &p.references).await
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let plan = self.plan.as_ref().ok_or_else(invalid)?;
        plan.after(c).await?;
        let before = plan.plan.inserted_digest(c).await?;
        flush_service_prerequisite_effects(c).await?;
        plan.after(c).await?;
        if plan.plan.inserted_digest(c).await? != before {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SERVICE_PARTS_POSTING_SOURCE_CHANGED",
                "The original posting references changed before prerequisite completion.",
            )
            .into());
        }
        Ok(())
    }
}
struct PrerequisiteIds;
impl IdentifierSource for PrerequisiteIds {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("UUID record identity")
    }
}
pub struct ServicePrerequisiteOutcome {
    pub result: CommandJson,
    pub replayed: bool,
}
/// Run only under the selected Account coordinator after the writer sources were locked.
pub async fn execute_service_prerequisite<C: Clock + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    key: &str,
    input: &NormalizedPrerequisiteCommand,
) -> Result<ServicePrerequisiteOutcome, Error> {
    let purpose = required(current, &input.input)?;
    let selected = capture_service_parts_command_authority(c, current, purpose, clock).await?;
    selected.require_current(c, clock).await?;
    let company = &current.selection().workspace().company_id;
    let receipt = read_service_prerequisite_receipt(c, key).await?;
    let op = operation(&input.input);
    let replay = receipt.facts().is_some_and(|f| {
        f.operation == op
            && f.actor_id == current.user_id().as_str()
            && f.company_id.as_deref() == Some(company.as_str())
    });
    let purchase_purpose = if replay {
        CapabilityPurpose::History
    } else {
        CapabilityPurpose::NewWork
    };
    let purchases = PostingPurchases::capture(c, current, clock.now()).await?;
    purchases
        .require_for(c, purchase_purpose, clock.now())
        .await?;
    let record_id = receipt
        .facts()
        .map(|f| f.record_id.clone())
        .unwrap_or_else(|| {
            format!(
                "{}-{}",
                match input.input {
                    ServicePrerequisiteInput::Chart { .. } => "chart",
                    ServicePrerequisiteInput::Account { .. } => "acct",
                    ServicePrerequisiteInput::Period { .. } => "period",
                    ServicePrerequisiteInput::Rate { .. } => "fx",
                },
                uuid::Uuid::new_v4()
            )
        });
    let request = WorkflowRequest {
        key: key.into(),
        operation: op.into(),
        record_id,
        input: input.workflow_input.clone(),
    };
    let mut authority = PrerequisiteAuthority {
        current,
        clock,
        input,
        purchases: &purchases,
        purpose: purchase_purpose,
    };
    let mut effects = PrerequisiteEffects {
        current,
        clock,
        input,
        plan: None,
    };
    let outcome = PgCommandLedger::new(
        company.clone(),
        c,
        &mut authority,
        clock,
        &mut PrerequisiteIds,
    )
    .execute(&request, &mut effects)
    .await?;
    Ok(ServicePrerequisiteOutcome {
        result: outcome.result,
        replayed: outcome.replayed,
    })
}

use crate::field_service_reservation_read::{
    FieldServiceRenderedContext, context, selected_read_context,
};
use crate::service_parts_posting_setup::ServicePostingTarget;
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        HoldingWorkAuthorizer, SelectedContextRead, SelectedIngressFuture,
        SelectedTransportCallbacks, SelectedWorkIngress, WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use std::sync::Arc;
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServicePrerequisitesReadResponse {
    pub data: ServicePrerequisitesView,
    pub context: FieldServiceRenderedContext,
}
fn bounded_prerequisite_response(
    data: ServicePrerequisitesView,
    context: FieldServiceRenderedContext,
) -> Result<ServicePrerequisitesReadResponse, BusinessError> {
    let response = ServicePrerequisitesReadResponse { data, context };
    serde_json::to_writer(
        &mut crate::field_service_reservation_read::ReplyBudget(0),
        &response,
    )
    .map_err(|_| invalid())?;
    Ok(response)
}
pub struct ServicePrerequisiteCommandResponse {
    pub data: CommandJson,
    pub context: FieldServiceRenderedContext,
}
fn ensure_command_reply_bound(
    data: &CommandJson,
    context: &FieldServiceRenderedContext,
) -> Result<(), BusinessError> {
    let rendered = serde_json::to_string(context).map_err(|_| invalid())?;
    if data
        .as_str()
        .len()
        .saturating_add(rendered.len())
        .saturating_add(20)
        > 2 * 1024 * 1024
    {
        return Err(invalid());
    }
    Ok(())
}
struct ReadIngress {
    selected: SelectedContextRead,
    holding: erpx_server_shared::public::HoldingId,
    company: erpx_server_shared::public::CompanyId,
}
enum ReadDenial {
    View(Box<ServicePrerequisitesReadResponse>),
    Ingress(BusinessError),
}
fn read_denied(input: &ReadIngress, error: BusinessError) -> Result<ReadDenial, BusinessError> {
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
    Ok(ReadDenial::View(Box::new(bounded_prerequisite_response(
        unavailable(input.company.as_str(), "authority-required"),
        selected_read_context(&input.selected)?,
    )?)))
}
pub async fn read_selected_service_prerequisites<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    request: Input,
    target: ServicePostingTarget<C>,
) -> Result<ServicePrerequisitesReadResponse, Error>
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
        return Err(invalid().into());
    }
    let expected_holding = holding.clone();
    let expected_company = company.clone();
    let callbacks=SelectedTransportCallbacks::<_,ReadIngress,ReadDenial,_>::new::<ServicePrerequisitesReadResponse>(move|selected:&SelectedWorkIngress<'_>|->SelectedIngressFuture<ReadIngress,ReadDenial>{let result=selected.read_context().map(|selected|ReadIngress{selected,holding:expected_holding.clone(),company:expected_company.clone()}).map_err(ReadDenial::Ingress);Box::pin(async move{result})},read_denied,move|c,current,input:ReadIngress|{let clock=clock.clone();Box::pin(async move{if current.holding_id()!=&input.holding||current.selection().workspace().company_id!=input.company{return Err(BusinessError::new(FailureKind::Forbidden,"SERVICE_POSTING_SCOPE_REQUIRED","Use the current Company.").into());}let rendered=context(current)?;let data=read_service_prerequisites(c,current,clock.as_ref(),environment).await?;Ok(Ok(bounded_prerequisite_response(data,rendered)?))})});
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
                ServicePrerequisiteSources,
            ),
            callbacks,
        )
        .await?
    {
        Ok(view) => Ok(view),
        Err(ReadDenial::View(view)) => Ok(*view),
        Err(ReadDenial::Ingress(error)) => Err(error.into()),
    }
}
fn kind_name(kind: ServicePrerequisiteKind) -> &'static str {
    match kind {
        ServicePrerequisiteKind::Chart => "chart",
        ServicePrerequisiteKind::Account => "account",
        ServicePrerequisiteKind::Period => "period",
        ServicePrerequisiteKind::Rate => "rate",
    }
}
pub struct ServicePrerequisiteTransportCommand {
    pub key: Result<String, BusinessError>,
    pub body: axum::body::Body,
    pub kind: ServicePrerequisiteKind,
}
pub async fn execute_selected_service_prerequisite<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    request: Input,
    target: ServicePostingTarget<C>,
    command: ServicePrerequisiteTransportCommand,
) -> Result<ServicePrerequisiteCommandResponse, Error>
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
        return Err(invalid().into());
    }
    let ServicePrerequisiteTransportCommand { key, body, kind } = command;
    let body = Arc::new(tokio::sync::Mutex::new(Some(body)));
    let cached = Arc::new(tokio::sync::OnceCell::new());
    let callbacks=SelectedTransportCallbacks::<_,(String,NormalizedPrerequisiteCommand),BusinessError,_>::new::<ServicePrerequisiteCommandResponse>(move|_:&SelectedWorkIngress<'_>|->SelectedIngressFuture<(String,NormalizedPrerequisiteCommand),BusinessError>{let body=body.clone();let cached=cached.clone();let key=key.clone();Box::pin(async move{let key=key?;if key.is_empty(){return Err(BusinessError::invalid("COMMAND_KEY_REQUIRED","Idempotency-Key is required."));}let bytes=cached.get_or_init(||async{let body=body.lock().await.take()?;axum::body::to_bytes(body,2*1024*1024).await.ok().map(|v|v.to_vec())}).await.as_ref().ok_or_else(wire_invalid)?;let text=String::from_utf8_lossy(bytes);let text=text.strip_prefix('\u{feff}').unwrap_or(&text);let json=CommandJson::parse(text).map_err(|_|BusinessError::invalid("INVALID_JSON","Use valid JSON."))?;let input=normalize_service_prerequisite(&json,kind_name(kind))?;Ok((key,input))})},|_,error|Err(error),move|c,current,(key,input):(String,NormalizedPrerequisiteCommand)|{let clock=clock.clone();let holding=holding.clone();let company=company.clone();Box::pin(async move{if current.holding_id()!=&holding||current.selection().workspace().company_id!=company{return Err(BusinessError::new(FailureKind::Forbidden,"SERVICE_POSTING_SCOPE_REQUIRED","Use the current Company.").into());}let rendered=context(current)?;let result=execute_service_prerequisite(c,current,clock.as_ref(),&key,&input).await?;ensure_command_reply_bound(&result.result,&rendered)?;Ok(Ok(ServicePrerequisiteCommandResponse{data:result.result,context:rendered}))})});
    let permission = if kind == ServicePrerequisiteKind::Rate {
        "finance.approve"
    } else {
        "finance.write"
    };
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
                    permission: permission.into(),
                    module: None,
                    destination: "services/field-service".into(),
                    api: true,
                },
                ServicePrerequisiteWriteSources(kind),
            ),
            callbacks,
        )
        .await?
    {
        Ok(value) => Ok(value),
        Err(error) => Err(error.into()),
    }
}

/// Wire schemas for the distinct retained command receipts. Runtime emits the
/// original ordered CommandJson so lone UTF-16 surrogates remain replay-safe.
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServicePrerequisiteChartResult {
    pub id: String,
    pub name: String,
    pub base_currency: String,
    pub accounting_basis: String,
    pub company_id: String,
    pub version: u8,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServicePrerequisiteAccountResult {
    pub id: String,
    pub chart_id: String,
    pub code: String,
    pub name: String,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub parent_id: Option<Option<String>>,
    pub currency: String,
    pub is_control: bool,
    pub control_kind: String,
    pub company_id: String,
    pub version: u8,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServicePrerequisitePeriodResult {
    pub id: String,
    pub name: String,
    pub start_date: String,
    pub end_date: String,
    pub state: String,
    pub company_id: String,
    pub version: u8,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServicePrerequisiteRateResult {
    pub id: String,
    pub currency: String,
    pub effective_date: String,
    pub currency_per_usd: String,
    #[cfg_attr(feature="contracts",ts(type="number"),schema(value_type=i64,minimum=0,maximum=9_007_199_254_740_991_i64))]
    pub confirmed_at: i64,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServicePrerequisiteChartResponse {
    pub data: ServicePrerequisiteChartResult,
    pub context: FieldServiceRenderedContext,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServicePrerequisiteAccountResponse {
    pub data: ServicePrerequisiteAccountResult,
    pub context: FieldServiceRenderedContext,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServicePrerequisitePeriodResponse {
    pub data: ServicePrerequisitePeriodResult,
    pub context: FieldServiceRenderedContext,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServicePrerequisiteRateResponse {
    pub data: ServicePrerequisiteRateResult,
    pub context: FieldServiceRenderedContext,
}

#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServicePrerequisiteChartInput {
    pub name: String,
    pub base_currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub accounting_basis: Option<String>,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServicePrerequisiteAccountInput {
    pub chart_id: String,
    pub code: String,
    pub name: String,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub parent_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub is_control: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub control_kind: Option<String>,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServicePrerequisitePeriodInput {
    pub name: String,
    pub start_date: String,
    pub end_date: String,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServicePrerequisiteRateInput {
    pub currency: String,
    pub effective_date: String,
    pub currency_per_usd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub note: Option<String>,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServicePrerequisiteChartRequest {
    pub expected_version: u8,
    pub context_key: String,
    pub input: ServicePrerequisiteChartInput,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServicePrerequisiteAccountRequest {
    pub expected_version: u8,
    pub context_key: String,
    pub input: ServicePrerequisiteAccountInput,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServicePrerequisitePeriodRequest {
    pub expected_version: u8,
    pub context_key: String,
    pub input: ServicePrerequisitePeriodInput,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServicePrerequisiteRateRequest {
    pub expected_version: u8,
    pub context_key: String,
    pub input: ServicePrerequisiteRateInput,
}
