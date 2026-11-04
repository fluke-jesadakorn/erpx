//! PostgreSQL read adapter for the native Maintenance workspace.
//!
//! The caller owns the retained serializable Holding transaction and the
//! selected `AuthorizedWorkContext`. Work/event/plan tables are Maintenance
//! owned; physical Asset, people and node options are read through the public
//! Assets owner adapter. No command, purchase or independent transaction is
//! created here.
use crate::application::read::{
    MaintenanceReadFuture, MaintenanceReadRepository,
};
use crate::domain::read::{
    MaintenanceAssetOption, MaintenanceNodeOption, MaintenanceOccurrenceRow,
    MaintenancePersonOption, MaintenancePlanEventRow, MaintenancePlanRow, MaintenanceWorkEventRow,
    MaintenanceWorkRow,
};
use erpx_server_assets::public::asset_register::{
    AssetActor, AssetRepository, PgAssetRegister,
};
use erpx_server_identity_access::public::{
    effective_authority::AuthorizedWorkContext,
    operational_worker::{
        OperationalWorkerPermission, RawOperationalWorkerRequest, inspect_operational_worker_raw,
    },
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use sqlx::{Connection, PgConnection, Row};
use std::collections::HashMap;

type Error = PgTransactionError<BusinessError>;

const WORK_LIMIT: i64 = 2_000;
const PLAN_LIMIT: i64 = 300;
const HISTORY_LIMIT: i64 = 500;

fn read_limit(code: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        code,
        "This Maintenance read exceeds the bounded workspace size. Narrow the current scope.",
    )
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MAINTENANCE_READ_UNAVAILABLE",
        "The native Maintenance read source is unavailable.",
    )
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your selected work context changed. Review the current workspace and role.",
    )
}
async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("MAINTENANCE_READ_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("MAINTENANCE_READ_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgMaintenanceRead<'a> {
    connection: &'a mut PgConnection,
    current: &'a AuthorizedWorkContext,
    now: TimestampMillis,
    eligibility: HashMap<(String, String, String, Option<String>), bool>,
}
impl<'a> PgMaintenanceRead<'a> {
    pub fn new(
        connection: &'a mut PgConnection,
        current: &'a AuthorizedWorkContext,
        now: TimestampMillis,
    ) -> Self {
        Self {
            connection,
            current,
            now,
            eligibility: HashMap::new(),
        }
    }

    fn scope(&self) -> (String, Option<String>, Option<String>) {
        let workspace = self.current.selection().workspace();
        (
            workspace.company_id.as_str().to_owned(),
            workspace.branch_id.as_ref().map(RecordId::as_str).map(str::to_owned),
            workspace.department_id.as_ref().map(RecordId::as_str).map(str::to_owned),
        )
    }

    fn asset_actor(&self) -> AssetActor {
        let workspace = self.current.selection().workspace();
        AssetActor {
            user_id: self.current.user_id().clone(),
            company_id: workspace.company_id.clone(),
            branch_id: workspace.branch_id.clone(),
            department_id: workspace.department_id.clone(),
            permissions: self.current.command_iam_metadata().permissions.clone(),
            modules: self.current.modules().to_vec(),
        }
    }

    fn work_row(&self, row: &sqlx::postgres::PgRow) -> Result<MaintenanceWorkRow, Error> {
        Ok(MaintenanceWorkRow {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            branch_id: row.try_get("branch_id")?,
            department_id: row.try_get("department_id")?,
            asset_id: row.try_get("asset_id")?,
            title: row.try_get("title")?,
            priority: row.try_get("priority")?,
            status: row.try_get("status")?,
            owner_id: row.try_get("owner_id")?,
            scheduled_at: row.try_get("scheduled_at")?,
            downtime_started_at: row.try_get("downtime_started_at")?,
            downtime_ended_at: row.try_get("downtime_ended_at")?,
            blocked_reason: row.try_get("blocked_reason")?,
            completion_evidence: row.try_get("completion_evidence")?,
            resulting_condition: row.try_get("resulting_condition")?,
            version: row.try_get("version")?,
            created_by: row.try_get("created_by")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            asset_code: String::new(),
            asset_name: String::new(),
            owner_name: None,
            owner_eligible: 0,
            can_review: 0,
        })
    }

