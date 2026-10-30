//! Generic Stock Count update wire normalization for retained Service sources.
//! The update coordinator and Inventory revision port are not yet implemented.
use erpx_server_shared::public::BusinessError;
use erpx_server_workflow::public::commands::CommandJson;
fn invalid() -> BusinessError {
    BusinessError::invalid("VALIDATION_FAILED", "Use valid stock Count input.")
}
fn property(v: &CommandJson, key: &str) -> Result<CommandJson, BusinessError> {
    v.property(key)?.ok_or_else(invalid)
}
fn text(
    v: &CommandJson,
    min: usize,
    max: usize,
    trim: bool,
) -> Result<(String, CommandJson), BusinessError> {
    let mut u = v.string_utf16_units()?.ok_or_else(invalid)?;
    if trim {
        let white = |c: &u16| matches!(*c,0x0009..=0x000d|0x0020|0x00a0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff);
        let a = u.iter().position(|c| !white(c)).unwrap_or(u.len());
        let b = u.iter().rposition(|c| !white(c)).map_or(a, |i| i + 1);
        u = u[a..b].to_vec();
    }
    if !(min..=max).contains(&u.len()) {
        return Err(invalid());
    }
    Ok((
        String::from_utf16_lossy(&u),
        CommandJson::from_utf16_string(&u)?,
    ))
}
fn decimal(v: &CommandJson, scale: usize) -> Result<(i64, CommandJson), BusinessError> {
    let (s, j) = text(v, 1, 16, false)?;
    let mut p = s.split('.');
    let whole = p.next().ok_or_else(invalid)?;
    let fraction = p.next();
    if p.next().is_some()
        || whole.is_empty()
        || whole.len() > 12
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || fraction.is_some_and(|f| {
            f.is_empty() || f.len() > scale || !f.bytes().all(|c| c.is_ascii_digit())
        })
    {
        return Err(invalid());
    }
    let mut n = whole.parse::<i64>().map_err(|_| invalid())? * 10_i64.pow(scale as u32);
    if let Some(f) = fraction {
        n += f.parse::<i64>().map_err(|_| invalid())? * 10_i64.pow((scale - f.len()) as u32);
    }
    Ok((n, j))
}
fn optional(
    v: &CommandJson,
    key: &str,
    max: usize,
    trim: bool,
) -> Result<Option<(String, CommandJson)>, BusinessError> {
    v.property(key)?.map(|x| text(&x, 1, max, trim)).transpose()
}
#[derive(Clone, Debug)]
pub struct ServiceCountUpdateLot {
    pub lot_id: Option<String>,
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub manufactured_date: Option<String>,
    pub expiry_date: Option<String>,
    pub quantity_milli: i64,
    pub unit_cost_minor: Option<i64>,
}
#[derive(Clone, Debug)]
pub struct ServiceCountUpdateLine {
    pub product_id: String,
    pub quantity_milli: i64,
    pub unit_cost_minor: Option<i64>,
    pub allocations: Vec<ServiceCountUpdateLot>,
}
#[derive(Clone, Debug)]
pub struct ServiceCountUpdateInput {
    pub warehouse_id: String,
    pub count_date: String,
    pub reason: String,
    pub expected_version: Option<i64>,
    pub lines: Vec<ServiceCountUpdateLine>,
    pub normalized: CommandJson,
    /// A fresh execution must never use lossy decoded values as lookup keys.
    /// Replay must still use the original normalized UTF-16 identity first.
    pub contains_unpaired_surrogate: bool,
}
fn unpaired(v: &CommandJson, key: &str) -> Result<bool, BusinessError> {
    let Some(value) = v.property(key)? else {
        return Ok(false);
    };
    let Some(units) = value.string_utf16_units()? else {
        return Ok(false);
    };
    Ok(std::char::decode_utf16(units).any(|item| item.is_err()))
}
/// Mirrors the generic Count Zod projection, including unknown-field stripping,
/// default allocation arrays and dates, and optional version omission.
pub fn normalize_service_count_update(raw: &str) -> Result<ServiceCountUpdateInput, BusinessError> {
    let root = CommandJson::parse(raw)?;
    let (warehouse, wj) = text(&property(&root, "warehouseId")?, 1, usize::MAX, false)?;
    let (day, dj) = text(&property(&root, "countDate")?, 10, 10, false)?;
    if !crate::service_initial_physical_stock_command::date(&day) {
        return Err(invalid());
    }
    let (reason, rj) = text(&property(&root, "reason")?, 5, 2000, true)?;
    let version = root
        .property("expectedVersion")?
        .map(|v| {
            let n = v.as_str().parse::<f64>().map_err(|_| invalid())?;
            if !n.is_finite() || n.fract() != 0.0 || !(1.0..=9_007_199_254_740_991.0).contains(&n) {
                return Err(invalid());
            }
            Ok((n as i64, v))
        })
        .transpose()?;
    let lines = property(&root, "lines")?
        .array_elements()?
        .ok_or_else(invalid)?;
    if !(1..=50).contains(&lines.len()) {
        return Err(invalid());
    }
    let mut contains_unpaired_surrogate =
        unpaired(&root, "warehouseId")? || unpaired(&root, "reason")?;
    let mut encoded = Vec::new();
    let mut output = Vec::new();
    for line in lines {
        contains_unpaired_surrogate |= unpaired(&line, "productId")?;
        let (product, pj) = text(&property(&line, "productId")?, 1, usize::MAX, false)?;
        let (qty, qj) = decimal(&property(&line, "countedQuantity")?, 3)?;
        let cost = line
            .property("unitCost")?
            .map(|v| decimal(&v, 2))
            .transpose()?;
        let values = match line.property("allocations")? {
            Some(v) => v.array_elements()?.ok_or_else(invalid)?,
            None => vec![],
        };
        if values.len() > 500 {
            return Err(invalid());
        }
        let mut lots = Vec::new();
        let mut encoded_lots = Vec::new();
        for lot in values {
            for field in ["lotId", "lotCode", "serialNumber"] {
                contains_unpaired_surrogate |= unpaired(&lot, field)?;
            }
            let id = optional(&lot, "lotId", usize::MAX, false)?;
            let code = optional(&lot, "lotCode", 100, true)?;
            let serial = optional(&lot, "serialNumber", 160, true)?;
            let date = |key: &str| -> Result<(Option<String>, CommandJson), BusinessError> {
                match lot.property(key)? {
                    None => Ok((None, CommandJson::parse("\"\"")?)),
                    Some(v) => {
                        let (s, j) = text(&v, 0, 10, false)?;
                        if !s.is_empty() && !crate::service_initial_physical_stock_command::date(&s)
                        {
                            return Err(invalid());
                        }
                        Ok(((!s.is_empty()).then_some(s), j))
                    }
                }
            };
            let (manufactured, mj) = date("manufacturedDate")?;
            let (expiry, ej) = date("expiryDate")?;
            let (lqty, lqj) = decimal(&property(&lot, "countedQuantity")?, 3)?;
            let lcost = lot
                .property("unitCost")?
                .map(|v| decimal(&v, 2))
                .transpose()?;
            let mut f = Vec::new();
            for (name, value) in [
                ("lotId", &id),
                ("lotCode", &code),
                ("serialNumber", &serial),
            ] {
                if let Some((_, j)) = value {
                    f.push(format!("\"{name}\":{}", j.as_str()));
                }
            }
            f.push(format!("\"manufacturedDate\":{}", mj.as_str()));
            f.push(format!("\"expiryDate\":{}", ej.as_str()));
            f.push(format!("\"countedQuantity\":{}", lqj.as_str()));
            if let Some((_, j)) = &lcost {
                f.push(format!("\"unitCost\":{}", j.as_str()));
            }
            encoded_lots.push(format!("{{{}}}", f.join(",")));
            lots.push(ServiceCountUpdateLot {
                lot_id: id.map(|x| x.0),
                lot_code: code.map(|x| x.0),
                serial_number: serial.map(|x| x.0),
                manufactured_date: manufactured,
                expiry_date: expiry,
                quantity_milli: lqty,
                unit_cost_minor: lcost.map(|x| x.0),
            });
        }
        let cost_field = cost.as_ref().map_or_else(String::new, |(_, j)| {
            format!(",\"unitCost\":{}", j.as_str())
        });
        encoded.push(format!(
            "{{\"productId\":{},\"countedQuantity\":{}{cost_field},\"allocations\":[{}]}}",
            pj.as_str(),
            qj.as_str(),
            encoded_lots.join(",")
        ));
        output.push(ServiceCountUpdateLine {
            product_id: product,
            quantity_milli: qty,
            unit_cost_minor: cost.map(|x| x.0),
            allocations: lots,
        });
    }
    let version_field = version.as_ref().map_or_else(String::new, |(_, j)| {
        format!(",\"expectedVersion\":{}", j.as_str())
    });
    let normalized = CommandJson::parse(&format!(
        "{{\"warehouseId\":{},\"countDate\":{},\"reason\":{}{version_field},\"lines\":[{}]}}",
        wj.as_str(),
        dj.as_str(),
        rj.as_str(),
        encoded.join(",")
    ))?;
    Ok(ServiceCountUpdateInput {
        warehouse_id: warehouse,
        count_date: day,
        reason,
        expected_version: version.map(|x| x.0),
        lines: output,
        normalized,
        contains_unpaired_surrogate,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generic_update_matches_actual_zod_oracle() {
        let oracle: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/field_service/service-count-update-zod-oracle.v1.json"
        )))
        .unwrap();
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 17);
        for case in cases {
            let raw = case["raw"].as_str().unwrap();
            match case["normalized"].as_str() {
                Some(expected) => {
                    let normalized = normalize_service_count_update(raw).unwrap();
                    assert_eq!(normalized.normalized.as_str(), expected, "{}", case["name"]);
                    assert_eq!(
                        normalized.contains_unpaired_surrogate,
                        case["name"] == "surrogate",
                        "{}",
                        case["name"]
                    );
                }
                None => assert!(
                    normalize_service_count_update(raw).is_err(),
                    "{}",
                    case["name"]
                ),
            }
        }
    }
    #[test]
    fn generic_update_normalizes_defaults_and_strips_unknown_fields() {
        let parsed=normalize_service_count_update(r#"{"unknown":1,"warehouseId":"w","countDate":"2026-09-01","reason":"  first count  ","expectedVersion":1.0,"lines":[{"productId":"p","countedQuantity":"2.00","ignored":true},{"productId":"p2","countedQuantity":"1","allocations":[{"lotCode":" Lot ","countedQuantity":"1","unitCost":"2.00","unused":4}]}]}"#).unwrap();
        assert_eq!(parsed.expected_version, Some(1));
        assert_eq!(parsed.lines.len(), 2);
        assert_eq!(
            parsed.normalized.as_str(),
            r#"{"warehouseId":"w","countDate":"2026-09-01","reason":"first count","expectedVersion":1,"lines":[{"productId":"p","countedQuantity":"2.00","allocations":[]},{"productId":"p2","countedQuantity":"1","allocations":[{"lotCode":"Lot","manufacturedDate":"","expiryDate":"","countedQuantity":"1","unitCost":"2.00"}]}]}"#
        );
    }
    #[test]
    fn optional_version_is_omitted_and_bad_lot_inputs_rejected() {
        let body = r#"{"warehouseId":"w","countDate":"2026-09-01","reason":"hello","lines":[{"productId":"p","countedQuantity":"1"}]}"#;
        let parsed = normalize_service_count_update(body).unwrap();
        assert_eq!(parsed.expected_version, None);
        assert!(!parsed.normalized.as_str().contains("expectedVersion"));
        assert!(
            normalize_service_count_update(&body.replace(
                "\"countedQuantity\":\"1\"",
                "\"countedQuantity\":\"1.2345\""
            ))
            .is_err()
        );
    }
}

