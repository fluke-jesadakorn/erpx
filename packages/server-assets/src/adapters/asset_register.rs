//! PostgreSQL adapter for the native physical Asset register.
//!
//! The caller owns the Holding transaction, current WorkContext proof, IAM
//! source and Commercial purchase boundary. This adapter re-checks the current
//! scoped row, the late expected version and the immutable event history in
//! that same unit. It never opens a transaction or mints a Workflow receipt.
use crate::{
    application::asset::{
        AcquisitionCostPort, Asset, AssetChangeResult, AssetEvent, AssetFuture, AssetRepository,
        AssetWorkspaceData, FinanceAsset, FinanceCandidate, MaintenanceReference, NodeOption,
        PersonOption,
    },
    application::retirement_work::{AssetRetirementWorkEffect, AssetRetirementWorkSource, prepare_asset_retirement_work},
    domain::asset::{
        AssetChange, AssetTransition, MAX_SAFE_INTEGER, NewAsset, require_expected_version,
    },
    domain::maintenance::MaintenanceAssetLifecycle,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, UserId};
use sqlx::{Connection, PgConnection, Row};
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;
type Transaction = (String, i32, String, String);

/// Caller-owned Finance handoff for the native HTTP unit. It must write the
/// acquisition-cost snapshot on the supplied serializable Holding transaction;
/// the owner never mints a Money row itself.
pub trait AcquisitionCostProvider: Send + Sync {
    fn prepare<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        id: &'a str,
        asset: &'a NewAsset,
    ) -> AssetFuture<'a, (), BusinessError>;
}

/// A same-unit Finance effect retained until composition has written its
/// ledger/deferred effects. It is not Asset or actor authorization.
pub trait AcquisitionCostEffect: Send + Sync {
    fn require_current<'a>(&'a self, connection: &'a mut PgConnection) -> AssetFuture<'a, (), PgTransactionError<BusinessError>>;
}

/// Typed, retained acquisition handoff. Pre-snapshot locks are the caller's
/// responsibility; persistence errors preserve their SQLSTATE for retries.
pub trait RetainedAcquisitionCostProvider: Send + Sync {
    fn prepare_retained<'a>(&'a self, connection: &'a mut PgConnection, id: &'a str, asset: &'a NewAsset)
        -> AssetFuture<'a, Box<dyn AcquisitionCostEffect>, PgTransactionError<BusinessError>>;
}

const WORKSPACE_LIMIT: i64 = 2_000;
const OPTION_LIMIT: i64 = 1_000;
const HISTORY_LIMIT: i64 = 500;

fn read_limit() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ASSET_READ_LIMIT",
        "This Asset read exceeds the bounded workspace size. Narrow the current scope.",
    )
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ASSET_SOURCE_CHANGED",
        "The actual physical Asset source changed before commit. Refresh before retrying.",
    )
}
fn invalid_source() -> PersistenceError {
    PersistenceError::new("ASSET_SOURCE_INVALID")
}

async fn transaction(c: &mut PgConnection) -> Result<Transaction, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ASSET_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("ASSET_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as(
        "SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text",
    )
    .fetch_one(c)
    .await?)
}

async fn image_digest(
    c: &mut PgConnection,
    id: &str,
    company: &str,
) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT encode(sha256(convert_to(to_jsonb(a)::text,'UTF8')),'hex') FROM operational_assets a WHERE a.id=$1 AND a.company_id=$2",
    )
    .bind(id)
    .bind(company)
    .fetch_optional(c)
    .await?)
}

async fn event_digest(
    c: &mut PgConnection,
    event_id: &str,
    asset_id: &str,
) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex') FROM operational_asset_events e WHERE e.id=$1 AND e.asset_id=$2",
    )
    .bind(event_id)
    .bind(asset_id)
    .fetch_optional(c)
    .await?)
}

/// Retained after-image proof for one physical Asset command. The enclosing
/// coordinator calls `require_current` again after flushing deferred effects
/// and before the same unit commits.
pub struct AssetChangeReceipt {
    id: String,
    company: CompanyId,
    version: i64,
    event_id: String,
    expected_asset: String,
    expected_event: String,
    transaction: Transaction,
}
impl AssetChangeReceipt {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if transaction(c).await? != self.transaction
            || image_digest(c, &self.id, self.company.as_str())
                .await?
                .as_deref()
                != Some(self.expected_asset.as_str())
            || event_digest(c, &self.event_id, &self.id).await?.as_deref()
                != Some(self.expected_event.as_str())
        {
            return Err(changed().into());
        }
        Ok(())
    }
    pub fn version(&self) -> i64 {
        self.version
    }
}

