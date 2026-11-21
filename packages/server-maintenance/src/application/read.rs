//! Native Maintenance read use cases. The repository owns the retained
//! serializable source read; these functions only enforce selected read policy
//! and assemble owner presentation facts. They grant no write or purchase.
use crate::domain::read::{
    MaintenanceAssetOption, MaintenanceDetail, MaintenanceDowntimeEvidenceRow,
    MaintenanceNodeOption, MaintenanceOccurrenceRow, MaintenancePersonOption, MaintenancePlanDetail,
    MaintenancePlanEventRow, MaintenancePlanRow, MaintenanceReadActor, MaintenanceWorkEventRow,
    MaintenanceWorkRow, MaintenanceWorkspace,
};
use serde_json::json;
use erpx_server_shared::public::{BusinessError, FailureKind, TimestampMillis};
use std::future::Future;

pub type MaintenanceReadFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Retained Executive downtime evidence query. Numbers are the same bounded
/// window/cursor contract; the adapter clamps the row limit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceDowntimeQuery {
    pub from: i64,
    pub to: i64,
    pub query: String,
    pub limit: i32,
    pub cursor: Option<MaintenanceDowntimeCursor>,
    pub record_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceDowntimeCursor {
    pub ended_at: i64,
    pub id: String,
}

/// Repository queries stay on the caller's retained serializable transaction.
/// Source eligibility is a current People/IAM read, never a submitted label.
pub trait MaintenanceReadRepository: Send {
    type Error: From<BusinessError> + Send;
    fn works(&mut self) -> MaintenanceReadFuture<'_, Vec<MaintenanceWorkRow>, Self::Error>;
    fn history<'a>(
        &'a mut self,
        id: &'a str,
    ) -> MaintenanceReadFuture<'a, Vec<MaintenanceWorkEventRow>, Self::Error>;
    fn plans(&mut self) -> MaintenanceReadFuture<'_, Vec<MaintenancePlanRow>, Self::Error>;
    fn plan<'a>(
        &'a mut self,
        id: &'a str,
    ) -> MaintenanceReadFuture<'a, Option<MaintenancePlanRow>, Self::Error>;
    fn plan_history<'a>(
        &'a mut self,
        id: &'a str,
    ) -> MaintenanceReadFuture<'a, Vec<MaintenancePlanEventRow>, Self::Error>;
    fn occurrences<'a>(
        &'a mut self,
        id: &'a str,
    ) -> MaintenanceReadFuture<'a, Vec<MaintenanceOccurrenceRow>, Self::Error>;
    fn assets(&mut self) -> MaintenanceReadFuture<'_, Vec<MaintenanceAssetOption>, Self::Error>;
    fn people(&mut self) -> MaintenanceReadFuture<'_, Vec<MaintenancePersonOption>, Self::Error>;
    fn nodes(&mut self) -> MaintenanceReadFuture<'_, Vec<MaintenanceNodeOption>, Self::Error>;
    fn eligible<'a>(
        &'a mut self,
        user_id: &'a str,
        permission: &'a str,
        branch_id: &'a str,
        department_id: Option<&'a str>,
    ) -> MaintenanceReadFuture<'a, bool, Self::Error>;
    fn downtime(
        &mut self,
        query: &MaintenanceDowntimeQuery,
    ) -> MaintenanceReadFuture<'_, Vec<MaintenanceDowntimeEvidenceRow>, Self::Error>;
}

fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "MAINTENANCE_WORK_NOT_FOUND",
        "Maintenance work order not found in this scope.",
    )
}
fn plan_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "MAINTENANCE_PLAN_NOT_FOUND",
        "Preventive plan not found in this scope.",
    )
}
fn asset_map(assets: &[MaintenanceAssetOption]) -> std::collections::BTreeMap<&str, (&str, &str)> {
    assets
        .iter()
        .map(|asset| (asset.id.as_str(), (asset.code.as_str(), asset.name.as_str())))
        .collect()
}
fn owner_name(people: &[MaintenancePersonOption], owner: &str) -> Option<String> {
    people
        .iter()
        .find(|person| person.id == owner)
        .map(|person| person.name.clone())
}
async fn fill_work<R: MaintenanceReadRepository>(
    repository: &mut R,
    actor: &MaintenanceReadActor,
    mut row: MaintenanceWorkRow,
    people: &[MaintenancePersonOption],
) -> Result<MaintenanceWorkRow, R::Error> {
    row.owner_name = owner_name(people, &row.owner_id);
    row.owner_eligible = if repository
        .eligible(
            &row.owner_id,
            "records.write",
            &row.branch_id,
            row.department_id.as_deref(),
        )
        .await?
    {
        1
    } else {
        0
    };
    row.can_review = if repository
        .eligible(
            actor.user_id.as_str(),
            "records.review",
            &row.branch_id,
            row.department_id.as_deref(),
        )
        .await?
    {
        1
    } else {
        0
    };
    Ok(row)
}

