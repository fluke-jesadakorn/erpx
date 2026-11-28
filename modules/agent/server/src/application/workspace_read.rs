//! Native Agent workspace and evidence-record reads.
//!
//! This is a read projection only: the current Actor is required before any
//! source read, the port owns the SQL, and every row is revalidated here before
//! it can be returned. No HTTP, transaction or write authority lives in this
//! layer; the lead-owned composition binds the port to the mounted route.
use crate::application::governance::AgentActor;
use erpx_platform_shared_server::public::BusinessError;
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;

pub type WorkspaceReadFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn not_found() -> BusinessError {
    BusinessError::new(
        erpx_platform_shared_server::public::FailureKind::NotFound,
        "AGENT_RECORD_NOT_FOUND",
        "Agent record not found in this work scope.",
    )
}

fn identifier(value: &str) -> Result<(), BusinessError> {
    if value.is_empty() || value.len() > 200 || value.chars().any(char::is_control) {
        return Err(invalid(
            "AGENT_WORKSPACE_READ_INVALID",
            "Use one bounded Agent workspace read.",
        ));
    }
    Ok(())
}

/// The complete set of owner facts needed to render the workspace and record
/// reads. Every method is mandatory: no permissive default is supplied.
pub trait AgentWorkspaceReadPorts: Send {
    type Error: From<BusinessError> + Send;

    fn actor(&self) -> &AgentActor;
    fn now(&self) -> i64;

    fn agents<'a>(&'a mut self, company: &'a str) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error>;
    fn versions<'a>(
        &'a mut self,
        company: &'a str,
    ) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error>;
    fn pools<'a>(&'a mut self, company: &'a str) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error>;
    fn assignments<'a>(
        &'a mut self,
        company: &'a str,
    ) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error>;
    fn invocations<'a>(
        &'a mut self,
        company: &'a str,
    ) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error>;
    fn people<'a>(
        &'a mut self,
        company: &'a str,
        now: i64,
    ) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error>;
    fn members<'a>(&'a mut self, company: &'a str) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error>;
    fn pool_history<'a>(
        &'a mut self,
        company: &'a str,
    ) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error>;
    fn sessions<'a>(
        &'a mut self,
        company: &'a str,
        user: &'a str,
        manage: bool,
    ) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error>;
    fn invocation<'a>(
        &'a mut self,
        company: &'a str,
        id: &'a str,
    ) -> WorkspaceReadFuture<'a, Option<Value>, Self::Error>;
    fn session<'a>(
        &'a mut self,
        company: &'a str,
        id: &'a str,
        user: &'a str,
        manage: bool,
    ) -> WorkspaceReadFuture<'a, Option<Value>, Self::Error>;
}

fn effective(row: &mut Value, now: i64) {
    let status = row.get("status").and_then(Value::as_str).unwrap_or("").to_owned();
    let expires = row
        .get("expires_at")
        .and_then(Value::as_i64)
        .unwrap_or(i64::MAX);
    let value = if status == "active" && expires <= now {
        "expired"
    } else {
        &status
    };
    if let Some(map) = row.as_object_mut() {
        map.insert("effective_status".into(), Value::String(value.into()));
    }
}

fn empty_workspace(sessions: Option<Vec<Value>>) -> Value {
    json!({
        "agents": [], "versions": [], "pools": [], "assignments": [],
        "invocations": [], "people": [], "members": [], "poolHistory": [],
        "sessions": sessions.unwrap_or_default(),
    })
}

/// Render the exact workspace payload the retained TS application produced.
pub async fn agent_workspace_read<P: AgentWorkspaceReadPorts>(
    actor: &AgentActor,
    include_evidence: bool,
    ports: &mut P,
) -> Result<Value, P::Error> {
    actor.require_agent_module()?;
    identifier(actor.company_id())?;
    identifier(actor.user_id())?;
    let now = ports.now();
    let company = actor.company_id();
    let agents = ports.agents(company).await?;
    let versions = ports.versions(company).await?;
    let pools = ports.pools(company).await?;
    let assignments = ports.assignments(company).await?;
    let people = ports.people(company, now).await?;
    let members = ports.members(company).await?;
    let pool_history = ports.pool_history(company).await?;
    let (invocations, sessions) = if include_evidence {
        let rows = ports.invocations(company).await?;
        let mut sessions = ports
            .sessions(company, actor.user_id(), actor.can_manage())
            .await?;
        for session in &mut sessions {
            effective(session, now);
        }
        (rows, sessions)
    } else {
        (vec![], vec![])
    };
    Ok(json!({
        "agents": agents,
        "versions": versions,
        "pools": pools,
        "assignments": assignments,
        "invocations": invocations,
        "people": people,
        "members": members,
        "poolHistory": pool_history,
        "sessions": sessions,
    }))
}

