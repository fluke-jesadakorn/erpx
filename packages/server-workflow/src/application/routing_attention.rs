//! Native Routing-attention read application.
//!
//! Exact port of `packages/server-workflow/src/application/routing-attention.ts`.
//! The application owns only the manager gate, cursor scan, skip filters and
//! page assembly. Owner rows, flow access, resolved flows and company authority
//! are supplied by [`RoutingAttentionPorts`]; composition binds them to the
//! selected Holding's read connection and existing owner policies. No SQL,
//! transaction, retry or provider call happens here.
use crate::domain::routing_attention::{
    FlowKind, ROUTING_PAGE_SIZE, RoutingAttentionFlow, RoutingAttentionItem,
    RoutingAttentionPage, RoutingAttentionRow, RoutingQuery,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{future::Future, pin::Pin};

pub type RoutingFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The retained `WorkContext` projection used by the manager gate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingAttentionActor {
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

impl RoutingAttentionActor {
    pub fn new(
        user_id: impl Into<String>,
        company_id: impl Into<String>,
        branch_id: Option<String>,
        department_id: Option<String>,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            company_id: company_id.into(),
            branch_id,
            department_id,
        }
    }
}

/// Owner ports for one routing-attention scan.
pub trait RoutingAttentionPorts: Send {
    /// One owner page ordered by ascending `id` after the cursor.
    fn rows<'a>(
        &'a mut self,
        company_id: &'a str,
        after: &'a str,
        query: &'a RoutingQuery,
    ) -> RoutingFuture<'a, Vec<RoutingAttentionRow>>;

    /// Live Company organization-management authority for this actor.
    fn company_authority<'a>(
        &'a mut self,
        actor: &'a RoutingAttentionActor,
    ) -> RoutingFuture<'a, bool>;

    /// Owner flow read authorization. `Forbidden`/`NotFound` skip the row
    /// exactly like the retained error filter.
    fn flow_access<'a>(
        &'a mut self,
        actor: &'a RoutingAttentionActor,
        kind: FlowKind,
        id: &'a str,
    ) -> RoutingFuture<'a, ()>;

    /// Owner flow read projected to the attention subset.
    fn resolve_flow<'a>(
        &'a mut self,
        actor: &'a RoutingAttentionActor,
        kind: FlowKind,
        id: &'a str,
    ) -> RoutingFuture<'a, RoutingAttentionFlow>;
}

fn manager_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ROUTING_MANAGER_REQUIRED",
        "Use an active Company organization-management assignment to inspect routing.",
    )
}

fn skipped(error: &BusinessError) -> bool {
    matches!(error.kind, FailureKind::Forbidden | FailureKind::NotFound)
}

/// Refuse a branch/department-scoped or non-Company actor.
///
/// Composition performs the retained `prepare(holding)` resource binding
/// before calling this function.
pub async fn require_routing_manager(
    ports: &mut impl RoutingAttentionPorts,
    actor: &RoutingAttentionActor,
) -> Result<(), BusinessError> {
    if actor.branch_id.is_some()
        || actor.department_id.is_some()
        || !ports.company_authority(actor).await?
    {
        return Err(manager_required());
    }
    Ok(())
}