    fn plan_row(&self, row: &sqlx::postgres::PgRow) -> Result<MaintenancePlanRow, Error> {
        Ok(MaintenancePlanRow {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            branch_id: row.try_get("branch_id")?,
            department_id: row.try_get("department_id")?,
            asset_id: row.try_get("asset_id")?,
            title: row.try_get("title")?,
            priority: row.try_get("priority")?,
            owner_id: row.try_get("owner_id")?,
            interval_days: row.try_get("interval_days")?,
            time_zone: row.try_get("time_zone")?,
            first_due_local: row.try_get("first_due_local")?,
            next_due_local: row.try_get("next_due_local")?,
            next_due_at: row.try_get("next_due_at")?,
            lead_days: row.try_get("lead_days")?,
            status: row.try_get("status")?,
            version: row.try_get("version")?,
            created_by: row.try_get("created_by")?,
            edited_by: row.try_get("edited_by")?,
            approved_by: row.try_get("approved_by")?,
            approved_at: row.try_get("approved_at")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            asset_code: String::new(),
            asset_name: String::new(),
            owner_name: None,
            owner_eligible: 0,
        })
    }
}

impl MaintenanceReadRepository for PgMaintenanceRead<'_> {
    type Error = Error;

    fn works(&mut self) -> MaintenanceReadFuture<'_, Vec<MaintenanceWorkRow>, Error> {
        Box::pin(async move {
            let (company, branch, department) = self.scope();
            let rows = sqlx::query(
                "SELECT id,company_id,branch_id,department_id,asset_id,title,priority,status,owner_id,\
                 scheduled_at,downtime_started_at,downtime_ended_at,blocked_reason,completion_evidence,\
                 resulting_condition,version,created_by,created_at,updated_at \
                 FROM maintenance_work_orders WHERE company_id=$1 \
                 AND ($2::text IS NULL OR branch_id=$2) AND ($3::text IS NULL OR department_id=$3) \
                 ORDER BY updated_at DESC,id LIMIT $4",
            )
            .bind(&company)
            .bind(&branch)
            .bind(&department)
            .bind(WORK_LIMIT + 1)
            .fetch_all(&mut *self.connection)
            .await?;
            if i64::try_from(rows.len()).unwrap_or(i64::MAX) > WORK_LIMIT {
                return Err(read_limit("MAINTENANCE_READ_LIMIT").into());
            }
            rows.iter().map(|row| self.work_row(row)).collect()
        })
    }

    fn history<'a>(
        &'a mut self,
        id: &'a str,
    ) -> MaintenanceReadFuture<'a, Vec<MaintenanceWorkEventRow>, Error> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT e.id,e.work_order_id,e.version,e.action,e.actor_id,e.reason,e.status,\
                 e.owner_id,e.scheduled_at,e.downtime_started_at,e.downtime_ended_at,e.created_at \
                 FROM maintenance_work_events e JOIN maintenance_work_orders w \
                 ON w.id=e.work_order_id AND w.company_id=$2 \
                 WHERE e.work_order_id=$1 ORDER BY e.version LIMIT $3",
            )
            .bind(id)
            .bind(self.scope().0)
            .bind(HISTORY_LIMIT + 1)
            .fetch_all(&mut *self.connection)
            .await?;
            if i64::try_from(rows.len()).unwrap_or(i64::MAX) > HISTORY_LIMIT {
                return Err(read_limit("MAINTENANCE_HISTORY_LIMIT").into());
            }
            rows.iter()
                .map(|row| {
                    Ok(MaintenanceWorkEventRow {
                        id: row.try_get("id")?,
                        work_order_id: row.try_get("work_order_id")?,
                        version: row.try_get("version")?,
                        action: row.try_get("action")?,
                        actor_id: row.try_get("actor_id")?,
                        reason: row.try_get("reason")?,
                        status: row.try_get("status")?,
                        owner_id: row.try_get("owner_id")?,
                        scheduled_at: row.try_get("scheduled_at")?,
                        downtime_started_at: row.try_get("downtime_started_at")?,
                        downtime_ended_at: row.try_get("downtime_ended_at")?,
                        created_at: row.try_get("created_at")?,
                    })
                })
                .collect()
        })
    }

    fn plans(&mut self) -> MaintenanceReadFuture<'_, Vec<MaintenancePlanRow>, Error> {
        Box::pin(async move {
            let (company, branch, department) = self.scope();
            let rows = sqlx::query(
                "SELECT id,company_id,branch_id,department_id,asset_id,title,priority,owner_id,\
                 interval_days,time_zone,first_due_local,next_due_local,next_due_at,lead_days,status,\
                 version,created_by,edited_by,approved_by,approved_at,created_at,updated_at \
                 FROM maintenance_preventive_plans WHERE company_id=$1 \
                 AND ($2::text IS NULL OR branch_id=$2) AND ($3::text IS NULL OR department_id=$3) \
                 ORDER BY next_due_at,id LIMIT $4",
            )
            .bind(&company)
            .bind(&branch)
            .bind(&department)
            .bind(PLAN_LIMIT + 1)
            .fetch_all(&mut *self.connection)
            .await?;
            if i64::try_from(rows.len()).unwrap_or(i64::MAX) > PLAN_LIMIT {
                return Err(read_limit("MAINTENANCE_PLAN_LIMIT").into());
            }
            rows.iter().map(|row| self.plan_row(row)).collect()
        })
    }

    fn plan<'a>(
        &'a mut self,
        id: &'a str,
    ) -> MaintenanceReadFuture<'a, Option<MaintenancePlanRow>, Error> {
        Box::pin(async move {
            let (company, branch, department) = self.scope();
            let row = sqlx::query(
                "SELECT id,company_id,branch_id,department_id,asset_id,title,priority,owner_id,\
                 interval_days,time_zone,first_due_local,next_due_local,next_due_at,lead_days,status,\
                 version,created_by,edited_by,approved_by,approved_at,created_at,updated_at \
                 FROM maintenance_preventive_plans WHERE id=$1 AND company_id=$2 \
                 AND ($3::text IS NULL OR branch_id=$3) AND ($4::text IS NULL OR department_id=$4)",
            )
            .bind(id)
            .bind(&company)
            .bind(&branch)
            .bind(&department)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.as_ref().map(|row| self.plan_row(row)).transpose()
        })
    }

    fn plan_history<'a>(
        &'a mut self,
        id: &'a str,
    ) -> MaintenanceReadFuture<'a, Vec<MaintenancePlanEventRow>, Error> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT e.id,e.plan_id,e.version,e.action,e.actor_id,e.reason,e.created_at \
                 FROM maintenance_preventive_events e JOIN maintenance_preventive_plans p \
                 ON p.id=e.plan_id AND p.company_id=$2 WHERE e.plan_id=$1 \
                 ORDER BY e.version LIMIT $3",
            )
            .bind(id)
            .bind(self.scope().0)
            .bind(HISTORY_LIMIT + 1)
            .fetch_all(&mut *self.connection)
            .await?;
            if i64::try_from(rows.len()).unwrap_or(i64::MAX) > HISTORY_LIMIT {
                return Err(read_limit("MAINTENANCE_PLAN_HISTORY_LIMIT").into());
            }
            rows.iter()
                .map(|row| {
                    Ok(MaintenancePlanEventRow {
                        id: row.try_get("id")?,
                        plan_id: row.try_get("plan_id")?,
                        version: row.try_get("version")?,
                        action: row.try_get("action")?,
                        actor_id: row.try_get("actor_id")?,
                        reason: row.try_get("reason")?,
                        created_at: row.try_get("created_at")?,
                    })
                })
                .collect()
        })
    }

    fn occurrences<'a>(
        &'a mut self,
        id: &'a str,
    ) -> MaintenanceReadFuture<'a, Vec<MaintenanceOccurrenceRow>, Error> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT o.id,o.plan_id,o.plan_version,o.work_order_id,o.due_local,o.due_at,\
                 o.source_asset_version,o.generated_by,o.generated_at,w.status,w.version work_version \
                 FROM maintenance_preventive_occurrences o \
                 JOIN maintenance_preventive_plans p ON p.id=o.plan_id AND p.company_id=$2 \
                 JOIN maintenance_work_orders w ON w.id=o.work_order_id \
                 WHERE o.plan_id=$1 ORDER BY o.due_at DESC LIMIT $3",
            )
            .bind(id)
            .bind(self.scope().0)
            .bind(HISTORY_LIMIT + 1)
            .fetch_all(&mut *self.connection)
            .await?;
            if i64::try_from(rows.len()).unwrap_or(i64::MAX) > HISTORY_LIMIT {
                return Err(read_limit("MAINTENANCE_OCCURRENCE_LIMIT").into());
            }
            rows.iter()
                .map(|row| {
                    Ok(MaintenanceOccurrenceRow {
                        id: row.try_get("id")?,
                        plan_id: row.try_get("plan_id")?,
                        plan_version: row.try_get("plan_version")?,
                        work_order_id: row.try_get("work_order_id")?,
                        due_local: row.try_get("due_local")?,
                        due_at: row.try_get("due_at")?,
                        source_asset_version: row.try_get("source_asset_version")?,
                        generated_by: row.try_get("generated_by")?,
                        generated_at: row.try_get("generated_at")?,
                        status: row.try_get("status")?,
                        work_version: row.try_get("work_version")?,
                    })
                })
                .collect()
        })
    }

    fn assets(&mut self) -> MaintenanceReadFuture<'_, Vec<MaintenanceAssetOption>, Error> {
        Box::pin(async move {
            let actor = self.asset_actor();
            let mut register = PgAssetRegister::new(self.connection, actor);
            let workspace = register.workspace(false).await?;
            Ok(workspace
                .items
                .into_iter()
                .filter(|asset| {
                    matches!(
                        asset.lifecycle_status.as_str(),
                        "registered" | "in_service" | "maintenance"
                    )
                })
                .map(|asset| MaintenanceAssetOption {
                    id: asset.id,
                    code: asset.code,
                    name: asset.name,
                    version: asset.version,
                    condition: asset.condition,
                    lifecycle_status: asset.lifecycle_status,
                })
                .collect())
        })
    }

    fn people(&mut self) -> MaintenanceReadFuture<'_, Vec<MaintenancePersonOption>, Error> {
        Box::pin(async move {
            let actor = self.asset_actor();
            let mut register = PgAssetRegister::new(self.connection, actor);
            let workspace = register.workspace(false).await?;
            Ok(workspace
                .people
                .into_iter()
                .map(|person| MaintenancePersonOption {
                    id: person.id,
                    name: person.name,
                })
                .collect())
        })
    }

    fn nodes(&mut self) -> MaintenanceReadFuture<'_, Vec<MaintenanceNodeOption>, Error> {
        Box::pin(async move {
            let actor = self.asset_actor();
            let mut register = PgAssetRegister::new(self.connection, actor);
            let workspace = register.workspace(false).await?;
            Ok(workspace
                .nodes
                .into_iter()
                .map(|node| MaintenanceNodeOption {
                    id: node.id,
                    kind: node.kind,
                    display_name: node.display_name,
                    branch_id: node.branch_id,
                })
                .collect())
        })
    }

    fn eligible<'a>(
        &'a mut self,
        user_id: &'a str,
        permission: &'a str,
        branch_id: &'a str,
        department_id: Option<&'a str>,
    ) -> MaintenanceReadFuture<'a, bool, Error> {
        Box::pin(async move {
            let key = (
                user_id.to_owned(),
                permission.to_owned(),
                branch_id.to_owned(),
                department_id.map(str::to_owned),
            );
            if let Some(value) = self.eligibility.get(&key) {
                return Ok(*value);
            }
            let permission = match permission {
                "records.write" => OperationalWorkerPermission::Write,
                "records.review" => OperationalWorkerPermission::Review,
                _ => return Err(unavailable().into()),
            };
            let branch = RecordId::new(branch_id).map_err(|_| unavailable())?;
            let department = department_id
                .map(RecordId::new)
                .transpose()
                .map_err(|_| unavailable())?;
            let company = self.current.selection().workspace().company_id.clone();
            let worker = inspect_operational_worker_raw(
                &mut *self.connection,
                RawOperationalWorkerRequest {
                    company_id: company,
                    user_id: user_id.to_owned(),
                    branch_id: branch,
                    department_id: department,
                    permission,
                },
                self.now,
            )
            .await?;
            let value = worker.is_eligible();
            self.eligibility.insert(key, value);
            Ok(value)
        })
    }
}

