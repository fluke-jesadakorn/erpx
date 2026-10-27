use crate::domain::command_json::{CommandJson, OrderedJson};
use erpx_server_shared::public::{AssignmentId, BusinessError, CompanyId, UserId};
use sha2::{Digest, Sha256};
/// Presence is retained independently of JSON null, matching saved JS actors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandField<T> {
    Omitted,
    Null,
    Value(T),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkAuthority {
    pub session_id: String,
    pub revision: i64,
    pub assignment_version: i64,
}
/// Current adapter data. This type is not request-deserializable and grants no
/// work; the required owner/Identity policy validates it in the protected unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandActor {
    pub user_id: UserId,
    pub assignment_id: AssignmentId,
    pub role: String,
    pub company_id: CompanyId,
    pub branch_id: CommandField<String>,
    pub department_id: CommandField<String>,
    pub authority: CommandField<WorkAuthority>,
    pub iam: CommandField<CommandJson>,
    pub assignment_scope_kind: String,
    pub assignment_scope_node_id: Option<String>,
}
impl CommandActor {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.role.is_empty()
            || self.role.len() > 128
            || self.role.chars().any(char::is_control)
            || !matches!(
                self.assignment_scope_kind.as_str(),
                "platform" | "holding" | "branch" | "department"
            )
        {
            return Err(invalid_actor());
        }
        if let CommandField::Value(a) = &self.authority
            && (a.session_id.is_empty()
                || a.revision < 1
                || a.assignment_version < 1
                || a.revision > 9_007_199_254_740_991
                || a.assignment_version > 9_007_199_254_740_991)
        {
            return Err(invalid_actor());
        }
        Ok(())
    }
    fn authority_fields(&self) -> Result<Vec<(String, OrderedJson)>, BusinessError> {
        let mut values = vec![
            ("role".into(), OrderedJson::text(self.role.clone())),
            (
                "assignmentId".into(),
                OrderedJson::text(self.assignment_id.as_str()),
            ),
        ];
        add_json(&mut values, "iam", &self.iam)?;
        if let CommandField::Value(a) = &self.authority {
            values.extend(authority_value(a));
        }
        Ok(values)
    }
    pub fn receipt_authority(&self) -> Result<CommandJson, BusinessError> {
        self.validate()?;
        let mut values = self.authority_fields()?;
        values.push((
            "companyId".into(),
            OrderedJson::text(self.company_id.as_str()),
        ));
        add_string(&mut values, "branchId", &self.branch_id);
        add_string(&mut values, "departmentId", &self.department_id);
        values.push((
            "scopeKind".into(),
            OrderedJson::text(self.assignment_scope_kind.clone()),
        ));
        values.push((
            "scopeNodeId".into(),
            self.assignment_scope_node_id
                .as_ref()
                .map(|s| OrderedJson::text(s.clone()))
                .unwrap_or(OrderedJson::Null),
        ));
        CommandJson::from_value(OrderedJson::object(values))
    }
    pub fn outbox_payload(
        &self,
        record: &str,
        version: i64,
        state: Option<&str>,
        scope: &CommandEventScope,
    ) -> Result<CommandJson, BusinessError> {
        let mut values = vec![
            (
                "authority".into(),
                OrderedJson::object(self.authority_fields()?),
            ),
            ("id".into(), OrderedJson::text(record)),
        ];
        if let Some(state) = state {
            values.push(("state".into(), OrderedJson::text(state)));
        }
        values.push((
            "companyId".into(),
            OrderedJson::text(scope.company_id.as_str()),
        ));
        add_string(&mut values, "branchId", &scope.branch_id);
        add_string(&mut values, "departmentId", &scope.department_id);
        values.push(("version".into(), OrderedJson::Number(version as f64)));
        CommandJson::from_value(OrderedJson::object(values))
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandEventScope {
    pub company_id: CompanyId,
    pub branch_id: CommandField<String>,
    pub department_id: CommandField<String>,
}
impl From<&CommandActor> for CommandEventScope {
    fn from(a: &CommandActor) -> Self {
        Self {
            company_id: a.company_id.clone(),
            branch_id: a.branch_id.clone(),
            department_id: a.department_id.clone(),
        }
    }
}
fn authority_value(a: &WorkAuthority) -> Vec<(String, OrderedJson)> {
    vec![
        ("sessionId".into(), OrderedJson::text(a.session_id.clone())),
        ("revision".into(), OrderedJson::Number(a.revision as f64)),
        (
            "assignmentVersion".into(),
            OrderedJson::Number(a.assignment_version as f64),
        ),
    ]
}
fn add_string(values: &mut Vec<(String, OrderedJson)>, key: &str, value: &CommandField<String>) {
    match value {
        CommandField::Omitted => {}
        CommandField::Null => values.push((key.into(), OrderedJson::Null)),
        CommandField::Value(s) => values.push((key.into(), OrderedJson::text(s.clone()))),
    }
}
fn add_json(
    values: &mut Vec<(String, OrderedJson)>,
    key: &str,
    value: &CommandField<CommandJson>,
) -> Result<(), BusinessError> {
    match value {
        CommandField::Omitted => {}
        CommandField::Null => values.push((key.into(), OrderedJson::Null)),
        CommandField::Value(s) => values.push((key.into(), s.value()?)),
    }
    Ok(())
}
pub fn command_fingerprint(
    actor: &CommandActor,
    operation: &str,
    record_id: &str,
    input: &CommandJson,
) -> Result<String, BusinessError> {
    actor.validate()?;
    let mut values = vec![
        ("actor".into(), OrderedJson::text(actor.user_id.as_str())),
        (
            "assignment".into(),
            OrderedJson::text(actor.assignment_id.as_str()),
        ),
        ("role".into(), OrderedJson::text(actor.role.clone())),
    ];
    match &actor.authority {
        CommandField::Omitted => {}
        CommandField::Null => values.push(("authority".into(), OrderedJson::Null)),
        CommandField::Value(a) => {
            values.push(("authority".into(), OrderedJson::object(authority_value(a))))
        }
    }
    values.push((
        "company".into(),
        OrderedJson::text(actor.company_id.as_str()),
    ));
    add_string(&mut values, "branch", &actor.branch_id);
    add_string(&mut values, "department", &actor.department_id);
    values.push(("operation".into(), OrderedJson::text(operation)));
    values.push(("recordId".into(), OrderedJson::text(record_id)));
    values.push(("input".into(), input.value()?));
    Ok(hex::encode(Sha256::digest(
        OrderedJson::object(values).encode()?,
    )))
}
fn invalid_actor() -> BusinessError {
    BusinessError::invalid(
        "COMMAND_ACTOR_INVALID",
        "A current trusted command actor is required.",
    )
}

/// Reconstruct only the saved fingerprint fields for the explicit terminology
/// bridge; the ledger independently validates current authority before replay.
pub(crate) fn historical_actor(actor: &CommandActor, saved: &CommandJson) -> Option<CommandActor> {
    let value = saved.value().ok()?;
    if value.get("role")?.string()? != actor.role
        || value.get("assignmentId")?.string()? != actor.assignment_id.as_str()
        || value.get("companyId")?.string()? != actor.company_id.as_str()
    {
        return None;
    }
    fn same(value: Option<&OrderedJson>, field: &CommandField<String>) -> bool {
        match (field, value) {
            (CommandField::Omitted, None) | (CommandField::Null, Some(OrderedJson::Null)) => true,
            (CommandField::Value(a), Some(v)) => v.string() == Some(a.as_str()),
            _ => false,
        }
    }
    if !same(value.get("branchId"), &actor.branch_id)
        || !same(value.get("departmentId"), &actor.department_id)
    {
        return None;
    }
    let CommandField::Value(_) = actor.authority else {
        return None;
    };
    let number = |key: &str| -> Option<i64> {
        let OrderedJson::Number(v) = value.get(key)? else {
            return None;
        };
        if !v.is_finite() || v.fract() != 0.0 || *v < 1.0 || *v > 9_007_199_254_740_991.0 {
            return None;
        }
        Some(*v as i64)
    };
    let mut result = actor.clone();
    result.authority = CommandField::Value(WorkAuthority {
        session_id: value.get("sessionId")?.string()?.into(),
        revision: number("revision")?,
        assignment_version: number("assignmentVersion")?,
    });
    Some(result)
}
