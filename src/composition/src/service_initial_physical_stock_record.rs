//! Proven original Count record composition for the selected Holding unit.
use crate::{
    service_initial_physical_stock_flow::{CurrentServiceCountFlow, read_service_count_flow},
    service_initial_physical_stock_purchases::ServiceCountPurchases,
};
use erpx_server_commerce::public::service_initial_count_masters::{
    CurrentServiceCountRecordMasters, read_service_count_record_masters,
};
use erpx_server_commercial::public::purchases::CapabilityPurpose;
use erpx_server_identity_access::public::{
    effective_authority::AuthorizedWorkContext,
    service_initial_count_authority::{
        CurrentServiceCountAuthority, ServiceCountAuthorityPurpose, capture_service_count_authority,
    },
};
use erpx_server_inventory::public::service_initial_physical_count::{
    CurrentInitialCountRecord, read_initial_count_record,
};
use erpx_server_people_payroll::public::service_count_flow_people::read_service_count_member_names;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, UserId};
use erpx_server_workflow::public::{
    service_count_setup_receipt::{
        CurrentServiceCountSetupReceipt, read_service_count_setup_history,
    },
    service_initial_count_receipt::{
        CurrentServiceInitialCountReceipt, read_service_initial_count_receipt,
    },
    stock_count_flow::StockCountFlowRecord,
};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The original Service Count source changed.",
    )
}
fn absent() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "COUNT_NOT_FOUND",
        "The original Initial Physical Stock Count is unavailable.",
    )
}
fn str_field<'a>(v: &'a serde_json::Value, k: &str) -> Result<&'a str, BusinessError> {
    v.get(k)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(changed)
}
fn num(v: &serde_json::Value, k: &str) -> Result<i64, BusinessError> {
    v.get(k)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(changed)
}
pub struct ServiceCountRecordSource<'a> {
    pub authority: CurrentServiceCountAuthority<'a>,
    pub purchases: ServiceCountPurchases,
    pub original: CurrentInitialCountRecord,
    pub receipt: CurrentServiceInitialCountReceipt,
    pub setup: CurrentServiceCountSetupReceipt,
    pub masters: CurrentServiceCountRecordMasters,
    pub flow: CurrentServiceCountFlow,
    pub image: serde_json::Value,
    names: Vec<(String, String)>,
}
impl ServiceCountRecordSource<'_> {
    pub async fn require_current<C: Clock>(
        &self,
        c: &mut PgConnection,
        clock: &C,
    ) -> Result<(), Error> {
        self.original.require_current(c).await?;
        self.receipt.require_current(c).await?;
        self.setup.require_current(c).await?;
        self.masters.require_current(c).await?;
        self.authority.require_current(c, clock).await?;
        self.purchases.require_same(c, clock.now()).await?;
        let ids = self.image["decisions"]
            .as_array()
            .map_or_else(Vec::new, |v| {
                v.iter()
                    .filter_map(|d| d["actor_id"].as_str().map(str::to_owned))
                    .collect()
            });
        if read_service_count_member_names(
            c,
            &erpx_server_shared::public::CompanyId::new(&self.flow.view.record.company_id)?,
            &ids,
        )
        .await?
            != self.names
        {
            return Err(changed().into());
        }
        Ok(())
    }
}
pub async fn read_service_count_record<'a, C: Clock>(
    c: &mut PgConnection,
    current: &'a AuthorizedWorkContext,
    clock: &C,
    count_id: &str,
    purpose: ServiceCountAuthorityPurpose,
) -> Result<ServiceCountRecordSource<'a>, Error> {
    let scope = current.selection().workspace();
    let company = &scope.company_id;
    let authority = capture_service_count_authority(c, current, purpose, clock).await?;
    let purchases = ServiceCountPurchases::capture(c, current, clock.now()).await?;
    purchases
        .require_for(c, CapabilityPurpose::History, clock.now())
        .await?;
    let original = read_initial_count_record(
        c,
        company.as_str(),
        scope.branch_id.as_ref().map(|b| b.as_str()),
        count_id,
    )
    .await?
    .ok_or_else(absent)?;
    let image: serde_json::Value =
        serde_json::from_str(&original.data().image).map_err(|_| changed())?;
    let source = &image["source"];
    let count = &image["count"];
    let line = image["lines"]
        .as_array()
        .and_then(|v| v.first())
        .ok_or_else(changed)?;
    let create_key = str_field(source, "count_command_id")?;
    let receipt =
        read_service_initial_count_receipt(c, create_key, company.as_str(), count_id).await?;
    let r = receipt.facts().ok_or_else(absent)?;
    let proof: serde_json::Value = serde_json::from_str(&r.proof_json).map_err(|_| changed())?;
    if proof != source["proof_json"] || r.actor_id != str_field(count, "created_by")? {
        return Err(changed().into());
    }
    let setup = read_service_count_setup_history(
        c,
        company.as_str(),
        str_field(source, "service_setup_command_id")?,
    )
    .await?;
    let setup_fact = setup.facts().ok_or_else(absent)?;
    let setup_proof: serde_json::Value =
        serde_json::from_str(&setup_fact.proof).map_err(|_| changed())?;
    if setup_fact.request_hash != str_field(&proof, "serviceSetupRequestHash")?
        || setup_proof["mapping"]["chart_id"] != proof["chartId"]
        || setup_proof["mapping"]["inventory_account_id"] != proof["inventoryAccountId"]
        || setup_proof["mapping"]["currency"] != proof["currency"]
    {
        return Err(changed().into());
    }
    let warehouse = str_field(count, "warehouse_id")?;
    let product = str_field(line, "product_id")?;
    let masters = read_service_count_record_masters(
        c,
        company.as_str(),
        scope.branch_id.as_ref().map(|b| b.as_str()),
        warehouse,
        product,
    )
    .await?;
    if masters.data().is_none() {
        return Err(absent().into());
    }
    let record = StockCountFlowRecord {
        company_id: company.as_str().into(),
        count_id: count_id.into(),
        branch_id: str_field(count, "branch_id")?.into(),
        status: str_field(count, "status")?.into(),
        version: num(count, "version")?,
        creator: str_field(count, "created_by")?.into(),
        preparer: str_field(count, "prepared_by")?.into(),
        approved_by: count["approved_by"].as_str().map(str::to_owned),
    };
    let allowed = if ["draft", "rejected"].contains(&record.status.as_str()) {
        purchases.new_work_allowed
    } else {
        purchases.settle_allowed
    };
    let flow = read_service_count_flow(c, current, &authority, clock, record, allowed).await?;
    let ids = image["decisions"].as_array().map_or_else(Vec::new, |v| {
        v.iter()
            .filter_map(|d| d["actor_id"].as_str().map(str::to_owned))
            .collect()
    });
    let names = read_service_count_member_names(c, company, &ids).await?;
    let result = ServiceCountRecordSource {
        names,
        authority,
        purchases,
        original,
        receipt,
        setup,
        masters,
        flow,
        image,
    };
    result.require_current(c, clock).await?;
    result.flow.require_current(c, current, clock).await?;
    Ok(result)
}
fn amount(n: i64, scale: u32) -> String {
    let b = 10i64.pow(scale);
    format!("{}.{:0width$}", n / b, n % b, width = scale as usize)
}
impl ServiceCountRecordSource<'_> {
    pub fn wire(
        &self,
        current: &AuthorizedWorkContext,
    ) -> Result<serde_json::Value, BusinessError> {
        let count = &self.image["count"];
        let line = self.image["lines"]
            .as_array()
            .and_then(|v| v.first())
            .ok_or_else(changed)?;
        let (w, p) = self.masters.data().ok_or_else(changed)?;
        let warehouse: serde_json::Value = serde_json::from_str(w).map_err(|_| changed())?;
        let product: serde_json::Value = serde_json::from_str(p).map_err(|_| changed())?;
        let creator = UserId::new(str_field(count, "created_by")?)?;
        let preparer = UserId::new(str_field(count, "prepared_by")?)?;
        let same = self.authority.same_person(&creator) || self.authority.same_person(&preparer);
        let status = str_field(count, "status")?;
        let edit = !current.selection().workspace().department_id.is_some()
            && current.has_permission("inventory.prepare")
            && same
            && ["draft", "rejected"].contains(&status);
        let review = current.selection().workspace().branch_id.is_none()
            && current.selection().workspace().department_id.is_none()
            && current.has_permission("inventory.approve")
            && !same
            && ["submitted", "approved"].contains(&status)
            && self.flow.view.can_act;
        let lots=self.image["lots"].as_array().map_or_else(Vec::new,Clone::clone).iter().map(|l|Ok(serde_json::json!({"lotCode":l["lot_code"],"serialNumber":l["serial_number"],"manufacturedDate":l["manufactured_date"],"expiryDate":l["expiry_date"],"countedQuantity":amount(num(l,"counted_quantity_milli")?,3),"unitCost":l["unit_cost_minor"].as_i64().map(|n|amount(n,2))}))).collect::<Result<Vec<_>,BusinessError>>()?;

        let mut decisions = self.image["decisions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        decisions.sort_by_key(|d| std::cmp::Reverse(d["created_at"].as_i64().unwrap_or(0)));
        let decisions:Vec<_>=decisions.iter().flat_map(|d|{let id=d["actor_id"].as_str().unwrap_or("");let found:Vec<_>=self.names.iter().filter(|(user,_)|user==id).map(|(_,name)|name.as_str()).collect();let found=if found.is_empty(){vec![id]}else{found};found.into_iter().map(move |name|serde_json::json!({"action":d["action"],"reason":d["reason"],"actorName":name,"createdAt":d["created_at"]}))}).collect();

        Ok(
            serde_json::json!({"sourceKind":"service-initial-physical-stock-count","id":count["id"],"number":count["number"],"status":status,"version":count["version"],"countDate":count["count_date"],"reason":count["reason"],"warehouseId":count["warehouse_id"],"warehouseName":warehouse["name"],"productId":line["product_id"],"sku":product["sku"],"productName":line["description"],"unit":line["unit"],"trackingMode":product["tracking_mode"],"countedQuantity":amount(num(line,"counted_quantity_milli")?,3),"countedValue":amount(num(line,"counted_value_minor")?,2),"unitCost":line["unit_cost_minor"].as_i64().map(|n|amount(n,2)),"lots":lots,"journalId":count["journal_id"],"canSubmit":edit&&self.purchases.new_work_allowed&&self.flow.view.can_act,"canCancel":edit&&self.purchases.settle_allowed,"canApprove":status=="submitted"&&review&&self.purchases.settle_allowed,"canPost":status=="approved"&&review&&self.purchases.settle_allowed,"canReject":(["submitted","approved"].contains(&status)&&review&&self.purchases.settle_allowed),"decisions":decisions}),
        )
    }
}