use crate::service_initial_physical_stock_purchases::ServiceCountPurchases;
use crate::service_initial_physical_stock_sources::{
    ServiceInitialPhysicalStockEnvironment, require_count_modules,
};
use erpx_server_commerce::public::service_initial_count_masters::{
    CurrentServiceCountRecordMasters, read_service_count_record_masters,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue,
};
use erpx_server_identity_access::public::{
    effective_authority::AuthorizedWorkContext,
    service_initial_count_authority::{
        CurrentServiceCountAuthority, ServiceCountAuthorityPurpose, capture_service_count_authority,
    },
};
use erpx_server_inventory::public::{
    service_initial_count_revision::{
        InitialCountRevision, InitialCountRevisionAllocation, InitialCountRevisionPlan,
        check_generic_revision_movement, prepare_initial_count_revision,
        validate_generic_revision_line,
    },
    service_initial_physical_count::{
        CurrentInitialCountInventoryHistory, CurrentInitialCountRecord,
        InitialCountProductSnapshot, TrackingMode, read_initial_count_inventory_history,
        read_initial_count_record,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{Clock, FailureKind, IdentifierSource, UserId};
use erpx_server_workflow::public::{
    commands::{
        CommandActor, CommandDisposition, PgCommandAuthority, PgCommandLedger,
        PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
    },
    service_count_command_evidence::{
        CountCommandClock, CountCommandGeneratedIds, PlannedCountCommandEvidence,
        RetainedCountCommandEvidence, plan_count_command_evidence, verify_historical_count_command,
    },
    service_count_setup_receipt::{
        CurrentServiceCountSetupReceipt, read_service_count_setup_history,
    },
    service_initial_count_receipt::{
        CurrentServiceInitialCountReceipt, read_service_initial_count_receipt,
    },
};
use sqlx::PgConnection;
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The original Service first Count source changed.",
    )
}
fn value_string<'a>(v: &'a serde_json::Value, key: &str) -> Result<&'a str, BusinessError> {
    v[key].as_str().ok_or_else(changed)
}
struct UpdateSource<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    module: erpx_server_pms::public::initial_count_modules::CurrentInitialCountModules,
    authority: CurrentServiceCountAuthority<'a>,
    purchases: ServiceCountPurchases,
    catalogue: CommercialCatalogue,
    supply: PurchaseWitness,
    original: CurrentInitialCountRecord,
    receipt: CurrentServiceInitialCountReceipt,
    setup: CurrentServiceCountSetupReceipt,
    branch: String,
    image: serde_json::Value,
}
impl<C: Clock + Sync> UpdateSource<'_, C> {
    async fn require(
        &self,
        c: &mut PgConnection,
        purpose: CapabilityPurpose,
        plan: Option<&InitialCountRevisionPlan>,
    ) -> Result<(), Error> {
        self.module.require_current(c).await?;
        self.authority.require_current(c, self.clock).await?;
        self.purchases
            .require_for(c, purpose, self.clock.now())
            .await?;
        PgCompanyPurchaseReader::new(
            self.current.holding_id().clone(),
            self.current.selection().workspace().company_id.clone(),
            c,
            &self.catalogue,
        )
        .revalidate(&self.supply, self.clock.now())
        .await?
        .require_allowed()?;
        self.receipt.require_current(c).await?;
        self.setup.require_current(c).await?;
        if let Some(plan) = plan {
            plan.require_current(c).await?;
        } else {
            self.original.require_current(c).await?;
        }
        Ok(())
    }
}
async fn update_source<'a, C: Clock + Sync>(
    c: &mut PgConnection,
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    id: &str,
    environment: &'a ServiceInitialPhysicalStockEnvironment,
) -> Result<UpdateSource<'a, C>, Error> {
    let module = require_count_modules(c, current, environment).await?;
    let authority =
        capture_service_count_authority(c, current, ServiceCountAuthorityPurpose::Prepare, clock)
            .await?;
    let purchases = ServiceCountPurchases::capture(c, current, clock.now()).await?;
    purchases
        .require_for(c, CapabilityPurpose::History, clock.now())
        .await?;
    let catalogue = current_package_catalogue()?;
    let supply = {
        let mut reader = PgCompanyPurchaseReader::new(
            current.holding_id().clone(),
            current.selection().workspace().company_id.clone(),
            c,
            &catalogue,
        );
        let observed = reader
            .inspect(
                &PurchaseRequest {
                    capabilities: vec!["supply.counts".into()],
                    purpose: CapabilityPurpose::NewWork,
                    mode: PackageEnforcementMode::Enforced,
                },
                clock.now(),
            )
            .await?;
        observed.require_allowed()?;
        observed.witness
    };
    let w = current.selection().workspace();
    let original = read_initial_count_record(
        c,
        w.company_id.as_str(),
        w.branch_id.as_ref().map(|b| b.as_str()),
        id,
    )
    .await?
    .ok_or_else(changed)?;
    let image: serde_json::Value =
        serde_json::from_str(&original.data().image).map_err(|_| changed())?;
    let source = &image["source"];
    let count = &image["count"];
    let receipt = read_service_initial_count_receipt(
        c,
        value_string(source, "count_command_id")?,
        w.company_id.as_str(),
        id,
    )
    .await?;
    let facts = receipt.facts().ok_or_else(changed)?;
    let proof: serde_json::Value =
        serde_json::from_str(&facts.proof_json).map_err(|_| changed())?;
    if proof != source["proof_json"] || facts.actor_id != value_string(count, "created_by")? {
        return Err(changed().into());
    }
    let setup = read_service_count_setup_history(
        c,
        w.company_id.as_str(),
        value_string(source, "service_setup_command_id")?,
    )
    .await?;
    let setup_facts = setup.facts().ok_or_else(changed)?;
    let setup_proof: serde_json::Value =
        serde_json::from_str(&setup_facts.proof).map_err(|_| changed())?;
    if Some(setup_facts.request_hash.as_str()) != proof["serviceSetupRequestHash"].as_str()
        || setup_proof["mapping"]["chart_id"] != proof["chartId"]
        || setup_proof["mapping"]["inventory_account_id"] != proof["inventoryAccountId"]
        || setup_proof["mapping"]["currency"] != proof["currency"]
    {
        return Err(changed().into());
    }
    let branch = value_string(count, "branch_id")?.to_owned();
    module.require_current(c).await?;
    original.require_current(c).await?;
    receipt.require_current(c).await?;
    setup.require_current(c).await?;
    Ok(UpdateSource {
        current,
        clock,
        module,
        authority,
        purchases,
        catalogue,
        supply,
        original,
        receipt,
        setup,
        branch,
        image,
    })
}
struct UpdateCompletion<'a, C: Clock + Sync> {
    source: &'a UpdateSource<'a, C>,
    history: &'a CurrentInitialCountInventoryHistory,
    masters: &'a CurrentServiceCountRecordMasters,
    warehouse_masters: &'a CurrentServiceCountRecordMasters,
    plan: &'a InitialCountRevisionPlan,
    applied: AtomicBool,
    clock: CountCommandClock,
    generated: &'a CountCommandGeneratedIds,
    evidence: OnceLock<PlannedCountCommandEvidence>,
    retained: OnceLock<RetainedCountCommandEvidence>,
}
impl<C: Clock + Sync> UpdateCompletion<'_, C> {
    async fn require(&self, c: &mut PgConnection) -> Result<(), Error> {
        let plan = self.applied.load(Ordering::SeqCst).then_some(self.plan);
        self.source
            .require(c, CapabilityPurpose::NewWork, plan)
            .await?;
        self.history.require_current(c).await?;
        self.masters.require_current(c).await?;
        self.warehouse_masters.require_current(c).await?;
        if let Some(retained) = self.retained.get() {
            retained.require_current(c).await?;
        }
        Ok(())
    }
}
struct UpdateAuthority<'a, C: Clock + Sync> {
    source: &'a UpdateSource<'a, C>,
    request: &'a WorkflowRequest,
    completion: Option<&'a UpdateCompletion<'a, C>>,
}
impl<C: Clock + Sync> PgCommandAuthority for UpdateAuthority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        use sqlx::Connection;
        if !c.is_in_transaction() {
            return Err(changed().into());
        }
        Ok(())
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        r: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if r.key != self.request.key
            || r.operation != self.request.operation
            || r.record_id != self.request.record_id
            || r.input != self.request.input
        {
            return Err(changed().into());
        }
        if let Some(completion) = self.completion {
            completion.require(c).await?
        } else {
            self.source
                .require(c, CapabilityPurpose::History, None)
                .await?
        }
        Ok(crate::service_initial_physical_stock_create::actor(
            self.source.current,
        )?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        r: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        if r.key != self.request.key {
            return Err(changed().into());
        }
        match disposition {
            CommandDisposition::Replay => {
                self.source
                    .require(c, CapabilityPurpose::History, None)
                    .await?;
                verify_historical_count_command(c, actor, r, &self.source.branch, None).await?
            }
            CommandDisposition::Execute => self.completion.ok_or_else(changed)?.require(c).await?,
        }
        Ok(())
    }
}
struct UpdateEffects<'a, C: Clock + Sync> {
    completion: &'a UpdateCompletion<'a, C>,
    version: i64,
    id: &'a str,
}
impl<C: Clock + Sync> PgOwnerCommandEffects for UpdateEffects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        use sqlx::Connection;
        if !c.is_in_transaction() {
            return Err(changed().into());
        }
        Ok(())
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        request: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        self.completion.require(c).await?;
        let output = WorkflowOutput {
            version: self.version,
            result: CommandJson::from_serializable(
                &serde_json::json!({"id":self.id,"status":"draft","version":self.version}),
            )?,
            event_scope: None,
        };
        let evidence =
            plan_count_command_evidence(c, actor, request, &output, self.completion.clock).await?;
        self.completion
            .evidence
            .set(evidence)
            .map_err(|_| changed())?;
        Ok(output)
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.completion.require(c).await?;
        self.completion
            .plan
            .apply(c, &self.completion.source.original, self.completion.history)
            .await?;
        self.completion.applied.store(true, Ordering::SeqCst);
        self.completion.require(c).await
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.completion.require(c).await?;
        let retained = self
            .completion
            .evidence
            .get()
            .ok_or_else(changed)?
            .retain(c, self.completion.generated)
            .await?;
        self.completion
            .retained
            .set(retained)
            .map_err(|_| changed())?;
        self.completion.require(c).await
    }
}
/// Execute only after the selected Account coordinator prepares the provided
/// environment sources on its serializable Holding transaction.
pub struct ServiceCountUpdateCommand<'a> {
    pub key: &'a str,
    pub id: &'a str,
    pub raw: &'a str,
}
pub async fn execute_service_initial_count_update<C: Clock + Sync, I: IdentifierSource>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    ids: &mut I,
    command: ServiceCountUpdateCommand<'_>,
    environment: &ServiceInitialPhysicalStockEnvironment,
) -> Result<serde_json::Value, Error> {
    let input = normalize_service_count_update(command.raw)?;
    let source = update_source(c, current, clock, command.id, environment).await?;
    let request = WorkflowRequest {
        key: command.key.into(),
        operation: "inventory.count.update".into(),
        record_id: command.id.into(),
        input: input.normalized.clone(),
    };
    let mut replay_authority = UpdateAuthority {
        source: &source,
        request: &request,
        completion: None,
    };
    let replay = PgCommandLedger::new(
        current.selection().workspace().company_id.clone(),
        c,
        &mut replay_authority,
        clock,
        ids,
    )
    .replay(
        &request,
        &mut crate::service_initial_physical_stock_create::ReplayOnly,
    )
    .await?;
    if let Some(result) = replay {
        return serde_json::from_str(result.result.as_str()).map_err(|_| changed().into());
    }
    source
        .purchases
        .require_for(c, CapabilityPurpose::NewWork, clock.now())
        .await?;
    let count = &source.image["count"];
    let creator = UserId::new(value_string(count, "created_by")?)?;
    let preparer = UserId::new(value_string(count, "prepared_by")?)?;
    if !source.authority.same_person(&creator) && !source.authority.same_person(&preparer) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "COUNT_LOCKED",
            "Only the preparer can edit a draft or returned count.",
        )
        .into());
    }
    if !["draft", "rejected"].contains(&value_string(count, "status")?) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "COUNT_LOCKED",
            "Only the preparer can edit a draft or returned count.",
        )
        .into());
    }
    if input.warehouse_id != value_string(count, "warehouse_id")? {
        return Err(BusinessError::invalid(
            "WAREHOUSE_LOCKED",
            "Create a separate count for another warehouse.",
        )
        .into());
    }
    let mut products = std::collections::BTreeSet::new();
    if input
        .lines
        .iter()
        .any(|line| !products.insert(line.product_id.clone()))
    {
        return Err(BusinessError::invalid("DUPLICATE_PRODUCT", "Count each product once.").into());
    }
    let w = current.selection().workspace();
    let original_product = value_string(&source.image["lines"][0], "product_id")?;
    let warehouse_masters = read_service_count_record_masters(
        c,
        w.company_id.as_str(),
        w.branch_id.as_ref().map(|v| v.as_str()),
        &input.warehouse_id,
        original_product,
    )
    .await?;
    let warehouse: serde_json::Value =
        serde_json::from_str(warehouse_masters.data().ok_or_else(changed)?.0)
            .map_err(|_| changed())?;
    if warehouse["status"].as_str() != Some("active") {
        return Err(BusinessError::invalid(
            "WAREHOUSE_INVALID",
            "Choose an active warehouse in your scope.",
        )
        .into());
    }
    let mut prepared = Vec::with_capacity(input.lines.len());
    for line in &input.lines {
        let masters = read_service_count_record_masters(
            c,
            w.company_id.as_str(),
            w.branch_id.as_ref().map(|v| v.as_str()),
            &input.warehouse_id,
            &line.product_id,
        )
        .await?;
        let product: serde_json::Value = serde_json::from_str(
            masters
                .data()
                .ok_or_else(|| {
                    BusinessError::invalid(
                        "PRODUCT_INVALID",
                        "Choose an active stock item in this company.",
                    )
                })?
                .1,
        )
        .map_err(|_| changed())?;
        if product["status"].as_str() != Some("active") {
            return Err(BusinessError::invalid(
                "PRODUCT_INVALID",
                "Choose an active stock item in this company.",
            )
            .into());
        }
        let tracking = match product["tracking_mode"].as_str() {
            Some("none") => TrackingMode::None,
            Some("lot") => TrackingMode::Lot,
            Some("serial") => TrackingMode::Serial,
            _ => return Err(changed().into()),
        };
        let snapshot = InitialCountProductSnapshot {
            version: product["version"].as_i64().ok_or_else(changed)?,
            description: value_string(&product, "name")?.into(),
            unit: value_string(&product, "unit")?.into(),
            tracking,
        };
        let history = read_initial_count_inventory_history(
            c,
            &input.warehouse_id,
            &line.product_id,
            Some(command.id),
        )
        .await?;
        let revision = InitialCountRevision {
            company_id: w.company_id.as_str().into(),
            branch_id: source.branch.clone(),
            count_id: command.id.into(),
            warehouse_id: input.warehouse_id.clone(),
            product_id: line.product_id.clone(),
            expected_version: input.expected_version.unwrap_or(0),
            count_date: input.count_date.clone(),
            reason: input.reason.clone(),
            actor_id: current.user_id().as_str().into(),
            now: 0,
            product: snapshot,
            quantity_milli: line.quantity_milli,
            unit_cost_minor: line.unit_cost_minor,
            allocations: line
                .allocations
                .iter()
                .map(|a| InitialCountRevisionAllocation {
                    lot_id: a.lot_id.clone(),
                    lot_code: a.lot_code.clone(),
                    serial_number: a.serial_number.clone(),
                    manufactured_date: a.manufactured_date.clone(),
                    expiry_date: a.expiry_date.clone(),
                    quantity_milli: a.quantity_milli,
                    unit_cost_minor: a.unit_cost_minor,
                })
                .collect(),
        };
        validate_generic_revision_line(c, &history, &revision).await?;
        prepared.push((masters, history, revision));
    }
    for (_, history, revision) in &prepared {
        check_generic_revision_movement(c, history, &revision.count_date).await?;
    }
    if count["version"].as_i64() != Some(input.expected_version.unwrap_or(0)) {
        return Err(BusinessError::new(FailureKind::Conflict,"CONFLICT","The record changed, a reference is unavailable, or this value already exists. Refresh and review again.").into());
    }
    if prepared.len() != 1
        || prepared[0].2.product_id != value_string(&source.image["lines"][0], "product_id")?
    {
        return Err(changed().into());
    }
    let (masters, history, mut revision) = prepared.pop().ok_or_else(changed)?;
    revision.now = clock.now().get();
    let version = revision
        .expected_version
        .checked_add(1)
        .ok_or_else(changed)?;
    let plan = prepare_initial_count_revision(c, &source.original, &history, revision).await?;
    let fixed = CountCommandClock::sample(clock)?;
    let generated = CountCommandGeneratedIds::default();
    let completion = UpdateCompletion {
        source: &source,
        history: &history,
        masters: &masters,
        warehouse_masters: &warehouse_masters,
        plan: &plan,
        applied: AtomicBool::new(false),
        clock: fixed,
        generated: &generated,
        evidence: OnceLock::new(),
        retained: OnceLock::new(),
    };
    let mut effects = UpdateEffects {
        completion: &completion,
        version,
        id: command.id,
    };
    let mut authority = UpdateAuthority {
        source: &source,
        request: &request,
        completion: Some(&completion),
    };
    let mut wrapped = generated.wrap(ids);
    let outcome = PgCommandLedger::new(
        w.company_id.clone(),
        c,
        &mut authority,
        &fixed,
        &mut wrapped,
    )
    .execute(&request, &mut effects)
    .await?;
    completion.require(c).await?;
    serde_json::from_str(outcome.result.as_str()).map_err(|_| changed().into())
}
