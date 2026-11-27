//! Work navigation query and read-home command use cases.
//!
//! No SQL, HTTP or authority resolution lives here. The owner port executes
//! the retained SQL on one caller-owned Holding transaction; this layer only
//! validates command facts and composes the exact browser wire value.
use crate::adapters::work_navigation_read::NativeWorkNavigationRequest;
use crate::domain::navigation_wire::canonical_envelope;
use crate::domain::work_navigation_wire::{attention_items_json, navigation_snapshot_json};
use crate::adapters::work_capability_probes::InventoryPreparation;
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use serde_json::Value;
use std::future::Future;

/// Mirrors `workspaceBase(publicScope(actor))` for a Holding scope. All three
/// slugs are already validated by the composition gate.
pub fn navigation_workspace_base(
    holding_slug: &str,
    branch_slug: Option<&str>,
    department_slug: Option<&str>,
) -> String {
    let mut base = format!("/holdings/{holding_slug}");
    if let Some(branch) = branch_slug {
        base.push_str(&format!("/branches/{branch}"));
    }
    if let Some(department) = department_slug {
        base.push_str(&format!("/departments/{department}"));
    }
    base
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "NAVIGATION_COMMAND_INVALID",
        "Use one bounded Work navigation command.",
    )
}

/// The exact `/navigation?include=attention` read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NavigationQuery {
    pub include_attention: bool,
    pub now: TimestampMillis,
}

/// Owner facts the composition gate must supply; they are never invented here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NavigationFacts<'a> {
    pub workspace_base: &'a str,
    pub beta: bool,
    pub inventory: InventoryPreparation,
    pub expense_available: bool,
    pub routing_attention: bool,
}

/// The exact `POST /navigation` `{action: "read-home"}` command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkHomeReadCommand {
    pub idempotency_key: String,
    pub read_through: i64,
    pub workspace_base: String,
}

impl MarkHomeReadCommand {
    /// Validate the key used to admit one cursor update per workspace and user.
    pub fn validate(&self) -> Result<(), BusinessError> {
        let key = &self.idempotency_key;
        if !(8..=180).contains(&key.len())
            || !key.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.' | b'-')
            })
            || self.read_through <= 0
            || self.workspace_base.is_empty()
            || self.workspace_base.len() > 120
        {
            return Err(invalid());
        }
        Ok(())
    }
}

/// Owner SQL port. Both methods run on the same caller-held transaction.
pub trait WorkNavigationPort {
    type Error: From<BusinessError> + Send;

    fn unread_home<'a>(
        &'a mut self,
        current: &'a AuthorizedWorkContext,
        workspace_base: &'a str,
        now: i64,
    ) -> impl Future<Output = Result<i64, Self::Error>> + Send + 'a;

    fn snapshot<'a>(
        &'a mut self,
        current: &'a AuthorizedWorkContext,
        request: NativeWorkNavigationRequest,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<crate::domain::work_navigation::WorkNavigationFact, Self::Error>>
           + Send
           + 'a;

    fn mark_home_read<'a>(
        &'a mut self,
        current: &'a AuthorizedWorkContext,
        command: &'a MarkHomeReadCommand,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;
}

/// Composes the exact `{data: NavigationSnapshot}` response from native facts.
pub async fn read_navigation<P: WorkNavigationPort>(
    port: &mut P,
    query: NavigationQuery,
    current: &AuthorizedWorkContext,
    facts: NavigationFacts<'_>,
) -> Result<Value, P::Error> {
    let home_unread = port
        .unread_home(current, facts.workspace_base, query.now.get())
        .await?;
    let snapshot = port
        .snapshot(
            current,
            NativeWorkNavigationRequest {
                beta: facts.beta,
                inventory: facts.inventory,
                expense_available: facts.expense_available,
                home_unread,
                include_attention: query.include_attention,
                routing_attention: facts.routing_attention,
            },
            query.now,
        )
        .await?;
    let attention = attention_items_json(snapshot.attention.as_ref());
    Ok(canonical_envelope(navigation_snapshot_json(
        &snapshot,
        query.now.get(),
        attention,
    )))
}

/// Marks the Home cursor once and returns the same `{data: {home}}` envelope as
/// the retained route's read-after-write count.
pub async fn mark_home_read<P: WorkNavigationPort>(
    port: &mut P,
    command: &MarkHomeReadCommand,
    current: &AuthorizedWorkContext,
    now: TimestampMillis,
) -> Result<Value, P::Error> {
    command.validate().map_err(P::Error::from)?;
    port.mark_home_read(current, command).await?;
    let home = port
        .unread_home(current, &command.workspace_base, now.get())
        .await?;
    Ok(canonical_envelope(serde_json::json!({"home": home})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::work_card_availability::{WorkCapabilitiesFact, WorkItemFact, WorkQueue, WorkSnapshotFact};

    struct Fake {
        home: i64,
        marked: Vec<i64>,
    }

    impl WorkNavigationPort for Fake {
        type Error = BusinessError;
        async fn unread_home<'a>(
            &'a mut self,
            _: &'a AuthorizedWorkContext,
            _: &'a str,
            _: i64,
        ) -> Result<i64, Self::Error> {
            Ok(self.home)
        }
        async fn snapshot<'a>(
            &'a mut self,
            _: &'a AuthorizedWorkContext,
            request: NativeWorkNavigationRequest,
            _: TimestampMillis,
        ) -> Result<crate::domain::work_navigation::WorkNavigationFact, Self::Error> {
            let item = WorkItemFact {
                id: "task-1".into(),
                kind: "staff_task".into(),
                queue: WorkQueue::Todo,
                can_act: Some(true),
                action_kind: None,
                relationships: Vec::new(),
                updated_at: 1,
                source_id: None,
                href: "/support/ticket-1".into(),
            };
            let snapshot = WorkSnapshotFact { items: vec![item] };
            Ok(crate::domain::work_navigation::work_navigation(
                crate::domain::work_navigation::WorkNavigationFacts {
                    snapshot: &snapshot,
                    unavailable_sources: &[],
                    capabilities: &WorkCapabilitiesFact::default(),
                    home_unread: request.home_unread,
                    include_attention: request.include_attention,
                    routing_attention: false,
                },
            ))
        }
        async fn mark_home_read<'a>(
            &'a mut self,
            _: &'a AuthorizedWorkContext,
            command: &'a MarkHomeReadCommand,
        ) -> Result<(), Self::Error> {
            self.marked.push(command.read_through);
            Ok(())
        }
    }

    fn command() -> MarkHomeReadCommand {
        MarkHomeReadCommand {
            idempotency_key: "work:home:read:1".into(),
            read_through: 1_700_000_000_000,
            workspace_base: "/holdings/acme".into(),
        }
    }

    #[test]
    fn command_key_and_cursor_are_bounded() {
        assert!(command().validate().is_ok());
        let mut bad = command();
        bad.idempotency_key.clear();
        assert!(bad.validate().is_err());
        let mut bad = command();
        bad.read_through = 0;
        assert!(bad.validate().is_err());
    }

    #[test]
    fn workspace_base_matches_the_retained_scope_path() {
        assert_eq!(navigation_workspace_base("acme", None, None), "/holdings/acme");
        assert_eq!(
            navigation_workspace_base("acme", Some("bangkok"), Some("sales")),
            "/holdings/acme/branches/bangkok/departments/sales"
        );
    }
}
