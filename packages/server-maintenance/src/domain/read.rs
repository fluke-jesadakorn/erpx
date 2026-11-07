//! Native Maintenance read facts for the owner workspace and HTTP ingress.
//! These projections are presentation facts only. They are not a current
//! authorization proof, purchase admission or command capability.
use erpx_server_runtime::public::http_support::{Value, json};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis, UserId,
};

#[derive(Clone, Debug)]
pub struct MaintenanceReadActor {
    pub user_id: UserId,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}
impl MaintenanceReadActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }
    pub fn can_write(&self) -> bool {
        self.has("records.write")
    }
    pub fn can_review(&self) -> bool {
        self.has("records.review")
    }
    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("records.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission.",
            ));
        }
        if !self.modules.iter().any(|module| module == "maintenance") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MODULE_FORBIDDEN",
                "Your current role cannot access maintenance.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct MaintenanceAssetOption {
    pub id: String,
    pub code: String,
    pub name: String,
    pub version: i64,
    pub condition: String,
    pub lifecycle_status: String,
}
impl MaintenanceAssetOption {
    pub fn value(&self) -> Value {
        json!({
            "id": self.id,
            "code": self.code,
            "name": self.name,
            "version": self.version,
            "condition": self.condition,
            "lifecycle_status": self.lifecycle_status,
        })
    }
}

#[derive(Clone, Debug)]
pub struct MaintenancePersonOption {
    pub id: String,
    pub name: String,
}
impl MaintenancePersonOption {
    pub fn value(&self) -> Value {
        json!({"id": self.id, "name": self.name})
    }
}

#[derive(Clone, Debug)]
pub struct MaintenanceNodeOption {
    pub id: String,
    pub kind: String,
    pub display_name: String,
    pub branch_id: Option<String>,
}
impl MaintenanceNodeOption {
    pub fn value(&self) -> Value {
        json!({
            "id": self.id,
            "kind": self.kind,
            "display_name": self.display_name,
            "branch_id": self.branch_id,
        })
    }
}

#[derive(Clone, Debug)]
pub struct MaintenanceWorkRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub asset_id: String,
    pub title: String,
    pub priority: String,
    pub status: String,
    pub owner_id: String,
    pub scheduled_at: Option<i64>,
    pub downtime_started_at: Option<i64>,
    pub downtime_ended_at: Option<i64>,
    pub blocked_reason: Option<String>,
    pub completion_evidence: Option<String>,
    pub resulting_condition: Option<String>,
    pub version: i64,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub asset_code: String,
    pub asset_name: String,
    pub owner_name: Option<String>,
    /// Presentation queue hints, never authority. Commands re-check the actual
    /// current People/IAM references.
    pub owner_eligible: i64,
    pub can_review: i64,
}
impl MaintenanceWorkRow {
    pub fn value(&self) -> Value {
        json!({
            "id": self.id,
            "company_id": self.company_id,
            "branch_id": self.branch_id,
            "department_id": self.department_id,
            "asset_id": self.asset_id,
            "title": self.title,
            "priority": self.priority,
            "status": self.status,
            "owner_id": self.owner_id,
            "scheduled_at": self.scheduled_at,
            "downtime_started_at": self.downtime_started_at,
            "downtime_ended_at": self.downtime_ended_at,
            "blocked_reason": self.blocked_reason,
            "completion_evidence": self.completion_evidence,
            "resulting_condition": self.resulting_condition,
            "version": self.version,
            "created_by": self.created_by,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "asset_code": self.asset_code,
            "asset_name": self.asset_name,
            "owner_name": self.owner_name,
            "owner_eligible": self.owner_eligible,
            "can_review": self.can_review,
        })
    }
}

#[derive(Clone, Debug)]
pub struct MaintenanceWorkEventRow {
    pub id: String,
    pub work_order_id: String,
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    pub status: String,
    pub owner_id: String,
    pub scheduled_at: Option<i64>,
    pub downtime_started_at: Option<i64>,
    pub downtime_ended_at: Option<i64>,
    pub created_at: i64,
}
impl MaintenanceWorkEventRow {
    pub fn value(&self) -> Value {
        json!({
            "id": self.id,
            "work_order_id": self.work_order_id,
            "version": self.version,
            "action": self.action,
            "actor_id": self.actor_id,
            "reason": self.reason,
            "status": self.status,
            "owner_id": self.owner_id,
            "scheduled_at": self.scheduled_at,
            "downtime_started_at": self.downtime_started_at,
            "downtime_ended_at": self.downtime_ended_at,
            "created_at": self.created_at,
        })
    }
}