pub struct PgAssetRegister<'c, 's> {
    connection: &'c mut PgConnection,
    actor: crate::application::asset::AssetActor,
    cost: Option<Arc<dyn AcquisitionCostProvider>>,
    retained_cost_provider: Option<Arc<dyn RetainedAcquisitionCostProvider>>,
    retained_cost_effect: Option<Box<dyn AcquisitionCostEffect>>,
    retirement_source: Option<Box<dyn AssetRetirementWorkSource<Effect = Box<dyn AssetRetirementWorkEffect + 's>> + 's>>,
    retirement_effect: Option<Box<dyn AssetRetirementWorkEffect + 's>>,
}
impl<'c, 's> PgAssetRegister<'c, 's> {
    pub fn new(
        connection: &'c mut PgConnection,
        actor: crate::application::asset::AssetActor,
    ) -> Self {
        Self {
            connection,
            actor,
            cost: None,
            retained_cost_provider: None,
            retained_cost_effect: None,
            retirement_source: None,
            retirement_effect: None,
        }
    }

    /// Bind the Finance acquisition-cost handoff written on this same
    /// serializable transaction. Without it, a positive-cost registration
    /// fails closed instead of leaving a physical Asset without Money evidence.
    pub fn with_cost_provider(mut self, provider: Arc<dyn AcquisitionCostProvider>) -> Self {
        self.cost = Some(provider);
        self
    }

    /// Select the typed retained path explicitly. It never falls back to the
    /// legacy provider if preparation fails or returns an error.
    pub fn with_retained_cost_provider(mut self, provider: Arc<dyn RetainedAcquisitionCostProvider>) -> Self {
        self.retained_cost_provider = Some(provider);
        self
    }

    /// Transfer the witness to composition, which must keep it through all
    /// deferred/receipt effects and call require_current before commit.
    pub fn take_acquisition_cost_effect(&mut self) -> Option<Box<dyn AcquisitionCostEffect>> {
        self.retained_cost_effect.take()
    }

    /// Bind the Maintenance owner source to this same unit; the coordinator
    /// takes all required source locks before the first snapshot.
    pub fn with_retirement_work_source<S>(mut self, source: S) -> Self
    where S: AssetRetirementWorkSource<Effect = Box<dyn AssetRetirementWorkEffect + 's>> + 's {
        self.retirement_source = Some(Box::new(source));
        self
    }

