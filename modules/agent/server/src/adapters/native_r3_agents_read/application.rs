//! Query port and pure payload rendering for the native Agents read. The port
//! executes no SQL; the PostgreSQL adapter owns the statements and binds.

use super::domain::{AgentsFacts, EVIDENCE_KEYS, WORKSPACE_KEYS, effective, invalid};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;

pub type AgentsReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The committed scope supplied by the caller. Authority and session resolution
/// stay in composition; the owner read only binds these facts to SQL.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentsReadRequest<'a> {
    pub company: &'a str,
    pub user: &'a str,
    pub now: i64,
    pub manage: bool,
    pub evidence_view: bool,
}

/// One read unit returning the exact wire payload (never a plan or a fallback).
pub trait AgentsRead: Send + Sync {
    fn read<'a>(&'a self, request: AgentsReadRequest<'a>) -> AgentsReadFuture<'a, Value>;
}

fn object_keys(value: &Value) -> Option<Vec<&str>> {
    value.as_object().map(|map| map.keys().map(String::as_str).collect())
}

/// Renders the exact `agentWorkspace` shape, or the smaller evidence view.
/// All nine facts are required; a missing or renamed field fails closed.
pub fn render_agents_payload(
    facts: AgentsFacts,
    now: i64,
    evidence_view: bool,
) -> Result<Value, BusinessError> {
    let AgentsFacts {
        agents,
        versions,
        pools,
        assignments,
        invocations,
        people,
        members,
        pool_history,
        mut sessions,
    } = facts;
    for session in &mut sessions {
        effective(session, now);
    }
    let value = if evidence_view {
        json!({ "now": now, "invocations": invocations, "sessions": sessions })
    } else {
        json!({
            "agents": agents,
            "versions": versions,
            "pools": pools,
            "assignments": assignments,
            "invocations": invocations,
            "people": people,
            "members": members,
            "poolHistory": pool_history,
            "sessions": sessions,
        })
    };
    let expected: &[&str] = if evidence_view { &EVIDENCE_KEYS } else { &WORKSPACE_KEYS };
    let actual = object_keys(&value).ok_or_else(invalid)?;
    if actual.len() != expected.len() || expected.iter().any(|key| !actual.contains(key)) {
        return Err(invalid());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_shape_matches_the_retained_reader() {
        let value = render_agents_payload(AgentsFacts::default(), 1_800_000_000_000, false)
            .expect("workspace");
        let map = value.as_object().expect("object");
        assert_eq!(map.len(), 9);
        assert!(WORKSPACE_KEYS.iter().all(|key| map.contains_key(*key)));
        assert_eq!(value["poolHistory"], json!([]));
    }

    #[test]
    fn evidence_view_returns_only_now_invocations_sessions() {
        let mut facts = AgentsFacts::default();
        facts.invocations.push(json!({"id":"i"}));
        facts.sessions.push(json!({"id":"s","status":"active","expires_at":1}));
        let value = render_agents_payload(facts, 2, true).expect("evidence");
        assert_eq!(value["now"], json!(2));
        assert_eq!(value["invocations"], json!([{"id":"i"}]));
        assert_eq!(value["sessions"][0]["effective_status"], json!("expired"));
        assert_eq!(value.as_object().map(|map| map.len()), Some(3));
    }
}
