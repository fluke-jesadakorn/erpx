//! Parse frozen synthetic Node-oracle actors only. These explicit test facts
//! are never a production IAM projection or permission-bearing authority.
use erpx_server_shared::public::{AssignmentId, CompanyId, UserId};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandField, CommandJson, WorkAuthority,
};
use serde_json::Value;

fn string_field(value: &Value, name: &str) -> CommandField<String> {
    match value.get(name) {
        None => CommandField::Omitted,
        Some(Value::Null) => CommandField::Null,
        Some(value) => CommandField::Value(value.as_str().expect("frozen string field").into()),
    }
}
pub fn parse_frozen_actor(raw: &str) -> CommandActor {
    let ordered = CommandJson::parse(raw).expect("frozen valid command actor JSON");
    let value: Value = serde_json::from_str(raw).expect("frozen actor data");
    let authority = match value.get("authority") {
        None => CommandField::Omitted,
        Some(Value::Null) => CommandField::Null,
        Some(value) => CommandField::Value(WorkAuthority {
            session_id: value["sessionId"].as_str().unwrap().into(),
            revision: value["revision"].as_i64().unwrap(),
            assignment_version: value["assignmentVersion"].as_i64().unwrap(),
        }),
    };
    let iam = match value.get("iam") {
        None => CommandField::Omitted,
        Some(Value::Null) => CommandField::Null,
        Some(_) => CommandField::Value(ordered.property("iam").unwrap().unwrap()),
    };
    CommandActor {
        user_id: UserId::new(value["userId"].as_str().unwrap()).unwrap(),
        assignment_id: AssignmentId::new(value["assignmentId"].as_str().unwrap()).unwrap(),
        role: value["role"].as_str().unwrap().into(),
        company_id: CompanyId::new(value["companyId"].as_str().unwrap()).unwrap(),
        branch_id: string_field(&value, "branchId"),
        department_id: string_field(&value, "departmentId"),
        authority,
        iam,
        // The frozen oracle's synthetic assignment is explicitly Holding/null.
        // Real service composition must use the retained original assignment.
        assignment_scope_kind: "holding".into(),
        assignment_scope_node_id: None,
    }
}
