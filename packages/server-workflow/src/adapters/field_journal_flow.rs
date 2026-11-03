//! Field-origin journal assignment: Workflow-owned route/default/history only.
//! Composition must retain native Finance/Field source and current authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FLOW_SOURCE_CHANGED",
        "The Field journal workflow changed. Reload its current source.",
    )
    .into()
}
async fn unit(c: &mut PgConnection) -> Result<(String, i32, String, String), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("WORKFLOW_OWNER_TRANSACTION_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub async fn lock_field_journal_flow_sources(c: &mut PgConnection) -> Result<(), Error> {
    unit(c).await?;
    sqlx::query("LOCK TABLE flow_routes,flow_defaults,flow_events IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
#[derive(Clone,PartialEq, Eq)]
pub struct FieldJournalFlowBasis {
    unit: (String, i32, String, String),
    company: CompanyId,
    id: String,
    route: Option<Value>,
    defaults: Vec<Value>,
    history: Vec<Value>,
    events: Vec<Value>,
    revision: i64,
}
impl FieldJournalFlowBasis {
    pub fn version(&self) -> Result<i64, Error> {
        match &self.route {
            Some(r) => r["version"]
                .as_i64()
                .filter(|n| *n >= 1 && *n <= 9_007_199_254_740_991)
                .ok_or_else(changed),
            None => Ok(0),
        }
    }
    pub fn revision(&self) -> i64 {
        self.revision
    }
    pub fn history(&self) -> &[Value] {
        &self.history
    }
    pub fn selected(&self, key: &str) -> Result<Option<String>, Error> {
        if let Some(route) = &self.route {
            let steps: Value =
                serde_json::from_str(route["steps_json"].as_str().ok_or_else(changed)?)
                    .map_err(|_| changed())?;
            if steps.is_null() {
                return Err(changed());
            }
            if let Some(primary) = steps
                .get(key)
                .and_then(|s| s.get("primaryUserId"))
                .filter(|p| !p.is_null())
            {
                // A malformed selected primary must remain ineligible, never fall back.
                return Ok(Some(
                    primary
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| primary.to_string()),
                ));
            }
        }
        Ok(self
            .defaults
            .iter()
            .find(|v| v["step_key"] == key)
            .and_then(|v| v["primary_user_id"].as_str())
            .map(str::to_owned))
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if read_field_journal_flow_basis(c, &self.company, &self.id).await? != *self {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_field_journal_flow_basis(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<FieldJournalFlowBasis, Error> {
    let unit = unit(c).await?;
    let route:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(r) FROM flow_routes r WHERE company_id=$1 AND kind='journal' AND record_id=$2 FOR SHARE").bind(company.as_str()).bind(id).fetch_optional(&mut *c).await?;
    let latest: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(revision),0) FROM flow_defaults WHERE company_id=$1",
    )
    .bind(company.as_str())
    .fetch_one(&mut *c)
    .await?;
    let revision = route
        .as_ref()
        .map(|r| r["defaults_revision"].as_i64().ok_or_else(changed))
        .transpose()?
        .unwrap_or(latest);
    if !(0..=9_007_199_254_740_991).contains(&revision) {
        return Err(changed());
    }
    let defaults:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(d) FROM flow_defaults d WHERE company_id=$1 AND kind='journal' AND branch_id IS NULL AND department_id IS NULL AND revision<=$2 ORDER BY revision DESC LIMIT 1001 FOR SHARE").bind(company.as_str()).bind(revision).fetch_all(&mut *c).await?;
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(e) FROM flow_events e WHERE company_id=$1 AND kind='journal' AND record_id=$2 ORDER BY created_at,id LIMIT 10001 FOR SHARE").bind(company.as_str()).bind(id).fetch_all(&mut *c).await?;
    if defaults.len() > 1000
        || rows.len() > 10000
        || route.as_ref().is_some_and(|r| r.to_string().len() > 65536)
    {
        return Err(changed());
    }
    let mut history = Vec::new();
    for row in &rows {
        let at = row["created_at"]
            .as_i64()
            .filter(|v| (0..=9_007_199_254_740_991).contains(v))
            .ok_or_else(changed)?;
        history.push(json!({"action":row["action"],"actorName":row["actor_name"],"at":at,"reason":row["reason"],"step":row["step_key"]}));
    }
    Ok(FieldJournalFlowBasis {
        unit,
        company: company.clone(),
        id: id.into(),
        route,
        defaults,
        history,
        events: rows,
        revision,
    })
}

/// Workflow-owned exact assignment effects. Foreign permissions, participant
/// eligibility and Field journal lineage must stay retained by the composition.
pub struct FieldJournalFlowAssignment {
    before: FieldJournalFlowBasis,
    route: Value,
    event: Value,
    default: Option<Value>,
    after: Option<FieldJournalFlowBasis>,
    inserted_default: Option<Value>,
    pub recipient: String,
    pub message: String,
    pub result: Value,
}
pub struct FieldJournalFlowAssignmentInput<'a> {
    pub step: &'a str,
    pub user: &'a str,
    pub expected_flow: i64,
    pub expected_record: i64,
    pub reason: &'a str,
    pub save_default: bool,
    pub actor: &'a str,
    pub actor_name: &'a str,
    pub command: &'a str,
    pub event: &'a str,
    pub now: i64,
}
impl FieldJournalFlowBasis {
    pub fn plan_assignment(
        self,
        view: &Value,
        input: FieldJournalFlowAssignmentInput<'_>,
    ) -> Result<FieldJournalFlowAssignment, Error> {
        let error = |kind, code, message| Error::from(BusinessError::new(kind, code, message));
        if view["kind"] != "journal" || view["recordId"] != self.id || view["canManage"] != true {
            return Err(error(
                FailureKind::Forbidden,
                "FLOW_ADMIN_REQUIRED",
                "A company administrator must assign workflow responsibilities.",
            ));
        }
        if self.version()? != input.expected_flow
            || view["recordVersion"].as_i64() != Some(input.expected_record)
        {
            return Err(error(
                FailureKind::Conflict,
                "VERSION_CONFLICT",
                "The workflow changed. Refresh before assigning.",
            ));
        }
        let steps = view["steps"]
            .as_array()
            .filter(|s| s.len() == 1)
            .ok_or_else(changed)?;
        let current = steps.iter().position(|s| s["status"] == "current");
        let selected = steps.iter().position(|s| s["key"] == input.step);
        if current.is_none() || selected.is_none() || selected < current {
            return Err(error(
                FailureKind::Conflict,
                "FLOW_STEP_LOCKED",
                "Historical steps cannot be reassigned.",
            ));
        }
        let step = &steps[selected.ok_or_else(changed)?];
        let person = step["candidates"]
            .as_array()
            .and_then(|p| p.iter().find(|p| p["userId"] == input.user))
            .ok_or_else(|| {
                error(
                    FailureKind::InvalidInput,
                    "FLOW_STAFF_INELIGIBLE",
                    "Choose an active eligible staff member for this step.",
                )
            })?;
        let mut saved = match &self.route {
            Some(r) => serde_json::from_str::<Value>(r["steps_json"].as_str().ok_or_else(changed)?)
                .map_err(|_| changed())?,
            None => {
                json!({"draft":{"primaryUserId":steps[0]["primary"]["userId"],"teamName":steps[0]["teamName"],"defaultRevision":self.revision}})
            }
        };
        // Assignment repairs a malformed route only through an explicit error; it
        // must never silently discard its other saved responsibility.
        let map = saved.as_object_mut().ok_or_else(changed)?;
        let entry = map
            .entry(input.step.to_owned())
            .or_insert_with(|| json!({}));
        let entry = entry.as_object_mut().ok_or_else(changed)?;
        entry.insert("primaryUserId".into(), json!(input.user));
        entry.insert("teamName".into(), step["teamName"].clone());
        let next = self
            .version()?
            .max(1)
            .checked_add(1)
            .filter(|n| *n <= 9_007_199_254_740_991)
            .ok_or_else(changed)?;
        let route = json!({"company_id":self.company.as_str(),"kind":"journal","record_id":self.id,"version":next,"defaults_revision":self.revision,"steps_json":saved.to_string(),"created_at":self.route.as_ref().map(|r|r["created_at"].clone()).unwrap_or_else(||json!(input.now))});
        let event = json!({"id":input.event,"command_id":input.command,"company_id":self.company.as_str(),"kind":"journal","record_id":self.id,"step_key":input.step,"action":"assign","actor_user_id":input.actor,"actor_name":input.actor_name,"reason":format!("{} → {}",input.reason,person["name"].as_str().ok_or_else(changed)?),"record_version":input.expected_record,"created_at":input.now,"next_step_key":null,"next_user_id":null});
        let default=input.save_default.then(||json!({"company_id":self.company.as_str(),"kind":"journal","step_key":input.step,"branch_id":null,"department_id":null,"primary_user_id":input.user,"created_by":input.actor,"reason":input.reason,"created_at":input.now}));
        let result = json!({"id":self.id,"version":next});
        Ok(FieldJournalFlowAssignment {
            before: self,
            route,
            event,
            default,
            after: None,
            inserted_default: None,
            recipient: input.user.into(),
            message: format!(
                "{}: assigned to you",
                step["action"].as_str().ok_or_else(changed)?
            ),
            result,
        })
    }
}
impl FieldJournalFlowAssignment {
    pub async fn apply(&mut self, c: &mut PgConnection) -> Result<(), Error> {
        if self.after.is_some() {
            return Err(changed());
        }
        self.before.require_current(c).await?;
        let marker = json!({"database":self.before.unit.0,"pid":self.before.unit.1.to_string(),"xid":self.before.unit.2,"started":self.before.unit.3,"route":self.route,"event":self.event});
        sqlx::query("SELECT set_config('erpx.field_journal_flow',$1,true)")
            .bind(marker.to_string())
            .execute(&mut *c)
            .await?;
        let r = &self.route;
        let affected=sqlx::query("INSERT INTO flow_routes(company_id,kind,record_id,version,defaults_revision,steps_json,created_at)VALUES($1,'journal',$2,$3,$4,$5,$6) ON CONFLICT(company_id,kind,record_id) DO UPDATE SET version=EXCLUDED.version,steps_json=EXCLUDED.steps_json WHERE flow_routes.version=$7").bind(&self.before.company.as_str()).bind(&self.before.id).bind(r["version"].as_i64().ok_or_else(changed)?).bind(self.before.revision).bind(r["steps_json"].as_str().ok_or_else(changed)?).bind(r["created_at"].as_i64().ok_or_else(changed)?).bind(self.before.version()?).execute(&mut *c).await?.rows_affected();
        if affected != 1 {
            return Err(changed());
        }
        if let Some(d) = &self.default {
            self.inserted_default=Some(sqlx::query_scalar("INSERT INTO flow_defaults(company_id,kind,step_key,branch_id,department_id,primary_user_id,created_by,reason,created_at)VALUES($1,'journal',$2,NULL,NULL,$3,$4,$5,$6)RETURNING to_jsonb(flow_defaults)").bind(self.before.company.as_str()).bind(d["step_key"].as_str().ok_or_else(changed)?).bind(d["primary_user_id"].as_str().ok_or_else(changed)?).bind(d["created_by"].as_str().ok_or_else(changed)?).bind(d["reason"].as_str().ok_or_else(changed)?).bind(d["created_at"].as_i64().ok_or_else(changed)?).fetch_one(&mut *c).await?);
        }
        let e = &self.event;
        sqlx::query("INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at)VALUES($1,$2,$3,'journal',$4,$5,$11,$6,$7,$8,$9,$10)").bind(e["id"].as_str().ok_or_else(changed)?).bind(e["command_id"].as_str().ok_or_else(changed)?).bind(self.before.company.as_str()).bind(&self.before.id).bind(e["step_key"].as_str().ok_or_else(changed)?).bind(e["actor_user_id"].as_str().ok_or_else(changed)?).bind(e["actor_name"].as_str().ok_or_else(changed)?).bind(e["reason"].as_str()).bind(e["record_version"].as_i64().ok_or_else(changed)?).bind(e["created_at"].as_i64().ok_or_else(changed)?).bind(e["action"].as_str().ok_or_else(changed)?).execute(&mut *c).await?;
        let after =
            read_field_journal_flow_basis(c, &self.before.company, &self.before.id).await?;
        let mut events = self.before.events.clone();
        events.push(e.clone());
        events.sort_by(|a, b| {
            a["created_at"]
                .as_i64()
                .cmp(&b["created_at"].as_i64())
                .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
        });
        if after.route.as_ref() != Some(r)
            || after.events != events
            || after.defaults != self.before.defaults
            || after.revision != self.before.revision
            || after.unit != self.before.unit
        {
            return Err(changed());
        }
        self.after = Some(after);
        self.require_after(c).await
    }
    pub async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.after
            .as_ref()
            .ok_or_else(changed)?
            .require_current(c)
            .await?;
        let event: Option<Value> =
            sqlx::query_scalar("SELECT to_jsonb(e) FROM flow_events e WHERE id=$1 FOR SHARE")
                .bind(self.event["id"].as_str().ok_or_else(changed)?)
                .fetch_optional(&mut *c)
                .await?;
        if event.as_ref() != Some(&self.event) {
            return Err(changed());
        }
        if let (Some(planned), Some(inserted)) = (&self.default, &self.inserted_default) {
            let row: Option<Value> = sqlx::query_scalar(
                "SELECT to_jsonb(d) FROM flow_defaults d WHERE revision=$1 FOR SHARE",
            )
            .bind(inserted["revision"].as_i64().ok_or_else(changed)?)
            .fetch_optional(c)
            .await?;
            let mut expected = planned.clone();
            expected["revision"] = inserted["revision"].clone();
            if row.as_ref() != Some(&expected) {
                return Err(changed());
            }
        } else if self.default.is_some() {
            return Err(changed());
        }
        Ok(())
    }
}
