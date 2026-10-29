//! Initial physical Count selection/readiness assembled from owner witnesses.
//! Call only from the selected Holding unit after all utility fences. This is
//! not routed while scheduled-module and full command composition are absent.
use crate::service_initial_physical_stock_purchases::ServiceCountPurchases;
use crate::service_initial_physical_stock_sources::{
    ServiceInitialPhysicalStockEnvironment, require_count_modules,
};
use erpx_server_commerce::public::{
    service_initial_count_masters::{read_service_count_mapping, read_service_count_masters},
    service_stock_masters::read_service_stock_mapping,
};
use erpx_server_finance::public::{
    initial_count_finance::read_initial_count_readiness,
    stock_master_money::read_stock_master_workspace_chart,
};
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_identity_access::public::service_initial_count_authority::{
    ServiceCountAuthorityPurpose, capture_service_count_authority,
};
use erpx_server_inventory::public::{
    service_initial_physical_count::read_initial_count_inventory_history,
    service_parts_posting::read_service_initial_stock_schema,
};
use erpx_server_organization::public::field_service_workspace::read_service_stock_branches;
use erpx_server_quality::public::initial_count_history::read_initial_count_quality_history;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use erpx_server_workflow::public::{
    commands::CommandJson, service_count_setup_receipt::read_service_count_setup_receipt,
    service_parts_posting::read_service_stock_posting_receipt,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The original Service first Count source changed.",
    )
}
#[derive(Clone, Default)]
pub struct InitialCountSelection {
    pub warehouse_id: Option<String>,
    pub product_id: Option<String>,
    pub count_date: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitialCountReadiness {
    schema_available: bool,
    posting_configured: bool,
    variance_account_available: bool,
    posting_period_available: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitialCountSelectionView {
    warehouse_id: Option<String>,
    product_id: Option<String>,
    count_date: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitialCountWorkspace {
    company_id: String,
    available: bool,
    currency: Option<String>,
    document_currency: Option<String>,
    pub(crate) context_key: Option<String>,
    unavailable_reason: Option<&'static str>,
    pub(crate) can_prepare: bool,
    can_create_warehouse: bool,
    can_create_product: bool,
    selection: InitialCountSelectionView,
    readiness: InitialCountReadiness,
    warehouses: Vec<serde_json::Value>,
    branches: Vec<serde_json::Value>,
    products: Vec<serde_json::Value>,
    record: Option<serde_json::Value>,
}
fn encode(value: &str) -> Result<String, BusinessError> {
    serde_json::to_string(value).map_err(|_| changed())
}
/// Preserve the exact original JSON.stringify identity and ordered source image.
struct SelectedContextSources<'a> {
    warehouse: &'a str,
    product: &'a str,
    date: &'a str,
    ordered_mapping: &'a str,
    setup_command: &'a str,
    setup_hash: &'a str,
    warehouse_row: &'a str,
    product_row: &'a str,
    finance_image: &'a str,
}
fn selected_context(
    current: &AuthorizedWorkContext,
    iam: &impl Serialize,
    sources: SelectedContextSources<'_>,
) -> Result<String, BusinessError> {
    let SelectedContextSources {
        warehouse,
        product,
        date,
        ordered_mapping,
        setup_command,
        setup_hash,
        warehouse_row,
        product_row,
        finance_image,
    } = sources;
    let mapping = CommandJson::parse(ordered_mapping)?;
    let mapping = mapping.as_str().strip_suffix('}').ok_or_else(changed)?;
    let goods = format!(
        "{mapping},\"service_setup_command_id\":{},\"service_setup_request_hash\":{}}}",
        encode(setup_command)?,
        encode(setup_hash)?
    );
    let source = CommandJson::parse(&format!(
        "[{goods},{{\"warehouse\":{warehouse_row},\"product\":{product_row}}},{}]",
        encode(finance_image)?
    ))?;
    let s = current.selection();
    let a = s.assignment();
    let w = s.workspace();
    let identity = serde_json::to_value(iam).map_err(|_| changed())?;
    let item = |name: &str| {
        identity
            .get(name)
            .cloned()
            .unwrap_or(serde_json::Value::Null)
    };
    let authority = format!(
        "{{\"sessionId\":{},\"revision\":{},\"assignmentVersion\":{}}}",
        encode(current.session_id().as_str())?,
        s.revision(),
        a.version
    );
    let branch = w
        .branch_id
        .as_ref()
        .map_or(Ok("null".to_owned()), |x| encode(x.as_str()))?;
    let key = CommandJson::parse(&format!(
        "[[{},{},{},null,{},{},{},{},{},{},{},{}],{},{},{},{}]",
        encode(current.holding_id().as_str())?,
        encode(w.company_id.as_str())?,
        branch,
        encode(current.user_id().as_str())?,
        encode(a.id.as_str())?,
        encode(&a.role)?,
        authority,
        item("queuedAssignmentVersion"),
        item("identityVersion"),
        item("roleRevision"),
        item("hrAppointmentVersion"),
        encode(warehouse)?,
        encode(product)?,
        encode(date)?,
        encode(source.as_str())?
    ))?;
    Ok(hex::encode(Sha256::digest(key.as_str().as_bytes())))
}
/// This read carries no route or write grant; the caller must enforce all
/// scheduled modules and use the same retained selected Holding transaction.
pub async fn read_initial_count_selection<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    selection: InitialCountSelection,
    environment: &ServiceInitialPhysicalStockEnvironment,
) -> Result<InitialCountWorkspace, Error> {
    let module = require_count_modules(c, current, environment).await?;
    let scope = current.selection().workspace();
    let company = &scope.company_id;
    let authority =
        capture_service_count_authority(c, current, ServiceCountAuthorityPurpose::Read, clock)
            .await?;
    let purchases = ServiceCountPurchases::capture(c, current, clock.now()).await?;
    purchases
        .require_for(
            c,
            erpx_server_commercial::public::purchases::CapabilityPurpose::History,
            clock.now(),
        )
        .await?;
    let can_prepare = current.has_permission("inventory.prepare") && purchases.new_work_allowed;
    let schema = read_service_initial_stock_schema(c).await?;
    let mapping = read_service_stock_mapping(c, company).await?;
    let ordered = read_service_count_mapping(c, company.as_str()).await?;
    let branches =
        read_service_stock_branches(c, company, scope.branch_id.as_ref().map(|x| x.as_str()))
            .await?;
    let masters = read_service_count_masters(
        c,
        company.as_str(),
        scope.branch_id.as_ref().map(|x| x.as_str()),
        selection.warehouse_id.as_deref(),
        selection.product_id.as_deref(),
    )
    .await?;
    let mut output = InitialCountWorkspace {
        company_id: company.as_str().into(),
        available: true,
        currency: None,
        document_currency: None,
        context_key: None,
        unavailable_reason: None,
        can_prepare: false,
        can_create_warehouse: false,
        can_create_product: false,
        selection: InitialCountSelectionView {
            warehouse_id: selection.warehouse_id.clone(),
            product_id: selection.product_id.clone(),
            count_date: selection.count_date.clone(),
        },
        readiness: InitialCountReadiness {
            schema_available: schema.available(),
            posting_configured: false,
            variance_account_available: false,
            posting_period_available: false,
        },
        warehouses: vec![],
        branches: vec![],
        products: vec![],
        record: None,
    };
    if can_prepare {
        output.warehouses =
            serde_json::from_str(&masters.data().warehouses).map_err(|_| changed())?;
        output.products = serde_json::from_str(&masters.data().products).map_err(|_| changed())?;
        output.branches = branches
            .branches()
            .map(|b| serde_json::json!({"id":b.id,"name":b.display_name}))
            .collect();
    }
    let Some(mapping_text) = mapping.mapping_json() else {
        output.unavailable_reason = Some("service-posting-required");
        authority.require_current(c, clock).await?;
        purchases.require_same(c, clock.now()).await?;
        mapping.require_current(c).await?;
        ordered.require_current(c).await?;
        masters.require_current(c).await?;
        schema.require_current(c).await?;
        branches.require_current(c).await?;
        module.require_current(c).await?;
        return Ok(output);
    };
    let m: serde_json::Value = serde_json::from_str(mapping_text).map_err(|_| changed())?;
    let chart = m["chart_id"].as_str().ok_or_else(changed)?;
    let inventory = m["inventory_account_id"].as_str().ok_or_else(changed)?;
    let currency = m["currency"].as_str().ok_or_else(changed)?;
    let receipt = read_service_stock_posting_receipt(c, company, mapping_text).await?;
    let chart_fact = read_stock_master_workspace_chart(c, company, chart, currency).await?;
    if !chart_fact.active_chart_base_matches_mapping() || receipt.facts().is_none() {
        output.unavailable_reason = Some("service-posting-required")
    } else {
        output.readiness.posting_configured = true;
        output.document_currency = Some(currency.into());
        output.can_create_warehouse = can_prepare && schema.available();
        output.can_create_product = output.can_create_warehouse;
    }
    if output.readiness.posting_configured {
        let today: String=sqlx::query_scalar("SELECT to_char(to_timestamp($1::double precision / 1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD')").bind(clock.now().get()).fetch_one(&mut *c).await?;
        let date = selection.count_date.as_deref().unwrap_or(&today);
        let finance = read_initial_count_readiness(
            c,
            current.holding_id(),
            company,
            Some(chart),
            Some(inventory),
            date,
        )
        .await?;
        output.currency = finance.basis().map(|b| b.currency.clone());
        output.readiness.variance_account_available = finance.basis().is_some();
        output.readiness.posting_period_available = finance.period_id().is_some();
        if !schema.available() {
            output.unavailable_reason = Some("schema-required")
        } else if let (Some(warehouse), Some(product), Some(count_date)) = (
            selection.warehouse_id.as_deref(),
            selection.product_id.as_deref(),
            selection.count_date.as_deref(),
        ) {
            if let (Some(w), Some(p)) = (&masters.data().warehouse, &masters.data().product) {
                let history =
                    read_initial_count_inventory_history(c, warehouse, product, None).await?;
                let quality = read_initial_count_quality_history(c, warehouse, product).await?;
                output.unavailable_reason = if history.data().has_history || quality.has_history() {
                    Some("prior-stock-history")
                } else if finance.basis().is_none() {
                    Some("variance-account-required")
                } else if finance.period_id().is_none() {
                    Some("period-required")
                } else {
                    None
                };
                output.can_prepare = can_prepare && output.unavailable_reason.is_none();
                if output.can_prepare {
                    let origin = receipt.facts().ok_or_else(changed)?;
                    let setup = read_service_count_setup_receipt(
                        c,
                        company.as_str(),
                        &origin.command_id,
                        mapping_text,
                    )
                    .await?;
                    let fact = setup.facts().ok_or_else(changed)?;
                    output.context_key = Some(selected_context(
                        current,
                        authority.iam(),
                        SelectedContextSources {
                            warehouse,
                            product,
                            date: count_date,
                            ordered_mapping: ordered.image().ok_or_else(changed)?,
                            setup_command: &fact.command_id,
                            setup_hash: &fact.request_hash,
                            warehouse_row: w,
                            product_row: p,
                            finance_image: finance.context_image(),
                        },
                    )?);
                    setup.require_current(c).await?;
                }
                history.require_current(c).await?;
                quality.require_current(c).await?;
            } else {
                output.unavailable_reason = Some("selection-unavailable")
            }
        } else {
            output.unavailable_reason = Some("selection-required")
        }
        finance.require_current(c).await?;
    }
    chart_fact.require_current(c).await?;
    receipt.require_current(c).await?;
    authority.require_current(c, clock).await?;
    purchases.require_same(c, clock.now()).await?;
    mapping.require_current(c).await?;
    ordered.require_current(c).await?;
    masters.require_current(c).await?;
    schema.require_current(c).await?;
    branches.require_current(c).await?;
    module.require_current(c).await?;
    Ok(output)
}

/// Historical Count workspace preserves the retained sparse selection shape.
pub async fn read_initial_count_workspace_record<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    id: &str,
    environment: &ServiceInitialPhysicalStockEnvironment,
) -> Result<InitialCountWorkspace, Error> {
    let module = require_count_modules(c, current, environment).await?;
    let source = crate::service_initial_physical_stock_record::read_service_count_record(
        c, current, clock, id, ServiceCountAuthorityPurpose::Read,
    ).await?;
    let schema = read_service_initial_stock_schema(c).await?;
    let currency = source.image["source"]["currency"].as_str().ok_or_else(changed)?.to_owned();
    let record = source.wire(current)?;
    let output = InitialCountWorkspace {
        company_id: current.selection().workspace().company_id.as_str().into(),
        available: true, currency: Some(currency), document_currency: None, context_key: None,
        unavailable_reason: None, can_prepare: false, can_create_warehouse: false,
        can_create_product: false,
        selection: InitialCountSelectionView { warehouse_id: None, product_id: None, count_date: None },
        readiness: InitialCountReadiness { schema_available: schema.available(), posting_configured: false, variance_account_available: false, posting_period_available: false },
        warehouses: vec![], branches: vec![], products: vec![], record: Some(record),
    };
    source.require_current(c, clock).await?;
    source.flow.require_current(c, current, clock).await?;
    schema.require_current(c).await?;
    module.require_current(c).await?;
    Ok(output)
}
