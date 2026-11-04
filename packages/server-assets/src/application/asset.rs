//! Native Asset register, condition, placement, maintenance-lifecycle and
//! retirement use cases.
//!
//! The repository owns the current Holding transaction and SQL effects; this
//! layer owns the retained command order: current permission, exact replay,
//! current source, action policy, then the late expected-version guard. It
//! never mints Commercial purchase, IAM, Money or Workflow authority.
use crate::domain::asset::{
    AssetAction, AssetChange, AssetChangeInput, AssetState, AssetTransition, NewAsset, asset_after,
    json_integer, require_independent_retirement_review, validate_currency, validate_reason,
};
use crate::domain::maintenance::{MaintenanceAssetCondition, MaintenanceAssetLifecycle};
use erpx_server_shared::public::{
    BusinessError, CompanyId, DecimalAmount, FailureKind, RecordId, UserId,
};
use serde::Serialize;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type AssetFuture<'a, T, E = BusinessError> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ASSET_NOT_FOUND",
        "Asset not found in this scope.",
    )
}

/// Current authority supplied by the composition root. It is not a request
/// deserializer and grants no Commercial purchase by itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetActor {
    pub user_id: UserId,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
}
impl AssetActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }
    pub fn require(&self, permission: &str) -> Result<(), BusinessError> {
        if self.has(permission) {
            return Ok(());
        }
        Err(forbidden(
            "ASSETS_ACCESS_CHANGED",
            "Choose a current authorized Assets or Maintenance work role.",
        ))
    }
    pub fn require_module(&self) -> Result<(), BusinessError> {
        if self.modules.iter().any(|value| value == "assets") {
            return Ok(());
        }
        Err(forbidden(
            "MODULE_FORBIDDEN",
            "Your current role cannot access assets.",
        ))
    }
    pub fn can_read_finance(&self) -> bool {
        self.has("finance.read")
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Asset {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub code: String,
    pub name: String,
    pub category: String,
    pub currency: String,
    pub cost_minor: i64,
    pub acquired_date: Option<String>,
    pub warranty_end: Option<String>,
    pub finance_asset_id: Option<String>,
    pub finance_asset_version: Option<i64>,
    pub condition: String,
    pub lifecycle_status: String,
    pub placement: Option<String>,
    pub custodian_id: Option<String>,
    pub custodian_name: Option<String>,
    pub retirement_reason: Option<String>,
    pub version: i64,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub can_review: bool,
}
impl Asset {
    pub fn state(&self) -> Result<AssetState, BusinessError> {
        let invalid = || {
            BusinessError::new(
                FailureKind::Unavailable,
                "ASSET_SOURCE_INVALID",
                "The physical Asset record is unavailable.",
            )
        };
        let condition = match self.condition.as_str() {
            "good" => MaintenanceAssetCondition::Good,
            "degraded" => MaintenanceAssetCondition::Degraded,
            "unsafe" => MaintenanceAssetCondition::Unsafe,
            _ => return Err(invalid()),
        };
        let lifecycle = match self.lifecycle_status.as_str() {
            "registered" => MaintenanceAssetLifecycle::Registered,
            "in_service" => MaintenanceAssetLifecycle::InService,
            "maintenance" => MaintenanceAssetLifecycle::Maintenance,
            "retirement_pending" => MaintenanceAssetLifecycle::RetirementPending,
            "retired" => MaintenanceAssetLifecycle::Retired,
            _ => return Err(invalid()),
        };
        Ok(AssetState {
            version: self.version,
            condition,
            lifecycle,
            placement: self.placement.clone(),
            custodian_id: self.custodian_id.clone(),
            retirement_reason: self.retirement_reason.clone(),
            finance_asset_id: self.finance_asset_id.clone(),
            finance_asset_version: self.finance_asset_version,
            currency: self.currency.clone(),
            cost_minor: self.cost_minor,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AssetEvent {
    pub id: String,
    pub asset_id: String,
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    pub condition: String,
    pub lifecycle_status: String,
    pub placement: Option<String>,
    pub custodian_id: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct MaintenanceReference {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub owner_id: String,
    pub scheduled_at: Option<i64>,
    pub version: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct FinanceAsset {
    pub id: String,
    pub asset_number: String,
    pub name: Option<String>,
    pub status: String,
    pub currency: String,
    pub cost_minor: i64,
    pub accumulated_depreciation_minor: Option<i64>,
    pub version: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct FinanceCandidate {
    pub id: String,
    pub asset_number: String,
    pub name: Option<String>,
    pub currency: String,
    pub cost_minor: i64,
    pub version: i64,
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PersonOption {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct NodeOption {
    pub id: String,
    pub kind: String,
    pub display_name: String,
    pub branch_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AssetWorkspaceData {
    pub items: Vec<Asset>,
    pub people: Vec<PersonOption>,
    pub nodes: Vec<NodeOption>,
    pub finance_assets: Vec<FinanceCandidate>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetWorkspace {
    pub items: Vec<Asset>,
    pub people: Vec<PersonOption>,
    pub nodes: Vec<NodeOption>,
    pub finance_assets: Vec<FinanceCandidate>,
    pub can_link_finance: bool,
    pub can_create: bool,
    pub actor_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetDetail {
    pub items: Vec<Asset>,
    pub people: Vec<PersonOption>,
    pub nodes: Vec<NodeOption>,
    pub finance_assets: Vec<FinanceCandidate>,
    pub can_link_finance: bool,
    pub can_create: bool,
    pub actor_id: String,
    pub asset: Asset,
    pub finance: Option<FinanceAsset>,
    pub history: Vec<AssetEvent>,
    pub maintenance: Vec<MaintenanceReference>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AssetChangeResult {
    pub id: String,
    pub version: i64,
    pub status: String,
}
impl AssetChangeResult {
    pub fn value(&self) -> Value {
        let mut value = serde_json::Map::new();
        value.insert("id".into(), Value::String(self.id.clone()));
        value.insert("version".into(), Value::Number(self.version.into()));
        value.insert("status".into(), Value::String(self.status.clone()));
        Value::Object(value)
    }
}

pub struct AssetChangeOutcome<Receipt> {
    pub result: AssetChangeResult,
    pub receipt: Option<Receipt>,
}

/// Exact prior receipt lookup. The composition binds this to the actual
/// Workflow ledger; the owner adapter does not mint receipts itself.
pub trait AssetCommandLedger: Send {
    type Error: From<BusinessError> + Send;
    fn replay<'a>(
        &'a mut self,
        key: &'a str,
        operation: &'a str,
        record_id: &'a str,
        input: &'a Value,
    ) -> AssetFuture<'a, Option<Value>, Self::Error>;
}

/// Owner-effect tests and direct adapter composition only. This is not a
/// production replay ledger and must never replace Workflow in a mounted
/// command path.
pub struct NoCommandLedger<E> {
    marker: std::marker::PhantomData<E>,
}
impl<E> Default for NoCommandLedger<E> {
    fn default() -> Self {
        Self {
            marker: std::marker::PhantomData,
        }
    }
}
impl<E: From<BusinessError> + Send> AssetCommandLedger for NoCommandLedger<E> {
    type Error = E;
    fn replay<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a Value,
    ) -> AssetFuture<'a, Option<Value>, E> {
        Box::pin(async { Ok(None) })
    }
}

pub trait AssetRepository: Send {
    type Error: From<BusinessError> + Send;
    type Receipt: Send;

    fn workspace<'a>(
        &'a mut self,
        include_finance: bool,
    ) -> AssetFuture<'a, AssetWorkspaceData, Self::Error>;
    fn asset<'a>(&'a mut self, id: &'a str) -> AssetFuture<'a, Option<Asset>, Self::Error>;
    fn history<'a>(&'a mut self, id: &'a str) -> AssetFuture<'a, Vec<AssetEvent>, Self::Error>;
    fn maintenance<'a>(
        &'a mut self,
        id: &'a str,
    ) -> AssetFuture<'a, Vec<MaintenanceReference>, Self::Error>;
    fn finance<'a>(&'a mut self, id: &'a str)
    -> AssetFuture<'a, Option<FinanceAsset>, Self::Error>;
    fn retirement_requester<'a>(
        &'a mut self,
        id: &'a str,
    ) -> AssetFuture<'a, Option<UserId>, Self::Error>;
    fn has_posted_disposal<'a>(
        &'a mut self,
        finance_asset_id: Option<&'a str>,
    ) -> AssetFuture<'a, bool, Self::Error>;
    fn apply_change<'a>(
        &'a mut self,
        id: &'a str,
        change: &'a AssetChange,
        transition: &'a AssetTransition,
    ) -> AssetFuture<'a, (AssetChangeResult, Self::Receipt), Self::Error>;
    fn register<'a>(
        &'a mut self,
        id: &'a str,
        key: &'a str,
        asset: &'a NewAsset,
        at: i64,
    ) -> AssetFuture<'a, (AssetChangeResult, Self::Receipt), Self::Error>;
    fn currency_scale<'a>(
        &'a mut self,
        currency: &'a str,
    ) -> AssetFuture<'a, Option<u32>, Self::Error>;
}

/// Finance/Money handoff supplied by composition. It must create the
/// acquisition-cost snapshot in the same transaction before registration.
pub trait AcquisitionCostPort: Send {
    type Error: From<BusinessError> + Send;
    fn prepare<'a>(
        &'a mut self,
        id: &'a str,
        asset: &'a NewAsset,
    ) -> AssetFuture<'a, (), Self::Error>;
}

pub async fn asset_workspace<R: AssetRepository>(
    actor: &AssetActor,
    repository: &mut R,
) -> Result<AssetWorkspace, R::Error> {
    actor.require("records.read")?;
    actor.require_module()?;
    let data = repository.workspace(actor.can_read_finance()).await?;
    Ok(AssetWorkspace {
        items: data.items,
        people: data.people,
        nodes: data.nodes,
        finance_assets: data.finance_assets,
        can_link_finance: actor.has("records.write") && actor.has("finance.write"),
        can_create: actor.has("records.write"),
        actor_id: actor.user_id.as_str().to_owned(),
    })
}

pub async fn asset_detail<R: AssetRepository>(
    actor: &AssetActor,
    repository: &mut R,
    id: &str,
) -> Result<AssetDetail, R::Error> {
    actor.require("records.read")?;
    actor.require_module()?;
    let data = repository.workspace(actor.can_read_finance()).await?;
    let asset = data
        .items
        .iter()
        .find(|item| item.id == id)
        .cloned()
        .ok_or_else(not_found)?;
    let finance = match asset.finance_asset_id.as_deref() {
        Some(finance_id) if actor.can_read_finance() => repository.finance(finance_id).await?,
        _ => None,
    };
    Ok(AssetDetail {
        items: data.items,
        people: data.people,
        nodes: data.nodes,
        finance_assets: data.finance_assets,
        can_link_finance: actor.has("records.write") && actor.has("finance.write"),
        can_create: actor.has("records.write"),
        actor_id: actor.user_id.as_str().to_owned(),
        asset,
        finance,
        history: repository.history(id).await?,
        maintenance: repository.maintenance(id).await?,
    })
}

/// Acquire the one cost value accepted by the retained create schema. The
/// decimal form needs the current Finance currency scale; the minor-unit form
/// is already exact.
async fn registration_cost<R: AssetRepository>(
    repository: &mut R,
    raw: &Value,
) -> Result<i64, R::Error> {
    let object = raw.as_object().ok_or_else(|| {
        BusinessError::invalid(
            "VALIDATION_FAILED",
            "Review the Asset registration details.",
        )
    })?;
    let cost_minor = match (object.get("costMinor"), object.get("cost")) {
        (Some(value), None) => json_integer(value, 0).ok_or_else(|| {
            BusinessError::invalid(
                "VALIDATION_FAILED",
                "Acquisition cost must be a nonnegative safe integer.",
            )
        })?,
        (None, Some(Value::String(cost))) => {
            let currency = object
                .get("currency")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    BusinessError::invalid("VALIDATION_FAILED", "Use a three-letter ISO currency.")
                })
                .and_then(validate_currency)?;
            let scale = repository.currency_scale(&currency).await?.ok_or_else(|| {
                BusinessError::new(
                    FailureKind::InvalidInput,
                    "ASSET_CURRENCY_SCALE_REQUIRED",
                    "Configure the Company currency scale before entering a decimal acquisition cost.",
                )
            })?;
            let amount = DecimalAmount::parse(
                cost,
                u8::try_from(scale).map_err(|_| {
                    BusinessError::invalid("VALIDATION_FAILED", "Use the current currency scale.")
                })?,
            )?;
            i64::try_from(amount.units()).map_err(|_| {
                BusinessError::new(
                    FailureKind::InvalidInput,
                    "MONEY_OVERFLOW",
                    "Acquisition cost exceeds supported ledger precision.",
                )
            })?
        }
        _ => {
            return Err(BusinessError::invalid(
                "VALIDATION_FAILED",
                "Supply one acquisition cost.",
            )
            .into());
        }
    };
    Ok(cost_minor)
}

pub async fn create_asset<
    R: AssetRepository,
    L: AssetCommandLedger<Error = R::Error>,
    M: AcquisitionCostPort<Error = R::Error>,
>(
    actor: &AssetActor,
    repository: &mut R,
    ledger: &mut L,
    cost: &mut M,
    raw: &Value,
    key: &str,
    at: i64,
) -> Result<AssetChangeOutcome<R::Receipt>, R::Error> {
    actor.require("records.write")?;
    actor.require_module()?;
    let cost_minor = registration_cost(repository, raw).await?;
    let asset = NewAsset::parse(raw, cost_minor)?;
    let id = format!("asset-{key}");
    if let Some(prior) = ledger.replay(key, "asset.create", &id, raw).await? {
        return Ok(AssetChangeOutcome {
            result: AssetChangeResult {
                id,
                version: prior.get("version").and_then(Value::as_i64).unwrap_or(1),
                status: prior
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("registered")
                    .to_owned(),
            },
            receipt: None,
        });
    }
    if asset.finance_asset_id.is_some() {
        actor.require("finance.write")?;
    }
    cost.prepare(&id, &asset).await?;
    let scoped = asset.scoped(actor.branch_id.as_ref(), actor.department_id.as_ref());
    let (result, receipt) = repository.register(&id, key, &scoped, at).await?;
    Ok(AssetChangeOutcome {
        result,
        receipt: Some(receipt),
    })
}

/// Registration for an owner unit that also carries the Finance acquisition
/// handoff. One mutable borrow keeps the cost snapshot and the register row in
/// the same serializable transaction. Replay is checked before any cost or
/// owner effect.
pub async fn create_asset_in_unit<R, L>(
    actor: &AssetActor,
    unit: &mut R,
    ledger: &mut L,
    raw: &Value,
    key: &str,
    at: i64,
) -> Result<AssetChangeOutcome<<R as AssetRepository>::Receipt>, <R as AssetRepository>::Error>
where
    R: AssetRepository + AcquisitionCostPort<Error = <R as AssetRepository>::Error>,
    L: AssetCommandLedger<Error = <R as AssetRepository>::Error>,
{
    actor.require("records.write")?;
    actor.require_module()?;
    let cost_minor = registration_cost(unit, raw).await?;
    let asset = NewAsset::parse(raw, cost_minor)?;
    let id = format!("asset-{key}");
    if let Some(prior) = ledger.replay(key, "asset.create", &id, raw).await? {
        return Ok(AssetChangeOutcome {
            result: AssetChangeResult {
                id,
                version: prior.get("version").and_then(Value::as_i64).unwrap_or(1),
                status: prior
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("registered")
                    .to_owned(),
            },
            receipt: None,
        });
    }
    if asset.finance_asset_id.is_some() {
        actor.require("finance.write")?;
    }
    AcquisitionCostPort::prepare(unit, &id, &asset).await?;
    let scoped = asset.scoped(actor.branch_id.as_ref(), actor.department_id.as_ref());
    let (result, receipt) = unit.register(&id, key, &scoped, at).await?;
    Ok(AssetChangeOutcome {
        result,
        receipt: Some(receipt),
    })
}

pub async fn change_asset<R: AssetRepository, L: AssetCommandLedger<Error = R::Error>>(
    actor: &AssetActor,
    repository: &mut R,
    ledger: &mut L,
    id: &str,
    raw: &Value,
    key: &str,
    at: i64,
) -> Result<AssetChangeOutcome<R::Receipt>, R::Error> {
    actor.require("records.write")?;
    actor.require_module()?;
    let input = AssetChangeInput::parse(raw)?;
    let operation = input.action.operation();
    if let Some(prior) = ledger.replay(key, operation, id, raw).await? {
        return Ok(AssetChangeOutcome {
            result: AssetChangeResult {
                id: id.to_owned(),
                version: prior.get("version").and_then(Value::as_i64).unwrap_or(0),
                status: prior
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("registered")
                    .to_owned(),
            },
            receipt: None,
        });
    }
    let asset = repository.asset(id).await?.ok_or_else(not_found)?;
    let state = asset.state()?;
    match &input.action {
        AssetAction::LinkFinance { .. } => actor.require("finance.write")?,
        AssetAction::RetireApprove => actor.require("records.review")?,
        _ => {}
    }
    let prior_request = match input.action {
        AssetAction::RetireRequest | AssetAction::RetireApprove => {
            repository.retirement_requester(id).await?
        }
        _ => None,
    };
    if matches!(input.action, AssetAction::RetireApprove) {
        let requester = prior_request.clone().ok_or_else(|| {
            BusinessError::new(
                FailureKind::Conflict,
                "ASSET_RETIREMENT_NOT_PENDING",
                "Retirement is not awaiting review.",
            )
        })?;
        require_independent_retirement_review(&requester, &actor.user_id)?;
    }
    let has_posted_disposal =
        if matches!(input.action, AssetAction::RetireApprove) && state.cost_minor > 0 {
            repository
                .has_posted_disposal(state.finance_asset_id.as_deref())
                .await?
        } else {
            true
        };
    let change = AssetChange {
        expected_version: input.expected_version,
        action: input.action,
        actor_id: actor.user_id.clone(),
        command_key: key.to_owned(),
        reason: validate_reason(raw.get("reason").and_then(Value::as_str).ok_or_else(|| {
            BusinessError::invalid(
                "VALIDATION_FAILED",
                "Enter a reason between 3 and 2000 characters.",
            )
        })?)?,
        at,
    };
    change.validate()?;
    let transition = asset_after(&state, &change, prior_request.as_ref(), has_posted_disposal)?;
    let (result, receipt) = repository.apply_change(id, &change, &transition).await?;
    Ok(AssetChangeOutcome {
        result,
        receipt: Some(receipt),
    })
}