    /// Retain through all deferred effects and recheck before committing.
    pub fn take_retirement_work_effect(&mut self) -> Option<Box<dyn AssetRetirementWorkEffect + 's>> {
        self.retirement_effect.take()
    }

    fn scope_args(&self) -> (String, Option<String>, Option<String>) {
        (
            self.actor.company_id.as_str().to_owned(),
            self.actor
                .branch_id
                .as_ref()
                .map(|id| id.as_str().to_owned()),
            self.actor
                .department_id
                .as_ref()
                .map(|id| id.as_str().to_owned()),
        )
    }

    async fn rows(&mut self) -> Result<Vec<Asset>, Error> {
        let (company, branch, department) = self.scope_args();
        let rows = sqlx::query(
            "SELECT a.*,(SELECT legal_name FROM hr_people WHERE company_id=a.company_id AND user_id=a.custodian_id AND merged_into_id IS NULL LIMIT 1) custodian_name \
             FROM operational_assets a \
             WHERE a.company_id=$1 AND ($2::text IS NULL OR a.branch_id=$2) AND ($3::text IS NULL OR a.department_id=$3) \
             ORDER BY a.updated_at DESC,a.id LIMIT $4",
        )
        .bind(&company)
        .bind(&branch)
        .bind(&department)
        .bind(WORKSPACE_LIMIT + 1)
        .fetch_all(&mut *self.connection)
        .await?;
        if i64::try_from(rows.len()).unwrap_or(i64::MAX) > WORKSPACE_LIMIT {
            return Err(read_limit().into());
        }
        rows.into_iter().map(|row| self.map_asset(&row)).collect()
    }

    fn map_asset(&self, row: &sqlx::postgres::PgRow) -> Result<Asset, Error> {
        Ok(Asset {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            branch_id: row.try_get("branch_id")?,
            department_id: row.try_get("department_id")?,
            code: row.try_get("code")?,
            name: row.try_get("name")?,
            category: row.try_get("category")?,
            currency: row.try_get("currency")?,
            cost_minor: row.try_get("cost_minor")?,
            acquired_date: row.try_get("acquired_date")?,
            warranty_end: row.try_get("warranty_end")?,
            finance_asset_id: row.try_get("finance_asset_id")?,
            finance_asset_version: row.try_get("finance_asset_version")?,
            condition: row.try_get("condition")?,
            lifecycle_status: row.try_get("lifecycle_status")?,
            placement: row.try_get("placement")?,
            custodian_id: row.try_get("custodian_id")?,
            custodian_name: row.try_get("custodian_name")?,
            retirement_reason: row.try_get("retirement_reason")?,
            version: row.try_get("version")?,
            created_by: row.try_get("created_by")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            can_review: self.actor.has("records.review"),
        })
    }

    async fn current_row(&mut self, id: &str) -> Result<Option<Asset>, Error> {
        let (company, branch, department) = self.scope_args();
        let row = sqlx::query(
            "SELECT a.*,(SELECT legal_name FROM hr_people WHERE company_id=a.company_id AND user_id=a.custodian_id AND merged_into_id IS NULL LIMIT 1) custodian_name \
             FROM operational_assets a \
             WHERE a.id=$1 AND a.company_id=$2 AND ($3::text IS NULL OR a.branch_id=$3) AND ($4::text IS NULL OR a.department_id=$4) \
             FOR UPDATE",
        )
        .bind(id)
        .bind(&company)
        .bind(&branch)
        .bind(&department)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.as_ref().map(|row| self.map_asset(row)).transpose()
    }

    async fn latest_requester(&mut self, id: &str) -> Result<Option<UserId>, Error> {
        let actor: Option<String> = sqlx::query_scalar(
            "SELECT e.actor_id FROM operational_asset_events e JOIN operational_assets a ON a.id=e.asset_id AND a.company_id=$2 WHERE e.asset_id=$1 AND e.action='retire-request' ORDER BY e.version DESC LIMIT 1",
        )
        .bind(id)
        .bind(self.actor.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        actor.map(UserId::new).transpose().map_err(Into::into)
    }

    async fn posted_disposal(&mut self, finance_asset_id: Option<&str>) -> Result<bool, Error> {
        let Some(finance_asset_id) = finance_asset_id else {
            return Ok(false);
        };
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM finance_assets f \
             JOIN finance_asset_disposals d ON d.asset_id=f.id AND d.company_id=f.company_id \
             JOIN finance_journals j ON j.id=d.journal_id AND j.company_id=f.company_id \
             WHERE f.id=$1 AND f.company_id=$2 AND f.status='retired' AND d.status='posted' AND j.status='posted')",
        )
        .bind(finance_asset_id)
        .bind(self.actor.company_id.as_str())
        .fetch_one(&mut *self.connection)
        .await?)
    }

    /// Late command guards and owner effects. `transition` was derived from
    /// the same scoped row by the application; the version CAS below rejects a
    /// concurrent change before any write.
    async fn apply_transition(
        &mut self,
        id: &str,
        change: &crate::domain::asset::AssetChange,
        transition: &AssetTransition,
    ) -> Result<(AssetChangeResult, AssetChangeReceipt), Error> {
        let current = self.current_row(id).await?.ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "ASSET_NOT_FOUND",
                "Asset not found in this scope.",
            )
        })?;
        let state = current.state()?;
        require_expected_version(&state, change.expected_version)?;
        if transition.version != state.version + 1 {
            return Err(changed().into());
        }
        match &change.action {
            crate::domain::asset::AssetAction::Place {
                placement: _,
                custodian_id,
            } => {
                if state.cost_minor > 0 {
                    let recognized = if let Some(finance_asset_id) =
                        state.finance_asset_id.as_deref()
                    {
                        sqlx::query_scalar::<_, bool>(
                            "SELECT EXISTS(SELECT 1 FROM finance_assets WHERE id=$1 AND company_id=$2 AND status='in_service' AND activation_journal_id IS NOT NULL)",
                        )
                        .bind(finance_asset_id)
                        .bind(self.actor.company_id.as_str())
                        .fetch_one(&mut *self.connection)
                        .await?
                    } else {
                        false
                    };
                    if !recognized {
                        return Err(conflict(
                            "CONFLICT",
                            "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
                        )
                        .into());
                    }
                }
                let custodian = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=$1 AND p.user_id=$2 AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active')",
                )
                .bind(self.actor.company_id.as_str())
                .bind(custodian_id.as_str())
                .fetch_one(&mut *self.connection)
                .await?;
                if !custodian {
                    return Err(conflict(
                        "CONFLICT",
                        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
                    )
                    .into());
                }
            }
            crate::domain::asset::AssetAction::LinkFinance {
                finance_asset_id,
                finance_asset_version,
            } => {
                if state.finance_asset_id.is_some() {
                    return Err(conflict(
                        "ASSET_FINANCE_LINK_IMMUTABLE",
                        "The recorded Finance asset cannot be replaced.",
                    )
                    .into());
                }
                sqlx::query(
                    "SELECT id FROM finance_assets WHERE id=$1 AND company_id=$2 FOR SHARE",
                )
                .bind(finance_asset_id.as_str())
                .bind(self.actor.company_id.as_str())
                .fetch_optional(&mut *self.connection)
                .await?;
                let source = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM finance_assets f WHERE f.id=$1 AND f.company_id=$2 AND f.version=$3 AND f.currency=$4 AND f.cost_minor=$5 AND (f.status='in_service' OR (f.status='retired' AND $6='retirement_pending')))",
                )
                .bind(finance_asset_id.as_str())
                .bind(self.actor.company_id.as_str())
                .bind(finance_asset_version)
                .bind(&state.currency)
                .bind(state.cost_minor)
                .bind(state.lifecycle.as_str())
                .fetch_one(&mut *self.connection)
                .await?;
                if !source {
                    return Err(conflict(
                        "CONFLICT",
                        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
                    )
                    .into());
                }
            }
            crate::domain::asset::AssetAction::RetireRequest => {
                if state.lifecycle == MaintenanceAssetLifecycle::RetirementPending {
                    let requester = self.latest_requester(id).await?;
                    if requester.as_ref() != Some(&change.actor_id) {
                        return Err(BusinessError::new(
                            FailureKind::Forbidden,
                            "ASSET_RETIREMENT_REFRESH_FORBIDDEN",
                            "Only the original retirement requester can review and renew this pending request.",
                        )
                        .into());
                    }
                }
                // Maintenance owns the globally keyed work predicate and its
                // retained same-unit source. A missing handoff fails closed.
                let source = self.retirement_source.as_deref_mut().ok_or_else(||
                    PersistenceError::new("ASSET_RETIREMENT_WORK_SOURCE_REQUIRED"))?;
                let effect = prepare_asset_retirement_work(source, &mut *self.connection, id).await?;
                self.retirement_effect = Some(effect);
            }
            crate::domain::asset::AssetAction::RetireApprove => {
                self.require_independent_request(id, &change.actor_id)
                    .await?;
                if state.cost_minor > 0
                    && !self
                        .posted_disposal(state.finance_asset_id.as_deref())
                        .await?
                {
                    return Err(conflict(
                        "ASSET_FINANCE_DISPOSAL_REQUIRED",
                        "Finance must post the linked asset disposal before physical retirement can be approved.",
                    )
                    .into());
                }
            }
            crate::domain::asset::AssetAction::Condition { .. } => {}
        }
        let event_id = format!("asset-event-{}", change.command_key);
        let expected_asset = sqlx::query_scalar::<_, String>(
            "SELECT encode(sha256(convert_to((to_jsonb(a)||jsonb_build_object(\
             'lifecycle_status',$3::text,'condition',$4::text,'placement',$5::text,\
             'custodian_id',$6::text,'retirement_reason',$7::text,'version',$8::bigint,\
             'updated_at',$9::bigint,'finance_asset_id',$10::text,'finance_asset_version',$11::bigint))::text,'UTF8')),'hex') \
             FROM operational_assets a WHERE a.id=$1 AND a.company_id=$2",
        )
        .bind(id)
        .bind(self.actor.company_id.as_str())
        .bind(transition.lifecycle.as_str())
        .bind(transition.condition.as_str())
        .bind(&transition.placement)
        .bind(&transition.custodian_id)
        .bind(&transition.retirement_reason)
        .bind(transition.version)
        .bind(change.at)
        .bind(&transition.finance_asset_id)
        .bind(transition.finance_asset_version)
        .fetch_one(&mut *self.connection)
        .await?;
        let expected_event =
            expected_event_digest(&mut *self.connection, &event_id, id, change, transition).await?;
        let updated = sqlx::query(
            "UPDATE operational_assets SET lifecycle_status=$3,condition=$4,placement=$5,custodian_id=$6,\
             retirement_reason=coalesce($7,retirement_reason),finance_asset_id=$8,finance_asset_version=$9,\
             version=version+1,updated_at=$10 WHERE id=$1 AND company_id=$2 AND version=$11",
        )
        .bind(id)
        .bind(self.actor.company_id.as_str())
        .bind(transition.lifecycle.as_str())
        .bind(transition.condition.as_str())
        .bind(&transition.placement)
        .bind(&transition.custodian_id)
        .bind(&transition.retirement_reason)
        .bind(&transition.finance_asset_id)
        .bind(transition.finance_asset_version)
        .bind(change.at)
        .bind(change.expected_version)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(changed().into());
        }
        sqlx::query(
            "INSERT INTO operational_asset_events(id,asset_id,version,action,actor_id,reason,condition,lifecycle_status,placement,custodian_id,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(&event_id)
        .bind(id)
        .bind(change.expected_version + 1)
        .bind(change.action.name())
        .bind(change.actor_id.as_str())
        .bind(&change.reason)
        .bind(transition.condition.as_str())
        .bind(transition.lifecycle.as_str())
        .bind(&transition.placement)
        .bind(&transition.custodian_id)
        .bind(change.at)
        .execute(&mut *self.connection)
        .await?;
        if matches!(
            change.action,
            crate::domain::asset::AssetAction::LinkFinance { .. }
        ) && let (Some(finance_asset_id), Some(finance_asset_version)) = (
            transition.finance_asset_id.as_deref(),
            transition.finance_asset_version,
        ) {
            sqlx::query(
                "INSERT INTO operational_asset_finance_links(id,asset_id,finance_asset_id,asset_version,finance_asset_version,cost_minor,currency,reason,created_by,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            )
            .bind(format!("asset-finance-link-{}", change.command_key))
            .bind(id)
            .bind(finance_asset_id)
            .bind(change.expected_version + 1)
            .bind(finance_asset_version)
            .bind(state.cost_minor)
            .bind(&state.currency)
            .bind(&change.reason)
            .bind(change.actor_id.as_str())
            .bind(change.at)
            .execute(&mut *self.connection)
            .await?;
        }
        let receipt = AssetChangeReceipt {
            id: id.to_owned(),
            company: self.actor.company_id.clone(),
            version: transition.version,
            event_id,
            expected_asset,
            expected_event,
            transaction: transaction(&mut *self.connection).await?,
        };
        receipt.require_current(&mut *self.connection).await?;
        Ok((
            AssetChangeResult {
                id: id.to_owned(),
                version: transition.version,
                status: transition.lifecycle.as_str().to_owned(),
            },
            receipt,
        ))
    }

    /// Review must be independent from the latest retained request and must
    /// still describe the current pending row. Later condition updates are
    /// allowed; any later lifecycle/placement/custodian change is not.
    async fn require_independent_request(
        &mut self,
        id: &str,
        approver: &UserId,
    ) -> Result<(), Error> {
        let row = sqlx::query(
            "SELECT e.actor_id,e.lifecycle_status,e.reason,e.placement,e.custodian_id \
             FROM operational_asset_events e JOIN operational_assets a ON a.id=e.asset_id AND a.company_id=$2 \
             WHERE e.asset_id=$1 AND e.action='retire-request' ORDER BY e.version DESC LIMIT 1",
        )
        .bind(id)
        .bind(self.actor.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Err(conflict(
                "ASSET_RETIREMENT_NOT_PENDING",
                "Retirement is not awaiting review.",
            )
            .into());
        };
        let requester: String = row.try_get("actor_id")?;
        if requester == approver.as_str() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ASSET_RETIREMENT_REVIEW_INDEPENDENT",
                "A different reviewer must approve this retirement.",
            )
            .into());
        }
        let current = self.current_row(id).await?.ok_or_else(changed)?;
        let matches = row.try_get::<String, _>("lifecycle_status")? == "retirement_pending"
            && row.try_get::<String, _>("reason")?
                == current.retirement_reason.clone().unwrap_or_default()
            && row.try_get::<Option<String>, _>("placement")? == current.placement
            && row.try_get::<Option<String>, _>("custodian_id")? == current.custodian_id;
        if !matches {
            return Err(changed().into());
        }
        Ok(())
    }
}