/// The one record kind accepted by the native evidence-record read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentRecordReadKind {
    Invocation,
    Session,
}

impl AgentRecordReadKind {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "agent_invocation" => Ok(Self::Invocation),
            "agent_session" => Ok(Self::Session),
            _ => Err(invalid(
                "AGENT_RECORD_KIND_INVALID",
                "Choose one bounded Agent evidence record kind.",
            )),
        }
    }
}

/// Read one invocation or private-session record, with the same privacy scope,
/// name fall-back and session payload the retained TS application produced.
pub async fn agent_evidence_record_read<P: AgentWorkspaceReadPorts>(
    actor: &AgentActor,
    kind: AgentRecordReadKind,
    id: &str,
    ports: &mut P,
) -> Result<Value, P::Error> {
    actor.require_agent_module()?;
    identifier(actor.company_id())?;
    identifier(actor.user_id())?;
    identifier(id)?;
    let now = ports.now();
    let company = actor.company_id();
    let mut row = match kind {
        AgentRecordReadKind::Invocation => ports
            .invocation(company, id)
            .await?
            .ok_or_else(not_found)?,
        AgentRecordReadKind::Session => ports
            .session(company, id, actor.user_id(), actor.can_manage())
            .await?
            .ok_or_else(not_found)?,
    };
    let name = row
        .get("turn_id")
        .and_then(Value::as_str)
        .or_else(|| row.get("conversation_key").and_then(Value::as_str))
        .unwrap_or(id)
        .to_owned();
    let session_data = if kind == AgentRecordReadKind::Session {
        effective(&mut row, now);
        let mut sessions = vec![row.clone()];
        for session in &mut sessions {
            effective(session, now);
        }
        Some(empty_workspace(Some(sessions)))
    } else {
        None
    };
    if let Some(map) = row.as_object_mut() {
        map.insert("name".into(), Value::String(name));
        if kind == AgentRecordReadKind::Session {
            effective(&mut row, now);
        }
    }
    Ok(json!({ "row": row, "sessionData": session_data }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::governance::AgentActor;
    use erpx_platform_shared_server::public::{AssignmentId, CompanyId, UserId};
    use erpx_workflow_server::public::commands::{
        CommandActor, CommandField, CommandJson, WorkAuthority,
    };
    use serde_json::json;

    struct Fake {
        actor: AgentActor,
        rows: Vec<Value>,
    }

    impl AgentWorkspaceReadPorts for Fake {
        type Error = BusinessError;
        fn actor(&self) -> &AgentActor {
            &self.actor
        }
        fn now(&self) -> i64 {
            1_800_000_000_000
        }
        fn agents<'a>(&'a mut self, _: &'a str) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error> {
            Box::pin(async { Ok(self.rows.clone()) })
        }
        fn versions<'a>(&'a mut self, _: &'a str) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error> {
            Box::pin(async { Ok(vec![]) })
        }
        fn pools<'a>(&'a mut self, _: &'a str) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error> {
            Box::pin(async { Ok(vec![]) })
        }
        fn assignments<'a>(&'a mut self, _: &'a str) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error> {
            Box::pin(async { Ok(vec![]) })
        }
        fn invocations<'a>(&'a mut self, _: &'a str) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error> {
            Box::pin(async { Ok(vec![]) })
        }
        fn people<'a>(&'a mut self, _: &'a str, _: i64) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error> {
            Box::pin(async { Ok(vec![]) })
        }
        fn members<'a>(&'a mut self, _: &'a str) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error> {
            Box::pin(async { Ok(vec![]) })
        }
        fn pool_history<'a>(&'a mut self, _: &'a str) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error> {
            Box::pin(async { Ok(vec![]) })
        }
        fn sessions<'a>(
            &'a mut self,
            _: &'a str,
            _: &'a str,
            _: bool,
        ) -> WorkspaceReadFuture<'a, Vec<Value>, Self::Error> {
            Box::pin(async {
                Ok(vec![json!({
                    "id": "s", "status": "active", "expires_at": 1_700_000_000_000_i64
                })])
            })
        }
        fn invocation<'a>(
            &'a mut self,
            _: &'a str,
            id: &'a str,
        ) -> WorkspaceReadFuture<'a, Option<Value>, Self::Error> {
            let id = id.to_owned();
            Box::pin(async move { Ok(Some(json!({ "id": id, "turn_id": "t" }))) })
        }
        fn session<'a>(
            &'a mut self,
            _: &'a str,
            id: &'a str,
            _: &'a str,
            _: bool,
        ) -> WorkspaceReadFuture<'a, Option<Value>, Self::Error> {
            let id = id.to_owned();
            Box::pin(async move {
                Ok(Some(json!({
                    "id": id, "conversation_key": "c", "status": "active", "expires_at": 1_700_000_000_000_i64
                })))
            })
        }
    }

    fn command_actor(company: &str, user: &str) -> CommandActor {
        CommandActor {
            user_id: UserId::new(user).unwrap(),
            assignment_id: AssignmentId::new("assignment").unwrap(),
            role: "holding_owner".into(),
            company_id: CompanyId::new(company).unwrap(),
            branch_id: CommandField::Omitted,
            department_id: CommandField::Omitted,
            authority: CommandField::Value(WorkAuthority {
                session_id: "session".into(),
                revision: 1,
                assignment_version: 1,
            }),
            iam: CommandField::Value(
                CommandJson::parse(
                    r#"{"roleRevision":1,"identityVersion":1,"permissions":[],"grantablePermissions":[],"principalId":"principal","personId":null,"personUserIds":[],"freshUntil":null}"#,
                )
                .unwrap(),
            ),
            assignment_scope_kind: "holding".into(),
            assignment_scope_node_id: None,
        }
    }

    fn actor() -> AgentActor {
        AgentActor::new(
            "holding",
            command_actor("company", "user"),
            vec!["agent".into()],
            vec![],
        )
    }

    #[tokio::test]
    async fn workspace_renders_the_retained_shape_and_expired_status() {
        let mut ports = Fake {
            actor: actor(),
            rows: vec![json!({ "id": "a" })],
        };
        let actor = ports.actor.clone();
        let value = agent_workspace_read(&actor, true, &mut ports)
            .await
            .expect("workspace");
        assert_eq!(value["agents"], json!([{ "id": "a" }]));
        assert_eq!(value["sessions"][0]["effective_status"], json!("expired"));
        assert_eq!(value.as_object().map(|map| map.len()), Some(9));
    }

    #[tokio::test]
    async fn record_read_renames_and_scopes_the_session_payload() {
        let mut ports = Fake {
            actor: actor(),
            rows: vec![],
        };
        let actor = ports.actor.clone();
        let value = agent_evidence_record_read(
            &actor,
            AgentRecordReadKind::Session,
            "session",
            &mut ports,
        )
        .await
        .expect("record");
        assert_eq!(value["row"]["name"], json!("c"));
        assert_eq!(value["row"]["effective_status"], json!("expired"));
        assert_eq!(value["sessionData"]["sessions"][0]["id"], json!("session"));
    }

    #[tokio::test]
    async fn invalid_kind_and_id_fail_closed() {
        assert!(AgentRecordReadKind::parse("other").is_err());
        let mut ports = Fake {
            actor: actor(),
            rows: vec![],
        };
        let actor = ports.actor.clone();
        assert!(
            agent_evidence_record_read(
                &actor,
                AgentRecordReadKind::Invocation,
                "bad\nid",
                &mut ports,
            )
            .await
            .is_err()
        );
    }
}
