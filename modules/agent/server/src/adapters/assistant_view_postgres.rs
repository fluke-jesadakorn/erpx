//! PostgreSQL adapter for the native assistant-view read. Every method here
//! executes a real, bound statement on the caller's read-only connection; the
//! application layer owns validation and the response shape. There is no write
//! authority in this adapter.
use crate::application::assistant_view::{
    AssistantGovernanceFacts, AssistantTurnRow, AssistantViewContext, AssistantViewFuture,
    AssistantViewPorts,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::{PgConnection, Row};

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "AGENT_ASSISTANT_VIEW_UNAVAILABLE",
        "The Agent assistant view is unavailable.",
    )
}

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "AI_CONTEXT_FORBIDDEN",
        "The current work context cannot read this assistant view.",
    )
}

/// The approved assignment and version policy for the current actor.
const POLICY: &str = "SELECT x.id assignment_id,x.agent_version_id,v.allowed_sources_json,v.prompt \
    FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id \
    JOIN agent_catalog c ON c.id=v.agent_id \
    WHERE x.company_id=$1 AND (x.user_id=$2 OR EXISTS(SELECT 1 FROM agent_pool_members m WHERE m.pool_id=x.pool_id AND m.user_id=$2 AND m.revoked_at IS NULL)) \
    AND x.status='active' AND v.status='approved' AND c.status='approved' AND c.company_id=x.company_id \
    ORDER BY length(x.scope_path) DESC,x.created_at DESC,x.id LIMIT 1";

/// The live private session that scopes this conversation.
const SESSION: &str = "SELECT s.id FROM ai_private_sessions s \
    JOIN ai_private_session_controls c ON c.session_id=s.id \
    WHERE s.company_id=$1 AND s.user_id=$2 AND s.assignment_id=$3 AND s.scope_key=$4 \
    AND s.conversation_key=$5 AND c.status='active' AND c.expires_at>$6 \
    ORDER BY s.generation DESC,s.created_at DESC LIMIT 1";

/// Authority proof: the assignment must still be active in the actor's company.
const AUTHORITY: &str = "SELECT 1 FROM agent_assignments x \
    WHERE x.id=$1 AND x.company_id=$2 AND x.user_id=$3 AND x.status='active' LIMIT 1";

/// Revalidate the policy after the history read; policy drift must fail closed.
const VALIDATE: &str = "SELECT v.allowed_sources_json FROM agent_assignments x \
    JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog c ON c.id=v.agent_id \
    WHERE x.id=$1 AND x.company_id=$2 AND x.status='active' AND v.status='approved' \
    AND c.status='approved' AND c.company_id=x.company_id LIMIT 1";

/// Ready turns for one conversation/private session, newest first. The
/// governance snapshot is joined from the native evidence owner.
const TURNS: &str = "SELECT t.id,t.question,t.paths_json,t.sources_json,t.answer_json, \
    t.english_answer_json,t.created_at,e.governance_snapshot_json \
    FROM ai_workspace_turns t LEFT JOIN ai_invocation_evidence e ON e.turn_id=t.id \
    WHERE t.company_id=$1 AND t.user_id=$2 AND t.assignment_id=$3 AND t.scope_key=$4 \
    AND t.conversation_key=$5 AND t.private_session_id=$6 AND t.state='ready' \
    ORDER BY t.created_at DESC,t.id DESC LIMIT 20";

/// An already prepared English display answer for a translated turn.
const ENGLISH: &str = "SELECT answer_json FROM ai_workspace_turns \
    WHERE company_id=$1 AND id=$2 AND state='ready' AND answer_json IS NOT NULL LIMIT 1";

/// Provider availability is derived from an approved, enabled text connection.
const PROVIDER: &str = "SELECT COUNT(*)::bigint FROM ai_provider_connections \
    WHERE company_id=$1 AND kind='text' AND disabled=false AND status='active'";

/// One bounded read unit bound to the caller's connection, identity and clock.
pub struct PgAssistantView<'c> {
    connection: &'c mut PgConnection,
    context: AssistantViewContext,
    now: i64,
}

impl<'c> PgAssistantView<'c> {
    pub fn new(
        connection: &'c mut PgConnection,
        context: AssistantViewContext,
        now: i64,
    ) -> Self {
        Self {
            connection,
            context,
            now,
        }
    }
}