pub async fn maintenance_workspace<R: MaintenanceReadRepository>(
    actor: &MaintenanceReadActor,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<MaintenanceWorkspace, R::Error> {
    actor.require_read(now)?;
    let mut works = repository.works().await?;
    let assets = repository.assets().await?;
    let people = repository.people().await?;
    let nodes = repository.nodes().await?;
    let assets_by_id = asset_map(&assets);
    for work in &mut works {
        if let Some((code, name)) = assets_by_id.get(work.asset_id.as_str()) {
            work.asset_code = (*code).to_owned();
            work.asset_name = (*name).to_owned();
        }
    }
    let mut items = Vec::with_capacity(works.len());
    for work in works {
        items.push(fill_work(repository, actor, work, &people).await?);
    }
    Ok(MaintenanceWorkspace {
        items,
        assets,
        people,
        nodes,
        can_create: actor.can_write(),
    })
}

pub async fn maintenance_detail<R: MaintenanceReadRepository>(
    actor: &MaintenanceReadActor,
    repository: &mut R,
    id: &str,
    now: TimestampMillis,
) -> Result<MaintenanceDetail, R::Error> {
    let workspace = maintenance_workspace(actor, repository, now).await?;
    let work = workspace
        .items
        .iter()
        .find(|item| item.id == id)
        .cloned()
        .ok_or_else(not_found)?;
    Ok(MaintenanceDetail {
        history: repository.history(id).await?,
        consumptions: Vec::new(),
        spares: json!({"stock": [], "lots": []}),
        workspace,
        work,
    })
}

async fn fill_plan<R: MaintenanceReadRepository>(
    repository: &mut R,
    mut plan: MaintenancePlanRow,
    assets: &[MaintenanceAssetOption],
    people: &[MaintenancePersonOption],
) -> Result<MaintenancePlanRow, R::Error> {
    if let Some(asset) = assets.iter().find(|asset| asset.id == plan.asset_id) {
        plan.asset_code = asset.code.clone();
        plan.asset_name = asset.name.clone();
    }
    plan.owner_name = owner_name(people, &plan.owner_id);
    plan.owner_eligible = if repository
        .eligible(
            &plan.owner_id,
            "records.write",
            &plan.branch_id,
            plan.department_id.as_deref(),
        )
        .await?
    {
        1
    } else {
        0
    };
    Ok(plan)
}

pub async fn maintenance_plans<R: MaintenanceReadRepository>(
    actor: &MaintenanceReadActor,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Vec<MaintenancePlanRow>, R::Error> {
    actor.require_read(now)?;
    let plans = repository.plans().await?;
    let assets = repository.assets().await?;
    let people = repository.people().await?;
    let mut items = Vec::with_capacity(plans.len());
    for plan in plans {
        items.push(fill_plan(repository, plan, &assets, &people).await?);
    }
    Ok(items)
}

pub async fn maintenance_plan_detail<R: MaintenanceReadRepository>(
    actor: &MaintenanceReadActor,
    repository: &mut R,
    id: &str,
    now: TimestampMillis,
) -> Result<MaintenancePlanDetail, R::Error> {
    actor.require_read(now)?;
    let plan = repository.plan(id).await?.ok_or_else(plan_not_found)?;
    let assets = repository.assets().await?;
    let people = repository.people().await?;
    let plan = fill_plan(repository, plan, &assets, &people).await?;
    let asset_version = assets
        .iter()
        .find(|asset| asset.id == plan.asset_id)
        .map(|asset| asset.version);
    Ok(MaintenancePlanDetail {
        actor_id: actor.user_id.as_str().to_owned(),
        can_write: actor.can_write(),
        can_review: actor.can_review(),
        asset_version,
        people,
        history: repository.plan_history(id).await?,
        occurrences: repository.occurrences(id).await?,
        plan,
    })
}

/// Executive/read projection of completed and cancelled downtime windows. The
/// selected read permission, module and scope are re-checked here; the adapter
/// returns the exact bounded owner rows.
pub async fn maintenance_downtime_evidence<R: MaintenanceReadRepository>(
    actor: &MaintenanceReadActor,
    repository: &mut R,
    query: &MaintenanceDowntimeQuery,
    now: TimestampMillis,
) -> Result<Vec<MaintenanceDowntimeEvidenceRow>, R::Error> {
    actor.require_read(now)?;
    repository.downtime(query).await
}
