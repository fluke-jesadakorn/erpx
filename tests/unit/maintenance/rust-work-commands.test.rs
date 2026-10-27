//! Actual B0 portable application oracle. Synthetic ports do not prove any
//! PostgreSQL authorization, purchase admission or stock/journal effect.
use erpx_server_maintenance::public::{
    MaintenanceActorFacts, MaintenanceState, MaintenanceWorkFacts, work_commands::*, work_wire::*,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, FailureKind, RecordId, TimestampMillis, UserId,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
type Trace = Arc<Mutex<Vec<Value>>>;
fn record(t: &Trace, v: Value) {
    t.lock().unwrap().push(v);
}
#[derive(Debug)]
enum ProbeError {
    Business(BusinessError),
    Wire(MaintenanceWireFailure),
}
impl From<BusinessError> for ProbeError {
    fn from(e: BusinessError) -> Self {
        Self::Business(e)
    }
}
impl From<MaintenanceWireFailure> for ProbeError {
    fn from(e: MaintenanceWireFailure) -> Self {
        Self::Wire(e)
    }
}
struct Access {
    trace: Trace,
    write: bool,
    review: bool,
}
impl MaintenanceChangeAccess for Access {
    type Error = ProbeError;
    fn authorize_write(&self) -> Result<(), ProbeError> {
        record(&self.trace, json!(["authorize", true]));
        if !self.write {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Synthetic current permission denied",
            )
            .into());
        }
        Ok(())
    }
    fn actor(&self) -> MaintenanceActorFacts {
        MaintenanceActorFacts {
            user_id: UserId::new("actor").unwrap(),
            can_review: self.review,
        }
    }
}
struct Repository {
    trace: Trace,
    row: Value,
    replay: bool,
}
struct PreparedConsumption {
    request: Value,
}
fn request(c: &MaintenanceWorkCommand) -> Value {
    json!({"key":c.key().as_str(),"operation":c.operation(),"id":c.work_id().as_str(),"input":serde_json::from_str::<Value>(c.receipt_input().as_str()).unwrap()})
}
fn facts(s: &Value) -> MaintenanceWorkFacts {
    MaintenanceWorkFacts {
        owner_id: UserId::new(s["owner_id"].as_str().unwrap()).unwrap(),
        status: s["status"].as_str().unwrap().into(),
        owner_eligible: s["owner_eligible"].as_i64().unwrap() > 0,
        reviewer_eligible: s["can_review"].as_i64().unwrap() > 0,
        scheduled_at: s["scheduled_at"].as_i64(),
        downtime_started_at: s["downtime_started_at"].as_i64(),
    }
}
impl MaintenanceChangeRepository for Repository {
    type Error = ProbeError;
    type Source = Value;
    type PreparedConsumption = PreparedConsumption;
    type Output = Value;
    async fn replay(&mut self, c: &MaintenanceWorkCommand) -> Result<Option<Value>, ProbeError> {
        record(&self.trace, json!(["replay", request(c)]));
        Ok(self
            .replay
            .then(|| json!({"id":"work","version":8,"status":"in_progress"})))
    }
    async fn work(&mut self, id: &RecordId) -> Result<Option<Value>, ProbeError> {
        record(&self.trace, json!(["work", id.as_str()]));
        Ok((!self.row.is_null()).then(|| self.row.clone()))
    }
    fn policy_facts(&self, s: &Value) -> MaintenanceWorkFacts {
        facts(s)
    }
    fn now(&self) -> TimestampMillis {
        record(&self.trace, json!(["clock"]));
        TimestampMillis::new(1800000000100)
    }
    async fn prepare_consumption(
        &mut self,
        c: &MaintenanceWorkCommand,
        s: &Value,
    ) -> Result<PreparedConsumption, ProbeError> {
        record(
            &self.trace,
            json!(["prepareConsumption", request(c), s["id"]]),
        );
        Ok(PreparedConsumption {
            request: request(c),
        })
    }
    async fn change(
        &mut self,
        c: &MaintenanceWorkCommand,
        s: Value,
        state: MaintenanceState,
        now: TimestampMillis,
        prepared: Option<PreparedConsumption>,
    ) -> Result<Value, ProbeError> {
        assert_eq!(prepared.unwrap().request, request(c));
        let view = json!({"status":state.status,"owner":state.owner_id.as_str(),"scheduled":state.scheduled_at,"downtimeStart":state.downtime_started_at,"downtimeEnd":state.downtime_ended_at});
        record(
            &self.trace,
            json!(["change", request(c), s["id"], view, now.get()]),
        );
        Ok(json!({"id":"work","version":c.input().expected_version+1,"status":state.status}))
    }
}
fn error(e: ProbeError) -> Value {
    match e {
        ProbeError::Business(e) => {
            let status = match e.kind {
                FailureKind::Forbidden => 403,
                FailureKind::NotFound => 404,
                FailureKind::Conflict => 409,
                _ => 422,
            };
            json!({"name":"Error","status":status,"code":e.code,"issues":null})
        }
        ProbeError::Wire(e) => {
            assert!(!e.invalid_json);
            json!({"name":"ZodError","status":null,"code":null,"issues":e.issues.iter().map(|i|json!({"code":i.code,"path":i.path.iter().map(|p|match p{MaintenanceIssuePath::Field(s)=>json!(s),MaintenanceIssuePath::Index(n)=>json!(n)}).collect::<Vec<_>>()})).collect::<Vec<_>>()})
        }
    }
}
#[tokio::test]
async fn original_permission_parse_replay_source_clock_preparation_and_final_version_order_are_retained()
 {
    let data: Value = serde_json::from_str(include_str!(
        "../../../fixtures/maintenance/rust-work/conformance.v1.json"
    ))
    .unwrap();
    let rows: Vec<_> = data["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| !r["id"].as_str().unwrap().starts_with("authority-"))
        .collect();
    assert_eq!(rows.len(), 9);
    for r in rows {
        let trace = Trace::default();
        let options = &r["options"];
        let access = Access {
            trace: trace.clone(),
            write: options["write"].as_bool().unwrap_or(true),
            review: options["review"].as_bool().unwrap_or(false),
        };
        let repository = Repository {
            trace: trace.clone(),
            row: r["source"].clone(),
            replay: options["replay"].as_bool().unwrap_or(false),
        };
        let body = serde_json::to_vec(&r["input"]).unwrap();
        let factory_trace = trace.clone();
        let result = change_maintenance_work(
            &access,
            || {
                let parsed = normalize_maintenance_change(&body)?;
                Ok(parsed.into_command(CommandKey::new("part-command")?, RecordId::new("work")?))
            },
            move || {
                record(&factory_trace, json!(["repository"]));
                Ok(repository)
            },
        )
        .await;
        let calls = trace.lock().unwrap().clone();
        let observed = match result {
            Ok(result) => json!({"result":result,"calls":calls}),
            Err(e) => json!({"error":error(e),"calls":calls}),
        };
        assert_eq!(observed, r["expected"], "{}", r["id"]);
    }
}