/// Generic Stock Count presentation assembled only from public owner facts in
/// the current selected Holding unit. The DTO preserves generic column names.
pub async fn read_service_count_generic_detail<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    id: &str,
    environment: &crate::service_initial_physical_stock_sources::ServiceInitialPhysicalStockEnvironment,
) -> Result<serde_json::Value, Error> {
    use erpx_server_commerce::public::service_initial_count_masters::read_service_count_mapping;
    use erpx_server_finance::public::initial_count_finance::read_service_count_display_currency;
    use erpx_server_inventory::public::service_count_generic_detail::read_service_count_generic_facts;
    use erpx_server_workflow::public::service_initial_count_receipt::read_service_count_generic_audit;
    let module = crate::service_initial_physical_stock_sources::require_count_modules(
        c,
        current,
        environment,
    )
    .await?;
    let source =
        read_service_count_record(c, current, clock, id, ServiceCountAuthorityPurpose::Read)
            .await?;
    let company = &current.selection().workspace().company_id;
    let mut data = read_service_count_generic_facts(c, &source.original).await?;
    let (warehouse, product) = source.masters.data().ok_or_else(changed)?;
    let warehouse: serde_json::Value = serde_json::from_str(warehouse).map_err(|_| changed())?;
    let product: serde_json::Value = serde_json::from_str(product).map_err(|_| changed())?;
    let mapping = read_service_count_mapping(c, company.as_str()).await?;
    let mapping_data: serde_json::Value =
        serde_json::from_str(mapping.image().ok_or_else(absent)?).map_err(|_| changed())?;
    let currency =
        read_service_count_display_currency(c, company, str_field(&mapping_data, "chart_id")?)
            .await?
            .ok_or_else(absent)?;
    data["count"]["warehouse_name"] = warehouse["name"].clone();
    data["count"]["currency"] = currency.into();
    for line in data["lines"].as_array_mut().ok_or_else(changed)? {
        if line["product_id"] != product["id"] {
            return Err(changed().into());
        }
        line["sku"] = product["sku"].clone();
        line["tracking_mode"] = product["tracking_mode"].clone();
    }
    let mut history = read_service_count_generic_audit(c, &source.receipt).await?;
    let ids: std::collections::BTreeSet<String> = history
        .as_array()
        .ok_or_else(changed)?
        .iter()
        .chain(data["decisions"].as_array().ok_or_else(changed)?.iter())
        .map(|row| str_field(row, "actor_id").map(str::to_owned))
        .collect::<Result<_, _>>()?;
    let ids = ids.into_iter().collect::<Vec<_>>();
    let names = read_service_count_member_names(c, company, &ids).await?;
    // The legacy left join returns every matching People row. Preserve that
    // behavior instead of silently choosing a duplicate name.
    let enrich =
        |rows: &serde_json::Value, audit: bool| -> Result<serde_json::Value, BusinessError> {
            let mut out = Vec::new();
            for row in rows.as_array().ok_or_else(changed)? {
                let actor = str_field(row, "actor_id")?;
                let matches = names
                    .iter()
                    .filter(|(id, _)| id == actor)
                    .map(|(_, name)| name.as_str())
                    .collect::<Vec<_>>();
                for name in if matches.is_empty() {
                    vec![actor]
                } else {
                    matches
                } {
                    let mut row = row.clone();
                    row["actor_name"] = name.into();
                    if audit {
                        row.as_object_mut().ok_or_else(changed)?.remove("actor_id");
                    }
                    out.push(row);
                }
            }
            if out.len() > 500 {
                return Err(changed());
            }
            Ok(serde_json::Value::Array(out))
        };
    history = enrich(&history, true)?;
    data["decisions"] = enrich(&data["decisions"], false)?;
    data["history"] = history;
    source.require_current(c, clock).await?;
    source.flow.require_current(c, current, clock).await?;
    mapping.require_current(c).await?;
    if read_service_count_display_currency(c, company, str_field(&mapping_data, "chart_id")?)
        .await?
        .as_deref()
        != data["count"]["currency"].as_str()
        || read_service_count_member_names(c, company, &ids).await? != names
    {
        return Err(changed().into());
    }
    module.require_current(c).await?;
    Ok(data)
}