#[derive(Clone, Debug)]
pub struct MaintenancePlanRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub asset_id: String,
    pub title: String,
    pub priority: String,
    pub owner_id: String,
    pub interval_days: i32,
    pub time_zone: String,
    pub first_due_local: String,
    pub next_due_local: String,
    pub next_due_at: i64,
    pub lead_days: i32,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub edited_by: String,
    pub approved_by: Option<String>,
    pub approved_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub asset_code: String,
    pub asset_name: String,
    pub owner_name: Option<String>,
    pub owner_eligible: i64,
}
impl MaintenancePlanRow {
    pub fn value(&self) -> Value {
        json!({
            "id": self.id,
            "company_id": self.company_id,
            "branch_id": self.branch_id,
            "department_id": self.department_id,
            "asset_id": self.asset_id,
            "title": self.title,
            "priority": self.priority,
            "owner_id": self.owner_id,
            "interval_days": self.interval_days,
            "time_zone": self.time_zone,
            "first_due_local": self.first_due_local,
            "next_due_local": self.next_due_local,
            "next_due_at": self.next_due_at,
            "lead_days": self.lead_days,
            "status": self.status,
            "version": self.version,
            "created_by": self.created_by,
            "edited_by": self.edited_by,
            "approved_by": self.approved_by,
            "approved_at": self.approved_at,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "asset_code": self.asset_code,
            "asset_name": self.asset_name,
            "owner_name": self.owner_name,
            "owner_eligible": self.owner_eligible,
        })
    }
}

#[derive(Clone, Debug)]
pub struct MaintenancePlanEventRow {
    pub id: String,
    pub plan_id: String,
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    pub created_at: i64,
}
impl MaintenancePlanEventRow {
    pub fn value(&self) -> Value {
        json!({
            "id": self.id,
            "plan_id": self.plan_id,
            "version": self.version,
            "action": self.action,
            "actor_id": self.actor_id,
            "reason": self.reason,
            "created_at": self.created_at,
        })
    }
}

#[derive(Clone, Debug)]
pub struct MaintenanceOccurrenceRow {
    pub id: String,
    pub plan_id: String,
    pub plan_version: i64,
    pub work_order_id: String,
    pub due_local: String,
    pub due_at: i64,
    pub source_asset_version: i64,
    pub generated_by: String,
    pub generated_at: i64,
    pub status: String,
    pub work_version: i64,
    pub definition_json: String,
}
impl MaintenanceOccurrenceRow {
    pub fn value(&self) -> Value {
        json!({
            "id": self.id,
            "plan_id": self.plan_id,
            "plan_version": self.plan_version,
            "work_order_id": self.work_order_id,
            "due_local": self.due_local,
            "due_at": self.due_at,
            "source_asset_version": self.source_asset_version,
            "generated_by": self.generated_by,
            "generated_at": self.generated_at,
            "status": self.status,
            "work_version": self.work_version,
            "definition_json": self.definition_json,
        })
    }
}

#[derive(Clone, Debug)]
pub struct MaintenanceWorkspace {
    pub items: Vec<MaintenanceWorkRow>,
    pub assets: Vec<MaintenanceAssetOption>,
    pub people: Vec<MaintenancePersonOption>,
    pub nodes: Vec<MaintenanceNodeOption>,
    pub can_create: bool,
}
impl MaintenanceWorkspace {
    pub fn value(&self) -> Value {
        json!({
            "items": self.items.iter().map(MaintenanceWorkRow::value).collect::<Vec<_>>(),
            "assets": self.assets.iter().map(MaintenanceAssetOption::value).collect::<Vec<_>>(),
            "people": self.people.iter().map(MaintenancePersonOption::value).collect::<Vec<_>>(),
            "nodes": self.nodes.iter().map(MaintenanceNodeOption::value).collect::<Vec<_>>(),
            "canCreate": self.can_create,
        })
    }
}

#[derive(Clone, Debug)]
pub struct MaintenanceDetail {
    pub workspace: MaintenanceWorkspace,
    pub work: MaintenanceWorkRow,
    pub history: Vec<MaintenanceWorkEventRow>,
    pub consumptions: Vec<Value>,
    pub spares: Value,
}
impl MaintenanceDetail {
    pub fn value(&self) -> Value {
        let mut value = self.workspace.value();
        let object = value.as_object_mut().expect("workspace object");
        object.insert("work".into(), self.work.value());
        object.insert(
            "history".into(),
            Value::Array(self.history.iter().map(MaintenanceWorkEventRow::value).collect()),
        );
        object.insert("consumptions".into(), Value::Array(self.consumptions.clone()));
        object.insert("spares".into(), self.spares.clone());
        value
    }
}

#[derive(Clone, Debug)]
pub struct MaintenancePlanDetail {
    pub plan: MaintenancePlanRow,
    pub actor_id: String,
    pub can_write: bool,
    pub can_review: bool,
    pub asset_version: Option<i64>,
    pub people: Vec<MaintenancePersonOption>,
    pub history: Vec<MaintenancePlanEventRow>,
    pub occurrences: Vec<MaintenanceOccurrenceRow>,
}
impl MaintenancePlanDetail {
    pub fn value(&self) -> Value {
        json!({
            "plan": self.plan.value(),
            "actorId": self.actor_id,
            "canWrite": self.can_write,
            "canReview": self.can_review,
            "assetVersion": self.asset_version,
            "people": self.people.iter().map(MaintenancePersonOption::value).collect::<Vec<_>>(),
            "history": self.history.iter().map(MaintenancePlanEventRow::value).collect::<Vec<_>>(),
            "occurrences": self.occurrences.iter().map(MaintenanceOccurrenceRow::value).collect::<Vec<_>>(),
        })
    }
}