/// Run one routing-attention scan for the validated query.
///
/// The scan keeps reading owner pages until a short page or the limit overflow
/// so an old blocker is found after hundreds of newer documents.
pub async fn routing_attention(
    ports: &mut impl RoutingAttentionPorts,
    actor: &RoutingAttentionActor,
    input: &serde_json::Value,
    checked_at: i64,
) -> Result<RoutingAttentionPage, BusinessError> {
    let query = RoutingQuery::parse(input)?;
    require_routing_manager(ports, actor).await?;
    let limit = usize::try_from(query.limit).unwrap_or(usize::MAX);
    let mut items = Vec::new();
    let mut after = query.after.clone();
    let mut exhausted = false;
    while !exhausted && items.len() <= limit {
        let rows = ports
            .rows(&actor.company_id, &after, &query)
            .await?;
        if rows.is_empty() {
            break;
        }
        let page_len = rows.len();
        for row in rows {
            after.clone_from(&row.id);
            if row.archived {
                continue;
            }
            match ports.flow_access(actor, query.kind, &after).await {
                Ok(()) => {}
                Err(error) if skipped(&error) => continue,
                Err(error) => return Err(error),
            }
            // Expense payables are routed by the expense owner, not the generic
            // invoice workflow.
            if query.kind == FlowKind::Invoice
                && row.source_type.as_deref() == Some("expense_claim")
            {
                continue;
            }
            let flow = ports.resolve_flow(actor, query.kind, &after).await?;
            if !flow.can_manage || flow.blocker.is_empty() {
                continue;
            }
            items.push(RoutingAttentionItem {
                id: after.clone(),
                kind: query.kind.as_str().to_owned(),
                title: row.display_title(),
                state: flow.state,
                blocker: flow.blocker,
                action: flow.action,
                team: flow.team,
                record_version: flow.record_version,
                route_version: flow.route_version,
            });
            if items.len() > limit {
                break;
            }
        }
        exhausted = page_len < ROUTING_PAGE_SIZE;
    }
    let more = items.len() > limit;
    let visible: Vec<RoutingAttentionItem> = items.into_iter().take(limit).collect();
    let next_after = if more {
        visible.last().map(|item| item.id.clone())
    } else {
        None
    };
    Ok(RoutingAttentionPage {
        kind: query.kind.as_str().to_owned(),
        items: visible,
        next_after,
        checked_at,
        coverage: "named_flow".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::routing_attention::RoutingAttentionRow;
    use serde_json::json;
    use std::collections::{HashMap, VecDeque};

    #[derive(Default)]
    struct Scripted {
        pages: VecDeque<Vec<RoutingAttentionRow>>,
        authority: bool,
        access: HashMap<String, FailureKind>,
        flows: HashMap<String, RoutingAttentionFlow>,
        authority_calls: usize,
        seen_after: Vec<String>,
    }

    impl Scripted {
        fn flow(state: &str, can_manage: bool, blocker: &str) -> RoutingAttentionFlow {
            RoutingAttentionFlow {
                state: state.into(),
                blocker: blocker.into(),
                can_manage,
                action: Some("Approve".into()),
                team: Some("Company".into()),
                record_version: 3,
                route_version: 1,
            }
        }
    }

    impl RoutingAttentionPorts for Scripted {
        fn rows<'a>(
            &'a mut self,
            _company_id: &'a str,
            after: &'a str,
            _query: &'a RoutingQuery,
        ) -> RoutingFuture<'a, Vec<RoutingAttentionRow>> {
            self.seen_after.push(after.to_owned());
            Box::pin(async move { Ok(self.pages.pop_front().unwrap_or_default()) })
        }

        fn company_authority<'a>(
            &'a mut self,
            _actor: &'a RoutingAttentionActor,
        ) -> RoutingFuture<'a, bool> {
            self.authority_calls += 1;
            Box::pin(async move { Ok(self.authority) })
        }

        fn flow_access<'a>(
            &'a mut self,
            _actor: &'a RoutingAttentionActor,
            _kind: FlowKind,
            id: &'a str,
        ) -> RoutingFuture<'a, ()> {
            let error = self.access.get(id).map(|kind| {
                BusinessError::new(kind.clone(), "FLOW_ACCESS", "The flow read was refused.")
            });
            Box::pin(async move { error.map_or(Ok(()), Err) })
        }

        fn resolve_flow<'a>(
            &'a mut self,
            _actor: &'a RoutingAttentionActor,
            _kind: FlowKind,
            id: &'a str,
        ) -> RoutingFuture<'a, RoutingAttentionFlow> {
            let flow = self.flows.get(id).cloned().unwrap_or_else(|| {
                Scripted::flow("draft", true, "No eligible staff member.")
            });
            Box::pin(async move { Ok(flow) })
        }
    }

    fn actor() -> RoutingAttentionActor {
        RoutingAttentionActor::new("owner", "company", None, None)
    }

    fn row(id: &str) -> RoutingAttentionRow {
        RoutingAttentionRow {
            id: id.into(),
            title: Some(format!("Document {id}")),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn refuses_branch_scoped_and_non_company_actors() {
        let mut ports = Scripted {
            authority: true,
            ..Scripted::default()
        };
        let error = routing_attention(
            &mut ports,
            &RoutingAttentionActor::new("owner", "company", Some("branch".into()), None),
            &json!({}),
            1,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "ROUTING_MANAGER_REQUIRED");

        let mut ports = Scripted::default();
        let error = routing_attention(&mut ports, &actor(), &json!({}), 1)
            .await
            .unwrap_err();
        assert_eq!(error.code, "ROUTING_MANAGER_REQUIRED");
    }

    #[tokio::test]
    async fn invalid_queries_fail_before_authority_is_read() {
        let mut ports = Scripted {
            authority: true,
            ..Scripted::default()
        };
        let error = routing_attention(&mut ports, &actor(), &json!({"limit": 0}), 1)
            .await
            .unwrap_err();
        assert_eq!(error.code, "ROUTING_QUERY_INVALID");
        assert_eq!(ports.authority_calls, 0);
    }

    #[tokio::test]
    async fn pages_only_source_authorized_blockers() {
        let mut ports = Scripted {
            authority: true,
            ..Scripted::default()
        };
        let mut archived = row("a-archived");
        archived.archived = true;
        let mut expense_claim = row("b-expense");
        expense_claim.source_type = Some("expense_claim".into());
        ports.pages.push_back(vec![
            archived,
            row("c-forbidden"),
            row("d-missing"),
            expense_claim,
            row("e-unmanaged"),
            row("f-clear"),
            row("g-blocked"),
        ]);
        ports
            .access
            .insert("c-forbidden".into(), FailureKind::Forbidden);
        ports
            .access
            .insert("d-missing".into(), FailureKind::NotFound);
        ports
            .flows
            .insert("e-unmanaged".into(), Scripted::flow("draft", false, "Hidden."));
        ports
            .flows
            .insert("f-clear".into(), Scripted::flow("posted", true, ""));

        let page = routing_attention(&mut ports, &actor(), &json!({"kind": "invoice"}), 7)
            .await
            .unwrap();
        assert_eq!(page.kind, "invoice");
        assert_eq!(page.coverage, "named_flow");
        assert_eq!(page.checked_at, 7);
        assert_eq!(
            page.items.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
            ["g-blocked"]
        );
        assert_eq!(page.items[0].title, "Document g-blocked");
        assert_eq!(page.items[0].state, "draft");
        assert_eq!(page.items[0].action.as_deref(), Some("Approve"));
        assert_eq!(page.items[0].team.as_deref(), Some("Company"));
        assert_eq!(page.items[0].record_version, 3);
        assert_eq!(page.items[0].route_version, 1);
        assert_eq!(page.next_after, None);
    }

    #[tokio::test]
    async fn overflow_reports_the_last_visible_cursor() {
        let mut ports = Scripted {
            authority: true,
            ..Scripted::default()
        };
        ports
            .pages
            .push_back(vec![row("a"), row("b"), row("c")]);
        let page = routing_attention(&mut ports, &actor(), &json!({"limit": 1}), 1)
            .await
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, "a");
        assert_eq!(page.next_after.as_deref(), Some("a"));
    }

    #[tokio::test]
    async fn page_serializes_the_retained_camel_case_wire_shape() {
        let mut ports = Scripted {
            authority: true,
            ..Scripted::default()
        };
        ports.pages.push_back(vec![row("a")]);
        let page = routing_attention(&mut ports, &actor(), &json!({"limit": 1}), 9)
            .await
            .unwrap();
        let value = serde_json::to_value(&page).unwrap();
        assert_eq!(value["kind"], "expense");
        assert_eq!(value["coverage"], "named_flow");
        assert_eq!(value["checkedAt"], 9);
        assert_eq!(value["nextAfter"], serde_json::Value::Null);
        assert_eq!(value["items"][0]["recordVersion"], 3);
        assert_eq!(value["items"][0]["routeVersion"], 1);
        assert_eq!(value["items"][0]["action"], "Approve");
    }

    #[tokio::test]
    async fn reads_more_than_one_hundred_rows_to_reach_an_old_blocker() {
        let mut ports = Scripted {
            authority: true,
            ..Scripted::default()
        };
        let mut first: Vec<RoutingAttentionRow> = (0..100)
            .map(|index| {
                let id = format!("{index:03}");
                let mut entry = row(&id);
                entry.archived = true;
                entry
            })
            .collect();
        first.push(row("old-blocker"));
        ports.pages.push_back(first);
        ports
            .flows
            .insert("old-blocker".into(), Scripted::flow("draft", true, "Repair routing."));

        let page = routing_attention(&mut ports, &actor(), &json!({}), 1)
            .await
            .unwrap();
        assert_eq!(
            page.items.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
            ["old-blocker"]
        );
        assert_eq!(ports.seen_after, ["", "old-blocker"]);
    }
}