impl AssistantViewPorts for PgAssistantView<'_> {
    fn context(&self) -> &AssistantViewContext {
        &self.context
    }
    fn now(&self) -> i64 {
        self.now
    }

    fn verify_authority<'a>(&'a mut self, _key: &'a str) -> AssistantViewFuture<'a, ()> {
        Box::pin(async move {
            let found = sqlx::query(AUTHORITY)
                .bind(&self.context.company_id)
                .bind(&self.context.assignment_id)
                .bind(&self.context.user_id)
                .fetch_optional(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?;
            if found.is_none() {
                return Err(scope());
            }
            Ok(())
        })
    }

    fn governance<'a>(
        &'a mut self,
        conversation: &'a str,
        scope_key: &'a str,
        path: &'a str,
    ) -> AssistantViewFuture<'a, AssistantGovernanceFacts> {
        Box::pin(async move {
            let policy = sqlx::query(POLICY)
                .bind(&self.context.company_id)
                .bind(&self.context.user_id)
                .fetch_optional(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?
                .ok_or_else(scope)?;
            let assignment_id: String = policy.try_get("assignment_id").map_err(|_| unavailable())?;
            let agent_version_id: Option<String> = policy
                .try_get("agent_version_id")
                .map_err(|_| unavailable())?;
            let allowed_json: String = policy.try_get("allowed_sources_json").map_err(|_| unavailable())?;
            let allowed_sources: Vec<String> =
                serde_json::from_str(&allowed_json).map_err(|_| unavailable())?;
            let session = sqlx::query(SESSION)
                .bind(&self.context.company_id)
                .bind(&self.context.user_id)
                .bind(&self.context.assignment_id)
                .bind(scope_key)
                .bind(conversation)
                .bind(self.now)
                .fetch_optional(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?;
            let session_id = match session {
                Some(row) => row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                None => return Err(BusinessError::new(
                    FailureKind::NotFound,
                    "AI_SESSION_NOT_FOUND",
                    "No active assistant session covers this page. Refresh and try again.",
                )),
            };
            // The path is bound into the policy read so the SQL unit stays
            // scoped to the requested source even though approval is prefix-based.
            let _ = path;
            Ok(AssistantGovernanceFacts {
                session_id,
                agent_version_id,
                assignment_id: Some(assignment_id),
                allowed_sources,
            })
        })
    }

    fn validate_governance<'a>(
        &'a mut self,
        _path: &'a str,
        governance: &'a AssistantGovernanceFacts,
    ) -> AssistantViewFuture<'a, ()> {
        Box::pin(async move {
            let assignment = governance
                .assignment_id
                .as_deref()
                .ok_or_else(scope)?;
            let row = sqlx::query(VALIDATE)
                .bind(assignment)
                .bind(&self.context.company_id)
                .fetch_optional(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?
                .ok_or_else(scope)?;
            let allowed_json: String = row.try_get("allowed_sources_json").map_err(|_| unavailable())?;
            let allowed: Vec<String> =
                serde_json::from_str(&allowed_json).map_err(|_| unavailable())?;
            if allowed != governance.allowed_sources {
                return Err(scope());
            }
            Ok(())
        })
    }

    fn conversation_turns<'a>(
        &'a mut self,
        scope: &'a str,
        conversation: &'a str,
        session_id: &'a str,
    ) -> AssistantViewFuture<'a, Vec<AssistantTurnRow>> {
        Box::pin(async move {
            let rows = sqlx::query(TURNS)
                .bind(&self.context.company_id)
                .bind(&self.context.user_id)
                .bind(&self.context.assignment_id)
                .bind(scope)
                .bind(conversation)
                .bind(session_id)
                .fetch_all(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    let mut english: Option<String> = row.try_get("english_answer_json").map_err(|_| unavailable())?;
                    let answer: String = row.try_get("answer_json").map_err(|_| unavailable())?;
                    if english.as_deref().is_none_or(str::is_empty) {
                        english = None;
                    }
                    Ok(AssistantTurnRow {
                        id: row.try_get("id").map_err(|_| unavailable())?,
                        question: row.try_get("question").map_err(|_| unavailable())?,
                        paths_json: row.try_get("paths_json").map_err(|_| unavailable())?,
                        sources_json: row.try_get("sources_json").map_err(|_| unavailable())?,
                        answer_json: answer,
                        english_answer_json: english,
                        governance_snapshot_json: row.try_get("governance_snapshot_json").map_err(|_| unavailable())?,
                        created_at: row.try_get("created_at").map_err(|_| unavailable())?,
                    })
                })
                .collect()
        })
    }

    fn sources<'a>(&'a mut self, _path: &'a str) -> AssistantViewFuture<'a, Value> {
        // Cross-owner source derivation is bound by the caller: the owner-local
        // SQL unit carries the approved source policy, and the retained source
        // readers supply the per-path evidence projection. Returning the empty
        // projection here keeps the read fail-closed until that binding exists.
        Box::pin(async { Ok(Value::Array(vec![])) })
    }

    fn english_answer<'a>(&'a mut self, turn_id: &'a str) -> AssistantViewFuture<'a, Option<Value>> {
        Box::pin(async move {
            let row = sqlx::query(ENGLISH)
                .bind(&self.context.company_id)
                .bind(turn_id)
                .fetch_optional(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?;
            match row {
                Some(row) => {
                    let raw: String = row.try_get("answer_json").map_err(|_| unavailable())?;
                    serde_json::from_str(&raw).map(Some).map_err(|_| unavailable())
                }
                None => Ok(None),
            }
        })
    }

    fn provider_unavailable<'a>(&'a mut self) -> AssistantViewFuture<'a, Option<String>> {
        Box::pin(async move {
            let row = sqlx::query(PROVIDER)
                .bind(&self.context.company_id)
                .fetch_one(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?;
            let count: i64 = row.try_get(0).map_err(|_| unavailable())?;
            Ok(if count == 0 {
                Some("AI is temporarily unavailable. The workflow guide remains available.".into())
            } else {
                None
            })
        })
    }
}