async fn expected_event_digest(
    c: &mut PgConnection,
    event_id: &str,
    asset_id: &str,
    change: &AssetChange,
    transition: &AssetTransition,
) -> Result<String, Error> {
    Ok(sqlx::query_scalar(
        "SELECT encode(sha256(convert_to(jsonb_build_object(\
         'id',$1::text,'asset_id',$2::text,'version',$3::bigint,'action',$4::text,\
         'actor_id',$5::text,'reason',$6::text,'condition',$7::text,'lifecycle_status',$8::text,\
         'placement',$9::text,'custodian_id',$10::text,'created_at',$11::bigint)::text,'UTF8')),'hex')",
    )
    .bind(event_id)
    .bind(asset_id)
    .bind(change.expected_version + 1)
    .bind(change.action.name())
    .bind(change.actor_id.as_str())
    .bind(&change.reason)
    .bind(transition.condition.as_str())
    .bind(transition.lifecycle.as_str())
    .bind(&transition.placement)
    .bind(&transition.custodian_id)
    .bind(change.at)
    .fetch_one(c)
    .await?)
}

impl AssetRepository for PgAssetRegister<'_, '_> {
    type Error = Error;
    type Receipt = AssetChangeReceipt;

    fn workspace<'a>(
        &'a mut self,
        include_finance: bool,
    ) -> AssetFuture<'a, AssetWorkspaceData, Error> {
        Box::pin(async move {
            let items = self.rows().await?;
            let people = sqlx::query(
                "SELECT DISTINCT p.user_id id,p.legal_name name FROM hr_people p \
                 JOIN hr_engagements e ON e.person_id=p.id WHERE p.company_id=$1 AND p.user_id IS NOT NULL \
                 AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' ORDER BY p.legal_name LIMIT $2",
            )
            .bind(self.actor.company_id.as_str())
            .bind(OPTION_LIMIT + 1)
            .fetch_all(&mut *self.connection)
            .await?;
            if i64::try_from(people.len()).unwrap_or(i64::MAX) > OPTION_LIMIT {
                return Err(read_limit().into());
            }
            let people = people
                .into_iter()
                .map(|row| {
                    Ok(PersonOption {
                        id: row.try_get("id")?,
                        name: row.try_get("name")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?;
            let nodes = sqlx::query(
                "SELECT id,kind,display_name,branch_id FROM organization_nodes WHERE company_id=$1 AND status='active' AND kind IN ('branch','headquarters','department') ORDER BY kind,display_name LIMIT $2",
            )
            .bind(self.actor.company_id.as_str())
            .bind(OPTION_LIMIT + 1)
            .fetch_all(&mut *self.connection)
            .await?;
            if i64::try_from(nodes.len()).unwrap_or(i64::MAX) > OPTION_LIMIT {
                return Err(read_limit().into());
            }
            let nodes = nodes
                .into_iter()
                .map(|row| {
                    Ok(NodeOption {
                        id: row.try_get("id")?,
                        kind: row.try_get("kind")?,
                        display_name: row.try_get("display_name")?,
                        branch_id: row.try_get("branch_id")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?;
            let finance_assets = if include_finance {
                let rows = sqlx::query(
                    "SELECT f.id,f.asset_number,f.name,f.currency,f.cost_minor,f.version,f.status \
                     FROM finance_assets f WHERE f.company_id=$1 AND f.status IN ('in_service','retired') \
                     AND NOT EXISTS(SELECT 1 FROM operational_assets a WHERE a.finance_asset_id=f.id) ORDER BY f.asset_number LIMIT $2",
                )
                .bind(self.actor.company_id.as_str())
                .bind(OPTION_LIMIT + 1)
                .fetch_all(&mut *self.connection)
                .await?;
                if i64::try_from(rows.len()).unwrap_or(i64::MAX) > OPTION_LIMIT {
                    return Err(read_limit().into());
                }
                rows.into_iter()
                    .map(|row| {
                        Ok(FinanceCandidate {
                            id: row.try_get("id")?,
                            asset_number: row.try_get("asset_number")?,
                            name: row.try_get("name")?,
                            currency: row.try_get("currency")?,
                            cost_minor: row.try_get("cost_minor")?,
                            version: row.try_get("version")?,
                            status: row.try_get("status")?,
                        })
                    })
                    .collect::<Result<Vec<_>, sqlx::Error>>()?
            } else {
                Vec::new()
            };
            Ok(AssetWorkspaceData {
                items,
                people,
                nodes,
                finance_assets,
            })
        })
    }

    fn asset<'a>(&'a mut self, id: &'a str) -> AssetFuture<'a, Option<Asset>, Error> {
        Box::pin(async move {
            let (company, branch, department) = self.scope_args();
            let row = sqlx::query(
                "SELECT a.*,(SELECT legal_name FROM hr_people WHERE company_id=a.company_id AND user_id=a.custodian_id AND merged_into_id IS NULL LIMIT 1) custodian_name \
                 FROM operational_assets a WHERE a.id=$1 AND a.company_id=$2 \
                 AND ($3::text IS NULL OR a.branch_id=$3) AND ($4::text IS NULL OR a.department_id=$4)",
            )
            .bind(id)
            .bind(&company)
            .bind(&branch)
            .bind(&department)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.as_ref().map(|row| self.map_asset(row)).transpose()
        })
    }

    fn history<'a>(&'a mut self, id: &'a str) -> AssetFuture<'a, Vec<AssetEvent>, Error> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT e.id,e.asset_id,e.version,e.action,e.actor_id,e.reason,e.condition,e.lifecycle_status,e.placement,e.custodian_id,e.created_at \
                 FROM operational_asset_events e JOIN operational_assets a ON a.id=e.asset_id AND a.company_id=$2 \
                 WHERE e.asset_id=$1 ORDER BY e.version LIMIT $3",
            )
            .bind(id)
            .bind(self.actor.company_id.as_str())
            .bind(HISTORY_LIMIT + 1)
            .fetch_all(&mut *self.connection)
            .await?;
            if i64::try_from(rows.len()).unwrap_or(i64::MAX) > HISTORY_LIMIT {
                return Err(read_limit().into());
            }
            Ok(rows
                .into_iter()
                .map(|row| {
                    Ok(AssetEvent {
                        id: row.try_get("id")?,
                        asset_id: row.try_get("asset_id")?,
                        version: row.try_get("version")?,
                        action: row.try_get("action")?,
                        actor_id: row.try_get("actor_id")?,
                        reason: row.try_get("reason")?,
                        condition: row.try_get("condition")?,
                        lifecycle_status: row.try_get("lifecycle_status")?,
                        placement: row.try_get("placement")?,
                        custodian_id: row.try_get("custodian_id")?,
                        created_at: row.try_get("created_at")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?)
        })
    }

    fn maintenance<'a>(
        &'a mut self,
        id: &'a str,
    ) -> AssetFuture<'a, Vec<MaintenanceReference>, Error> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT id,title,status,priority,owner_id,scheduled_at,version FROM maintenance_work_orders \
                 WHERE asset_id=$1 AND company_id=$2 ORDER BY updated_at DESC,id LIMIT $3",
            )
            .bind(id)
            .bind(self.actor.company_id.as_str())
            .bind(HISTORY_LIMIT + 1)
            .fetch_all(&mut *self.connection)
            .await?;
            if i64::try_from(rows.len()).unwrap_or(i64::MAX) > HISTORY_LIMIT {
                return Err(read_limit().into());
            }
            Ok(rows
                .into_iter()
                .map(|row| {
                    Ok(MaintenanceReference {
                        id: row.try_get("id")?,
                        title: row.try_get("title")?,
                        status: row.try_get("status")?,
                        priority: row.try_get("priority")?,
                        owner_id: row.try_get("owner_id")?,
                        scheduled_at: row.try_get("scheduled_at")?,
                        version: row.try_get("version")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?)
        })
    }

    fn finance<'a>(&'a mut self, id: &'a str) -> AssetFuture<'a, Option<FinanceAsset>, Error> {
        Box::pin(async move {
            sqlx::query(
                "SELECT id,asset_number,name,status,currency,cost_minor,accumulated_depreciation_minor,version \
                 FROM finance_assets WHERE id=$1 AND company_id=$2",
            )
            .bind(id)
            .bind(self.actor.company_id.as_str())
            .fetch_optional(&mut *self.connection)
            .await?
            .map(|row| {
                Ok::<FinanceAsset, Error>(FinanceAsset {
                    id: row.try_get("id")?,
                    asset_number: row.try_get("asset_number")?,
                    name: row.try_get("name")?,
                    status: row.try_get("status")?,
                    currency: row.try_get("currency")?,
                    cost_minor: row.try_get("cost_minor")?,
                    accumulated_depreciation_minor: row
                        .try_get("accumulated_depreciation_minor")?,
                    version: row.try_get("version")?,
                })
            })
            .transpose()
        })
    }

    fn retirement_requester<'a>(
        &'a mut self,
        id: &'a str,
    ) -> AssetFuture<'a, Option<UserId>, Error> {
        Box::pin(async move { self.latest_requester(id).await })
    }

    fn has_posted_disposal<'a>(
        &'a mut self,
        finance_asset_id: Option<&'a str>,
    ) -> AssetFuture<'a, bool, Error> {
        Box::pin(async move { self.posted_disposal(finance_asset_id).await })
    }

    fn apply_change<'a>(
        &'a mut self,
        id: &'a str,
        change: &'a crate::domain::asset::AssetChange,
        transition: &'a AssetTransition,
    ) -> AssetFuture<'a, (AssetChangeResult, AssetChangeReceipt), Error> {
        Box::pin(async move { self.apply_transition(id, change, transition).await })
    }

    fn register<'a>(
        &'a mut self,
        id: &'a str,
        key: &'a str,
        asset: &'a NewAsset,
        at: i64,
    ) -> AssetFuture<'a, (AssetChangeResult, AssetChangeReceipt), Error> {
        Box::pin(async move {
            if !(0..=MAX_SAFE_INTEGER).contains(&at) {
                return Err(invalid_source().into());
            }
            let branch = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 AND kind IN ('branch','headquarters') AND status='active')",
            )
            .bind(asset.branch_id.as_str())
            .bind(self.actor.company_id.as_str())
            .fetch_one(&mut *self.connection)
            .await?;
            if !branch {
                return Err(conflict(
                    "CONFLICT",
                    "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
                )
                .into());
            }
            if let Some(department) = asset.department_id.as_ref() {
                let exists = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 AND branch_id=$3 AND kind='department' AND status='active')",
                )
                .bind(department.as_str())
                .bind(self.actor.company_id.as_str())
                .bind(asset.branch_id.as_str())
                .fetch_one(&mut *self.connection)
                .await?;
                if !exists {
                    return Err(conflict(
                        "CONFLICT",
                        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
                    )
                    .into());
                }
            }
            let duplicate = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM operational_assets WHERE company_id=$1 AND code=$2)",
            )
            .bind(self.actor.company_id.as_str())
            .bind(&asset.code)
            .fetch_one(&mut *self.connection)
            .await?;
            if duplicate {
                return Err(conflict(
                    "ASSET_CODE_EXISTS",
                    "This asset code is already used in the Company.",
                )
                .into());
            }
            if let (Some(finance_asset_id), Some(finance_asset_version)) =
                (asset.finance_asset_id.as_ref(), asset.finance_asset_version)
            {
                sqlx::query(
                    "SELECT id FROM finance_assets WHERE id=$1 AND company_id=$2 FOR SHARE",
                )
                .bind(finance_asset_id.as_str())
                .bind(self.actor.company_id.as_str())
                .fetch_optional(&mut *self.connection)
                .await?;
                let source = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM finance_assets WHERE id=$1 AND company_id=$2 AND version=$3 AND status='in_service' AND cost_minor=$4 AND currency=$5)",
                )
                .bind(finance_asset_id.as_str())
                .bind(self.actor.company_id.as_str())
                .bind(finance_asset_version)
                .bind(asset.cost_minor)
                .bind(&asset.currency)
                .fetch_one(&mut *self.connection)
                .await?;
                if !source {
                    return Err(conflict(
                        "CONFLICT",
                        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
                    )
                    .into());
                }
            }
            sqlx::query(
                "INSERT INTO operational_assets(id,company_id,branch_id,department_id,code,name,category,currency,cost_minor,acquired_date,warranty_end,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$13)",
            )
            .bind(id)
            .bind(self.actor.company_id.as_str())
            .bind(asset.branch_id.as_str())
            .bind(asset.department_id.as_deref())
            .bind(&asset.code)
            .bind(&asset.name)
            .bind(&asset.category)
            .bind(&asset.currency)
            .bind(asset.cost_minor)
            .bind(&asset.acquired_date)
            .bind(&asset.warranty_end)
            .bind(self.actor.user_id.as_str())
            .bind(at)
            .execute(&mut *self.connection)
            .await?;
            if let (Some(finance_asset_id), Some(finance_asset_version)) =
                (asset.finance_asset_id.as_ref(), asset.finance_asset_version)
            {
                sqlx::query("UPDATE operational_assets SET finance_asset_id=$2,finance_asset_version=$3 WHERE id=$1")
                    .bind(id)
                    .bind(finance_asset_id.as_str())
                    .bind(finance_asset_version)
                    .execute(&mut *self.connection)
                    .await?;
                sqlx::query(
                    "INSERT INTO operational_asset_finance_links(id,asset_id,finance_asset_id,asset_version,finance_asset_version,cost_minor,currency,reason,created_by,created_at) \
                     VALUES($1,$2,$3,1,$4,$5,$6,$7,$8,$9)",
                )
                .bind(format!("asset-finance-link-{key}"))
                .bind(id)
                .bind(finance_asset_id.as_str())
                .bind(finance_asset_version)
                .bind(asset.cost_minor)
                .bind(&asset.currency)
                .bind("Finance recognition linked at physical registration")
                .bind(self.actor.user_id.as_str())
                .bind(at)
                .execute(&mut *self.connection)
                .await?;
            }
            let event_id = format!("asset-event-{key}");
            sqlx::query(
                "INSERT INTO operational_asset_events(id,asset_id,version,action,actor_id,reason,condition,lifecycle_status,placement,custodian_id,created_at) \
                 VALUES($1,$2,1,'register',$3,'Asset registered','good','registered',NULL,NULL,$4)",
            )
            .bind(&event_id)
            .bind(id)
            .bind(self.actor.user_id.as_str())
            .bind(at)
            .execute(&mut *self.connection)
            .await?;
            let expected_asset =
                image_digest(&mut *self.connection, id, self.actor.company_id.as_str())
                    .await?
                    .ok_or_else(changed)?;
            let expected_event = event_digest(&mut *self.connection, &event_id, id)
                .await?
                .ok_or_else(changed)?;
            let receipt = AssetChangeReceipt {
                id: id.to_owned(),
                company: self.actor.company_id.clone(),
                version: 1,
                event_id,
                expected_asset,
                expected_event,
                transaction: transaction(&mut *self.connection).await?,
            };
            receipt.require_current(&mut *self.connection).await?;
            Ok((
                AssetChangeResult {
                    id: id.to_owned(),
                    version: 1,
                    status: "registered".to_owned(),
                },
                receipt,
            ))
        })
    }

    fn currency_scale<'a>(&'a mut self, currency: &'a str) -> AssetFuture<'a, Option<u32>, Error> {
        Box::pin(async move {
            let scale: Option<i32> = sqlx::query_scalar(
                "SELECT scale::int FROM company_currency_scales WHERE company_id=$1 AND currency=$2",
            )
            .bind(self.actor.company_id.as_str())
            .bind(currency)
            .fetch_optional(&mut *self.connection)
            .await?;
            scale
                .map(|scale| u32::try_from(scale).map_err(|_| invalid_source().into()))
                .transpose()
        })
    }
}

impl AcquisitionCostPort for PgAssetRegister<'_, '_> {
    type Error = Error;

    fn prepare<'a>(
        &'a mut self,
        id: &'a str,
        asset: &'a NewAsset,
    ) -> AssetFuture<'a, (), Self::Error> {
        Box::pin(async move {
            if let Some(provider) = self.retained_cost_provider.as_ref() {
                if self.retained_cost_effect.is_some() {
                    return Err(BusinessError::new(FailureKind::Conflict,"ASSET_ACQUISITION_EFFECT_ALREADY_PREPARED","The acquisition-cost effect is already prepared.").into());
                }
                let effect = provider.prepare_retained(&mut *self.connection, id, asset).await?;
                self.retained_cost_effect = Some(effect);
                return Ok(());
            }
            if let Some(provider) = self.cost.as_ref() {
                return provider
                    .prepare(&mut *self.connection, id, asset)
                    .await
                    .map_err(Into::into);
            }
            if asset.cost_minor == 0 {
                return Ok(());
            }
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "ASSET_ACQUISITION_COST_UNAVAILABLE",
                "Finance acquisition-cost posting is unavailable for this registration.",
            )
            .into())
        })
    }
}