/// Open the actual owner read on the caller's already-retained serializable
/// transaction. No transaction is begun or committed here.
pub async fn read_maintenance_workspace(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    now: TimestampMillis,
) -> Result<crate::domain::read::MaintenanceWorkspace, Error> {
    serializable(c).await?;
    if now.get() >= current.expires_at() {
        return Err(changed().into());
    }
    let mut repository = PgMaintenanceRead::new(&mut *c, current, now);
    crate::application::read::maintenance_workspace(
        &crate::domain::read::MaintenanceReadActor {
            user_id: current.user_id().clone(),
            company_id: current.selection().workspace().company_id.clone(),
            branch_id: current.selection().workspace().branch_id.clone(),
            department_id: current.selection().workspace().department_id.clone(),
            permissions: current.command_iam_metadata().permissions.clone(),
            modules: current.modules().to_vec(),
            expires_at: current.expires_at(),
        },
        &mut repository,
        now,
    )
    .await
}

pub async fn read_maintenance_detail(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &str,
    now: TimestampMillis,
) -> Result<crate::domain::read::MaintenanceDetail, Error> {
    serializable(c).await?;
    if now.get() >= current.expires_at() {
        return Err(changed().into());
    }
    let mut repository = PgMaintenanceRead::new(&mut *c, current, now);
    crate::application::read::maintenance_detail(
        &crate::domain::read::MaintenanceReadActor {
            user_id: current.user_id().clone(),
            company_id: current.selection().workspace().company_id.clone(),
            branch_id: current.selection().workspace().branch_id.clone(),
            department_id: current.selection().workspace().department_id.clone(),
            permissions: current.command_iam_metadata().permissions.clone(),
            modules: current.modules().to_vec(),
            expires_at: current.expires_at(),
        },
        &mut repository,
        id,
        now,
    )
    .await
}

pub async fn read_maintenance_plans(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    now: TimestampMillis,
) -> Result<Vec<crate::domain::read::MaintenancePlanRow>, Error> {
    serializable(c).await?;
    if now.get() >= current.expires_at() {
        return Err(changed().into());
    }
    let mut repository = PgMaintenanceRead::new(&mut *c, current, now);
    crate::application::read::maintenance_plans(
        &crate::domain::read::MaintenanceReadActor {
            user_id: current.user_id().clone(),
            company_id: current.selection().workspace().company_id.clone(),
            branch_id: current.selection().workspace().branch_id.clone(),
            department_id: current.selection().workspace().department_id.clone(),
            permissions: current.command_iam_metadata().permissions.clone(),
            modules: current.modules().to_vec(),
            expires_at: current.expires_at(),
        },
        &mut repository,
        now,
    )
    .await
}

pub async fn read_maintenance_plan_detail(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &str,
    now: TimestampMillis,
) -> Result<crate::domain::read::MaintenancePlanDetail, Error> {
    serializable(c).await?;
    if now.get() >= current.expires_at() {
        return Err(changed().into());
    }
    let mut repository = PgMaintenanceRead::new(&mut *c, current, now);
    crate::application::read::maintenance_plan_detail(
        &crate::domain::read::MaintenanceReadActor {
            user_id: current.user_id().clone(),
            company_id: current.selection().workspace().company_id.clone(),
            branch_id: current.selection().workspace().branch_id.clone(),
            department_id: current.selection().workspace().department_id.clone(),
            permissions: current.command_iam_metadata().permissions.clone(),
            modules: current.modules().to_vec(),
            expires_at: current.expires_at(),
        },
        &mut repository,
        id,
        now,
    )
    .await
}

/// Exact scope check for a configured HTTP target. It is data only.
pub fn require_read_scope(
    current: &AuthorizedWorkContext,
    company: &CompanyId,
) -> Result<(), BusinessError> {
    if &current.selection().workspace().company_id != company {
        return Err(changed());
    }
    Ok(())
}
